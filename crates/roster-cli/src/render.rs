use anstyle::{AnsiColor, Color, RgbColor, Style};
use roster_core::limits::{LimitWindow, reset_phrase, weekly};
use roster_core::model::AgentColor;
use roster_core::status::{AgentState, Overview};
use roster_core::view::{Shown, as_of_local, shown_health, usage_note};
use std::fmt::Write;

pub use roster_core::view::{age, now};

fn rgb(c: AgentColor) -> Color {
    let (r, g, b) = c.rgb();
    Color::Rgb(RgbColor(r, g, b))
}

fn paint(text: &str, st: Style) -> String {
    format!("{st}{text}{st:#}")
}

/// Usage and status colors, the same as the app's. Never an agent's own color: an agent's
/// color says who it is, these say how much is used or what state it is in.
const HOT: (u8, u8, u8) = (0xff, 0x5a, 0x4f);
const WARN: (u8, u8, u8) = (0xf2, 0xb6, 0x32);
const CALM: (u8, u8, u8) = (0xc3, 0xce, 0xde);
const READY: (u8, u8, u8) = (0x3c, 0xcf, 0x8e);

fn fg((r, g, b): (u8, u8, u8)) -> Style {
    Style::new().fg_color(Some(Color::Rgb(RgbColor(r, g, b))))
}

/// The bar's color from usage alone: under 70 calm, 70–89 amber, 90 and up red.
pub fn usage_color(used_percent: f64) -> (u8, u8, u8) {
    if used_percent >= 90.0 {
        HOT
    } else if used_percent >= 70.0 {
        WARN
    } else {
        CALM
    }
}

/// One limit in the table: "week ████████░░   94%  resets in 21h 52m".
fn window_line(w: &LimitWindow, now: i64) -> String {
    let dim = Style::new().dimmed();
    let filled = ((w.used_percent / 10.0).round() as usize).min(10);
    let level = usage_color(w.used_percent);
    let hot = level == HOT;
    let pct = format!("{:>3.0}%", w.used_percent);
    format!(
        "{:<5} {}{}  {}  {}",
        w.label,
        paint(&"█".repeat(filled), fg(level)),
        paint(&"░".repeat(10 - filled), dim),
        if level == CALM {
            pct
        } else {
            paint(&pct, fg(level))
        },
        paint(
            &if w.resets_at > 0 {
                reset_phrase(w.resets_at, now)
            } else {
                String::new()
            },
            if hot { fg(HOT) } else { dim }
        )
    )
}

pub fn overview_text(o: &Overview) -> String {
    let mut s = String::new();
    let bold = Style::new().bold();
    let dim = Style::new().dimmed();
    let newest = o
        .agents
        .iter()
        .filter_map(|a| a.limits.as_ref().map(|l| l.as_of))
        .max();
    let header = format!("{} agents", o.agents.len());
    let _ = write!(
        s,
        "\n {}  {}",
        paint("Agent Roster", bold),
        paint(&header, dim)
    );
    if let Some(t) = newest {
        let _ = write!(
            s,
            "      {}",
            paint(&format!("limits as of {}", age(now() - t)), dim)
        );
    }
    s.push_str("\n\n");
    let name_w = o
        .agents
        .iter()
        .map(|a| a.agent.name.len())
        .max()
        .unwrap_or(6)
        .max(6);
    for a in &o.agents {
        let color = Style::new().fg_color(Some(rgb(a.agent.color))).bold();
        let shown = shown_health(a, now());
        // `codex login status` says "Logged in" for an expired login; the check knows better.
        let state_now = match a.state {
            AgentState::Ready if shown == Shown::SignIn => AgentState::NeedsSignIn,
            st => st,
        };
        let (dot, state_style) = match state_now {
            AgentState::Ready => ("●", fg(READY)),
            AgentState::NeedsSignIn => ("●", fg(WARN)),
            AgentState::Working => ("●", Style::new().fg_color(Some(rgb(a.agent.color)))),
            AgentState::NotInstalled => ("○", dim),
        };
        let state = format!("{dot} {:<13}", state_now.label());
        let lead_painted = format!(
            "  {}  {:<11}  ",
            paint(&format!("{:<name_w$}", a.agent.name), color),
            a.agent.tool.display()
        );
        let _ = write!(s, "{lead_painted}{}", paint(&state, state_style));
        let note = usage_note(a, shown, now()).unwrap_or_default();
        match a.limits.as_ref().and_then(|l| weekly(&l.windows)) {
            Some(w) if shown != Shown::NoSubscription => {
                s.push_str(&window_line(w, now()));
                if shown == Shown::Unreachable {
                    let at = a.limits.as_ref().map(|l| l.as_of).unwrap_or(0);
                    let _ = write!(s, "  {}", paint(&as_of_local(at, now()), dim));
                }
                if shown == Shown::LimitReached {
                    // Which limit, with its own reset (the week line shows the week's).
                    let _ = write!(s, "  {}", paint(&note, fg(HOT)));
                }
            }
            // The state column already says it.
            _ if shown == Shown::NotInstalled => {}
            _ => {
                let _ = write!(s, "{}", paint(&note, dim));
            }
        }
        s.push('\n');
    }
    if !o.calls.is_empty() {
        s.push('\n');
        for c in &o.calls {
            let find = |id: &str| o.agents.iter().find(|a| a.agent.id == id);
            let (Some(parent), Some(child)) =
                (c.parent_id.as_deref().and_then(find), find(&c.agent_id))
            else {
                continue;
            };
            let task = c
                .task
                .as_deref()
                .map(|t| format!("  \"{t}\""))
                .unwrap_or_default();
            let _ = writeln!(
                s,
                "  {} {} {}{}  {}",
                paint(
                    &parent.agent.name,
                    Style::new().fg_color(Some(rgb(parent.agent.color))).bold()
                ),
                paint(
                    "━━━━━━━━━━━━━━━▶",
                    Style::new().fg_color(Some(rgb(child.agent.color)))
                ),
                paint(
                    &child.agent.name,
                    Style::new().fg_color(Some(rgb(child.agent.color))).bold()
                ),
                task,
                paint(&age(now() - c.started_at).replace(" ago", ""), dim)
            );
        }
    }
    let hint = Style::new().fg_color(Some(AnsiColor::BrightBlack.into()));
    let _ = writeln!(
        s,
        "\n  {}",
        paint(
            "roster add <name>    roster limits    roster ui    roster changes",
            hint
        )
    );
    s
}

pub fn overview_json(o: &Overview) -> serde_json::Value {
    let agents: Vec<serde_json::Value> = o
        .agents
        .iter()
        .map(|a| {
            serde_json::json!({
                "id": a.agent.id,
                "name": a.agent.name,
                "tool": a.agent.tool,
                "config_dir": a.agent.config_dir,
                "color": a.agent.color,
                "role": a.agent.role,
                "default": a.agent.default,
                "share": a.agent.share,
                "good_at": roster_core::model::good_at_of(&a.agent),
                "status": a.state.label(),
                "email": a.sign_in.email,
                "plan": a.sign_in.plan.clone().or_else(|| a.limits.as_ref().and_then(|l| l.plan.clone())),
                "limits": a.limits,
                "health": shown_health(a, now()).key(),
            })
        })
        .collect();
    serde_json::json!({ "agents": agents, "calls": o.calls.iter().map(|c| serde_json::json!({
        "from": c.parent_id, "to": c.agent_id, "task": c.task, "cwd": c.cwd, "started_at": c.started_at
    })).collect::<Vec<_>>() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overview_shows_week_only() {
        use roster_core::detect::SignIn;
        use roster_core::limits::AgentLimits;
        use roster_core::model::{Agent, LimitsSource, Tool};
        use roster_core::status::{AgentState, AgentStatus, Overview};
        let w = |label: &str, used: f64| LimitWindow {
            label: label.into(),
            used_percent: used,
            resets_at: now() + 3600,
        };
        let o = Overview {
            agents: vec![AgentStatus {
                agent: Agent {
                    id: "a1".into(),
                    name: "agent1".into(),
                    tool: Tool::Claude,
                    config_dir: "~/.claude".into(),
                    color: AgentColor::Cobalt,
                    role: None,
                    default: false,
                    share: false,
                    limits: LimitsSource::ClaudeStatusline,
                    advanced_links: vec![],
                    good_at: None,
                },
                state: AgentState::Ready,
                sign_in: SignIn {
                    signed_in: true,
                    email: None,
                    plan: None,
                },
                limits: Some(AgentLimits {
                    as_of: now(),
                    plan: None,
                    windows: vec![w("5 hr", 22.0), w("week", 61.0)],
                }),
                health: None,
                sign_in_known: true,
                signed_in_at: None,
            }],
            calls: vec![],
            tools: vec![],
        };
        let text = overview_text(&o);
        assert!(text.contains("week"), "{text}");
        assert!(!text.contains("5 hr"), "{text}");
    }

    #[test]
    fn rows_use_the_health_words() {
        use roster_core::detect::SignIn;
        use roster_core::model::{Agent, LimitsSource, Tool};
        use roster_core::status::{AgentState, AgentStatus, Overview};
        use roster_core::usage::{Health, SavedHealth};
        let row = |name: &str, health: Option<Health>| AgentStatus {
            agent: Agent {
                id: name.into(),
                name: name.into(),
                tool: Tool::Claude,
                config_dir: "~/.claude".into(),
                color: AgentColor::Cobalt,
                role: None,
                default: false,
                share: false,
                limits: LimitsSource::Off,
                advanced_links: vec![],
                good_at: None,
            },
            state: AgentState::Ready,
            sign_in: SignIn {
                signed_in: true,
                email: None,
                plan: None,
            },
            limits: None,
            health: health.map(|h| SavedHealth {
                health: h,
                checked_at: now(),
                known: Some(h),
            }),
            sign_in_known: true,
            signed_in_at: None,
        };
        let o = Overview {
            agents: vec![
                row("fresh", None),
                row("apikey", Some(Health::NoSubscription)),
            ],
            calls: vec![],
            tools: vec![],
        };
        let text = overview_text(&o);
        assert!(text.contains("checking"), "{text}");
        assert!(text.contains("no subscription"), "{text}");
        assert!(!text.contains("usage shows after"), "{text}");
        let json = overview_json(&o);
        assert_eq!(json["agents"][0]["health"], "checking");
        assert_eq!(json["agents"][1]["health"], "no-subscription");
    }

    fn codex_row(health: roster_core::usage::Health, windows: Vec<LimitWindow>) -> Overview {
        use roster_core::detect::SignIn;
        use roster_core::limits::AgentLimits;
        use roster_core::model::{Agent, LimitsSource, Tool};
        use roster_core::status::AgentStatus;
        use roster_core::usage::SavedHealth;
        Overview {
            agents: vec![AgentStatus {
                agent: Agent {
                    id: "x1".into(),
                    name: "boss".into(),
                    tool: Tool::Codex,
                    config_dir: "~/.codex".into(),
                    color: AgentColor::Orchid,
                    role: None,
                    default: false,
                    share: false,
                    limits: LimitsSource::Off,
                    advanced_links: vec![],
                    good_at: None,
                },
                state: AgentState::Ready,
                sign_in: SignIn {
                    signed_in: true,
                    email: None,
                    plan: None,
                },
                limits: Some(AgentLimits {
                    as_of: now() - 60,
                    plan: None,
                    windows,
                }),
                health: Some(SavedHealth {
                    health,
                    checked_at: now() - 30,
                    known: Some(health),
                }),
                sign_in_known: true,
                signed_in_at: None,
            }],
            calls: vec![],
            tools: vec![],
        }
    }

    #[test]
    fn an_expired_login_says_sign_in_even_with_numbers() {
        use roster_core::usage::Health;
        let week = LimitWindow {
            label: "week".into(),
            used_percent: 30.0,
            resets_at: now() + 86_400,
        };
        let text = overview_text(&codex_row(Health::SignedOut, vec![week]));
        assert!(text.contains("sign in"), "{text}");
        assert!(!text.contains("ready"), "{text}");
        assert_eq!(
            overview_json(&codex_row(Health::SignedOut, vec![]))["agents"][0]["health"],
            "sign-in"
        );
    }

    #[test]
    fn a_used_up_five_hour_limit_says_which_and_its_reset() {
        use roster_core::usage::Health;
        let w = |label: &str, used: f64, secs: i64| LimitWindow {
            label: label.into(),
            used_percent: used,
            resets_at: now() + secs,
        };
        let text = overview_text(&codex_row(
            Health::LimitReached,
            vec![
                w("5 hr", 100.0, 2 * 3600 + 120),
                w("week", 40.0, 5 * 86_400 + 7200),
            ],
        ));
        assert!(
            text.contains("5-hour limit reached, resets in 2h"),
            "{text}"
        );
        assert!(text.contains("week"), "{text}");
    }

    #[test]
    fn window_line_past_reset() {
        let now = 1_000_000;
        let w = LimitWindow {
            label: "week".into(),
            used_percent: 94.0,
            resets_at: now - 1,
        };
        let line = window_line(&w, now);
        assert!(line.contains("resetting now"), "{line}");
        assert!(!line.contains("resets in resetting"), "{line}");
    }

    #[test]
    fn bar_color_comes_from_usage_never_from_the_agent() {
        assert_eq!(usage_color(12.0), CALM);
        assert_eq!(usage_color(69.9), CALM);
        assert_eq!(usage_color(70.0), WARN);
        assert_eq!(usage_color(89.0), WARN);
        assert_eq!(usage_color(90.0), HOT);
        assert_eq!(usage_color(100.0), HOT);
        // No agent color is a usage color, so a row can't look "almost out" by its color.
        for c in AgentColor::ORDER {
            assert!(
                ![HOT, WARN].contains(&c.rgb()),
                "{} is a warning color",
                c.label()
            );
        }
    }
}
