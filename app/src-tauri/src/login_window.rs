//! The Agent Roster login window: the provider's own sign-in page in a private webview.
//! Safety rules (docs/superpowers/specs/2026-11-10-in-app-sign-in-design.md):
//! - only pages `roster_core::login::allowed_in_window` accepts, on every navigation; anything
//!   else stops the login and sends the person to their browser;
//! - popups only for Google's own sign-in (`accounts.google.com`), every other one is refused;
//! - a private session per login (incognito plus its own data folder, deleted after);
//! - no Tauri capabilities (`capabilities/*.json` grant to `main` only), no injected script,
//!   no reading the page, and URLs are never logged;
//! - the current host is shown by Agent Roster in the window title, never by the page;
//! - closing the window really destroys it and cancels the login.

use crate::sign_in::{
    LoginUi, LoginWindowSpec, NEEDS_BROWSER, SignInUpdate, WINDOW_FAILED, WindowHooks,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::{AppHandle, Emitter, Manager, Url, WebviewUrl, WebviewWindowBuilder};

/// May the login window show this page? What the core allows, never one of the app's own
/// pages (those could reach the app's commands), and only web pages.
pub fn page_allowed(url: &Url, allowed: &dyn Fn(&str) -> bool, app_origins: &[Url]) -> bool {
    matches!(url.scheme(), "https" | "http")
        && !url
            .host_str()
            .is_some_and(|h| h.to_ascii_lowercase().ends_with(".localhost"))
        && !app_origins.iter().any(|o| o.origin() == url.origin())
        && allowed(url.as_str())
}

/// Popups are refused, except Google's own sign-in window, which "Continue with Google" on
/// claude.com opens and talks back to.
pub fn popup_allowed(url: &Url) -> bool {
    url.scheme() == "https" && url.host_str() == Some("accounts.google.com") && url.port().is_none()
}

/// "Sign in codex_2 · auth.openai.com 🔒": the page's real host, as ASCII (punycode for
/// international names), so a look-alike name can't hide. Never the path or query.
pub fn title_for(agent: &str, url: &Url) -> String {
    let host = url.host_str().unwrap_or("?");
    match url.scheme() {
        "https" => format!("Sign in {agent} · {host} 🔒"),
        _ => format!("Sign in {agent} · {host} (this computer)"),
    }
}

pub fn first_title(spec: &LoginWindowSpec) -> String {
    format!("Sign in {} · {}", spec.agent_name, spec.tool_label)
}

/// The parent of every login's private data folder.
fn data_root() -> PathBuf {
    std::env::temp_dir().join("agent-roster-login")
}

/// Removes data folders left by a login window that didn't close cleanly (a crash).
pub fn clean_old_data() {
    let _ = std::fs::remove_dir_all(data_root());
}

/// The webview may hold its folder for a moment after the window is gone.
fn remove_later(dir: PathBuf) {
    std::thread::spawn(move || {
        for _ in 0..40 {
            if std::fs::remove_dir_all(&dir).is_ok() || !dir.exists() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    });
}

/// The app's own page addresses: a login window never shows these.
fn app_origins(app: &AppHandle) -> Vec<Url> {
    let mut v = vec![];
    if let Some(dev) = app.config().build.dev_url.clone() {
        v.push(dev);
    }
    for u in [
        "tauri://localhost",
        "http://tauri.localhost",
        "https://tauri.localhost",
    ] {
        if let Ok(u) = Url::parse(u) {
            v.push(u);
        }
    }
    v
}

pub struct TauriLoginUi {
    app: AppHandle,
    /// Agent id → the label of its login window.
    windows: Arc<Mutex<HashMap<String, String>>>,
    next: AtomicU64,
}

impl TauriLoginUi {
    pub fn new(app: AppHandle) -> TauriLoginUi {
        TauriLoginUi {
            app,
            windows: Arc::default(),
            next: AtomicU64::new(1),
        }
    }

    fn label_of(&self, agent_id: &str) -> Option<String> {
        self.windows
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(agent_id)
            .cloned()
    }
}

/// Runs a hook off the webview's own callback, which must return before the window can close.
fn later(hooks: &Arc<WindowHooks>, blocked: bool) {
    let h = Arc::clone(hooks);
    std::thread::spawn(move || {
        if blocked {
            (h.on_blocked)()
        } else {
            (h.on_closed)()
        }
    });
}

fn set_title_later(app: &AppHandle, label: &str, title: String) {
    let (app, label) = (app.clone(), label.to_string());
    std::thread::spawn(move || {
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.set_title(&title);
        }
    });
}

impl LoginUi for TauriLoginUi {
    fn open(&self, spec: &LoginWindowSpec, hooks: WindowHooks) -> Result<(), String> {
        let url: Url = spec.url.parse().map_err(|_| WINDOW_FAILED.to_string())?;
        let origins = app_origins(&self.app);
        let allowed = roster_core::login::allowed_in_window;
        if !page_allowed(&url, &allowed, &origins) {
            return Err(NEEDS_BROWSER.into());
        }
        let n = self.next.fetch_add(1, Ordering::Relaxed);
        let label = format!("login-{n}");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let dir = data_root().join(format!("{label}-{stamp}"));
        std::fs::create_dir_all(&dir).map_err(|_| WINDOW_FAILED.to_string())?;
        let hooks = Arc::new(hooks);
        // One "needs your browser" per window, however many blocked requests follow.
        let blocked = Arc::new(AtomicBool::new(false));

        let (nav_app, nav_label, nav_name) =
            (self.app.clone(), label.clone(), spec.agent_name.clone());
        let (nav_hooks, nav_blocked) = (Arc::clone(&hooks), Arc::clone(&blocked));
        let (pop_hooks, pop_blocked) = (Arc::clone(&hooks), Arc::clone(&blocked));
        let load_name = spec.agent_name.clone();
        let built = WebviewWindowBuilder::new(&self.app, &label, WebviewUrl::External(url))
            .title(first_title(spec))
            .inner_size(520.0, 760.0)
            .min_inner_size(420.0, 560.0)
            .incognito(true)
            .data_directory(dir.clone())
            .on_navigation(move |u| {
                if page_allowed(u, &allowed, &origins) {
                    set_title_later(&nav_app, &nav_label, title_for(&nav_name, u));
                    true
                } else {
                    if !nav_blocked.swap(true, Ordering::SeqCst) {
                        later(&nav_hooks, true);
                    }
                    false
                }
            })
            .on_new_window(move |u, _features| {
                if popup_allowed(&u) {
                    NewWindowResponse::Allow
                } else {
                    if !pop_blocked.swap(true, Ordering::SeqCst) {
                        later(&pop_hooks, true);
                    }
                    NewWindowResponse::Deny
                }
            })
            .on_page_load(move |w, p| {
                if p.event() == PageLoadEvent::Finished {
                    let _ = w.set_title(&title_for(&load_name, p.url()));
                }
            })
            .build();
        let window = match built {
            Ok(w) => w,
            Err(_) => {
                remove_later(dir);
                return Err(WINDOW_FAILED.into());
            }
        };
        let windows = Arc::clone(&self.windows);
        let (agent_id, my_label) = (spec.agent_id.clone(), label.clone());
        let closed = Arc::new(AtomicBool::new(false));
        window.on_window_event(move |e| {
            if let tauri::WindowEvent::Destroyed = e
                && !closed.swap(true, Ordering::SeqCst)
            {
                {
                    let mut m = windows.lock().unwrap_or_else(|e| e.into_inner());
                    if m.get(&agent_id) == Some(&my_label) {
                        m.remove(&agent_id);
                    }
                }
                later(&hooks, false);
                remove_later(dir.clone());
            }
        });
        self.windows
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(spec.agent_id.clone(), label);
        Ok(())
    }

    fn focus(&self, agent_id: &str) -> bool {
        let Some(w) = self
            .label_of(agent_id)
            .and_then(|l| self.app.get_webview_window(&l))
        else {
            return false;
        };
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        true
    }

    fn close(&self, agent_id: &str) {
        if let Some(w) = self
            .label_of(agent_id)
            .and_then(|l| self.app.get_webview_window(&l))
        {
            let _ = w.destroy();
        }
    }

    fn notify(&self, u: &SignInUpdate) {
        let _ = self.app.emit_to("main", "sign-in", u);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    /// Stands in for the core's list: provider sites and the loopback callback.
    fn fake_allowed(u: &str) -> bool {
        let u = url(u);
        matches!(
            (u.scheme(), u.host_str().unwrap_or("")),
            (
                "https",
                "auth.openai.com" | "claude.com" | "platform.claude.com"
            ) | ("http", "localhost" | "127.0.0.1")
        )
    }

    #[test]
    fn only_pages_the_core_allows_and_never_the_apps_own() {
        let app = [url("http://localhost:1420"), url("http://tauri.localhost")];
        let ok = |s: &str| page_allowed(&url(s), &fake_allowed, &app);
        assert!(ok("https://auth.openai.com/log-in?x=1"));
        assert!(ok("https://platform.claude.com/oauth/code/success"));
        assert!(ok("http://127.0.0.1:1455/auth/callback?code=x"));
        assert!(ok("http://127.0.0.1:1455/success"));
        assert!(!ok("https://evil.example.com/"));
        assert!(!ok("http://auth.openai.com/"));
        // The app's own pages can reach its commands: never inside a login window.
        assert!(!ok("http://localhost:1420/"));
        assert!(!ok("http://localhost:1420/callback"));
        assert!(!ok("http://tauri.localhost/"));
        assert!(!ok("http://evil.localhost/"));
        assert!(!ok("data:text/html,hi"));
        assert!(!ok("javascript:alert(1)"));
    }

    #[test]
    fn popups_only_for_googles_own_sign_in() {
        assert!(popup_allowed(&url(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id=x"
        )));
        assert!(!popup_allowed(&url("http://accounts.google.com/")));
        assert!(!popup_allowed(&url(
            "https://accounts.google.com.evil.example/"
        )));
        assert!(!popup_allowed(&url("https://accounts.google.com:8443/")));
        assert!(!popup_allowed(&url("https://login.microsoftonline.com/")));
        assert!(!popup_allowed(&url("https://auth.openai.com/")));
    }

    #[test]
    fn the_title_shows_the_real_host_and_nothing_else() {
        assert_eq!(
            title_for(
                "codex_2",
                &url("https://auth.openai.com/log-in?code=secret#x")
            ),
            "Sign in codex_2 · auth.openai.com 🔒"
        );
        // International names show as punycode, so a look-alike can't pass for the real one.
        assert_eq!(
            title_for("a", &url("https://аuth.openai.com/")),
            "Sign in a · xn--uth-5cd.openai.com 🔒"
        );
        assert_eq!(
            title_for("a", &url("http://127.0.0.1:1455/auth/callback?code=x")),
            "Sign in a · 127.0.0.1 (this computer)"
        );
        let spec = LoginWindowSpec {
            agent_id: "x".into(),
            agent_name: "claude_2".into(),
            tool_label: "Claude Code".into(),
            url: String::new(),
        };
        assert_eq!(first_title(&spec), "Sign in claude_2 · Claude Code");
    }

    /// Remote pages get no IPC: every capability grants to the main window only, by label,
    /// with no remote URLs, so a login window (any other label) gets nothing, after any redirect.
    #[test]
    fn capabilities_are_for_the_main_window_only() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities");
        let mut seen = 0;
        for e in std::fs::read_dir(&dir).unwrap() {
            let path = e.unwrap().path();
            let name = path.display().to_string();
            assert_eq!(
                path.extension().and_then(|x| x.to_str()),
                Some("json"),
                "{name}: only JSON capabilities are checked here"
            );
            let v: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            assert_eq!(v["windows"], serde_json::json!(["main"]), "{name}");
            assert!(v.get("webviews").is_none(), "{name}");
            assert!(v.get("remote").is_none(), "{name}");
            seen += 1;
        }
        assert!(seen > 0);
        // No capabilities declared inline in the config either.
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let inline = &conf["app"]["security"]["capabilities"];
        assert!(inline.is_null(), "inline capabilities: {inline}");
    }
}
