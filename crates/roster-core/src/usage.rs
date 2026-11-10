//! Asks Claude Code itself for an agent's usage: `claude -p /usage`.
//!
//! `/usage` runs inside Claude Code without calling the model (no turns, no cost), and Claude
//! Code fetches the numbers with the agent's own login. Agent Roster never opens a credential
//! file. Works the same on macOS, Linux and Windows, with no terminal and no status line.

use crate::detect::{CommandRunner, tool_info};
use crate::limits::{AgentLimits, LimitWindow, write_claude_limits};
use crate::model::{Agent, Roster, Tool};
use crate::paths::RosterPaths;
use std::path::{Path, PathBuf};

/// No saved session, and no user or project settings, so the user's hooks don't run.
pub const USAGE_ARGS: &[&str] = &[
    "-p",
    "/usage",
    "--output-format",
    "json",
    "--no-session-persistence",
    "--setting-sources",
    "local",
];

#[derive(Debug, PartialEq)]
pub enum UsageReply {
    Limits(AgentLimits),
    /// This Claude Code sent `/usage` to the model: it has no local `/usage` in print mode.
    Unsupported,
    Nothing,
}

/// Reads the JSON `claude -p /usage --output-format json` prints.
pub fn parse_usage_reply(json: &str, now: i64) -> UsageReply {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json.trim()) else {
        return UsageReply::Nothing;
    };
    let turns = v.get("num_turns").and_then(|x| x.as_u64()).unwrap_or(0);
    let cost = v
        .get("total_cost_usd")
        .and_then(|x| x.as_f64())
        .unwrap_or(0.0);
    if turns > 0 || cost > 0.0 {
        return UsageReply::Unsupported;
    }
    let Some(text) = v.get("result").and_then(|x| x.as_str()) else {
        return UsageReply::Nothing;
    };
    match parse_usage_text(text, now) {
        Some(w) => UsageReply::Limits(AgentLimits {
            as_of: now,
            plan: None,
            windows: vec![w],
        }),
        None => UsageReply::Nothing,
    }
}

/// The weekly line of `/usage`, for all models:
/// `Current week (all models): 72% used · resets Nov 13, 6am (Asia/Dhaka)`.
/// A reset time it can't read becomes 0 (unknown); the percent still counts.
pub fn parse_usage_text(text: &str, now: i64) -> Option<LimitWindow> {
    let weeks: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("Current week"))
        .collect();
    let line = weeks
        .iter()
        .find(|l| l.contains("(all models)"))
        .or(weeks.first())?;
    let rest = line.split_once(':')?.1;
    let used: f64 = rest.split('%').next()?.trim().parse().ok()?;
    let resets_at = rest
        .split_once("resets ")
        .and_then(|(_, r)| parse_reset(r, now))
        .unwrap_or(0);
    Some(LimitWindow {
        label: "week".into(),
        used_percent: used,
        resets_at,
    })
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

/// `Nov 13, 6am (Asia/Dhaka)`, `Nov 13, 5:59pm`, or `6am (Asia/Dhaka)` for later today.
/// Unix seconds, or None when it can't be read.
pub fn parse_reset(s: &str, now: i64) -> Option<i64> {
    use jiff::tz::TimeZone;
    let s = s.trim();
    let (when, tz) = match s.rsplit_once(" (") {
        Some((w, z)) => (w, Some(z.trim_end_matches(')').trim())),
        None => (s, None),
    };
    let tz = tz
        .and_then(|z| TimeZone::get(z).ok())
        .unwrap_or_else(TimeZone::system);
    let words: Vec<String> = when
        .replace(',', " ")
        .split_whitespace()
        .filter(|w| !w.eq_ignore_ascii_case("at"))
        .map(|w| w.to_ascii_lowercase())
        .collect();
    let mut month = None;
    let mut day = None;
    let mut time = None;
    let mut i = 0;
    while i < words.len() {
        let w = &words[i];
        if let Some(m) = MONTHS.iter().position(|m| w.starts_with(m)) {
            month = Some(m as i8 + 1);
        } else if w.ends_with("am") || w.ends_with("pm") {
            time = Some(parse_clock(w)?);
        } else if words
            .get(i + 1)
            .is_some_and(|n| n.as_str() == "am" || n.as_str() == "pm")
        {
            time = Some(parse_clock(&format!("{w}{}", words[i + 1]))?);
            i += 1;
        } else if let Ok(d) = w.parse::<i8>() {
            day = Some(d);
        } else {
            return None;
        }
        i += 1;
    }
    let (hour, minute) = time.unwrap_or((0, 0));
    let today = jiff::Timestamp::from_second(now).ok()?.to_zoned(tz.clone());
    let at = |date: jiff::civil::Date| -> Option<i64> {
        Some(
            date.at(hour, minute, 0, 0)
                .to_zoned(tz.clone())
                .ok()?
                .timestamp()
                .as_second(),
        )
    };
    match (month, day) {
        (Some(m), Some(d)) => {
            let this_year = at(jiff::civil::Date::new(today.year(), m, d).ok()?)?;
            // No year is printed: a date well in the past means next year's.
            if this_year < now - 2 * 86_400 {
                at(jiff::civil::Date::new(today.year() + 1, m, d).ok()?)
            } else {
                Some(this_year)
            }
        }
        (None, None) if time.is_some() => {
            let t = at(today.date())?;
            if t < now {
                at(today.date().tomorrow().ok()?)
            } else {
                Some(t)
            }
        }
        _ => None,
    }
}

/// `6am`, `5:59pm`, `12am` as (hour, minute) on a 24-hour clock.
fn parse_clock(w: &str) -> Option<(i8, i8)> {
    let pm = w.ends_with("pm");
    let digits = &w[..w.len() - 2];
    let (h, m) = match digits.split_once(':') {
        Some((h, m)) => (h.parse::<i8>().ok()?, m.parse::<i8>().ok()?),
        None => (digits.parse::<i8>().ok()?, 0),
    };
    if !(1..=12).contains(&h) || !(0..60).contains(&m) {
        return None;
    }
    Some(((h % 12) + if pm { 12 } else { 0 }, m))
}

fn unsupported_marker(p: &RosterPaths) -> PathBuf {
    p.limits_dir().join("claude-usage-unsupported")
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Asks Claude Code for one agent's usage. `version` is the Claude Code version: one that
/// once sent `/usage` to the model is never asked again, so a check can't cost a turn twice.
pub fn fetch_claude_usage(
    agent: &Agent,
    r: &Roster,
    p: &RosterPaths,
    tool_path: &Path,
    version: Option<&str>,
    runner: &dyn CommandRunner,
) -> Option<AgentLimits> {
    let version = version.unwrap_or("unknown");
    if std::fs::read_to_string(unsupported_marker(p)).is_ok_and(|v| v.trim() == version) {
        return None;
    }
    let env = crate::run::build_env(agent, r, p, None);
    let out = runner.run(tool_path, USAGE_ARGS, &env).ok()?;
    match parse_usage_reply(&String::from_utf8_lossy(&out.stdout), now()) {
        UsageReply::Limits(l) => Some(l),
        UsageReply::Unsupported => {
            let _ = std::fs::create_dir_all(p.limits_dir());
            let _ = std::fs::write(unsupported_marker(p), version);
            None
        }
        UsageReply::Nothing => None,
    }
}

/// Asks Claude Code for every Claude agent's usage, side by side, and saves what comes back
/// where the overview reads it. Returns how many agents got fresh numbers.
pub fn refresh_claude_usage(
    r: &Roster,
    p: &RosterPaths,
    path_var: &str,
    runner: &(dyn CommandRunner + Sync),
) -> usize {
    let info = tool_info(Tool::Claude, path_var, &p.bin_dir(), runner);
    let Some(path) = info.path else {
        return 0;
    };
    let version = info.version.as_deref();
    std::thread::scope(|s| {
        let handles: Vec<_> = r
            .agents
            .iter()
            .filter(|a| a.tool == Tool::Claude)
            .map(|a| {
                let path = &path;
                s.spawn(move || {
                    fetch_claude_usage(a, r, p, path, version, runner)
                        .is_some_and(|l| write_claude_limits(p, &a.id, &l).is_ok())
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or(false))
            .filter(|ok| *ok)
            .count()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DHAKA_NOV_10_1PM: i64 = 1_794_294_000; // 2026-11-10 13:00 Asia/Dhaka (UTC+6)

    fn reply(result: &str, turns: u64, cost: f64) -> String {
        serde_json::json!({"type": "result", "num_turns": turns, "total_cost_usd": cost, "result": result})
            .to_string()
    }

    const USAGE: &str = "You are currently using your subscription to power your Claude Code usage\n\nCurrent week (all models): 72% used · resets Nov 13, 6am (Asia/Dhaka)\nCurrent week (Fable): 18% used · resets Nov 13, 5:59am (Asia/Dhaka)\n\nWhat's contributing to your usage?";

    #[test]
    fn reads_the_all_models_week() {
        let w = parse_usage_text(USAGE, DHAKA_NOV_10_1PM).unwrap();
        assert_eq!(w.label, "week");
        assert_eq!(w.used_percent, 72.0);
        // 2026-11-13 06:00 at UTC+6 is 2026-11-13 00:00 UTC.
        assert_eq!(w.resets_at, 1_794_528_000);
    }

    #[test]
    fn reset_with_minutes_and_pm() {
        assert_eq!(
            parse_reset("Nov 13, 5:59pm (Asia/Dhaka)", DHAKA_NOV_10_1PM),
            Some(1_794_528_000 + 11 * 3600 + 59 * 60)
        );
    }

    #[test]
    fn reset_with_only_a_time_is_the_next_one() {
        // 6am has passed at 1pm, so it is tomorrow's 6am.
        assert_eq!(
            parse_reset("6am (Asia/Dhaka)", DHAKA_NOV_10_1PM),
            Some(1_794_528_000 - 2 * 86_400)
        );
        assert_eq!(
            parse_reset("3pm (Asia/Dhaka)", DHAKA_NOV_10_1PM),
            Some(DHAKA_NOV_10_1PM + 2 * 3600)
        );
    }

    #[test]
    fn reset_early_january_seen_in_december_is_next_year() {
        let dec_30 = 1_798_614_000; // 2026-12-30 13:00 UTC+6
        let jan_2 = parse_reset("Jan 2, 6am (Asia/Dhaka)", dec_30).unwrap();
        assert!(jan_2 > dec_30 && jan_2 - dec_30 < 4 * 86_400);
    }

    #[test]
    fn unreadable_reset_keeps_the_percent() {
        let w = parse_usage_text(
            "Current week (all models): 40% used · resets sometime soon",
            DHAKA_NOV_10_1PM,
        )
        .unwrap();
        assert_eq!((w.used_percent, w.resets_at), (40.0, 0));
    }

    #[test]
    fn falls_back_to_any_week_line() {
        let w = parse_usage_text("Current week: 5% used", DHAKA_NOV_10_1PM).unwrap();
        assert_eq!(w.used_percent, 5.0);
    }

    #[test]
    fn reply_with_a_model_turn_is_unsupported() {
        assert_eq!(
            parse_usage_reply(&reply("I can't run /usage", 1, 0.01), 0),
            UsageReply::Unsupported
        );
    }

    #[test]
    fn reply_without_numbers_or_json_is_nothing() {
        assert_eq!(
            parse_usage_reply(&reply("Not logged in", 0, 0.0), 0),
            UsageReply::Nothing
        );
        assert_eq!(parse_usage_reply("garbage", 0), UsageReply::Nothing);
    }

    #[test]
    fn reply_with_numbers() {
        match parse_usage_reply(&reply(USAGE, 0, 0.0), DHAKA_NOV_10_1PM) {
            UsageReply::Limits(l) => {
                assert_eq!(l.as_of, DHAKA_NOV_10_1PM);
                assert_eq!(l.windows[0].used_percent, 72.0);
            }
            other => panic!("{other:?}"),
        }
    }
}
