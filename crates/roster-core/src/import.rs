//! Brings the earlier bash script's roster (`roster.conf`) onto the real roster, in place.

use crate::model::{Agent, AgentColor, Roster, Tool};
use crate::names::validate_name;
use crate::ops::{
    Action, Ctx, NOTHING_ELSE, OpError, Plan, StepKind, default_limits, finish, step, unique_id,
};
use crate::paths::{RosterPaths, collapse_tilde, expand_tilde};
use std::path::{Path, PathBuf};

const PROTOTYPE_MARKER_LINE: &str = "# generated-by-agent-roster";

pub fn detect_prototype(p: &RosterPaths) -> Option<PathBuf> {
    let f = p.config_dir.join("roster.conf");
    f.is_file().then_some(f)
}

fn is_link(p: &Path) -> bool {
    std::fs::symlink_metadata(p)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

struct Line {
    name: String,
    tool: Tool,
    dir: String,
}

fn parse(conf: &str) -> Result<Vec<Line>, OpError> {
    let mut out = vec![];
    for raw in conf.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        let [name, tool, dir] = parts[..] else {
            return Err(OpError::Refused(format!(
                "This line in roster.conf isn't in the expected form: {raw}"
            )));
        };
        let tool = match tool {
            "claude" => Tool::Claude,
            "codex" => Tool::Codex,
            other => {
                return Err(OpError::Refused(format!(
                    "roster.conf names an unknown tool: {other}"
                )));
            }
        };
        out.push(Line {
            name: name.to_string(),
            tool,
            dir: dir.to_string(),
        });
    }
    Ok(out)
}

pub fn plan_import(r: &Roster, p: &RosterPaths, ctx: &Ctx) -> Result<Plan, OpError> {
    if !r.agents.is_empty() {
        return Err(OpError::Refused(
            "The roster already has agents, so nothing was imported.".into(),
        ));
    }
    let conf_path = detect_prototype(p)
        .ok_or_else(|| OpError::Refused("No earlier roster.conf was found.".into()))?;
    let conf = std::fs::read_to_string(&conf_path).map_err(|e| OpError::Refused(e.to_string()))?;
    let lines = parse(&conf)?;
    let mut after = Roster::default();
    let mut actions = vec![];
    let mut kept_links = vec![];
    let mut own_files: Vec<(String, String)> = vec![]; // (file, agent name)
    for (i, l) in lines.iter().enumerate() {
        validate_name(&l.name, &after, ctx.lookup, None)?;
        let dir = expand_tilde(&l.dir, &p.home);
        let standard = expand_tilde(l.tool.standard_dir(), &p.home);
        let adopted = dir == standard;
        let mut agent = Agent {
            id: unique_id(&after),
            name: l.name.clone(),
            tool: l.tool,
            config_dir: collapse_tilde(&dir, &p.home),
            color: AgentColor::ORDER[i % AgentColor::ORDER.len()],
            role: None,
            default: adopted && !after.agents.iter().any(|a| a.tool == l.tool && a.default),
            share: false,
            limits: default_limits(l.tool),
            advanced_links: vec![],
        };
        if !adopted {
            let (note, skills, advanced): (&str, &[&str], &[&str]) = match l.tool {
                Tool::Claude => (
                    "CLAUDE.md",
                    &["skills", "agents", "commands"],
                    &["settings.json", "hooks"],
                ),
                Tool::Codex => ("AGENTS.md", &["skills"], &["config.toml"]),
            };
            for s in skills {
                if is_link(&dir.join(s))
                    && std::fs::read_link(dir.join(s)).ok() == Some(standard.join(s))
                {
                    agent.share = true;
                }
            }
            for a in advanced {
                if is_link(&dir.join(a)) {
                    agent.advanced_links.push(a.to_string());
                    kept_links.push(collapse_tilde(&dir.join(a), &p.home));
                }
            }
            // A linked instruction file would make notes edit the main file; start this agent's own.
            if is_link(&dir.join(note)) {
                actions.push(Action::RemoveFile(dir.join(note)));
                own_files.push((collapse_tilde(&dir.join(note), &p.home), l.name.clone()));
            }
        }
        after.agents.push(agent);
    }
    let old_bin = p.home.join(".local").join("bin");
    let mut old_cmds = vec![];
    if let Ok(entries) = std::fs::read_dir(&old_bin) {
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let second = text.lines().nth(1).unwrap_or("").trim();
            let is_old_shim = second == PROTOTYPE_MARKER_LINE;
            let is_old_script = path.file_name().is_some_and(|n| n == "agent-roster")
                && text.contains("agent-roster — named launchers");
            if is_old_shim || is_old_script {
                if is_old_shim {
                    old_cmds.push(path.file_name().unwrap().to_string_lossy().to_string());
                }
                actions.push(Action::RemoveFile(path));
            }
        }
    }
    actions.push(Action::WriteFile {
        path: conf_path.with_extension("conf.imported"),
        content: conf.clone(),
    });
    actions.push(Action::RemoveFile(conf_path));
    let names: Vec<&str> = after.agents.iter().map(|a| a.name.as_str()).collect();
    let mut steps = vec![
        step(
            StepKind::Add,
            format!(
                "Brings your {} agents from the earlier script onto the roster: {}.",
                names.len(),
                names.join(", ")
            ),
        ),
        step(
            StepKind::Note,
            "Keeps every login folder where it is. Nothing is moved.",
        ),
    ];
    if !kept_links.is_empty() {
        steps.push(step(
            StepKind::Note,
            format!(
                "Keeps these shared links as they are: {}.",
                kept_links.join(", ")
            ),
        ));
    }
    if !old_cmds.is_empty() {
        steps.push(step(
            StepKind::Change,
            format!(
                "Replaces the old {} commands in ~/.local/bin with new ones.",
                old_cmds.join(", ")
            ),
        ));
    }
    for (file, name) in &own_files {
        let shared = after.by_name(name).is_some_and(|a| a.share);
        let text = if shared {
            format!(
                "Gives {file} its own file instead of a link. Your main rules still reach {name} through an import line."
            )
        } else {
            format!(
                "Gives {file} its own file instead of a link, without your main rules. Run `roster share {name} on` to bring them back through an import line."
            )
        };
        steps.push(step(StepKind::Change, text));
    }
    steps.push(step(
        StepKind::Note,
        "Rewrites the roster note in each agent's instructions.",
    ));
    steps.push(step(StepKind::Note, NOTHING_ELSE));
    Ok(finish(
        "Imported the earlier script's roster".into(),
        steps,
        after,
        actions,
        p,
        ctx,
    ))
}
