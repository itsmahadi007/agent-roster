# Agent Roster — Design

**Date:** 2026-11-09
**Status:** APPROVED 2026-11-09 (v3)
**Domain:** agentroster.mahadihassan.com (free subdomain of the owner's domain, DNS on Cloudflare; replaces agentroster.mahadihassan.com, which costs money)
**Owner:** Mahadi Hassan
**Mockups:** `docs/mockups/app-window.html`, `docs/mockups/terminal.html` (PNG copies alongside)
**Working prototype:** `prototype/agent-roster` (bash, macOS, installed and tested on the owner's Mac)

---

## 1. Intent

People increasingly run more than one AI coding account: two Claude Code subscriptions, two Codex
subscriptions, a personal one and a work one, their own and their boss's. Today both CLIs support
this only through an environment variable that points at a separate config folder
(`CLAUDE_CONFIG_DIR`, `CODEX_HOME`). Nobody sets that up by hand without friction, and no tool
gives the accounts names that both the human and the AI understand.

**Agent Roster** lets anyone give each account a name they choose (`boss`, `worker1`, `agent3`),
then:

- type that name in **any terminal** (Terminal, iTerm, cmux, Warp, VS Code, Windows Terminal) to
  open that tool on that account;
- let **any agent call any other agent by name** ("get worker2 to make the hero image"), because
  every agent is told who is on the roster and how to call them;
- see every account's **login state, limits and reset times** in one place;
- do all of this on **macOS, Windows and Linux**, installed the way people install other tools
  (`brew`, `winget`, `apt`/`dnf`, a one-line script).

The owner wants it for himself and his boss today, and as a public open-source project.

### What makes it special
1. **Names that the AI understands.** Other switchers only swap logins. Agent Roster writes a
   roster note into each agent's instructions, so delegation between accounts and between tools
   (Claude ↔ Codex) is natural language. Which agent is good at what (e.g. Codex for images) is
   left to the agents' own intelligence, not hard-coded by us.
2. **Never breaks anything.** It previews every change, records every change with a backup, can
   undo any of them, and never installs hooks. (Some agent workspaces add hooks to your tools' configs without asking; this is the anti-example.)
3. **Two faces, one core.** A `roster` command for terminal people and a small, good-looking app
   window for everyone else, both editing the same roster.

## 2. Goals, non-goals, anti-goals

### Goals (v1)
- G1. Add, rename, recolor, set-default and remove agents for **Claude Code** and **Codex**.
- G2. A launcher command per agent name, usable from any terminal, on all three OSes.
- G3. The existing default logins (`~/.claude`, `~/.codex`) are **adopted in place** as the first
  agents. They are never moved or rewritten.
- G4. Sign-in per agent from the app (opens a terminal running that agent's normal login) or CLI.
- G5. Roster note so agents can call each other by name, with optional one-line **role** per agent.
- G6. Limits and reset times per agent (Codex always; Claude opt-in, see §6.6).
- G7. Live view of which agent is working for which (the "cable").
- G8. Change journal with per-change undo, undo-all, and uninstall that restores everything.
- G9. Menu bar (macOS) / tray (Windows, Linux) with quick "open a terminal as …".
- G10. Install with one line per OS (`curl | sh`, `irm | iex`), plus Homebrew formula (own tap),
  Scoop bucket, and later .deb/.rpm/AppImage; CLI also via `cargo install agent-roster`.
  Homebrew cask and winget wait for code signing (§7.2).
- G11. Import the owner's existing bash-prototype roster on first run.
- G12. **Works fully without a GUI.** Every action is available from the `roster` CLI, so a
  headless Linux server or any machine reached over SSH can add agents, sign in, see limits and
  undo, with no app installed. (A full-screen terminal UI for SSH follows in v0.2, §6.8.)

### Non-goals (v1)
- Gemini CLI or other tools (designed for, built later; the UI shows "Gemini · soon").
- The full-screen terminal UI (v0.2). v0.1 covers SSH with plain CLI commands.
- Syncing rosters or logins between computers. Logins stay per machine, always.
- Auto-update inside the app (package managers handle updates).
- Any team/server/cloud component.

### Anti-goals (failure even if it "works")
- **Account rotation / quota pooling.** Switching accounts automatically to dodge one workload's
  rate limit breaks Anthropic's and OpenAI's terms. Agent Roster never does it and never suggests
  it. Delegation is always an explicit choice by the user or the calling agent.
- Touching anything outside its own folders and its own **marked sections** without a preview.
- Installing hooks into any tool.
- Storing, reading or transmitting credentials. Agent Roster never opens `auth.json`,
  `.credentials.json` or Keychain entries.
- Telemetry by default.
- A heavy app. Target installed size under 30 MB for the macOS and Windows app (some Electron
  agent workspaces exceed 600 MB). The Linux AppImage is exempt because it bundles WebKitGTK; .deb/.rpm use the system one.

## 3. Users and moments
- **The owner (Mahadi):** two Claude + two Codex accounts, works in cmux, wants `boss` to hand work
  to `worker1/2` and to Codex for images.
- **His boss:** less terminal-heavy; uses the app window to set up, then types names in a terminal.
- **Public OSS user:** a developer with a personal and a work subscription, on any OS.

## 4. Product shape (decided)

| Decision | Choice | Decided by |
|---|---|---|
| Form | `roster` CLI **plus** its own small desktop window (not a browser tab) | Owner (picked "C, with A's window") |
| Daily use | Type the agent's name in any terminal; app need not be running | Owner |
| Platforms | macOS, Windows, Linux | Owner |
| Distribution | Install like other tools: brew / winget / apt-dnf / script | Owner |
| Visual direction | "Patch bay": deep navy, user-chosen agent colors, names set large in a wide face, live cable between calling agents | Owner approved mockups v3 |
| Limits + reset times | Shown on every agent, app and terminal | Owner |
| Safety features | Preview, journal + undo, no hooks, sharing off by default | Owner ("keep those stable features") |
| Desktop stack | **Tauri 2** (latest stable 2.12.x), not Electron | Claude's call, owner agreed |
| Core language | **Rust** workspace shared by CLI and app | Claude's call |
| Frontend | **Svelte 5 + Vite** inside Tauri | Claude's call |
| License | MIT | Claude's call (owner's guess matched) |

## 5. Architecture

```
agent-roster/                 (cargo workspace, public repo)
├─ crates/
│  ├─ roster-core/            all logic, no UI
│  │   roster model + file IO · name rules · account folders · launchers
│  │   roster note writer · sharing · change journal (backup/undo)
│  │   tool detection · limits readers · session (cable) records
│  │   platform adapters: macos / linux / windows
│  └─ roster-cli/             binary `roster` (clap 4)
├─ app/                       Tauri 2 shell + Svelte 5 UI, calls roster-core via commands
├─ packaging/                 brew tap formula+cask, winget manifests, install.sh / install.ps1
└─ docs/
```

One core means the CLI and the app can never disagree. The app is a thin UI over `roster-core`
functions exposed as Tauri commands.

### 5.1 Files Agent Roster owns

| What | macOS / Linux | Windows |
|---|---|---|
| Roster file | `~/.config/agent-roster/roster.toml` | `%APPDATA%\AgentRoster\roster.toml` |
| Account folders (secondary agents) | `~/.agent-roster/accounts/<id>/` | `%LOCALAPPDATA%\AgentRoster\accounts\<id>\` |
| Launchers | `~/.agent-roster/bin/` | `%LOCALAPPDATA%\AgentRoster\bin\` |
| Journal + backups | `~/.agent-roster/journal/` | `%LOCALAPPDATA%\AgentRoster\journal\` |
| Live session records | `~/.agent-roster/run/` | `%LOCALAPPDATA%\AgentRoster\run\` |

Account folders are keyed by a **stable id**, not the name, so renaming `worker1` → `designer`
renames the launcher and roster entry but never moves a login folder.

### 5.2 Roster file (format is a locked decision, see §9)

```toml
version = 1

[[agent]]
id = "a1b2c3"
name = "boss"
tool = "claude"                 # claude | codex   (gemini later)
config_dir = "~/.claude"        # adopted default, never moved
color = "cobalt"
role = "plans work and reviews"  # optional, goes into the roster note
default = true                  # bare `claude` resolves here (only one per tool)
share = false                   # share skills/agents/commands with the primary of this tool
limits = "codex-sessions"       # codex-sessions | claude-statusline | off
```

### 5.3 Launchers
- Each name becomes a tiny shim in the launcher folder: a shell script on macOS/Linux, a `.cmd`
  (plus a no-extension sh shim for Git Bash) on Windows.
- The shim only runs `roster run <id> -- <args>`. `roster run`:
  1. sets `CLAUDE_CONFIG_DIR` or `CODEX_HOME` for that agent (unsets it for an adopted default);
  2. clears `CLAUDECODE` so an agent can launch another agent from inside a session;
  3. sets `ROSTER_AGENT=<id>` and records parent → child if a parent `ROSTER_AGENT` was present
     (this is how the cable is drawn, no hooks needed);
  4. prints a one-line colored banner and sets the terminal tab title, **only when stdout is an
     interactive TTY**, so `-p` / `exec` output stays clean for calling agents;
  5. spawns the real `claude` / `codex` found on PATH (so cmux's own shims still wrap it and
     cmux notifications keep working), forwards signals, returns its exit code, writes the end
     record.
- Name rules: letters, digits, `-`, `_`; must not start with a digit; must not collide with an
  existing command on PATH (unless it is one of our own shims). Plain digits like `1` are refused
  with a clear message suggesting `a1`.
- **PATH:** the launcher folder must be on PATH. If it is not, Agent Roster shows the exact change
  (one marked line in the user's shell rc on macOS/Linux; the user PATH value on Windows), asks,
  journals it, and can undo it.
- Bare `claude` / `codex` are **not** shadowed. They keep opening the adopted default, exactly as
  before installation. "Default" in the UI means "which agent bare `claude` is"; changing the
  default to a secondary agent is offered only as an explicit opt-in that installs a `claude`
  shim, previewed and journaled. (Claude's call: shadowing the real command by default is too
  invasive for "never break anything".)

### 5.4 Roster note (how agents know each other)
- Written between `<!-- agent-roster:begin -->` / `<!-- agent-roster:end -->` markers in:
  - `~/.claude/CLAUDE.md` and `~/.codex/AGENTS.md` for adopted defaults;
  - each secondary agent's own `CLAUDE.md` / `AGENTS.md` inside its account folder (files we create).
    If that path is a symlink (the bash prototype made them), it is replaced by a real file first,
    so writing never edits the primary's file through the link.
- Content: the table of names, tools and roles; how to call another agent non-interactively
  (Claude: `<name> -p "<task>"`, add `--permission-mode acceptEdits` to let it edit; Codex:
  `<name> exec "<task>"`, `--full-auto` to let it edit); "never call bare `claude`/`codex` from
  inside a session, use a roster name"; "pass a self-contained task, relay the result"; how to check
  which account you are.
- It lists what each agent is good at ("Good at" column): the tool's defaults (Claude Code: coding, reviewing code, long tasks; Codex: coding, making images), or what the person set. Changed by the owner on 2026-11-10, replacing the earlier rule that the note doesn't list capabilities. A Leader role adds one sentence naming the leader(s).
- Rewritten on every roster change; re-running is idempotent (verified in the prototype).

### 5.5 Sharing (off by default)
One toggle, labelled **"Share my skills"** (the mockup's "skills and settings" label changes).
- **Claude, when on:** the account folder links to the primary's `skills/`, `agents/`,
  `commands/` (symlinks on macOS/Linux, directory junctions on Windows, which need no admin), and
  the secondary's own `CLAUDE.md` gets an `@import` of the primary's `CLAUDE.md`, so global rules
  follow without a file link.
- **Codex, when on:** links `skills/` only. Codex has no import, so the primary's `AGENTS.md` is
  copied into the secondary's `AGENTS.md` above the roster note and re-copied on every roster
  write. The copy is marked so the user knows where it came from.
- `settings.json`, `hooks/` and `config.toml` are **never** shared by the toggle: hooks and status
  lines inside them can misbehave under a second account. An "Advanced" row per agent lets a user
  link them explicitly, previewed and journaled. (Claude's call.)

### 5.6 Change journal
- Every mutation is an operation record: what, when, which files, and a backup of each file's
  previous content (or "did not exist").
- `roster changes` / the "What changed" screen list them; `roster undo <n>` and `roster undo --all`
  restore them. Uninstall runs undo-all, then deletes Agent Roster's own folders. Account folders
  with logins are kept unless the user explicitly asks to delete them (shown with a warning).
- Undo refuses if a file changed since we wrote it, and shows the difference instead of
  overwriting the user's newer edits.
- **Uninstall boundary (Claude's call):** `roster uninstall` (and the app's Uninstall button) is
  the command that restores everything. The macOS cask and the Windows NSIS uninstaller call it.
  Homebrew formulas, apt/dnf and winget cannot run per-user cleanup, so their docs and the
  post-install message say "run `roster uninstall` first". Leftovers after a bare package removal
  are only Agent Roster's own folders and marked sections, never broken user config.

## 6. Features in detail

### 6.1 First run
Detect `claude` and `codex` on PATH and their versions, and whether each default folder is signed
in. Claude: `claude auth status --json` gives `loggedIn`, `email`, `subscriptionType`, `planTier`
(verified 2026-11-09). Codex: `codex login status` prints "Logged in using ChatGPT" or similar and
gives no email, so Codex rows show the plan from limits data instead. Show them as the first agents with suggested names, offer rename, change nothing until
**Continue**.

**Read boundary (Claude's call).** Allowed: running each tool's own status command
(`codex login status`, Claude's equivalent) and reading **non-secret account metadata** such as
the account email Claude keeps in its user state file. Forbidden: opening token files
(`auth.json`, `.credentials.json`) or Keychain/Credential Manager secrets. Where no allowed source
gives an email, the row shows the tool name only.

**Importing the bash prototype (Claude's call).** If `~/.config/agent-roster/roster.conf` and
marked launchers exist, offer import with a full preview:
- Adopt every prototype folder **in place** (`config_dir = ~/.claude-2` etc.). Never move them:
  moving would break the macOS Keychain login, which is keyed to the folder path.
- Keep the agents' names and order.
- Found shared links (`settings.json`, `hooks`, `CLAUDE.md`, `config.toml`, `skills`) are listed.
  Default: keep `skills` (maps to "Share my skills" on), keep `settings.json`/`hooks` as the
  explicit Advanced links so the owner's working setup does not change, and replace linked
  `CLAUDE.md`/`AGENTS.md` with real files before writing notes.
- Old launchers in `~/.local/bin` (marker line 2) are removed and replaced by v1 launchers, and the
  old marked blocks are rewritten in v1 format. Every step is journaled, so import is undoable.

### 6.2 Add agent
Name typed large in its color, color swatches, tool choice, share toggle (off), optional role,
"What will happen" preview, then **Add and sign in**. CLI: `roster add <name> --claude|--codex
[--color] [--role] [--share] [--yes]`.

### 6.3 Sign in
The app opens the user's terminal with that agent's login running: Claude `claude auth login`,
Codex `codex login` (both run through `roster run` so the right folder is used). Over SSH, Codex
uses `codex login --device-auth`; Claude's flow prints a link to open elsewhere (spike confirms). macOS: Terminal or the user's chosen app; Windows: Windows Terminal,
falling back to `cmd`; Linux: `$TERMINAL`, then `x-terminal-emulator`, gnome-terminal, konsole.
Status flips to "ready" when the login is detected.

### 6.4 Main window
Patch-bay rows per mockup v3: stripe and name in the agent's color, tool, account email (when the
tool reports it), limit bars with "resets in", status (ready / needs sign in / working / idle),
jack on the right. A cable joins jacks while one agent is running another (from `run/` records),
with one pulse animation (respects reduced motion). Side panel: "On the line now" (who asked whom,
the task text when available, where, since when), the names to type, limit warnings, the
no-hooks line.

### 6.5 Terminal UI
`roster` with no arguments prints the overview from the terminal mockup (agents, status, limit bars,
reset times, live calls). Other commands: `add`, `rename`, `color`, `role`, `default`, `remove`,
`login`, `limits`, `changes`, `undo`, `ui` (opens the app), `doctor` (checks PATH, tool versions,
markers), `run` (internal). Colors respect `NO_COLOR` and non-TTY output. `--json` on read commands.

### 6.6 Limits and reset times
- **Codex:** read the newest session file under that agent's `CODEX_HOME/sessions/` and take the
  last `rate_limits` record (`used_percent`, `window_minutes`, `resets_at`, `plan_type`). Read-only.
  Verified present on the owner's Mac 2026-11-09.
- **Claude Code:** limits reach only the status line (`rate_limits.five_hour` / `seven_day` with
  `used_percentage`, `resets_at`). Capturing them needs a status line command in that agent's
  `settings.json`. This is **opt-in per Claude agent, off by default**, previewed and journaled.
  If the agent already has a status line, ours runs the existing one with the same input and
  prints its output unchanged, so the user's line is preserved. (Claude's call: owner was asked
  "ask first or off by default" and moved on; off-by-default with a clear "Turn on limits" button
  satisfies both.)
- Both: show "as of <time>"; numbers update when an agent is used. 90% or more is coral.

### 6.7 Menu bar / tray
List agents with color dot and week %, "Open a terminal as …", "Add agent", "Open Agent Roster".
Built with Tauri's tray support.

### 6.8 Without a GUI: SSH and servers
- **v0.1:** the CLI is complete on its own (G12). On a machine with no display, `roster ui` says
  "no desktop here, use the commands" instead of failing. Sign-in over SSH uses each tool's
  terminal flow: the tool prints a link or a device code to open on any other device (builder to
  confirm the exact flag per tool version, e.g. a device-code option for Codex). Live calls
  ("cable") show in `roster` output as text lines.
- **v0.2: full-screen terminal UI.** `roster` opens an interactive screen (ratatui) that mirrors
  the app: patch-bay rows in agent colors, limit bars, live calls, keys for add / rename / sign in
  / undo. It is option B from the first mockup round, built on the same `roster-core`, so it adds
  no new logic. `roster --plain` keeps today's printed overview for scripts.

## 7. Platforms, releases and distribution

### 7.1 Release order (owner decided 2026-11-09)
The owner is on macOS and his boss is on Windows. The owner added Linux to the first release too.

| Release | Platforms | What ships |
|---|---|---|
| **v0.1** | macOS (Apple Silicon + Intel), Windows 10/11, Linux (x86_64 + arm64) | CLI + app window + tray on all three; CLI-only install for headless/SSH machines; replaces the bash prototype on the owner's Mac; boss installs on Windows |
| **v0.2** | all | Full-screen terminal UI for SSH (§6.8) |
| **v1.0** | all | Package-manager channels polished, docs, public announcement |

### 7.2 No code signing for now (owner decided 2026-11-09)
No Apple Developer ID and no Windows certificate until real usage justifies it. The distribution
is chosen so unsigned builds still install cleanly:
- **macOS:** builds are ad-hoc signed (required on Apple Silicon, free). The primary install is a
  one-line script: `curl -fsSL agentroster.mahadihassan.com | sh`. Files fetched by `curl` carry no quarantine
  flag, so Gatekeeper does not block the app. The script installs the `roster` CLI and puts
  `Agent Roster.app` in `~/Applications`. A Homebrew **formula** for the CLI goes in the owner's tap
  (formulas are not quarantined). The Homebrew **cask** waits for signing.
- **Windows:** primary install is `irm agentroster.mahadihassan.com/install.ps1 | iex` (PowerShell downloads carry no
  Mark-of-the-Web, so SmartScreen does not interrupt). Also an own **Scoop** bucket, which accepts
  unsigned apps. The NSIS `.exe` on GitHub Releases works but shows a one-time SmartScreen warning,
  documented with screenshots. **winget** is attempted; if unsigned submissions are flagged, it
  waits for signing.
- A manual `.dmg` download is documented with the one-time "Open anyway" step in System Settings.
- When signing arrives: notarize in CI, then add the Homebrew cask and winget.

- **Linux:** `curl -fsSL agentroster.mahadihassan.com | sh` detects the distro and whether a desktop is
  present. With a desktop it installs CLI + app (`.deb` / `.rpm`, or AppImage elsewhere). Without
  one (a server, an SSH box) it installs the **CLI only**, which has no WebKitGTK or GUI
  dependency. `.deb`/`.rpm`/AppImage also on GitHub Releases; AUR later.

### 7.3 Builds
GitHub Actions with `tauri-action`: macOS universal `.app` (zipped for the script) and `.dmg`,
Windows NSIS `.exe` and portable zip, Linux `.deb`, `.rpm`, `.AppImage` (x86_64 and arm64).
**CLI-only archives per target** (static where possible, musl on Linux) for headless machines.
`cargo install agent-roster` for Rust users.

### 7.4 Windows test access
The owner has no Windows machine. Spike and release testing use a **Windows 11 ARM VM in UTM**
on the owner's Mac (free evaluation image) plus GitHub's Windows runners for automated tests.
Final sign-off is a 15-minute check on the boss's PC before v0.1 is announced to him.
Linux uses an **Ubuntu desktop VM in UTM** for the app and tray, plus Docker containers (Ubuntu,
Fedora, Debian) for headless CLI installs and SSH use.

## 8. Solo-AI feasibility (playbook Gate 1, folded in)
| Surface | Load | Note |
|---|---|---|
| Build | 🟡 | Rust + Svelte + Tauri; AI-led build is realistic; the core is small |
| Packaging on 3 OSes | 🟡 | The heaviest part; `tauri-action` covers most of it |
| Ops | 🟢 | No servers. Releases only |
| Maintenance | 🟡 | Breaks if Claude Code or Codex change config-dir or limits formats; `roster doctor` + tests on fixtures catch it early |
| Bus factor of one | 🟢 | Tiny surface, MIT, documented |
| Money | n/a | Free OSS. Portfolio + daily-use tool. Gate 0 verdict: not a business, build because the owner and his boss use it daily |

## 9. Decision ledger (playbook Gate 3, folded in)
| # | Decision | Reversible? | Status |
|---|---|---|---|
| D1 | Rust core + Tauri 2 shell | Expensive to reverse | **Lock after spike** |
| D2 | Roster file format v1 + id-keyed account folders | Expensive (users' data) | **Lock after spike** |
| D3 | Launchers call `roster run` (not plain env shims) | Cheap | Keep swappable |
| D4 | Svelte 5 frontend | Cheap | Swappable |
| D5 | Never shadow bare `claude`/`codex` by default | Cheap | Revisit if users ask |
| D6 | Claude limits via opt-in status line | Cheap | Revisit if Claude exposes limits another way |
| D7 | Own Homebrew tap before official cask | Cheap | Revisit at notability |
| D8 | CLI ships as its own package with no GUI dependency; the app depends on the CLI, never the reverse | Medium | Lock after spike |

## 10. De-risking spike (playbook Gate 2, folded in) — before the real build
Throwaway, one to two days, proves the riskiest assumptions on **every** OS:
1. **Login isolation per folder on Windows and Linux.** Two folders, two accounts, both stay signed
   in at once for Claude Code (no macOS Keychain there) and for Codex.
2. **Launcher on PATH** on Windows (`.cmd` in cmd, PowerShell, Windows Terminal, Git Bash) and
   Linux (bash, zsh, fish).
3. **Agent calling agent** non-interactively on each OS (`<name> -p` / `<name> exec`), including
   from inside a running session.
4. **Tauri 2 window + tray + "open terminal running X"** on each OS.
5. **Codex limits** readable from session files on each OS.
6. **Unsigned install path:** the `curl | sh` app install opens without a Gatekeeper block on
   macOS, and the `irm | iex` install runs without SmartScreen on Windows.
7. **Headless:** the CLI-only build installs and fully works in a Linux container over SSH,
   including a sign-in flow with no browser on that machine.
Pass = all seven on macOS, Windows and Linux. Any failure shrinks scope before the real build.

## 11. What exists today
- Bash prototype installed on the owner's Mac (`agent1..4`, roster note in `~/.claude/CLAUDE.md`
  and `~/.codex/AGENTS.md`, `~/.claude-2` and `~/.codex-2` with shared-config symlinks). Tested:
  `agent1 -p` and `agent3 exec` from inside a Claude Code session. Agent Roster v1 imports it.
- Mockups v3 (app window with limits) and terminal mockup, approved in conversation.

## 12. Left to the builder
- Exact crate choices (directories, toml, serde, which, notify), test layout, CI matrix details.
- Svelte component structure and state management.
- Exact copy beyond what the mockups show, following their voice: plain verbs, sentence case.
- How to detect "signed in" per tool without reading secrets (file/Keychain-entry presence).

## 13. Owner decisions after the builder check (2026-11-09)
1. **Code signing:** none for now; add certificates if real usage appears (see §7.2).
2. **Boss's OS is Windows, owner's is macOS:** v0.1 ships both, and Linux too (see §7.1).
3. **Non-GUI for SSH:** v0.1 CLI works fully headless; full-screen terminal UI in v0.2 (§6.8).
4. **Name and domain:** keep "Agent Roster". No paid domain: the owner has no budget, so the
   project uses `agentroster.mahadihassan.com`, a free Cloudflare subdomain of his existing domain,
   serving the install scripts and a small landing page. The install line is
   `curl -fsSL agentroster.mahadihassan.com | sh`. Known name clash: agentroster.dev is a live, different product
   (role prompts for coding agents); accepted by the owner.

## 14. Calls made after the builder check (owner to skim)
- Sharing is one "Share my skills" toggle; settings and hooks only via an explicit Advanced link.
- Codex sharing copies the primary `AGENTS.md`, since Codex has no import.
- Reading non-secret account metadata and running tools' own status commands is allowed;
  token files and keychain secrets are not.
- Prototype import adopts folders in place and keeps the owner's current links so nothing changes
  on day one.
- `roster uninstall` is the full restore; package-manager removals point to it.
- The 30 MB cap applies to macOS and Windows; the Linux AppImage is exempt.
