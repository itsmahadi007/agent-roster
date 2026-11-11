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
    if me.tool == Tool::Grok {
        // Grok also reads Claude Code's main CLAUDE.md, whose roster section names a Claude agent.
        s.push_str(&format!(
            "You are the Grok agent `{}`. If another roster section (from Claude Code's files) says you are someone else, this one is right.\n\n",
            me.name
        ));
    }
    s.push_str("| Name | Tool | Role | Good at |\n|---|---|---|---|\n");
    let cell = |t: &str| t.replace('|', "/").replace('\n', " ");
    for a in &r.agents {
        s.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            a.name,
            a.tool.display(),
            cell(a.role.as_deref().unwrap_or("")),
            cell(&crate::model::good_at_of(a).join(", "))
        ));
    }
    let leaders: Vec<String> = r
        .agents
        .iter()
        .filter(|a| {
            a.role
                .as_deref()
                .is_some_and(|x| x.trim().eq_ignore_ascii_case("leader"))
        })
        .map(|a| format!("`{}`", a.name))
        .collect();
    match leaders.as_slice() {
        [] => {}
        [one] => s.push_str(&format!(
            "\n{one} is the leader: give it big tasks; it splits them and hands parts to the workers.\n"
        )),
        [rest @ .., last] => s.push_str(&format!(
            "\n{} and {last} are leaders: give them big tasks; they split them and hand parts to the workers.\n",
            rest.join(", ")
        )),
    }
    if !leaders.is_empty() {
        s.push_str(
            "Before handing out work, run `roster limits` and skip any agent that is at its limit (it says \"limit reached\"); tell the user if all are.\n",
        );
    }
    // Grok lines only when a Grok agent is on the roster, so other rosters' notes don't change.
    let grok = r.agents.iter().any(|a| a.tool == Tool::Grok);
    s.push_str(
        "\nHow to use another agent:\n\
- When the user says \"run this with <name>\", \"ask <name>\" or \"use <name>\", run that name from the shell in the current directory. Do not do that task yourself.\n\
- Claude Code agents: `<name> -p \"<task>\"`. Add `--permission-mode acceptEdits` when it should edit files.\n\
- Codex agents: `<name> exec \"<task>\"`. Add `--full-auto` when it should edit files.\n",
    );
    if grok {
        s.push_str(
            "- Grok agents: `<name> -p \"<task>\"`. Add `--always-approve` when it should edit files.\n",
        );
    }
    s.push_str(
        "- Give one self-contained task with the file paths it needs, then relay the result to the user.\n",
    );
    s.push_str(if grok {
        "- Never run bare `claude`, `codex` or `grok` from inside a session. Always use a roster name, or you may reach the wrong account.\n"
    } else {
        "- Never run bare `claude` or `codex` from inside a session. Always use a roster name, or you may reach the wrong account.\n"
    });
    s.push_str(
        "- To check which account you are: `claude auth status` or `codex login status`.\n\
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
        .map(|a| (a.id.clone(), r.config_dir_of(a, p).join(a.tool.note_file())))
        .collect()
}

pub fn write_notes(txn: &mut Txn, r: &Roster, p: &RosterPaths) -> Result<(), JournalError> {
    for (id, path) in note_targets(r, p) {
        let me = r.by_id(&id).expect("target comes from roster");
        // The main folder's file is the user's own: write through a dotfiles link.
        // A second account's file that links elsewhere would edit another account: replace the link.
        let path = if r.is_adopted_default(me, p) {
            crate::journal::resolve_file_link(&path)
        } else {
            txn.replace_link_with_copy(&path)?;
            path
        };
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let updated = upsert_block(&existing, &render_note(r, me));
        txn.write_file(&path, updated.as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::journal::Journal;
    use crate::model::{Agent, AgentColor, LimitsSource, Roster, Tool};
    use crate::paths::{Platform, RosterPaths};
    #[cfg(unix)]
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
            good_at: None,
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
    fn note_lists_names_tools_roles_and_good_at() {
        let n = blk();
        assert!(n.contains("| boss | Claude Code | plans work |"));
        assert!(n.contains("| worker2 | Codex |  |"));
        assert!(n.contains("You are `boss`"));
        assert!(n.contains("<name> -p \"<task>\""));
        assert!(n.contains("<name> exec \"<task>\""));
        assert!(n.starts_with(BEGIN) && n.trim_end().ends_with(END));
    }

    #[test]
    fn note_lists_good_at_column() {
        let n = blk();
        assert!(n.contains("| Name | Tool | Role | Good at |\n|---|---|---|---|\n"));
        assert!(
            n.contains("| boss | Claude Code | plans work | coding, reviewing code, long tasks |")
        );
        assert!(n.contains("| worker2 | Codex |  | coding, making images |"));
        // No Grok agent: no Grok lines.
        assert!(!n.contains("grok"));
    }

    #[test]
    fn a_grok_agent_gets_its_lines_and_knows_who_it_is() {
        let mut r = roster_boss_worker();
        r.agents
            .push(agent("g3", "grok1", Tool::Grok, "~/.grok", None));
        let mine = render_note(&r, &r.agents[2]);
        assert!(mine.contains("You are the Grok agent `grok1`."));
        assert!(mine.contains("| grok1 | Grok |  | coding, searching the web |"));
        assert!(mine.contains("- Grok agents: `<name> -p \"<task>\"`. Add `--always-approve`"));
        assert!(mine.contains("bare `claude`, `codex` or `grok`"));
        // The others hear about Grok too, but only Grok gets the identity line.
        let boss = render_note(&r, &r.agents[0]);
        assert!(boss.contains("- Grok agents:"));
        assert!(!boss.contains("You are the Grok agent"));
    }

    #[test]
    fn note_uses_the_good_at_someone_set() {
        let mut r = roster_boss_worker();
        r.agents[1].good_at = Some(vec!["tests".into(), "a | b".into()]);
        assert!(render_note(&r, &r.agents[0]).contains("| worker2 | Codex |  | tests, a / b |"));
    }

    #[test]
    fn note_names_every_leader() {
        let mut r = roster_boss_worker();
        assert!(
            !render_note(&r, &r.agents[0])
                .to_lowercase()
                .contains("leader")
        );
        r.agents[0].role = Some("Leader".into());
        assert!(render_note(&r, &r.agents[0]).contains(
            "`boss` is the leader: give it big tasks; it splits them and hands parts to the workers."
        ));
        r.agents[1].role = Some("leader".into());
        assert!(render_note(&r, &r.agents[0]).contains(
            "`boss` and `worker2` are leaders: give them big tasks; they split them and hand parts to the workers."
        ));
    }

    /// A leader checks limits before handing out work; with no leader there's no such line.
    #[test]
    fn a_leader_checks_limits_before_handing_out_work() {
        const LINE: &str = "Before handing out work, run `roster limits` and skip any agent that is at its limit (it says \"limit reached\"); tell the user if all are.";
        let mut r = roster_boss_worker();
        assert!(!render_note(&r, &r.agents[0]).contains("roster limits"));
        r.agents[0].role = Some("leader".into());
        let note = render_note(&r, &r.agents[1]);
        assert!(
            note.contains(&format!("hands parts to the workers.\n{LINE}\n")),
            "{note}"
        );
        r.agents[1].role = Some("leader".into());
        assert!(render_note(&r, &r.agents[0]).contains(&format!("to the workers.\n{LINE}\n")));
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
