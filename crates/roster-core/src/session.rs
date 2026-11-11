//! Live session records: which agents are running, and which agent started which.

use crate::model::Tool;
use crate::paths::RosterPaths;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub pid: u32,
    pub agent_id: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub cwd: String,
    #[serde(default)]
    pub task: Option<String>,
    pub started_at: i64,
}

/// The task text when an agent was started non-interactively by another agent.
pub fn task_from_args(tool: Tool, args: &[String]) -> Option<String> {
    let raw = match tool {
        Tool::Claude => {
            let i = args.iter().position(|a| a == "-p" || a == "--print")?;
            args.get(i + 1).filter(|a| !a.starts_with('-')).cloned()
        }
        Tool::Codex => {
            let i = args.iter().position(|a| a == "exec")?;
            let mut it = args[i + 1..].iter();
            let mut found = None;
            while let Some(a) = it.next() {
                if matches!(
                    a.as_str(),
                    "-C" | "--cd"
                        | "-m"
                        | "--model"
                        | "-s"
                        | "--sandbox"
                        | "-c"
                        | "--config"
                        | "-p"
                        | "--profile"
                ) {
                    it.next();
                } else if !a.starts_with('-') {
                    found = Some(a.clone());
                    break;
                }
            }
            found
        }
        Tool::Grok => {
            let i = args.iter().position(|a| a == "-p" || a == "--single")?;
            args.get(i + 1).filter(|a| !a.starts_with('-')).cloned()
        }
    }?;
    Some(raw.chars().take(120).collect())
}

/// Writes a record under `run_dir/<pid>-<tag>.json`.
pub fn start(p: &RosterPaths, rec: &SessionRecord, tag: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(p.run_dir())?;
    let path = p.run_dir().join(format!("{}-{tag}.json", rec.pid));
    std::fs::write(
        &path,
        serde_json::to_string(rec).map_err(std::io::Error::other)?,
    )?;
    Ok(path)
}

pub fn finish(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
}

/// Every running agent session. Records whose process is gone are deleted.
pub fn live_sessions(p: &RosterPaths) -> Vec<SessionRecord> {
    let mut out = vec![];
    let Ok(entries) = std::fs::read_dir(p.run_dir()) else {
        return out;
    };
    for e in entries.flatten() {
        let path = e.path();
        let rec: Option<SessionRecord> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok());
        match rec {
            Some(r) if pid_alive(r.pid) => out.push(r),
            _ => finish(&path),
        }
    }
    out.sort_by(|a, b| {
        a.started_at
            .cmp(&b.started_at)
            .then(a.agent_id.cmp(&b.agent_id))
    });
    out
}

/// Running sessions that another agent started.
pub fn live_calls(p: &RosterPaths) -> Vec<SessionRecord> {
    live_sessions(p)
        .into_iter()
        .filter(|r| r.parent_id.is_some())
        .collect()
}

#[cfg(unix)]
pub fn pid_alive(pid: u32) -> bool {
    let r = unsafe { libc::kill(pid as libc::pid_t, 0) };
    r == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
pub fn pid_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return false;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code) != 0;
        CloseHandle(h);
        ok && code == STILL_ACTIVE as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Tool;
    use crate::paths::{Platform, RosterPaths};

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn task_extraction() {
        assert_eq!(
            task_from_args(Tool::Claude, &s(&["-p", "write tests"])),
            Some("write tests".into())
        );
        assert_eq!(
            task_from_args(Tool::Claude, &s(&["--print", "x"])),
            Some("x".into())
        );
        assert_eq!(
            task_from_args(Tool::Codex, &s(&["exec", "--full-auto", "make hero image"])),
            Some("make hero image".into())
        );
        assert_eq!(
            task_from_args(Tool::Codex, &s(&["exec", "-C", "/tmp", "go"])),
            Some("go".into())
        );
        assert_eq!(
            task_from_args(Tool::Grok, &s(&["--always-approve", "-p", "fix the test"])),
            Some("fix the test".into())
        );
        assert_eq!(
            task_from_args(Tool::Grok, &s(&["--single", "go"])),
            Some("go".into())
        );
        assert_eq!(task_from_args(Tool::Claude, &s(&[])), None);
        assert_eq!(
            task_from_args(Tool::Claude, &s(&["-p", &"x".repeat(300)]))
                .unwrap()
                .chars()
                .count(),
            120
        );
    }

    #[test]
    fn live_calls_drop_dead_pids() {
        let d = tempfile::tempdir().unwrap();
        let p = RosterPaths::for_home(d.path(), Platform::Mac, &|_| None);
        let mine = SessionRecord {
            pid: std::process::id(),
            agent_id: "c3".into(),
            parent_id: Some("a1".into()),
            cwd: "/w".into(),
            task: Some("t".into()),
            started_at: 1,
        };
        let solo = SessionRecord {
            pid: std::process::id(),
            agent_id: "b2".into(),
            parent_id: None,
            cwd: "/w".into(),
            task: None,
            started_at: 1,
        };
        let dead = SessionRecord {
            pid: 4_000_000,
            agent_id: "x".into(),
            parent_id: Some("a1".into()),
            cwd: "/w".into(),
            task: None,
            started_at: 1,
        };
        start(&p, &mine, "mine").unwrap();
        start(&p, &solo, "solo").unwrap();
        let dead_path = start(&p, &dead, "dead").unwrap();
        let calls = live_calls(&p);
        assert_eq!(calls, vec![mine.clone()]);
        assert!(!dead_path.exists(), "stale record deleted");
        assert_eq!(live_sessions(&p).len(), 2);
    }
}
