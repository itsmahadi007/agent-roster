//! Asks Codex itself for an agent's limits: `codex app-server` speaks JSON-RPC over stdio.
//! After the `initialize` handshake, `account/rateLimits/read` returns the same numbers the
//! Codex status line shows, fetched with the agent's own login. It works before the agent's
//! first message, when Codex's session files have no numbers yet.
//!
//! Only `initialize`, `account/read` and `account/rateLimits/read` are ever sent: nothing that
//! starts a thread or a turn, so a check never uses the model or costs anything.
//!
//! Framing: one JSON object per line each way, with no `"jsonrpc"` field. Replies can come
//! in any order and notifications (`account/updated`, `remoteControl/status/changed`) are
//! mixed in. Parsing (`Exchange`, `check_from_replies`) is pure; the process part is thin.
//!
//! What Codex 0.167.0 answers (recorded in `tests/fixtures/codex-app-server/`):
//! - signed out: `account/read` → `{"account":null,"requiresOpenaiAuth":true}`;
//!   rate limits → error -32600 "codex account authentication required to read rate limits".
//! - login expired or revoked: `account/read` → `account: null` (the refresh failed);
//!   rate limits → error -32603 "... wham/usage failed: 401 Unauthorized ...".
//! - API key: `account/read` → `{"type":"apiKey"}`; rate limits → error -32600
//!   "chatgpt authentication required to read rate limits".
//! - ChatGPT plan: `{"type":"chatgpt","planType":"pro",...}`; rate limits → `rateLimits`
//!   with `primary: {usedPercent, windowDurationMins: 10080, resetsAt}`, `secondary: null`.

use super::{Check, Health, sys};
use crate::limits::{AgentLimits, LimitWindow, read_codex_limits, window_label};
use crate::model::{Agent, Roster};
use crate::paths::RosterPaths;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::Command;
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

/// How long one whole check may take, start to finish.
pub const TIMEOUT: Duration = Duration::from_secs(15);

const INITIALIZE: i64 = 1;
const ACCOUNT: i64 = 2;
const RATE_LIMITS: i64 = 3;

/// A JSON-RPC error from app-server.
#[derive(Debug, Clone, PartialEq)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

pub type Reply = Result<Value, RpcError>;

/// What app-server answered. `None`: no answer (it crashed, hung or could not start).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Replies {
    pub account: Option<Reply>,
    pub rate_limits: Option<Reply>,
}

/// The conversation with app-server, one line at a time, with no I/O.
#[derive(Debug, Default)]
pub struct Exchange {
    replies: Replies,
    initialized: bool,
}

impl Exchange {
    pub fn new() -> Self {
        Exchange::default()
    }

    /// The first line to send: `initialize`.
    pub fn opening(&self) -> String {
        json!({
            "id": INITIALIZE,
            "method": "initialize",
            "params": {
                "clientInfo": {
                    "name": "agent-roster",
                    "title": "Agent Roster",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "capabilities": null,
            },
        })
        .to_string()
    }

    /// Takes one line app-server printed; returns the lines to send back.
    pub fn on_line(&mut self, line: &str) -> Vec<String> {
        let Ok(Value::Object(msg)) = serde_json::from_str::<Value>(line.trim()) else {
            return vec![];
        };
        let id = msg.get("id");
        if msg.contains_key("method") {
            // A request from the server needs an answer; a notification does not.
            return match id {
                Some(id) => vec![
                    json!({"id": id, "error": {"code": -32601, "message": "not supported"}})
                        .to_string(),
                ],
                None => vec![],
            };
        }
        let reply: Reply = match msg.get("error") {
            Some(e) => Err(RpcError {
                code: e.get("code").and_then(Value::as_i64).unwrap_or(0),
                message: e
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            }),
            None => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
        };
        match id.and_then(Value::as_i64) {
            Some(INITIALIZE) if !self.initialized => match reply {
                Ok(_) => {
                    self.initialized = true;
                    vec![
                        json!({"method": "initialized"}).to_string(),
                        json!({"id": ACCOUNT, "method": "account/read",
                            "params": {"refreshToken": false}})
                        .to_string(),
                        json!({"id": RATE_LIMITS, "method": "account/rateLimits/read",
                            "params": {"excludeResetCreditDetails": true}})
                        .to_string(),
                    ]
                }
                Err(e) => {
                    self.replies.account = Some(Err(e.clone()));
                    self.replies.rate_limits = Some(Err(e));
                    vec![]
                }
            },
            Some(ACCOUNT) => {
                self.replies.account = Some(reply);
                vec![]
            }
            Some(RATE_LIMITS) => {
                self.replies.rate_limits = Some(reply);
                vec![]
            }
            _ => vec![],
        }
    }

    pub fn is_done(&self) -> bool {
        self.replies.account.is_some() && self.replies.rate_limits.is_some()
    }

    pub fn into_replies(self) -> Replies {
        self.replies
    }
}

/// Feeds app-server's output through an `Exchange`, as if it had been said in order.
pub fn replies_from_lines<'a>(lines: impl IntoIterator<Item = &'a str>) -> Replies {
    let mut ex = Exchange::new();
    for l in lines {
        ex.on_line(l);
    }
    ex.into_replies()
}

/// The limits in an `account/rateLimits/read` result (its `rateLimits` snapshot), weekly
/// window last. A window with no duration is skipped, except the secondary, which is the
/// weekly one in every Codex that has two. `None` when there is no snapshot at all.
pub fn parse_rate_limits(result: &Value, now: i64) -> Option<AgentLimits> {
    let snap = result.get("rateLimits").filter(|s| s.is_object())?;
    let mut windows = vec![];
    for key in ["primary", "secondary"] {
        let Some(w) = snap.get(key).filter(|w| w.is_object()) else {
            continue;
        };
        let Some(used_percent) = w.get("usedPercent").and_then(Value::as_f64) else {
            continue;
        };
        let label = match w.get("windowDurationMins").and_then(Value::as_i64) {
            Some(mins) => window_label(mins),
            None if key == "secondary" => "week".into(),
            None => continue,
        };
        windows.push(LimitWindow {
            label,
            used_percent,
            resets_at: w.get("resetsAt").and_then(Value::as_i64).unwrap_or(0),
        });
    }
    windows.sort_by_key(|w| w.label == "week");
    Some(AgentLimits {
        as_of: now,
        plan: snap
            .get("planType")
            .and_then(Value::as_str)
            .map(String::from),
        windows,
    })
}

/// An error that means the login is missing, expired or revoked (not a network problem).
fn is_auth_error(e: &RpcError) -> bool {
    let m = e.message.to_ascii_lowercase();
    [
        "authentication required",
        "401",
        "unauthorized",
        "not logged in",
        "log in again",
        "sign in again",
        "could not be refreshed",
    ]
    .iter()
    .any(|s| m.contains(s))
}

/// Decides the agent's health (and numbers) from what app-server answered:
/// - no account: `SignedOut` (or `NoSubscription` when the provider needs no OpenAI login);
/// - an API key, Bedrock or GCP account: `NoSubscription` (no plan limits to read);
/// - an auth error from either call: `SignedOut`; any other error or no answer: `Unreachable`;
/// - `rateLimitReachedType` set, a window at 100%, spend control reached, or
///   `ordinaryUsageAllowed: false`: `LimitReached`;
/// - no windows at all on a free or unknown plan: `NoSubscription`; otherwise `Ok`.
pub fn check_from_replies(replies: &Replies, now: i64) -> Check {
    let reply = |r: &Option<Reply>| -> Result<Value, Health> {
        match r {
            Some(Ok(v)) => Ok(v.clone()),
            Some(Err(e)) if is_auth_error(e) => Err(Health::SignedOut),
            _ => Err(Health::Unreachable),
        }
    };
    let account = match reply(&replies.account) {
        Ok(v) => v,
        Err(h) => return Check::health(h),
    };
    let plan_of_account = match account.get("account").filter(|a| !a.is_null()) {
        None => {
            let needs_login = account
                .get("requiresOpenaiAuth")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            return Check::health(if needs_login {
                Health::SignedOut
            } else {
                Health::NoSubscription
            });
        }
        Some(a) if a.get("type").and_then(Value::as_str) != Some("chatgpt") => {
            return Check::health(Health::NoSubscription);
        }
        Some(a) => a.get("planType").and_then(Value::as_str).map(String::from),
    };
    let result = match reply(&replies.rate_limits) {
        Ok(v) => v,
        Err(h) => return Check::health(h),
    };
    let Some(mut limits) = parse_rate_limits(&result, now) else {
        return Check::health(Health::Unreachable);
    };
    if limits.plan.is_none() {
        limits.plan = plan_of_account;
    }
    let snap = &result["rateLimits"];
    let reached = snap["rateLimitReachedType"].is_string()
        || snap["spendControlReached"] == Value::Bool(true)
        || limits.windows.iter().any(|w| w.used_percent >= 100.0);
    let blocked = result["ordinaryUsageAllowed"] == Value::Bool(false);
    let health = if reached || (blocked && !limits.windows.is_empty()) {
        Health::LimitReached
    } else if limits.windows.is_empty()
        && (blocked || matches!(limits.plan.as_deref(), None | Some("free" | "unknown")))
    {
        Health::NoSubscription
    } else {
        Health::Ok
    };
    Check {
        health,
        limits: (!limits.windows.is_empty()).then_some(limits),
    }
}

fn send(input: &mut dyn Write, lines: &[String]) -> std::io::Result<()> {
    for l in lines {
        input.write_all(l.as_bytes())?;
        input.write_all(b"\n")?;
    }
    input.flush()
}

/// Talks to app-server: writes to its stdin, reads its stdout lines from `lines`. Stops when
/// both answers are in, the output ends, or `timeout` passes, and returns what it got.
fn converse(lines: &Receiver<String>, input: &mut dyn Write, timeout: Duration) -> Replies {
    let deadline = Instant::now() + timeout;
    let mut ex = Exchange::new();
    if send(input, &[ex.opening()]).is_err() {
        return Replies::default();
    }
    while !ex.is_done() {
        let left = deadline.saturating_duration_since(Instant::now());
        match lines.recv_timeout(left) {
            Ok(line) => {
                if send(input, &ex.on_line(&line)).is_err() {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
        }
    }
    ex.into_replies()
}

/// Starts `<tool> app-server` hidden, talks to it, and always stops its whole tree.
fn ask_app_server(
    tool_path: &Path,
    env: &crate::run::EnvChanges,
    cwd: &Path,
    timeout: Duration,
) -> Replies {
    let mut cmd = Command::new(tool_path);
    cmd.arg("app-server").current_dir(cwd);
    for (k, v) in env {
        match v {
            Some(v) => cmd.env(k, v),
            None => cmd.env_remove(k),
        };
    }
    let Ok(mut child) = sys::spawn_piped(&mut cmd) else {
        return Replies::default();
    };
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        sys::kill_tree(&mut child);
        return Replies::default();
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
    let replies = converse(&rx, &mut stdin, timeout);
    drop(stdin);
    sys::kill_tree(&mut child);
    replies
}

/// `fetch_codex_usage` with everything passed in. When app-server can't be asked, the
/// numbers come from Codex's own session files instead, and the health stays `Unreachable`.
fn fetch_with(
    tool_path: &Path,
    env: &crate::run::EnvChanges,
    cwd: &Path,
    codex_home: &Path,
    timeout: Duration,
    now: i64,
) -> Check {
    let replies = ask_app_server(tool_path, env, cwd, timeout);
    let mut check = check_from_replies(&replies, now);
    if check.health == Health::Unreachable && check.limits.is_none() {
        check.limits = read_codex_limits(codex_home);
    }
    check
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Asks one Codex agent (its CODEX_HOME comes from `run::build_env`) for its limits and
/// account health. Never blocks longer than its timeout; never leaves a process behind.
pub fn fetch_codex_usage(agent: &Agent, r: &Roster, p: &RosterPaths, tool_path: &Path) -> Check {
    let env = crate::run::build_env(agent, r, p, None);
    let codex_home = r.config_dir_of(agent, p);
    let cwd = if codex_home.is_dir() {
        codex_home.clone()
    } else {
        std::env::temp_dir()
    };
    fetch_with(tool_path, &env, &cwd, &codex_home, TIMEOUT, now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::LimitWindow;
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::mpsc::channel;
    use std::time::Instant;

    const NOW: i64 = 1_794_314_302;

    fn fixture(name: &str) -> String {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/codex-app-server")
            .join(name);
        std::fs::read_to_string(p).unwrap()
    }

    fn from_fixture(name: &str) -> Check {
        let text = fixture(name);
        check_from_replies(&replies_from_lines(text.lines()), NOW)
    }

    fn sent(lines: &[String]) -> Vec<Value> {
        lines
            .iter()
            .map(|l| {
                assert!(!l.contains('\n'), "one message per line: {l}");
                serde_json::from_str(l).unwrap()
            })
            .collect()
    }

    fn methods(lines: &[String]) -> Vec<String> {
        sent(lines)
            .iter()
            .map(|v| v["method"].as_str().unwrap_or("").to_string())
            .collect()
    }

    const INIT_OK: &str = r#"{"id":1,"result":{"userAgent":"x","codexHome":"/h","platformFamily":"unix","platformOs":"linux"}}"#;
    const SIGNED_IN: &str = r#"{"id":2,"result":{"account":{"type":"chatgpt","email":"you@example.com","planType":"plus","workspaceName":null},"requiresOpenaiAuth":true}}"#;

    fn limits_reply(rate_limits: Value, ordinary: Value) -> String {
        json!({"id": 3, "result": {"ordinaryUsageAllowed": ordinary, "rateLimits": rate_limits,
            "rateLimitsByLimitId": null, "rateLimitResetCredits": null, "accountId": null,
            "rateLimitUpsell": null}})
        .to_string()
    }

    fn snapshot(primary: Value, secondary: Value, plan: &str, reached: Value) -> Value {
        json!({"limitId": "codex", "limitName": null, "normalModelSlug": null,
            "primary": primary, "secondary": secondary, "credits": null, "individualLimit": null,
            "spendControlReached": null, "planType": plan, "rateLimitReachedType": reached})
    }

    fn window(used: i64, mins: i64, resets: i64) -> Value {
        json!({"usedPercent": used, "windowDurationMins": mins, "resetsAt": resets})
    }

    fn check_of(account: &str, limits: &str) -> Check {
        check_from_replies(&replies_from_lines([INIT_OK, account, limits]), NOW)
    }

    // The handshake.

    #[test]
    fn opens_with_initialize() {
        let ex = Exchange::new();
        let v = &sent(&[ex.opening()])[0];
        assert_eq!(v["method"], "initialize");
        assert_eq!(v["id"], 1);
        assert_eq!(v["params"]["clientInfo"]["name"], "agent-roster");
        assert!(v["params"]["clientInfo"]["version"].is_string());
    }

    #[test]
    fn after_initialize_asks_for_the_account_and_limits_only() {
        let mut ex = Exchange::new();
        let out = ex.on_line(INIT_OK);
        assert_eq!(
            methods(&out),
            ["initialized", "account/read", "account/rateLimits/read"]
        );
        let v = sent(&out);
        assert!(v[0].get("id").is_none(), "initialized is a notification");
        assert_eq!(v[1]["params"]["refreshToken"], false);
        assert!(!ex.is_done());
    }

    #[test]
    fn skips_notifications_and_noise_and_refuses_server_requests() {
        let mut ex = Exchange::new();
        ex.on_line(INIT_OK);
        assert!(ex.on_line("WARNING: not json").is_empty());
        assert!(
            ex.on_line(r#"{"method":"account/updated","params":{"authMode":"chatgpt"}}"#)
                .is_empty()
        );
        let out = ex.on_line(r#"{"id":"s1","method":"attestation/generate","params":{}}"#);
        let v = &sent(&out)[0];
        assert_eq!(v["id"], "s1");
        assert!(v["error"]["code"].is_i64());
        assert!(v.get("method").is_none());
        assert!(!ex.is_done());
    }

    #[test]
    fn done_once_both_answers_are_in_any_order() {
        let mut ex = Exchange::new();
        ex.on_line(INIT_OK);
        ex.on_line(r#"{"error":{"code":-32600,"message":"nope"},"id":3}"#);
        assert!(!ex.is_done());
        ex.on_line(SIGNED_IN);
        assert!(ex.is_done());
        let r = ex.into_replies();
        assert!(r.account.unwrap().is_ok());
        assert_eq!(
            r.rate_limits.unwrap().unwrap_err(),
            RpcError {
                code: -32600,
                message: "nope".into()
            }
        );
    }

    #[test]
    fn a_failed_initialize_ends_the_exchange() {
        let mut ex = Exchange::new();
        let out = ex.on_line(r#"{"id":1,"error":{"code":-32600,"message":"bad"}}"#);
        assert!(out.is_empty());
        assert!(ex.is_done());
        let c = check_from_replies(&ex.into_replies(), NOW);
        assert_eq!(c.health, Health::Unreachable);
    }

    // Real replies, recorded from Codex 0.167.0 and sanitized.

    #[test]
    fn real_signed_in_pro_account() {
        let c = from_fixture("signed-in-pro.jsonl");
        assert_eq!(c.health, Health::Ok);
        assert_eq!(
            c.limits,
            Some(AgentLimits {
                as_of: NOW,
                plan: Some("pro".into()),
                windows: vec![LimitWindow {
                    label: "week".into(),
                    used_percent: 7.0,
                    resets_at: 1_794_896_999,
                }],
            })
        );
    }

    #[test]
    fn real_signed_out() {
        assert_eq!(
            from_fixture("signed-out.jsonl"),
            Check::health(Health::SignedOut)
        );
    }

    #[test]
    fn real_expired_login() {
        // account/read: null; rateLimits: -32603 with a 401 from the backend.
        assert_eq!(
            from_fixture("expired-login.jsonl"),
            Check::health(Health::SignedOut)
        );
    }

    #[test]
    fn real_api_key_login_has_no_subscription() {
        assert_eq!(
            from_fixture("api-key.jsonl"),
            Check::health(Health::NoSubscription)
        );
    }

    // Shapes from the generated schema (GetAccountRateLimitsResponse, Account, PlanType).

    #[test]
    fn five_hour_and_weekly_windows_week_last() {
        let c = check_of(
            SIGNED_IN,
            &limits_reply(
                snapshot(
                    window(30, 300, NOW + 3600),
                    window(55, 10080, NOW + 86_400),
                    "plus",
                    Value::Null,
                ),
                json!(true),
            ),
        );
        assert_eq!(c.health, Health::Ok);
        let l = c.limits.unwrap();
        assert_eq!(l.plan.as_deref(), Some("plus"));
        let labels: Vec<_> = l.windows.iter().map(|w| w.label.as_str()).collect();
        assert_eq!(labels, ["5 hr", "week"]);
        assert_eq!(l.windows[1].used_percent, 55.0);
        assert_eq!(l.windows[1].resets_at, NOW + 86_400);
    }

    #[test]
    fn secondary_without_a_duration_is_the_week() {
        let l = parse_rate_limits(
            &serde_json::from_str::<Value>(&limits_reply(
                snapshot(
                    window(30, 300, NOW + 3600),
                    json!({"usedPercent": 40, "windowDurationMins": null, "resetsAt": NOW + 9}),
                    "plus",
                    Value::Null,
                ),
                json!(true),
            ))
            .unwrap()["result"],
            NOW,
        )
        .unwrap();
        assert_eq!(
            crate::limits::weekly(&l.windows).unwrap().used_percent,
            40.0
        );
    }

    #[test]
    fn limit_reached_type_means_limit_reached() {
        let c = check_of(
            SIGNED_IN,
            &limits_reply(
                snapshot(
                    window(80, 10080, NOW + 60),
                    Value::Null,
                    "plus",
                    json!("rate_limit_reached"),
                ),
                json!(false),
            ),
        );
        assert_eq!(c.health, Health::LimitReached);
        assert_eq!(c.limits.unwrap().windows[0].used_percent, 80.0);
    }

    #[test]
    fn a_full_window_means_limit_reached() {
        let c = check_of(
            SIGNED_IN,
            &limits_reply(
                snapshot(
                    window(100, 10080, NOW + 60),
                    Value::Null,
                    "pro",
                    Value::Null,
                ),
                Value::Null,
            ),
        );
        assert_eq!(c.health, Health::LimitReached);
    }

    #[test]
    fn usage_not_allowed_means_limit_reached() {
        let c = check_of(
            SIGNED_IN,
            &limits_reply(
                snapshot(window(20, 10080, NOW + 60), Value::Null, "pro", Value::Null),
                json!(false),
            ),
        );
        assert_eq!(c.health, Health::LimitReached);
    }

    #[test]
    fn free_plan_without_limits_has_no_subscription() {
        let account = r#"{"id":2,"result":{"account":{"type":"chatgpt","email":"you@example.com","planType":"free","workspaceName":null},"requiresOpenaiAuth":true}}"#;
        let c = check_of(
            account,
            &limits_reply(
                snapshot(Value::Null, Value::Null, "free", Value::Null),
                Value::Null,
            ),
        );
        assert_eq!(c.health, Health::NoSubscription);
    }

    #[test]
    fn a_provider_that_needs_no_openai_login_has_no_subscription() {
        let account = r#"{"id":2,"result":{"account":null,"requiresOpenaiAuth":false}}"#;
        let limits = r#"{"id":3,"error":{"code":-32600,"message":"codex account authentication required to read rate limits"}}"#;
        assert_eq!(check_of(account, limits).health, Health::NoSubscription);
    }

    #[test]
    fn a_network_error_is_unreachable() {
        let limits = r#"{"id":3,"error":{"code":-32603,"message":"failed to fetch codex rate limits: error sending request for url (https://chatgpt.com/backend-api/wham/usage)"}}"#;
        assert_eq!(
            check_of(SIGNED_IN, limits),
            Check::health(Health::Unreachable)
        );
    }

    #[test]
    fn no_answer_is_unreachable() {
        assert_eq!(
            check_from_replies(&Replies::default(), NOW),
            Check::health(Health::Unreachable)
        );
        let partial = replies_from_lines([INIT_OK, SIGNED_IN]);
        assert_eq!(
            check_from_replies(&partial, NOW).health,
            Health::Unreachable
        );
    }

    // The thin process part, with channels instead of a process.

    #[test]
    fn converse_answers_the_handshake_and_collects_replies() {
        let (tx, rx) = channel();
        for l in fixture("signed-in-pro.jsonl").lines() {
            tx.send(l.to_string()).unwrap();
        }
        let mut input = vec![];
        let r = converse(&rx, &mut input, Duration::from_secs(5));
        let text = String::from_utf8(input).unwrap();
        let lines: Vec<String> = text.lines().map(String::from).collect();
        assert!(text.ends_with('\n'));
        assert_eq!(
            methods(&lines),
            [
                "initialize",
                "initialized",
                "account/read",
                "account/rateLimits/read"
            ]
        );
        assert_eq!(check_from_replies(&r, NOW).health, Health::Ok);
    }

    #[test]
    fn converse_gives_up_at_the_timeout() {
        let (_tx, rx) = channel::<String>();
        let start = Instant::now();
        let r = converse(&rx, &mut vec![], Duration::from_millis(200));
        assert!(start.elapsed() < Duration::from_secs(2));
        assert_eq!(r, Replies::default());
    }

    #[test]
    fn converse_stops_when_the_output_ends() {
        let (tx, rx) = channel();
        tx.send(INIT_OK.to_string()).unwrap();
        drop(tx);
        let start = Instant::now();
        let r = converse(&rx, &mut vec![], Duration::from_secs(10));
        assert!(start.elapsed() < Duration::from_secs(2));
        assert_eq!(r, Replies::default());
    }

    #[test]
    fn a_tool_that_cannot_start_falls_back_to_session_files() {
        let d = tempfile::tempdir().unwrap();
        let day = d.path().join("sessions/2026/11/09");
        std::fs::create_dir_all(&day).unwrap();
        std::fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex-rollout.jsonl"),
            day.join("rollout-2026-11-09T13-09-44-x.jsonl"),
        )
        .unwrap();
        let c = fetch_with(
            &d.path().join("no-such-codex"),
            &vec![],
            d.path(),
            d.path(),
            Duration::from_secs(2),
            NOW,
        );
        assert_eq!(c.health, Health::Unreachable);
        assert_eq!(c.limits.unwrap().windows[0].used_percent, 93.0);
    }

    /// Asks a real Codex: `ROSTER_LIVE_CODEX_HOME=<dir> cargo test -p roster-core live_codex
    /// -- --ignored --nocapture`. `ROSTER_LIVE_CODEX` picks the binary (else `codex` on PATH).
    #[test]
    #[ignore]
    fn live_codex() {
        let home = std::env::var("ROSTER_LIVE_CODEX_HOME").expect("set ROSTER_LIVE_CODEX_HOME");
        let tool = std::env::var_os("ROSTER_LIVE_CODEX")
            .map(PathBuf::from)
            .unwrap_or_else(|| which::which("codex").expect("codex on PATH"));
        let env = vec![("CODEX_HOME".to_string(), Some(home.clone()))];
        let start = Instant::now();
        let c = fetch_with(
            &tool,
            &env,
            Path::new(&home),
            Path::new(&home),
            TIMEOUT,
            NOW,
        );
        println!("{:?} in {:?}", c, start.elapsed());
        assert_ne!(c.health, Health::Unreachable);
    }
}
