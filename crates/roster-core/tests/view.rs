use roster_core::detect::{CommandRunner, SignIn};
use roster_core::journal::Op;
use roster_core::limits::{AgentLimits, LimitWindow};
use roster_core::model::{Agent, AgentColor, LimitsSource, Roster, Tool};
use roster_core::paths::{Platform, RosterPaths};
use roster_core::session::SessionRecord;
use roster_core::status::{AgentState, AgentStatus, Overview};
use roster_core::view::{
    CandidateView, FirstRunView, WindowView, changes_view, first_run_view, overview_view,
};
use std::path::{Path, PathBuf};
use std::process::Output;

const NOW: i64 = 1_800_000_000;
const HOME: &str = "/home/you";

fn agent(id: &str, name: &str, tool: Tool, color: AgentColor, limits: LimitsSource) -> Agent {
    Agent {
        id: id.into(),
        name: name.into(),
        tool,
        config_dir: format!("~/.agent-roster/accounts/{id}"),
        color,
        role: None,
        default: false,
        share: false,
        limits,
        advanced_links: vec![],
        good_at: None,
    }
}

fn win(label: &str, used: f64, resets_in: i64) -> LimitWindow {
    LimitWindow {
        label: label.into(),
        used_percent: used,
        resets_at: NOW + resets_in,
    }
}

fn status(a: Agent, state: AgentState, windows: Vec<LimitWindow>) -> AgentStatus {
    let signed_in = !matches!(state, AgentState::NeedsSignIn | AgentState::NotInstalled);
    AgentStatus {
        agent: a,
        state,
        sign_in: SignIn {
            signed_in,
            email: signed_in.then(|| "you@example.com".to_string()),
            plan: None,
        },
        limits: (!windows.is_empty()).then_some(AgentLimits {
            as_of: NOW - 240,
            plan: None,
            windows,
        }),
    }
}

fn call(from: &str, to: &str) -> SessionRecord {
    SessionRecord {
        pid: 1,
        agent_id: to.into(),
        parent_id: Some(from.into()),
        cwd: format!("{HOME}/projects/portfolio"),
        task: Some("Write tests for the login page and run them.".into()),
        started_at: NOW - 120,
    }
}

fn sample() -> Overview {
    let mut a1 = agent(
        "a1",
        "agent1",
        Tool::Claude,
        AgentColor::Cobalt,
        LimitsSource::ClaudeStatusline,
    );
    a1.default = true;
    a1.config_dir = "~/.claude".into();
    Overview {
        agents: vec![
            status(
                a1,
                AgentState::Working,
                vec![
                    win("5 hr", 22.0, 3 * 3600),
                    win("week", 61.0, 3 * 86_400 + 16 * 3600),
                ],
            ),
            status(
                agent(
                    "a2",
                    "agent2",
                    Tool::Claude,
                    AgentColor::Marigold,
                    LimitsSource::Off,
                ),
                AgentState::NeedsSignIn,
                vec![],
            ),
            status(
                agent(
                    "a3",
                    "agent3",
                    Tool::Codex,
                    AgentColor::Jade,
                    LimitsSource::CodexSessions,
                ),
                AgentState::Working,
                vec![win("week", 93.0, 21 * 3600 + 52 * 60)],
            ),
            status(
                agent(
                    "a4",
                    "boss",
                    Tool::Codex,
                    AgentColor::Orchid,
                    LimitsSource::CodexSessions,
                ),
                AgentState::Ready,
                vec![
                    win("5 hr", 8.0, 2 * 3600),
                    win("week", 24.0, 5 * 86_400 + 2 * 3600),
                ],
            ),
        ],
        calls: vec![call("a1", "a3")],
        tools: vec![],
    }
}

#[test]
fn calling_parent_and_working_child() {
    let v = overview_view(&sample(), Path::new(HOME), NOW);
    assert_eq!(v.agents[0].status_text, "calling agent3");
    assert_eq!(v.agents[2].status_text, "working");
    assert!(v.agents[0].live && v.agents[2].live);
    assert!(!v.agents[1].live && !v.agents[3].live);
    assert_eq!(v.calls[0].from_name, "agent1");
    assert_eq!(v.calls[0].to_name, "agent3");
    assert_eq!(v.calls[0].from_color, "#4d7cff");
    assert_eq!(v.calls[0].detail, "started 2m ago in ~/projects/portfolio");
    assert_eq!(
        v.agents[0].default_label.as_deref(),
        Some("default for claude")
    );
    assert_eq!(v.agents[0].email.as_deref(), Some("you@example.com"));
}

#[test]
fn hot_single_window_is_coral() {
    let v = overview_view(&sample(), Path::new(HOME), NOW);
    let a3 = &v.agents[2];
    assert_eq!(
        a3.windows,
        vec![WindowView {
            label: "week".into(),
            used_percent: 93,
            hot: true
        }]
    );
    assert_eq!(a3.reset_text.as_deref(), Some("resets in 21h 52m"));
    assert!(a3.reset_hot);
    assert_eq!(a3.tray_text, "93% week");
    assert!(a3.tray_hot);
    assert_eq!(v.warnings.len(), 1);
    assert_eq!(v.warnings[0].name, "agent3");
    assert_eq!(
        v.warnings[0].text,
        "has used 93% of its week. It resets in 21h 52m."
    );
}

#[test]
fn past_reset_says_resetting_now() {
    let mut o = sample();
    o.agents[2].limits.as_mut().unwrap().windows[0].resets_at = NOW - 60;
    let v = overview_view(&o, Path::new(HOME), NOW);
    assert_eq!(v.agents[2].reset_text.as_deref(), Some("resetting now"));
    assert_eq!(
        v.warnings[0].text,
        "has used 93% of its week. It is resetting now."
    );
}

#[test]
fn two_windows_reset_line_names_the_last() {
    let v = overview_view(&sample(), Path::new(HOME), NOW);
    let a1 = &v.agents[0];
    assert_eq!(a1.reset_text.as_deref(), Some("week resets in 3d 16h"));
    assert!(!a1.reset_hot);
    assert_eq!(a1.tray_text, "61% week");
    assert_eq!(a1.windows[0].label, "5 hr");
    assert_eq!(a1.windows[0].used_percent, 22);
    assert_eq!(v.limits_updated.as_deref(), Some("Last update 4m ago."));
}

#[test]
fn notes_when_no_limits() {
    let mut o = sample();
    let v = overview_view(&o, Path::new(HOME), NOW);
    assert_eq!(
        v.agents[1].limits_note.as_deref(),
        Some("usage shows after sign in")
    );
    assert_eq!(v.agents[1].tray_text, "sign in");
    assert_eq!(v.agents[1].state, "sign-in");
    assert_eq!(v.agents[2].claude_limits, None);
    assert_eq!(v.agents[0].claude_limits, Some(true));
    assert_eq!(v.agents[0].limits_note, None);

    o.agents[1].state = AgentState::Ready;
    o.agents[1].sign_in.signed_in = true;
    let v = overview_view(&o, Path::new(HOME), NOW);
    assert_eq!(v.agents[1].limits_note.as_deref(), Some("usage not shown"));
    assert_eq!(v.agents[1].claude_limits, Some(false));
    assert_eq!(v.agents[1].tray_text, "");

    o.agents[1].state = AgentState::NotInstalled;
    let v = overview_view(&o, Path::new(HOME), NOW);
    assert_eq!(v.agents[1].state, "not-installed");
    assert_eq!(v.agents[1].limits_note.as_deref(), Some("not installed"));
    assert_eq!(v.agents[1].tray_text, "not installed");
}

#[test]
fn headline_counts_in_words() {
    let mut o = sample();
    let v = overview_view(&o, Path::new(HOME), NOW);
    assert_eq!(
        v.headline,
        "Four accounts, one quota each. Type a name in any terminal to start it."
    );
    o.agents.truncate(1);
    o.calls.clear();
    let v = overview_view(&o, Path::new(HOME), NOW);
    assert_eq!(
        v.headline,
        "One account. Type its name in any terminal to start it."
    );
    let one = o.agents[0].clone();
    o.agents = (0..12).map(|_| one.clone()).collect();
    let v = overview_view(&o, Path::new(HOME), NOW);
    assert!(v.headline.starts_with("12 accounts"), "{}", v.headline);
}

#[test]
fn palette_lists_the_five_colors() {
    let v = overview_view(&sample(), Path::new(HOME), NOW);
    let labels: Vec<&str> = v.palette.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, ["cobalt", "marigold", "jade", "orchid", "coral"]);
    assert_eq!(v.palette[3].hex, "#c06bf0");
}

fn op(id: u64, summary: &str) -> Op {
    Op {
        id,
        at: NOW - (10 - id as i64) * 60,
        summary: summary.into(),
        changes: vec![],
        undone: false,
    }
}

#[test]
fn changes_newest_first_with_agent_color() {
    let r = Roster {
        version: 1,
        agents: vec![agent(
            "a4",
            "boss",
            Tool::Codex,
            AgentColor::Orchid,
            LimitsSource::CodexSessions,
        )],
        statusline: vec![],
    };
    let mut ops = vec![
        op(1, "Updated notes"),
        op(2, "Added boss (Codex)"),
        op(3, "Added bossy (Codex)"),
    ];
    ops[0].undone = true;
    let v = changes_view(&ops, &r);
    assert_eq!(v.iter().map(|c| c.id).collect::<Vec<_>>(), [3, 2, 1]);
    assert_eq!(v[0].color, None);
    assert_eq!(v[1].color.as_deref(), Some("#c06bf0"));
    assert_eq!(v[2].color, None);
    assert!(v[2].undone);
    assert_eq!(v[1].summary, "Added boss (Codex)");
}

struct FakeRunner;
impl CommandRunner for FakeRunner {
    fn run(
        &self,
        _prog: &Path,
        args: &[&str],
        _env: &[(String, Option<String>)],
    ) -> std::io::Result<Output> {
        let stdout = match args {
            ["--version"] => "2.1.0 (Claude Code)\n".to_string(),
            ["auth", "status", "--json"] => {
                r#"{"loggedIn":true,"email":"you@example.com"}"#.to_string()
            }
            _ => String::new(),
        };
        Ok(Output {
            status: ok_status(),
            stdout: stdout.into_bytes(),
            stderr: vec![],
        })
    }
}

#[cfg(unix)]
fn ok_status() -> std::process::ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    std::process::ExitStatus::from_raw(0)
}
#[cfg(windows)]
fn ok_status() -> std::process::ExitStatus {
    use std::os::windows::process::ExitStatusExt;
    std::process::ExitStatus::from_raw(0)
}

fn fake_tool_dir(home: &Path, name: &str) -> PathBuf {
    let dir = home.join("tools");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    });
    std::fs::write(&file, "#!/bin/sh\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    dir
}

#[test]
fn first_run_lists_installed_tools_with_versions() {
    let d = tempfile::tempdir().unwrap();
    let p = RosterPaths::for_home(d.path(), Platform::current(), &|_| None);
    let dir = fake_tool_dir(d.path(), "claude");
    let v = first_run_view(
        &Roster::default(),
        &p,
        &dir.display().to_string(),
        &FakeRunner,
    );
    assert_eq!(v.candidates.len(), 2);
    let c = &v.candidates[0];
    assert_eq!(c.tool, Tool::Claude);
    assert!(c.installed && c.signed_in);
    assert_eq!(c.version.as_deref(), Some("2.1.0"));
    assert_eq!(c.email.as_deref(), Some("you@example.com"));
    assert_eq!(c.suggested_name, "claude1");
    assert_eq!(v.candidates[1].suggested_name, "codex1");
    assert!(!v.candidates[1].installed);
    assert!(!v.prototype);
}

#[test]
fn first_run_skips_tools_already_on_roster() {
    let d = tempfile::tempdir().unwrap();
    let p = RosterPaths::for_home(d.path(), Platform::current(), &|_| None);
    let dir = fake_tool_dir(d.path(), "claude");
    let mut a = agent(
        "a1",
        "agent1",
        Tool::Claude,
        AgentColor::Cobalt,
        LimitsSource::Off,
    );
    a.config_dir = "~/.claude".into();
    let r = Roster {
        version: 1,
        agents: vec![a],
        statusline: vec![],
    };
    let v = first_run_view(&r, &p, &dir.display().to_string(), &FakeRunner);
    assert_eq!(v.candidates.len(), 1);
    assert_eq!(v.candidates[0].tool, Tool::Codex);
    assert!(!v.candidates[0].installed);
}

fn sample_first_run() -> FirstRunView {
    FirstRunView {
        candidates: vec![
            CandidateView {
                tool: Tool::Claude,
                tool_label: "Claude Code".into(),
                installed: true,
                version: Some("2.1.0".into()),
                email: Some("you@example.com".into()),
                signed_in: true,
                suggested_name: "claude1".into(),
            },
            CandidateView {
                tool: Tool::Codex,
                tool_label: "Codex".into(),
                installed: true,
                version: Some("0.167.0".into()),
                email: None,
                signed_in: true,
                suggested_name: "codex1".into(),
            },
        ],
        prototype: false,
    }
}

fn sample_changes() -> Vec<roster_core::view::ChangeView> {
    let r = Roster {
        version: 1,
        agents: vec![agent(
            "a4",
            "boss",
            Tool::Codex,
            AgentColor::Orchid,
            LimitsSource::CodexSessions,
        )],
        statusline: vec![],
    };
    let mut ops = vec![
        op(1, "Added agent1 (existing Claude Code login)"),
        op(2, "Added boss (Codex)"),
        op(3, "Turned on Claude limits for agent1"),
    ];
    ops[0].undone = true;
    changes_view(&ops, &r)
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/src/lib/fixtures")
}

#[test]
fn fixtures_match() {
    let files = [
        (
            "overview.json",
            serde_json::to_string_pretty(&overview_view(&sample(), Path::new(HOME), NOW)).unwrap(),
        ),
        (
            "first-run.json",
            serde_json::to_string_pretty(&sample_first_run()).unwrap(),
        ),
        (
            "changes.json",
            serde_json::to_string_pretty(&sample_changes()).unwrap(),
        ),
    ];
    let dir = fixture_dir();
    for (name, json) in files {
        let path = dir.join(name);
        if std::env::var("UPDATE_FIXTURES").is_ok() {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(&path, format!("{json}\n")).unwrap();
        } else {
            let on_disk = std::fs::read_to_string(&path)
                .unwrap_or_else(|_| panic!("{} missing; run with UPDATE_FIXTURES=1", name));
            assert_eq!(
                on_disk.replace("\r\n", "\n").trim_end(),
                json,
                "{name} is out of date; run with UPDATE_FIXTURES=1"
            );
        }
    }
}

#[test]
fn next_color_is_the_one_add_would_pick() {
    let mut o = sample();
    let v = overview_view(&o, Path::new(HOME), NOW);
    assert_eq!(v.next_color, "coral");
    o.agents.truncate(1);
    let v = overview_view(&o, Path::new(HOME), NOW);
    assert_eq!(v.next_color, "marigold");
}

#[test]
fn agent_view_carries_what_the_edit_sheet_needs() {
    let mut o = sample();
    o.agents[0].agent.role = Some("reviews pull requests".into());
    o.agents[0].agent.share = true;
    let v = overview_view(&o, Path::new(HOME), NOW);
    let a = &v.agents[0];
    assert_eq!(a.role.as_deref(), Some("reviews pull requests"));
    assert!(a.share);
    assert!(a.default);
    assert_eq!(a.color_label, "cobalt");
    let b = &v.agents[1];
    assert_eq!(b.role, None);
    assert!(!b.share && !b.default);
    assert_eq!(b.color_label, "marigold");
}

#[test]
fn a_signed_in_agent_without_numbers_says_how_to_get_them() {
    let mut o = sample();
    o.agents[3].limits = None;
    o.agents[3].state = AgentState::Ready;
    let v = overview_view(&o, Path::new(HOME), NOW);
    let name = &v.agents[3].name;
    assert_eq!(
        v.agents[3].limits_note.as_deref(),
        Some(format!("usage shows after you use {name}").as_str())
    );
}

#[test]
fn agent_view_shows_good_at_defaults_until_someone_sets_them() {
    let mut o = sample();
    let v = overview_view(&o, Path::new(HOME), NOW);
    assert_eq!(
        v.agents[0].good_at,
        ["coding", "reviewing code", "long tasks"]
    );
    assert!(!v.agents[0].good_at_custom);
    o.agents[0].agent.good_at = Some(vec!["planning".into()]);
    let v = overview_view(&o, Path::new(HOME), NOW);
    assert_eq!(v.agents[0].good_at, ["planning"]);
    assert!(v.agents[0].good_at_custom);
}
