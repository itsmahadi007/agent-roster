//! Desktop glue shared by the CLI and the app: where `roster` and the app live, and how to
//! open a terminal running a command.

use crate::names::CommandLookup;
use crate::paths::{Platform, RosterPaths};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Linux needs a display server; macOS and Windows always have a desktop.
pub fn has_desktop(platform: Platform, env: &dyn Fn(&str) -> Option<String>) -> bool {
    match platform {
        Platform::Linux => ["DISPLAY", "WAYLAND_DISPLAY"]
            .iter()
            .any(|k| env(k).is_some_and(|v| !v.is_empty())),
        _ => true,
    }
}

/// Usual install places of the `roster` command, for apps started without the user's PATH.
pub fn roster_bin_candidates(p: &RosterPaths) -> Vec<PathBuf> {
    let h = &p.home;
    match p.platform {
        Platform::Windows => vec![
            p.data_dir.join("roster.exe"),
            h.join(".cargo").join("bin").join("roster.exe"),
        ],
        _ => vec![
            h.join(".local/bin/roster"),
            h.join(".cargo/bin/roster"),
            PathBuf::from("/opt/homebrew/bin/roster"),
            PathBuf::from("/usr/local/bin/roster"),
            PathBuf::from("/usr/bin/roster"),
        ],
    }
}

pub fn find_roster_bin(p: &RosterPaths, lookup: &dyn CommandLookup) -> Option<PathBuf> {
    lookup
        .find("roster")
        .or_else(|| roster_bin_candidates(p).into_iter().find(|c| c.is_file()))
}

pub fn app_candidates(p: &RosterPaths, env: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = env("ROSTER_APP")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .into_iter()
        .collect();
    let h = &p.home;
    match p.platform {
        Platform::Mac => {
            v.push(h.join("Applications").join("Agent Roster.app"));
            v.push(PathBuf::from("/Applications/Agent Roster.app"));
        }
        Platform::Windows => {
            for k in ["LOCALAPPDATA", "ProgramFiles"] {
                if let Some(base) = env(k) {
                    v.push(
                        PathBuf::from(base)
                            .join("Agent Roster")
                            .join("agent-roster-app.exe"),
                    );
                }
            }
        }
        Platform::Linux => {
            v.push(PathBuf::from("/usr/bin/agent-roster-app"));
            v.push(h.join(".local/bin/agent-roster-app"));
            v.push(h.join("Applications/agent-roster.AppImage"));
        }
    }
    v
}

pub fn find_app(p: &RosterPaths, env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    app_candidates(p, env).into_iter().find(|c| c.exists())
}

pub fn app_launch_command(platform: Platform, app: &Path) -> Vec<String> {
    let a = app.display().to_string();
    if platform == Platform::Mac && app.extension().is_some_and(|e| e == "app") {
        vec!["open".into(), a]
    } else {
        vec![a]
    }
}

#[derive(Debug)]
pub enum DesktopError {
    NoTerminal,
    Io(String),
}
impl fmt::Display for DesktopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DesktopError::NoTerminal => write!(
                f,
                "No terminal app was found. Set TERMINAL to your terminal's command."
            ),
            DesktopError::Io(e) => write!(f, "{e}"),
        }
    }
}
impl std::error::Error for DesktopError {}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

/// The command that opens a new terminal window running `argv`.
/// On macOS this writes a `.command` file into `scratch`, opened by the user's terminal app.
pub fn terminal_command(
    platform: Platform,
    env: &dyn Fn(&str) -> Option<String>,
    lookup: &dyn CommandLookup,
    argv: &[String],
    scratch: &Path,
) -> Result<Vec<String>, DesktopError> {
    let io = |e: std::io::Error| DesktopError::Io(e.to_string());
    match platform {
        Platform::Mac => {
            std::fs::create_dir_all(scratch).map_err(io)?;
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let file = scratch.join(format!(
                "agent-roster-{}-{stamp}-{}.command",
                std::process::id(),
                NEXT_FILE.fetch_add(1, Ordering::Relaxed)
            ));
            let line: Vec<String> = argv.iter().map(|a| sh_quote(a)).collect();
            std::fs::write(&file, format!("#!/bin/sh\nexec {}\n", line.join(" "))).map_err(io)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))
                    .map_err(io)?;
            }
            Ok(vec!["open".into(), file.display().to_string()])
        }
        Platform::Windows => {
            let mut cmd = match lookup.find("wt") {
                Some(wt) => vec![wt.display().to_string()],
                None => ["cmd.exe", "/c", "start", "", "cmd.exe", "/k"]
                    .map(String::from)
                    .to_vec(),
            };
            cmd.extend(argv.iter().cloned());
            Ok(cmd)
        }
        Platform::Linux => {
            // gnome-terminal takes `--` before the command; every other terminal takes `-e`.
            let flag_for = |cmd: &str| {
                if Path::new(cmd)
                    .file_name()
                    .is_some_and(|n| n == "gnome-terminal")
                {
                    "--"
                } else {
                    "-e"
                }
            };
            let mut term: Option<(String, &str)> =
                env("TERMINAL").filter(|t| !t.trim().is_empty()).map(|t| {
                    let flag = flag_for(&t);
                    (t, flag)
                });
            if term.is_none() {
                for (name, flag) in [
                    ("x-terminal-emulator", "-e"),
                    ("gnome-terminal", "--"),
                    ("konsole", "-e"),
                    ("xterm", "-e"),
                ] {
                    if let Some(found) = lookup.find(name) {
                        term = Some((found.display().to_string(), flag));
                        break;
                    }
                }
            }
            let (t, flag) = term.ok_or(DesktopError::NoTerminal)?;
            let mut cmd = vec![t, flag.to_string()];
            cmd.extend(argv.iter().cloned());
            Ok(cmd)
        }
    }
}
