use super::*;
use roster_core::model::Tool;
use roster_core::names::CommandLookup;
use roster_core::paths::{Platform, RosterPaths};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct NoCmds;
impl CommandLookup for NoCmds {
    fn find(&self, _: &str) -> Option<PathBuf> {
        None
    }
}

struct World {
    _d: tempfile::TempDir,
    b: Backend,
    opened: Arc<Mutex<Vec<Vec<String>>>>,
}

fn world_with(roster_bin: Option<PathBuf>) -> World {
    let d = tempfile::tempdir().unwrap();
    let p = RosterPaths::for_home(d.path(), Platform::current(), &|_| None);
    std::fs::create_dir_all(d.path().join(".claude")).unwrap();
    std::fs::create_dir_all(d.path().join(".codex")).unwrap();
    let opened = Arc::new(Mutex::new(vec![]));
    let rec = opened.clone();
    let scratch = d.path().join("scratch");
    let b = Backend::new(BackendConfig {
        path_var: p.bin_dir().display().to_string(),
        p,
        roster_bin,
        scratch,
        app_dirs: vec![d.path().join("Applications")],
        lookup: Box::new(NoCmds),
        env: Box::new(|k: &str| (k == "TERMINAL").then(|| "xterm".to_string())),
        opener: Box::new(move |argv: &[String]| {
            rec.lock().unwrap().push(argv.to_vec());
            Ok(())
        }),
    });
    World { _d: d, b, opened }
}

fn world() -> World {
    world_with(Some(PathBuf::from("/usr/local/bin/roster")))
}

fn add_req(name: &str) -> OpRequest {
    OpRequest::Add {
        name: name.into(),
        tool: Tool::Codex,
        color: None,
        role: None,
        share: false,
    }
}

impl World {
    fn add(&self, name: &str) -> u64 {
        let plan = self.b.preview(add_req(name)).unwrap();
        self.b.apply(plan.token).unwrap().unwrap()
    }
    fn names(&self) -> Vec<String> {
        Roster::load(&self.b.p)
            .unwrap()
            .agents
            .into_iter()
            .map(|a| a.name)
            .collect()
    }
}

#[test]
fn op_request_json_shape() {
    let r: OpRequest = serde_json::from_str(
        r#"{"op":"add","name":"boss","tool":"codex","color":"orchid","role":null,"share":false}"#,
    )
    .unwrap();
    assert!(matches!(r, OpRequest::Add { ref name, tool: Tool::Codex, .. } if name == "boss"));
    let r: OpRequest =
        serde_json::from_str(r#"{"op":"adopt-all","picks":[["claude","claude1"]]}"#).unwrap();
    assert!(
        matches!(r, OpRequest::AdoptAll { ref picks } if picks[0] == (Tool::Claude, "claude1".to_string()))
    );
    let r: OpRequest = serde_json::from_str(r#"{"op":"enable-limits","name":"agent1"}"#).unwrap();
    assert!(matches!(r, OpRequest::EnableLimits { .. }));
    let r: OpRequest = serde_json::from_str(r#"{"op":"import"}"#).unwrap();
    assert!(matches!(r, OpRequest::Import));
}

#[test]
fn preview_add_then_apply_records_one_change() {
    let w = world();
    let plan = w.b.preview(add_req("boss")).unwrap();
    let core = roster_core::ops::plan_add(
        &Roster::load(&w.b.p).unwrap(),
        &w.b.p,
        &w.b.ctx(),
        roster_core::ops::AddRequest {
            name: "boss".into(),
            tool: Tool::Codex,
            color: None,
            role: None,
            share: false,
        },
    )
    .unwrap();
    let texts: Vec<&str> = plan.steps.iter().map(|s| s.text.as_str()).collect();
    let core_texts: Vec<&str> = core.steps.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(texts, core_texts);
    assert_eq!(plan.steps[0].kind, "add");
    assert_eq!(plan.summary, core.summary);
    let id = w.b.apply(plan.token).unwrap();
    assert!(id.is_some());
    assert_eq!(w.names(), ["boss"]);
    let changes = w.b.changes().unwrap();
    assert_eq!(changes.len(), 1);
    assert!(
        changes[0].summary.contains("boss"),
        "{}",
        changes[0].summary
    );
}

#[test]
fn apply_refuses_when_roster_changed_since_preview() {
    let w = world();
    let plan = w.b.preview(add_req("boss")).unwrap();
    w.add("worker1");
    let err = w.b.apply(plan.token).unwrap_err();
    assert_eq!(
        err,
        "Your roster changed since this preview. Look at the new preview before applying."
    );
    assert_eq!(w.names(), ["worker1"]);
}

#[test]
fn apply_twice_says_expired() {
    let w = world();
    let plan = w.b.preview(add_req("boss")).unwrap();
    w.b.apply(plan.token).unwrap();
    assert_eq!(
        w.b.apply(plan.token).unwrap_err(),
        "That preview has expired. Look at it again."
    );
}

#[test]
fn preview_refuses_without_roster_binary() {
    let w = world_with(None);
    assert_eq!(
        w.b.preview(add_req("boss")).unwrap_err(),
        "Agent Roster's `roster` command wasn't found. If you opened the app from the downloaded disk image, drag it to Applications and open it from there. Otherwise reinstall Agent Roster."
    );
}

#[test]
fn bad_name_preview_shows_core_message() {
    let w = world();
    let core = roster_core::names::validate_name("1", &Roster::default(), &NoCmds, None)
        .unwrap_err()
        .to_string();
    assert_eq!(w.b.preview(add_req("1")).unwrap_err(), core);
}

#[test]
fn undo_restores_and_lists_undone() {
    let w = world();
    let id = w.add("boss");
    w.b.undo(id).unwrap();
    assert!(w.names().is_empty());
    let c = w.b.changes().unwrap();
    assert!(c[0].undone);
    assert!(w.b.undo(id).is_err());
    w.add("boss");
    w.add("worker1");
    assert_eq!(w.b.undo_all().unwrap(), 2);
    assert!(w.names().is_empty());
}

fn last_open(w: &World) -> Vec<String> {
    w.opened
        .lock()
        .unwrap()
        .last()
        .cloned()
        .expect("nothing opened")
}

fn command_text(argv: &[String]) -> String {
    if cfg!(target_os = "macos") {
        assert_eq!(argv[0], "open");
        std::fs::read_to_string(Path::new(&argv[1])).unwrap()
    } else {
        argv.join(" ")
    }
}

#[test]
fn sign_in_opens_terminal_with_roster_login() {
    let w = world();
    w.add("boss");
    assert!(w.b.sign_in("boss", None).unwrap());
    let text = command_text(&last_open(&w));
    if cfg!(target_os = "macos") {
        assert!(
            text.contains("'/usr/local/bin/roster' 'login' 'boss'"),
            "{text}"
        );
    } else {
        assert!(text.ends_with("/usr/local/bin/roster login boss"), "{text}");
    }
    assert!(w.b.sign_in("nobody", None).is_err());
}

#[test]
fn open_terminal_as_runs_the_agent() {
    let w = world();
    w.add("boss");
    let id = Roster::load(&w.b.p).unwrap().agents[0].id.clone();
    assert!(w.b.open_terminal_as("boss", None).unwrap());
    let text = command_text(&last_open(&w));
    if cfg!(target_os = "macos") {
        assert!(text.contains(&format!("'run' '{id}'")), "{text}");
    } else {
        assert!(text.ends_with(&format!("run {id}")), "{text}");
    }
}

/// A lookup that writes roster.toml the first time it is asked, like the CLI saving a
/// change while the app is still planning.
struct WritesRosterMidPlan {
    file: PathBuf,
    done: std::sync::atomic::AtomicBool,
}
impl CommandLookup for WritesRosterMidPlan {
    fn find(&self, _: &str) -> Option<PathBuf> {
        if !self.done.swap(true, std::sync::atomic::Ordering::SeqCst) {
            std::fs::create_dir_all(self.file.parent().unwrap()).unwrap();
            std::fs::write(&self.file, "version = 1\n").unwrap();
        }
        None
    }
}

#[test]
fn preview_taken_while_roster_changes_is_stale() {
    let d = tempfile::tempdir().unwrap();
    let p = RosterPaths::for_home(d.path(), Platform::current(), &|_| None);
    let b = Backend::new(BackendConfig {
        path_var: p.bin_dir().display().to_string(),
        lookup: Box::new(WritesRosterMidPlan {
            file: p.roster_file(),
            done: Default::default(),
        }),
        p,
        roster_bin: Some(PathBuf::from("/usr/local/bin/roster")),
        scratch: d.path().join("scratch"),
        app_dirs: vec![],
        env: Box::new(|_| None),
        opener: Box::new(|_| Ok(())),
    });
    let plan = b.preview(add_req("boss")).unwrap();
    assert_eq!(b.apply(plan.token).unwrap_err(), STALE);
}

#[test]
fn rename_previews_then_applies_and_keeps_the_login() {
    let r: OpRequest =
        serde_json::from_str(r#"{"op":"rename","name":"boss","new_name":"lead"}"#).unwrap();
    assert!(
        matches!(r, OpRequest::Rename { ref name, ref new_name } if name == "boss" && new_name == "lead")
    );
    let w = world();
    w.add("boss");
    let dir_before = Roster::load(&w.b.p).unwrap().agents[0].config_dir.clone();
    let plan = w.b.preview(r).unwrap();
    assert_eq!(plan.summary, "Renamed boss to lead");
    assert_eq!(plan.steps[0].kind, "change");
    w.b.apply(plan.token).unwrap();
    assert_eq!(w.names(), ["lead"]);
    assert_eq!(
        Roster::load(&w.b.p).unwrap().agents[0].config_dir,
        dir_before
    );
}

fn agent_of(w: &World, name: &str) -> roster_core::model::Agent {
    Roster::load(&w.b.p)
        .unwrap()
        .agents
        .into_iter()
        .find(|a| a.name == name)
        .unwrap()
}

fn run(w: &World, json: &str) -> PlanView {
    let req: OpRequest = serde_json::from_str(json).unwrap();
    let plan = w.b.preview(req).unwrap();
    w.b.apply(plan.token).unwrap();
    plan
}

#[test]
fn color_role_share_and_default_change_from_the_window() {
    let w = world();
    w.add("boss");
    w.add("helper");
    run(&w, r#"{"op":"color","name":"boss","color":"orchid"}"#);
    assert_eq!(agent_of(&w, "boss").color, AgentColor::Orchid);
    run(
        &w,
        r#"{"op":"role","name":"boss","role":"reviews pull requests"}"#,
    );
    assert_eq!(
        agent_of(&w, "boss").role.as_deref(),
        Some("reviews pull requests")
    );
    run(&w, r#"{"op":"role","name":"boss","role":null}"#);
    assert_eq!(agent_of(&w, "boss").role, None);
    run(&w, r#"{"op":"share","name":"boss","on":true}"#);
    assert!(agent_of(&w, "boss").share);
    run(&w, r#"{"op":"default","name":"helper"}"#);
    assert!(agent_of(&w, "helper").default);
    assert!(!agent_of(&w, "boss").default);
}

#[test]
fn remove_from_the_window_keeps_the_login_folder() {
    let w = world();
    w.add("boss");
    let dir = roster_core::paths::expand_tilde(&agent_of(&w, "boss").config_dir, &w.b.p.home);
    assert!(dir.exists());
    // The window never offers deleting the login folder.
    let bad: Result<OpRequest, _> =
        serde_json::from_str(r#"{"op":"remove","name":"boss","delete_login_folder":true}"#);
    assert!(bad.is_err());
    let plan = run(&w, r#"{"op":"remove","name":"boss"}"#);
    assert!(plan.summary.contains("boss"));
    assert!(w.names().is_empty());
    assert!(dir.exists());
}

#[test]
fn good_at_previews_then_applies() {
    let w = world();
    w.add("boss");
    let plan = run(
        &w,
        r#"{"op":"good-at","name":"boss","items":["tests","docs"]}"#,
    );
    assert_eq!(plan.summary, "Set what boss is good at");
    assert_eq!(
        agent_of(&w, "boss").good_at,
        Some(vec!["tests".to_string(), "docs".to_string()])
    );
    run(&w, r#"{"op":"good-at","name":"boss","items":null}"#);
    assert_eq!(agent_of(&w, "boss").good_at, None);
}

#[test]
fn only_terminals_found_on_this_computer_can_be_used() {
    let w = world();
    w.add("boss");
    let before = w.opened.lock().unwrap().len();
    assert!(
        w.b.open_terminal_as("boss", Some("Not A Terminal"))
            .is_err()
    );
    assert_eq!(w.opened.lock().unwrap().len(), before);
}

#[cfg(target_os = "macos")]
#[test]
fn copy_mode_terminal_is_opened_and_reported() {
    let w = world();
    w.add("boss");
    std::fs::create_dir_all(w.b.p.home.join("Applications/Warp.app")).unwrap();
    std::fs::create_dir_all(w.b.p.home.join("Applications/iTerm.app")).unwrap();
    let found = w.b.terminals();
    assert!(found.iter().any(|t| t.id == "Warp" && !t.runs));
    assert!(!w.b.open_terminal_as("boss", Some("Warp")).unwrap());
    assert_eq!(last_open(&w), ["open", "-a", "Warp"]);
    assert!(w.b.open_terminal_as("boss", Some("iTerm")).unwrap());
    assert_eq!(last_open(&w)[..3], ["open", "-a", "iTerm"]);
}

#[test]
fn open_site_opens_only_the_fixed_url() {
    let w = world();
    w.b.open_site().unwrap();
    assert_eq!(
        last_open(&w).last().map(String::as_str),
        Some("https://mahadihassan.com")
    );
}

#[test]
fn old_previews_are_dropped_after_many_new_ones() {
    let w = world();
    w.add("boss");
    let req = || -> OpRequest {
        serde_json::from_str(r#"{"op":"color","name":"boss","color":"coral"}"#).unwrap()
    };
    let first = w.b.preview(req()).unwrap().token;
    for _ in 0..40 {
        w.b.preview(req()).unwrap();
    }
    assert_eq!(w.b.apply(first).unwrap_err(), EXPIRED);
    let last = w.b.preview(req()).unwrap().token;
    assert!(w.b.apply(last).is_ok());
}

#[test]
fn usage_checks_run_one_at_a_time_and_at_most_every_30_seconds() {
    let g = UsageGate::default();
    let t0 = Instant::now();
    assert!(g.begin(t0, false));
    // Running: nobody else starts, urgent or not.
    assert!(!g.begin(t0, false));
    assert!(!g.begin(t0, true));
    g.end();
    assert!(!g.begin(t0 + Duration::from_secs(10), false));
    // An agent never checked doesn't wait.
    assert!(g.begin(t0 + Duration::from_secs(10), true));
    g.end();
    assert!(!g.begin(t0 + Duration::from_secs(39), false));
    assert!(g.begin(t0 + USAGE_GAP + Duration::from_secs(10), false));
}

#[test]
fn a_checking_row_asks_for_a_check() {
    let mut v: OverviewView =
        serde_json::from_str(include_str!("../../src/lib/fixtures/overview.json")).unwrap();
    assert!(!needs_check(&v));
    v.agents[1].health = "checking".into();
    assert!(needs_check(&v));
}

#[test]
fn refresh_usage_checks_and_saves_health() {
    let w = world();
    w.add("boss");
    // No codex on this PATH: the check says so and saves it.
    assert!(w.b.refresh_usage().unwrap());
    let id = w.b.roster().unwrap().agents[0].id.clone();
    assert_eq!(
        roster_core::usage::read_health(&w.b.p, &id).unwrap().health,
        roster_core::usage::Health::NotInstalled
    );
    // A second click right after is skipped.
    assert!(!w.b.refresh_usage().unwrap());
}
