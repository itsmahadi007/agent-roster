//! How much of each agent's limit is used, and when it resets.

use crate::paths::RosterPaths;
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LimitWindow {
    pub label: String,
    pub used_percent: f64,
    pub resets_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentLimits {
    /// When these numbers were recorded (unix seconds).
    pub as_of: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    pub windows: Vec<LimitWindow>,
}

pub fn window_label(minutes: i64) -> String {
    match minutes {
        10080 => "week".into(),
        m if m >= 60 && m % 60 == 0 => format!("{} hr", m / 60),
        m => format!("{m} min"),
    }
}

pub fn format_reset(resets_at: i64, now: i64) -> String {
    let secs = resets_at - now;
    if secs <= 0 {
        return "resetting now".into();
    }
    let (d, h, m) = (secs / 86_400, (secs % 86_400) / 3600, (secs % 3600) / 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{}m", m.max(1))
    }
}

fn mtime(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn sorted_children(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).map(|r| r.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    v.sort_unstable_by(|a, b| b.cmp(a));
    v
}

/// Session files from the newest few day folders (sessions/YYYY/MM/DD), newest first by mtime.
fn recent_session_files(codex_home: &Path) -> Vec<PathBuf> {
    let mut day_dirs = vec![];
    let root = codex_home.join("sessions");
    for y in sorted_children(&root).into_iter().filter(|p| p.is_dir()) {
        for m in sorted_children(&y).into_iter().filter(|p| p.is_dir()) {
            for d in sorted_children(&m).into_iter().filter(|p| p.is_dir()) {
                day_dirs.push(d);
                if day_dirs.len() >= 3 {
                    break;
                }
            }
            if day_dirs.len() >= 3 {
                break;
            }
        }
        if day_dirs.len() >= 3 {
            break;
        }
    }
    let mut files: Vec<PathBuf> = day_dirs
        .iter()
        .flat_map(|d| sorted_children(d))
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    // Flat layouts (older Codex versions) keep files directly in sessions/.
    files.extend(sorted_children(&root).into_iter().filter(|p| p.extension().is_some_and(|e| e == "jsonl")));
    files.sort_by_key(|f| std::cmp::Reverse(mtime(f)));
    files
}

fn tail_text(path: &Path, max: u64) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    if len > max {
        f.seek(SeekFrom::Start(len - max)).ok()?;
    }
    let mut buf = vec![];
    f.read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

fn limits_from_line(line: &str) -> Option<(Option<String>, Vec<LimitWindow>)> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    if v.get("type")?.as_str()? != "event_msg" {
        return None;
    }
    let payload = v.get("payload")?;
    if payload.get("type")?.as_str()? != "token_count" {
        return None;
    }
    let rl = payload.get("rate_limits")?;
    if rl.is_null() {
        return None;
    }
    let mut windows = vec![];
    for key in ["primary", "secondary"] {
        if let Some(w) = rl.get(key).filter(|w| !w.is_null()) {
            windows.push(LimitWindow {
                label: window_label(w.get("window_minutes")?.as_i64()?),
                used_percent: w.get("used_percent")?.as_f64()?,
                resets_at: w.get("resets_at")?.as_i64()?,
            });
        }
    }
    windows.sort_by_key(|w| if w.label == "week" { 1 } else { 0 });
    let plan = rl.get("plan_type").and_then(|x| x.as_str()).map(String::from);
    Some((plan, windows))
}

/// Latest limits Codex recorded for this config folder. Read-only.
pub fn read_codex_limits(codex_home: &Path) -> Option<AgentLimits> {
    for file in recent_session_files(codex_home).into_iter().take(5) {
        for max in [256 * 1024, u64::MAX] {
            let Some(text) = tail_text(&file, max) else { break };
            if let Some((plan, windows)) = text.lines().rev().find_map(limits_from_line) {
                return Some(AgentLimits { as_of: mtime(&file), plan, windows });
            }
            if std::fs::metadata(&file).map(|m| m.len() <= max).unwrap_or(true) {
                break;
            }
        }
    }
    None
}

fn claude_limits_path(p: &RosterPaths, agent_id: &str) -> PathBuf {
    p.limits_dir().join(format!("{agent_id}.json"))
}

pub fn read_claude_limits(p: &RosterPaths, agent_id: &str) -> Option<AgentLimits> {
    serde_json::from_str(&std::fs::read_to_string(claude_limits_path(p, agent_id)).ok()?).ok()
}

pub fn write_claude_limits(p: &RosterPaths, agent_id: &str, l: &AgentLimits) -> std::io::Result<()> {
    std::fs::create_dir_all(p.limits_dir())?;
    let path = claude_limits_path(p, agent_id);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string(l).map_err(std::io::Error::other)?)?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::{Platform, RosterPaths};
    use std::path::PathBuf;

    fn home_with_fixture(name: &str) -> (tempfile::TempDir, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let day = d.path().join("sessions/2026/11/09");
        std::fs::create_dir_all(&day).unwrap();
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
        std::fs::copy(src, day.join("rollout-2026-11-09T13-09-44-x.jsonl")).unwrap();
        let home = d.path().to_path_buf();
        (d, home)
    }

    #[test]
    fn codex_last_token_count_wins() {
        let (_d, h) = home_with_fixture("codex-rollout.jsonl");
        let l = read_codex_limits(&h).unwrap();
        assert_eq!(l.windows, vec![LimitWindow { label: "week".into(), used_percent: 93.0, resets_at: 1794287948 }]);
        assert_eq!(l.plan.as_deref(), Some("pro"));
        assert!(l.as_of > 0);
    }

    #[test]
    fn codex_skips_malformed_lines_and_reads_both_windows() {
        let (_d, h) = home_with_fixture("codex-rollout-bad.jsonl");
        let l = read_codex_limits(&h).unwrap();
        assert_eq!(l.windows.len(), 2);
        assert_eq!(l.windows[0].label, "5 hr");
        assert_eq!(l.windows[1].label, "week");
    }

    #[test]
    fn codex_no_sessions_is_none() {
        let d = tempfile::tempdir().unwrap();
        assert!(read_codex_limits(d.path()).is_none());
    }

    #[test]
    fn labels() {
        assert_eq!(window_label(300), "5 hr");
        assert_eq!(window_label(10080), "week");
        assert_eq!(window_label(60), "1 hr");
        assert_eq!(window_label(30), "30 min");
    }

    #[test]
    fn reset_format() {
        let now = 1_000_000;
        assert_eq!(format_reset(now + 318_797, now), "3d 16h");
        assert_eq!(format_reset(now + 78_745, now), "21h 52m");
        assert_eq!(format_reset(now + 600, now), "10m");
        assert_eq!(format_reset(now - 5, now), "resetting now");
    }

    #[test]
    fn claude_limits_file_round_trip() {
        let d = tempfile::tempdir().unwrap();
        let p = RosterPaths::for_home(d.path(), Platform::Mac, &|_| None);
        let l = AgentLimits { as_of: 5, plan: None, windows: vec![LimitWindow { label: "week".into(), used_percent: 61.0, resets_at: 1794528000 }] };
        write_claude_limits(&p, "a1", &l).unwrap();
        assert_eq!(read_claude_limits(&p, "a1"), Some(l));
        assert_eq!(read_claude_limits(&p, "nobody"), None);
    }
}
