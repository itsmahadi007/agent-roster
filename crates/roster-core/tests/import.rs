#![cfg(unix)]
use roster_core::import::{detect_prototype, plan_import};
use roster_core::journal::Journal;
use roster_core::model::{LimitsSource, Roster, Tool};
use roster_core::names::CommandLookup;
use roster_core::ops::{Ctx, apply};
use roster_core::paths::{Platform, RosterPaths};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

struct NoCmds;
impl CommandLookup for NoCmds {
    fn find(&self, _: &str) -> Option<PathBuf> {
        None
    }
}

const OLD_SHIM: &str = "#!/usr/bin/env bash\n# generated-by-agent-roster\n# agent1 -> claude using /h/.claude\nunset CLAUDECODE\nexec claude \"$@\"\n";

fn mahadi_home() -> (tempfile::TempDir, RosterPaths) {
    let d = tempfile::tempdir().unwrap();
    let h = d.path();
    let p = RosterPaths::for_home(h, Platform::current(), &|_| None);
    fs::create_dir_all(&p.config_dir).unwrap();
    fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/prototype/roster.conf"), p.config_dir.join("roster.conf")).unwrap();
    for dir in [".claude/skills", ".claude/hooks", ".codex/skills", ".claude-2", ".codex-2", ".local/bin"] {
        fs::create_dir_all(h.join(dir)).unwrap();
    }
    fs::write(h.join(".claude/CLAUDE.md"), "# Global rules\n\n<!-- agent-roster:begin -->\nold\n<!-- agent-roster:end -->\n").unwrap();
    fs::write(h.join(".claude/settings.json"), "{\"model\":\"opus\"}").unwrap();
    fs::write(h.join(".codex/config.toml"), "model = \"x\"\n").unwrap();
    for item in ["CLAUDE.md", "settings.json", "hooks", "skills"] {
        symlink(h.join(".claude").join(item), h.join(".claude-2").join(item)).unwrap();
    }
    for item in ["config.toml", "skills"] {
        symlink(h.join(".codex").join(item), h.join(".codex-2").join(item)).unwrap();
    }
    for n in ["agent1", "agent2", "agent3", "agent4"] {
        fs::write(h.join(".local/bin").join(n), OLD_SHIM).unwrap();
    }
    fs::write(h.join(".local/bin/agent-roster"), "#!/usr/bin/env bash\n# agent-roster — named launchers for several Claude Code / Codex accounts on one Mac.\n").unwrap();
    fs::write(h.join(".local/bin/mytool"), "#!/bin/sh\necho mine\n").unwrap();
    (d, p)
}

fn ctx(p: &RosterPaths) -> Ctx<'static> {
    Ctx { lookup: &NoCmds, path_var: p.bin_dir().display().to_string(), roster_bin: PathBuf::from("/h/.local/bin/roster") }
}

fn run_import(p: &RosterPaths) -> (Journal, u64) {
    let mut j = Journal::open(p).unwrap();
    let c = ctx(p);
    let plan = plan_import(&Roster::load(p).unwrap(), p, &c).unwrap();
    let id = apply(plan, p, &mut j, &c).unwrap().unwrap();
    (j, id)
}

#[test]
fn detects_prototype() {
    let (_d, p) = mahadi_home();
    assert!(detect_prototype(&p).is_some());
}

#[test]
fn imports_four_agents_in_place() {
    let (_d, p) = mahadi_home();
    run_import(&p);
    let r = Roster::load(&p).unwrap();
    let names: Vec<&str> = r.agents.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["agent1", "agent2", "agent3", "agent4"]);
    let dirs: Vec<&str> = r.agents.iter().map(|a| a.config_dir.as_str()).collect();
    assert_eq!(dirs, vec!["~/.claude", "~/.claude-2", "~/.codex", "~/.codex-2"]);
    assert!(r.by_name("agent1").unwrap().default && r.by_name("agent3").unwrap().default);
    assert!(!r.by_name("agent2").unwrap().default && !r.by_name("agent4").unwrap().default);
    assert_eq!(r.by_name("agent4").unwrap().tool, Tool::Codex);
    assert_eq!(r.by_name("agent3").unwrap().limits, LimitsSource::CodexSessions);
    assert!(r.agents.iter().all(|a| a.role.is_none()), "\"default `claude`\" notes are not roles");
}

#[test]
fn settings_link_kept_claude_md_link_replaced() {
    let (d, p) = mahadi_home();
    run_import(&p);
    let h = d.path();
    let is_link = |x: &Path| fs::symlink_metadata(x).unwrap().file_type().is_symlink();
    assert!(is_link(&h.join(".claude-2/settings.json")));
    assert!(is_link(&h.join(".claude-2/hooks")));
    assert!(is_link(&h.join(".claude-2/skills")));
    assert!(is_link(&h.join(".codex-2/config.toml")));
    assert!(!is_link(&h.join(".claude-2/CLAUDE.md")));
    let r = Roster::load(&p).unwrap();
    let a2 = r.by_name("agent2").unwrap();
    assert!(a2.share);
    assert_eq!(a2.advanced_links, vec!["settings.json".to_string(), "hooks".to_string()]);
    let sec = fs::read_to_string(h.join(".claude-2/CLAUDE.md")).unwrap();
    assert!(sec.contains("You are `agent2`") && sec.contains("@~/.claude/CLAUDE.md"));
    assert_eq!(sec.matches("# Global rules").count(), 0, "rules come through the import line, not a copy");
    let main = fs::read_to_string(h.join(".claude/CLAUDE.md")).unwrap();
    assert!(main.starts_with("# Global rules\n") && main.contains("You are `agent1`") && !main.contains("\nold\n"));
}

#[test]
fn old_launchers_removed_with_backup_and_undo_restores_them() {
    let (d, p) = mahadi_home();
    let h = d.path();
    let (mut j, op) = run_import(&p);
    for n in ["agent1", "agent2", "agent3", "agent4", "agent-roster"] {
        assert!(!h.join(".local/bin").join(n).exists(), "{n}");
    }
    assert!(h.join(".local/bin/mytool").exists());
    assert!(p.bin_dir().join("agent1").exists());
    assert!(p.config_dir.join("roster.conf.imported").exists() && !p.config_dir.join("roster.conf").exists());
    j.undo(op).unwrap();
    assert_eq!(fs::read_to_string(h.join(".local/bin/agent1")).unwrap(), OLD_SHIM);
    assert!(fs::symlink_metadata(h.join(".claude-2/CLAUDE.md")).unwrap().file_type().is_symlink());
    assert!(p.config_dir.join("roster.conf").exists());
    assert!(Roster::load(&p).unwrap().agents.is_empty());
}

#[test]
fn import_refused_when_roster_not_empty() {
    let (_d, p) = mahadi_home();
    run_import(&p);
    assert!(plan_import(&Roster::load(&p).unwrap(), &p, &ctx(&p)).is_err());
}
