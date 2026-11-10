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
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
};

/// Shown in logs and `roster doctor`.
pub const PLATFORM: &str = "windows";

/// The job of each child spawned by `spawn_contained`, by process id. Handles are kept as `usize`
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
    spawn_contained(cmd)
}

/// Spawns hidden with the caller's stdin, stdout and stderr, inside a Job Object so
/// `kill_tree` (or this process exiting) stops everything the child starts. Every child
/// spawned this way must end with `kill_tree`, which also frees its job.
///
/// The child starts suspended and runs only once it is in its job, so nothing it starts can
/// escape. If it can't be put in a job it is stopped before it ever ran, and this fails.
pub fn spawn_contained(cmd: &mut Command) -> std::io::Result<Child> {
    use std::os::windows::process::CommandExt;
    let mut child = hide(cmd)
        .creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED)
        .spawn()?;
    test_hooks::before_assign();
    let job = match job_for(&child) {
        Ok(job) => job,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
    };
    JOBS.lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((child.id(), job as usize));
    if let Err(e) = resume(child.id()) {
        kill_tree(&mut child);
        return Err(e);
    }
    Ok(child)
}

/// A new kill-on-close job holding `child`.
fn job_for(child: &Child) -> std::io::Result<HANDLE> {
    if test_hooks::fail_assign() {
        return Err(std::io::Error::other("job assignment refused (test)"));
    }
    // SAFETY: plain Win32 calls on a handle we own; the struct is zeroed then filled.
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return Err(std::io::Error::last_os_error());
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
            Ok(job)
        } else {
            let e = std::io::Error::last_os_error();
            CloseHandle(job);
            Err(e)
        }
    }
}

/// Lets a child started suspended run: resumes its threads (a suspended new process has
/// only its primary one). `std` doesn't hand out the thread handle, so the thread is found
/// by its process id.
fn resume(pid: u32) -> std::io::Result<()> {
    // SAFETY: plain Win32 calls; every handle opened here is closed here.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snap == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        let mut entry = THREADENTRY32 {
            dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        let mut resumed = 0;
        let mut more = Thread32First(snap, &mut entry) != 0;
        while more {
            if entry.th32OwnerProcessID == pid {
                let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                if !thread.is_null() {
                    if ResumeThread(thread) != u32::MAX {
                        resumed += 1;
                    }
                    CloseHandle(thread);
                }
            }
            more = Thread32Next(snap, &mut entry) != 0;
        }
        CloseHandle(snap);
        if resumed == 0 {
            return Err(std::io::Error::other(
                "the new process could not be resumed",
            ));
        }
    }
    Ok(())
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

/// Install folders shared by every user. None on Windows: npm and WinGet install per user.
pub fn system_tool_dirs() -> Vec<PathBuf> {
    Vec::new()
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

#[cfg(not(test))]
mod test_hooks {
    pub fn before_assign() {}
    pub fn fail_assign() -> bool {
        false
    }
}

/// Lets a test hold the child between spawning and assigning its job, or make the
/// assignment fail. Per thread, so tests running alongside are not affected.
#[cfg(test)]
mod test_hooks {
    use std::cell::Cell;
    use std::time::Duration;
    thread_local! {
        pub static DELAY: Cell<Option<Duration>> = const { Cell::new(None) };
        pub static FAIL: Cell<bool> = const { Cell::new(false) };
    }
    pub fn before_assign() {
        if let Some(d) = DELAY.get() {
            std::thread::sleep(d);
        }
    }
    pub fn fail_assign() -> bool {
        FAIL.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::time::Duration;

    /// A `.cmd` that at once starts a helper which appends to `out` until `stop` exists,
    /// then waits 30 s.
    const SPAWNER: &str = concat!(
        "@echo off\r\n",
        "start \"\" /b cmd /d /c \"for /l %%i in (0,0,1) do @((if exist stop exit) & ",
        "echo x>>out & ping -n 1 -w 100 192.0.2.1 >nul)\"\r\n",
        "ping -n 30 127.0.0.1 >nul\r\n",
    );

    fn spawner(dir: &Path) -> Command {
        let f = dir.join("spawner.cmd");
        std::fs::write(&f, SPAWNER).unwrap();
        let mut c = Command::new(f);
        c.current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        c
    }

    /// Stops the helper even when the test fails, so it can't run on forever.
    struct Stop(PathBuf);
    impl Drop for Stop {
        fn drop(&mut self) {
            let _ = std::fs::write(&self.0, "");
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    fn out_size(dir: &Path) -> u64 {
        std::fs::metadata(dir.join("out"))
            .map(|m| m.len())
            .unwrap_or(0)
    }

    /// The child starts a helper at once. Even when its job is assigned late, the helper
    /// must be in it: the child can't run before it is contained.
    #[test]
    fn nothing_the_child_starts_escapes_its_job() {
        let d = tempfile::tempdir().unwrap();
        let _stop = Stop(d.path().join("stop"));
        let mut cmd = spawner(d.path());
        test_hooks::DELAY.set(Some(Duration::from_millis(1500)));
        let child = spawn_contained(&mut cmd);
        test_hooks::DELAY.set(None);
        let mut child = child.unwrap();
        let started = std::time::Instant::now();
        while out_size(d.path()) == 0 && started.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(out_size(d.path()) > 0, "the helper never started");
        kill_tree(&mut child);
        std::thread::sleep(Duration::from_millis(300));
        let before = out_size(d.path());
        std::thread::sleep(Duration::from_millis(1200));
        assert_eq!(out_size(d.path()), before, "the helper is still running");
    }

    /// When the child can't be put in a job it is stopped before it runs, and the spawn fails.
    #[test]
    fn a_child_that_cannot_be_contained_never_runs() {
        let d = tempfile::tempdir().unwrap();
        let _stop = Stop(d.path().join("stop"));
        let mut cmd = spawner(d.path());
        test_hooks::FAIL.set(true);
        let got = spawn_contained(&mut cmd);
        test_hooks::FAIL.set(false);
        let failed = got.is_err();
        if let Ok(mut c) = got {
            kill_tree(&mut c);
        }
        assert!(failed, "an uncontained child was handed back");
        std::thread::sleep(Duration::from_millis(1500));
        assert_eq!(out_size(d.path()), 0, "the child ran");
    }

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
