//! Every change Agent Roster makes, with backups, so each one can be undone.

use crate::paths::RosterPaths;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Change {
    Created { path: PathBuf, after_sha256: String },
    Modified { path: PathBuf, backup: PathBuf, after_sha256: String },
    Removed { path: PathBuf, backup: PathBuf },
    LinkRemoved { path: PathBuf, target: PathBuf },
    LinkReplaced { path: PathBuf, target: PathBuf, after_sha256: String },
    DirCreated { path: PathBuf },
    Linked { link: PathBuf, target: PathBuf },
    UserPath { before: Option<String>, after: String },
    DirMoved { from: PathBuf, to: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Op {
    pub id: u64,
    pub at: i64,
    pub summary: String,
    pub changes: Vec<Change>,
    #[serde(default)]
    pub undone: bool,
}

#[derive(Debug)]
pub enum JournalError {
    Io(std::io::Error),
    IsLink(PathBuf),
    Unsupported(&'static str),
    Corrupt(String),
}
impl fmt::Display for JournalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JournalError::Io(e) => write!(f, "{e}"),
            JournalError::IsLink(p) => write!(f, "{} is a link to another file, so it was left alone", p.display()),
            JournalError::Unsupported(w) => write!(f, "{w} isn't supported on this system"),
            JournalError::Corrupt(e) => write!(f, "the change history file is damaged: {e}"),
        }
    }
}
impl std::error::Error for JournalError {}
impl From<std::io::Error> for JournalError {
    fn from(e: std::io::Error) -> Self {
        JournalError::Io(e)
    }
}

#[derive(Debug)]
pub enum UndoError {
    Conflict { path: PathBuf },
    AlreadyUndone(u64),
    NotFound(u64),
    Io(String),
}
impl fmt::Display for UndoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UndoError::Conflict { path } => write!(
                f,
                "{} changed after Agent Roster wrote it, so it was not overwritten. Look at the file, then undo by hand if you still want to.",
                path.display()
            ),
            UndoError::AlreadyUndone(id) => write!(f, "change {id} is already undone"),
            UndoError::NotFound(id) => write!(f, "there is no change {id}"),
            UndoError::Io(e) => write!(f, "{e}"),
        }
    }
}
impl std::error::Error for UndoError {}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn file_sha(path: &Path) -> Option<String> {
    fs::read(path).ok().map(|b| sha256_hex(&b))
}

fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).map(|m| m.file_type().is_symlink()).unwrap_or(false)
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub struct Journal {
    dir: PathBuf,
    ops: Vec<Op>,
}

impl Journal {
    pub fn open(p: &RosterPaths) -> Result<Journal, JournalError> {
        let dir = p.journal_dir();
        let file = dir.join("journal.jsonl");
        let mut ops = vec![];
        if let Ok(text) = fs::read_to_string(&file) {
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                ops.push(serde_json::from_str(line).map_err(|e| JournalError::Corrupt(e.to_string()))?);
            }
        }
        Ok(Journal { dir, ops })
    }

    pub fn ops(&self) -> &[Op] {
        &self.ops
    }

    pub fn begin(&mut self, summary: &str) -> Txn<'_> {
        let id = self.ops.iter().map(|o| o.id).max().unwrap_or(0) + 1;
        Txn { journal: self, id, summary: summary.to_string(), changes: vec![], committed: false, backups: 0 }
    }

    fn backup_dir(&self, id: u64) -> PathBuf {
        self.dir.join("backups").join(id.to_string())
    }

    fn save_all(&self) -> Result<(), JournalError> {
        fs::create_dir_all(&self.dir)?;
        let mut text = String::new();
        for op in &self.ops {
            text.push_str(&serde_json::to_string(op).map_err(|e| JournalError::Corrupt(e.to_string()))?);
            text.push('\n');
        }
        let tmp = self.dir.join("journal.jsonl.tmp");
        fs::write(&tmp, text)?;
        fs::rename(tmp, self.dir.join("journal.jsonl"))?;
        Ok(())
    }

    fn append(&mut self, op: Op) -> Result<(), JournalError> {
        fs::create_dir_all(&self.dir)?;
        let mut f = fs::OpenOptions::new().create(true).append(true).open(self.dir.join("journal.jsonl"))?;
        writeln!(f, "{}", serde_json::to_string(&op).map_err(|e| JournalError::Corrupt(e.to_string()))?)?;
        self.ops.push(op);
        Ok(())
    }

    pub fn undo(&mut self, op_id: u64) -> Result<(), UndoError> {
        let idx = self.ops.iter().position(|o| o.id == op_id).ok_or(UndoError::NotFound(op_id))?;
        if self.ops[idx].undone {
            return Err(UndoError::AlreadyUndone(op_id));
        }
        let changes = self.ops[idx].changes.clone();
        // Files this undo deletes before it reaches older changes to the same path.
        let mut freed: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
        for c in changes.iter().rev() {
            check_undo(c, &freed)?;
            match c {
                Change::Created { path, .. } | Change::LinkReplaced { path, .. } => {
                    freed.insert(path.clone());
                }
                Change::Linked { link, .. } => {
                    freed.insert(link.clone());
                }
                _ => {}
            }
        }
        for c in changes.iter().rev() {
            apply_undo(c).map_err(|e| UndoError::Io(e.to_string()))?;
        }
        self.ops[idx].undone = true;
        self.save_all().map_err(|e| UndoError::Io(e.to_string()))?;
        Ok(())
    }

    /// Undo every change, newest first. Stops at the first conflict.
    pub fn undo_all(&mut self) -> Result<Vec<u64>, UndoError> {
        let mut ids: Vec<u64> = self.ops.iter().filter(|o| !o.undone).map(|o| o.id).collect();
        ids.sort_unstable_by(|a, b| b.cmp(a));
        let mut done = vec![];
        for id in ids {
            self.undo(id)?;
            done.push(id);
        }
        Ok(done)
    }
}

fn check_undo(c: &Change, freed: &std::collections::HashSet<PathBuf>) -> Result<(), UndoError> {
    let conflict = |p: &Path| Err(UndoError::Conflict { path: p.to_path_buf() });
    match c {
        Change::Created { path, after_sha256 }
        | Change::Modified { path, after_sha256, .. }
        | Change::LinkReplaced { path, after_sha256, .. } => {
            match file_sha(path) {
                Some(cur) if &cur == after_sha256 && !is_link(path) => Ok(()),
                None if !path.exists() => Ok(()), // already gone: nothing to protect
                _ => conflict(path),
            }
        }
        Change::Removed { path, .. } | Change::LinkRemoved { path, .. } => {
            if (path.exists() || is_link(path)) && !freed.contains(path) { conflict(path) } else { Ok(()) }
        }
        Change::Linked { link, .. } => {
            if link.exists() && !is_link(link) && !is_junction(link) { conflict(link) } else { Ok(()) }
        }
        Change::DirCreated { .. } | Change::UserPath { .. } => Ok(()),
        Change::DirMoved { from, .. } => {
            if from.exists() && !freed.contains(from) { conflict(from) } else { Ok(()) }
        }
    }
}

fn apply_undo(c: &Change) -> Result<(), JournalError> {
    match c {
        Change::Created { path, .. } => {
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
        Change::Modified { path, backup, .. } | Change::Removed { path, backup } => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(backup, path)?;
        }
        Change::LinkRemoved { path, target } => make_file_link(target, path)?,
        Change::LinkReplaced { path, target, .. } => {
            if path.exists() {
                fs::remove_file(path)?;
            }
            make_file_link(target, path)?;
        }
        Change::DirCreated { path } => {
            let _ = fs::remove_dir(path); // only succeeds when empty
        }
        Change::Linked { link, .. } => remove_link(link)?,
        Change::UserPath { before, .. } => set_user_path(before.as_deref())?,
        Change::DirMoved { from, to } => fs::rename(to, from)?,
    }
    Ok(())
}

pub struct Txn<'a> {
    journal: &'a mut Journal,
    id: u64,
    summary: String,
    changes: Vec<Change>,
    committed: bool,
    backups: u32,
}

impl Txn<'_> {
    pub fn id(&self) -> u64 {
        self.id
    }

    fn backup(&mut self, path: &Path) -> Result<PathBuf, JournalError> {
        let dir = self.journal.backup_dir(self.id);
        fs::create_dir_all(&dir)?;
        self.backups += 1;
        let b = dir.join(self.backups.to_string());
        fs::copy(path, &b)?;
        Ok(b)
    }

    /// Records each missing ancestor of `dir`, outermost first.
    pub fn ensure_dir(&mut self, dir: &Path) -> Result<(), JournalError> {
        let mut missing = vec![];
        let mut cur = Some(dir);
        while let Some(d) = cur {
            if d.exists() {
                break;
            }
            missing.push(d.to_path_buf());
            cur = d.parent();
        }
        for d in missing.into_iter().rev() {
            fs::create_dir(&d)?;
            self.changes.push(Change::DirCreated { path: d });
        }
        Ok(())
    }

    pub fn write_file(&mut self, path: &Path, bytes: &[u8]) -> Result<(), JournalError> {
        if is_link(path) {
            return Err(JournalError::IsLink(path.to_path_buf()));
        }
        let after = sha256_hex(bytes);
        if let Some(parent) = path.parent() {
            self.ensure_dir(parent)?;
        }
        // A file already touched in this transaction keeps its first backup.
        let existing = self.changes.iter_mut().find(|c| match c {
            Change::Created { path: p, .. } | Change::Modified { path: p, .. } | Change::LinkReplaced { path: p, .. } => p == path,
            _ => false,
        });
        if let Some(c) = existing {
            match c {
                Change::Created { after_sha256, .. }
                | Change::Modified { after_sha256, .. }
                | Change::LinkReplaced { after_sha256, .. } => *after_sha256 = after,
                _ => unreachable!(),
            }
            fs::write(path, bytes)?;
            return Ok(());
        }
        if path.exists() {
            if file_sha(path).as_deref() == Some(after.as_str()) {
                return Ok(()); // unchanged: record nothing
            }
            let backup = self.backup(path)?;
            fs::write(path, bytes)?;
            self.changes.push(Change::Modified { path: path.to_path_buf(), backup, after_sha256: after });
        } else {
            fs::write(path, bytes)?;
            self.changes.push(Change::Created { path: path.to_path_buf(), after_sha256: after });
        }
        Ok(())
    }

    #[cfg(unix)]
    pub fn set_executable(&mut self, path: &Path) -> Result<(), JournalError> {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
        Ok(())
    }

    pub fn remove_file(&mut self, path: &Path) -> Result<(), JournalError> {
        if is_link(path) {
            let target = fs::read_link(path)?;
            fs::remove_file(path)?;
            self.changes.push(Change::LinkRemoved { path: path.to_path_buf(), target });
            return Ok(());
        }
        if !path.exists() {
            return Ok(());
        }
        let backup = self.backup(path)?;
        fs::remove_file(path)?;
        self.changes.push(Change::Removed { path: path.to_path_buf(), backup });
        Ok(())
    }

    /// Links a directory: a symlink on macOS/Linux, a junction on Windows (no admin needed).
    pub fn link_dir(&mut self, link: &Path, target: &Path) -> Result<(), JournalError> {
        if let Some(parent) = link.parent() {
            self.ensure_dir(parent)?;
        }
        make_dir_link(target, link)?;
        self.changes.push(Change::Linked { link: link.to_path_buf(), target: target.to_path_buf() });
        Ok(())
    }

    /// Turns a symlinked file into a real file holding the target's bytes.
    pub fn replace_link_with_copy(&mut self, path: &Path) -> Result<(), JournalError> {
        if !is_link(path) {
            return Ok(());
        }
        let target = fs::read_link(path)?;
        let bytes = fs::read(path).unwrap_or_default();
        fs::remove_file(path)?;
        fs::write(path, &bytes)?;
        self.changes.push(Change::LinkReplaced { path: path.to_path_buf(), target, after_sha256: sha256_hex(&bytes) });
        Ok(())
    }

    /// Windows only: sets the per-user PATH, remembering the old value.
    pub fn set_windows_user_path(&mut self, value: &str) -> Result<(), JournalError> {
        let before = get_user_path()?;
        set_user_path(Some(value))?;
        self.changes.push(Change::UserPath { before, after: value.to_string() });
        Ok(())
    }

    /// Moves a whole folder into this change's backups, so deleting it can be undone.
    pub fn move_dir_to_backup(&mut self, dir: &Path) -> Result<(), JournalError> {
        if !dir.exists() {
            return Ok(());
        }
        let backups = self.journal.backup_dir(self.id);
        fs::create_dir_all(&backups)?;
        self.backups += 1;
        let to = backups.join(format!("dir-{}", self.backups));
        fs::rename(dir, &to)?;
        self.changes.push(Change::DirMoved { from: dir.to_path_buf(), to });
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn commit(mut self) -> Result<u64, JournalError> {
        self.committed = true;
        let op = Op { id: self.id, at: now(), summary: self.summary.clone(), changes: std::mem::take(&mut self.changes), undone: false };
        let id = op.id;
        self.journal.append(op)?;
        Ok(id)
    }
}

impl Drop for Txn<'_> {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        for c in self.changes.iter().rev() {
            let _ = apply_undo(c);
        }
        let _ = fs::remove_dir_all(self.journal.backup_dir(self.id));
    }
}

#[cfg(unix)]
fn make_file_link(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}
#[cfg(windows)]
fn make_file_link(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, link).or_else(|_| fs::copy(target, link).map(|_| ()))
}

#[cfg(unix)]
fn make_dir_link(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}
#[cfg(windows)]
fn make_dir_link(target: &Path, link: &Path) -> std::io::Result<()> {
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .stdout(std::process::Stdio::null())
        .status()?;
    if status.success() { Ok(()) } else { Err(std::io::Error::other("mklink /J failed")) }
}

#[cfg(unix)]
fn is_junction(_: &Path) -> bool {
    false
}
#[cfg(windows)]
fn is_junction(p: &Path) -> bool {
    fs::read_link(p).is_ok()
}

fn remove_link(link: &Path) -> std::io::Result<()> {
    if !is_link(link) && !is_junction(link) {
        return Ok(());
    }
    fs::remove_file(link).or_else(|_| fs::remove_dir(link))
}

#[cfg(windows)]
fn get_user_path() -> Result<Option<String>, JournalError> {
    let env = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).open_subkey("Environment")?;
    Ok(env.get_value::<String, _>("Path").ok())
}
#[cfg(windows)]
fn set_user_path(v: Option<&str>) -> Result<(), JournalError> {
    let (env, _) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).create_subkey("Environment")?;
    match v {
        Some(v) => env.set_value("Path", &v.to_string())?,
        None => {
            let _ = env.delete_value("Path");
        }
    }
    crate::launcher::broadcast_env_change();
    Ok(())
}
#[cfg(not(windows))]
fn get_user_path() -> Result<Option<String>, JournalError> {
    Err(JournalError::Unsupported("Changing the Windows user PATH"))
}
#[cfg(not(windows))]
fn set_user_path(_: Option<&str>) -> Result<(), JournalError> {
    Err(JournalError::Unsupported("Changing the Windows user PATH"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::{Platform, RosterPaths};
    use std::fs;

    fn setup() -> (tempfile::TempDir, RosterPaths, Journal) {
        let d = tempfile::tempdir().unwrap();
        let p = RosterPaths::for_home(d.path(), Platform::Mac, &|_| None);
        let j = Journal::open(&p).unwrap();
        (d, p, j)
    }

    #[test]
    fn write_then_undo_restores_original() {
        let (d, _p, mut j) = setup();
        let f = d.path().join("CLAUDE.md");
        fs::write(&f, "A").unwrap();
        let mut t = j.begin("edit");
        t.write_file(&f, b"B").unwrap();
        let id = t.commit().unwrap();
        assert_eq!(fs::read_to_string(&f).unwrap(), "B");
        j.undo(id).unwrap();
        assert_eq!(fs::read_to_string(&f).unwrap(), "A");
    }

    #[test]
    fn created_file_undo_deletes_it() {
        let (d, _p, mut j) = setup();
        let f = d.path().join("new/deep/file");
        let mut t = j.begin("create");
        t.write_file(&f, b"x").unwrap();
        let id = t.commit().unwrap();
        j.undo(id).unwrap();
        assert!(!f.exists());
        assert!(!d.path().join("new").exists(), "created parent dirs are removed when empty");
    }

    #[test]
    fn conflict_when_user_edited_after() {
        let (d, _p, mut j) = setup();
        let f = d.path().join("f");
        fs::write(&f, "A").unwrap();
        let mut t = j.begin("edit");
        t.write_file(&f, b"B").unwrap();
        let id = t.commit().unwrap();
        fs::write(&f, "C").unwrap();
        assert!(matches!(j.undo(id), Err(UndoError::Conflict { path }) if path == f));
        assert_eq!(fs::read_to_string(&f).unwrap(), "C");
    }

    #[test]
    fn undo_all_newest_first() {
        let (d, _p, mut j) = setup();
        let mut t = j.begin("one");
        t.write_file(&d.path().join("x"), b"1").unwrap();
        t.commit().unwrap();
        let mut t = j.begin("two");
        t.write_file(&d.path().join("y"), b"2").unwrap();
        t.commit().unwrap();
        assert_eq!(j.undo_all().unwrap(), vec![2, 1]);
        assert!(!d.path().join("x").exists() && !d.path().join("y").exists());
    }

    #[test]
    fn uncommitted_txn_rolls_back_on_drop() {
        let (d, _p, mut j) = setup();
        let f = d.path().join("f");
        fs::write(&f, "A").unwrap();
        {
            let mut t = j.begin("abandoned");
            t.write_file(&f, b"B").unwrap();
            t.write_file(&d.path().join("g"), b"G").unwrap();
        }
        assert_eq!(fs::read_to_string(&f).unwrap(), "A");
        assert!(!d.path().join("g").exists());
        assert!(j.ops().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn replace_link_with_copy_keeps_target_untouched() {
        let (d, _p, mut j) = setup();
        let target = d.path().join("target.md");
        let link = d.path().join("link.md");
        fs::write(&target, "T").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let mut t = j.begin("unlink");
        t.replace_link_with_copy(&link).unwrap();
        t.write_file(&link, b"N").unwrap();
        let id = t.commit().unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "T");
        assert_eq!(fs::read_to_string(&link).unwrap(), "N");
        j.undo(id).unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink(), "undo restores the link");
    }

    #[cfg(unix)]
    #[test]
    fn write_through_a_link_is_refused() {
        let (d, _p, mut j) = setup();
        let target = d.path().join("t");
        let link = d.path().join("l");
        fs::write(&target, "T").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let mut t = j.begin("x");
        assert!(matches!(t.write_file(&link, b"N"), Err(JournalError::IsLink(_))));
    }

    #[test]
    fn remove_file_undo_restores_it() {
        let (d, _p, mut j) = setup();
        let f = d.path().join("old");
        fs::write(&f, "keep me").unwrap();
        let mut t = j.begin("remove");
        t.remove_file(&f).unwrap();
        let id = t.commit().unwrap();
        assert!(!f.exists());
        j.undo(id).unwrap();
        assert_eq!(fs::read_to_string(&f).unwrap(), "keep me");
    }

    #[test]
    fn journal_persists_across_open() {
        let (d, p, mut j) = setup();
        let mut t = j.begin("persist");
        t.write_file(&d.path().join("x"), b"1").unwrap();
        t.commit().unwrap();
        let j2 = Journal::open(&p).unwrap();
        assert_eq!(j2.ops().len(), 1);
        assert_eq!(j2.ops()[0].summary, "persist");
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;
    use crate::paths::{Platform, RosterPaths};

    #[test]
    fn undo_remove_then_recreate_in_one_op() {
        let d = tempfile::tempdir().unwrap();
        let p = RosterPaths::for_home(d.path(), Platform::Mac, &|_| None);
        let mut j = Journal::open(&p).unwrap();
        let f = d.path().join("f");
        std::fs::write(&f, "old").unwrap();
        let mut t = j.begin("swap");
        t.remove_file(&f).unwrap();
        t.write_file(&f, b"new").unwrap();
        let id = t.commit().unwrap();
        j.undo(id).unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "old");
    }
}
