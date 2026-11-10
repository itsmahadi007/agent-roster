//! The Claude sign-in bridge end to end: a fake `claude auth login` calls `$BROWSER <link>`,
//! `$BROWSER` is the real `roster` binary (copied to a folder with a space in its name), and
//! the link must come back to `start_login_with` over the one-time channel.

use roster_core::login::{LoginEvent, start_login_with};
use roster_core::model::{Agent, AgentColor, LimitsSource, Roster, Tool};
use roster_core::paths::{Platform, RosterPaths};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

const LINK: &str = "https://claude.com/cai/oauth/authorize?code=true&client_id=example&redirect_uri=http%3A%2F%2Flocalhost%3A50123%2Fcallback&state=example";

/// A fake `claude`: calls `$BROWSER` with `link`, then waits `wait_secs` and exits 0.
/// It also notes the CLAUDE_CONFIG_DIR it was given.
fn fake_claude(dir: &Path, name: &str, link: &str, wait_secs: u32) -> PathBuf {
    #[cfg(windows)]
    {
        let f = dir.join(format!("{name}.cmd"));
        let link = link.replace('%', "%%");
        std::fs::write(
            &f,
            format!(
                "@echo off\r\nif not \"%1\"==\"auth\" exit /b 9\r\n\
                 >\"%~dp0{name}.seen\" echo %CLAUDE_CONFIG_DIR%\r\n\
                 \"%BROWSER%\" \"{link}\"\r\n\
                 >\"%~dp0{name}.browser-exit\" echo %ERRORLEVEL%\r\n\
                 ping -n {} 127.0.0.1 >nul\r\nexit /b 0\r\n",
                wait_secs + 1
            ),
        )
        .unwrap();
        f
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let f = dir.join(name);
        std::fs::write(
            &f,
            format!(
                "#!/bin/sh\n[ \"$1\" = auth ] || exit 9\n\
                 echo \"$CLAUDE_CONFIG_DIR\" > \"$(dirname \"$0\")/{name}.seen\"\n\
                 \"$BROWSER\" '{link}'\n\
                 echo $? > \"$(dirname \"$0\")/{name}.browser-exit\"\n\
                 sleep {wait_secs}\nexit 0\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        f
    }
}

struct Setup {
    d: tempfile::TempDir,
    roster_bin: PathBuf,
    p: RosterPaths,
    r: Roster,
}

fn setup() -> Setup {
    let d = tempfile::tempdir().unwrap();
    let spaced = d.path().join("Agent Roster dir");
    std::fs::create_dir_all(&spaced).unwrap();
    let src = PathBuf::from(env!("CARGO_BIN_EXE_roster"));
    let roster_bin = spaced.join(src.file_name().unwrap());
    std::fs::copy(&src, &roster_bin).unwrap();
    let platform = if cfg!(windows) {
        Platform::Windows
    } else {
        Platform::Linux
    };
    let p = RosterPaths::for_home(d.path(), platform, &|_| None);
    let config = d.path().join("claude-2");
    std::fs::create_dir_all(&config).unwrap();
    let r = Roster {
        version: 1,
        agents: vec![Agent {
            id: "claude_2".into(),
            name: "claude_2".into(),
            tool: Tool::Claude,
            config_dir: config.display().to_string(),
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
    Setup {
        d,
        roster_bin,
        p,
        r,
    }
}

fn start(s: &Setup, tool: &Path) -> (Receiver<LoginEvent>, roster_core::login::LoginHandle) {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let h = start_login_with(
        &s.r.agents[0],
        &s.r,
        &s.p,
        tool,
        Some(&s.roster_bin),
        Box::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        }),
    )
    .unwrap();
    (rx, h)
}

fn next(rx: &Receiver<LoginEvent>, secs: u64) -> LoginEvent {
    rx.recv_timeout(Duration::from_secs(secs))
        .expect("no event in time")
}

#[test]
fn the_link_comes_back_and_exit_0_is_done() {
    let s = setup();
    let tool = fake_claude(s.d.path(), "claude-ok", LINK, 1);
    let (rx, _h) = start(&s, &tool);
    assert_eq!(next(&rx, 20), LoginEvent::Link(LINK.into()));
    assert_eq!(next(&rx, 20), LoginEvent::Done);
    let seen = std::fs::read_to_string(s.d.path().join("claude-ok.seen")).unwrap();
    assert_eq!(
        seen.trim(),
        s.d.path().join("claude-2").display().to_string()
    );
    let code = std::fs::read_to_string(s.d.path().join("claude-ok.browser-exit")).unwrap();
    assert_eq!(code.trim(), "0");
}

#[test]
fn cancel_after_the_link_stops_claude() {
    let s = setup();
    let tool = fake_claude(s.d.path(), "claude-hang", LINK, 60);
    let (rx, h) = start(&s, &tool);
    assert_eq!(next(&rx, 20), LoginEvent::Link(LINK.into()));
    assert!(
        !h.wait_ended(Duration::from_millis(200)),
        "ended while running"
    );
    h.cancel();
    assert_eq!(next(&rx, 10), LoginEvent::Failed("canceled".into()));
    // App exit waits on this: the process tree is stopped.
    assert!(h.wait_ended(Duration::from_secs(10)), "never stopped");
    assert!(
        rx.recv_timeout(Duration::from_secs(1)).is_err(),
        "one end only"
    );
}

#[test]
fn a_link_off_claude_never_arrives() {
    let s = setup();
    let tool = fake_claude(
        s.d.path(),
        "claude-evil",
        "https://evil.example.com/login",
        30,
    );
    let (rx, _h) = start(&s, &tool);
    match next(&rx, 30) {
        LoginEvent::Failed(why) => {
            assert!(why.contains("didn't hand over a sign-in link"), "{why}");
            assert!(!why.contains("http"));
        }
        e => panic!("{e:?}"),
    }
    let code = std::fs::read_to_string(s.d.path().join("claude-evil.browser-exit")).unwrap();
    assert_ne!(code.trim(), "0");
}

/// Started with the hand-off variables, `roster` never runs a normal command.
#[test]
fn roster_with_the_handoff_variables_does_nothing_else() {
    let bin = env!("CARGO_BIN_EXE_roster");
    for args in [vec!["--version"], vec!["list"], vec![], vec![LINK, LINK]] {
        let out = Command::new(bin)
            .args(&args)
            .env("ROSTER_LOGIN_CHANNEL", "x")
            .env("ROSTER_LOGIN_NONCE", "y")
            .output()
            .unwrap();
        assert!(!out.status.success(), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
    }
    let out = Command::new(bin)
        .arg(LINK)
        .env("ROSTER_LOGIN_NONCE", "y")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
}
