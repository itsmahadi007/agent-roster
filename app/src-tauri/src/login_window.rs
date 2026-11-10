//! The Agent Roster login window: the provider's own sign-in page in a private webview.
//! Safety rules (docs/superpowers/specs/2026-11-10-in-app-sign-in-design.md):
//! - only pages `roster_core::login::allowed_in_window` accepts, on every navigation; anything
//!   else stops the login and sends the person to their browser;
//! - popups only for Google's own sign-in (`accounts.google.com`), every other one is refused.
//!   An allowed popup is a window Agent Roster makes (never the webview's own default popup),
//!   with the same page rules, title, private session and lifecycle as its login window,
//!   and it closes with its login;
//! - a private session per login (incognito plus its own data folder, deleted after);
//! - no Tauri capabilities (`capabilities/*.json` grant to `main` only), no injected script,
//!   no reading the page, and URLs are never logged;
//! - the current host is shown by Agent Roster in the window title, never by the page;
//! - closing the window really destroys it and cancels the login.

use crate::sign_in::{
    LoginUi, LoginWindowSpec, NEEDS_BROWSER, SignInUpdate, WINDOW_FAILED, WindowHooks,
};
use roster_core::login::Callback;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::webview::{NewWindowFeatures, NewWindowResponse, PageLoadEvent};
use tauri::{AppHandle, Emitter, Manager, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

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

/// The core's page rules, with local pages pinned to the callback server this login's link
/// returns to (its host and port), so no other local server can be reached from the login
/// window. With no callback in the link, no local page at all.
pub fn pinned(
    allowed: fn(&str) -> bool,
    callback: Option<Callback>,
) -> impl Fn(&str) -> bool + Clone + Send + Sync + 'static {
    move |s: &str| {
        let loopback = s
            .parse::<Url>()
            .ok()
            .and_then(|u| {
                u.host_str()
                    .map(|h| h.eq_ignore_ascii_case("localhost") || h == "127.0.0.1")
            })
            .unwrap_or(false);
        allowed(s) && (!loopback || callback.as_ref().is_some_and(|c| c.matches(s)))
    }
}

/// Popups are refused, except Google's own sign-in window, which "Continue with Google" on
/// claude.com opens and talks back to (the core's single rule).
pub fn popup_allowed(url: &Url) -> bool {
    roster_core::login::allowed_popup(url.as_str())
}

/// What a page's request for a new window (`window.open`, a `target=_blank` link) gets.
/// Never the webview's own default popup: that one would be outside every rule here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupDecision {
    /// A window Agent Roster makes, ruled like its login window.
    Open,
    Refuse,
}

/// Google's sign-in from a login window opens as a managed popup; anything else, and any
/// popup a popup asks for, is refused.
pub fn popup_decision(url: &Url, from_popup: bool) -> PopupDecision {
    if !from_popup && popup_allowed(url) {
        PopupDecision::Open
    } else {
        PopupDecision::Refuse
    }
}

/// The pages a login window and its popups may show (`page_allowed`), one object for both.
pub struct PageRules {
    allowed: Box<dyn Fn(&str) -> bool + Send + Sync>,
    app_origins: Vec<Url>,
}

impl PageRules {
    pub fn new(
        allowed: impl Fn(&str) -> bool + Send + Sync + 'static,
        app_origins: Vec<Url>,
    ) -> Arc<PageRules> {
        Arc::new(PageRules {
            allowed: Box::new(allowed),
            app_origins,
        })
    }

    pub fn allows(&self, url: &Url) -> bool {
        page_allowed(url, &*self.allowed, &self.app_origins)
    }
}

/// May a popup show this page? The login window's rules, plus the empty `about:blank` the
/// managed popup is created on (it loads nothing; the runtime then loads Google into it).
pub fn popup_page_allowed(url: &Url, rules: &PageRules) -> bool {
    url.as_str() == "about:blank" || rules.allows(url)
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

/// The login windows, by agent and login. A window is entered before it is made, because
/// its hooks can end the login while it is being made; a login that ended by then gets its
/// window closed as soon as it exists. An older login of an agent never reaches a newer
/// one's window.
#[derive(Default)]
pub struct WindowTable {
    slots: HashMap<(String, u64), Slot>,
}

struct Slot {
    label: String,
    /// Its login ended: the window goes (or is never kept, if still being made).
    closing: bool,
    /// Its popups' labels.
    popups: Vec<String>,
}

impl WindowTable {
    fn key(agent: &str, login: u64) -> (String, u64) {
        (agent.to_string(), login)
    }

    /// Before the window is made.
    pub fn opening(&mut self, agent: &str, login: u64, label: &str) {
        self.slots.insert(
            Self::key(agent, login),
            Slot {
                label: label.to_string(),
                closing: false,
                popups: vec![],
            },
        );
    }

    /// The window now exists. False: its login ended meanwhile, so it must be closed now.
    pub fn built(&mut self, agent: &str, login: u64) -> bool {
        let key = Self::key(agent, login);
        match self.slots.get(&key) {
            Some(s) if !s.closing => true,
            _ => {
                self.slots.remove(&key);
                false
            }
        }
    }

    /// The window couldn't be made.
    pub fn forget(&mut self, agent: &str, login: u64) {
        self.slots.remove(&Self::key(agent, login));
    }

    /// This login's window, to focus; none once its login ended.
    pub fn label(&self, agent: &str, login: u64) -> Option<&str> {
        self.slots
            .get(&Self::key(agent, login))
            .filter(|s| !s.closing)
            .map(|s| s.label.as_str())
    }

    /// The login ended: the windows to close now, its popups too (a window still being
    /// made is closed once made).
    pub fn close(&mut self, agent: &str, login: u64) -> Vec<String> {
        match self.slots.get_mut(&Self::key(agent, login)) {
            Some(s) => {
                s.closing = true;
                std::iter::once(s.label.clone())
                    .chain(s.popups.iter().cloned())
                    .collect()
            }
            None => vec![],
        }
    }

    /// The login window is gone: returns its popups, which go with it.
    pub fn destroyed(&mut self, agent: &str, login: u64) -> Vec<String> {
        self.slots
            .remove(&Self::key(agent, login))
            .map(|s| s.popups)
            .unwrap_or_default()
    }

    /// A popup of this login's window. False (no popup) once the login ended.
    pub fn add_popup(&mut self, agent: &str, login: u64, label: &str) -> bool {
        match self.slots.get_mut(&Self::key(agent, login)) {
            Some(s) if !s.closing => {
                s.popups.push(label.to_string());
                true
            }
            _ => false,
        }
    }

    /// Is this popup still wanted (its login on, not closed)?
    pub fn popup_wanted(&self, agent: &str, login: u64, label: &str) -> bool {
        self.slots
            .get(&Self::key(agent, login))
            .is_some_and(|s| !s.closing && s.popups.iter().any(|p| p == label))
    }

    /// The login window moved on to another page: its popups are done.
    pub fn take_popups(&mut self, agent: &str, login: u64) -> Vec<String> {
        self.slots
            .get_mut(&Self::key(agent, login))
            .map(|s| std::mem::take(&mut s.popups))
            .unwrap_or_default()
    }

    /// A popup is gone (closed by the person or by itself).
    pub fn popup_destroyed(&mut self, agent: &str, login: u64, label: &str) {
        if let Some(s) = self.slots.get_mut(&Self::key(agent, login)) {
            s.popups.retain(|p| p != label);
        }
    }
}

pub struct TauriLoginUi {
    app: AppHandle,
    windows: Arc<Mutex<WindowTable>>,
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

    fn table(&self) -> std::sync::MutexGuard<'_, WindowTable> {
        self.windows.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn destroy(app: &AppHandle, labels: &[String]) {
    for l in labels {
        if let Some(w) = app.get_webview_window(l) {
            let _ = w.destroy();
        }
    }
}

/// `destroy` off a window's own callback.
fn destroy_later(app: &AppHandle, labels: Vec<String>) {
    if labels.is_empty() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || destroy(&app, &labels));
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

/// One login window and what its popups share with it.
struct LoginWindow {
    app: AppHandle,
    agent: String,
    agent_name: String,
    login: u64,
    label: String,
    dir: PathBuf,
    rules: Arc<PageRules>,
    hooks: Arc<WindowHooks>,
    /// One "needs your browser" per login, however many blocked requests follow.
    blocked: AtomicBool,
    windows: Arc<Mutex<WindowTable>>,
    popups: AtomicU64,
}

impl LoginWindow {
    fn table(&self) -> std::sync::MutexGuard<'_, WindowTable> {
        self.windows.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The page went somewhere it may not: the login goes to the browser.
    fn block(&self) {
        if !self.blocked.swap(true, Ordering::SeqCst) {
            later(&self.hooks, true);
        }
    }

    /// Every navigation of the login window or of one of its popups (`label`).
    fn navigate(&self, label: &str, u: &Url) -> bool {
        self.navigate_by(label, u, self.rules.allows(u))
    }

    /// A popup's navigation: the same, plus its blank first page.
    fn navigate_popup(&self, label: &str, u: &Url) -> bool {
        if u.as_str() == "about:blank" {
            return true;
        }
        self.navigate_by(label, u, popup_page_allowed(u, &self.rules))
    }

    fn navigate_by(&self, label: &str, u: &Url, allowed: bool) -> bool {
        if allowed {
            set_title_later(&self.app, label, title_for(&self.agent_name, u));
            true
        } else {
            self.block();
            false
        }
    }

    fn new_window(
        self: &Arc<Self>,
        u: &Url,
        features: NewWindowFeatures,
        from_popup: bool,
    ) -> NewWindowResponse<tauri::Wry> {
        match popup_decision(u, from_popup) {
            PopupDecision::Open => match self.open_popup(u, features) {
                Some(window) => NewWindowResponse::Create { window },
                None => NewWindowResponse::Deny,
            },
            PopupDecision::Refuse => {
                // A popup's own popups are only refused; the login window's go to the browser.
                if !from_popup {
                    self.block();
                }
                NewWindowResponse::Deny
            }
        }
    }

    /// Makes the popup the page asked for: the webview runtime then loads the page into it
    /// and keeps `window.opener` working. Same rules, title, private session (the opener's
    /// own WebView2 environment / WKWebView configuration, incognito) and no capabilities
    /// (those are for `main` only); closed with its login.
    fn open_popup(self: &Arc<Self>, u: &Url, features: NewWindowFeatures) -> Option<WebviewWindow> {
        let n = self.popups.fetch_add(1, Ordering::Relaxed) + 1;
        let label = format!("{}-popup-{n}", self.label);
        if !self.table().add_popup(&self.agent, self.login, &label) {
            return None;
        }
        let blank: Url = "about:blank".parse().ok()?;
        let (nav, nav_label) = (Arc::clone(self), label.clone());
        let popups = Arc::clone(self);
        let load_name = self.agent_name.clone();
        let built = WebviewWindowBuilder::new(&self.app, &label, WebviewUrl::External(blank))
            .window_features(features)
            .title(title_for(&self.agent_name, u))
            .incognito(true)
            .data_directory(self.dir.clone())
            .on_navigation(move |u| nav.navigate_popup(&nav_label, u))
            .on_new_window(move |u, f| popups.new_window(&u, f, true))
            .on_page_load(move |w, p| {
                if p.event() == PageLoadEvent::Finished {
                    let _ = w.set_title(&title_for(&load_name, p.url()));
                }
            })
            .build();
        let Ok(window) = built else {
            self.table()
                .popup_destroyed(&self.agent, self.login, &label);
            return None;
        };
        let (me, my_label) = (Arc::clone(self), label.clone());
        window.on_window_event(move |e| {
            if let tauri::WindowEvent::Destroyed = e {
                me.table().popup_destroyed(&me.agent, me.login, &my_label);
            }
        });
        // The login may have ended while the popup was being made.
        if !self.table().popup_wanted(&self.agent, self.login, &label) {
            let _ = window.destroy();
            return None;
        }
        Some(window)
    }
}

impl LoginUi for TauriLoginUi {
    fn open(&self, spec: &LoginWindowSpec, hooks: WindowHooks) -> Result<(), String> {
        let url: Url = spec.url.parse().map_err(|_| WINDOW_FAILED.to_string())?;
        let rules = PageRules::new(
            pinned(
                roster_core::login::allowed_in_window,
                roster_core::login::redirect_callback(&spec.url),
            ),
            app_origins(&self.app),
        );
        if !rules.allows(&url) {
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
        let lw = Arc::new(LoginWindow {
            app: self.app.clone(),
            agent: spec.agent_id.clone(),
            agent_name: spec.agent_name.clone(),
            login: spec.login,
            label: label.clone(),
            dir: dir.clone(),
            rules,
            hooks: Arc::new(hooks),
            blocked: AtomicBool::new(false),
            windows: Arc::clone(&self.windows),
            popups: AtomicU64::new(0),
        });
        let (agent, login) = (spec.agent_id.clone(), spec.login);
        // Entered before the window is made: its hooks may end the login meanwhile.
        self.table().opening(&agent, login, &label);

        let (nav, nav_label) = (Arc::clone(&lw), label.clone());
        let pop = Arc::clone(&lw);
        let load = Arc::clone(&lw);
        let built = WebviewWindowBuilder::new(&self.app, &label, WebviewUrl::External(url))
            .title(first_title(spec))
            .inner_size(520.0, 760.0)
            .min_inner_size(420.0, 560.0)
            .incognito(true)
            .data_directory(dir.clone())
            .on_navigation(move |u| nav.navigate(&nav_label, u))
            .on_new_window(move |u, f| pop.new_window(&u, f, false))
            .on_page_load(move |w, p| match p.event() {
                // The login window moved on (Google's popup closes itself when done, and may
                // leave an empty frame): its popups are done.
                PageLoadEvent::Started => {
                    let done = load.table().take_popups(&load.agent, load.login);
                    destroy_later(&load.app, done);
                }
                PageLoadEvent::Finished => {
                    let _ = w.set_title(&title_for(&load.agent_name, p.url()));
                }
            })
            .build();
        let window = match built {
            Ok(w) => w,
            Err(_) => {
                self.table().forget(&agent, login);
                remove_later(dir);
                return Err(WINDOW_FAILED.into());
            }
        };
        let closed = Arc::new(AtomicBool::new(false));
        let gone = Arc::clone(&lw);
        window.on_window_event(move |e| {
            if let tauri::WindowEvent::Destroyed = e
                && !closed.swap(true, Ordering::SeqCst)
            {
                let popups = gone.table().destroyed(&gone.agent, gone.login);
                destroy_later(&gone.app, popups);
                later(&gone.hooks, false);
                remove_later(gone.dir.clone());
            }
        });
        if !self.table().built(&agent, login) {
            // Its login ended while the window was being made.
            let _ = window.destroy();
        }
        Ok(())
    }

    fn focus(&self, agent_id: &str, login: u64) -> bool {
        let label = self.table().label(agent_id, login).map(str::to_string);
        let Some(w) = label.and_then(|l| self.app.get_webview_window(&l)) else {
            return false;
        };
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        true
    }

    fn close(&self, agent_id: &str, login: u64) {
        let labels = self.table().close(agent_id, login);
        destroy(&self.app, &labels);
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
    fn loopback_pages_only_on_the_logins_own_port() {
        // The link sends the browser back to Codex's 1455: another local server (any other
        // port, or the same port under the other loopback name) must not be reachable.
        let link = "https://auth.openai.com/oauth/authorize?redirect_uri=http%3A%2F%2F127.0.0.1%3A1455%2Fauth%2Fcallback";
        let ok = pinned(fake_allowed, roster_core::login::redirect_callback(link));
        assert!(ok("http://127.0.0.1:1455/auth/callback"));
        assert!(ok("http://127.0.0.1:1455/success"));
        assert!(!ok("http://127.0.0.1:8080/"));
        assert!(!ok("http://localhost:3000/callback"));
        assert!(!ok("http://localhost:1455/success"));
        assert!(ok("https://auth.openai.com/log-in"));
        // No callback in the link: no local page at all, the provider's pages still.
        let none = pinned(fake_allowed, None);
        assert!(!none("http://localhost:3000/callback"));
        assert!(!none("http://127.0.0.1:1455/auth/callback"));
        assert!(none("https://auth.openai.com/log-in"));
    }

    #[test]
    fn a_popup_may_start_blank_but_the_login_window_may_not() {
        // The managed popup is created on about:blank (Tauri's documented way); refusing it
        // would end the Google sign-in before it starts.
        let rules = PageRules::new(fake_allowed, vec![]);
        assert!(popup_page_allowed(&url("about:blank"), &rules));
        assert!(!rules.allows(&url("about:blank")));
        assert!(
            popup_page_allowed(&url("https://accounts.google.com/o/oauth2/v2/auth"), &rules)
                == rules.allows(&url("https://accounts.google.com/o/oauth2/v2/auth"))
        );
        // Nothing else that isn't a web page.
        assert!(!popup_page_allowed(&url("about:srcdoc"), &rules));
        assert!(!popup_page_allowed(&url("data:text/html,hi"), &rules));
        assert!(!popup_page_allowed(
            &url("https://evil.example.com/"),
            &rules
        ));
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
            login: 1,
            agent_name: "claude_2".into(),
            tool_label: "Claude Code".into(),
            url: String::new(),
        };
        assert_eq!(first_title(&spec), "Sign in claude_2 · Claude Code");
    }

    /// A login that ends while its window is still being made: the window is closed as
    /// soon as it exists, never left open.
    #[test]
    fn a_window_closed_while_opening_is_closed_once_made() {
        let mut t = WindowTable::default();
        t.opening("a", 1, "login-1");
        assert_eq!(t.close("a", 1), ["login-1"]);
        assert!(!t.built("a", 1), "kept a window whose login ended");
        assert_eq!(t.label("a", 1), None);
        // An ordinary one stays.
        t.opening("a", 2, "login-2");
        assert!(t.built("a", 2));
        assert_eq!(t.label("a", 2), Some("login-2"));
    }

    /// Windows are kept by login, not only by agent: an older login of the same agent can't
    /// close, focus or forget a newer login's window.
    #[test]
    fn an_older_login_never_touches_a_newer_ones_window() {
        let mut t = WindowTable::default();
        t.opening("a", 1, "login-1");
        assert!(t.built("a", 1));
        t.opening("a", 2, "login-2");
        assert!(t.built("a", 2));
        assert_eq!(t.close("a", 1), ["login-1"]);
        assert_eq!(t.label("a", 1), None);
        assert_eq!(t.label("a", 2), Some("login-2"));
        // The old window's "destroyed" arrives late.
        t.destroyed("a", 1);
        assert_eq!(t.label("a", 2), Some("login-2"));
        assert!(t.close("a", 3).is_empty());
        assert_eq!(t.label("b", 2), None);
        t.destroyed("a", 2);
        assert_eq!(t.label("a", 2), None);
    }

    /// A page's popup is never an unmanaged webview: either a window Agent Roster makes (and
    /// rules) or nothing. A popup's own popups are refused.
    #[test]
    fn a_popup_is_a_managed_window_or_refused() {
        let google = url("https://accounts.google.com/o/oauth2/v2/auth?client_id=x");
        assert_eq!(popup_decision(&google, false), PopupDecision::Open);
        assert_eq!(popup_decision(&google, true), PopupDecision::Refuse);
        assert_eq!(
            popup_decision(&url("https://login.microsoftonline.com/"), false),
            PopupDecision::Refuse
        );
        assert_eq!(
            popup_decision(&url("https://accounts.google.com.evil.example/"), false),
            PopupDecision::Refuse
        );
    }

    /// The popup is checked by the very rules of its login window: the core's pages, the
    /// login's own callback only, never the app's pages.
    #[test]
    fn a_popup_follows_its_login_windows_page_rules() {
        let link = "https://claude.com/cai/oauth/authorize?redirect_uri=http%3A%2F%2Flocalhost%3A61234%2Fcallback";
        let rules = PageRules::new(
            pinned(
                roster_core::login::allowed_in_window,
                roster_core::login::redirect_callback(link),
            ),
            vec![url("http://tauri.localhost")],
        );
        assert!(rules.allows(&url("https://accounts.google.com/signin/v2")));
        assert!(rules.allows(&url("https://claude.ai/login")));
        assert!(rules.allows(&url("http://localhost:61234/callback?code=x")));
        assert!(!rules.allows(&url("http://localhost:8080/")));
        assert!(!rules.allows(&url("https://evil.example.com/")));
        assert!(!rules.allows(&url("http://tauri.localhost/")));
        // Shared, not copied: the popup gets the same rules object.
        let popup = Arc::clone(&rules);
        assert!(!popup.allows(&url("https://appleid.apple.com/")));
    }

    #[test]
    fn popups_live_and_die_with_their_login() {
        let mut t = WindowTable::default();
        t.opening("a", 1, "login-1");
        assert!(t.built("a", 1));
        assert!(t.add_popup("a", 1, "login-1-popup-1"));
        assert!(!t.add_popup("a", 2, "login-2-popup-1"), "no such login");
        assert_eq!(t.close("a", 1), ["login-1", "login-1-popup-1"]);
        assert!(
            !t.add_popup("a", 1, "login-1-popup-2"),
            "a popup after its login ended"
        );
        // The login window going takes its popups with it.
        t.opening("b", 3, "login-3");
        assert!(t.built("b", 3));
        assert!(t.add_popup("b", 3, "login-3-popup-1"));
        assert_eq!(t.destroyed("b", 3), ["login-3-popup-1"]);
    }

    /// Google's popup closes itself when done, which leaves an empty frame on some systems:
    /// once its login window moves on to another page, the popups go.
    #[test]
    fn popups_go_when_their_login_window_moves_on() {
        let mut t = WindowTable::default();
        t.opening("a", 1, "login-1");
        assert!(t.built("a", 1));
        assert!(t.add_popup("a", 1, "p1"));
        assert_eq!(t.take_popups("a", 1), ["p1"]);
        assert!(t.take_popups("a", 1).is_empty());
        assert!(t.add_popup("a", 1, "p2"));
        t.popup_destroyed("a", 1, "p2");
        assert_eq!(t.close("a", 1), ["login-1"]);
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
