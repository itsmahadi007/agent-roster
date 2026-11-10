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
//! STUB: the signatures are final; the bodies are being written.

use crate::model::{Agent, Roster};
use crate::paths::RosterPaths;
use std::path::Path;

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
}

impl LoginHandle {
    pub fn new(cancel: Box<dyn Fn() + Send + Sync>) -> Self {
        LoginHandle { cancel }
    }
    /// Stops the login and everything it started. A `Failed("canceled")` follows.
    pub fn cancel(&self) {
        (self.cancel)()
    }
}

/// Starts signing `agent` in. `on_event` is called from a background thread.
pub fn start_login(
    _agent: &Agent,
    _r: &Roster,
    _p: &RosterPaths,
    _tool_path: &Path,
    _on_event: Box<dyn Fn(LoginEvent) + Send + Sync>,
) -> std::io::Result<LoginHandle> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "signing in inside Agent Roster is not built yet",
    ))
}

/// Is this page allowed in the login window? First page and every later one. Only https on the
/// tested provider sites, or the tool's own loopback callback (`http://localhost|127.0.0.1`).
/// Everything else (Google, Microsoft, Apple, company sign-in) must go to the browser.
pub fn allowed_in_window(_url: &str) -> bool {
    false
}
