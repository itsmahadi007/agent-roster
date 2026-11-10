//! Everything the app window shows, computed here so the app's JavaScript holds no roster logic.

use crate::detect::{CommandRunner, sign_in_for, tool_info};
use crate::import::detect_prototype;
use crate::journal::Op;
use crate::limits::{LimitWindow, format_reset};
use crate::model::{Agent, AgentColor, LimitsSource, Roster, Tool};
use crate::paths::{RosterPaths, collapse_tilde, expand_tilde};
use crate::status::{AgentState, AgentStatus, Overview};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn age(secs: i64) -> String {
    let s = secs.max(0);
    if s < 60 {
        "just now".into()
    } else if s < 3600 {
        format!("{}m ago", s / 60)
    } else if s < 86_400 {
        format!("{}h ago", s / 3600)
    } else {
        format!("{}d ago", s / 86_400)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowView {
    pub label: String,
    pub used_percent: u32,
    pub hot: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentView {
    pub id: String,
    pub name: String,
    pub tool: Tool,
    pub tool_label: String,
    pub color: String,
    /// The palette name of `color`, as the color picker and `roster color` take it.
    pub color_label: String,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub default_label: Option<String>,
    /// A bare `claude` or `codex` opens this agent.
    pub default: bool,
    /// The one line other agents see about it.
    pub role: Option<String>,
    /// Uses the skills of the main account.
    pub share: bool,
    pub state: String,
    pub status_text: String,
    pub windows: Vec<WindowView>,
    pub reset_text: Option<String>,
    pub reset_hot: bool,
    pub limits_note: Option<String>,
    pub claude_limits: Option<bool>,
    pub live: bool,
    pub tray_text: String,
    pub tray_hot: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallView {
    pub from_id: String,
    pub from_name: String,
    pub from_color: String,
    pub to_id: String,
    pub to_name: String,
    pub to_color: String,
    pub task: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WarningView {
    pub name: String,
    pub color: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColorView {
    pub label: String,
    pub hex: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverviewView {
    pub headline: String,
    pub agents: Vec<AgentView>,
    pub calls: Vec<CallView>,
    pub warnings: Vec<WarningView>,
    pub limits_updated: Option<String>,
    pub palette: Vec<ColorView>,
    /// The color `roster add` picks when none is given.
    pub next_color: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateView {
    pub tool: Tool,
    pub tool_label: String,
    pub installed: bool,
    pub version: Option<String>,
    pub email: Option<String>,
    pub signed_in: bool,
    pub suggested_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FirstRunView {
    pub candidates: Vec<CandidateView>,
    pub prototype: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeView {
    pub id: u64,
    pub summary: String,
    pub at: i64,
    pub undone: bool,
    pub color: Option<String>,
}

const HOT: u32 = 90;

fn rounded(w: &LimitWindow) -> u32 {
    w.used_percent.round().clamp(0.0, 100.0) as u32
}

fn tool_word(t: Tool) -> &'static str {
    match t {
        Tool::Claude => "claude",
        Tool::Codex => "codex",
    }
}

fn state_key(s: AgentState) -> &'static str {
    match s {
        AgentState::Ready => "ready",
        AgentState::NeedsSignIn => "sign-in",
        AgentState::Working => "working",
        AgentState::NotInstalled => "not-installed",
    }
}

fn count_word(n: usize) -> String {
    const WORDS: [&str; 11] = [
        "Zero", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten",
    ];
    WORDS
        .get(n)
        .map(|w| w.to_string())
        .unwrap_or_else(|| n.to_string())
}

fn headline(n: usize) -> String {
    if n == 1 {
        "One account. Type its name in any terminal to start it.".into()
    } else {
        format!(
            "{} accounts, one quota each. Type a name in any terminal to start it.",
            count_word(n)
        )
    }
}

/// The window the tray and warnings talk about: the week, else the longest listed.
fn main_window(windows: &[LimitWindow]) -> Option<&LimitWindow> {
    windows
        .iter()
        .find(|w| w.label == "week")
        .or_else(|| windows.last())
}

fn agent_view(s: &AgentStatus, o: &Overview, now: i64) -> AgentView {
    let a = &s.agent;
    let windows: Vec<LimitWindow> = s
        .limits
        .as_ref()
        .map(|l| l.windows.clone())
        .unwrap_or_default();
    let calling = o
        .calls
        .iter()
        .filter(|c| c.parent_id.as_deref() == Some(a.id.as_str()))
        .find_map(|c| o.agents.iter().find(|x| x.agent.id == c.agent_id));
    let live = o
        .calls
        .iter()
        .any(|c| c.agent_id == a.id || c.parent_id.as_deref() == Some(a.id.as_str()));
    let status_text = match calling {
        Some(child) => format!("calling {}", child.agent.name),
        None => s.state.label().to_string(),
    };
    let (reset_text, reset_hot) = match windows.as_slice() {
        [] => (None, false),
        [w] => (
            Some(format!("resets in {}", format_reset(w.resets_at, now))),
            rounded(w) >= HOT,
        ),
        [.., w] => (
            Some(format!(
                "{} resets in {}",
                w.label,
                format_reset(w.resets_at, now)
            )),
            rounded(w) >= HOT,
        ),
    };
    let limits_note = if !windows.is_empty() {
        None
    } else {
        // The tools write their usage down while they run, so numbers come with the next use.
        Some(match (s.state, a.tool, a.limits) {
            (AgentState::NotInstalled, _, _) => "not installed".to_string(),
            (AgentState::NeedsSignIn, _, _) => "usage shows after sign in".to_string(),
            (_, Tool::Claude, LimitsSource::Off) => "usage not shown".to_string(),
            _ => format!("usage shows after you use {}", a.name),
        })
    };
    let (tray_text, tray_hot) = match (s.state, main_window(&windows)) {
        (AgentState::NotInstalled, _) => ("not installed".to_string(), false),
        (AgentState::NeedsSignIn, _) => ("sign in".to_string(), false),
        (_, Some(w)) => (format!("{}% {}", rounded(w), w.label), rounded(w) >= HOT),
        (_, None) => (String::new(), false),
    };
    AgentView {
        id: a.id.clone(),
        name: a.name.clone(),
        tool: a.tool,
        tool_label: a.tool.display().to_string(),
        color: a.color.hex().to_string(),
        color_label: a.color.label().to_string(),
        email: s.sign_in.email.clone(),
        plan: s
            .sign_in
            .plan
            .clone()
            .or_else(|| s.limits.as_ref().and_then(|l| l.plan.clone())),
        default_label: a
            .default
            .then(|| format!("default for {}", tool_word(a.tool))),
        default: a.default,
        role: a.role.clone(),
        share: a.share,
        state: state_key(s.state).to_string(),
        status_text,
        windows: windows
            .iter()
            .map(|w| WindowView {
                label: w.label.clone(),
                used_percent: rounded(w),
                hot: rounded(w) >= HOT,
            })
            .collect(),
        reset_text,
        reset_hot,
        limits_note,
        claude_limits: (a.tool == Tool::Claude).then_some(a.limits != LimitsSource::Off),
        live,
        tray_text,
        tray_hot,
    }
}

pub fn overview_view(o: &Overview, home: &Path, now: i64) -> OverviewView {
    let agents: Vec<AgentView> = o.agents.iter().map(|s| agent_view(s, o, now)).collect();
    let calls = o
        .calls
        .iter()
        .filter_map(|c| {
            let find = |id: &str| o.agents.iter().find(|a| a.agent.id == id);
            let from = find(c.parent_id.as_deref()?)?;
            let to = find(&c.agent_id)?;
            Some(CallView {
                from_id: from.agent.id.clone(),
                from_name: from.agent.name.clone(),
                from_color: from.agent.color.hex().into(),
                to_id: to.agent.id.clone(),
                to_name: to.agent.name.clone(),
                to_color: to.agent.color.hex().into(),
                task: c.task.clone(),
                detail: format!(
                    "started {} in {}",
                    age(now - c.started_at),
                    collapse_tilde(Path::new(&c.cwd), home)
                ),
            })
        })
        .collect();
    let mut warnings = vec![];
    for s in &o.agents {
        for w in s.limits.iter().flat_map(|l| l.windows.iter()) {
            if rounded(w) >= HOT {
                let what = if w.label == "week" {
                    "its week".to_string()
                } else {
                    format!("its {} limit", w.label)
                };
                warnings.push(WarningView {
                    name: s.agent.name.clone(),
                    color: s.agent.color.hex().into(),
                    text: format!(
                        "has used {}% of {what}. It resets in {}.",
                        rounded(w),
                        format_reset(w.resets_at, now)
                    ),
                });
            }
        }
    }
    let limits_updated = o
        .agents
        .iter()
        .filter_map(|s| s.limits.as_ref().map(|l| l.as_of))
        .max()
        .map(|t| format!("Last update {}.", age(now - t)));
    let roster = Roster {
        agents: o.agents.iter().map(|s| s.agent.clone()).collect(),
        ..Roster::default()
    };
    OverviewView {
        next_color: roster.next_color().label().into(),
        headline: headline(o.agents.len()),
        agents,
        calls,
        warnings,
        limits_updated,
        palette: AgentColor::ORDER
            .iter()
            .map(|c| ColorView {
                label: c.label().into(),
                hex: c.hex().into(),
            })
            .collect(),
    }
}

/// The existing logins `roster init` would adopt, with what each tool reports about them.
pub fn first_run_view(
    r: &Roster,
    p: &RosterPaths,
    path_var: &str,
    runner: &(dyn CommandRunner + Sync),
) -> FirstRunView {
    let candidates = [(Tool::Claude, "claude1"), (Tool::Codex, "codex1")]
        .into_iter()
        .filter(|(tool, _)| {
            let standard = expand_tilde(tool.standard_dir(), &p.home);
            !r.agents.iter().any(|a| r.config_dir_of(a, p) == standard)
        })
        .map(|(tool, name)| {
            let info = tool_info(tool, path_var, &p.bin_dir(), runner);
            let sign_in = info
                .path
                .as_ref()
                .map(|path| {
                    let probe = Agent {
                        id: "first-run".into(),
                        name: name.into(),
                        tool,
                        config_dir: tool.standard_dir().into(),
                        color: AgentColor::Cobalt,
                        role: None,
                        default: true,
                        share: false,
                        limits: LimitsSource::Off,
                        advanced_links: vec![],
                    };
                    sign_in_for(&probe, r, p, path, runner)
                })
                .unwrap_or_default();
            CandidateView {
                tool,
                tool_label: tool.display().into(),
                installed: info.path.is_some(),
                version: info.version,
                email: sign_in.email,
                signed_in: sign_in.signed_in,
                suggested_name: name.into(),
            }
        })
        .collect();
    FirstRunView {
        candidates,
        prototype: detect_prototype(p).is_some(),
    }
}

fn names_word(summary: &str, name: &str) -> bool {
    summary
        .split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
        .any(|w| w == name)
}

pub fn changes_view(ops: &[Op], r: &Roster) -> Vec<ChangeView> {
    ops.iter()
        .rev()
        .map(|op| ChangeView {
            id: op.id,
            summary: op.summary.clone(),
            at: op.at,
            undone: op.undone,
            color: r
                .agents
                .iter()
                .find(|a| names_word(&op.summary, &a.name))
                .map(|a| a.color.hex().to_string()),
        })
        .collect()
}
