//! Finds Claude Code and Codex, and asks each tool whether an agent is signed in.

use crate::model::{Agent, Roster, Tool};
use crate::paths::RosterPaths;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// Runs a program. Injected so tests never call the real tools.
pub trait CommandRunner {
    /// `env` entries with `None` are removed from the child's environment.
    fn run(
        &self,
        prog: &Path,
        args: &[&str],
        env: &[(String, Option<String>)],
    ) -> std::io::Result<Output>;
}

pub struct RealRunner {
    pub timeout: std::time::Duration,
    /// Folder the program runs in. `None` keeps this process's folder.
    pub cwd: Option<PathBuf>,
}

impl Default for RealRunner {
    fn default() -> Self {
        RealRunner {
            timeout: std::time::Duration::from_secs(5),
            cwd: None,
        }
    }
}

impl CommandRunner for RealRunner {
    fn run(
        &self,
        prog: &Path,
        args: &[&str],
        env: &[(String, Option<String>)],
    ) -> std::io::Result<Output> {
        // Runs with a time limit, so a stuck tool can't hang the overview. The child goes in
        // the platform's container (a kill-on-close Job Object on Windows, a session of its
        // own elsewhere), so stopping it stops what it started too: npm's `claude.cmd` is
        // `cmd.exe` running `node`, and killing only `cmd.exe` would leave `node` running.
        use crate::usage::sys;
        use std::io::Read;
        let mut c = Command::new(prog);
        crate::proc::hide_console(&mut c)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = &self.cwd {
            c.current_dir(dir);
        }
        for (k, v) in env {
            match v {
                Some(v) => c.env(k, v),
                None => c.env_remove(k),
            };
        }
        let mut child = sys::spawn_contained(&mut c)?;
        let mut out = child.stdout.take().expect("piped");
        let mut err = child.stderr.take().expect("piped");
        let out_reader = std::thread::spawn(move || {
            let mut b = vec![];
            let _ = out.read_to_end(&mut b);
            b
        });
        let err_reader = std::thread::spawn(move || {
            let mut b = vec![];
            let _ = err.read_to_end(&mut b);
            b
        });
        let deadline = std::time::Instant::now() + self.timeout;
        let status = loop {
            let waited = match child.try_wait() {
                Ok(st) => st,
                Err(e) => {
                    sys::kill_tree(&mut child);
                    return Err(e);
                }
            };
            if let Some(st) = waited {
                break st;
            }
            if std::time::Instant::now() >= deadline {
                sys::kill_tree(&mut child);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("{} did not answer in time", prog.display()),
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        };
        // Stop anything the tool left behind: it could hold the pipes open, and on Windows
        // this also frees the child's job.
        sys::kill_tree(&mut child);
        Ok(Output {
            status,
            stdout: out_reader.join().unwrap_or_default(),
            stderr: err_reader.join().unwrap_or_default(),
        })
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

/// Finds the real tool on PATH, then in the folders tools usually install to (npm, WinGet,
/// Homebrew, ~/.local/bin), skipping Agent Roster's own launcher folder.
pub fn find_tool(tool: Tool, path_var: &str, p: &RosterPaths) -> Option<PathBuf> {
    find_tool_in(tool, path_var, &p.bin_dir(), &usual_tool_dirs(p))
}

/// The usual install folders for this person. `%APPDATA%` and `%LOCALAPPDATA%` are the
/// folders that hold Agent Roster's own config and data. The rest of the environment and the
/// folders shared by every user (Homebrew) count only for the real home, so a test with a
/// temp home never finds the machine's real tools.
fn usual_tool_dirs(p: &RosterPaths) -> Vec<PathBuf> {
    let real_home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .is_some_and(|h| Path::new(&h) == p.home);
    let parent = |d: &Path| d.parent().map(|x| x.as_os_str().to_owned());
    let env = |k: &str| match k {
        "APPDATA" => parent(&p.config_dir),
        "LOCALAPPDATA" => parent(&p.data_dir),
        _ if real_home => std::env::var_os(k),
        _ => None,
    };
    let mut dirs = crate::usage::sys::tool_dirs_with(&p.home, &env);
    if real_home {
        dirs.extend(crate::usage::sys::system_tool_dirs());
    }
    dirs
}

/// `find_tool` with the extra folders given.
pub fn find_tool_in(
    tool: Tool,
    path_var: &str,
    exclude: &Path,
    extra: &[PathBuf],
) -> Option<PathBuf> {
    let dirs: Vec<PathBuf> = std::env::split_paths(path_var)
        .chain(extra.iter().cloned())
        .filter(|d| d != exclude && !d.as_os_str().is_empty())
        .collect();
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
    token
        .chars()
        .next()
        .filter(|c| c.is_ascii_digit())
        .map(|_| token.to_string())
}

pub fn tool_info(
    tool: Tool,
    path_var: &str,
    paths: &RosterPaths,
    runner: &dyn CommandRunner,
) -> ToolInfo {
    let path = find_tool(tool, path_var, paths);
    let version = path.as_ref().and_then(|p| {
        let out = runner
            .run(p, &["--version"], &[("CLAUDECODE".into(), None)])
            .ok()?;
        parse_version(tool, &String::from_utf8_lossy(&out.stdout))
    });
    ToolInfo {
        tool,
        path,
        version,
    }
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
    SignIn {
        signed_in: success && stdout.trim_start().starts_with("Logged in"),
        email: None,
        plan: None,
    }
}

pub fn sign_in_for(
    agent: &Agent,
    r: &Roster,
    p: &RosterPaths,
    tool_path: &Path,
    runner: &dyn CommandRunner,
) -> SignIn {
    ask_sign_in(agent, r, p, tool_path, runner).unwrap_or_default()
}

/// Asks the tool whether an agent is signed in. `None` when the tool could not be asked
/// (timed out, crashed, printed something unreadable): that is not "signed out".
pub fn ask_sign_in(
    agent: &Agent,
    r: &Roster,
    p: &RosterPaths,
    tool_path: &Path,
    runner: &dyn CommandRunner,
) -> Option<SignIn> {
    let env = crate::run::build_env(agent, r, p, None);
    match agent.tool {
        Tool::Claude => {
            let out = runner
                .run(tool_path, &["auth", "status", "--json"], &env)
                .ok()?;
            let json = String::from_utf8_lossy(&out.stdout);
            crate::usage::claude_health(&json)?;
            Some(parse_claude_status(&json))
        }
        Tool::Codex => {
            let out = runner.run(tool_path, &["login", "status"], &env).ok()?;
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            Some(parse_codex_status(&text, out.status.success()))
        }
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
        assert!(
            parse_codex_status(
                include_str!("../tests/fixtures/codex-login-status.txt"),
                true
            )
            .signed_in
        );
        assert!(!parse_codex_status("Not logged in\n", false).signed_in);
    }

    #[test]
    fn versions() {
        assert_eq!(
            parse_version(Tool::Claude, "2.1.319 (Claude Code)\n").as_deref(),
            Some("2.1.319")
        );
        assert_eq!(
            parse_version(Tool::Codex, "codex-cli 0.167.0\n").as_deref(),
            Some("0.167.0")
        );
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
        assert_eq!(
            find_tool_in(Tool::Claude, &path_var.to_string_lossy(), &ours, &[]),
            Some(real.join("claude"))
        );
    }

    /// A runnable stand-in for `codex` in `dir`.
    fn fake_codex(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        #[cfg(windows)]
        let f = dir.join("codex.cmd");
        #[cfg(unix)]
        let f = dir.join("codex");
        std::fs::write(&f, "echo\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        f
    }

    #[test]
    fn find_tool_looks_in_usual_folders_missing_from_path() {
        // The app can start with a shorter PATH than a terminal (Explorer keeps the PATH from
        // sign-in, so a tool installed later with npm is missing): the usual install folders
        // are searched after PATH.
        let d = tempfile::tempdir().unwrap();
        let npm = d.path().join("npm");
        let in_npm = fake_codex(&npm);
        let ours = d.path().join("ours");
        assert_eq!(
            find_tool_in(Tool::Codex, "", &ours, std::slice::from_ref(&npm)),
            Some(in_npm)
        );
        // A copy on PATH wins over the usual folders.
        let on_path = fake_codex(&d.path().join("onpath"));
        let path_var = std::env::join_paths([on_path.parent().unwrap()]).unwrap();
        let found = find_tool_in(Tool::Codex, &path_var.to_string_lossy(), &ours, &[npm]);
        assert_eq!(found, Some(on_path));
    }

    #[test]
    fn a_roster_in_a_temp_home_only_searches_inside_it() {
        // Tests use a temp home: they must never find (and run) the machine's real tools in
        // Homebrew, /usr/local/bin or the real npm folder.
        let d = tempfile::tempdir().unwrap();
        let p = RosterPaths::for_home(d.path(), crate::paths::Platform::current(), &|_| None);
        let dirs = usual_tool_dirs(&p);
        assert!(!dirs.is_empty());
        for dir in dirs {
            assert!(dir.starts_with(d.path()), "{} is outside", dir.display());
        }
    }

    struct FakeRunner {
        seen: RefCell<Vec<crate::run::EnvChanges>>,
        stdout: String,
    }
    impl CommandRunner for FakeRunner {
        fn run(
            &self,
            _prog: &Path,
            _args: &[&str],
            env: &[(String, Option<String>)],
        ) -> std::io::Result<Output> {
            self.seen.borrow_mut().push(env.to_vec());
            #[cfg(unix)]
            let status = std::os::unix::process::ExitStatusExt::from_raw(0);
            #[cfg(windows)]
            let status = std::os::windows::process::ExitStatusExt::from_raw(0);
            Ok(Output {
                status,
                stdout: self.stdout.clone().into_bytes(),
                stderr: vec![],
            })
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
                agent(
                    "w1",
                    "worker1",
                    Tool::Claude,
                    "~/.agent-roster/accounts/w1",
                    None,
                ),
            ],
            statusline: vec![],
        };
        let runner = FakeRunner {
            seen: RefCell::new(vec![]),
            stdout: include_str!("../tests/fixtures/claude-auth-status.json").into(),
        };
        let tool = PathBuf::from("/x/claude");
        let s = sign_in_for(&r.agents[1], &r, &p, &tool, &runner);
        assert!(s.signed_in);
        sign_in_for(&r.agents[0], &r, &p, &tool, &runner);
        let seen = runner.seen.borrow();
        let dir = d
            .path()
            .join(".agent-roster/accounts/w1")
            .display()
            .to_string();
        assert!(seen[0].contains(&("CLAUDE_CONFIG_DIR".to_string(), Some(dir))));
        assert!(seen[1].contains(&("CLAUDE_CONFIG_DIR".to_string(), None)));
    }
}

#[cfg(all(test, unix))]
mod timeout_tests {
    use super::*;

    #[test]
    fn stuck_command_times_out() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("stuck");
        std::fs::write(&f, "#!/bin/sh\nsleep 10\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        let runner = RealRunner {
            timeout: std::time::Duration::from_millis(500),
            ..Default::default()
        };
        let start = std::time::Instant::now();
        assert!(runner.run(&f, &[], &[]).is_err());
        assert!(start.elapsed() < std::time::Duration::from_secs(3));
    }
}

#[cfg(test)]
mod tree_tests {
    use super::*;
    use std::time::Duration;

    /// A tool that starts a helper, which appends to `out` every 100 ms or so until a `stop`
    /// file appears, and then hangs. Run in `dir`.
    #[cfg(windows)]
    fn hanging_tool(dir: &Path) -> PathBuf {
        let f = dir.join("hang.cmd");
        std::fs::write(
            &f,
            "@echo off\r\n\
             start \"\" /b cmd /d /c \"for /l %%i in (0,0,1) do @((if exist stop exit) & echo x>>out & ping -n 1 -w 100 192.0.2.1 >nul)\"\r\n\
             ping -n 30 127.0.0.1 >nul\r\n",
        )
        .unwrap();
        f
    }

    #[cfg(unix)]
    fn hanging_tool(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let f = dir.join("hang");
        std::fs::write(
            &f,
            "#!/bin/sh\n( while [ ! -e stop ]; do echo x >> out; sleep 0.1; done ) &\nsleep 30\n",
        )
        .unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        f
    }

    /// Stops the helper even when the test fails, so it can't run on forever.
    struct Stop(PathBuf);
    impl Drop for Stop {
        fn drop(&mut self) {
            let _ = std::fs::write(&self.0, "");
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    /// `npm`'s `claude.cmd` is `cmd.exe` starting `node`: a timeout must stop the whole tree,
    /// not only the program it started.
    #[test]
    fn a_timeout_stops_everything_the_tool_started() {
        let d = tempfile::tempdir().unwrap();
        let tool = hanging_tool(d.path());
        let _stop = Stop(d.path().join("stop"));
        let runner = RealRunner {
            timeout: Duration::from_millis(2000),
            cwd: Some(d.path().to_path_buf()),
        };
        let err = runner.run(&tool, &[], &[]).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        let size = || {
            std::fs::metadata(d.path().join("out"))
                .map(|m| m.len())
                .unwrap_or(0)
        };
        std::thread::sleep(Duration::from_millis(300));
        let before = size();
        assert!(before > 0, "the helper never started");
        std::thread::sleep(Duration::from_millis(1200));
        assert_eq!(size(), before, "the helper is still running");
    }
}
