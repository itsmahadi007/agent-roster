//! The small command files that make each agent name runnable from any terminal.

use crate::journal::{JournalError, Txn};
use crate::model::Roster;
use crate::names::{LAUNCHER_MARKER, is_our_shim};
use crate::paths::{Platform, RosterPaths};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const RC_BEGIN: &str = "# >>> agent-roster >>>";
pub const RC_END: &str = "# <<< agent-roster <<<";

/// File suffix and content for each launcher file of one agent.
pub fn shim_text(platform: Platform, id: &str, roster_bin: &Path) -> Vec<(String, String)> {
    let bin = roster_bin.to_string_lossy();
    let sh = |b: &str| {
        format!("#!/bin/sh\n# {LAUNCHER_MARKER} id={id}\nexec \"{b}\" run {id} -- \"$@\"\n")
    };
    match platform {
        Platform::Mac | Platform::Linux => vec![(String::new(), sh(&bin))],
        Platform::Windows => vec![
            (
                ".cmd".to_string(),
                format!(
                    "@echo off\r\nrem {LAUNCHER_MARKER} id={id}\r\n\"{bin}\" run {id} -- %*\r\n"
                ),
            ),
            (String::new(), sh(&bin.replace('\\', "/"))),
        ],
    }
}

/// Writes one launcher per agent and removes launchers for agents that no longer exist.
pub fn write_launchers(
    txn: &mut Txn,
    r: &Roster,
    p: &RosterPaths,
    roster_bin: &Path,
) -> Result<(), JournalError> {
    let dir = p.bin_dir();
    txn.ensure_dir(&dir)?;
    let mut keep = HashSet::new();
    for a in &r.agents {
        for (suffix, text) in shim_text(p.platform, &a.id, roster_bin) {
            let path = dir.join(format!("{}{}", a.name, suffix));
            txn.write_file(&path, text.as_bytes())?;
            #[cfg(unix)]
            txn.set_executable(&path)?;
            keep.insert(path);
        }
    }
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let path = e.path();
            if !keep.contains(&path) && is_our_shim(&path) {
                txn.remove_file(&path)?;
            }
        }
    }
    Ok(())
}

/// What must change so the launcher folder is on PATH.
#[derive(Debug, Clone, PartialEq)]
pub struct PathSetup {
    pub rc_files: Vec<PathBuf>,
    /// Windows: the new per-user PATH value.
    pub windows_user_path: Option<String>,
}

pub fn path_has_bin(p: &RosterPaths, path_var: &str) -> bool {
    std::env::split_paths(path_var).any(|d| d == p.bin_dir())
}

/// Shell startup files that should carry the PATH line on this machine.
pub fn default_shell_rcs(p: &RosterPaths) -> Vec<PathBuf> {
    let h = &p.home;
    let mut v = vec![];
    match p.platform {
        Platform::Windows => return v,
        Platform::Mac => {
            v.push(h.join(".zshrc"));
            for f in [".bashrc", ".bash_profile"] {
                if h.join(f).exists() {
                    v.push(h.join(f));
                }
            }
        }
        Platform::Linux => {
            v.push(h.join(".bashrc"));
            if h.join(".zshrc").exists() {
                v.push(h.join(".zshrc"));
            }
        }
    }
    if h.join(".config/fish").exists() {
        v.push(h.join(".config/fish/conf.d/agent-roster.fish"));
    }
    v
}

pub fn path_setup_needed(
    p: &RosterPaths,
    path_var: &str,
    shell_rcs: &[PathBuf],
) -> Option<PathSetup> {
    if path_has_bin(p, path_var) {
        return None;
    }
    if p.platform == Platform::Windows {
        let current = windows_user_path().unwrap_or_default();
        if std::env::split_paths(&current).any(|d| d == p.bin_dir()) {
            return None;
        }
        let bin = p.bin_dir().to_string_lossy().to_string();
        let value = if current.trim().is_empty() {
            bin
        } else {
            format!("{};{}", current.trim_end_matches(';'), bin)
        };
        return Some(PathSetup {
            rc_files: vec![],
            windows_user_path: Some(value),
        });
    }
    let missing: Vec<PathBuf> = shell_rcs
        .iter()
        .filter(|rc| {
            !std::fs::read_to_string(rc)
                .map(|t| t.contains(RC_BEGIN))
                .unwrap_or(false)
        })
        .cloned()
        .collect();
    if missing.is_empty() {
        None
    } else {
        Some(PathSetup {
            rc_files: missing,
            windows_user_path: None,
        })
    }
}

fn rc_block(p: &RosterPaths, rc: &Path) -> String {
    let rel = crate::paths::collapse_tilde(&p.bin_dir(), &p.home).replacen('~', "$HOME", 1);
    let line = if rc.extension().is_some_and(|e| e == "fish") {
        format!("fish_add_path --append {rel}")
    } else {
        format!("export PATH=\"$PATH:{rel}\"")
    };
    format!("{RC_BEGIN}\n{line}\n{RC_END}\n")
}

pub fn upsert_rc(existing: &str, block: &str) -> String {
    if let (Some(s), Some(e)) = (existing.find(RC_BEGIN), existing.find(RC_END)) {
        let mut end = e + RC_END.len();
        if existing[end..].starts_with('\n') {
            end += 1;
        }
        return format!("{}{}{}", &existing[..s], block, &existing[end..]);
    }
    let t = existing.trim_end();
    if t.is_empty() {
        block.to_string()
    } else {
        format!("{t}\n\n{block}")
    }
}

pub fn remove_rc(existing: &str) -> String {
    if let (Some(s), Some(e)) = (existing.find(RC_BEGIN), existing.find(RC_END)) {
        let mut end = e + RC_END.len();
        if existing[end..].starts_with('\n') {
            end += 1;
        }
        let before = existing[..s].trim_end_matches('\n');
        let mut out = before.to_string();
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&existing[end..]);
        return out;
    }
    existing.to_string()
}

pub fn apply_path_setup(
    txn: &mut Txn,
    setup: &PathSetup,
    p: &RosterPaths,
) -> Result<(), JournalError> {
    for rc in &setup.rc_files {
        let real = crate::journal::resolve_file_link(rc);
        let existing = std::fs::read_to_string(&real).unwrap_or_default();
        txn.write_file(&real, upsert_rc(&existing, &rc_block(p, rc)).as_bytes())?;
    }
    if let Some(v) = &setup.windows_user_path {
        txn.set_windows_user_path(v)?;
    }
    Ok(())
}

#[cfg(windows)]
fn windows_user_path() -> Option<String> {
    let env = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey("Environment")
        .ok()?;
    env.get_value::<String, _>("Path").ok()
}
#[cfg(not(windows))]
fn windows_user_path() -> Option<String> {
    None
}

/// Tells running programs on Windows that environment variables changed.
#[cfg(windows)]
pub fn broadcast_env_change() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
    };
    let name: Vec<u16> = "Environment\0".encode_utf16().collect();
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            name.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            2000,
            std::ptr::null_mut(),
        );
    }
}
#[cfg(not(windows))]
pub fn broadcast_env_change() {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::Journal;
    use crate::model::{Roster, Tool};
    use crate::note::tests::agent;
    use crate::paths::{Platform, RosterPaths};
    use std::fs;
    use std::path::Path;

    #[test]
    fn unix_shim_exact() {
        assert_eq!(
            shim_text(
                Platform::Mac,
                "a1b2c3",
                Path::new("/opt/homebrew/bin/roster")
            )[0]
            .1,
            "#!/bin/sh\n# generated-by-agent-roster id=a1b2c3\nexec \"/opt/homebrew/bin/roster\" run a1b2c3 -- \"$@\"\n"
        );
    }

    #[test]
    fn windows_shim_has_cmd_and_sh() {
        let s = shim_text(Platform::Windows, "x", Path::new(r"C:\r.exe"));
        assert_eq!(s.len(), 2);
        let cmd = s.iter().find(|(suf, _)| suf == ".cmd").unwrap();
        assert_eq!(
            cmd.1,
            "@echo off\r\nrem generated-by-agent-roster id=x\r\n\"C:\\r.exe\" run x -- %*\r\n"
        );
        let sh = s.iter().find(|(suf, _)| suf.is_empty()).unwrap();
        assert!(sh.1.contains("exec \"C:/r.exe\" run x"));
    }

    fn setup() -> (tempfile::TempDir, RosterPaths) {
        let d = tempfile::tempdir().unwrap();
        let p = RosterPaths::for_home(d.path(), Platform::current(), &|_| None);
        (d, p)
    }

    #[test]
    fn writes_one_launcher_per_agent() {
        let (_d, p) = setup();
        let r = Roster {
            version: 1,
            agents: vec![agent("a1", "boss", Tool::Claude, "~/.claude", None)],
            statusline: vec![],
        };
        let mut j = Journal::open(&p).unwrap();
        let mut t = j.begin("launchers");
        write_launchers(&mut t, &r, &p, Path::new("/usr/local/bin/roster")).unwrap();
        t.commit().unwrap();
        let shim = p
            .bin_dir()
            .join(if cfg!(windows) { "boss.cmd" } else { "boss" });
        assert!(fs::read_to_string(&shim).unwrap().contains("run a1 --"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&shim).unwrap().permissions().mode() & 0o777,
                0o755
            );
        }
    }

    #[test]
    fn stale_launcher_removed_foreign_file_kept() {
        let (_d, p) = setup();
        fs::create_dir_all(p.bin_dir()).unwrap();
        fs::write(
            p.bin_dir().join("old"),
            "#!/bin/sh\n# generated-by-agent-roster id=zz\n",
        )
        .unwrap();
        fs::write(p.bin_dir().join("mine"), "#!/bin/sh\necho mine\n").unwrap();
        let r = Roster::default();
        let mut j = Journal::open(&p).unwrap();
        let mut t = j.begin("launchers");
        write_launchers(&mut t, &r, &p, Path::new("/r")).unwrap();
        t.commit().unwrap();
        assert!(!p.bin_dir().join("old").exists());
        assert!(p.bin_dir().join("mine").exists());
    }

    #[cfg(unix)] // shell startup files are a macOS/Linux thing; Windows uses the registry
    #[test]
    fn rc_block_appended_once() {
        let (d, p) = setup();
        let rc = d.path().join(".zshrc");
        fs::write(&rc, "export EDITOR=vim\n").unwrap();
        let setup = PathSetup {
            rc_files: vec![rc.clone()],
            windows_user_path: None,
        };
        let mut j = Journal::open(&p).unwrap();
        for _ in 0..2 {
            let mut t = j.begin("path");
            apply_path_setup(&mut t, &setup, &p).unwrap();
            t.commit().unwrap();
        }
        let text = fs::read_to_string(&rc).unwrap();
        assert_eq!(text.matches(RC_BEGIN).count(), 1);
        assert!(text.starts_with("export EDITOR=vim\n"));
        assert!(text.contains("export PATH=\"$PATH:$HOME/.agent-roster/bin\""));
    }

    #[cfg(unix)] // shell startup files are a macOS/Linux thing; Windows uses the registry
    #[test]
    fn fish_uses_fish_add_path() {
        let (d, p) = setup();
        let fish = d.path().join(".config/fish/conf.d/agent-roster.fish");
        let setup = PathSetup {
            rc_files: vec![fish.clone()],
            windows_user_path: None,
        };
        let mut j = Journal::open(&p).unwrap();
        let mut t = j.begin("path");
        apply_path_setup(&mut t, &setup, &p).unwrap();
        t.commit().unwrap();
        assert!(
            fs::read_to_string(&fish)
                .unwrap()
                .contains("fish_add_path --append $HOME/.agent-roster/bin")
        );
    }

    #[test]
    fn no_setup_when_on_path() {
        let (d, p) = setup();
        let path_var = std::env::join_paths([p.bin_dir(), d.path().join("other")]).unwrap();
        assert!(
            path_setup_needed(&p, &path_var.to_string_lossy(), &[d.path().join(".zshrc")])
                .is_none()
        );
        assert!(
            path_setup_needed(&p, "/usr/bin", &[d.path().join(".zshrc")]).is_some()
                || cfg!(windows)
        );
    }
}
