//! Every change to the roster is planned first (so it can be previewed), then applied
//! in one journal transaction (so it can be undone).

use crate::journal::{Journal, JournalError, Txn};
use crate::launcher::{
    PathSetup, apply_path_setup, default_shell_rcs, path_setup_needed, write_launchers,
};
use crate::model::{
    Agent, AgentColor, LimitsSource, ModelError, Roster, StatuslineChain, Tool, new_id,
};
use crate::names::{CommandLookup, NameError, validate_name};
use crate::note::{remove_block, write_notes};
use crate::paths::{Platform, RosterPaths, collapse_tilde, expand_tilde};
use crate::share::{apply_share, upsert_shared};
use crate::statusline::settings_real_path;
use std::fmt;
use std::path::PathBuf;

/// What an operation needs from the outside world.
pub struct Ctx<'a> {
    pub lookup: &'a dyn CommandLookup,
    pub path_var: String,
    /// Absolute path of the `roster` binary, written into launchers.
    pub roster_bin: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    Add,
    Change,
    Remove,
    Note,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub kind: StepKind,
    pub text: String,
}

pub(crate) fn step(kind: StepKind, text: impl Into<String>) -> Step {
    Step {
        kind,
        text: text.into(),
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Action {
    EnsureDir(PathBuf),
    StripNote(PathBuf),
    MoveToBackup(PathBuf),
    PathSetup(PathSetup),
    DefaultFn { tool: Tool, target: Option<String> },
    WriteFile { path: PathBuf, content: String },
    RemoveFile(PathBuf),
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub summary: String,
    pub steps: Vec<Step>,
    pub roster_after: Roster,
    actions: Vec<Action>,
}

#[derive(Debug)]
pub enum OpError {
    Name(NameError),
    NotFound(String),
    Refused(String),
    Model(ModelError),
    Journal(JournalError),
}
impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpError::Name(e) => write!(f, "{e}"),
            OpError::NotFound(n) => {
                write!(f, "No agent is named {n}. Run `roster` to see your agents.")
            }
            OpError::Refused(why) => write!(f, "{why}"),
            OpError::Model(e) => write!(f, "{e}"),
            OpError::Journal(e) => write!(f, "{e}"),
        }
    }
}
impl std::error::Error for OpError {}
impl From<NameError> for OpError {
    fn from(e: NameError) -> Self {
        OpError::Name(e)
    }
}
impl From<ModelError> for OpError {
    fn from(e: ModelError) -> Self {
        OpError::Model(e)
    }
}
impl From<JournalError> for OpError {
    fn from(e: JournalError) -> Self {
        OpError::Journal(e)
    }
}

pub(crate) const NOTHING_ELSE: &str = "Nothing else on your computer changes.";

pub(crate) fn finish(
    summary: String,
    mut steps: Vec<Step>,
    roster_after: Roster,
    mut actions: Vec<Action>,
    p: &RosterPaths,
    ctx: &Ctx,
) -> Plan {
    if !roster_after.agents.is_empty()
        && let Some(setup) = path_setup_needed(p, &ctx.path_var, &default_shell_rcs(p))
    {
        let text = if setup.windows_user_path.is_some() {
            "Adds Agent Roster's command folder to your PATH.".to_string()
        } else {
            let files: Vec<String> = setup
                .rc_files
                .iter()
                .map(|f| collapse_tilde(f, &p.home))
                .collect();
            format!(
                "Adds Agent Roster's command folder to PATH in {}.",
                files.join(" and ")
            )
        };
        let at = steps
            .iter()
            .position(|s| s.text == NOTHING_ELSE)
            .unwrap_or(steps.len());
        steps.insert(at, step(StepKind::Change, text));
        actions.push(Action::PathSetup(setup));
    }
    Plan {
        summary,
        steps,
        roster_after,
        actions,
    }
}

fn find<'a>(r: &'a Roster, name: &str) -> Result<&'a Agent, OpError> {
    r.by_name(name)
        .ok_or_else(|| OpError::NotFound(name.to_string()))
}

pub(crate) fn unique_id(r: &Roster) -> String {
    loop {
        let id = new_id();
        if r.by_id(&id).is_none() {
            return id;
        }
    }
}

pub struct AddRequest {
    pub name: String,
    pub tool: Tool,
    pub color: Option<AgentColor>,
    pub role: Option<String>,
    pub share: bool,
}

pub(crate) fn default_limits(tool: Tool) -> LimitsSource {
    match tool {
        Tool::Codex => LimitsSource::CodexSessions,
        // Grok's usage is asked from Grok itself (`usage::grok`); there is nothing to turn on.
        Tool::Claude | Tool::Grok => LimitsSource::Off,
    }
}

/// Puts the tool's existing standard folder (`~/.claude` or `~/.codex`) on the roster, in place.
pub fn plan_adopt(
    r: &Roster,
    p: &RosterPaths,
    ctx: &Ctx,
    tool: Tool,
    name: &str,
) -> Result<Plan, OpError> {
    validate_name(name, r, ctx.lookup, None)?;
    let standard = expand_tilde(tool.standard_dir(), &p.home);
    if let Some(a) = r.agents.iter().find(|a| r.config_dir_of(a, p) == standard) {
        return Err(OpError::Refused(format!(
            "Your existing {} login is already on the roster as {}.",
            tool.display(),
            a.name
        )));
    }
    let mut after = r.clone();
    let has_default = r.agents.iter().any(|a| a.tool == tool && a.default);
    after.agents.push(Agent {
        id: unique_id(r),
        name: name.to_string(),
        tool,
        config_dir: tool.standard_dir().to_string(),
        color: r.next_color(),
        role: None,
        default: !has_default,
        share: false,
        limits: default_limits(tool),
        advanced_links: vec![],
        good_at: None,
    });
    let steps = vec![
        step(
            StepKind::Add,
            format!("Adds a {name} command for every terminal."),
        ),
        step(
            StepKind::Note,
            format!(
                "Uses your existing {} login. Nothing is moved.",
                tool.display()
            ),
        ),
        step(
            StepKind::Note,
            format!("Tells your agents that {name} exists."),
        ),
        step(StepKind::Note, NOTHING_ELSE),
    ];
    Ok(finish(
        format!("Added {name} (existing {} login)", tool.display()),
        steps,
        after,
        vec![],
        p,
        ctx,
    ))
}

/// Puts several existing logins on the roster as one change (the app's first run).
pub fn plan_adopt_all(
    r: &Roster,
    p: &RosterPaths,
    ctx: &Ctx,
    picks: &[(Tool, String)],
) -> Result<Plan, OpError> {
    if picks.is_empty() {
        return Err(OpError::Refused("Pick at least one login to add.".into()));
    }
    let mut roster = r.clone();
    let mut steps: Vec<Step> = vec![];
    let mut actions: Vec<Action> = vec![];
    for (tool, name) in picks {
        let plan = plan_adopt(&roster, p, ctx, *tool, name)?;
        for s in plan.steps {
            if s.text != NOTHING_ELSE && !steps.contains(&s) {
                steps.push(s);
            }
        }
        for a in plan.actions {
            let dup_path = matches!(a, Action::PathSetup(_))
                && actions.iter().any(|x| matches!(x, Action::PathSetup(_)));
            if !dup_path {
                actions.push(a);
            }
        }
        roster = plan.roster_after;
    }
    steps.push(step(StepKind::Note, NOTHING_ELSE));
    let names: Vec<&str> = picks.iter().map(|(_, n)| n.as_str()).collect();
    let summary = match names.as_slice() {
        [one] => format!("Added {one} (existing login)"),
        [init @ .., last] => format!("Added {} and {last} (existing logins)", init.join(", ")),
        [] => unreachable!("checked above"),
    };
    Ok(Plan {
        summary,
        steps,
        roster_after: roster,
        actions,
    })
}

pub fn plan_add(r: &Roster, p: &RosterPaths, ctx: &Ctx, req: AddRequest) -> Result<Plan, OpError> {
    validate_name(&req.name, r, ctx.lookup, None)?;
    let id = unique_id(r);
    let dir = p.accounts_dir().join(&id);
    let mut after = r.clone();
    after.agents.push(Agent {
        id,
        name: req.name.clone(),
        tool: req.tool,
        config_dir: collapse_tilde(&dir, &p.home),
        color: req.color.unwrap_or_else(|| r.next_color()),
        role: req.role.filter(|x| !x.trim().is_empty()),
        default: false,
        share: req.share,
        limits: default_limits(req.tool),
        advanced_links: vec![],
        good_at: None,
    });
    let n = &req.name;
    let mut steps = vec![
        step(
            StepKind::Add,
            format!("Adds a {n} command for every terminal."),
        ),
        step(StepKind::Add, "Creates a private folder for this account."),
    ];
    if req.share {
        steps.push(step(
            StepKind::Change,
            format!("Shares your {} skills with {n}.", req.tool.display()),
        ));
    }
    steps.push(step(
        StepKind::Note,
        format!("Tells your other agents that {n} exists."),
    ));
    steps.push(step(StepKind::Note, NOTHING_ELSE));
    Ok(finish(
        format!("Added {n} ({})", req.tool.display()),
        steps,
        after,
        vec![Action::EnsureDir(dir)],
        p,
        ctx,
    ))
}

pub fn plan_rename(
    r: &Roster,
    p: &RosterPaths,
    ctx: &Ctx,
    name: &str,
    new_name: &str,
) -> Result<Plan, OpError> {
    let a = find(r, name)?;
    validate_name(new_name, r, ctx.lookup, Some(&a.id))?;
    let mut after = r.clone();
    let old = a.name.clone();
    after
        .agents
        .iter_mut()
        .find(|x| x.id == a.id)
        .expect("exists")
        .name = new_name.to_string();
    let steps = vec![
        step(
            StepKind::Change,
            format!("Renames the {old} command to {new_name}."),
        ),
        step(
            StepKind::Note,
            "Updates the roster note for your agents. The login stays as it is.",
        ),
    ];
    Ok(finish(
        format!("Renamed {old} to {new_name}"),
        steps,
        after,
        vec![],
        p,
        ctx,
    ))
}

pub fn plan_remove(
    r: &Roster,
    p: &RosterPaths,
    ctx: &Ctx,
    name: &str,
    delete_login_folder: bool,
) -> Result<Plan, OpError> {
    let a = find(r, name)?.clone();
    let adopted = r.is_adopted_default(&a, p);
    if delete_login_folder && adopted {
        return Err(OpError::Refused(format!(
            "{} is your original {} folder. Agent Roster never deletes it.",
            a.config_dir,
            a.tool.display()
        )));
    }
    let mut after = r.clone();
    after.agents.retain(|x| x.id != a.id);
    let dir = r.config_dir_of(&a, p);
    let note_file = dir.join(a.tool.note_file());
    let mut steps = vec![step(
        StepKind::Remove,
        format!("Removes the {} command.", a.name),
    )];
    let mut actions = vec![];
    if delete_login_folder {
        steps.push(step(
            StepKind::Remove,
            format!(
                "Moves {}'s login folder into Agent Roster's backups. Undo brings it back.",
                a.name
            ),
        ));
        actions.push(Action::MoveToBackup(dir));
    } else {
        steps.push(step(
            StepKind::Note,
            format!(
                "Keeps {}'s login folder, so adding it back later needs no new sign-in.",
                a.name
            ),
        ));
        // The main folder's file is the user's own: strip the note through a dotfiles link.
        let target = if adopted {
            crate::journal::resolve_file_link(&note_file)
        } else {
            note_file
        };
        actions.push(Action::StripNote(target));
    }
    steps.push(step(
        StepKind::Note,
        format!("Tells your other agents that {} is gone.", a.name),
    ));
    Ok(finish(
        format!("Removed {}", a.name),
        steps,
        after,
        actions,
        p,
        ctx,
    ))
}

fn edit(r: &Roster, name: &str, f: impl FnOnce(&mut Agent)) -> Result<(Roster, Agent), OpError> {
    let a = find(r, name)?;
    let mut after = r.clone();
    let target = after
        .agents
        .iter_mut()
        .find(|x| x.id == a.id)
        .expect("exists");
    f(target);
    let changed = target.clone();
    Ok((after, changed))
}

pub fn plan_set_color(
    r: &Roster,
    p: &RosterPaths,
    ctx: &Ctx,
    name: &str,
    color: AgentColor,
) -> Result<Plan, OpError> {
    let (after, a) = edit(r, name, |a| a.color = color)?;
    let steps = vec![step(
        StepKind::Change,
        format!("Shows {} in {}.", a.name, color.label()),
    )];
    Ok(finish(
        format!("Changed {}'s color", a.name),
        steps,
        after,
        vec![],
        p,
        ctx,
    ))
}

/// A status line command that is one of ours (any `roster` path, quoted or not). Chaining to
/// it would loop; another tool's `... statusline` command is kept as the chain.
fn is_roster_statusline(cmd: &str) -> bool {
    let Some(prog) = cmd.trim().strip_suffix(" statusline") else {
        return false;
    };
    let prog = prog.trim().trim_matches('"').trim_matches('\'');
    let file = prog.rsplit(['/', '\\']).next().unwrap_or(prog);
    file == "roster" || file == "roster.exe"
}

pub fn plan_set_good_at(
    r: &Roster,
    p: &RosterPaths,
    ctx: &Ctx,
    name: &str,
    items: Option<Vec<String>>,
) -> Result<Plan, OpError> {
    let items: Option<Vec<String>> = items.map(|v| {
        v.iter()
            .map(|x| x.trim().replace('|', "/").replace('\n', " "))
            .filter(|x| !x.is_empty())
            .fold(Vec::<String>::new(), |mut v, x| {
                if !v.contains(&x) {
                    v.push(x);
                }
                v
            })
    });
    let (after, a) = edit(r, name, |a| a.good_at = items.clone())?;
    let (summary, text) = match &items {
        Some(v) => (
            format!("Set what {} is good at", a.name),
            format!(
                "Tells your agents that {} is good at: {}.",
                a.name,
                if v.is_empty() {
                    "nothing listed".to_string()
                } else {
                    v.join(", ")
                }
            ),
        ),
        None => (
            format!("Reset what {} is good at", a.name),
            format!(
                "Goes back to what {} is usually good at: {}.",
                a.tool.display(),
                crate::model::default_good_at(a.tool).join(", ")
            ),
        ),
    };
    Ok(finish(
        summary,
        vec![step(StepKind::Note, text)],
        after,
        vec![],
        p,
        ctx,
    ))
}

pub fn plan_set_role(
    r: &Roster,
    p: &RosterPaths,
    ctx: &Ctx,
    name: &str,
    role: Option<String>,
) -> Result<Plan, OpError> {
    let role = role.filter(|x| !x.trim().is_empty());
    let (after, a) = edit(r, name, |a| a.role = role.clone())?;
    let text = match &role {
        Some(t) => format!("Tells your agents that {} does: {t}.", a.name),
        None => format!("Clears {}'s role in the roster note.", a.name),
    };
    Ok(finish(
        format!("Changed {}'s role", a.name),
        vec![step(StepKind::Note, text)],
        after,
        vec![],
        p,
        ctx,
    ))
}

pub fn plan_set_share(
    r: &Roster,
    p: &RosterPaths,
    ctx: &Ctx,
    name: &str,
    on: bool,
) -> Result<Plan, OpError> {
    let a = find(r, name)?;
    if r.is_adopted_default(a, p) {
        return Err(OpError::Refused(format!(
            "{} already is your main {} folder, so there is nothing to share.",
            a.name,
            a.tool.display()
        )));
    }
    let (after, a) = edit(r, name, |a| a.share = on)?;
    let text = if on {
        format!(
            "Links your {} skills into {}'s folder. Settings and hooks stay separate.",
            a.tool.display(),
            a.name
        )
    } else {
        format!("Removes the skill links from {}'s folder.", a.name)
    };
    Ok(finish(
        format!(
            "Turned sharing {} for {}",
            if on { "on" } else { "off" },
            a.name
        ),
        vec![step(StepKind::Change, text)],
        after,
        vec![],
        p,
        ctx,
    ))
}

/// Chooses which agent a bare `claude` / `codex` opens in interactive shells.
pub fn plan_set_default(
    r: &Roster,
    p: &RosterPaths,
    ctx: &Ctx,
    name: &str,
) -> Result<Plan, OpError> {
    let a = find(r, name)?.clone();
    let mut after = r.clone();
    for x in after.agents.iter_mut().filter(|x| x.tool == a.tool) {
        x.default = x.id == a.id;
    }
    let bin = a.tool.binary();
    let (target, text) = if r.is_adopted_default(&a, p) {
        (
            None,
            format!(
                "Typing {bin} opens {} again, the way it did before Agent Roster.",
                a.name
            ),
        )
    } else {
        (
            Some(a.name.clone()),
            format!(
                "Typing {bin} in a new terminal opens {}. Agents calling tools are not affected.",
                a.name
            ),
        )
    };
    Ok(finish(
        format!("Made {} the default for {bin}", a.name),
        vec![step(StepKind::Change, text)],
        after,
        vec![Action::DefaultFn {
            tool: a.tool,
            target,
        }],
        p,
        ctx,
    ))
}

fn default_markers(tool: Tool) -> (String, String) {
    let b = tool.binary();
    (
        format!("# >>> agent-roster default {b} >>>"),
        format!("# <<< agent-roster default {b} <<<"),
    )
}

fn upsert_marked(existing: &str, begin: &str, end: &str, block: Option<&str>) -> String {
    let mut base = existing.to_string();
    if let (Some(s), Some(e)) = (existing.find(begin), existing.find(end)) {
        let mut e = e + end.len();
        if existing[e..].starts_with('\n') {
            e += 1;
        }
        let before = existing[..s].trim_end_matches('\n');
        base = if before.is_empty() {
            existing[e..].to_string()
        } else {
            format!("{before}\n{}", &existing[e..])
        };
    }
    match block {
        Some(b) => {
            let t = base.trim_end();
            if t.is_empty() {
                b.to_string()
            } else {
                format!("{t}\n\n{b}")
            }
        }
        None => base,
    }
}

fn apply_default_fn(
    txn: &mut Txn,
    p: &RosterPaths,
    tool: Tool,
    target: Option<&str>,
) -> Result<(), JournalError> {
    let (begin, end) = default_markers(tool);
    let bin = tool.binary();
    if p.platform == Platform::Windows {
        let profile = p
            .home
            .join("Documents")
            .join("PowerShell")
            .join("Microsoft.PowerShell_profile.ps1");
        let block = target.map(|t| format!("{begin}\nfunction {bin} {{ {t} @args }}\n{end}\n"));
        let existing = std::fs::read_to_string(&profile).unwrap_or_default();
        let updated = upsert_marked(&existing, &begin, &end, block.as_deref());
        if updated != existing {
            txn.write_file(&profile, updated.as_bytes())?;
        }
        return Ok(());
    }
    for rc in default_shell_rcs(p) {
        let is_fish = rc.extension().is_some_and(|e| e == "fish");
        let rc = if is_fish {
            rc.with_file_name(format!("agent-roster-default-{bin}.fish"))
        } else {
            rc
        };
        let block = target.map(|t| {
            if is_fish {
                format!("{begin}\nfunction {bin}; {t} $argv; end\n{end}\n")
            } else {
                format!("{begin}\n{bin}() {{ {t} \"$@\"; }}\n{end}\n")
            }
        });
        let existing = std::fs::read_to_string(&rc).unwrap_or_default();
        if block.is_none() && existing.is_empty() {
            continue;
        }
        let updated = upsert_marked(&existing, &begin, &end, block.as_deref());
        if updated != existing {
            let rc = crate::journal::resolve_file_link(&rc);
            txn.write_file(&rc, updated.as_bytes())?;
        }
    }
    Ok(())
}

/// Applies a plan in one journal transaction. Returns `None` when nothing changed.
pub fn apply(
    plan: Plan,
    p: &RosterPaths,
    journal: &mut Journal,
    ctx: &Ctx,
) -> Result<Option<u64>, OpError> {
    let toml = plan.roster_after.to_toml_string()?;
    let mut t = journal.begin(&plan.summary);
    for action in &plan.actions {
        match action {
            Action::EnsureDir(d) => t.ensure_dir(d)?,
            Action::StripNote(f) => {
                if let Ok(existing) = std::fs::read_to_string(f) {
                    let updated = upsert_shared(&remove_block(&existing), None);
                    if updated != existing {
                        t.replace_link_with_copy(f)?;
                        t.write_file(f, updated.as_bytes())?;
                    }
                }
            }
            Action::MoveToBackup(d) => t.move_dir_to_backup(d)?,
            Action::PathSetup(s) => apply_path_setup(&mut t, s, p)?,
            Action::DefaultFn { tool, target } => {
                apply_default_fn(&mut t, p, *tool, target.as_deref())?
            }
            Action::WriteFile { path, content } => t.write_file(path, content.as_bytes())?,
            Action::RemoveFile(path) => t.remove_file(path)?,
        }
    }
    t.write_file(&p.roster_file(), toml.as_bytes())?;
    write_launchers(&mut t, &plan.roster_after, p, &ctx.roster_bin)?;
    write_notes(&mut t, &plan.roster_after, p)?;
    apply_share(&mut t, &plan.roster_after, p)?;
    if t.is_empty() {
        return Ok(None);
    }
    Ok(Some(t.commit()?))
}

fn claude_agent<'a>(r: &'a Roster, name: &str) -> Result<&'a Agent, OpError> {
    let a = find(r, name)?;
    if a.tool != Tool::Claude {
        return Err(OpError::Refused(format!(
            "{} is a Codex agent. Codex limits are read automatically.",
            a.name
        )));
    }
    Ok(a)
}

fn read_settings(
    path: &std::path::Path,
) -> Result<serde_json::Map<String, serde_json::Value>, OpError> {
    match std::fs::read_to_string(path) {
        Ok(t) if !t.trim().is_empty() => match serde_json::from_str::<serde_json::Value>(&t) {
            Ok(serde_json::Value::Object(m)) => Ok(m),
            _ => Err(OpError::Refused(format!(
                "{} isn't valid JSON, so Agent Roster left it alone.",
                path.display()
            ))),
        },
        _ => Ok(serde_json::Map::new()),
    }
}

fn settings_text(m: serde_json::Map<String, serde_json::Value>) -> String {
    let mut t =
        serde_json::to_string_pretty(&serde_json::Value::Object(m)).unwrap_or_else(|_| "{}".into());
    t.push('\n');
    t
}

/// Turns on Claude limits for an agent by routing its status line through `roster statusline`.
pub fn plan_enable_claude_limits(
    r: &Roster,
    p: &RosterPaths,
    ctx: &Ctx,
    name: &str,
) -> Result<Plan, OpError> {
    let a = claude_agent(r, name)?.clone();
    let real = settings_real_path(&a, r, p);
    let real_s = real.display().to_string();
    let mut after = r.clone();
    let sharing: Vec<String> = r
        .agents
        .iter()
        .filter(|x| x.tool == Tool::Claude && settings_real_path(x, r, p) == real)
        .map(|x| x.id.clone())
        .collect();
    for x in after.agents.iter_mut().filter(|x| sharing.contains(&x.id)) {
        x.limits = LimitsSource::ClaudeStatusline;
    }
    if r.statusline.iter().any(|s| s.settings == real_s) {
        let steps = vec![step(
            StepKind::Note,
            format!(
                "Usage is already shown for {}, through {}.",
                a.name,
                collapse_tilde(&real, &p.home)
            ),
        )];
        return Ok(finish(
            format!("Showing usage for {}", a.name),
            steps,
            after,
            vec![],
            p,
            ctx,
        ));
    }
    let mut settings = read_settings(&real)?;
    // Claude runs this through a shell, so a path with spaces (like the copy inside the app) is quoted.
    let bin = ctx.roster_bin.display().to_string();
    let our_cmd = if bin.contains(' ') {
        format!("\"{bin}\" statusline")
    } else {
        format!("{bin} statusline")
    };
    let mut line = settings
        .get("statusLine")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let chain = line
        .get("command")
        .and_then(|c| c.as_str())
        .map(String::from)
        .filter(|c| !is_roster_statusline(c));
    line.insert("type".into(), "command".into());
    line.insert("command".into(), our_cmd.into());
    settings.insert("statusLine".into(), serde_json::Value::Object(line));
    after.statusline.push(StatuslineChain {
        settings: real_s,
        chain: chain.clone(),
    });
    let mut steps = vec![step(
        StepKind::Change,
        format!(
            "Changes the status line in {} so Agent Roster can read {}'s weekly use.",
            collapse_tilde(&real, &p.home),
            a.name
        ),
    )];
    if chain.is_some() {
        steps.push(step(
            StepKind::Note,
            "Your current status line still shows, unchanged.",
        ));
    }
    steps.push(step(
        StepKind::Note,
        "Hiding usage again puts your old status line back exactly.",
    ));
    Ok(finish(
        format!("Showing usage for {}", a.name),
        steps,
        after,
        vec![Action::WriteFile {
            path: real,
            content: settings_text(settings),
        }],
        p,
        ctx,
    ))
}

pub fn plan_disable_claude_limits(
    r: &Roster,
    p: &RosterPaths,
    ctx: &Ctx,
    name: &str,
) -> Result<Plan, OpError> {
    let a = claude_agent(r, name)?.clone();
    let real = settings_real_path(&a, r, p);
    let real_s = real.display().to_string();
    let Some(entry) = r.statusline.iter().find(|s| s.settings == real_s).cloned() else {
        return Err(OpError::Refused(format!(
            "Usage isn't shown for {}.",
            a.name
        )));
    };
    let mut after = r.clone();
    after.statusline.retain(|s| s.settings != real_s);
    for x in after
        .agents
        .iter_mut()
        .filter(|x| x.tool == Tool::Claude && settings_real_path(x, r, p) == real)
    {
        x.limits = LimitsSource::Off;
    }
    let mut settings = read_settings(&real)?;
    match &entry.chain {
        Some(cmd) => {
            let mut line = settings
                .get("statusLine")
                .and_then(|v| v.as_object())
                .cloned()
                .unwrap_or_default();
            line.insert("type".into(), "command".into());
            line.insert("command".into(), cmd.clone().into());
            settings.insert("statusLine".into(), serde_json::Value::Object(line));
        }
        None => {
            settings.remove("statusLine");
        }
    }
    let steps = vec![step(
        StepKind::Change,
        format!(
            "Puts the original status line back in {}.",
            collapse_tilde(&real, &p.home)
        ),
    )];
    Ok(finish(
        format!("Stopped showing usage for {}", a.name),
        steps,
        after,
        vec![Action::WriteFile {
            path: real,
            content: settings_text(settings),
        }],
        p,
        ctx,
    ))
}
