//! Signing an agent in without a terminal: Agent Roster starts the tool's own login, hidden,
//! and gets back the sign-in link, which the app shows in its own login window (or in the
//! browser). The tool finishes the login itself through its own local callback server, so
//! Agent Roster never collects a password, a code or a token.
//!
//! - Codex: `codex app-server` → `account/login/start {type:"chatgpt"}` → `authUrl`; then
//!   `account/login/completed`; `account/login/cancel` when the window is closed.
//! - Claude Code: `claude auth login --claudeai` with `BROWSER` set to `roster.exe` and a
//!   one-time authenticated channel; `roster.exe` hands the link back over that channel.
//!
//! The tool's process tree is always stopped at the end (done, failed or canceled). Every
//! spawn is hidden. URLs, codes and the tools' output are never logged, and `Failed` texts are
//! fixed (`msg`).

pub mod allow;
pub mod bridge;
pub mod claude;
pub mod codex;

pub use allow::{
    Callback, allowed_popup, callback_matches, is_first_page, redirect_callback, redirect_port,
};

use crate::model::{Agent, Roster, Tool};
use crate::names::PathLookup;
use crate::paths::RosterPaths;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// What a running login reports, in order: one `Link`, then `Done` or `Failed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginEvent {
    /// The provider's sign-in page (https, provider origin already checked).
    Link(String),
    /// The tool saved its login.
    Done,
    /// The login ended without signing in: a short reason for people, with no URL, code or
    /// token in it.
    Failed(String),
}

/// A login in progress. Dropping it does not stop it; call `cancel`.
pub struct LoginHandle {
    cancel: Box<dyn Fn() + Send + Sync>,
    end: Option<EndSignal>,
}

impl LoginHandle {
    /// A handle with nothing to wait for once canceled.
    pub fn new(cancel: Box<dyn Fn() + Send + Sync>) -> Self {
        LoginHandle { cancel, end: None }
    }
    /// A handle whose `wait_ended` waits for `end`: signalled once the login has stopped
    /// everything it started.
    pub fn with_end(cancel: Box<dyn Fn() + Send + Sync>, end: EndSignal) -> Self {
        LoginHandle {
            cancel,
            end: Some(end),
        }
    }
    /// Stops the login and everything it started. A `Failed("canceled")` follows.
    pub fn cancel(&self) {
        (self.cancel)()
    }
    /// Waits, at most `within`, for the login to have stopped everything it started. True
    /// once it has (or when there is nothing to wait for).
    pub fn wait_ended(&self, within: Duration) -> bool {
        self.end.as_ref().is_none_or(|e| e.wait(within))
    }
}

/// Set once a login's process tree is stopped, so app exit can wait for it.
#[derive(Clone, Default)]
pub struct EndSignal(Arc<(Mutex<bool>, Condvar)>);

impl EndSignal {
    pub fn signal(&self) {
        let (done, cv) = &*self.0;
        *done.lock().unwrap_or_else(|e| e.into_inner()) = true;
        cv.notify_all();
    }
    /// True once signalled; false when `within` passed first.
    pub fn wait(&self, within: Duration) -> bool {
        let (done, cv) = &*self.0;
        let guard = done.lock().unwrap_or_else(|e| e.into_inner());
        let (guard, _) = cv
            .wait_timeout_while(guard, within, |d| !*d)
            .unwrap_or_else(|e| e.into_inner());
        *guard
    }
}

/// Starts signing `agent` in. `on_event` is called from a background thread.
///
/// Claude Code needs `roster` as its `BROWSER`: the copy next to this program first (same
/// version, so it knows the hand-off), else the user's own (see `roster_for_bridge`).
pub fn start_login(
    agent: &Agent,
    r: &Roster,
    p: &RosterPaths,
    tool_path: &Path,
    on_event: Box<dyn Fn(LoginEvent) + Send + Sync>,
) -> std::io::Result<LoginHandle> {
    let roster_bin = match agent.tool {
        Tool::Claude => Some(roster_for_bridge(p).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "the roster command is missing",
            )
        })?),
        Tool::Codex => None,
    };
    start_login_with(agent, r, p, tool_path, roster_bin.as_deref(), on_event)
}

/// The `roster` program Claude Code's `BROWSER` points at: the one shipped next to the
/// running program, else the user's own `roster`.
pub fn roster_for_bridge(p: &RosterPaths) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    if exe
        .file_stem()
        .is_some_and(|s| s.eq_ignore_ascii_case("roster"))
    {
        return Some(exe);
    }
    crate::desktop::bundled_roster(p.platform, &exe).or_else(|| {
        let lookup = PathLookup {
            exclude: Some(p.bin_dir()),
        };
        crate::desktop::find_roster_bin(p, &lookup)
    })
}

/// Only one Codex sign-in at a time: they all need port 1455.
static CODEX_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Gives one `Failed` from a background thread, and a handle with nothing to cancel.
fn failed_now(why: &'static str, on_event: Box<dyn Fn(LoginEvent) + Send + Sync>) -> LoginHandle {
    std::thread::spawn(move || on_event(LoginEvent::Failed(why.into())));
    LoginHandle::new(Box::new(|| {}))
}

/// Is something already listening on Codex's callback port? Only checked by trying to
/// listen there for a moment; whatever is there is left alone.
fn codex_port_busy() -> bool {
    std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, codex::CALLBACK_PORT)).is_err()
}

/// `start_login` with `roster_bin` (Claude Code's `BROWSER`) passed in, so tests can use
/// a fake. `None` is fine for Codex.
pub fn start_login_with(
    agent: &Agent,
    r: &Roster,
    p: &RosterPaths,
    tool_path: &Path,
    roster_bin: Option<&Path>,
    on_event: Box<dyn Fn(LoginEvent) + Send + Sync>,
) -> std::io::Result<LoginHandle> {
    let env = crate::run::build_env(agent, r, p, None);
    let dir = r.config_dir_of(agent, p);
    let cwd = if dir.is_dir() {
        dir
    } else {
        std::env::temp_dir()
    };
    match agent.tool {
        Tool::Codex => {
            if CODEX_ACTIVE.swap(true, Ordering::SeqCst) {
                return Ok(failed_now(msg::CODEX_BUSY, on_event));
            }
            let release = || CODEX_ACTIVE.store(false, Ordering::SeqCst);
            if codex_port_busy() {
                release();
                return Ok(failed_now(msg::PORT_BUSY, on_event));
            }
            let (tx, rx) = channel();
            let cancel = tx.clone();
            let end = EndSignal::default();
            let ended = end.clone();
            let on_end = Box::new(move || {
                release();
                ended.signal();
            });
            codex::run(tool_path, &env, &cwd, tx, rx, on_event, on_end)
                .inspect_err(|_| release())?;
            Ok(LoginHandle::with_end(
                Box::new(move || {
                    let _ = cancel.send(codex::Input::Cancel);
                }),
                end,
            ))
        }
        Tool::Claude => {
            let roster_bin = roster_bin.ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "the roster command is missing",
                )
            })?;
            let (tx, rx) = channel();
            let cancel = tx.clone();
            let end = EndSignal::default();
            let ended = end.clone();
            claude::run(
                tool_path,
                &env,
                &cwd,
                roster_bin,
                tx,
                rx,
                on_event,
                Box::new(move || ended.signal()),
            )?;
            Ok(LoginHandle::with_end(
                Box::new(move || {
                    let _ = cancel.send(claude::Input::Cancel);
                }),
                end,
            ))
        }
    }
}

/// Is this page allowed in the login window? First page and every later one. Only https on the
/// tested provider sites (and Google's own sign-in), or the tool's own loopback callback
/// (`http://localhost|127.0.0.1:<port>`). Everything else (Microsoft, Apple, company sign-in)
/// must go to the browser. See `allow`.
pub fn allowed_in_window(url: &str) -> bool {
    allow::allowed_in_window(url)
}

/// What people are told when a login ends without signing in. Fixed texts: never a URL, a
/// code, a token or anything a tool printed.
pub mod msg {
    pub const CANCELED: &str = "canceled";
    pub const PORT_BUSY: &str =
        "Another Codex sign-in is using the sign-in port. Finish or close it, then try again.";
    pub const CODEX_BUSY: &str =
        "Another Codex sign-in is open in Agent Roster. Finish or close it, then try again.";
    pub const CODEX_START_FAILED: &str =
        "Codex couldn't start signing in. Try again, or use Sign in with your browser.";
    pub const CODEX_STOPPED: &str = "Codex stopped before the sign-in finished. Try again.";
    pub const NOT_FINISHED: &str = "The sign-in didn't finish. Try again.";
    pub const TIMED_OUT: &str = "The sign-in took too long. Try again.";
    pub const BAD_LINK: &str =
        "The sign-in link wasn't on the expected site. Use Sign in with your browser.";
    pub const NO_CLAUDE_LINK: &str =
        "Claude Code didn't hand over a sign-in link. Use Sign in with your browser.";
    pub const CLAUDE_FAILED: &str = "Claude Code couldn't finish signing in. Try again.";
    pub const CLAUDE_START_FAILED: &str =
        "Claude Code couldn't start signing in. Try again, or use Sign in with your browser.";
}
