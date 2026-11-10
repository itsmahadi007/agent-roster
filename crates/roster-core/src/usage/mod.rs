//! Asks each tool itself for an agent's usage, limits and account health, with no terminal
//! and no status line: Claude Code through `claude -p /usage`, Codex through
//! `codex app-server` (`account/rateLimits/read`). Agent Roster never opens a credential file.
//!
//! Parsing is shared by every platform. How a tool is found, spawned and stopped differs:
//! that lives in `windows.rs` and `unix.rs`, and `sys` picks the right one at build time.

pub mod claude;
pub mod codex;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

/// The platform-specific half: finding a tool, spawning it hidden, stopping its whole tree.
pub mod sys {
    #[cfg(unix)]
    pub use super::unix::*;
    #[cfg(windows)]
    pub use super::windows::*;
}

pub use claude::{
    USAGE_ARGS, UsageReply, check_claude, claude_health, fetch_claude_usage, parse_reset,
    parse_usage_reply,
};

use crate::detect::{CommandRunner, RealRunner, find_tool, tool_info};
use crate::limits::{AgentLimits, read_agent_limits, weekly, write_agent_limits};
use crate::model::{Roster, Tool};
use crate::paths::RosterPaths;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// What the last check learned about an account, beyond its numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Health {
    /// Signed in with a plan; the numbers (if any) are fresh.
    Ok,
    /// Not signed in, or the sign-in expired: the agent needs `roster login <name>`.
    SignedOut,
    /// Signed in, but no subscription (none, ended, or an API key without plan limits).
    NoSubscription,
    /// A limit is used up (100%, or the tool says so). The window says when it resets.
    LimitReached,
    /// The tool is not on this computer.
    NotInstalled,
    /// The tool could not be asked (offline, timed out, crashed). Keep the last numbers.
    Unreachable,
}

/// One check of one agent.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub health: Health,
    /// Fresh numbers. `None` when the check got none (keep showing the saved ones).
    pub limits: Option<AgentLimits>,
}

impl Check {
    pub fn health(health: Health) -> Self {
        Check {
            health,
            limits: None,
        }
    }
}

impl Check {
    /// A signed-in check whose week is used up says so.
    pub fn settled(self) -> Self {
        let full = self
            .limits
            .as_ref()
            .and_then(|l| weekly(&l.windows))
            .is_some_and(|w| w.used_percent.round() >= 100.0);
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

/// The last check of one agent, saved so the overview (every 10 s) never asks the tools.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedHealth {
    /// What the newest check found; `Unreachable` when it could not ask the tool.
    pub health: Health,
    /// When the newest check ran (unix seconds).
    pub checked_at: i64,
    /// The newest health a check did learn (never `Unreachable`), so a failed check never
    /// forgets that an agent was signed in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub known: Option<Health>,
}

impl SavedHealth {
    /// What is known about the account: the newest health a check could learn.
    pub fn effective(&self) -> Option<Health> {
        if self.health == Health::Unreachable {
            self.known
        } else {
            Some(self.health)
        }
    }

    /// The newest check could not reach the tool.
    pub fn stale(&self) -> bool {
        self.health == Health::Unreachable
    }
}

fn health_path(p: &RosterPaths, agent_id: &str) -> PathBuf {
    p.limits_dir().join(format!("{agent_id}.health.json"))
}

pub fn read_health(p: &RosterPaths, agent_id: &str) -> Option<SavedHealth> {
    serde_json::from_str(&std::fs::read_to_string(health_path(p, agent_id)).ok()?).ok()
}

/// The saved health after a check with `health` at `now`.
pub fn next_health(prev: Option<&SavedHealth>, health: Health, now: i64) -> SavedHealth {
    let known = if health == Health::Unreachable {
        prev.and_then(SavedHealth::effective)
    } else {
        Some(health)
    };
    SavedHealth {
        health,
        checked_at: now,
        known,
    }
}

/// Saves one check: its health always, its numbers only when it has some that are not older
/// than the saved ones. Saved numbers are never replaced with nothing.
pub fn record_check(
    p: &RosterPaths,
    agent_id: &str,
    check: &Check,
    now: i64,
) -> std::io::Result<()> {
    if let Some(l) = &check.limits {
        let newer = read_agent_limits(p, agent_id).is_none_or(|old| l.as_of >= old.as_of);
        if newer {
            write_agent_limits(p, agent_id, l)?;
        }
    }
    let saved = next_health(read_health(p, agent_id).as_ref(), check.health, now);
    std::fs::create_dir_all(p.limits_dir())?;
    let path = health_path(p, agent_id);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(
        &tmp,
        serde_json::to_string(&saved).map_err(std::io::Error::other)?,
    )?;
    std::fs::rename(tmp, path)
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// How long one usage check may take before it counts as unreachable.
pub const CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// The runner for usage checks: a longer time limit than the overview's, and an empty
/// folder to run in, so no project's local settings or hooks apply.
pub fn usage_runner(p: &RosterPaths) -> RealRunner {
    let dir = p.limits_dir().join("cwd");
    let _ = std::fs::create_dir_all(&dir);
    RealRunner {
        timeout: CHECK_TIMEOUT,
        cwd: Some(dir),
    }
}

/// Checks every agent side by side and saves what each check learned where the overview
/// reads it. Claude Code is asked with `claude auth status` and `claude -p /usage`; Codex
/// with `codex app-server`. Returns each agent's id and check.
pub fn refresh_all(
    r: &Roster,
    p: &RosterPaths,
    path_var: &str,
    runner: &(dyn CommandRunner + Sync),
) -> Vec<(String, Check)> {
    let claude = tool_info(Tool::Claude, path_var, &p.bin_dir(), runner);
    let codex = find_tool(Tool::Codex, path_var, &p.bin_dir());
    let checks: Vec<(String, Check)> = std::thread::scope(|s| {
        let handles: Vec<_> = r
            .agents
            .iter()
            .map(|a| {
                let (claude, codex) = (&claude, &codex);
                s.spawn(move || {
                    let check = match (a.tool, &claude.path, codex) {
                        (Tool::Claude, Some(path), _) => {
                            check_claude(a, r, p, path, claude.version.as_deref(), runner)
                        }
                        (Tool::Codex, _, Some(path)) => {
                            codex::fetch_codex_usage(a, r, p, path).settled()
                        }
                        _ => Check::health(Health::NotInstalled),
                    };
                    (a.id.clone(), check)
                })
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    });
    let at = now();
    for (id, check) in &checks {
        let _ = record_check(p, id, check, at);
    }
    checks
}

#[cfg(test)]
pub(crate) mod testing {
    use crate::detect::CommandRunner;
    use std::path::Path;
    use std::process::Output;
    use std::sync::Mutex;

    /// Answers `auth status` and `-p /usage` with recorded replies; `None` is a timeout.
    pub struct Scripted {
        pub auth: Option<String>,
        pub usage: Option<String>,
        pub calls: Mutex<Vec<Vec<String>>>,
    }

    impl Scripted {
        pub fn new(auth: Option<&str>, usage: Option<&str>) -> Self {
            Scripted {
                auth: auth.map(String::from),
                usage: usage.map(String::from),
                calls: Mutex::new(vec![]),
            }
        }
        pub fn asked_usage(&self) -> bool {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .any(|c| c.iter().any(|a| a == "/usage"))
        }
    }

    impl CommandRunner for Scripted {
        fn run(
            &self,
            _prog: &Path,
            args: &[&str],
            _env: &[(String, Option<String>)],
        ) -> std::io::Result<Output> {
            self.calls
                .lock()
                .unwrap()
                .push(args.iter().map(|s| s.to_string()).collect());
            let reply = match args {
                ["--version"] => Some("2.1.322 (Claude Code)\n".to_string()),
                ["auth", ..] => self.auth.clone(),
                _ => self.usage.clone(),
            };
            #[cfg(unix)]
            let status = std::os::unix::process::ExitStatusExt::from_raw(0);
            #[cfg(windows)]
            let status = std::os::windows::process::ExitStatusExt::from_raw(0);
            match reply {
                Some(out) => Ok(Output {
                    status,
                    stdout: out.into_bytes(),
                    stderr: vec![],
                }),
                None => Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "did not answer in time",
                )),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::LimitWindow;
    use crate::paths::Platform;

    fn limits(as_of: i64, used: f64) -> AgentLimits {
        AgentLimits {
            as_of,
            plan: None,
            windows: vec![LimitWindow {
                label: "week".into(),
                used_percent: used,
                resets_at: as_of + 3600,
            }],
        }
    }

    fn paths() -> (tempfile::TempDir, RosterPaths) {
        let d = tempfile::tempdir().unwrap();
        let p = RosterPaths::for_home(d.path(), Platform::current(), &|_| None);
        (d, p)
    }

    #[test]
    fn a_full_week_settles_as_limit_reached() {
        let full = Check {
            health: Health::Ok,
            limits: Some(limits(10, 100.0)),
        };
        assert_eq!(full.settled().health, Health::LimitReached);
        let almost = Check {
            health: Health::Ok,
            limits: Some(limits(10, 99.4)),
        };
        assert_eq!(almost.settled().health, Health::Ok);
        // A stale number never turns a failed check into a verdict.
        let stale = Check {
            health: Health::Unreachable,
            limits: Some(limits(10, 100.0)),
        };
        assert_eq!(stale.settled().health, Health::Unreachable);
    }

    #[test]
    fn unreachable_keeps_what_was_known() {
        let ok = next_health(None, Health::Ok, 100);
        assert_eq!(ok.effective(), Some(Health::Ok));
        let lost = next_health(Some(&ok), Health::Unreachable, 200);
        assert_eq!(lost.health, Health::Unreachable);
        assert_eq!(lost.checked_at, 200);
        assert_eq!(lost.effective(), Some(Health::Ok));
        assert!(lost.stale());
        let still = next_health(Some(&lost), Health::Unreachable, 300);
        assert_eq!(still.effective(), Some(Health::Ok));
        let never = next_health(None, Health::Unreachable, 100);
        assert_eq!(never.effective(), None);
        let out = next_health(Some(&still), Health::SignedOut, 400);
        assert_eq!(out.effective(), Some(Health::SignedOut));
        assert!(!out.stale());
    }

    #[test]
    fn record_saves_health_and_numbers() {
        let (_d, p) = paths();
        let c = Check {
            health: Health::Ok,
            limits: Some(limits(100, 40.0)),
        };
        record_check(&p, "a1", &c, 100).unwrap();
        assert_eq!(read_agent_limits(&p, "a1"), c.limits);
        assert_eq!(read_health(&p, "a1").unwrap().health, Health::Ok);
    }

    #[test]
    fn record_never_replaces_numbers_with_nothing_or_older_ones() {
        let (_d, p) = paths();
        let fresh = Check {
            health: Health::Ok,
            limits: Some(limits(200, 40.0)),
        };
        record_check(&p, "a1", &fresh, 200).unwrap();
        record_check(&p, "a1", &Check::health(Health::Unreachable), 300).unwrap();
        assert_eq!(read_agent_limits(&p, "a1"), fresh.limits);
        let older = Check {
            health: Health::Unreachable,
            limits: Some(limits(150, 10.0)),
        };
        record_check(&p, "a1", &older, 400).unwrap();
        assert_eq!(read_agent_limits(&p, "a1"), fresh.limits);
        let h = read_health(&p, "a1").unwrap();
        assert_eq!((h.health, h.known), (Health::Unreachable, Some(Health::Ok)));
    }

    #[test]
    fn refresh_says_not_installed_when_the_tool_is_missing() {
        let (d, p) = paths();
        let a = |id: &str, tool| crate::model::Agent {
            id: id.into(),
            name: id.into(),
            tool,
            config_dir: format!("~/.agent-roster/accounts/{id}"),
            color: crate::model::AgentColor::Cobalt,
            role: None,
            default: false,
            share: false,
            limits: crate::model::LimitsSource::Off,
            advanced_links: vec![],
            good_at: None,
        };
        let r = Roster {
            version: 1,
            agents: vec![a("c1", Tool::Claude), a("x1", Tool::Codex)],
            statusline: vec![],
        };
        let empty = d.path().join("no-tools");
        std::fs::create_dir_all(&empty).unwrap();
        let runner = testing::Scripted::new(None, None);
        let checks = refresh_all(&r, &p, &empty.display().to_string(), &runner);
        assert_eq!(
            checks,
            vec![
                ("c1".to_string(), Check::health(Health::NotInstalled)),
                ("x1".to_string(), Check::health(Health::NotInstalled)),
            ]
        );
        assert_eq!(read_health(&p, "c1").unwrap().health, Health::NotInstalled);
    }
}
