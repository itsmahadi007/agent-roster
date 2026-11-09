use roster_core::journal::Journal;
use roster_core::model::{AgentColor, Roster, Tool};
use roster_core::names::CommandLookup;
use roster_core::ops::{AddRequest, Ctx, apply, plan_add, plan_adopt, plan_remove, plan_rename, plan_set_default, plan_set_share};
use roster_core::paths::{Platform, RosterPaths};
use std::fs;
use std::path::{Path, PathBuf};

struct NoCmds;
impl CommandLookup for NoCmds {
    fn find(&self, _: &str) -> Option<PathBuf> {
        None
    }
}

struct World {
    _d: tempfile::TempDir,
    p: RosterPaths,
    j: Journal,
}

impl World {
    fn new() -> World {
        let d = tempfile::tempdir().unwrap();
        let p = RosterPaths::for_home(d.path(), Platform::current(), &|_| None);
        fs::create_dir_all(d.path().join(".claude")).unwrap();
        fs::write(d.path().join(".claude/CLAUDE.md"), "# My rules\n").unwrap();
        fs::create_dir_all(d.path().join(".claude/skills/tidy")).unwrap();
        fs::create_dir_all(d.path().join(".codex")).unwrap();
        let j = Journal::open(&p).unwrap();
        World { _d: d, p, j }
    }
    fn home(&self) -> &Path {
        &self.p.home
    }
    fn ctx(&self) -> Ctx<'static> {
        Ctx { lookup: &NoCmds, path_var: self.p.bin_dir().display().to_string(), roster_bin: PathBuf::from("/usr/local/bin/roster") }
    }
    fn roster(&self) -> Roster {
        Roster::load(&self.p).unwrap()
    }
    fn adopt_both(&mut self) {
        let ctx = self.ctx();
        let plan = plan_adopt(&self.roster(), &self.p, &ctx, Tool::Claude, "agent1").unwrap();
        apply(plan, &self.p, &mut self.j, &ctx).unwrap();
        let plan = plan_adopt(&self.roster(), &self.p, &ctx, Tool::Codex, "agent3").unwrap();
        apply(plan, &self.p, &mut self.j, &ctx).unwrap();
    }
    fn add(&mut self, name: &str, tool: Tool, share: bool) {
        let ctx = self.ctx();
        let req = AddRequest { name: name.into(), tool, color: None, role: None, share };
        let plan = plan_add(&self.roster(), &self.p, &ctx, req).unwrap();
        apply(plan, &self.p, &mut self.j, &ctx).unwrap();
    }
    fn shim(&self, name: &str) -> PathBuf {
        self.p.bin_dir().join(if cfg!(windows) { format!("{name}.cmd") } else { name.to_string() })
    }
}

#[test]
fn adopt_marks_default_in_place() {
    let mut w = World::new();
    w.adopt_both();
    let r = w.roster();
    assert_eq!(r.agents.len(), 2);
    assert_eq!(r.agents[0].config_dir, "~/.claude");
    assert!(r.agents[0].default && r.agents[1].default);
    assert!(fs::read_to_string(w.home().join(".claude/CLAUDE.md")).unwrap().starts_with("# My rules\n"));
}

#[test]
fn add_creates_folder_shim_note_and_one_op() {
    let mut w = World::new();
    w.adopt_both();
    let before = w.j.ops().len();
    w.add("boss", Tool::Codex, false);
    let r = w.roster();
    let boss = r.by_name("boss").unwrap();
    assert!(w.p.accounts_dir().join(&boss.id).is_dir());
    assert!(w.shim("boss").exists());
    assert!(fs::read_to_string(w.home().join(".claude/CLAUDE.md")).unwrap().contains("| boss | Codex |"));
    assert!(fs::read_to_string(w.p.accounts_dir().join(&boss.id).join("AGENTS.md")).unwrap().contains("You are `boss`"));
    assert_eq!(w.j.ops().len(), before + 1);
    assert_eq!(boss.color, AgentColor::Jade, "third agent gets the third color");
}

#[test]
fn add_preview_text_exact() {
    let mut w = World::new();
    w.adopt_both();
    let req = AddRequest { name: "boss".into(), tool: Tool::Codex, color: None, role: None, share: false };
    let plan = plan_add(&w.roster(), &w.p, &w.ctx(), req).unwrap();
    assert_eq!(
        plan.steps.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(),
        vec![
            "Adds a boss command for every terminal.",
            "Creates a private folder for this account.",
            "Tells your other agents that boss exists.",
            "Nothing else on your computer changes."
        ]
    );
}

#[test]
fn add_preview_mentions_path_when_needed() {
    let w = World::new();
    let mut ctx = w.ctx();
    ctx.path_var = "/usr/bin".into();
    let req = AddRequest { name: "boss".into(), tool: Tool::Codex, color: None, role: None, share: false };
    let plan = plan_add(&w.roster(), &w.p, &ctx, req).unwrap();
    assert!(plan.steps.iter().any(|s| s.text.contains("PATH")), "{:?}", plan.steps);
}

#[test]
fn rename_swaps_launcher_keeps_folder_and_id() {
    let mut w = World::new();
    w.adopt_both();
    w.add("worker1", Tool::Claude, false);
    let id = w.roster().by_name("worker1").unwrap().id.clone();
    let ctx = w.ctx();
    let plan = plan_rename(&w.roster(), &w.p, &ctx, "worker1", "designer").unwrap();
    apply(plan, &w.p, &mut w.j, &ctx).unwrap();
    assert!(!w.shim("worker1").exists());
    assert!(fs::read_to_string(w.shim("designer")).unwrap().contains(&format!("run {id} --")));
    assert!(w.p.accounts_dir().join(&id).is_dir());
    assert_eq!(w.roster().by_id(&id).unwrap().name, "designer");
}

#[test]
fn remove_keeps_login_folder_by_default_and_drops_note_row() {
    let mut w = World::new();
    w.adopt_both();
    w.add("boss", Tool::Codex, false);
    let id = w.roster().by_name("boss").unwrap().id.clone();
    let ctx = w.ctx();
    let plan = plan_remove(&w.roster(), &w.p, &ctx, "boss", false).unwrap();
    apply(plan, &w.p, &mut w.j, &ctx).unwrap();
    assert!(w.p.accounts_dir().join(&id).is_dir());
    assert!(!w.shim("boss").exists());
    assert!(!fs::read_to_string(w.home().join(".claude/CLAUDE.md")).unwrap().contains("| boss |"));
    assert!(!fs::read_to_string(w.p.accounts_dir().join(&id).join("AGENTS.md")).unwrap().contains("agent-roster:begin"));
}

#[test]
fn removing_an_adopted_folder_is_refused() {
    let mut w = World::new();
    w.adopt_both();
    assert!(plan_remove(&w.roster(), &w.p, &w.ctx(), "agent1", true).is_err());
}

#[cfg(unix)]
#[test]
fn share_on_links_skills_not_settings() {
    let mut w = World::new();
    w.adopt_both();
    fs::write(w.home().join(".claude/settings.json"), "{}").unwrap();
    w.add("worker1", Tool::Claude, true);
    let dir = w.p.accounts_dir().join(&w.roster().by_name("worker1").unwrap().id);
    assert!(fs::symlink_metadata(dir.join("skills")).unwrap().file_type().is_symlink());
    assert!(!dir.join("settings.json").exists());
    assert!(fs::read_to_string(dir.join("CLAUDE.md")).unwrap().contains("@~/.claude/CLAUDE.md"));
    let ctx = w.ctx();
    let plan = plan_set_share(&w.roster(), &w.p, &ctx, "worker1", false).unwrap();
    apply(plan, &w.p, &mut w.j, &ctx).unwrap();
    assert!(!dir.join("skills").exists());
    assert!(!fs::read_to_string(dir.join("CLAUDE.md")).unwrap().contains("@~/.claude/CLAUDE.md"));
}

#[test]
fn undo_add_restores_everything() {
    let mut w = World::new();
    w.adopt_both();
    let claude_md = fs::read(w.home().join(".claude/CLAUDE.md")).unwrap();
    w.add("boss", Tool::Codex, false);
    let id = w.roster().by_name("boss").unwrap().id.clone();
    let op = w.j.ops().last().unwrap().id;
    w.j.undo(op).unwrap();
    assert!(!w.shim("boss").exists());
    assert_eq!(fs::read(w.home().join(".claude/CLAUDE.md")).unwrap(), claude_md);
    assert!(w.roster().by_name("boss").is_none());
    assert!(!w.p.accounts_dir().join(&id).exists());
}

#[test]
fn default_secondary_writes_interactive_function_only() {
    let mut w = World::new();
    w.adopt_both();
    w.add("worker1", Tool::Claude, false);
    let ctx = w.ctx();
    let plan = plan_set_default(&w.roster(), &w.p, &ctx, "worker1").unwrap();
    apply(plan, &w.p, &mut w.j, &ctx).unwrap();
    let r = w.roster();
    assert!(r.by_name("worker1").unwrap().default && !r.by_name("agent1").unwrap().default);
    if !cfg!(windows) {
        let rc = fs::read_to_string(w.home().join(if cfg!(target_os = "macos") { ".zshrc" } else { ".bashrc" })).unwrap();
        assert!(rc.contains("claude() { worker1 \"$@\"; }"));
    }
    assert!(!w.p.bin_dir().join("claude").exists());
    let ctx = w.ctx();
    let plan = plan_set_default(&w.roster(), &w.p, &ctx, "agent1").unwrap();
    apply(plan, &w.p, &mut w.j, &ctx).unwrap();
    if !cfg!(windows) {
        let rc = fs::read_to_string(w.home().join(if cfg!(target_os = "macos") { ".zshrc" } else { ".bashrc" })).unwrap();
        assert!(!rc.contains("claude()"));
    }
}
