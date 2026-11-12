//! Asks Grok itself for an agent's plan usage, in three steps, each one only when the one
//! before it gave nothing:
//!
//! 1. `grok usage --json`: Grok's documented way to print the plan's usage limit. Today it only
//!    answers on Grok's "daemon route"; on the shell route it says to use `/usage` and exits 1.
//!    Its JSON shape isn't documented, so the parser looks for the usage percent anywhere.
//! 2. `grok agent --no-leader stdio`, then the ACP extension method `_x.ai/billing`: what the
//!    TUI's `/usage` calls. It works today, but xAI doesn't document it (its `x.ai/*` methods
//!    are "non-exhaustive and may expand"), so it may change or go away in any release.
//! 3. Neither answered with numbers: the agent's health comes from whether it is signed in,
//!    with no numbers, as before Agent Roster read Grok's usage. Nothing breaks.
//!
//! Only `grok usage`, `initialize` and `_x.ai/billing` are ever run or sent: no session, no
//! prompt, so a check never uses the model or costs anything. Agent Roster never reads Grok's
//! `auth.json`; Grok fetches the numbers with the agent's own login.
//!
//! What Grok 1.0.77 answers (recorded in `tests/fixtures/grok-acp/`):
//! - `grok usage --json`, shell route: stderr "`grok usage` can't show your plan's usage here
//!   yet. Run /usage inside grok instead.", exit 1.
//! - `_x.ai/billing`, signed in: `{"config":{"creditUsagePercent":52.0,"currentPeriod":{"type":
//!   "USAGE_PERIOD_TYPE_WEEKLY","start":..,"end":..},..},"subscription_tier":"SuperGrok Heavy"}`
//! - `_x.ai/billing`, signed out: error -32000 "Authentication required".
//! - a Grok without the method: error -32601 "Method not found".

use super::{Check, Health, sys};
use crate::detect::CommandRunner;
use crate::limits::{AgentLimits, LimitWindow, window_label};
use crate::model::{Agent, Roster};
use crate::paths::RosterPaths;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Output};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::{Duration, Instant};

/// How long one whole check may take, start to finish (Grok's agent starts in about 2 s).
pub const TIMEOUT: Duration = Duration::from_secs(15);

const INITIALIZE: i64 = 1;
const BILLING: i64 = 2;
const BILLING_METHOD: &str = "_x.ai/billing";

/// A JSON-RPC error from Grok's agent.
#[derive(Debug, Clone, PartialEq)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

/// What `_x.ai/billing` answered. `None`: no answer (crashed, hung, could not start, or
/// `initialize` failed).
pub type Billing = Option<Result<Value, RpcError>>;

/// The conversation with Grok's agent, one line at a time, with no I/O.
#[derive(Debug, Default)]
pub struct Exchange {
    billing: Billing,
    initialized: bool,
    failed: bool,
}

impl Exchange {
    pub fn new() -> Self {
        Exchange::default()
    }

    /// The first line to send: `initialize`.
    pub fn opening(&self) -> String {
        json!({
            "jsonrpc": "2.0",
            "id": INITIALIZE,
            "method": "initialize",
            "params": {
                "protocolVersion": 1,
                "clientCapabilities": {},
                "clientInfo": {"name": "agent-roster", "version": env!("CARGO_PKG_VERSION")},
            },
        })
        .to_string()
    }

    /// Takes one line Grok printed; returns the lines to send back.
    pub fn on_line(&mut self, line: &str) -> Vec<String> {
        let Ok(Value::Object(msg)) = serde_json::from_str::<Value>(line.trim()) else {
            return vec![];
        };
        let id = msg.get("id");
        if msg.contains_key("method") {
            // A request from the agent needs an answer; a notification does not.
            return match id {
                Some(id) => vec![
                    json!({"jsonrpc": "2.0", "id": id,
                        "error": {"code": -32601, "message": "not supported"}})
                    .to_string(),
                ],
                None => vec![],
            };
        }
        let reply = match msg.get("error") {
            Some(e) => Err(RpcError {
                code: e.get("code").and_then(Value::as_i64).unwrap_or(0),
                message: [e.get("message"), e.get("data")]
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(": "),
            }),
            None => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
        };
        match id.and_then(Value::as_i64) {
            Some(INITIALIZE) if !self.initialized => match reply {
                Ok(_) => {
                    self.initialized = true;
                    vec![
                        json!({"jsonrpc": "2.0", "id": BILLING, "method": BILLING_METHOD,
                            "params": {}})
                        .to_string(),
                    ]
                }
                Err(_) => {
                    self.failed = true;
                    vec![]
                }
            },
            Some(BILLING) => {
                self.billing = Some(reply);
                vec![]
            }
            _ => vec![],
        }
    }

    pub fn is_done(&self) -> bool {
        self.failed || self.billing.is_some()
    }

    pub fn into_billing(self) -> Billing {
        self.billing
    }
}

/// Feeds Grok's output through an `Exchange`, as if it had been said in order.
pub fn billing_from_lines<'a>(lines: impl IntoIterator<Item = &'a str>) -> Billing {
    let mut ex = Exchange::new();
    for l in lines {
        ex.on_line(l);
    }
    ex.into_billing()
}

fn timestamp(v: &Value) -> Option<i64> {
    v.as_str()?
        .parse::<jiff::Timestamp>()
        .ok()
        .map(|t| t.as_second())
}

/// The object holding `creditUsagePercent`: `result.config` for `_x.ai/billing`; for
/// `grok usage --json`, whose shape isn't documented, wherever it is.
fn usage_object(v: &Value) -> Option<&serde_json::Map<String, Value>> {
    let obj = v.as_object()?;
    if obj.get("creditUsagePercent").is_some_and(Value::is_number) {
        return Some(obj);
    }
    obj.values().find_map(usage_object)
}

/// The plan tier, wherever the reply puts it.
fn tier(v: &Value) -> Option<String> {
    let obj = v.as_object()?;
    ["subscription_tier", "subscriptionTier"]
        .iter()
        .find_map(|k| obj.get(*k).and_then(Value::as_str).map(String::from))
        .or_else(|| obj.values().find_map(tier))
}

/// The limits in a usage reply: one window, the plan's current usage period. `None` when the
/// reply carries no usage percent.
pub fn parse_billing(result: &Value, now: i64) -> Option<AgentLimits> {
    let config = usage_object(result)?;
    let used_percent = config.get("creditUsagePercent").and_then(Value::as_f64)?;
    let period = config.get("currentPeriod");
    let start = period
        .and_then(|p| timestamp(&p["start"]))
        .or_else(|| config.get("billingPeriodStart").and_then(timestamp));
    let end = period
        .and_then(|p| timestamp(&p["end"]))
        .or_else(|| config.get("billingPeriodEnd").and_then(timestamp));
    let kind = period
        .and_then(|p| p.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let label = if kind.ends_with("WEEKLY") {
        "week".to_string()
    } else if kind.ends_with("MONTHLY") {
        "month".to_string()
    } else if kind.ends_with("DAILY") {
        "day".to_string()
    } else {
        match (start, end) {
            (Some(s), Some(e)) if e > s => window_label((e - s) / 60),
            _ => "week".to_string(),
        }
    };
    Some(AgentLimits {
        as_of: now,
        plan: tier(result),
        windows: vec![LimitWindow {
            label,
            used_percent,
            resets_at: end.unwrap_or(0),
        }],
    })
}

/// Decides the agent's health (and numbers) from what `_x.ai/billing` answered:
/// - "authentication required" (or any 401): `SignedOut`;
/// - no answer, or any other error (an older Grok, offline): `Unreachable`;
/// - a result without a usage percent: `NoSubscription`;
/// - otherwise `Ok`, settled to `LimitReached` at 100%.
pub fn check_from_billing(billing: &Billing, now: i64) -> Check {
    match billing {
        None => Check::health(Health::Unreachable),
        Some(Err(e)) => {
            let m = e.message.to_ascii_lowercase();
            let auth = [
                "authentication required",
                "unauthorized",
                "401",
                "not signed in",
            ]
            .iter()
            .any(|s| m.contains(s));
            Check::health(if auth {
                Health::SignedOut
            } else {
                Health::Unreachable
            })
        }
        Some(Ok(result)) => match parse_billing(result, now) {
            Some(limits) => Check {
                health: Health::Ok,
                limits: Some(limits),
            }
            .settled_any(),
            None => Check::health(Health::NoSubscription),
        },
    }
}

impl Check {
    /// Like `settled`, but for a plan whose only window may not be a week.
    fn settled_any(self) -> Self {
        let full = self
            .limits
            .as_ref()
            .is_some_and(|l| l.windows.iter().any(|w| w.used_percent.round() >= 100.0));
        if self.health == Health::Ok && full {
            Check {
                health: Health::LimitReached,
                ..self
            }
        } else {
            self
        }
    }
}

fn send(input: &mut dyn Write, lines: &[String]) -> std::io::Result<()> {
    for l in lines {
        input.write_all(l.as_bytes())?;
        input.write_all(b"\n")?;
    }
    input.flush()
}

/// Starts `grok agent --no-leader stdio` hidden, talks to it, and always stops its whole tree.
fn ask_agent(
    tool_path: &Path,
    env: &crate::run::EnvChanges,
    cwd: &Path,
    timeout: Duration,
) -> Billing {
    let mut cmd = Command::new(tool_path);
    cmd.args(["agent", "--no-leader", "stdio"]).current_dir(cwd);
    for (k, v) in env {
        match v {
            Some(v) => cmd.env(k, v),
            None => cmd.env_remove(k),
        };
    }
    let Ok(mut child) = sys::spawn_piped(&mut cmd) else {
        return None;
    };
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        sys::kill_tree(&mut child);
        return None;
    };
    let (tx, rx) = channel();
    // Ends by itself once the tree is killed and the pipe closes.
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).split(b'\n') {
            let Ok(line) = line else { break };
            if tx
                .send(String::from_utf8_lossy(&line).into_owned())
                .is_err()
            {
                break;
            }
        }
    });
    let deadline = Instant::now() + timeout;
    let mut ex = Exchange::new();
    if send(&mut stdin, &[ex.opening()]).is_ok() {
        while !ex.is_done() {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(line) => {
                    if send(&mut stdin, &ex.on_line(&line)).is_err() {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
            }
        }
    }
    drop(stdin);
    sys::kill_tree(&mut child);
    ex.into_billing()
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// What `grok usage --json` (the documented route) said. `None`: nothing usable (the shell
/// route's refusal, an old background service, a crash, a timeout, JSON without numbers);
/// the next step is tried.
pub fn check_from_usage_command(out: &std::io::Result<Output>, now: i64) -> Option<Check> {
    let out = out.as_ref().ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr).to_ascii_lowercase();
    if out.status.success() {
        if let Some(limits) = serde_json::from_str::<Value>(stdout.trim())
            .ok()
            .and_then(|v| parse_billing(&v, now))
        {
            return Some(
                Check {
                    health: Health::Ok,
                    limits: Some(limits),
                }
                .settled_any(),
            );
        }
        // "Usage limits are managed by your team.": signed in, no plan numbers to show.
        if stdout.to_ascii_lowercase().contains("managed by your team") {
            return Some(Check::health(Health::Ok));
        }
        return None;
    }
    // On the daemon route a missing login says so; the shell route never reads the account.
    (stderr.contains("not signed in") || stderr.contains("not logged in"))
        .then(|| Check::health(Health::SignedOut))
}

/// The last step: neither route gave numbers. A Grok that doesn't know `_x.ai/billing` (or
/// answers it with something unreadable) is signed in, with no numbers, as before; a Grok
/// that didn't answer at all is `Unreachable`, which keeps the last numbers shown.
fn without_numbers(billing: &Billing, check: Check) -> Check {
    match (billing, check.health) {
        (Some(_), Health::Unreachable) => Check::health(Health::Ok),
        _ => check,
    }
}

/// Asks one Grok agent (its GROK_HOME comes from `run::build_env`) for its usage and account
/// health: `grok usage --json` first, then `_x.ai/billing` (see the module notes). An agent
/// with no login is `SignedOut` without starting Grok. Never blocks longer than its timeouts;
/// never leaves a process behind.
pub fn fetch_grok_usage(
    agent: &Agent,
    r: &Roster,
    p: &RosterPaths,
    tool_path: &Path,
    cwd: &Path,
    runner: &(dyn CommandRunner + Sync),
) -> Check {
    if !crate::detect::grok_sign_in(&r.config_dir_of(agent, p)).signed_in {
        return Check::health(Health::SignedOut);
    }
    let env = crate::run::build_env(agent, r, p, None);
    let at = now();
    if let Some(c) =
        check_from_usage_command(&runner.run(tool_path, &["usage", "--json"], &env), at)
    {
        return c;
    }
    let billing = ask_agent(tool_path, &env, cwd, TIMEOUT);
    without_numbers(&billing, check_from_billing(&billing, at))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const NOW: i64 = 1_794_314_302;

    fn fixture(name: &str) -> String {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/grok-acp")
            .join(name);
        std::fs::read_to_string(p).unwrap()
    }

    fn from_fixture(name: &str) -> Check {
        check_from_billing(&billing_from_lines(fixture(name).lines()), NOW)
    }

    #[test]
    fn a_plan_reads_as_its_weekly_window() {
        let c = from_fixture("supergrok.jsonl");
        assert_eq!(c.health, Health::Ok);
        let l = c.limits.unwrap();
        assert_eq!(l.plan.as_deref(), Some("SuperGrok Heavy"));
        assert_eq!(
            l.windows,
            vec![LimitWindow {
                label: "week".into(),
                used_percent: 52.0,
                // 2026-11-16T13:54:22Z
                resets_at: 1_794_837_262,
            }]
        );
        assert!(crate::limits::weekly(&l.windows).is_some());
    }

    #[test]
    fn signed_out_and_old_grok() {
        assert_eq!(
            from_fixture("signed-out.jsonl"),
            Check::health(Health::SignedOut)
        );
        assert_eq!(
            from_fixture("old-grok.jsonl"),
            Check::health(Health::Unreachable)
        );
        assert_eq!(
            check_from_billing(&None, NOW),
            Check::health(Health::Unreachable)
        );
    }

    #[test]
    fn a_full_period_is_limit_reached() {
        let r = json!({"config": {"creditUsagePercent": 100.0,
            "currentPeriod": {"type": "USAGE_PERIOD_TYPE_MONTHLY",
                "start": "2026-11-01T00:00:00Z", "end": "2026-12-01T00:00:00Z"}}});
        let c = check_from_billing(&Some(Ok(r)), NOW);
        assert_eq!(c.health, Health::LimitReached);
        assert_eq!(c.limits.unwrap().windows[0].label, "month");
        let none = check_from_billing(&Some(Ok(json!({"config": {}}))), NOW);
        assert_eq!(none, Check::health(Health::NoSubscription));
    }

    #[test]
    fn exchange_asks_billing_only_after_initialize() {
        let mut ex = Exchange::new();
        let open: Value = serde_json::from_str(&ex.opening()).unwrap();
        assert_eq!(open["method"], "initialize");
        assert_eq!(open["jsonrpc"], "2.0");
        assert!(
            ex.on_line(r#"{"jsonrpc":"2.0","method":"x.ai/fs_notify","params":{}}"#)
                .is_empty()
        );
        let back = ex.on_line(r#"{"jsonrpc":"2.0","id":7,"method":"fs/read","params":{}}"#);
        assert!(back[0].contains("not supported"));
        let next = ex.on_line(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#);
        assert_eq!(next.len(), 1);
        let sent: Value = serde_json::from_str(&next[0]).unwrap();
        assert_eq!(sent["method"], BILLING_METHOD);
        assert!(!ex.is_done());
        ex.on_line(r#"{"jsonrpc":"2.0","id":2,"result":{"config":{}}}"#);
        assert!(ex.is_done());
    }

    #[test]
    fn a_failed_initialize_ends_the_talk() {
        let mut ex = Exchange::new();
        let back = ex.on_line(r#"{"jsonrpc":"2.0","id":1,"error":{"code":-1,"message":"x"}}"#);
        assert!(back.is_empty());
        assert!(ex.is_done());
        assert_eq!(ex.into_billing(), None);
    }

    fn output(code: i32, stdout: &str, stderr: &str) -> std::io::Result<Output> {
        #[cfg(unix)]
        let status = std::os::unix::process::ExitStatusExt::from_raw(code << 8);
        #[cfg(windows)]
        let status = std::os::windows::process::ExitStatusExt::from_raw(code as u32);
        Ok(Output {
            status,
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        })
    }

    #[test]
    fn the_shell_routes_refusal_moves_on_to_the_next_step() {
        let refusal = fixture("usage-shell-route.stderr.txt");
        assert_eq!(
            check_from_usage_command(&output(1, "", &refusal), NOW),
            None
        );
        let timed_out = Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "slow"));
        assert_eq!(check_from_usage_command(&timed_out, NOW), None);
        assert_eq!(check_from_usage_command(&output(0, "{}", ""), NOW), None);
    }

    #[test]
    fn the_documented_route_is_read_in_any_likely_shape() {
        // The billing reply as is, the bare config, or nested deeper: the percent is found.
        let billing = fixture("supergrok.jsonl");
        let result =
            serde_json::from_str::<Value>(billing.lines().nth(1).unwrap()).unwrap()["result"]
                .to_string();
        let config = json!({"usage": {"creditUsagePercent": 30.5,
            "billingPeriodEnd": "2026-11-16T13:54:22Z"}, "subscriptionTier": "SuperGrok"})
        .to_string();
        for (stdout, used, plan) in [
            (result.as_str(), 52.0, "SuperGrok Heavy"),
            (config.as_str(), 30.5, "SuperGrok"),
        ] {
            let c = check_from_usage_command(&output(0, stdout, ""), NOW).unwrap();
            assert_eq!(c.health, Health::Ok);
            let l = c.limits.unwrap();
            assert_eq!(l.plan.as_deref(), Some(plan));
            assert_eq!(l.windows[0].used_percent, used);
            assert_eq!(l.windows[0].resets_at, 1_794_837_262);
        }
        let team = output(0, "Usage limits are managed by your team.\n", "");
        assert_eq!(
            check_from_usage_command(&team, NOW),
            Some(Check::health(Health::Ok))
        );
        let out = output(1, "", "You are not signed in. Run grok login.");
        assert_eq!(
            check_from_usage_command(&out, NOW),
            Some(Check::health(Health::SignedOut))
        );
    }

    #[test]
    fn a_grok_without_either_route_stays_signed_in_without_numbers() {
        let old = billing_from_lines(fixture("old-grok.jsonl").lines());
        assert_eq!(
            without_numbers(&old, check_from_billing(&old, NOW)),
            Check::health(Health::Ok)
        );
        // No answer at all keeps the last numbers: still `Unreachable`.
        assert_eq!(
            without_numbers(&None, check_from_billing(&None, NOW)),
            Check::health(Health::Unreachable)
        );
        let out = billing_from_lines(fixture("signed-out.jsonl").lines());
        assert_eq!(
            without_numbers(&out, check_from_billing(&out, NOW)),
            Check::health(Health::SignedOut)
        );
    }
}
