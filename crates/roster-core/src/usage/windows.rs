//! Windows: how usage checks find, spawn and stop the tools.
//!
//! `codex` from npm is `codex.cmd`, which starts `node`, which starts the native binary: a
//! process tree. Killing only the child would leave the rest running (and holding our pipes),
//! so every child spawned here goes into a Job Object that kills the whole tree when it is
//! closed, including when Agent Roster itself exits or crashes.

use std::ffi::OsString;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};

/// Shown in logs and `roster doctor`.
pub const PLATFORM: &str = "windows";

/// The job of each child spawned by `spawn_piped`, by process id. Handles are kept as `usize`
/// so the table can be shared between threads.
static JOBS: Mutex<Vec<(u32, usize)>> = Mutex::new(Vec::new());

/// No console window for the child.
pub fn hide(cmd: &mut Command) -> &mut Command {
    crate::proc::hide_console(cmd)
}

/// Spawns hidden with stdin and stdout piped and stderr discarded, inside a Job Object so
/// `kill_tree` (or this process exiting) stops everything the child starts.
pub fn spawn_piped(cmd: &mut Command) -> std::io::Result<Child> {
    hide(cmd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let child = cmd.spawn()?;
    if let Some(job) = job_for(&child) {
        JOBS.lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((child.id(), job as usize));
    }
    Ok(child)
}

/// A new kill-on-close job holding `child`, or None when Windows refuses.
fn job_for(child: &Child) -> Option<HANDLE> {
    // SAFETY: plain Win32 calls on a handle we own; the struct is zeroed then filled.
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return None;
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let ok = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) != 0
            && AssignProcessToJobObject(job, child.as_raw_handle() as HANDLE) != 0;
        if ok {
            Some(job)
        } else {
            CloseHandle(job);
            None
        }
    }
}

/// Stops the child and everything it started, then waits for it.
pub fn kill_tree(child: &mut Child) {
    let pid = child.id();
    let job = {
        let mut jobs = JOBS.lock().unwrap_or_else(|e| e.into_inner());
        jobs.iter()
            .position(|(p, _)| *p == pid)
            .map(|i| jobs.swap_remove(i).1)
    };
    match job {
        Some(job) => {
            // SAFETY: the handle came from `job_for` and is closed exactly once, here.
            unsafe {
                TerminateJobObject(job as HANDLE, 1);
                CloseHandle(job as HANDLE);
            }
        }
        None if child.try_wait().ok().flatten().is_none() => taskkill_tree(pid),
        None => {}
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// For a child that is not in a job: `taskkill /T` walks the tree while the root is alive.
fn taskkill_tree(pid: u32) {
    let exe = std::env::var_os("SystemRoot")
        .map(|r| PathBuf::from(r).join(r"System32\taskkill.exe"))
        .unwrap_or_else(|| PathBuf::from("taskkill"));
    let mut c = Command::new(exe);
    let _ = hide(&mut c)
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Folders where Claude Code and Codex usually live that the app's PATH may lack.
pub fn extra_tool_dirs(home: &Path) -> Vec<PathBuf> {
    tool_dirs_with(home, &|k| std::env::var_os(k))
}

/// `extra_tool_dirs` with the environment passed in, so it can be tested.
/// npm's global folder, WinGet's links, and the native installers' `~\.local\bin`.
pub fn tool_dirs_with(home: &Path, env: &dyn Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
    let dir = |var: &str, fallback: &str| {
        env(var)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(fallback))
    };
    vec![
        dir("APPDATA", r"AppData\Roaming").join("npm"),
        dir("LOCALAPPDATA", r"AppData\Local").join(r"Microsoft\WinGet\Links"),
        home.join(r".local\bin"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::time::Duration;

    #[test]
    fn tool_dirs_from_the_environment() {
        let env = |k: &str| match k {
            "APPDATA" => Some(OsString::from(r"D:\Roaming")),
            "LOCALAPPDATA" => Some(OsString::from(r"D:\Local")),
            _ => None,
        };
        assert_eq!(
            tool_dirs_with(Path::new(r"C:\Users\you"), &env),
            vec![
                PathBuf::from(r"D:\Roaming\npm"),
                PathBuf::from(r"D:\Local\Microsoft\WinGet\Links"),
                PathBuf::from(r"C:\Users\you\.local\bin"),
            ]
        );
    }

    #[test]
    fn tool_dirs_without_the_environment_use_home() {
        assert_eq!(
            tool_dirs_with(Path::new(r"C:\Users\you"), &|_| None),
            vec![
                PathBuf::from(r"C:\Users\you\AppData\Roaming\npm"),
                PathBuf::from(r"C:\Users\you\AppData\Local\Microsoft\WinGet\Links"),
                PathBuf::from(r"C:\Users\you\.local\bin"),
            ]
        );
    }

    /// `cmd` starts `ping`, which holds the stdout pipe. Killing only `cmd` would leave the
    /// pipe open for 30 seconds; killing the tree closes it at once.
    #[test]
    fn kill_tree_stops_grandchildren() {
        let mut cmd = Command::new("cmd");
        cmd.args(["/d", "/c", "ping -n 30 127.0.0.1"]);
        let mut child = spawn_piped(&mut cmd).unwrap();
        let mut out = child.stdout.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut b = vec![];
            let _ = out.read_to_end(&mut b);
            let _ = tx.send(());
        });
        std::thread::sleep(Duration::from_millis(500));
        kill_tree(&mut child);
        assert!(child.try_wait().unwrap().is_some());
        assert!(
            rx.recv_timeout(Duration::from_secs(5)).is_ok(),
            "a grandchild still holds stdout"
        );
    }
}
