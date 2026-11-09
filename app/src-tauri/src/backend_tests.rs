use super::*;
use roster_core::model::Tool;
use roster_core::names::CommandLookup;
use roster_core::paths::{Platform, RosterPaths};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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
        "Agent Roster's `roster` command wasn't found. Install it, then open the app again."
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
    w.b.sign_in("boss").unwrap();
    let text = command_text(&last_open(&w));
    if cfg!(target_os = "macos") {
        assert!(
            text.contains("'/usr/local/bin/roster' 'login' 'boss'"),
            "{text}"
        );
    } else {
        assert!(text.ends_with("/usr/local/bin/roster login boss"), "{text}");
    }
    assert!(w.b.sign_in("nobody").is_err());
}

#[test]
fn open_terminal_as_runs_the_agent() {
    let w = world();
    w.add("boss");
    let id = Roster::load(&w.b.p).unwrap().agents[0].id.clone();
    w.b.open_terminal_as("boss").unwrap();
    let text = command_text(&last_open(&w));
    if cfg!(target_os = "macos") {
        assert!(text.contains(&format!("'run' '{id}'")), "{text}");
    } else {
        assert!(text.ends_with(&format!("run {id}")), "{text}");
    }
}
