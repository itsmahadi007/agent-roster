//! The roster note: the section in each agent's instructions that says who else is on the roster.

use crate::journal::{JournalError, Txn};
use crate::model::{Agent, Roster, Tool};
use crate::paths::RosterPaths;
use std::path::PathBuf;

pub const BEGIN: &str = "<!-- agent-roster:begin -->";
pub const END: &str = "<!-- agent-roster:end -->";

/// The full marked section for one agent's instruction file.
pub fn render_note(r: &Roster, me: &Agent) -> String {
    let mut s = String::new();
    s.push_str(BEGIN);
    s.push_str("\n## Agent roster\n\n");
    s.push_str(&format!(
        "You are `{}`. These are the other AI accounts on this computer. Each one has its own login and its own limits.\n\n",
        me.name
    ));
    s.push_str("| Name | Tool | Role |\n|---|---|---|\n");
    for a in &r.agents {
        let role = a
            .role
            .as_deref()
            .unwrap_or("")
            .replace('|', "/")
            .replace('\n', " ");
        s.push_str(&format!(
            "| {} | {} | {} |\n",
            a.name,
            a.tool.display(),
            role
        ));
    }
    s.push_str(
        "\nHow to use another agent:\n\
- When the user says \"run this with <name>\", \"ask <name>\" or \"use <name>\", run that name from the shell in the current directory. Do not do that task yourself.\n\
- Claude Code agents: `<name> -p \"<task>\"`. Add `--permission-mode acceptEdits` when it should edit files.\n\
- Codex agents: `<name> exec \"<task>\"`. Add `--full-auto` when it should edit files.\n\
- Give one self-contained task with the file paths it needs, then relay the result to the user.\n\
- Never run bare `claude` or `codex` from inside a session. Always use a roster name, or you may reach the wrong account.\n\
- To check which account you are: `claude auth status` or `codex login status`.\n\
- The `roster` command manages this list. Agent Roster rewrites this section, so don't edit it by hand.\n",
    );
    s.push_str(END);
    s.push('\n');
    s
}

fn block_range(text: &str) -> Option<(usize, usize)> {
    let start = text.find(BEGIN)?;
    let end_rel = text[start..].find(END)?;
    let mut end = start + end_rel + END.len();
    if text[end..].starts_with("\r\n") {
        end += 2;
    } else if text[end..].starts_with('\n') {
        end += 1;
    }
    Some((start, end))
}

/// Puts `block` in place of an existing marked section, or appends it.
pub fn upsert_block(existing: &str, block: &str) -> String {
    if let Some((start, end)) = block_range(existing) {
        return format!("{}{}{}", &existing[..start], block, &existing[end..]);
    }
    let trimmed = existing.trim_end();
    if trimmed.is_empty() {
        block.to_string()
    } else {
        format!("{trimmed}\n\n{block}")
    }
}

/// Removes the marked section and the blank line before it.
pub fn remove_block(existing: &str) -> String {
    match block_range(existing) {
        Some((start, end)) => {
            let before = existing[..start].trim_end_matches(['\n', '\r']);
            let after = &existing[end..];
            let mut out = String::from(before);
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(after);
            out
        }
        None => existing.to_string(),
    }
}

/// The instruction file each agent reads: `CLAUDE.md` or `AGENTS.md` inside its config folder.
pub fn note_targets(r: &Roster, p: &RosterPaths) -> Vec<(String, PathBuf)> {
    r.agents
        .iter()
        .map(|a| {
            let file = match a.tool {
                Tool::Claude => "CLAUDE.md",
                Tool::Codex => "AGENTS.md",
            };
            (a.id.clone(), r.config_dir_of(a, p).join(file))
        })
        .collect()
}

pub fn write_notes(txn: &mut Txn, r: &Roster, p: &RosterPaths) -> Result<(), JournalError> {
    for (id, path) in note_targets(r, p) {
        let me = r.by_id(&id).expect("target comes from roster");
        txn.replace_link_with_copy(&path)?;
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let updated = upsert_block(&existing, &render_note(r, me));
        txn.write_file(&path, updated.as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::journal::Journal;
    use crate::model::{Agent, AgentColor, LimitsSource, Roster, Tool};
    use crate::paths::{Platform, RosterPaths};
    use std::fs;

    pub(crate) fn agent(id: &str, name: &str, tool: Tool, dir: &str, role: Option<&str>) -> Agent {
        Agent {
            id: id.into(),
            name: name.into(),
            tool,
            config_dir: dir.into(),
            color: AgentColor::Cobalt,
            role: role.map(String::from),
            default: dir == "~/.claude" || dir == "~/.codex",
            share: false,
            limits: LimitsSource::Off,
            advanced_links: vec![],
        }
    }

    fn roster_boss_worker() -> Roster {
        Roster {
            version: 1,
            agents: vec![
                agent("a1", "boss", Tool::Claude, "~/.claude", Some("plans work")),
                agent("b2", "worker2", Tool::Codex, "~/.codex", None),
            ],
            statusline: vec![],
        }
    }

    fn paths(home: &std::path::Path) -> RosterPaths {
        RosterPaths::for_home(home, Platform::Mac, &|_| None)
    }

    fn blk() -> String {
        render_note(&roster_boss_worker(), &roster_boss_worker().agents[0])
    }

    #[test]
    fn inserts_once_and_is_idempotent() {
        let a = upsert_block("# Rules\n", &blk());
        assert_eq!(upsert_block(&a, &blk()), a);
        assert_eq!(a.matches(BEGIN).count(), 1);
        assert!(a.starts_with("# Rules\n"));
    }

    #[test]
    fn preserves_text_outside_block() {
        let first = upsert_block("before\n", &blk());
        let with_after = format!("{first}\nafter\n");
        let replaced = upsert_block(
            &with_after,
            &render_note(&roster_boss_worker(), &roster_boss_worker().agents[1]),
        );
        assert!(replaced.starts_with("before\n"));
        assert!(replaced.ends_with("after\n"));
        assert!(replaced.contains("You are `worker2`"));
    }

    #[test]
    fn replaces_prototype_block() {
        let old = include_str!("../tests/fixtures/prototype-claude-md.md");
        let new = upsert_block(old, &blk());
        assert!(!new.contains("oldname"));
        assert!(new.contains("| boss | Claude Code | plans work |"));
        assert!(new.contains("- Keep replies short."));
        assert_eq!(new.matches(BEGIN).count(), 1);
    }

    #[test]
    fn note_lists_names_tools_roles_and_no_capabilities() {
        let n = blk();
        assert!(n.contains("| boss | Claude Code | plans work |"));
        assert!(n.contains("| worker2 | Codex |  |"));
        assert!(n.contains("You are `boss`"));
        assert!(n.contains("<name> -p \"<task>\""));
        assert!(n.contains("<name> exec \"<task>\""));
        assert!(!n.to_lowercase().contains("image"));
        assert!(n.starts_with(BEGIN) && n.trim_end().ends_with(END));
    }

    #[test]
    fn pipe_in_role_does_not_break_table() {
        let mut r = roster_boss_worker();
        r.agents[0].role = Some("plans | reviews".into());
        assert!(render_note(&r, &r.agents[0]).contains("| boss | Claude Code | plans / reviews |"));
    }

    #[test]
    fn remove_block_leaves_rest() {
        let with = upsert_block("# Rules\n", &blk());
        assert_eq!(remove_block(&with), "# Rules\n");
    }

    #[test]
    fn targets_follow_tool_and_folder() {
        let d = tempfile::tempdir().unwrap();
        let p = paths(d.path());
        let mut r = roster_boss_worker();
        r.agents.push(agent(
            "c3",
            "worker1",
            Tool::Claude,
            "~/.agent-roster/accounts/c3",
            None,
        ));
        let t = note_targets(&r, &p);
        assert_eq!(t[0].1, d.path().join(".claude/CLAUDE.md"));
        assert_eq!(t[1].1, d.path().join(".codex/AGENTS.md"));
        assert_eq!(t[2].1, d.path().join(".agent-roster/accounts/c3/CLAUDE.md"));
    }

    #[cfg(unix)]
    #[test]
    fn linked_note_file_is_replaced_not_followed() {
        let d = tempfile::tempdir().unwrap();
        let p = paths(d.path());
        fs::create_dir_all(d.path().join(".claude")).unwrap();
        fs::create_dir_all(d.path().join(".claude-2")).unwrap();
        let primary = d.path().join(".claude/CLAUDE.md");
        fs::write(&primary, "# Mine\n").unwrap();
        std::os::unix::fs::symlink(&primary, d.path().join(".claude-2/CLAUDE.md")).unwrap();
        let r = Roster {
            version: 1,
            agents: vec![
                agent("a1", "agent1", Tool::Claude, "~/.claude", None),
                agent("a2", "agent2", Tool::Claude, "~/.claude-2", None),
            ],
            statusline: vec![],
        };
        let mut j = Journal::open(&p).unwrap();
        let mut t = j.begin("notes");
        write_notes(&mut t, &r, &p).unwrap();
        t.commit().unwrap();
        let prim = fs::read_to_string(&primary).unwrap();
        let sec = fs::read_to_string(d.path().join(".claude-2/CLAUDE.md")).unwrap();
        assert!(prim.contains("You are `agent1`") && !prim.contains("You are `agent2`"));
        assert!(sec.contains("You are `agent2`"));
        assert!(
            !fs::symlink_metadata(d.path().join(".claude-2/CLAUDE.md"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
}
