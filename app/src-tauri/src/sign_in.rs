//! Signing an agent in from the app: in the person's browser (a terminal running
//! `roster login <name>`, today's way and the default) or in an Agent Roster login window.
//! This module holds the setting and what the backend needs to drive a login window without
//! knowing about Tauri, so tests can fake both the login and the window.

use roster_core::login::{LoginEvent, LoginHandle};
use roster_core::model::{Agent, Roster};
use roster_core::paths::RosterPaths;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Where Sign in sends the person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SignInWith {
    /// A terminal running the tool's own sign-in, which opens the browser. The owner's default.
    #[default]
    Browser,
    /// An Agent Roster window showing the provider's sign-in page.
    Window,
}

/// The app's own preferences (not roster changes, so not journaled).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub sign_in_with: SignInWith,
}

/// The app's settings file, next to the roster file.
pub fn settings_file(p: &RosterPaths) -> PathBuf {
    p.config_dir.join("app-settings.json")
}

/// The saved settings; the defaults when the file is missing or unreadable.
pub fn load_settings(file: &Path) -> AppSettings {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_settings(file: &Path, s: &AppSettings) -> Result<(), String> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, file).map_err(|e| e.to_string())
}

/// What the main window hears about an in-app sign-in (event `sign-in`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SignInUpdate {
    pub id: String,
    pub name: String,
    /// "waiting" (the login window is open), "done", "failed" or "canceled".
    pub state: &'static str,
    /// Why it failed, for people: no URL, code or token.
    pub reason: Option<String>,
}

/// One login window to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginWindowSpec {
    pub agent_id: String,
    pub agent_name: String,
    pub tool_label: String,
    pub url: String,
}

type Hook = Box<dyn Fn() + Send + Sync>;

/// What the login window reports back.
pub struct WindowHooks {
    /// The page went somewhere the window may not show (Google, company sign-in, ...).
    pub on_blocked: Hook,
    /// The window was closed (by the person, or by the app).
    pub on_closed: Hook,
}

/// The login windows and the main window's sign-in event, behind a trait so tests can fake them.
pub trait LoginUi: Send + Sync {
    fn open(&self, spec: &LoginWindowSpec, hooks: WindowHooks) -> Result<(), String>;
    /// Brings this agent's login window to the front; false when there is none.
    fn focus(&self, agent_id: &str) -> bool;
    fn close(&self, agent_id: &str);
    fn notify(&self, u: &SignInUpdate);
}

/// Everything a login needs to start.
pub struct LoginRequest<'a> {
    pub agent: &'a Agent,
    pub roster: &'a Roster,
    pub paths: &'a RosterPaths,
    pub path_var: &'a str,
}

pub type OnEvent = Box<dyn Fn(LoginEvent) + Send + Sync>;
pub type LoginStarter =
    Box<dyn Fn(&LoginRequest, OnEvent) -> Result<LoginHandle, String> + Send + Sync>;

/// The real login, from roster-core.
pub fn real_starter() -> LoginStarter {
    Box::new(|req: &LoginRequest, on_event: OnEvent| {
        let tool = roster_core::detect::find_tool(req.agent.tool, req.path_var, req.paths)
            .ok_or_else(|| {
                format!(
                    "{} isn't installed on this computer.",
                    req.agent.tool.display()
                )
            })?;
        roster_core::login::start_login(req.agent, req.roster, req.paths, &tool, on_event)
            .map_err(|e| e.to_string())
    })
}

pub const NEEDS_BROWSER: &str = "This sign-in needs your browser.";
pub const WINDOW_FAILED: &str = "The sign-in window couldn't open.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_the_browser() {
        assert_eq!(AppSettings::default().sign_in_with, SignInWith::Browser);
        let d = tempfile::tempdir().unwrap();
        assert_eq!(
            load_settings(&d.path().join("missing.json")).sign_in_with,
            SignInWith::Browser
        );
        let bad = d.path().join("bad.json");
        std::fs::write(&bad, "not json").unwrap();
        assert_eq!(load_settings(&bad).sign_in_with, SignInWith::Browser);
        std::fs::write(&bad, "{}").unwrap();
        assert_eq!(load_settings(&bad).sign_in_with, SignInWith::Browser);
    }

    #[test]
    fn the_setting_is_saved_and_read_back() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("sub").join("app-settings.json");
        let s = AppSettings {
            sign_in_with: SignInWith::Window,
        };
        save_settings(&f, &s).unwrap();
        assert_eq!(load_settings(&f), s);
        assert!(
            std::fs::read_to_string(&f)
                .unwrap()
                .contains(r#""sign_in_with": "window""#)
        );
    }
}
