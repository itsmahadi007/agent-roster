use anstyle::{AnsiColor, Color, RgbColor, Style};
use roster_core::limits::format_reset;
use roster_core::model::AgentColor;
use roster_core::status::{AgentState, Overview};
use std::fmt::Write;

pub use roster_core::view::{age, now};

fn rgb(c: AgentColor) -> Color {
    let (r, g, b) = c.rgb();
    Color::Rgb(RgbColor(r, g, b))
}

fn paint(text: &str, st: Style) -> String {
    format!("{st}{text}{st:#}")
}

pub fn overview_text(o: &Overview) -> String {
    let mut s = String::new();
    let bold = Style::new().bold();
    let dim = Style::new().dimmed();
    let coral = Style::new().fg_color(Some(rgb(AgentColor::Coral)));
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
        let (dot, state_style) = match a.state {
            AgentState::Ready => ("●", Style::new().fg_color(Some(rgb(AgentColor::Jade)))),
            AgentState::NeedsSignIn => {
                ("●", Style::new().fg_color(Some(rgb(AgentColor::Marigold))))
            }
            AgentState::Working => ("●", Style::new().fg_color(Some(rgb(a.agent.color)))),
            AgentState::NotInstalled => ("○", dim),
        };
        let state = format!("{dot} {:<13}", a.state.label());
        let lead = format!(
            "  {:<name_w$}  {:<11}  ",
            a.agent.name,
            a.agent.tool.display()
        );
        let lead_painted = format!(
            "  {}  {:<11}  ",
            paint(&format!("{:<name_w$}", a.agent.name), color),
            a.agent.tool.display()
        );
        let pad = " ".repeat(lead.chars().count() + 15);
        let _ = write!(s, "{lead_painted}{}", paint(&state, state_style));
        match &a.limits {
            Some(l) if !l.windows.is_empty() => {
                for (i, w) in l.windows.iter().enumerate() {
                    if i > 0 {
                        s.push('\n');
                        s.push_str(&pad);
                    }
                    let filled = ((w.used_percent / 10.0).round() as usize).min(10);
                    let hot = w.used_percent >= 90.0;
                    let bar_style = if hot {
                        coral
                    } else {
                        Style::new().fg_color(Some(rgb(a.agent.color)))
                    };
                    let pct = format!("{:>3.0}%", w.used_percent);
                    let _ = write!(
                        s,
                        "{:<5} {}{}  {}  {}",
                        w.label,
                        paint(&"█".repeat(filled), bar_style),
                        paint(&"░".repeat(10 - filled), dim),
                        if hot { paint(&pct, coral) } else { pct },
                        paint(
                            &format!("resets in {}", format_reset(w.resets_at, now())),
                            if hot { coral } else { dim }
                        )
                    );
                }
            }
            _ => {
                let note = match (a.state, a.agent.tool, a.agent.limits) {
                    (AgentState::NotInstalled, _, _) => "",
                    (AgentState::NeedsSignIn, _, _) => "limits appear after sign in",
                    (
                        _,
                        roster_core::model::Tool::Claude,
                        roster_core::model::LimitsSource::Off,
                    ) => "limits off: roster limits enable",
                    _ => "no limit data yet",
                };
                let _ = write!(s, "{}", paint(note, dim));
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
                "status": a.state.label(),
                "email": a.sign_in.email,
                "plan": a.sign_in.plan.clone().or_else(|| a.limits.as_ref().and_then(|l| l.plan.clone())),
                "limits": a.limits,
            })
        })
        .collect();
    serde_json::json!({ "agents": agents, "calls": o.calls.iter().map(|c| serde_json::json!({
        "from": c.parent_id, "to": c.agent_id, "task": c.task, "cwd": c.cwd, "started_at": c.started_at
    })).collect::<Vec<_>>() })
}
