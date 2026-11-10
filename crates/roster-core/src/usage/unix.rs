//! macOS and Linux: how usage checks find, spawn and stop the tools.
//!
//! Every child goes into its own process group, so `kill_tree` can stop the child and
//! everything it started (`codex` from npm is a `node` script that starts the native binary).

use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// Shown in logs and `roster doctor`.
pub const PLATFORM: &str = "unix";

/// Puts the child in a new process group (there is no console window to hide).
pub fn hide(cmd: &mut Command) -> &mut Command {
    cmd.process_group(0)
}

/// Spawns in its own process group with stdin and stdout piped and stderr discarded.
pub fn spawn_piped(cmd: &mut Command) -> std::io::Result<Child> {
    hide(cmd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

/// Stops the child's whole process group, then waits for the child.
pub fn kill_tree(child: &mut Child) {
    // While the child is unreaped its id can't be reused, so the group is still ours.
    if child.try_wait().ok().flatten().is_none()
        && let Ok(pgid) = libc::pid_t::try_from(child.id())
    {
        // SAFETY: a plain signal to the group the child leads (see `hide`).
        unsafe {
            libc::kill(-pgid, libc::SIGKILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Folders where Claude Code and Codex usually live that the app's PATH may lack (an app
/// started from the Dock or a desktop menu does not get the shell's PATH).
pub fn extra_tool_dirs(home: &Path) -> Vec<PathBuf> {
    tool_dirs_with(home, &|k| std::env::var_os(k))
}

/// `extra_tool_dirs` with the environment passed in, so it can be tested.
/// `NPM_CONFIG_PREFIX` adds npm's global folder when it was moved.
pub fn tool_dirs_with(home: &Path, env: &dyn Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
    let mut dirs = vec![
        home.join(".local/bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        home.join(".npm-global/bin"),
    ];
    if let Some(prefix) = env("NPM_CONFIG_PREFIX").filter(|v| !v.is_empty()) {
        let bin = PathBuf::from(prefix).join("bin");
        if !dirs.contains(&bin) {
            dirs.push(bin);
        }
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::time::Duration;

    #[test]
    fn tool_dirs() {
        assert_eq!(
            tool_dirs_with(Path::new("/home/you"), &|_| None),
            vec![
                PathBuf::from("/home/you/.local/bin"),
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
                PathBuf::from("/home/you/.npm-global/bin"),
            ]
        );
        let env = |k: &str| (k == "NPM_CONFIG_PREFIX").then(|| OsString::from("/opt/npm"));
        assert_eq!(
            tool_dirs_with(Path::new("/home/you"), &env).last(),
            Some(&PathBuf::from("/opt/npm/bin"))
        );
    }

    /// `sh` starts `sleep`, which holds the stdout pipe. Killing only `sh` would leave the
    /// pipe open for 30 seconds; killing the group closes it at once.
    #[test]
    fn kill_tree_stops_grandchildren() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "sleep 30; true"]);
        let mut child = spawn_piped(&mut cmd).unwrap();
        let mut out = child.stdout.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut b = vec![];
            let _ = out.read_to_end(&mut b);
            let _ = tx.send(());
        });
        std::thread::sleep(Duration::from_millis(300));
        kill_tree(&mut child);
        assert!(child.try_wait().unwrap().is_some());
        assert!(
            rx.recv_timeout(Duration::from_secs(5)).is_ok(),
            "a grandchild still holds stdout"
        );
    }
}
