//! Starting an agent: the environment it runs with, and `roster run`.

use crate::model::{Agent, Roster};
use crate::paths::RosterPaths;

pub type EnvChanges = Vec<(String, Option<String>)>;

use crate::detect::find_tool;
use crate::limits::{format_reset, read_claude_limits, read_codex_limits};
use crate::model::Tool;
use crate::session::{SessionRecord, task_from_args};
use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum RunError {
    Roster(crate::model::ModelError),
    UnknownAgent(String),
    ToolMissing(Tool),
    Io(std::io::Error),
}
impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunError::Roster(e) => write!(f, "{e}"),
            RunError::UnknownAgent(id) => write!(f, "No agent on the roster has the id {id}. Run `roster doctor` to repair launchers."),
            RunError::ToolMissing(t) => write!(f, "{} isn't installed on this computer, or isn't on PATH.", t.display()),
            RunError::Io(e) => write!(f, "{e}"),
        }
    }
}
impl std::error::Error for RunError {}

/// Everything needed to start an agent, decided before anything runs.
#[derive(Debug, Clone)]
pub struct RunPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: EnvChanges,
    pub banner: Option<String>,
    pub title: Option<String>,
    pub record: SessionRecord,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn prepare_run(
    id: &str,
    args: Vec<String>,
    p: &RosterPaths,
    env: &dyn Fn(&str) -> Option<String>,
    interactive: bool,
) -> Result<RunPlan, RunError> {
    let r = Roster::load(p).map_err(RunError::Roster)?;
    let agent = r.by_id(id).ok_or_else(|| RunError::UnknownAgent(id.to_string()))?;
    let path_var = env("PATH").unwrap_or_default();
    let program = find_tool(agent.tool, &path_var, &p.bin_dir()).ok_or(RunError::ToolMissing(agent.tool))?;
    let parent = env("ROSTER_AGENT").filter(|x| !x.is_empty() && x != id);
    let env_changes = build_env(agent, &r, p, parent.as_deref());
    let record = SessionRecord {
        pid: std::process::id(),
        agent_id: agent.id.clone(),
        parent_id: parent,
        cwd: std::env::current_dir().map(|d| d.display().to_string()).unwrap_or_default(),
        task: task_from_args(agent.tool, &args),
        started_at: now(),
    };
    let (banner, title) = if interactive {
        let color = env("NO_COLOR").is_none();
        (Some(banner_text(agent, &r, p, color)), Some(format!("{} \u{2014} {}", agent.name, agent.tool.display())))
    } else {
        (None, None)
    };
    Ok(RunPlan { program, args, env: env_changes, banner, title, record })
}

fn banner_text(agent: &Agent, r: &Roster, p: &RosterPaths, color: bool) -> String {
    let limits = match agent.tool {
        Tool::Codex => read_codex_limits(&r.config_dir_of(agent, p)),
        Tool::Claude => read_claude_limits(p, &agent.id),
    };
    let limit_text = limits
        .and_then(|l| l.windows.into_iter().max_by_key(|w| if w.label == "week" { 1 } else { 0 }))
        .map(|w| format!("  {} {:.0}%, resets in {}", w.label, w.used_percent, format_reset(w.resets_at, now())))
        .unwrap_or_default();
    if color {
        let (cr, cg, cb) = agent.color.rgb();
        format!(
            "\u{1b}[1;38;2;14;21;31;48;2;{cr};{cg};{cb}m {} \u{1b}[0m \u{1b}[38;2;159;179;200m{}\u{1b}[0m\u{1b}[2m{}\u{1b}[0m\n",
            agent.name,
            agent.tool.display(),
            limit_text
        )
    } else {
        format!("[{}] {}{}\n", agent.name, agent.tool.display(), limit_text)
    }
}

/// Starts the agent. On macOS/Linux the tool replaces this process, keeping its pid,
/// so the session record stays valid exactly as long as the tool runs.
pub fn exec_run(plan: RunPlan, p: &RosterPaths) -> Result<i32, RunError> {
    use std::io::Write;
    if let Some(b) = &plan.banner {
        let _ = std::io::stderr().write_all(b.as_bytes());
    }
    if let Some(t) = &plan.title {
        let _ = std::io::stderr().write_all(format!("\u{1b}]0;{t}\u{7}").as_bytes());
    }
    let tag = plan.record.agent_id.clone();
    let record_path = crate::session::start(p, &plan.record, &tag).ok();
    let mut cmd = std::process::Command::new(&plan.program);
    cmd.args(&plan.args);
    for (k, v) in &plan.env {
        match v {
            Some(v) => cmd.env(k, v),
            None => cmd.env_remove(k),
        };
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = cmd.exec();
        if let Some(rp) = record_path {
            crate::session::finish(&rp);
        }
        Err(RunError::Io(err))
    }
    #[cfg(windows)]
    {
        let mut child = cmd.spawn().map_err(RunError::Io)?;
        unsafe {
            windows_sys::Win32::System::Console::SetConsoleCtrlHandler(None, 1);
        }
        let status = child.wait().map_err(RunError::Io)?;
        if let Some(rp) = record_path {
            crate::session::finish(&rp);
        }
        Ok(status.code().unwrap_or(1))
    }
}

/// The environment changes for running `agent`. `None` means "remove this variable".
pub fn build_env(agent: &Agent, r: &Roster, p: &RosterPaths, parent_agent: Option<&str>) -> EnvChanges {
    let dir = if r.is_adopted_default(agent, p) {
        None
    } else {
        Some(r.config_dir_of(agent, p).display().to_string())
    };
    vec![
        (agent.tool.dir_var().to_string(), dir),
        ("CLAUDECODE".to_string(), None),
        ("ROSTER_AGENT".to_string(), Some(agent.id.clone())),
        ("ROSTER_PARENT".to_string(), parent_agent.map(String::from)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Roster, Tool};
    use crate::note::tests::agent;
    use crate::paths::{Platform, RosterPaths};

    fn setup() -> (tempfile::TempDir, RosterPaths, Roster) {
        let d = tempfile::tempdir().unwrap();
        let p = RosterPaths::for_home(d.path(), Platform::Mac, &|_| None);
        let r = Roster {
            version: 1,
            agents: vec![
                agent("k1", "worker1", Tool::Claude, "~/.agent-roster/accounts/k1", None),
                agent("c3", "agent3", Tool::Codex, "~/.codex", None),
            ],
            statusline: vec![],
        };
        (d, p, r)
    }

    #[test]
    fn env_secondary_sets_dir() {
        let (_d, p, r) = setup();
        let e = build_env(&r.agents[0], &r, &p, None);
        let dir = p.home.join(".agent-roster/accounts/k1").display().to_string();
        assert!(e.contains(&("CLAUDE_CONFIG_DIR".into(), Some(dir))));
        assert!(e.contains(&("CLAUDECODE".into(), None)));
        assert!(e.contains(&("ROSTER_AGENT".into(), Some("k1".into()))));
    }

    #[test]
    fn env_adopted_default_unsets_dir() {
        let (_d, p, r) = setup();
        assert!(build_env(&r.agents[1], &r, &p, None).contains(&("CODEX_HOME".into(), None)));
    }

    #[test]
    fn env_records_parent() {
        let (_d, p, r) = setup();
        assert!(build_env(&r.agents[1], &r, &p, Some("k1")).contains(&("ROSTER_PARENT".into(), Some("k1".into()))));
        assert!(build_env(&r.agents[1], &r, &p, None).contains(&("ROSTER_PARENT".into(), None)));
    }

    fn fake_tool_dir(d: &std::path::Path, name: &str) -> std::path::PathBuf {
        let dir = d.join("tools");
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join(if cfg!(windows) { format!("{name}.exe") } else { name.to_string() });
        std::fs::write(&f, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        dir
    }

    fn saved(p: &RosterPaths, r: &Roster) {
        r.save(p).unwrap();
    }

    #[test]
    fn prepare_run_secondary_piped() {
        let (d, p, r) = setup();
        saved(&p, &r);
        let tools = fake_tool_dir(d.path(), "claude");
        let path_var = tools.display().to_string();
        let env = |k: &str| match k {
            "PATH" => Some(path_var.clone()),
            "ROSTER_AGENT" => Some("c3".to_string()),
            _ => None,
        };
        let plan = prepare_run("k1", vec!["-p".into(), "write tests".into()], &p, &env, false).unwrap();
        assert_eq!(plan.program, tools.join("claude"));
        assert_eq!(plan.args, vec!["-p".to_string(), "write tests".to_string()]);
        assert!(plan.env.contains(&("ROSTER_PARENT".into(), Some("c3".into()))));
        assert_eq!(plan.record.parent_id.as_deref(), Some("c3"));
        assert_eq!(plan.record.task.as_deref(), Some("write tests"));
        assert!(plan.banner.is_none(), "no banner when piped");
    }

    #[test]
    fn prepare_run_interactive_banner() {
        let (d, p, r) = setup();
        saved(&p, &r);
        let tools = fake_tool_dir(d.path(), "codex");
        let path_var = tools.display().to_string();
        let env = |k: &str| (k == "PATH").then(|| path_var.clone());
        let plan = prepare_run("c3", vec![], &p, &env, true).unwrap();
        let b = plan.banner.unwrap();
        assert!(b.contains("agent3") && b.contains("Codex"));
        let no_color = |k: &str| match k {
            "PATH" => Some(path_var.clone()),
            "NO_COLOR" => Some("1".into()),
            _ => None,
        };
        assert!(!prepare_run("c3", vec![], &p, &no_color, true).unwrap().banner.unwrap().contains('\u{1b}'));
    }

    #[test]
    fn prepare_run_errors() {
        let (_d, p, r) = setup();
        saved(&p, &r);
        let env = |k: &str| (k == "PATH").then(|| "/nonexistent".to_string());
        assert!(matches!(prepare_run("zz", vec![], &p, &env, false), Err(RunError::UnknownAgent(_))));
        assert!(matches!(prepare_run("c3", vec![], &p, &env, false), Err(RunError::ToolMissing(Tool::Codex))));
    }
}
