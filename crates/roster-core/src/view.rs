//! Everything the app window shows, computed here so the app's JavaScript holds no roster logic.

use crate::detect::{CommandRunner, sign_in_for, tool_info};
use crate::import::detect_prototype;
use crate::journal::Op;
use crate::limits::{LimitWindow, reset_phrase, weekly};
use crate::model::{Agent, AgentColor, LimitsSource, Roster, Tool};
use crate::paths::{RosterPaths, collapse_tilde, expand_tilde};
use crate::status::{AgentState, AgentStatus, Overview};
use crate::usage::{Health, SavedHealth};
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
    /// What other agents read it's good at: the list someone set, or the tool's defaults.
    pub good_at: Vec<String>,
    /// True when someone set `good_at`; false when it shows the defaults.
    pub good_at_custom: bool,
    pub state: String,
    pub status_text: String,
    pub windows: Vec<WindowView>,
    pub reset_text: Option<String>,
    pub reset_hot: bool,
    pub limits_note: Option<String>,
    /// The account as the row shows it: ok, sign-in, no-subscription, limit-reached,
    /// not-installed, checking or unreachable.
    pub health: String,
    /// "as of 3:40 pm" when the last check failed and the numbers are older.
    pub as_of: Option<String>,
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

/// What a row says about an agent's account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    Ok,
    SignIn,
    NoSubscription,
    LimitReached,
    NotInstalled,
    /// No check has finished since the agent was added, signed in or installed.
    Checking,
    /// The last check could not reach the tool; the numbers (if any) are older.
    Unreachable,
}

impl Shown {
    pub fn key(self) -> &'static str {
        match self {
            Shown::Ok => "ok",
            Shown::SignIn => "sign-in",
            Shown::NoSubscription => "no-subscription",
            Shown::LimitReached => "limit-reached",
            Shown::NotInstalled => "not-installed",
            Shown::Checking => "checking",
            Shown::Unreachable => "unreachable",
        }
    }
}

/// Puts together what the tools say right now (installed, signed in) with what the last
/// usage check learned. Never "sign in" for a tool that only failed to answer.
pub fn shown_health(s: &AgentStatus, now: i64) -> Shown {
    match s.state {
        AgentState::NotInstalled => return Shown::NotInstalled,
        AgentState::NeedsSignIn => return Shown::SignIn,
        AgentState::Ready | AgentState::Working => {}
    }
    let week = s.limits.as_ref().and_then(|l| weekly(&l.windows));
    let reset_passed = week.is_some_and(|w| w.resets_at > 0 && w.resets_at <= now);
    let full = week.is_some_and(|w| rounded(w) >= 100) && !reset_passed;
    let saved = s.health.as_ref();
    match saved.and_then(SavedHealth::effective) {
        Some(Health::NoSubscription) => Shown::NoSubscription,
        _ if full => Shown::LimitReached,
        Some(Health::LimitReached) => {
            // Numbers newer than that check (the status line) or a passed reset overrule it.
            let checked_at = saved.map(|h| h.checked_at).unwrap_or(0);
            let newer = s.limits.as_ref().is_some_and(|l| l.as_of > checked_at);
            if newer || reset_passed {
                Shown::Ok
            } else {
                Shown::LimitReached
            }
        }
        // That check came before this sign-in or install: the next one is awaited.
        Some(Health::SignedOut | Health::NotInstalled) => Shown::Checking,
        _ if saved.is_some_and(SavedHealth::stale) => Shown::Unreachable,
        Some(_) => Shown::Ok,
        None if week.is_some() => Shown::Ok,
        None => Shown::Checking,
    }
}

/// The short words for an agent's usage when the row has no numbers to show, or has to say
/// more than numbers. Shared by the app and `roster`.
pub fn usage_note(s: &AgentStatus, shown: Shown, now: i64) -> Option<String> {
    let week = s.limits.as_ref().and_then(|l| weekly(&l.windows));
    match shown {
        Shown::NotInstalled => Some("not installed".into()),
        Shown::SignIn => Some("sign in".into()),
        Shown::NoSubscription => Some("no subscription".into()),
        Shown::Checking if week.is_none() => Some("checking".into()),
        Shown::LimitReached => Some(match week {
            Some(w) if w.resets_at > now => {
                format!("limit reached, {}", reset_phrase(w.resets_at, now))
            }
            _ => "limit reached".into(),
        }),
        Shown::Unreachable if week.is_none() => Some("couldn't check".into()),
        Shown::Ok if week.is_none() => Some(format!("usage shows after you use {}", s.agent.name)),
        _ => None,
    }
}

fn agent_view(s: &AgentStatus, o: &Overview, now: i64) -> AgentView {
    let a = &s.agent;
    let shown = shown_health(s, now);
    let windows: Vec<LimitWindow> = s
        .limits
        .as_ref()
        .and_then(|l| weekly(&l.windows).cloned())
        .into_iter()
        .filter(|_| shown != Shown::NoSubscription)
        .collect();
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
    let (reset_text, reset_hot) = match windows.first() {
        None => (None, false),
        Some(w) if w.resets_at <= 0 => (None, rounded(w) >= HOT),
        Some(w) => (Some(reset_phrase(w.resets_at, now)), rounded(w) >= HOT),
    };
    let limits_note = usage_note(s, shown, now);
    let as_of = match (shown, s.limits.as_ref()) {
        (Shown::Unreachable, Some(l)) if !windows.is_empty() => Some(as_of_local(l.as_of, now)),
        _ => None,
    };
    let (tray_text, tray_hot) = match (shown, windows.first()) {
        (Shown::NotInstalled, _) => ("not installed".to_string(), false),
        (Shown::SignIn, _) => ("sign in".to_string(), false),
        (Shown::NoSubscription, _) => ("no subscription".to_string(), false),
        (Shown::LimitReached, _) => ("limit reached".to_string(), true),
        (_, Some(w)) => (format!("{}% {}", rounded(w), w.label), rounded(w) >= HOT),
        (Shown::Checking, None) => ("checking".to_string(), false),
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
        good_at: crate::model::good_at_of(a),
        good_at_custom: a.good_at.is_some(),
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
        health: shown.key().into(),
        as_of,
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
        let hot = s
            .limits
            .as_ref()
            .and_then(|l| weekly(&l.windows))
            .filter(|w| rounded(w) >= HOT);
        if let Some(w) = hot {
            warnings.push(WarningView {
                name: s.agent.name.clone(),
                color: s.agent.color.hex().into(),
                text: format!(
                    "has used {}% of its week. It {}.",
                    rounded(w),
                    if w.resets_at <= now {
                        "is resetting now".to_string()
                    } else {
                        reset_phrase(w.resets_at, now)
                    }
                ),
            });
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
                        good_at: None,
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

/// `as_of_text` in this computer's time zone.
pub fn as_of_local(t: i64, now: i64) -> String {
    as_of_text(t, now, &jiff::tz::TimeZone::system())
}

/// "as of 3:40 pm" for a time today, "as of Nov 9" for an earlier day, in `tz`.
pub fn as_of_text(t: i64, now: i64, tz: &jiff::tz::TimeZone) -> String {
    let zoned = |s: i64| {
        jiff::Timestamp::from_second(s)
            .ok()
            .map(|ts| ts.to_zoned(tz.clone()))
    };
    let (Some(at), Some(today)) = (zoned(t), zoned(now)) else {
        return format!("as of {}", age(now - t));
    };
    if at.date() == today.date() {
        let h = match at.hour() % 12 {
            0 => 12,
            h => h,
        };
        let ampm = if at.hour() < 12 { "am" } else { "pm" };
        format!("as of {h}:{:02} {ampm}", at.minute())
    } else {
        const MONTHS: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        format!("as of {} {}", MONTHS[(at.month() - 1) as usize], at.day())
    }
}
