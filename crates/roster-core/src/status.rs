//! One snapshot of every agent: installed, signed in, working, limits. Shared by the CLI and the app.

use crate::detect::{CommandRunner, SignIn, ToolInfo, ask_sign_in, tool_info};
use crate::limits::{AgentLimits, read_agent_limits, read_codex_limits};
use crate::model::{Agent, Roster, Tool};
use crate::paths::RosterPaths;
use crate::session::{SessionRecord, live_sessions};
use crate::usage::{SavedHealth, read_health};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    Ready,
    NeedsSignIn,
    Working,
    NotInstalled,
}

impl AgentState {
    pub fn label(&self) -> &'static str {
        match self {
            AgentState::Ready => "ready",
            AgentState::NeedsSignIn => "sign in",
            AgentState::Working => "working",
            AgentState::NotInstalled => "not installed",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentStatus {
    pub agent: Agent,
    pub state: AgentState,
    pub sign_in: SignIn,
    pub limits: Option<AgentLimits>,
    /// What the last usage check learned; `None` until one finishes.
    pub health: Option<SavedHealth>,
    /// False when the tool did not answer and `sign_in` repeats what the last check knew.
    pub sign_in_known: bool,
    /// When the app saw this agent become signed in or installed (`SignInWatch`); `None`
    /// when it saw no such change (and always in the CLI, which keeps no memory).
    pub signed_in_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct Overview {
    pub agents: Vec<AgentStatus>,
    pub calls: Vec<SessionRecord>,
    pub tools: Vec<ToolInfo>,
}

/// The newest numbers known for an agent. Agent Roster asks each tool itself (`usage`) and
/// saves the answer; Claude Code's status line, when on, writes the same file between checks,
/// and Codex's own session files are read too. The newest `as_of` wins.
pub fn limits_for(a: &Agent, r: &Roster, p: &RosterPaths) -> Option<AgentLimits> {
    let saved = read_agent_limits(p, &a.id);
    match a.tool {
        Tool::Claude | Tool::Grok => saved,
        Tool::Codex => match (saved, read_codex_limits(&r.config_dir_of(a, p))) {
            (Some(s), Some(f)) => Some(if f.as_of > s.as_of { f } else { s }),
            (s, f) => s.or(f),
        },
    }
}

pub fn overview(
    r: &Roster,
    p: &RosterPaths,
    path_var: &str,
    runner: &(dyn CommandRunner + Sync),
) -> Overview {
    let tools: Vec<ToolInfo> = Tool::ALL
        .into_iter()
        .map(|t| tool_info(t, path_var, p, runner))
        .collect();
    let sessions = live_sessions(p);
    let agents = std::thread::scope(|s| {
        let handles: Vec<_> = r
            .agents
            .iter()
            .map(|a| {
                let tool = tools
                    .iter()
                    .find(|t| t.tool == a.tool)
                    .and_then(|t| t.path.clone());
                let sessions = &sessions;
                s.spawn(move || {
                    let health = read_health(p, &a.id);
                    // A tool that can't be asked keeps what the last check knew: never "sign in"
                    // for an agent that was signed in.
                    let asked = tool.as_ref().map(|t| ask_sign_in(a, r, p, t, runner));
                    let sign_in_known = !matches!(asked, Some(None));
                    let sign_in = asked
                        .map(|answer| {
                            answer.unwrap_or_else(|| SignIn {
                                signed_in: health.as_ref().and_then(SavedHealth::effective)
                                    != Some(crate::usage::Health::SignedOut),
                                ..SignIn::default()
                            })
                        })
                        .unwrap_or_default();
                    let state = if tool.is_none() {
                        AgentState::NotInstalled
                    } else if sessions.iter().any(|x| x.agent_id == a.id) {
                        AgentState::Working
                    } else if sign_in.signed_in {
                        AgentState::Ready
                    } else {
                        AgentState::NeedsSignIn
                    };
                    AgentStatus {
                        agent: a.clone(),
                        state,
                        sign_in,
                        limits: limits_for(a, r, p),
                        health,
                        sign_in_known,
                        signed_in_at: None,
                    }
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("status thread"))
            .collect()
    });
    let calls = sessions
        .into_iter()
        .filter(|x| x.parent_id.is_some())
        .collect();
    Overview {
        agents,
        calls,
        tools,
    }
}

/// How long after Sign in is clicked in the app its follow-up checks keep running.
pub const SIGN_IN_WAIT: i64 = 5 * 60;

/// What the app remembers between overviews: when it saw each agent become signed in or
/// installed, and the sign-ins started from its Sign in button. The CLI keeps none.
#[derive(Debug, Default)]
pub struct SignInWatch {
    seen: HashMap<String, Seen>,
    started: HashMap<String, i64>,
}

#[derive(Debug, Clone, Copy)]
struct Seen {
    ready: bool,
    changed_at: Option<i64>,
}

impl SignInWatch {
    /// Notes what this overview found at `now` and fills in each agent's `signed_in_at`:
    /// the time the watch saw it go from signed out (or not installed) to signed in. The
    /// first look at an agent is no change, and neither is a tool that did not answer.
    pub fn observe(&mut self, o: &mut Overview, now: i64) {
        self.seen
            .retain(|id, _| o.agents.iter().any(|s| &s.agent.id == id));
        for s in &mut o.agents {
            let prev = self.seen.get(&s.agent.id).copied();
            if s.sign_in_known {
                let ready = s.state != AgentState::NotInstalled && s.sign_in.signed_in;
                let changed_at = match prev {
                    Some(p) if ready && !p.ready => Some(now),
                    Some(p) => p.changed_at,
                    None => None,
                };
                self.seen
                    .insert(s.agent.id.clone(), Seen { ready, changed_at });
                s.signed_in_at = changed_at;
            } else {
                s.signed_in_at = prev.and_then(|p| p.changed_at);
            }
        }
    }

    /// Sign in was clicked for this agent at `now`.
    pub fn sign_in_started(&mut self, agent_id: &str, now: i64) {
        self.started.insert(agent_id.to_string(), now);
    }

    /// True while a sign-in started from the app waits for a check that says it worked:
    /// until a check newer than the click says anything but signed out, and never longer
    /// than `SIGN_IN_WAIT`. It covers a login the status command can't tell apart, like an
    /// expired Codex login file replaced with a fresh one.
    pub fn awaiting_sign_in(&mut self, o: &Overview, now: i64) -> bool {
        self.started.retain(|id, at| {
            let Some(s) = o.agents.iter().find(|s| &s.agent.id == id) else {
                return false;
            };
            let settled = s.health.as_ref().is_some_and(|h| {
                h.checked_at > *at && h.effective() != Some(crate::usage::Health::SignedOut)
            });
            now - *at < SIGN_IN_WAIT && !settled
        });
        !self.started.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::{LimitWindow, write_agent_limits};
    use crate::model::{AgentColor, LimitsSource};
    use crate::paths::Platform;
    use crate::usage::{Check, Health, record_check, testing::Scripted};

    fn agent(id: &str, tool: Tool) -> Agent {
        Agent {
            id: id.into(),
            name: id.into(),
            tool,
            config_dir: format!("~/.agent-roster/accounts/{id}"),
            color: AgentColor::Cobalt,
            role: None,
            default: false,
            share: false,
            limits: LimitsSource::CodexSessions,
            advanced_links: vec![],
            good_at: None,
        }
    }

    fn week(as_of: i64, used: f64) -> AgentLimits {
        AgentLimits {
            as_of,
            plan: None,
            windows: vec![LimitWindow {
                label: "week".into(),
                used_percent: used,
                resets_at: as_of + 86_400,
            }],
        }
    }

    #[test]
    fn codex_numbers_newest_wins() {
        let d = tempfile::tempdir().unwrap();
        let p = RosterPaths::for_home(d.path(), Platform::current(), &|_| None);
        let r = Roster {
            version: 1,
            agents: vec![agent("x1", Tool::Codex)],
            statusline: vec![],
        };
        let day = r
            .config_dir_of(&r.agents[0], &p)
            .join("sessions/2026/11/09");
        std::fs::create_dir_all(&day).unwrap();
        let src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/codex-rollout.jsonl");
        std::fs::copy(src, day.join("rollout-x.jsonl")).unwrap();
        // Only session files: their numbers.
        let from_files = limits_for(&r.agents[0], &r, &p).unwrap();
        assert_eq!(from_files.windows[0].used_percent, 93.0);
        // Saved app-server numbers newer than the session file win.
        write_agent_limits(&p, "x1", &week(from_files.as_of + 60, 12.0)).unwrap();
        assert_eq!(
            limits_for(&r.agents[0], &r, &p).unwrap().windows[0].used_percent,
            12.0
        );
        // Older ones lose.
        write_agent_limits(&p, "x1", &week(from_files.as_of - 60, 12.0)).unwrap();
        assert_eq!(
            limits_for(&r.agents[0], &r, &p).unwrap().windows[0].used_percent,
            93.0
        );
    }

    #[test]
    fn a_tool_that_does_not_answer_never_flips_signed_in_to_sign_in() {
        let d = tempfile::tempdir().unwrap();
        let p = RosterPaths::for_home(d.path(), Platform::current(), &|_| None);
        let tools = d.path().join("tools");
        std::fs::create_dir_all(&tools).unwrap();
        std::fs::write(
            tools.join(if cfg!(windows) {
                "claude.exe"
            } else {
                "claude"
            }),
            "",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(tools.join("claude"), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        let r = Roster {
            version: 1,
            agents: vec![agent("c1", Tool::Claude), agent("c2", Tool::Claude)],
            statusline: vec![],
        };
        record_check(&p, "c1", &Check::health(Health::Ok), 100).unwrap();
        record_check(&p, "c2", &Check::health(Health::SignedOut), 100).unwrap();
        // `auth status` times out.
        let o = overview(
            &r,
            &p,
            &tools.display().to_string(),
            &Scripted::new(None, None),
        );
        assert_eq!(o.agents[0].state, AgentState::Ready);
        assert_eq!(o.agents[0].health.as_ref().unwrap().health, Health::Ok);
        assert_eq!(o.agents[1].state, AgentState::NeedsSignIn);
    }

    fn one(state: AgentState, signed_in: bool, health: Option<(Health, i64)>) -> Overview {
        Overview {
            agents: vec![AgentStatus {
                agent: agent("x1", Tool::Codex),
                state,
                sign_in: SignIn {
                    signed_in,
                    ..SignIn::default()
                },
                limits: None,
                health: health.map(|(h, at)| SavedHealth {
                    health: h,
                    checked_at: at,
                    known: Some(h),
                }),
                sign_in_known: true,
                signed_in_at: None,
            }],
            calls: vec![],
            tools: vec![],
        }
    }

    fn seen(w: &mut SignInWatch, mut o: Overview, now: i64) -> Option<i64> {
        w.observe(&mut o, now);
        o.agents[0].signed_in_at
    }

    #[test]
    fn the_watch_notes_when_an_agent_becomes_signed_in() {
        let mut w = SignInWatch::default();
        let out = || {
            one(
                AgentState::NeedsSignIn,
                false,
                Some((Health::SignedOut, 100)),
            )
        };
        let ready = || one(AgentState::Ready, true, Some((Health::SignedOut, 100)));
        // The first look is no change: an expired login stays as the check found it.
        assert_eq!(seen(&mut w, ready(), 110), None);
        assert_eq!(seen(&mut w, ready(), 120), None);
        // Signed out, then signed in: noted, and kept.
        assert_eq!(seen(&mut w, out(), 130), None);
        assert_eq!(seen(&mut w, ready(), 140), Some(140));
        assert_eq!(seen(&mut w, ready(), 150), Some(140));
        // Installed after being missing counts too.
        let mut w = SignInWatch::default();
        assert_eq!(
            seen(&mut w, one(AgentState::NotInstalled, false, None), 10),
            None
        );
        assert_eq!(seen(&mut w, ready(), 20), Some(20));
    }

    #[test]
    fn a_tool_that_did_not_answer_is_no_change() {
        let mut w = SignInWatch::default();
        let ready = || one(AgentState::Ready, true, Some((Health::SignedOut, 100)));
        assert_eq!(seen(&mut w, ready(), 110), None);
        // `login status` timed out: the overview fell back to the saved "signed out".
        let mut guessed = one(
            AgentState::NeedsSignIn,
            false,
            Some((Health::SignedOut, 100)),
        );
        guessed.agents[0].sign_in_known = false;
        assert_eq!(seen(&mut w, guessed, 120), None);
        assert_eq!(seen(&mut w, ready(), 130), None);
    }

    #[test]
    fn a_sign_in_from_the_app_waits_for_a_check_that_says_it_worked() {
        let mut w = SignInWatch::default();
        let o = |h, at| one(AgentState::Ready, true, Some((h, at)));
        assert!(!w.awaiting_sign_in(&o(Health::SignedOut, 100), 110));
        w.sign_in_started("x1", 120);
        assert!(w.awaiting_sign_in(&o(Health::SignedOut, 100), 130));
        // A check after the click that still says signed out: keep waiting.
        assert!(w.awaiting_sign_in(&o(Health::SignedOut, 140), 150));
        // One that says signed in ends it.
        assert!(!w.awaiting_sign_in(&o(Health::Ok, 160), 170));
        assert!(!w.awaiting_sign_in(&o(Health::SignedOut, 180), 190));
        // Never longer than SIGN_IN_WAIT.
        w.sign_in_started("x1", 200);
        assert!(w.awaiting_sign_in(&o(Health::SignedOut, 210), 220));
        assert!(!w.awaiting_sign_in(&o(Health::SignedOut, 210), 200 + SIGN_IN_WAIT));
        assert!(!w.awaiting_sign_in(&o(Health::SignedOut, 210), 230));
    }
}
