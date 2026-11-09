//! Finds Claude Code and Codex, and asks each tool whether an agent is signed in.

use crate::model::{Agent, Roster, Tool};
use crate::paths::RosterPaths;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// Runs a program. Injected so tests never call the real tools.
pub trait CommandRunner {
    /// `env` entries with `None` are removed from the child's environment.
    fn run(&self, prog: &Path, args: &[&str], env: &[(String, Option<String>)]) -> std::io::Result<Output>;
}

pub struct RealRunner;

impl CommandRunner for RealRunner {
    fn run(&self, prog: &Path, args: &[&str], env: &[(String, Option<String>)]) -> std::io::Result<Output> {
        let mut c = Command::new(prog);
        c.args(args).stdin(Stdio::null());
        for (k, v) in env {
            match v {
                Some(v) => c.env(k, v),
                None => c.env_remove(k),
            };
        }
        c.output()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolInfo {
    pub tool: Tool,
    pub path: Option<PathBuf>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SignIn {
    pub signed_in: bool,
    pub email: Option<String>,
    pub plan: Option<String>,
}

/// Finds the real tool on PATH, skipping Agent Roster's own launcher folder.
pub fn find_tool(tool: Tool, path_var: &str, exclude: &Path) -> Option<PathBuf> {
    let dirs: Vec<PathBuf> = std::env::split_paths(path_var).filter(|d| d != exclude).collect();
    let joined = std::env::join_paths(dirs).ok()?;
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    which::which_in(tool.binary(), Some(joined), cwd).ok()
}

pub fn parse_version(tool: Tool, stdout: &str) -> Option<String> {
    let line = stdout.lines().next()?.trim();
    let token = match tool {
        Tool::Claude => line.split_whitespace().next(),
        Tool::Codex => line.split_whitespace().last(),
    }?;
    token.chars().next().filter(|c| c.is_ascii_digit()).map(|_| token.to_string())
}

pub fn tool_info(tool: Tool, path_var: &str, exclude: &Path, runner: &dyn CommandRunner) -> ToolInfo {
    let path = find_tool(tool, path_var, exclude);
    let version = path.as_ref().and_then(|p| {
        let out = runner.run(p, &["--version"], &[("CLAUDECODE".into(), None)]).ok()?;
        parse_version(tool, &String::from_utf8_lossy(&out.stdout))
    });
    ToolInfo { tool, path, version }
}

/// Reads `claude auth status --json`. Never touches token files.
pub fn parse_claude_status(json: &str) -> SignIn {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return SignIn::default();
    };
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(String::from);
    SignIn {
        signed_in: v.get("loggedIn").and_then(|x| x.as_bool()).unwrap_or(false),
        email: s("email"),
        plan: s("subscriptionType"),
    }
}

/// Reads `codex login status`. Codex reports no email here.
pub fn parse_codex_status(stdout: &str, success: bool) -> SignIn {
    SignIn { signed_in: success && stdout.trim_start().starts_with("Logged in"), email: None, plan: None }
}

pub fn sign_in_for(agent: &Agent, r: &Roster, p: &RosterPaths, tool_path: &Path, runner: &dyn CommandRunner) -> SignIn {
    let env = crate::run::build_env(agent, r, p, None);
    match agent.tool {
        Tool::Claude => match runner.run(tool_path, &["auth", "status", "--json"], &env) {
            Ok(out) => parse_claude_status(&String::from_utf8_lossy(&out.stdout)),
            Err(_) => SignIn::default(),
        },
        Tool::Codex => match runner.run(tool_path, &["login", "status"], &env) {
            Ok(out) => {
                let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
                parse_codex_status(&text, out.status.success())
            }
            Err(_) => SignIn::default(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Roster, Tool};
    use crate::note::tests::agent;
    use crate::paths::{Platform, RosterPaths};
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};
    use std::process::Output;

    #[test]
    fn claude_status_signed_in() {
        let s = parse_claude_status(include_str!("../tests/fixtures/claude-auth-status.json"));
        assert!(s.signed_in);
        assert_eq!(s.email.as_deref(), Some("someone@example.com"));
        assert_eq!(s.plan.as_deref(), Some("max"));
    }

    #[test]
    fn claude_status_garbage_is_signed_out() {
        assert!(!parse_claude_status("not json").signed_in);
        assert!(!parse_claude_status("{\"loggedIn\": false}").signed_in);
    }

    #[test]
    fn codex_status() {
        assert!(parse_codex_status(include_str!("../tests/fixtures/codex-login-status.txt"), true).signed_in);
        assert!(!parse_codex_status("Not logged in\n", false).signed_in);
    }

    #[test]
    fn versions() {
        assert_eq!(parse_version(Tool::Claude, "2.1.319 (Claude Code)\n").as_deref(), Some("2.1.319"));
        assert_eq!(parse_version(Tool::Codex, "codex-cli 0.167.0\n").as_deref(), Some("0.167.0"));
    }

    #[cfg(unix)]
    #[test]
    fn find_tool_skips_our_bin() {
        let d = tempfile::tempdir().unwrap();
        let ours = d.path().join("ours");
        let real = d.path().join("real");
        for dir in [&ours, &real] {
            std::fs::create_dir_all(dir).unwrap();
            let f = dir.join("claude");
            std::fs::write(&f, "#!/bin/sh\n").unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path_var = std::env::join_paths([&ours, &real]).unwrap();
        assert_eq!(find_tool(Tool::Claude, &path_var.to_string_lossy(), &ours), Some(real.join("claude")));
    }

    struct FakeRunner {
        seen: RefCell<Vec<Vec<(String, Option<String>)>>>,
        stdout: String,
    }
    impl CommandRunner for FakeRunner {
        fn run(&self, _prog: &Path, _args: &[&str], env: &[(String, Option<String>)]) -> std::io::Result<Output> {
            self.seen.borrow_mut().push(env.to_vec());
            #[cfg(unix)]
            let status = std::os::unix::process::ExitStatusExt::from_raw(0);
            #[cfg(windows)]
            let status = std::os::windows::process::ExitStatusExt::from_raw(0);
            Ok(Output { status, stdout: self.stdout.clone().into_bytes(), stderr: vec![] })
        }
    }

    #[test]
    fn sign_in_uses_agent_env() {
        let d = tempfile::tempdir().unwrap();
        let p = RosterPaths::for_home(d.path(), Platform::Mac, &|_| None);
        let r = Roster {
            version: 1,
            agents: vec![
                agent("a1", "agent1", Tool::Claude, "~/.claude", None),
                agent("w1", "worker1", Tool::Claude, "~/.agent-roster/accounts/w1", None),
            ],
            statusline: vec![],
        };
        let runner = FakeRunner { seen: RefCell::new(vec![]), stdout: include_str!("../tests/fixtures/claude-auth-status.json").into() };
        let tool = PathBuf::from("/x/claude");
        let s = sign_in_for(&r.agents[1], &r, &p, &tool, &runner);
        assert!(s.signed_in);
        sign_in_for(&r.agents[0], &r, &p, &tool, &runner);
        let seen = runner.seen.borrow();
        let dir = d.path().join(".agent-roster/accounts/w1").display().to_string();
        assert!(seen[0].contains(&("CLAUDE_CONFIG_DIR".to_string(), Some(dir))));
        assert!(seen[1].contains(&("CLAUDE_CONFIG_DIR".to_string(), None)));
    }
}
