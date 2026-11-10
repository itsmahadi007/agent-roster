//! One snapshot of every agent: installed, signed in, working, limits. Shared by the CLI and the app.

use crate::detect::{CommandRunner, SignIn, ToolInfo, sign_in_for, tool_info};
use crate::limits::{AgentLimits, read_claude_limits, read_codex_limits};
use crate::model::{Agent, LimitsSource, Roster, Tool};
use crate::paths::RosterPaths;
use crate::session::{SessionRecord, live_sessions};

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
}

#[derive(Debug, Clone)]
pub struct Overview {
    pub agents: Vec<AgentStatus>,
    pub calls: Vec<SessionRecord>,
    pub tools: Vec<ToolInfo>,
}

pub fn limits_for(a: &Agent, r: &Roster, p: &RosterPaths) -> Option<AgentLimits> {
    match (a.tool, a.limits) {
        (Tool::Codex, LimitsSource::Off) => None,
        (Tool::Codex, _) => read_codex_limits(&r.config_dir_of(a, p)),
        // Agent Roster asks Claude Code for these itself (`usage`); the status line, when on,
        // writes the same file between checks.
        (Tool::Claude, _) => read_claude_limits(p, &a.id),
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
        .map(|t| tool_info(t, path_var, &p.bin_dir(), runner))
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
                    let sign_in = tool
                        .as_ref()
                        .map(|t| sign_in_for(a, r, p, t, runner))
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
