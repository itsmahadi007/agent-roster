//! "Share my skills": links a secondary agent to its tool's main folder.

use crate::journal::{JournalError, Txn};
use crate::model::{Roster, Tool};
use crate::note::remove_block;
use crate::paths::{RosterPaths, collapse_tilde, expand_tilde};
use std::path::Path;

pub const SHARED_BEGIN: &str = "<!-- agent-roster:shared-begin -->";
pub const SHARED_END: &str = "<!-- agent-roster:shared-end -->";

fn shared_dirs(tool: Tool) -> &'static [&'static str] {
    match tool {
        Tool::Claude => &["skills", "agents", "commands"],
        Tool::Codex | Tool::Grok => &["skills"],
    }
}

fn is_link(p: &Path) -> bool {
    std::fs::symlink_metadata(p)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// Puts the shared section at the top of the file, or removes it when `block` is `None`.
pub fn upsert_shared(existing: &str, block: Option<&str>) -> String {
    let range = existing.find(SHARED_BEGIN).and_then(|s| {
        let e = existing[s..].find(SHARED_END)? + s + SHARED_END.len();
        let e = if existing[e..].starts_with('\n') {
            e + 1
        } else {
            e
        };
        Some((s, e))
    });
    let without = match range {
        Some((s, e)) => format!(
            "{}{}",
            &existing[..s],
            existing[e..].trim_start_matches('\n')
        ),
        None => existing.to_string(),
    };
    match block {
        Some(b) if without.trim().is_empty() => b.to_string(),
        Some(b) => format!("{b}\n{without}"),
        None => without,
    }
}

pub fn apply_share(txn: &mut Txn, r: &Roster, p: &RosterPaths) -> Result<(), JournalError> {
    for a in &r.agents {
        if r.is_adopted_default(a, p) {
            continue;
        }
        let dir = r.config_dir_of(a, p);
        let primary = expand_tilde(a.tool.standard_dir(), &p.home);
        for name in shared_dirs(a.tool) {
            if a.advanced_links.iter().any(|l| l == name) {
                continue; // a link the user manages
            }
            let link = dir.join(name);
            let target = primary.join(name);
            let ours = std::fs::read_link(&link)
                .map(|t| t == target)
                .unwrap_or(false);
            if a.share {
                if !link.exists() && !is_link(&link) && target.is_dir() {
                    txn.link_dir(&link, &target)?;
                }
            } else if ours {
                txn.remove_file(&link)?;
            }
        }
        let (file, block) = match a.tool {
            Tool::Claude => {
                let src = collapse_tilde(&primary.join("CLAUDE.md"), &p.home);
                (
                    dir.join("CLAUDE.md"),
                    format!("{SHARED_BEGIN}\n@{src}\n{SHARED_END}\n"),
                )
            }
            Tool::Codex | Tool::Grok => {
                let src_path = primary.join("AGENTS.md");
                let content = remove_block(&std::fs::read_to_string(&src_path).unwrap_or_default());
                let src = collapse_tilde(&src_path, &p.home);
                (
                    dir.join("AGENTS.md"),
                    format!(
                        "{SHARED_BEGIN}\n<!-- copied from {src}; Agent Roster refreshes this copy -->\n{}\n{SHARED_END}\n",
                        content.trim()
                    ),
                )
            }
        };
        let existing = std::fs::read_to_string(&file).unwrap_or_default();
        let updated = upsert_shared(&existing, a.share.then_some(block.as_str()));
        if updated != existing {
            txn.replace_link_with_copy(&file)?;
            txn.write_file(&file, updated.as_bytes())?;
        }
    }
    Ok(())
}
