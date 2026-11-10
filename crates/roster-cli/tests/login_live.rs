//! Live checks against the real tools, with throwaway account folders. Ignored by default:
//! `ROSTER_LIVE_DIR=<scratch folder> cargo test -p agent-roster --test login_live -- --ignored`.
//! Each starts a real sign-in, checks the link arrives, then cancels: nobody signs in.

use roster_core::login::{LoginEvent, allowed_in_window, redirect_port, start_login_with};
use roster_core::model::{Agent, AgentColor, LimitsSource, Roster, Tool};
use roster_core::paths::{Platform, RosterPaths};
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::Duration;

fn live_dir() -> PathBuf {
    PathBuf::from(std::env::var("ROSTER_LIVE_DIR").expect("set ROSTER_LIVE_DIR"))
}

fn agent(tool: Tool, dir: &Path) -> (Roster, RosterPaths) {
    std::fs::create_dir_all(dir).unwrap();
    let platform = if cfg!(windows) {
        Platform::Windows
    } else {
        Platform::Linux
    };
    let p = RosterPaths::for_home(&live_dir(), platform, &|_| None);
    let r = Roster {
        version: 1,
        agents: vec![Agent {
            id: "live".into(),
            name: "live".into(),
            tool,
            config_dir: dir.display().to_string(),
            color: AgentColor::Cobalt,
            role: None,
            default: false,
            share: false,
            limits: LimitsSource::Off,
            advanced_links: vec![],
            good_at: None,
        }],
        ..Roster::default()
    };
    (r, p)
}

fn link_then_cancel(tool: Tool, tool_path: &Path, roster_bin: Option<&Path>, dir: &Path) {
    let (r, p) = agent(tool, dir);
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let h = start_login_with(
        &r.agents[0],
        &r,
        &p,
        tool_path,
        roster_bin,
        Box::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        }),
    )
    .unwrap();
    match rx.recv_timeout(Duration::from_secs(40)).unwrap() {
        LoginEvent::Link(url) => {
            assert!(allowed_in_window(&url));
            // Host and callback port only: never the query.
            let host = url.split('/').nth(2).unwrap_or("").to_string();
            eprintln!(
                "link arrived: host {host}, callback port {:?}",
                redirect_port(&url)
            );
        }
        e => panic!("{e:?}"),
    }
    h.cancel();
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(10)).unwrap(),
        LoginEvent::Failed("canceled".into())
    );
}

#[test]
#[ignore]
fn live_claude_link_arrives_through_roster_in_a_folder_with_spaces() {
    let spaced = live_dir().join("with space");
    std::fs::create_dir_all(&spaced).unwrap();
    let src = PathBuf::from(env!("CARGO_BIN_EXE_roster"));
    let bin = spaced.join(src.file_name().unwrap());
    std::fs::copy(&src, &bin).unwrap();
    let claude = which::which("claude").unwrap();
    link_then_cancel(
        Tool::Claude,
        &claude,
        Some(&bin),
        &live_dir().join("claudecfg"),
    );
}

#[test]
#[ignore]
fn live_codex_link_arrives() {
    let codex = which::which("codex").unwrap();
    link_then_cancel(Tool::Codex, &codex, None, &live_dir().join("codexhome"));
}
