use roster_core::journal::Journal;
use roster_core::limits::read_claude_limits;
use roster_core::model::{LimitsSource, Roster, Tool};
use roster_core::names::CommandLookup;
use roster_core::ops::{
    AddRequest, Ctx, apply, plan_add, plan_adopt, plan_disable_claude_limits,
    plan_enable_claude_limits,
};
use roster_core::paths::{Platform, RosterPaths};
use roster_core::statusline::handle_statusline;
use std::fs;
use std::path::PathBuf;

struct NoCmds;
impl CommandLookup for NoCmds {
    fn find(&self, _: &str) -> Option<PathBuf> {
        None
    }
}

fn world() -> (tempfile::TempDir, RosterPaths, Journal) {
    let d = tempfile::tempdir().unwrap();
    let p = RosterPaths::for_home(d.path(), Platform::current(), &|_| None);
    fs::create_dir_all(d.path().join(".claude")).unwrap();
    let j = Journal::open(&p).unwrap();
    (d, p, j)
}

fn ctx(p: &RosterPaths) -> Ctx<'static> {
    Ctx {
        lookup: &NoCmds,
        path_var: p.bin_dir().display().to_string(),
        roster_bin: PathBuf::from("/usr/local/bin/roster"),
    }
}

fn adopt(p: &RosterPaths, j: &mut Journal) {
    let c = ctx(p);
    let plan = plan_adopt(&Roster::load(p).unwrap(), p, &c, Tool::Claude, "agent1").unwrap();
    apply(plan, p, j, &c).unwrap();
}

const STDIN: &str = r#"{"model":{"id":"x"},"rate_limits":{"five_hour":{"used_percentage":22,"resets_at":1794220000},"seven_day":{"used_percentage":61,"resets_at":1794528000}}}"#;

#[test]
fn writes_limits_file_from_rate_limits() {
    let (_d, p, mut j) = world();
    adopt(&p, &mut j);
    let r = Roster::load(&p).unwrap();
    let id = r.agents[0].id.clone();
    let env = |k: &str| (k == "ROSTER_AGENT").then(|| id.clone());
    let out = handle_statusline(STDIN, &env, &r, &p, &|_, _| String::new());
    assert_eq!(out, "agent1");
    let l = read_claude_limits(&p, &id).unwrap();
    assert_eq!(l.windows.len(), 2);
    assert_eq!(l.windows[0].label, "5 hr");
    assert_eq!(l.windows[1].label, "week");
    assert_eq!(l.windows[1].used_percent, 61.0);
    assert_eq!(l.windows[1].resets_at, 1794528000);
}

#[test]
fn chains_existing_statusline_output_unchanged() {
    let (d, p, mut j) = world();
    fs::write(
        d.path().join(".claude/settings.json"),
        r#"{"statusLine":{"type":"command","command":"printf hello"},"model":"opus"}"#,
    )
    .unwrap();
    adopt(&p, &mut j);
    let c = ctx(&p);
    let plan = plan_enable_claude_limits(&Roster::load(&p).unwrap(), &p, &c, "agent1").unwrap();
    assert!(
        plan.steps[0]
            .text
            .ends_with("so Agent Roster can read agent1's weekly use."),
        "{}",
        plan.steps[0].text
    );
    apply(plan, &p, &mut j, &c).unwrap();
    let r = Roster::load(&p).unwrap();
    assert_eq!(r.statusline[0].chain.as_deref(), Some("printf hello"));
    let settings: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(d.path().join(".claude/settings.json")).unwrap())
            .unwrap();
    assert_eq!(
        settings["statusLine"]["command"],
        "/usr/local/bin/roster statusline"
    );
    assert_eq!(settings["model"], "opus");
    let seen = std::cell::RefCell::new((String::new(), String::new()));
    let shell = |cmd: &str, stdin: &str| {
        *seen.borrow_mut() = (cmd.to_string(), stdin.to_string());
        "hello".to_string()
    };
    let out = handle_statusline(STDIN, &|_| None, &r, &p, &shell);
    assert_eq!(out, "hello");
    assert_eq!(seen.borrow().0, "printf hello");
    assert_eq!(seen.borrow().1, STDIN);
}

#[cfg(unix)]
#[test]
fn shared_settings_written_once() {
    let (d, p, mut j) = world();
    let main = d.path().join(".claude/settings.json");
    fs::write(&main, "{}").unwrap();
    adopt(&p, &mut j);
    let c = ctx(&p);
    let plan = plan_add(
        &Roster::load(&p).unwrap(),
        &p,
        &c,
        AddRequest {
            name: "agent2".into(),
            tool: Tool::Claude,
            color: None,
            role: None,
            share: false,
        },
    )
    .unwrap();
    apply(plan, &p, &mut j, &c).unwrap();
    let r = Roster::load(&p).unwrap();
    let dir2 = r.config_dir_of(r.by_name("agent2").unwrap(), &p);
    std::os::unix::fs::symlink(&main, dir2.join("settings.json")).unwrap();
    let plan = plan_enable_claude_limits(&r, &p, &c, "agent2").unwrap();
    apply(plan, &p, &mut j, &c).unwrap();
    let r = Roster::load(&p).unwrap();
    assert_eq!(r.statusline.len(), 1);
    assert!(
        r.agents
            .iter()
            .all(|a| a.limits == LimitsSource::ClaudeStatusline)
    );
    assert!(
        fs::symlink_metadata(dir2.join("settings.json"))
            .unwrap()
            .file_type()
            .is_symlink(),
        "link left alone"
    );
    assert!(
        plan_enable_claude_limits(&r, &p, &c, "agent1")
            .unwrap()
            .steps
            .iter()
            .any(|s| s.text.contains("already"))
    );
}

#[test]
fn disable_restores_original_statusline() {
    let (d, p, mut j) = world();
    let original =
        r#"{"statusLine":{"type":"command","command":"bash ~/x.sh","padding":0},"model":"opus"}"#;
    fs::write(d.path().join(".claude/settings.json"), original).unwrap();
    adopt(&p, &mut j);
    let c = ctx(&p);
    let plan = plan_enable_claude_limits(&Roster::load(&p).unwrap(), &p, &c, "agent1").unwrap();
    apply(plan, &p, &mut j, &c).unwrap();
    let plan = plan_disable_claude_limits(&Roster::load(&p).unwrap(), &p, &c, "agent1").unwrap();
    apply(plan, &p, &mut j, &c).unwrap();
    let now: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(d.path().join(".claude/settings.json")).unwrap())
            .unwrap();
    let was: serde_json::Value = serde_json::from_str(original).unwrap();
    assert_eq!(now, was);
    let r = Roster::load(&p).unwrap();
    assert!(r.statusline.is_empty());
    assert_eq!(r.agents[0].limits, LimitsSource::Off);
}

#[test]
fn resolves_agent_by_env_then_dir_then_default() {
    let (_d, p, mut j) = world();
    adopt(&p, &mut j);
    let c = ctx(&p);
    let plan = plan_add(
        &Roster::load(&p).unwrap(),
        &p,
        &c,
        AddRequest {
            name: "worker".into(),
            tool: Tool::Claude,
            color: None,
            role: None,
            share: false,
        },
    )
    .unwrap();
    apply(plan, &p, &mut j, &c).unwrap();
    let r = Roster::load(&p).unwrap();
    let worker = r.by_name("worker").unwrap().clone();
    let dir = r.config_dir_of(&worker, &p).display().to_string();
    let none = |_: &str, _: &str| String::new();
    assert_eq!(
        handle_statusline(
            "{}",
            &|k| (k == "ROSTER_AGENT").then(|| worker.id.clone()),
            &r,
            &p,
            &none
        ),
        "worker"
    );
    assert_eq!(
        handle_statusline(
            "{}",
            &|k| (k == "CLAUDE_CONFIG_DIR").then(|| dir.clone()),
            &r,
            &p,
            &none
        ),
        "worker"
    );
    assert_eq!(handle_statusline("{}", &|_| None, &r, &p, &none), "agent1");
}

#[test]
fn enable_without_settings_file_then_disable_finds_it() {
    let (d, p, mut j) = world();
    adopt(&p, &mut j);
    let c = ctx(&p);
    let plan = plan_enable_claude_limits(&Roster::load(&p).unwrap(), &p, &c, "agent1").unwrap();
    apply(plan, &p, &mut j, &c).unwrap();
    assert!(d.path().join(".claude/settings.json").exists());
    let plan = plan_disable_claude_limits(&Roster::load(&p).unwrap(), &p, &c, "agent1").unwrap();
    apply(plan, &p, &mut j, &c).unwrap();
    assert!(Roster::load(&p).unwrap().statusline.is_empty());
}

#[test]
fn statusline_command_quotes_a_roster_path_with_spaces() {
    let (d, p, mut j) = world();
    adopt(&p, &mut j);
    let bin = PathBuf::from("/Applications/Agent Roster.app/Contents/MacOS/roster");
    let c = Ctx {
        lookup: &NoCmds,
        path_var: p.bin_dir().display().to_string(),
        roster_bin: bin.clone(),
    };
    let plan = plan_enable_claude_limits(&Roster::load(&p).unwrap(), &p, &c, "agent1").unwrap();
    apply(plan, &p, &mut j, &c).unwrap();
    let s: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(d.path().join(".claude/settings.json")).unwrap())
            .unwrap();
    assert_eq!(
        s["statusLine"]["command"],
        format!("\"{}\" statusline", bin.display())
    );
}

#[test]
fn another_tools_statusline_command_is_kept_as_the_chain() {
    let (d, p, mut j) = world();
    fs::write(
        d.path().join(".claude/settings.json"),
        r#"{"statusLine":{"type":"command","command":"npx ccusage statusline"}}"#,
    )
    .unwrap();
    adopt(&p, &mut j);
    let c = ctx(&p);
    let plan = plan_enable_claude_limits(&Roster::load(&p).unwrap(), &p, &c, "agent1").unwrap();
    apply(plan, &p, &mut j, &c).unwrap();
    assert_eq!(
        Roster::load(&p).unwrap().statusline[0].chain.as_deref(),
        Some("npx ccusage statusline")
    );
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// A session's transcript, last changed `ago` seconds ago: when it last talked to Claude.
fn transcript(dir: &std::path::Path, name: &str, ago: i64) -> PathBuf {
    let f = dir.join(format!("{name}.jsonl"));
    fs::write(&f, "{}\n").unwrap();
    let t = std::time::SystemTime::now() - std::time::Duration::from_secs(ago as u64);
    fs::File::options()
        .write(true)
        .open(&f)
        .unwrap()
        .set_modified(t)
        .unwrap();
    f
}

fn session(transcript: Option<&std::path::Path>, week: f64) -> String {
    let mut v = serde_json::json!({
        "rate_limits": {"seven_day": {"used_percentage": week, "resets_at": 1794528000}}
    });
    if let Some(t) = transcript {
        v["transcript_path"] = t.display().to_string().into();
    }
    v.to_string()
}

fn week_of(p: &RosterPaths, id: &str) -> f64 {
    read_claude_limits(p, id)
        .unwrap()
        .windows
        .last()
        .unwrap()
        .used_percent
}

fn adopted() -> (tempfile::TempDir, RosterPaths, Roster, String) {
    let (d, p, mut j) = world();
    adopt(&p, &mut j);
    let r = Roster::load(&p).unwrap();
    let id = r.agents[0].id.clone();
    (d, p, r, id)
}

#[test]
fn an_idle_session_never_hides_a_fresh_check() {
    // 2026-11-17 on the owner's Mac: Claude's own /usage said 9% after a bonus reset, while a
    // session idle since before it kept saying 89% every second, and the menu showed 89%.
    let (d, p, r, id) = adopted();
    let check = roster_core::usage::Check {
        health: roster_core::usage::Health::Ok,
        limits: Some(roster_core::limits::AgentLimits {
            as_of: now_secs() - 30,
            plan: None,
            windows: vec![roster_core::limits::LimitWindow {
                label: "week".into(),
                used_percent: 9.0,
                resets_at: 1794528000,
            }],
        }),
    };
    roster_core::usage::record_check(&p, &id, &check, now_secs() - 30).unwrap();
    let idle = transcript(d.path(), "idle", 3 * 3600);
    let env = |k: &str| (k == "ROSTER_AGENT").then(|| id.clone());
    for _ in 0..3 {
        handle_statusline(&session(Some(&idle), 89.0), &env, &r, &p, &|_, _| {
            String::new()
        });
    }
    assert_eq!(week_of(&p, &id), 9.0);
}

#[test]
fn a_session_that_talked_since_the_last_save_updates_the_numbers_even_down() {
    let (d, p, r, id) = adopted();
    let env = |k: &str| (k == "ROSTER_AGENT").then(|| id.clone());
    let old = transcript(d.path(), "old", 3 * 3600);
    handle_statusline(&session(Some(&old), 89.0), &env, &r, &p, &|_, _| {
        String::new()
    });
    assert_eq!(week_of(&p, &id), 89.0);
    // A bonus from the provider: the active session's next reply carries the lower number.
    let active = transcript(d.path(), "active", 5);
    handle_statusline(&session(Some(&active), 8.0), &env, &r, &p, &|_, _| {
        String::new()
    });
    assert_eq!(week_of(&p, &id), 8.0);
    // Its numbers are dated by its last talk with Claude, not by when the line was drawn.
    let as_of = read_claude_limits(&p, &id).unwrap().as_of;
    assert!(
        (now_secs() - 7..=now_secs() - 3).contains(&as_of),
        "{as_of}"
    );
}

#[test]
fn two_open_sessions_never_flip_the_numbers() {
    let (d, p, r, id) = adopted();
    let env = |k: &str| (k == "ROSTER_AGENT").then(|| id.clone());
    let idle = transcript(d.path(), "idle", 3 * 3600);
    let active = transcript(d.path(), "active", 5);
    for _ in 0..4 {
        handle_statusline(&session(Some(&active), 8.0), &env, &r, &p, &|_, _| {
            String::new()
        });
        handle_statusline(&session(Some(&idle), 89.0), &env, &r, &p, &|_, _| {
            String::new()
        });
        assert_eq!(week_of(&p, &id), 8.0);
    }
}

#[test]
fn a_session_without_a_transcript_waits_for_a_check_to_go_stale() {
    let (_d, p, r, id) = adopted();
    let env = |k: &str| (k == "ROSTER_AGENT").then(|| id.clone());
    let saved = |ago: i64| roster_core::limits::AgentLimits {
        as_of: now_secs() - ago,
        plan: None,
        windows: vec![roster_core::limits::LimitWindow {
            label: "week".into(),
            used_percent: 9.0,
            resets_at: 1794528000,
        }],
    };
    roster_core::limits::write_claude_limits(&p, &id, &saved(60)).unwrap();
    handle_statusline(&session(None, 89.0), &env, &r, &p, &|_, _| String::new());
    assert_eq!(week_of(&p, &id), 9.0);
    roster_core::limits::write_claude_limits(&p, &id, &saved(20 * 60)).unwrap();
    handle_statusline(&session(None, 89.0), &env, &r, &p, &|_, _| String::new());
    assert_eq!(week_of(&p, &id), 89.0);
}
