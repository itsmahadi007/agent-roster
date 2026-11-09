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
