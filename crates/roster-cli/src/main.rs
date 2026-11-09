mod prompt;
mod render;

use anstream::{eprintln, print, println};
use clap::{Args, Parser, Subcommand};
use prompt::{Confirm, confirm, print_plan};
use roster_core::detect::RealRunner;
use roster_core::import::{detect_prototype, plan_import};
use roster_core::journal::Journal;
use roster_core::model::{AgentColor, Roster, Tool};
use roster_core::names::{CommandLookup, PathLookup};
use roster_core::ops::{self, AddRequest, Ctx, Plan};
use roster_core::paths::{Platform, RosterPaths, expand_tilde};
use roster_core::status::overview;
use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "roster",
    version,
    about = "Name your AI coding accounts and use them from any terminal"
)]
struct Cli {
    /// Print the overview as JSON.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Args, Clone, Copy)]
struct Yes {
    /// Don't ask before making the change.
    #[arg(long, short = 'y')]
    yes: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// Put your existing Claude Code and Codex logins on the roster.
    Init {
        #[arg(long, default_value = "claude1")]
        claude_name: String,
        #[arg(long, default_value = "codex1")]
        codex_name: String,
        #[command(flatten)]
        yes: Yes,
    },
    /// Add an agent with its own login.
    Add {
        name: String,
        #[arg(long, conflicts_with = "codex", required_unless_present = "codex")]
        claude: bool,
        #[arg(long)]
        codex: bool,
        #[arg(long)]
        color: Option<String>,
        #[arg(long)]
        role: Option<String>,
        /// Share your skills with this agent.
        #[arg(long)]
        share: bool,
        #[command(flatten)]
        yes: Yes,
    },
    /// Rename an agent. Its login stays as it is.
    Rename {
        old: String,
        new: String,
        #[command(flatten)]
        yes: Yes,
    },
    /// Change an agent's color.
    Color {
        name: String,
        color: String,
        #[command(flatten)]
        yes: Yes,
    },
    /// Set the one-line role other agents see. Leave it out to clear it.
    Role {
        name: String,
        role: Option<String>,
        #[command(flatten)]
        yes: Yes,
    },
    /// Share your skills with an agent, or stop sharing.
    Share {
        name: String,
        #[arg(value_parser = ["on", "off"])]
        state: String,
        #[command(flatten)]
        yes: Yes,
    },
    /// Choose which agent a bare `claude` or `codex` opens in new terminals.
    Default {
        name: String,
        #[command(flatten)]
        yes: Yes,
    },
    /// Remove an agent. Its login folder is kept unless you ask.
    Remove {
        name: String,
        #[arg(long)]
        delete_login_folder: bool,
        #[command(flatten)]
        yes: Yes,
    },
    /// Sign an agent in, using the tool's own sign-in.
    Login {
        name: String,
        /// Use a device code (for SSH and machines without a browser).
        #[arg(long)]
        device: bool,
    },
    /// Show limits, or turn Claude limits on or off for an agent.
    Limits {
        #[arg(value_parser = ["enable", "disable"])]
        action: Option<String>,
        name: Option<String>,
        #[command(flatten)]
        yes: Yes,
    },
    /// List every change Agent Roster made.
    Changes,
    /// Undo a change by its number, the last one, or all of them.
    Undo {
        op: Option<u64>,
        #[arg(long, conflicts_with = "op")]
        last: bool,
        #[arg(long, conflicts_with_all = ["op", "last"])]
        all: bool,
        #[command(flatten)]
        yes: Yes,
    },
    /// Bring in the agents from the earlier agent-roster script.
    Import {
        #[command(flatten)]
        yes: Yes,
    },
    /// Check that everything is set up correctly.
    Doctor,
    /// Open the Agent Roster window.
    Ui,
    /// Undo every change and remove Agent Roster's files. Login folders are kept unless you ask.
    Uninstall {
        #[arg(long)]
        delete_login_folders: bool,
        #[command(flatten)]
        yes: Yes,
    },
    #[command(hide = true)]
    Run {
        id: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    #[command(hide = true)]
    Statusline,
}

struct App {
    p: RosterPaths,
    roster_bin: PathBuf,
}

fn env_var(k: &str) -> Option<String> {
    std::env::var(k).ok()
}

impl App {
    fn ctx<'a>(&self, lookup: &'a PathLookup) -> Ctx<'a> {
        Ctx {
            lookup,
            path_var: env_var("PATH").unwrap_or_default(),
            roster_bin: self.roster_bin.clone(),
        }
    }
    fn lookup(&self) -> PathLookup {
        PathLookup {
            exclude: Some(self.p.bin_dir()),
        }
    }
    fn roster(&self) -> Result<Roster, String> {
        Roster::load(&self.p).map_err(|e| e.to_string())
    }
    fn journal(&self) -> Result<Journal, String> {
        Journal::open(&self.p).map_err(|e| e.to_string())
    }
    /// Shows the plan, asks, applies. Returns the exit code.
    fn run_plan(&self, plan: Result<Plan, ops::OpError>, yes: Yes) -> Result<u8, String> {
        let plan = plan.map_err(|e| e.to_string())?;
        print_plan(&plan);
        match confirm(yes.yes) {
            Confirm::Yes => {}
            Confirm::No => {
                println!("Nothing changed.");
                return Ok(0);
            }
            Confirm::CannotAsk => return Ok(2),
        }
        let lookup = self.lookup();
        let mut j = self.journal()?;
        match ops::apply(plan, &self.p, &mut j, &self.ctx(&lookup)).map_err(|e| e.to_string())? {
            Some(id) => println!("Done. Change {id} is recorded; `roster undo {id}` reverses it."),
            None => println!("Nothing needed to change."),
        }
        Ok(0)
    }
}

fn tool_of(claude: bool) -> Tool {
    if claude { Tool::Claude } else { Tool::Codex }
}

fn parse_color(c: Option<String>) -> Result<Option<AgentColor>, String> {
    match c {
        None => Ok(None),
        Some(s) => AgentColor::parse(&s).map(Some).ok_or_else(|| {
            format!("{s} isn't one of the colors: cobalt, marigold, jade, orchid, coral.")
        }),
    }
}

fn show_overview(app: &App, json: bool) -> Result<u8, String> {
    let r = app.roster()?;
    if r.agents.is_empty() && !json {
        println!("\n Agent Roster has no agents yet.\n");
        let tools = [Tool::Claude, Tool::Codex].map(|t| {
            roster_core::detect::find_tool(
                t,
                &env_var("PATH").unwrap_or_default(),
                &app.p.bin_dir(),
            )
        });
        for (t, found) in [Tool::Claude, Tool::Codex].iter().zip(tools.iter()) {
            match found {
                Some(path) => println!("  {}  found at {}", t.display(), path.display()),
                None => println!("  {}  not installed", t.display()),
            }
        }
        if detect_prototype(&app.p).is_some() {
            println!(
                "\n  Your earlier agent-roster script's agents were found. Bring them in with:\n\n    roster import\n"
            );
        } else {
            println!(
                "\n  Put your existing logins on the roster with:\n\n    roster init\n\n  Then add more accounts with `roster add <name> --claude` or `--codex`.\n"
            );
        }
        return Ok(0);
    }
    let o = overview(
        &r,
        &app.p,
        &env_var("PATH").unwrap_or_default(),
        &RealRunner::default(),
    );
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&render::overview_json(&o)).unwrap_or_default()
        );
    } else {
        print!("{}", render::overview_text(&o));
    }
    Ok(0)
}

fn headless() -> bool {
    !roster_core::desktop::has_desktop(Platform::current(), &env_var)
}

const NO_APP: &str = "The app window isn't installed. Everything works from these commands; run `roster --help` to see them.";
const NO_DESKTOP: &str = "There's no desktop here. Everything works from these commands; run `roster --help` to see them.";

fn open_ui(app: &App) -> Result<u8, String> {
    if headless() {
        println!("{NO_DESKTOP}");
        return Ok(0);
    }
    let Some(found) = roster_core::desktop::find_app(&app.p, &env_var) else {
        println!("{NO_APP}");
        return Ok(0);
    };
    let cmd = roster_core::desktop::app_launch_command(app.p.platform, &found);
    std::process::Command::new(&cmd[0])
        .args(&cmd[1..])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Couldn't open {}: {e}", found.display()))?;
    println!("Opening Agent Roster.");
    Ok(0)
}

fn run_agent(app: &App, id: &str, args: Vec<String>) -> Result<u8, String> {
    let interactive = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let plan = roster_core::run::prepare_run(id, args, &app.p, &env_var, interactive)
        .map_err(|e| e.to_string())?;
    let code = roster_core::run::exec_run(plan, &app.p).map_err(|e| e.to_string())?;
    Ok(code.clamp(0, 255) as u8)
}

fn doctor(app: &App) -> Result<u8, String> {
    let r = app.roster()?;
    let path_var = env_var("PATH").unwrap_or_default();
    let mut ok = true;
    let mut check = |good: bool, text: String, fix: &str| {
        let mark = if good { "✓" } else { "✗" };
        println!("  {mark} {text}");
        if !good {
            ok = false;
            if !fix.is_empty() {
                println!("      {fix}");
            }
        }
    };
    println!();
    for t in [Tool::Claude, Tool::Codex] {
        let found = roster_core::detect::find_tool(t, &path_var, &app.p.bin_dir());
        let used = r.agents.iter().any(|a| a.tool == t);
        check(
            found.is_some() || !used,
            format!(
                "{} {}",
                t.display(),
                if found.is_some() {
                    "is installed"
                } else {
                    "is not installed"
                }
            ),
            "Install it, or remove its agents.",
        );
    }
    check(
        roster_core::launcher::path_has_bin(&app.p, &path_var),
        "Agent Roster's command folder is on PATH".into(),
        "Open a new terminal. If this stays red, run any `roster add` or `roster init` to set PATH up.",
    );
    for a in &r.agents {
        let shim = app
            .p
            .bin_dir()
            .join(if app.p.platform == Platform::Windows {
                format!("{}.cmd", a.name)
            } else {
                a.name.clone()
            });
        check(
            shim.exists(),
            format!("{} has its command", a.name),
            "Run `roster rename <name> <same name>` or re-run `roster init` to rewrite commands.",
        );
        let note = r.config_dir_of(a, &app.p).join(if a.tool == Tool::Claude {
            "CLAUDE.md"
        } else {
            "AGENTS.md"
        });
        let has_note = std::fs::read_to_string(&note)
            .map(|t| t.contains(roster_core::note::BEGIN))
            .unwrap_or(false);
        check(
            has_note,
            format!("{} has its roster note", a.name),
            "Any roster change rewrites notes.",
        );
    }
    for s in &r.statusline {
        let text = std::fs::read_to_string(&s.settings).unwrap_or_default();
        check(
            text.contains(" statusline"),
            format!("Claude limits status line is in {}", s.settings),
            "Run `roster limits disable <name>` then `enable` again.",
        );
    }
    println!();
    Ok(if ok { 0 } else { 1 })
}

fn changes(app: &App) -> Result<u8, String> {
    let j = app.journal()?;
    if j.ops().is_empty() {
        println!("Agent Roster hasn't changed anything yet.");
        return Ok(0);
    }
    for op in j.ops().iter().rev() {
        let undone = if op.undone { "  (undone)" } else { "" };
        println!(
            "  {:>3}  {:<9}  {}{}",
            op.id,
            render::age(render::now() - op.at),
            op.summary,
            undone
        );
    }
    println!("\n  Undo one with `roster undo <number>`, or everything with `roster undo --all`.");
    Ok(0)
}

fn undo(app: &App, op: Option<u64>, last: bool, all: bool, yes: Yes) -> Result<u8, String> {
    let mut j = app.journal()?;
    let target = if all {
        None
    } else if let Some(o) = op {
        Some(o)
    } else if last {
        Some(
            j.ops()
                .iter()
                .rev()
                .find(|o| !o.undone)
                .map(|o| o.id)
                .ok_or("There is nothing to undo.")?,
        )
    } else {
        return Err(
            "Say which change to undo: a number, --last or --all. `roster changes` lists them."
                .into(),
        );
    };
    match target {
        Some(id) => {
            let summary = j
                .ops()
                .iter()
                .find(|o| o.id == id)
                .map(|o| o.summary.clone())
                .unwrap_or_default();
            println!("Undo change {id}: {summary}");
            match confirm(yes.yes) {
                Confirm::Yes => {}
                Confirm::No => return Ok(0),
                Confirm::CannotAsk => return Ok(2),
            }
            j.undo(id).map_err(|e| e.to_string())?;
            println!("Undone.");
        }
        None => {
            println!("Undo every change Agent Roster made.");
            match confirm(yes.yes) {
                Confirm::Yes => {}
                Confirm::No => return Ok(0),
                Confirm::CannotAsk => return Ok(2),
            }
            let done = j.undo_all().map_err(|e| e.to_string())?;
            println!("Undid {} changes.", done.len());
        }
    }
    Ok(0)
}

fn uninstall(app: &App, delete_login_folders: bool, yes: Yes) -> Result<u8, String> {
    println!("Uninstall Agent Roster");
    println!("  - Undoes every change it made, newest first.");
    println!("  - Removes its commands, history and settings.");
    if delete_login_folders {
        println!("  - Deletes the login folders of agents it created. They can't be brought back.");
    } else {
        println!(
            "    Keeps the login folders of agents it created, in {}.",
            app.p.accounts_dir().display()
        );
    }
    match confirm(yes.yes) {
        Confirm::Yes => {}
        Confirm::No => return Ok(0),
        Confirm::CannotAsk => return Ok(2),
    }
    let mut j = app.journal()?;
    j.undo_all().map_err(|e| e.to_string())?;
    for d in [
        app.p.bin_dir(),
        app.p.run_dir(),
        app.p.limits_dir(),
        app.p.journal_dir(),
    ] {
        let _ = std::fs::remove_dir_all(d);
    }
    if delete_login_folders {
        let _ = std::fs::remove_dir_all(app.p.accounts_dir());
    }
    let _ = std::fs::remove_file(app.p.roster_file());
    let _ = std::fs::remove_dir(&app.p.config_dir);
    let _ = std::fs::remove_dir(&app.p.data_dir);
    println!(
        "Agent Roster is uninstalled. Remove the `roster` program itself with the tool you installed it with."
    );
    Ok(0)
}

/// The path `roster` was started from, not its resolved target: Homebrew's
/// /opt/homebrew/bin/roster survives upgrades, the Cellar path behind it does not.
fn roster_bin() -> Result<PathBuf, String> {
    if let Some(arg0) = std::env::args_os().next().map(PathBuf::from) {
        if arg0.components().count() > 1 {
            let abs = if arg0.is_absolute() {
                arg0
            } else {
                std::env::current_dir()
                    .map_err(|e| e.to_string())?
                    .join(arg0)
            };
            if abs.is_file() {
                return Ok(abs);
            }
        } else if let Some(found) = (PathLookup { exclude: None }).find(&arg0.to_string_lossy()) {
            return Ok(found);
        }
    }
    std::env::current_exe().map_err(|e| e.to_string())
}

fn real() -> Result<u8, String> {
    let cli = Cli::parse();
    let p = RosterPaths::detect();
    let roster_bin = roster_bin()?;
    let app = App { p, roster_bin };
    let lookup = app.lookup();
    let ctx = app.ctx(&lookup);
    match cli.cmd {
        None => show_overview(&app, cli.json),
        Some(Cmd::Init {
            claude_name,
            codex_name,
            yes,
        }) => {
            let mut r = app.roster()?;
            let mut any = false;
            let path_var = env_var("PATH").unwrap_or_default();
            for (tool, name) in [(Tool::Claude, claude_name), (Tool::Codex, codex_name)] {
                let installed =
                    roster_core::detect::find_tool(tool, &path_var, &app.p.bin_dir()).is_some();
                let standard = expand_tilde(tool.standard_dir(), &app.p.home);
                let on_roster = r
                    .agents
                    .iter()
                    .any(|a| r.config_dir_of(a, &app.p) == standard);
                if !installed || on_roster {
                    continue;
                }
                any = true;
                let code = app.run_plan(ops::plan_adopt(&r, &app.p, &ctx, tool, &name), yes)?;
                if code != 0 {
                    return Ok(code);
                }
                r = app.roster()?;
            }
            if !any {
                println!(
                    "Nothing to add: your installed tools are already on the roster, or none are installed."
                );
            }
            Ok(0)
        }
        Some(Cmd::Add {
            name,
            claude,
            codex: _,
            color,
            role,
            share,
            yes,
        }) => {
            let req = AddRequest {
                name,
                tool: tool_of(claude),
                color: parse_color(color)?,
                role,
                share,
            };
            app.run_plan(ops::plan_add(&app.roster()?, &app.p, &ctx, req), yes)
        }
        Some(Cmd::Rename { old, new, yes }) => app.run_plan(
            ops::plan_rename(&app.roster()?, &app.p, &ctx, &old, &new),
            yes,
        ),
        Some(Cmd::Color { name, color, yes }) => {
            let c = parse_color(Some(color))?.expect("parsed");
            app.run_plan(
                ops::plan_set_color(&app.roster()?, &app.p, &ctx, &name, c),
                yes,
            )
        }
        Some(Cmd::Role { name, role, yes }) => app.run_plan(
            ops::plan_set_role(&app.roster()?, &app.p, &ctx, &name, role),
            yes,
        ),
        Some(Cmd::Share { name, state, yes }) => app.run_plan(
            ops::plan_set_share(&app.roster()?, &app.p, &ctx, &name, state == "on"),
            yes,
        ),
        Some(Cmd::Default { name, yes }) => app.run_plan(
            ops::plan_set_default(&app.roster()?, &app.p, &ctx, &name),
            yes,
        ),
        Some(Cmd::Remove {
            name,
            delete_login_folder,
            yes,
        }) => app.run_plan(
            ops::plan_remove(&app.roster()?, &app.p, &ctx, &name, delete_login_folder),
            yes,
        ),
        Some(Cmd::Login { name, device }) => {
            let r = app.roster()?;
            let a = r.by_name(&name).ok_or_else(|| {
                format!("No agent is named {name}. Run `roster` to see your agents.")
            })?;
            let args: Vec<String> = match a.tool {
                Tool::Claude => vec!["auth".into(), "login".into()],
                Tool::Codex if device || headless() => vec!["login".into(), "--device-auth".into()],
                Tool::Codex => vec!["login".into()],
            };
            run_agent(&app, &a.id.clone(), args)
        }
        Some(Cmd::Limits { action, name, yes }) => match (action.as_deref(), name) {
            (None, _) => show_overview(&app, cli.json),
            (Some(act), Some(name)) => {
                let r = app.roster()?;
                let plan = if act == "enable" {
                    ops::plan_enable_claude_limits(&r, &app.p, &ctx, &name)
                } else {
                    ops::plan_disable_claude_limits(&r, &app.p, &ctx, &name)
                };
                app.run_plan(plan, yes)
            }
            (Some(_), None) => Err("Say which agent: `roster limits enable <name>`.".into()),
        },
        Some(Cmd::Changes) => changes(&app),
        Some(Cmd::Undo { op, last, all, yes }) => undo(&app, op, last, all, yes),
        Some(Cmd::Import { yes }) => app.run_plan(plan_import(&app.roster()?, &app.p, &ctx), yes),
        Some(Cmd::Doctor) => doctor(&app),
        Some(Cmd::Ui) => open_ui(&app),
        Some(Cmd::Uninstall {
            delete_login_folders,
            yes,
        }) => uninstall(&app, delete_login_folders, yes),
        Some(Cmd::Run { id, args }) => run_agent(&app, &id, args),
        Some(Cmd::Statusline) => {
            let mut stdin = String::new();
            let _ = std::io::stdin().read_to_string(&mut stdin);
            let r = app.roster().unwrap_or_default();
            print!(
                "{}",
                roster_core::statusline::handle_statusline(
                    &stdin,
                    &env_var,
                    &r,
                    &app.p,
                    &roster_core::statusline::real_shell
                )
            );
            Ok(0)
        }
    }
}

fn main() -> ExitCode {
    match real() {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}
