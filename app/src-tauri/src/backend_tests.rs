use super::*;
use crate::sign_in::{
    LoginUi, LoginWindowSpec, NEEDS_BROWSER, OnEvent, SignInUpdate, SignInWith, WindowHooks,
};
use roster_core::detect::SignIn;
use roster_core::login::{LoginEvent, LoginHandle};
use roster_core::model::Tool;
use roster_core::names::CommandLookup;
use roster_core::paths::{Platform, RosterPaths};
use roster_core::status::{AgentState, AgentStatus};
use roster_core::usage::{Health, SavedHealth};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;
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
    b: Arc<Backend>,
    opened: Arc<Mutex<Vec<Vec<String>>>>,
    ui: Arc<FakeUi>,
    login: Arc<FakeLogin>,
}

/// Login windows that are only recorded. Closing one reports it closed, as a real one does.
#[derive(Default)]
struct FakeUi {
    opened: Mutex<Vec<LoginWindowSpec>>,
    hooks: Mutex<HashMap<String, WindowHooks>>,
    closed: Mutex<Vec<String>>,
    focused: Mutex<Vec<String>>,
    updates: Mutex<Vec<SignInUpdate>>,
    refuse: Mutex<Option<String>>,
}

impl LoginUi for FakeUi {
    fn open(&self, spec: &LoginWindowSpec, hooks: WindowHooks) -> Result<(), String> {
        if let Some(e) = self.refuse.lock().unwrap().clone() {
            return Err(e);
        }
        self.opened.lock().unwrap().push(spec.clone());
        self.hooks
            .lock()
            .unwrap()
            .insert(spec.agent_id.clone(), hooks);
        Ok(())
    }
    fn focus(&self, agent_id: &str) -> bool {
        self.focused.lock().unwrap().push(agent_id.into());
        self.hooks.lock().unwrap().contains_key(agent_id)
    }
    fn close(&self, agent_id: &str) {
        self.closed.lock().unwrap().push(agent_id.into());
        let h = self.hooks.lock().unwrap().remove(agent_id);
        if let Some(h) = h {
            (h.on_closed)();
        }
    }
    fn notify(&self, u: &SignInUpdate) {
        self.updates.lock().unwrap().push(u.clone());
    }
}

impl FakeUi {
    /// The person closes the window with its X.
    fn person_closes(&self, agent_id: &str) {
        let h = self.hooks.lock().unwrap().remove(agent_id).expect("open");
        (h.on_closed)();
    }
    /// The page tried to go somewhere the window may not show.
    fn page_blocked(&self, agent_id: &str) {
        // Taken out while it runs: the backend closes the window from inside the hook.
        let h = self.hooks.lock().unwrap().remove(agent_id).expect("open");
        (h.on_blocked)();
        if self.closed.lock().unwrap().iter().any(|c| c == agent_id) {
            (h.on_closed)();
        } else {
            self.hooks.lock().unwrap().insert(agent_id.into(), h);
        }
    }
    fn last(&self) -> SignInUpdate {
        self.updates
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("an update")
    }
    fn states(&self) -> Vec<&'static str> {
        self.updates
            .lock()
            .unwrap()
            .iter()
            .map(|u| u.state)
            .collect()
    }
}

/// A login that only does what the test tells it. Cancel reports `Failed("canceled")` right
/// away, from inside `cancel`, as the core may.
#[derive(Default)]
struct FakeLogin {
    events: Mutex<Vec<Arc<OnEvent>>>,
    cancels: Arc<AtomicUsize>,
    refuse: Mutex<Option<String>>,
}

impl FakeLogin {
    fn starter(self: &Arc<Self>) -> crate::sign_in::LoginStarter {
        let me = Arc::clone(self);
        Box::new(move |_req, on_event| {
            if let Some(e) = me.refuse.lock().unwrap().clone() {
                return Err(e);
            }
            let on_event = Arc::new(on_event);
            me.events.lock().unwrap().push(Arc::clone(&on_event));
            let cancels = Arc::clone(&me.cancels);
            Ok(LoginHandle::new(Box::new(move || {
                cancels.fetch_add(1, Ordering::SeqCst);
                on_event(LoginEvent::Failed("canceled".into()));
            })))
        })
    }
    fn send(&self, e: LoginEvent) {
        let f = self
            .events
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("started");
        f(e);
    }
    fn started(&self) -> usize {
        self.events.lock().unwrap().len()
    }
    fn cancels(&self) -> usize {
        self.cancels.load(Ordering::SeqCst)
    }
}

fn world_with(roster_bin: Option<PathBuf>) -> World {
    let d = tempfile::tempdir().unwrap();
    let p = RosterPaths::for_home(d.path(), Platform::current(), &|_| None);
    std::fs::create_dir_all(d.path().join(".claude")).unwrap();
    std::fs::create_dir_all(d.path().join(".codex")).unwrap();
    let opened = Arc::new(Mutex::new(vec![]));
    let rec = opened.clone();
    let scratch = d.path().join("scratch");
    let login = Arc::new(FakeLogin::default());
    let b = Backend::new(BackendConfig {
        login: login.starter(),
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
    let b = Arc::new(b);
    let ui = Arc::new(FakeUi::default());
    b.set_login_ui(ui.clone());
    World {
        _d: d,
        b,
        opened,
        ui,
        login,
    }
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
        login: Arc::new(FakeLogin::default()).starter(),
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

/// One Codex agent as the overview finds it: signed in or not, and its last check.
fn codex_overview(w: &World, signed_in: bool, check: Option<(Health, i64)>) -> Overview {
    let agent = agent_of(w, "boss");
    Overview {
        agents: vec![AgentStatus {
            agent,
            state: if signed_in {
                AgentState::Ready
            } else {
                AgentState::NeedsSignIn
            },
            sign_in: SignIn {
                signed_in,
                ..SignIn::default()
            },
            limits: None,
            health: check.map(|(h, at)| SavedHealth {
                health: h,
                checked_at: at,
                known: Some(h),
            }),
            sign_in_known: true,
            signed_in_at: None,
        }],
        calls: vec![],
        tools: vec![],
    }
}

#[test]
fn an_expired_codex_login_offers_sign_in_and_asks_for_no_check() {
    let w = world();
    w.add("boss");
    // `codex login status` says logged in; app-server said signed out (expired-login.jsonl).
    for t in [1000, 1010, 1020, 1030] {
        let v =
            w.b.view_of(codex_overview(&w, true, Some((Health::SignedOut, 900))), t);
        assert_eq!(v.agents[0].health, "sign-in");
        assert!(!w.b.take_wanted(), "asked for a check at {t}");
    }
}

#[test]
fn a_new_agent_asks_for_one_check() {
    let w = world();
    w.add("boss");
    let v = w.b.view_of(codex_overview(&w, true, None), 1000);
    assert_eq!(v.agents[0].health, "checking");
    assert!(w.b.take_wanted());
    // That check finished.
    let v =
        w.b.view_of(codex_overview(&w, true, Some((Health::Ok, 1005))), 1010);
    assert_eq!(v.agents[0].health, "ok");
    assert!(!w.b.take_wanted());
}

#[test]
fn a_sign_in_seen_after_the_check_asks_for_the_next_one() {
    let w = world();
    w.add("boss");
    let out = Some((Health::SignedOut, 900));
    w.b.view_of(codex_overview(&w, false, out), 1000);
    assert!(!w.b.take_wanted());
    // Signed in from a terminal: the status command now says so.
    let v = w.b.view_of(codex_overview(&w, true, out), 1010);
    assert_eq!(v.agents[0].health, "checking");
    assert!(w.b.take_wanted());
    // The check after it says ok: nothing more is asked.
    let v =
        w.b.view_of(codex_overview(&w, true, Some((Health::Ok, 1020))), 1030);
    assert_eq!(v.agents[0].health, "ok");
    assert!(!w.b.take_wanted());
}

#[test]
fn sign_in_from_the_app_asks_for_checks_until_one_says_it_worked() {
    let w = world();
    w.add("boss");
    let t = roster_core::view::now();
    let expired = |at| codex_overview(&w, true, Some((Health::SignedOut, at)));
    w.b.view_of(expired(t - 100), t - 50);
    assert!(!w.b.take_wanted());
    // An expired login: the status command says logged in before and after `roster login`.
    assert!(w.b.sign_in("boss", None).unwrap());
    w.b.view_of(expired(t - 100), t + 10);
    assert!(w.b.take_wanted());
    w.b.view_of(expired(t + 40), t + 50);
    assert!(w.b.take_wanted());
    w.b.view_of(codex_overview(&w, true, Some((Health::Ok, t + 80))), t + 90);
    assert!(!w.b.take_wanted());
    w.b.view_of(expired(t + 100), t + 110);
    assert!(!w.b.take_wanted());
}

// --- Signing in from the app ---

fn boss_id(w: &World) -> String {
    agent_of(w, "boss").id
}

#[test]
fn sign_in_with_is_the_browser_until_changed_and_is_kept() {
    let w = world();
    assert_eq!(w.b.settings().sign_in_with, SignInWith::Browser);
    w.b.set_sign_in_with(SignInWith::Window).unwrap();
    assert_eq!(w.b.settings().sign_in_with, SignInWith::Window);
    // Kept in the app's own settings file, not the roster or its journal.
    let file = crate::sign_in::settings_file(&w.b.p);
    assert_eq!(
        crate::sign_in::load_settings(&file).sign_in_with,
        SignInWith::Window
    );
    assert!(w.b.changes().unwrap().is_empty());
    w.b.set_sign_in_with(SignInWith::Browser).unwrap();
    assert_eq!(w.b.settings().sign_in_with, SignInWith::Browser);
}

#[test]
fn the_sign_in_link_opens_the_login_window() {
    let w = world();
    w.add("boss");
    w.b.sign_in_in_app("boss").unwrap();
    assert_eq!(w.login.started(), 1);
    assert!(w.ui.opened.lock().unwrap().is_empty());
    w.login.send(LoginEvent::Link(
        "https://auth.openai.com/oauth/authorize?x=1".into(),
    ));
    let opened = w.ui.opened.lock().unwrap().clone();
    assert_eq!(
        opened,
        [LoginWindowSpec {
            agent_id: boss_id(&w),
            agent_name: "boss".into(),
            tool_label: "Codex".into(),
            url: "https://auth.openai.com/oauth/authorize?x=1".into(),
        }]
    );
    assert_eq!(w.ui.last().state, "waiting");
    assert_eq!(w.ui.last().name, "boss");
    // Nothing went through a terminal.
    assert!(w.opened.lock().unwrap().is_empty());
}

#[test]
fn a_second_click_focuses_the_open_window() {
    let w = world();
    w.add("boss");
    w.b.sign_in_in_app("boss").unwrap();
    w.login
        .send(LoginEvent::Link("https://auth.openai.com/a".into()));
    w.b.sign_in_in_app("boss").unwrap();
    assert_eq!(w.login.started(), 1);
    assert_eq!(*w.ui.focused.lock().unwrap(), [boss_id(&w)]);
    assert_eq!(w.ui.opened.lock().unwrap().len(), 1);
}

#[test]
fn done_closes_the_window_and_checks_usage_now() {
    let w = world();
    w.add("boss");
    // A check just ran, so a plain one would wait 30 s.
    assert!(w.b.refresh_usage().unwrap());
    assert!(!w.b.refresh_usage().unwrap());
    w.b.take_wanted();
    w.b.sign_in_in_app("boss").unwrap();
    w.login
        .send(LoginEvent::Link("https://auth.openai.com/a".into()));
    // The window stays open through every page; only the tool's own Done closes it.
    assert!(w.ui.closed.lock().unwrap().is_empty());
    w.login.send(LoginEvent::Done);
    assert_eq!(*w.ui.closed.lock().unwrap(), [boss_id(&w)]);
    assert_eq!(w.ui.states(), ["waiting", "done"]);
    assert_eq!(w.login.cancels(), 0, "a finished login is not canceled");
    assert!(w.b.take_wanted());
    assert!(
        w.b.refresh_usage().unwrap(),
        "the check after a sign-in skips the wait"
    );
    // The next sign-in starts a new login.
    w.b.sign_in_in_app("boss").unwrap();
    assert_eq!(w.login.started(), 2);
}

#[test]
fn failed_closes_the_window_and_says_why() {
    let w = world();
    w.add("boss");
    w.b.sign_in_in_app("boss").unwrap();
    w.login
        .send(LoginEvent::Link("https://auth.openai.com/a".into()));
    w.login.send(LoginEvent::Failed(
        "Another Codex sign-in is open. Finish or close it first.".into(),
    ));
    assert_eq!(*w.ui.closed.lock().unwrap(), [boss_id(&w)]);
    let u = w.ui.last();
    assert_eq!(u.state, "failed");
    assert_eq!(
        u.reason.as_deref(),
        Some("Another Codex sign-in is open. Finish or close it first.")
    );
    assert_eq!(w.login.cancels(), 0);
}

#[test]
fn closing_the_window_cancels_the_login() {
    let w = world();
    w.add("boss");
    w.b.sign_in_in_app("boss").unwrap();
    w.login
        .send(LoginEvent::Link("https://auth.openai.com/a".into()));
    w.ui.person_closes(&boss_id(&w));
    assert_eq!(w.login.cancels(), 1);
    // The core's own "canceled" that follows is not shown as a failure.
    assert_eq!(w.ui.states(), ["waiting", "canceled"]);
    w.b.sign_in_in_app("boss").unwrap();
    assert_eq!(w.login.started(), 2);
}

#[test]
fn a_page_the_window_may_not_show_sends_the_person_to_the_browser() {
    let w = world();
    w.add("boss");
    w.b.sign_in_in_app("boss").unwrap();
    w.login
        .send(LoginEvent::Link("https://auth.openai.com/a".into()));
    w.ui.page_blocked(&boss_id(&w));
    assert_eq!(w.login.cancels(), 1);
    assert_eq!(*w.ui.closed.lock().unwrap(), [boss_id(&w)]);
    let u = w.ui.last();
    assert_eq!(
        (u.state, u.reason.as_deref()),
        ("failed", Some(NEEDS_BROWSER))
    );
    assert_eq!(w.ui.states(), ["waiting", "failed"]);
}

#[test]
fn a_window_that_cannot_open_fails_the_login() {
    let w = world();
    w.add("boss");
    *w.ui.refuse.lock().unwrap() = Some(NEEDS_BROWSER.into());
    w.b.sign_in_in_app("boss").unwrap();
    w.login
        .send(LoginEvent::Link("https://accounts.example.com/a".into()));
    assert_eq!(w.login.cancels(), 1);
    let u = w.ui.last();
    assert_eq!(
        (u.state, u.reason.as_deref()),
        ("failed", Some(NEEDS_BROWSER))
    );
}

#[test]
fn a_login_that_cannot_start_says_why_and_can_be_tried_again() {
    let w = world();
    w.add("boss");
    *w.login.refuse.lock().unwrap() = Some("Codex isn't installed on this computer.".into());
    assert_eq!(
        w.b.sign_in_in_app("boss").unwrap_err(),
        "Codex isn't installed on this computer."
    );
    *w.login.refuse.lock().unwrap() = None;
    w.b.sign_in_in_app("boss").unwrap();
    assert_eq!(w.login.started(), 1);
    assert!(w.b.sign_in_in_app("nobody").is_err());
}
