//! Every path Agent Roster touches derives from `RosterPaths`.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Linux,
    Windows,
}

impl Platform {
    pub fn current() -> Platform {
        if cfg!(target_os = "macos") {
            Platform::Mac
        } else if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Linux
        }
    }
}

#[derive(Debug, Clone)]
pub struct RosterPaths {
    pub home: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub platform: Platform,
}

impl RosterPaths {
    pub fn for_home(home: &Path, platform: Platform, env: &dyn Fn(&str) -> Option<String>) -> RosterPaths {
        let (config_dir, data_dir) = match platform {
            Platform::Windows => {
                let roaming = env("APPDATA").map(PathBuf::from).unwrap_or_else(|| home.join("AppData").join("Roaming"));
                let local = env("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| home.join("AppData").join("Local"));
                (roaming.join("AgentRoster"), local.join("AgentRoster"))
            }
            Platform::Mac | Platform::Linux => {
                let base = env("XDG_CONFIG_HOME")
                    .filter(|v| !v.is_empty())
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(".config"));
                (base.join("agent-roster"), home.join(".agent-roster"))
            }
        };
        RosterPaths { home: home.to_path_buf(), config_dir, data_dir, platform }
    }

    pub fn detect() -> RosterPaths {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        RosterPaths::for_home(&home, Platform::current(), &|k| std::env::var(k).ok())
    }

    pub fn roster_file(&self) -> PathBuf {
        self.config_dir.join("roster.toml")
    }
    pub fn bin_dir(&self) -> PathBuf {
        self.data_dir.join("bin")
    }
    pub fn accounts_dir(&self) -> PathBuf {
        self.data_dir.join("accounts")
    }
    pub fn journal_dir(&self) -> PathBuf {
        self.data_dir.join("journal")
    }
    pub fn run_dir(&self) -> PathBuf {
        self.data_dir.join("run")
    }
    pub fn limits_dir(&self) -> PathBuf {
        self.data_dir.join("limits")
    }
}

pub fn expand_tilde(p: &str, home: &Path) -> PathBuf {
    if p == "~" {
        home.to_path_buf()
    } else if let Some(rest) = p.strip_prefix("~/").or_else(|| p.strip_prefix("~\\")) {
        home.join(rest)
    } else {
        PathBuf::from(p)
    }
}

pub fn collapse_tilde(p: &Path, home: &Path) -> String {
    match p.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.to_string_lossy().replace('\\', "/")),
        Err(_) => p.to_string_lossy().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_layout() {
        let p = RosterPaths::for_home(Path::new("/h"), Platform::Mac, &|_| None);
        assert_eq!(p.roster_file(), PathBuf::from("/h/.config/agent-roster/roster.toml"));
        assert_eq!(p.bin_dir(), PathBuf::from("/h/.agent-roster/bin"));
        assert_eq!(p.accounts_dir(), PathBuf::from("/h/.agent-roster/accounts"));
    }

    #[test]
    fn xdg_config_respected() {
        let p = RosterPaths::for_home(Path::new("/h"), Platform::Linux, &|k| {
            (k == "XDG_CONFIG_HOME").then(|| "/x".to_string())
        });
        assert_eq!(p.roster_file(), PathBuf::from("/x/agent-roster/roster.toml"));
    }

    #[test]
    fn windows_layout() {
        let env = |k: &str| match k {
            "APPDATA" => Some(r"C:\U\R".to_string()),
            "LOCALAPPDATA" => Some(r"C:\U\L".to_string()),
            _ => None,
        };
        let p = RosterPaths::for_home(Path::new(r"C:\U"), Platform::Windows, &env);
        assert_eq!(p.roster_file(), PathBuf::from(r"C:\U\R").join("AgentRoster").join("roster.toml"));
        assert_eq!(p.bin_dir(), PathBuf::from(r"C:\U\L").join("AgentRoster").join("bin"));
    }

    #[test]
    fn tilde_round_trip() {
        assert_eq!(expand_tilde("~/.claude", Path::new("/h")), PathBuf::from("/h/.claude"));
        assert_eq!(collapse_tilde(Path::new("/h/.codex-2"), Path::new("/h")), "~/.codex-2");
    }
}
