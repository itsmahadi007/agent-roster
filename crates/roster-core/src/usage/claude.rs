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
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// No saved session, no user or project settings (so the user's hooks don't run), and no
/// MCP servers started.
pub const USAGE_ARGS: &[&str] = &[
    "-p",
    "/usage",
    "--output-format",
    "json",
    "--no-session-persistence",
    "--setting-sources",
    "local",
    "--strict-mcp-config",
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

/// Reads the JSON `claude -p /usage --output-format json` prints. Only a reply that shows
/// it ran locally counts: no turns, no cost, and `"local_command": "usage"`.
pub fn parse_usage_reply(json: &str, now: i64) -> UsageReply {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json.trim()) else {
        return UsageReply::Nothing;
    };
    let turns = v.get("num_turns").and_then(|x| x.as_u64());
    let cost = v.get("total_cost_usd").and_then(|x| x.as_f64());
    if turns.is_some_and(|t| t > 0) || cost.is_some_and(|c| c > 0.0) {
        return UsageReply::Unsupported;
    }
    let local = v.get("local_command").and_then(|x| x.as_str()) == Some("usage");
    if turns.is_none() || cost.is_none() || !local {
        return UsageReply::Nothing;
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

/// The Claude Code version that once sent `/usage` to the model (plain text). Never asked again.
fn unsupported_marker(p: &RosterPaths) -> PathBuf {
    p.limits_dir().join("claude-usage-unsupported")
}

/// The Claude Code version that last answered `/usage` cleanly (plain text).
fn known_good_marker(p: &RosterPaths) -> PathBuf {
    p.limits_dir().join("claude-usage-known-good")
}

/// The ask of a version not yet known good, written before asking and removed when the reply
/// is clean or paid: `{"version": "2.1.322", "started_at": <unix seconds>}`.
fn probe_marker(p: &RosterPaths) -> PathBuf {
    p.limits_dir().join("claude-usage-probe")
}

/// How long a version whose probe never ended cleanly (killed, timed out, unreadable reply,
/// or still running) is not asked again. Even in the worst case a check costs one turn a day.
pub const PROBE_RETRY_SECS: i64 = 24 * 3600;

#[derive(Debug, Serialize, Deserialize)]
struct Probe {
    version: String,
    started_at: i64,
}

/// Whether to ask a Claude Code version for `/usage`.
#[derive(Debug, PartialEq)]
enum Gate {
    /// Known to answer locally.
    Ask,
    /// Not known yet: ask once, alone, writing a probe record first.
    Probe,
    /// Sent `/usage` to the model once, or its probe is running or never ended cleanly.
    Skip,
}

fn read_trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Unknown versions (`None`) are never known good: every ask of one is a probe.
fn gate(p: &RosterPaths, version: Option<&str>, now: i64) -> Gate {
    let v = version.unwrap_or("unknown");
    if read_trimmed(&unsupported_marker(p)).as_deref() == Some(v) {
        return Gate::Skip;
    }
    if version.is_some() && read_trimmed(&known_good_marker(p)).as_deref() == Some(v) {
        return Gate::Ask;
    }
    let probe: Option<Probe> = std::fs::read_to_string(probe_marker(p))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    match probe {
        Some(pr) if pr.version == v && now - pr.started_at < PROBE_RETRY_SECS => Gate::Skip,
        _ => Gate::Probe,
    }
}

/// True when the next ask of this version would be a probe: `refresh_all` then asks one agent
/// at a time until one answers cleanly.
pub fn needs_probe(p: &RosterPaths, version: Option<&str>) -> bool {
    gate(p, version, now()) == Gate::Probe
}

/// One gate decision and probe claim at a time in this process. Across processes the probe
/// record is created with `create_new`, so only one of them can claim it.
static PROBE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Decides whether to ask; for a probe, writes the probe record first. `Some(true)` is a
/// probe, `Some(false)` a plain ask, `None` don't ask.
fn begin_ask(p: &RosterPaths, version: Option<&str>, now: i64) -> Option<bool> {
    let _lock = PROBE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    match gate(p, version, now) {
        Gate::Skip => None,
        Gate::Ask => Some(false),
        Gate::Probe => {
            use std::io::Write;
            let path = probe_marker(p);
            std::fs::create_dir_all(p.limits_dir()).ok()?;
            // A record that no longer blocks (another version, or over a day old) makes way.
            let _ = std::fs::remove_file(&path);
            let record = Probe {
                version: version.unwrap_or("unknown").to_string(),
                started_at: now,
            };
            // Not written, not asked: the record is what bounds the cost.
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .ok()?;
            f.write_all(serde_json::to_string(&record).ok()?.as_bytes())
                .ok()?;
            f.sync_all().ok()?;
            Some(true)
        }
    }
}

/// A clean reply: the version is known good (when known) and the probe is over.
fn end_probe_clean(p: &RosterPaths, version: Option<&str>) {
    if let Some(v) = version {
        let _ = std::fs::write(known_good_marker(p), v);
    }
    let _ = std::fs::remove_file(probe_marker(p));
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

/// Asks `/usage`, guarded so a Claude Code version that sends it to the model costs at most
/// one turn: once if the reply says so (never asked again), once a day if it never answers.
fn ask_usage(
    agent: &Agent,
    r: &Roster,
    p: &RosterPaths,
    tool_path: &Path,
    version: Option<&str>,
    runner: &dyn CommandRunner,
) -> Asked {
    let Some(probing) = begin_ask(p, version, now()) else {
        return Asked::Skipped;
    };
    let env = crate::run::build_env(agent, r, p, None);
    // A reply that never came (killed at the time limit) keeps the probe record.
    let reply = match runner.run(tool_path, USAGE_ARGS, &env) {
        Ok(out) => parse_usage_reply(&String::from_utf8_lossy(&out.stdout), now()),
        Err(_) => UsageReply::Nothing,
    };
    match reply {
        UsageReply::Limits(l) => {
            if probing {
                end_probe_clean(p, version);
            }
            Asked::Limits(l)
        }
        UsageReply::NoPlan => {
            if probing {
                end_probe_clean(p, version);
            }
            Asked::NoPlan
        }
        UsageReply::Unsupported => {
            let _ = std::fs::create_dir_all(p.limits_dir());
            let _ = std::fs::write(unsupported_marker(p), version.unwrap_or("unknown"));
            let _ = std::fs::remove_file(probe_marker(p));
            Asked::Skipped
        }
        UsageReply::Nothing => Asked::Failed,
    }
}

/// Asks Claude Code for one agent's usage. `version` is the Claude Code version: one that
/// once sent `/usage` to the model is never asked again, and one not yet known to answer
/// locally is probed at most once a day until it answers cleanly.
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
        serde_json::json!({"type": "result", "num_turns": turns, "total_cost_usd": cost, "local_command": "usage", "result": result})
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
    fn a_reply_that_did_not_run_locally_is_not_clean() {
        // No turns and no cost, but nothing says `/usage` ran as a local command.
        let not_local = serde_json::json!({"type": "result", "num_turns": 0, "total_cost_usd": 0, "result": USAGE});
        assert_eq!(
            parse_usage_reply(&not_local.to_string(), DHAKA_NOV_10_1PM),
            UsageReply::Nothing
        );
        let other = serde_json::json!({"type": "result", "num_turns": 0, "total_cost_usd": 0, "local_command": "cost", "result": USAGE});
        assert_eq!(
            parse_usage_reply(&other.to_string(), DHAKA_NOV_10_1PM),
            UsageReply::Nothing
        );
        // Turns and cost must be there to show they are zero.
        let no_counts =
            serde_json::json!({"type": "result", "local_command": "usage", "result": USAGE});
        assert_eq!(
            parse_usage_reply(&no_counts.to_string(), DHAKA_NOV_10_1PM),
            UsageReply::Nothing
        );
    }

    #[test]
    fn usage_args_start_no_mcp_servers() {
        assert!(USAGE_ARGS.contains(&"--strict-mcp-config"));
        assert!(USAGE_ARGS.contains(&"--no-session-persistence"));
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

    const VERSION: &str = "2.1.322";

    fn temp_paths() -> (tempfile::TempDir, RosterPaths) {
        let d = tempfile::tempdir().unwrap();
        let p = RosterPaths::for_home(d.path(), Platform::current(), &|_| None);
        (d, p)
    }

    fn check_in(p: &RosterPaths, runner: &Scripted, version: Option<&str>) -> Check {
        let r = Roster {
            version: 1,
            agents: vec![agent()],
            statusline: vec![],
        };
        check_claude(&r.agents[0], &r, p, Path::new("claude"), version, runner)
    }

    fn marker(p: &RosterPaths, name: &str) -> Option<String> {
        std::fs::read_to_string(p.limits_dir().join(name)).ok()
    }

    fn probe_record(p: &RosterPaths) -> Option<serde_json::Value> {
        serde_json::from_str(&marker(p, "claude-usage-probe")?).ok()
    }

    fn signed_in(usage: Option<&str>) -> Scripted {
        Scripted::new(Some(SIGNED_IN), usage)
    }

    #[test]
    fn a_probe_that_never_answers_blocks_that_version_for_a_day() {
        let (_d, p) = temp_paths();
        // The first ask of a new version is killed after the time limit.
        let first = signed_in(None);
        assert_eq!(
            check_in(&p, &first, Some(VERSION)),
            Check::health(Health::Unreachable)
        );
        assert!(first.asked_usage());
        let rec = probe_record(&p).expect("a probe record");
        assert_eq!(rec["version"], VERSION);
        let started = rec["started_at"].as_i64().unwrap();
        assert!((now() - started).abs() < 60);
        // Within a day, that version is not asked again.
        let again = signed_in(Some(USAGE_SUB));
        assert_eq!(check_in(&p, &again, Some(VERSION)).health, Health::Ok);
        assert!(!again.asked_usage());
        // A day later it is probed once more.
        let a_day_ago = serde_json::json!({"version": VERSION, "started_at": started - 86_400});
        std::fs::write(
            p.limits_dir().join("claude-usage-probe"),
            a_day_ago.to_string(),
        )
        .unwrap();
        let later = signed_in(Some(USAGE_SUB));
        assert!(check_in(&p, &later, Some(VERSION)).limits.is_some());
        assert!(later.asked_usage());
    }

    #[test]
    fn an_unreadable_probe_reply_blocks_like_a_timeout() {
        let (_d, p) = temp_paths();
        let first = signed_in(Some("garbage"));
        assert_eq!(
            check_in(&p, &first, Some(VERSION)).health,
            Health::Unreachable
        );
        let again = signed_in(Some(USAGE_SUB));
        check_in(&p, &again, Some(VERSION));
        assert!(!again.asked_usage());
    }

    #[test]
    fn a_clean_reply_makes_the_version_known_good() {
        let (_d, p) = temp_paths();
        assert!(
            check_in(&p, &signed_in(Some(USAGE_SUB)), Some(VERSION))
                .limits
                .is_some()
        );
        assert_eq!(
            marker(&p, "claude-usage-known-good").as_deref(),
            Some(VERSION)
        );
        assert_eq!(probe_record(&p), None);
        // A known-good version that times out once is asked again at the next check.
        assert_eq!(
            check_in(&p, &signed_in(None), Some(VERSION)).health,
            Health::Unreachable
        );
        let next = signed_in(Some(USAGE_SUB));
        assert!(check_in(&p, &next, Some(VERSION)).limits.is_some());
        assert!(next.asked_usage());
    }

    #[test]
    fn no_plan_is_a_clean_reply_too() {
        let (_d, p) = temp_paths();
        let c = check_in(&p, &signed_in(Some(USAGE_OUT)), Some(VERSION));
        assert_eq!(c.health, Health::NoSubscription);
        assert_eq!(
            marker(&p, "claude-usage-known-good").as_deref(),
            Some(VERSION)
        );
    }

    #[test]
    fn a_new_version_is_probed_while_another_is_blocked() {
        let (_d, p) = temp_paths();
        check_in(&p, &signed_in(None), Some(VERSION));
        let newer = signed_in(Some(USAGE_SUB));
        assert!(check_in(&p, &newer, Some("2.1.400")).limits.is_some());
        assert!(newer.asked_usage());
        assert_eq!(
            marker(&p, "claude-usage-known-good").as_deref(),
            Some("2.1.400")
        );
    }

    #[test]
    fn an_unknown_version_is_never_known_good() {
        let (_d, p) = temp_paths();
        assert!(
            check_in(&p, &signed_in(Some(USAGE_SUB)), None)
                .limits
                .is_some()
        );
        assert_eq!(marker(&p, "claude-usage-known-good"), None);
        assert_eq!(probe_record(&p), None);
    }

    #[test]
    fn a_paid_reply_marks_the_version_and_it_is_never_asked_again() {
        let (_d, p) = temp_paths();
        let paid = serde_json::json!({"type": "result", "num_turns": 1, "total_cost_usd": 0.02, "result": "I can't run that."});
        let first = signed_in(Some(&paid.to_string()));
        assert_eq!(check_in(&p, &first, Some(VERSION)).health, Health::Ok);
        assert!(first.asked_usage());
        assert_eq!(
            marker(&p, "claude-usage-unsupported").as_deref(),
            Some(VERSION)
        );
        // Not at the next check, and not a day later either.
        let _ = std::fs::remove_file(p.limits_dir().join("claude-usage-probe"));
        let again = signed_in(Some(USAGE_SUB));
        assert_eq!(check_in(&p, &again, Some(VERSION)).health, Health::Ok);
        let r = Roster {
            version: 1,
            agents: vec![agent()],
            statusline: vec![],
        };
        let fetched = fetch_claude_usage(
            &r.agents[0],
            &r,
            &p,
            Path::new("claude"),
            Some(VERSION),
            &again,
        );
        assert_eq!(fetched, None);
        assert!(!again.asked_usage());
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
