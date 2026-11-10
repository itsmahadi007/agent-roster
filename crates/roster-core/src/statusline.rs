//! `roster statusline`: Claude Code's status line command when an agent's limits are turned on.
//! It saves the limits Claude passes in, then prints the user's own status line unchanged.

use crate::limits::{AgentLimits, LimitWindow, write_claude_limits};
use crate::model::{Agent, Roster, Tool};
use crate::paths::{RosterPaths, expand_tilde};
use std::path::PathBuf;

/// The real file behind an agent's `settings.json` (follows links).
pub fn settings_real_path(a: &Agent, r: &Roster, p: &RosterPaths) -> PathBuf {
    let dir = r.config_dir_of(a, p);
    let path = dir.join("settings.json");
    std::fs::canonicalize(&path)
        .or_else(|_| std::fs::canonicalize(&dir).map(|d| d.join("settings.json")))
        .unwrap_or(path)
}

fn resolve<'a>(
    r: &'a Roster,
    p: &RosterPaths,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<&'a Agent> {
    if let Some(a) =
        env("ROSTER_AGENT").and_then(|id| r.by_id(&id).filter(|a| a.tool == Tool::Claude))
    {
        return Some(a);
    }
    if let Some(dir) = env("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()) {
        let want = std::fs::canonicalize(&dir).unwrap_or_else(|_| PathBuf::from(&dir));
        if let Some(a) = r
            .agents
            .iter()
            .filter(|a| a.tool == Tool::Claude)
            .find(|a| {
                let d = r.config_dir_of(a, p);
                std::fs::canonicalize(&d).unwrap_or(d) == want
            })
        {
            return Some(a);
        }
    }
    let standard = expand_tilde(Tool::Claude.standard_dir(), &p.home);
    r.agents
        .iter()
        .find(|a| a.tool == Tool::Claude && r.config_dir_of(a, p) == standard)
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn windows_from(stdin_json: &str) -> Option<Vec<LimitWindow>> {
    let v: serde_json::Value = serde_json::from_str(stdin_json).ok()?;
    let rl = v.get("rate_limits")?;
    let mut out = vec![];
    for (key, label) in [("five_hour", "5 hr"), ("seven_day", "week")] {
        if let Some(w) = rl.get(key).filter(|w| !w.is_null()) {
            out.push(LimitWindow {
                label: label.to_string(),
                used_percent: w.get("used_percentage")?.as_f64()?,
                resets_at: w.get("resets_at")?.as_i64()?,
            });
        }
    }
    (!out.is_empty()).then_some(out)
}

/// `shell(command, stdin)` runs the user's original status line command.
pub fn handle_statusline(
    stdin_json: &str,
    env: &dyn Fn(&str) -> Option<String>,
    r: &Roster,
    p: &RosterPaths,
    shell: &dyn Fn(&str, &str) -> String,
) -> String {
    let Some(agent) = resolve(r, p, env) else {
        return String::new();
    };
    if let Some(windows) = windows_from(stdin_json) {
        let _ = write_claude_limits(
            p,
            &agent.id,
            &AgentLimits {
                as_of: now(),
                plan: None,
                windows,
            },
        );
    }
    let real = settings_real_path(agent, r, p).display().to_string();
    match r
        .statusline
        .iter()
        .find(|s| s.settings == real)
        .and_then(|s| s.chain.as_deref())
    {
        Some(cmd) => shell(cmd, stdin_json),
        None => agent.name.clone(),
    }
}

/// Runs a command through the system shell with `stdin`, returning its stdout.
pub fn real_shell(cmd: &str, stdin: &str) -> String {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut c = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", cmd]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", cmd]);
        c
    };
    let Ok(mut child) = crate::proc::hide_console(&mut c)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return String::new();
    };
    if let Some(mut s) = child.stdin.take() {
        let _ = s.write_all(stdin.as_bytes());
    }
    child
        .wait_with_output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}
