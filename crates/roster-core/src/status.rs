//! One snapshot of every agent: installed, signed in, working, limits. Shared by the CLI and the app.

use crate::detect::{CommandRunner, SignIn, ToolInfo, ask_sign_in, tool_info};
use crate::limits::{AgentLimits, read_agent_limits, read_codex_limits};
use crate::model::{Agent, Roster, Tool};
use crate::paths::RosterPaths;
use crate::session::{SessionRecord, live_sessions};
use crate::usage::{SavedHealth, read_health};

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
        Tool::Claude => saved,
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
    let tools: Vec<ToolInfo> = [Tool::Claude, Tool::Codex]
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
                    let sign_in = tool
                        .as_ref()
                        .map(|t| {
                            ask_sign_in(a, r, p, t, runner).unwrap_or_else(|| SignIn {
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
}
