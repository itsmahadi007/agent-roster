//! The app's bridge to roster-core. Every change is previewed first; the exact plan the
//! person saw is the one applied, and only if the roster file has not changed meanwhile.

use roster_core::desktop::terminal_command;
use roster_core::detect::RealRunner;
use roster_core::import::plan_import;
use roster_core::journal::Journal;
use roster_core::model::{AgentColor, Roster, Tool};
use roster_core::names::CommandLookup;
use roster_core::ops::{self, AddRequest, Ctx, Plan, StepKind};
use roster_core::paths::RosterPaths;
use roster_core::status::overview;
use roster_core::view::{
    ChangeView, FirstRunView, OverviewView, changes_view, first_run_view, now, overview_view,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

pub const NO_ROSTER_BIN: &str =
    "Agent Roster's `roster` command wasn't found. Install it, then open the app again.";
pub const STALE: &str =
    "Your roster changed since this preview. Look at the new preview before applying.";
pub const EXPIRED: &str = "That preview has expired. Look at it again.";

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
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
    path_var: String,
    lookup: Box<dyn CommandLookup + Send + Sync>,
    env: Env,
    opener: Opener,
    pending: Mutex<HashMap<u64, Pending>>,
    next: AtomicU64,
    /// One change or undo at a time.
    writing: Mutex<()>,
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
            path_var: c.path_var,
            lookup: c.lookup,
            env: c.env,
            opener: c.opener,
            pending: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            writing: Mutex::new(()),
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
        Ok(overview_view(&o, &self.p.home, now()))
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
        let roster_sha = self.roster_sha();
        let mut pending = self.pending.lock().map_err(|e| e.to_string())?;
        pending.insert(token, Pending { plan, roster_sha });
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

    fn open_in_terminal(&self, argv: Vec<String>) -> Result<(), String> {
        let cmd = terminal_command(
            self.p.platform,
            &*self.env,
            &*self.lookup,
            &argv,
            &self.scratch,
        )
        .map_err(|e| e.to_string())?;
        (self.opener)(&cmd).map_err(|e| format!("Couldn't open a terminal: {e}"))
    }

    fn agent_id(&self, name: &str) -> Result<String, String> {
        self.roster()?
            .by_name(name)
            .map(|a| a.id.clone())
            .ok_or_else(|| format!("No agent is named {name}."))
    }

    /// Opens the person's terminal running `roster login <name>`.
    pub fn sign_in(&self, name: &str) -> Result<(), String> {
        let bin = self.roster_bin()?;
        self.agent_id(name)?;
        self.open_in_terminal(vec![bin.display().to_string(), "login".into(), name.into()])
    }

    /// Opens the person's terminal running the agent itself.
    pub fn open_terminal_as(&self, name: &str) -> Result<(), String> {
        let bin = self.roster_bin()?;
        let id = self.agent_id(name)?;
        self.open_in_terminal(vec![bin.display().to_string(), "run".into(), id])
    }
}

#[cfg(test)]
#[path = "backend_tests.rs"]
mod tests;
