//! The app's bridge to roster-core. Every change is previewed first; the exact plan the
//! person saw is the one applied, and only if the roster file has not changed meanwhile.

use roster_core::desktop::{
    TerminalApp, app_open_command, find_terminals, terminal_command_in, url_open_command,
};
use roster_core::detect::RealRunner;
use roster_core::import::plan_import;
use roster_core::journal::Journal;
use roster_core::model::{AgentColor, Roster, Tool};
use roster_core::names::CommandLookup;
use roster_core::ops::{self, AddRequest, Ctx, Plan, StepKind};
use roster_core::paths::RosterPaths;
use roster_core::status::{Overview, SignInWatch, overview};
use roster_core::usage::{read_health, refresh_all, usage_runner};
use roster_core::view::{
    ChangeView, FirstRunView, OverviewView, changes_view, first_run_view, now, overview_view,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub const NO_ROSTER_BIN: &str = "Agent Roster's `roster` command wasn't found. If you opened the app from the downloaded disk image, drag it to Applications and open it from there. Otherwise reinstall Agent Roster.";
pub const STALE: &str =
    "Your roster changed since this preview. Look at the new preview before applying.";
const MAX_PENDING: usize = 16;
pub const EXPIRED: &str = "That preview has expired. Look at it again.";
/// How often the app asks every tool for usage on its own.
pub const USAGE_EVERY: Duration = Duration::from_secs(5 * 60);
/// The shortest time between two usage checks (Refresh clicked again and again).
pub const USAGE_GAP: Duration = Duration::from_secs(30);

/// One usage check at a time, and at most one per `USAGE_GAP`.
#[derive(Default)]
pub struct UsageGate {
    state: Mutex<GateState>,
}

#[derive(Default)]
struct GateState {
    running: bool,
    last: Option<Instant>,
}

impl UsageGate {
    /// Starts a check, unless one is running or one started less than `USAGE_GAP` ago.
    /// `urgent` (an agent was never checked) skips the wait, not a running check.
    pub fn begin(&self, now: Instant, urgent: bool) -> bool {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let recent = s
            .last
            .is_some_and(|t| now.saturating_duration_since(t) < USAGE_GAP);
        if s.running || (recent && !urgent) {
            return false;
        }
        s.running = true;
        s.last = Some(now);
        true
    }

    pub fn end(&self) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).running = false;
    }
}

/// The list shows an agent as "checking": a usage check should run soon. One finished check
/// ends "checking" for good (`view::shown_health`), so this never asks over and over.
pub fn needs_check(v: &OverviewView) -> bool {
    v.agents.iter().any(|a| a.health == "checking")
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
pub enum OpRequest {
    Add {
        name: String,
        tool: Tool,
        color: Option<AgentColor>,
        role: Option<String>,
        share: bool,
    },
    AdoptAll {
        picks: Vec<(Tool, String)>,
    },
    Import,
    EnableLimits {
        name: String,
    },
    DisableLimits {
        name: String,
    },
    Rename {
        name: String,
        new_name: String,
    },
    Color {
        name: String,
        color: AgentColor,
    },
    Role {
        name: String,
        role: Option<String>,
    },
    Share {
        name: String,
        on: bool,
    },
    Default {
        name: String,
    },
    GoodAt {
        name: String,
        items: Option<Vec<String>>,
    },
    /// The window only removes from the roster; the login folder always stays.
    Remove {
        name: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct StepView {
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanView {
    pub token: u64,
    pub summary: String,
    pub steps: Vec<StepView>,
}

type Env = Box<dyn Fn(&str) -> Option<String> + Send + Sync>;
type Opener = Box<dyn Fn(&[String]) -> std::io::Result<()> + Send + Sync>;

pub struct BackendConfig {
    pub p: RosterPaths,
    pub roster_bin: Option<PathBuf>,
    pub scratch: PathBuf,
    /// Folders holding macOS app bundles, searched for terminal apps.
    pub app_dirs: Vec<PathBuf>,
    pub path_var: String,
    pub lookup: Box<dyn CommandLookup + Send + Sync>,
    pub env: Env,
    pub opener: Opener,
}

struct Pending {
    plan: Plan,
    roster_sha: String,
}

pub struct Backend {
    pub p: RosterPaths,
    roster_bin: Option<PathBuf>,
    scratch: PathBuf,
    app_dirs: Vec<PathBuf>,
    path_var: String,
    lookup: Box<dyn CommandLookup + Send + Sync>,
    env: Env,
    opener: Opener,
    pending: Mutex<HashMap<u64, Pending>>,
    next: AtomicU64,
    /// One change or undo at a time.
    writing: Mutex<()>,
    usage_gate: UsageGate,
    /// Set by `overview` when a row says "checking" or a sign-in from the app waits for a
    /// check; the usage loop picks it up.
    wanted: AtomicBool,
    /// When the overview saw each agent sign in, and the Sign in clicks still waiting.
    watch: Mutex<SignInWatch>,
}

fn kind(k: StepKind) -> &'static str {
    match k {
        StepKind::Add => "add",
        StepKind::Change => "change",
        StepKind::Remove => "remove",
        StepKind::Note => "note",
    }
}

impl Backend {
    pub fn new(c: BackendConfig) -> Backend {
        Backend {
            p: c.p,
            roster_bin: c.roster_bin,
            scratch: c.scratch,
            app_dirs: c.app_dirs,
            path_var: c.path_var,
            lookup: c.lookup,
            env: c.env,
            opener: c.opener,
            pending: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            writing: Mutex::new(()),
            usage_gate: UsageGate::default(),
            wanted: AtomicBool::new(false),
            watch: Mutex::new(SignInWatch::default()),
        }
    }

    fn roster_bin(&self) -> Result<PathBuf, String> {
        self.roster_bin.clone().ok_or_else(|| NO_ROSTER_BIN.into())
    }

    pub(crate) fn ctx(&self) -> Ctx<'_> {
        Ctx {
            lookup: &*self.lookup,
            path_var: self.path_var.clone(),
            roster_bin: self.roster_bin.clone().unwrap_or_default(),
        }
    }

    fn roster(&self) -> Result<Roster, String> {
        Roster::load(&self.p).map_err(|e| e.to_string())
    }

    fn journal(&self) -> Result<Journal, String> {
        Journal::open(&self.p).map_err(|e| e.to_string())
    }

    fn roster_sha(&self) -> String {
        std::fs::read(self.p.roster_file())
            .map(|b| hex::encode(Sha256::digest(&b)))
            .unwrap_or_default()
    }

    pub fn overview(&self) -> Result<OverviewView, String> {
        let r = self.roster()?;
        let o = overview(&r, &self.p, &self.path_var, &RealRunner::default());
        Ok(self.view_of(o, now()))
    }

    /// The window's view of `o`; asks the usage loop for a check when a row says "checking"
    /// or a sign-in started here waits for one.
    pub(crate) fn view_of(&self, mut o: Overview, now: i64) -> OverviewView {
        let awaiting = {
            let mut w = self.watch.lock().unwrap_or_else(|e| e.into_inner());
            w.observe(&mut o, now);
            w.awaiting_sign_in(&o, now)
        };
        let v = overview_view(&o, &self.p.home, now);
        if needs_check(&v) || awaiting {
            self.wanted.store(true, Ordering::Relaxed);
        }
        v
    }

    /// Asks every tool for usage and account health, and saves the answers where
    /// `overview` reads them. Slow (seconds): never call it from the 10-second poll.
    /// Returns false when skipped: a check is running or one ran less than 30 s ago.
    pub fn refresh_usage(&self) -> Result<bool, String> {
        let r = self.roster()?;
        let urgent = r
            .agents
            .iter()
            .any(|a| read_health(&self.p, &a.id).is_none());
        if !self.usage_gate.begin(Instant::now(), urgent) {
            return Ok(false);
        }
        struct End<'a>(&'a UsageGate);
        impl Drop for End<'_> {
            fn drop(&mut self) {
                self.0.end();
            }
        }
        let _end = End(&self.usage_gate);
        refresh_all(&r, &self.p, &self.path_var, &usage_runner(&self.p));
        Ok(true)
    }

    /// True once after `overview` showed a row as "checking".
    pub fn take_wanted(&self) -> bool {
        self.wanted.swap(false, Ordering::Relaxed)
    }

    pub fn first_run(&self) -> Result<FirstRunView, String> {
        let r = self.roster()?;
        Ok(first_run_view(
            &r,
            &self.p,
            &self.path_var,
            &RealRunner::default(),
        ))
    }

    pub fn preview(&self, req: OpRequest) -> Result<PlanView, String> {
        self.roster_bin()?;
        // Hash before reading: a change made while planning must make this preview stale.
        let roster_sha = self.roster_sha();
        let r = self.roster()?;
        let ctx = self.ctx();
        let p = &self.p;
        let plan = match req {
            OpRequest::Add {
                name,
                tool,
                color,
                role,
                share,
            } => ops::plan_add(
                &r,
                p,
                &ctx,
                AddRequest {
                    name,
                    tool,
                    color,
                    role: role.filter(|s| !s.trim().is_empty()),
                    share,
                },
            ),
            OpRequest::AdoptAll { picks } => ops::plan_adopt_all(&r, p, &ctx, &picks),
            OpRequest::Import => plan_import(&r, p, &ctx),
            OpRequest::EnableLimits { name } => ops::plan_enable_claude_limits(&r, p, &ctx, &name),
            OpRequest::DisableLimits { name } => {
                ops::plan_disable_claude_limits(&r, p, &ctx, &name)
            }
            OpRequest::Rename { name, new_name } => {
                ops::plan_rename(&r, p, &ctx, &name, new_name.trim())
            }
            OpRequest::Color { name, color } => ops::plan_set_color(&r, p, &ctx, &name, color),
            OpRequest::Role { name, role } => ops::plan_set_role(&r, p, &ctx, &name, role),
            OpRequest::Share { name, on } => ops::plan_set_share(&r, p, &ctx, &name, on),
            OpRequest::Default { name } => ops::plan_set_default(&r, p, &ctx, &name),
            OpRequest::GoodAt { name, items } => ops::plan_set_good_at(&r, p, &ctx, &name, items),
            OpRequest::Remove { name } => ops::plan_remove(&r, p, &ctx, &name, false),
        }
        .map_err(|e| e.to_string())?;
        let token = self.next.fetch_add(1, Ordering::Relaxed);
        let view = PlanView {
            token,
            summary: plan.summary.clone(),
            steps: plan
                .steps
                .iter()
                .map(|s| StepView {
                    kind: kind(s.kind).into(),
                    text: s.text.clone(),
                })
                .collect(),
        };
        let mut pending = self.pending.lock().map_err(|e| e.to_string())?;
        pending.insert(token, Pending { plan, roster_sha });
        // A preview left open keeps being redone; keep only the newest few.
        while pending.len() > MAX_PENDING {
            let oldest = *pending.keys().min().expect("not empty");
            pending.remove(&oldest);
        }
        Ok(view)
    }

    pub fn apply(&self, token: u64) -> Result<Option<u64>, String> {
        let _w = self.writing.lock().map_err(|e| e.to_string())?;
        let pending = self
            .pending
            .lock()
            .map_err(|e| e.to_string())?
            .remove(&token)
            .ok_or(EXPIRED)?;
        if pending.roster_sha != self.roster_sha() {
            return Err(STALE.into());
        }
        let mut j = self.journal()?;
        ops::apply(pending.plan, &self.p, &mut j, &self.ctx()).map_err(|e| e.to_string())
    }

    pub fn changes(&self) -> Result<Vec<ChangeView>, String> {
        let r = self.roster()?;
        Ok(changes_view(self.journal()?.ops(), &r))
    }

    pub fn undo(&self, id: u64) -> Result<(), String> {
        let _w = self.writing.lock().map_err(|e| e.to_string())?;
        self.journal()?.undo(id).map_err(|e| e.to_string())
    }

    pub fn undo_all(&self) -> Result<usize, String> {
        let _w = self.writing.lock().map_err(|e| e.to_string())?;
        self.journal()?
            .undo_all()
            .map(|v| v.len())
            .map_err(|e| e.to_string())
    }

    /// Terminal apps found on this computer, in menu order.
    pub fn terminals(&self) -> Vec<TerminalApp> {
        let exists = |bundle: &str| self.app_dirs.iter().any(|d| d.join(bundle).is_dir());
        find_terminals(self.p.platform, &exists, &*self.lookup)
    }

    /// Runs `argv` in a terminal: the default one, or a found app. Returns false when the app
    /// can't run a command, so it was only opened and the person pastes the command themselves.
    fn open_in_terminal(&self, argv: Vec<String>, app: Option<&str>) -> Result<bool, String> {
        let chosen = match app {
            None => None,
            Some(id) => Some(
                self.terminals()
                    .into_iter()
                    .find(|t| t.id == id)
                    .ok_or_else(|| format!("{id} wasn't found on this computer."))?,
            ),
        };
        if let Some(t) = chosen.as_ref().filter(|t| !t.runs) {
            (self.opener)(&app_open_command(self.p.platform, &t.id))
                .map_err(|e| format!("Couldn't open {}: {e}", t.label))?;
            return Ok(false);
        }
        let cmd = terminal_command_in(
            self.p.platform,
            chosen.as_ref().map(|t| t.id.as_str()),
            &*self.env,
            &*self.lookup,
            &argv,
            &self.scratch,
        )
        .map_err(|e| e.to_string())?;
        (self.opener)(&cmd).map_err(|e| format!("Couldn't open a terminal: {e}"))?;
        Ok(true)
    }

    fn agent_id(&self, name: &str) -> Result<String, String> {
        self.roster()?
            .by_name(name)
            .map(|a| a.id.clone())
            .ok_or_else(|| format!("No agent is named {name}."))
    }

    /// Opens a terminal running `roster login <name>`. Until a check says the sign-in worked
    /// (at most `SIGN_IN_WAIT`), the usage loop keeps checking, 30 s apart.
    pub fn sign_in(&self, name: &str, app: Option<&str>) -> Result<bool, String> {
        let bin = self.roster_bin()?;
        let id = self.agent_id(name)?;
        let opened = self.open_in_terminal(
            vec![bin.display().to_string(), "login".into(), name.into()],
            app,
        )?;
        self.watch
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .sign_in_started(&id, now());
        Ok(opened)
    }

    /// Opens a terminal running the agent itself.
    pub fn open_terminal_as(&self, name: &str, app: Option<&str>) -> Result<bool, String> {
        let bin = self.roster_bin()?;
        let id = self.agent_id(name)?;
        self.open_in_terminal(vec![bin.display().to_string(), "run".into(), id], app)
    }

    /// The author's site, from the credit line. The address is fixed here, never passed in.
    pub fn open_site(&self) -> Result<(), String> {
        (self.opener)(&url_open_command(self.p.platform, SITE)).map_err(|e| e.to_string())
    }
}

pub const SITE: &str = "https://mahadihassan.com";

#[cfg(test)]
#[path = "backend_tests.rs"]
mod tests;
