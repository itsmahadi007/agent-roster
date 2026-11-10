//! Asks Claude Code itself for an agent's usage: `claude -p /usage`.
//!
//! `/usage` runs inside Claude Code without calling the model (no turns, no cost), and Claude
//! Code fetches the numbers with the agent's own login. Agent Roster never opens a credential
//! file. Works the same on macOS, Linux and Windows, with no terminal and no status line.

use super::{Check, Health};
use crate::detect::CommandRunner;
use crate::limits::{AgentLimits, LimitWindow};
use crate::model::{Agent, Roster};
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
    /// Signed out or an API key: `/usage` printed only a cost summary, no plan limits.
    NoPlan,
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
        // Without a plan, `/usage` prints this session's cost instead of plan limits.
        None if text.contains("Total cost:") => UsageReply::NoPlan,
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

/// What `claude auth status --json` says about the account. `None` when the reply can't be
/// read (the tool crashed or printed something else): that is not the same as signed out.
pub fn claude_health(json: &str) -> Option<Health> {
    let v: serde_json::Value = serde_json::from_str(json.trim()).ok()?;
    if !v.get("loggedIn")?.as_bool()? {
        return Some(Health::SignedOut);
    }
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("");
    let plan = s("subscriptionType");
    let has_plan = !plan.is_empty() && !plan.eq_ignore_ascii_case("free");
    Some(match s("authMethod") {
        // Older versions print no method: `/usage` tells whether there is a plan.
        "" if v.get("apiKeySource").is_none() => Health::Ok,
        "claude.ai" if has_plan => Health::Ok,
        // An API key, Bedrock, Vertex, or claude.ai with no plan: no weekly limit to show.
        _ => Health::NoSubscription,
    })
}

/// What asking `/usage` gave.
enum Asked {
    Limits(AgentLimits),
    NoPlan,
    /// Not asked: this Claude Code version can't answer without the model.
    Skipped,
    Failed,
}

fn ask_usage(
    agent: &Agent,
    r: &Roster,
    p: &RosterPaths,
    tool_path: &Path,
    version: Option<&str>,
    runner: &dyn CommandRunner,
) -> Asked {
    let version = version.unwrap_or("unknown");
    if std::fs::read_to_string(unsupported_marker(p)).is_ok_and(|v| v.trim() == version) {
        return Asked::Skipped;
    }
    let env = crate::run::build_env(agent, r, p, None);
    let Ok(out) = runner.run(tool_path, USAGE_ARGS, &env) else {
        return Asked::Failed;
    };
    match parse_usage_reply(&String::from_utf8_lossy(&out.stdout), now()) {
        UsageReply::Limits(l) => Asked::Limits(l),
        UsageReply::NoPlan => Asked::NoPlan,
        UsageReply::Unsupported => {
            let _ = std::fs::create_dir_all(p.limits_dir());
            let _ = std::fs::write(unsupported_marker(p), version);
            Asked::Skipped
        }
        UsageReply::Nothing => Asked::Failed,
    }
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
    match ask_usage(agent, r, p, tool_path, version, runner) {
        Asked::Limits(l) => Some(l),
        _ => None,
    }
}

/// Checks one Claude agent: its account from `claude auth status --json`, then its numbers
/// from `/usage` (only for a signed-in subscription). A tool that can't be asked gives
/// `Unreachable`, never `SignedOut`.
pub fn check_claude(
    agent: &Agent,
    r: &Roster,
    p: &RosterPaths,
    tool_path: &Path,
    version: Option<&str>,
    runner: &dyn CommandRunner,
) -> Check {
    let env = crate::run::build_env(agent, r, p, None);
    let Ok(out) = runner.run(tool_path, &["auth", "status", "--json"], &env) else {
        return Check::health(Health::Unreachable);
    };
    let Some(health) = claude_health(&String::from_utf8_lossy(&out.stdout)) else {
        return Check::health(Health::Unreachable);
    };
    if health != Health::Ok {
        return Check::health(health);
    }
    match ask_usage(agent, r, p, tool_path, version, runner) {
        Asked::Limits(l) => Check {
            health: Health::Ok,
            limits: Some(l),
        }
        .settled(),
        Asked::NoPlan => Check::health(Health::NoSubscription),
        Asked::Skipped => Check::health(Health::Ok),
        Asked::Failed => Check::health(Health::Unreachable),
    }
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

#[cfg(test)]
mod check_tests {
    use super::*;
    use crate::model::{AgentColor, LimitsSource, Tool};
    use crate::paths::Platform;
    use crate::usage::testing::Scripted;

    const SIGNED_IN: &str = include_str!("../../tests/fixtures/claude-auth-status.json");
    const SIGNED_OUT: &str =
        include_str!("../../tests/fixtures/claude-auth-status-signed-out.json");
    const API_KEY: &str = include_str!("../../tests/fixtures/claude-auth-status-api-key.json");
    const USAGE_SUB: &str = include_str!("../../tests/fixtures/claude-usage-subscription.json");
    const USAGE_OUT: &str = include_str!("../../tests/fixtures/claude-usage-signed-out.json");
    const USAGE_KEY: &str = include_str!("../../tests/fixtures/claude-usage-api-key.json");

    fn agent() -> Agent {
        Agent {
            id: "w1".into(),
            name: "worker1".into(),
            tool: Tool::Claude,
            config_dir: "~/.agent-roster/accounts/w1".into(),
            color: AgentColor::Cobalt,
            role: None,
            default: false,
            share: false,
            limits: LimitsSource::Off,
            advanced_links: vec![],
            good_at: None,
        }
    }

    fn check(runner: &Scripted) -> Check {
        let d = tempfile::tempdir().unwrap();
        let p = RosterPaths::for_home(d.path(), Platform::current(), &|_| None);
        let r = Roster {
            version: 1,
            agents: vec![agent()],
            statusline: vec![],
        };
        check_claude(
            &r.agents[0],
            &r,
            &p,
            Path::new("claude"),
            Some("2.1.322"),
            runner,
        )
    }

    #[test]
    fn auth_status_says_who_has_a_plan() {
        assert_eq!(claude_health(SIGNED_IN), Some(Health::Ok));
        assert_eq!(claude_health(SIGNED_OUT), Some(Health::SignedOut));
        assert_eq!(claude_health(API_KEY), Some(Health::NoSubscription));
        // Signed in to claude.ai, but no plan (none bought, or it ended).
        assert_eq!(
            claude_health(r#"{"loggedIn":true,"authMethod":"claude.ai","subscriptionType":null}"#),
            Some(Health::NoSubscription)
        );
        assert_eq!(
            claude_health(
                r#"{"loggedIn":true,"authMethod":"claude.ai","subscriptionType":"free"}"#
            ),
            Some(Health::NoSubscription)
        );
        // Bedrock, Vertex and other providers bill per use: no plan limits.
        assert_eq!(
            claude_health(
                r#"{"loggedIn":true,"authMethod":"third_party","apiProvider":"bedrock"}"#
            ),
            Some(Health::NoSubscription)
        );
    }

    #[test]
    fn unreadable_auth_status_is_not_signed_out() {
        assert_eq!(claude_health(""), None);
        assert_eq!(claude_health("Error: something broke"), None);
        assert_eq!(claude_health("{}"), None);
    }

    #[test]
    fn usage_without_a_plan_prints_only_cost() {
        assert_eq!(parse_usage_reply(USAGE_OUT, 0), UsageReply::NoPlan);
        assert_eq!(parse_usage_reply(USAGE_KEY, 0), UsageReply::NoPlan);
        match parse_usage_reply(USAGE_SUB, 1_794_294_000) {
            UsageReply::Limits(l) => assert_eq!(l.windows[0].used_percent, 75.0),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn signed_out_is_sign_in_without_asking_usage() {
        let runner = Scripted::new(Some(SIGNED_OUT), Some(USAGE_OUT));
        assert_eq!(check(&runner), Check::health(Health::SignedOut));
        assert!(!runner.asked_usage());
    }

    #[test]
    fn api_key_is_no_subscription() {
        let runner = Scripted::new(Some(API_KEY), Some(USAGE_KEY));
        assert_eq!(check(&runner).health, Health::NoSubscription);
    }

    #[test]
    fn subscription_gets_numbers() {
        let c = check(&Scripted::new(Some(SIGNED_IN), Some(USAGE_SUB)));
        assert_eq!(c.health, Health::Ok);
        assert_eq!(c.limits.unwrap().windows[0].used_percent, 75.0);
    }

    #[test]
    fn a_full_week_is_limit_reached() {
        let full = USAGE_SUB.replace("75% used", "100% used");
        let c = check(&Scripted::new(Some(SIGNED_IN), Some(&full)));
        assert_eq!(c.health, Health::LimitReached);
        assert_eq!(c.limits.unwrap().windows[0].used_percent, 100.0);
    }

    #[test]
    fn a_timeout_is_unreachable_never_signed_out() {
        assert_eq!(
            check(&Scripted::new(None, Some(USAGE_SUB))),
            Check::health(Health::Unreachable)
        );
        assert_eq!(
            check(&Scripted::new(Some(SIGNED_IN), None)),
            Check::health(Health::Unreachable)
        );
        assert_eq!(
            check(&Scripted::new(Some("crashed"), Some(USAGE_SUB))),
            Check::health(Health::Unreachable)
        );
    }
}
