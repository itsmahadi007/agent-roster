# Plan 2 — App Window, Tray and `roster ui` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Tauri 2 + Svelte 5 desktop window and menu bar/tray for Agent Roster that is a thin UI over `roster-core`, plus `roster ui` opening it.

**Architecture:** Everything the window shows is computed in `roster-core` (new `view` and `desktop` modules) and returned as serializable view structs; the app crate (`app/src-tauri`, workspace member `agent-roster-app`) holds a `Backend` that wraps core calls and is exposed through thin `#[tauri::command]` wrappers. Changes go preview → token → apply, so the exact plan the user saw is the one applied. The Svelte frontend only renders view structs and calls commands.

**Tech Stack:** Rust 2024, Tauri 2.12.5 (`tauri-build` 2.7.4, `tauri-cli` 2.12.4, features `tray-icon`, `image-png`), Svelte 5.57, Vite 8, `@sveltejs/vite-plugin-svelte` 7, Vitest 5 + jsdom + `@testing-library/svelte` 5, TypeScript 6.0 (svelte-check 4.7 does not accept TS 7), `@tauri-apps/api` 2.12, `@fontsource-variable/archivo` + `@fontsource/jetbrains-mono` (fonts bundled, the app works offline).

**Spec:** `docs/superpowers/specs/2026-11-09-agent-roster-design.md` §4–§6.7. **Visual source of truth:** `docs/mockups/app-window.html` (+ `app-window.png`).

## Global Constraints

- Desktop stack: Tauri 2 latest stable 2.x (2.12.5 today), not Electron; frontend Svelte 5 + Vite (spec §4).
- Visual direction "Patch bay": deep navy, user-chosen agent colors, names set large in a wide face (Archivo `wdth` 125, weight 800), live cable between calling agents (spec §4). Copy every color token from the mockup's `.pb` block verbatim: `--base:#111925 --raise:#18222f --well:#0c131c --line:#26323f --txt:#e6ebf2 --mut:#8592a5 --dim:#56637a`, agent colors from `AgentColor::hex()`.
- "90% or more is coral" (spec §6.6) — decided in core (`hot`), never in JS.
- No roster logic in JavaScript: names, validation, previews, limit math, reset text, states, labels all come from Rust.
- Every change: preview first, applied in one journal transaction, undoable (spec §2 anti-goals, §5.6). No hooks. Never read `auth.json`, `.credentials.json` or keychain secrets. Never write through a link into another account's files.
- Sign in: "opens the user's terminal with that agent's login running" — macOS: the user's default app for `.command` files (Terminal unless changed); Windows: Windows Terminal, falling back to `cmd`; Linux: `$TERMINAL`, then `x-terminal-emulator`, gnome-terminal, konsole (spec §6.3).
- Menu bar / tray: "List agents with color dot and week %, 'Open a terminal as …', 'Add agent', 'Open Agent Roster'" (spec §6.7). macOS tray icon is a monochrome template image.
- Share toggle label is **"Share my skills"** (spec §5.5 overrides the mockup's "skills and settings").
- `roster ui`: opens the app when installed; otherwise prints exactly today's text `The app window isn't installed. Everything works from these commands; run \`roster --help\` to see them.`; on Linux with no display prints `There's no desktop here. Everything works from these commands; run \`roster --help\` to see them.` (spec §6.8).
- The CLI never depends on the app (D8). `crates/roster-cli` gets no Tauri dependency.
- App identity: productName `Agent Roster`, identifier `com.mahadihassan.agentroster`, binary `agent-roster-app`.
- Icon: `assets/icon/icon-a.png` through `cargo tauri icon`. Plain sentence-case copy, plain verbs, no emoji.
- No real emails or names in tests or fixtures: use `you@example.com`, `boss@example.com`.
- Checks before every commit: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, plus `npm test` in `app/` once it exists. Local Windows clippy covers `-p roster-core -p agent-roster` only (the app's Windows build is checked in CI).
- All 104 existing tests keep passing. Core behavior changes only with a test.

## Review Focus

1. **App started from Finder/Dock has PATH `/usr/bin:/bin:/usr/sbin:/sbin`.** A person expects their agents to show as installed and signed in, and new launchers to call their real `roster`. → Task 5 resolves the login-shell PATH at startup (test `login_path_parses_marked_output`), Task 3 locates the `roster` binary (test `finds_roster_in_home_candidates`), and Task 5's `Backend` refuses previews when no `roster` binary is found (test `preview_refuses_without_roster_binary`).
2. **The roster changes between preview and apply** (the CLI ran in another terminal). A person expects the app not to apply a stale plan over it. → Task 5 test `apply_refuses_when_roster_changed_since_preview`.
3. **Paths with spaces** (`/Users/Jo Smith/.local/bin/roster`) in the terminal command. A person expects Sign in to work. → Task 3 test `mac_command_file_quotes_paths_with_spaces`.
4. **No agents and no tools installed** on first launch. A person expects a first-run screen that says nothing is installed and offers no broken Continue. → Task 8 test `first_run_without_tools_disables_continue`.
5. **A slow `claude auth status`** while the window polls. A person expects one request at a time, not a pile-up. → Task 6 test `poll_skips_while_a_request_is_in_flight`.

---

### Task 1: Core view of the overview, changes and first run

**Files:**
- Create: `crates/roster-core/src/view.rs`, `crates/roster-core/tests/view.rs`
- Create (generated by the test): `app/src/lib/fixtures/overview.json`, `app/src/lib/fixtures/first-run.json`, `app/src/lib/fixtures/changes.json`
- Modify: `crates/roster-core/src/lib.rs` (add `pub mod view;`), `crates/roster-cli/src/render.rs` (use `roster_core::view::{age, now}` instead of its own copies)

**Interfaces:**
- Consumes: `status::{Overview, AgentStatus, AgentState}`, `limits::format_reset`, `journal::Op`, `detect::{tool_info, sign_in_for, CommandRunner}`, `import::detect_prototype`.
- Produces (all `#[derive(Debug, Clone, PartialEq, Serialize)]`, field names serialize as written):
  - `pub fn now() -> i64`, `pub fn age(secs: i64) -> String` (moved verbatim from CLI render: "just now", "12m ago", "3h ago", "2d ago")
  - `WindowView { label: String, used_percent: u32, hot: bool }` (`used_percent` rounded; `hot = used_percent >= 90`)
  - `AgentView { id, name, tool: Tool, tool_label: String, color: String /*hex*/, email: Option<String>, default_label: Option<String> /*"default for claude"|"default for codex"*/, state: String /*"ready"|"sign-in"|"working"|"not-installed"*/, status_text: String, windows: Vec<WindowView>, reset_text: Option<String>, reset_hot: bool, limits_note: Option<String>, claude_limits: Option<bool>, live: bool, tray_text: String, tray_hot: bool }`
  - `CallView { from_id, from_name, from_color, to_id, to_name, to_color, task: Option<String>, detail: String }`
  - `WarningView { name, color, text }`
  - `OverviewView { headline: String, agents: Vec<AgentView>, calls: Vec<CallView>, warnings: Vec<WarningView>, limits_updated: Option<String> }`
  - `pub fn overview_view(o: &Overview, home: &Path, now: i64) -> OverviewView`
  - `CandidateView { tool: Tool, tool_label, installed: bool, version: Option<String>, email: Option<String>, signed_in: bool, suggested_name: String }`, `FirstRunView { candidates: Vec<CandidateView>, prototype: bool }`
  - `pub fn first_run_view(r: &Roster, p: &RosterPaths, path_var: &str, runner: &(dyn CommandRunner + Sync)) -> FirstRunView`
  - `ChangeView { id: u64, summary: String, at: i64, undone: bool, color: Option<String> }`
  - `pub fn changes_view(ops: &[Op], r: &Roster) -> Vec<ChangeView>` (newest first)

Rules the tests pin (from the mockup and the CLI's existing render):
- `status_text`: "calling <child name>" when the agent is the parent of a live call; else `AgentState::label()` ("ready", "sign in", "working", "not installed").
- `reset_text`: one window → "resets in 21h 52m"; several → "<last window label> resets in 3d 16h". `reset_hot` = that window is hot.
- `limits_note` (only when no windows): NeedsSignIn → "limits appear after sign in"; Claude with `LimitsSource::Off` → "limits off"; NotInstalled → "not installed"; else "no limit data yet".
- `claude_limits`: `None` for Codex, `Some(limits != Off)` for Claude.
- `tray_text`: "61% week" from the window labelled "week" (else the last window), "sign in", "not installed", or "" ; `tray_hot` from that window.
- `live`: agent is parent or child of any call.
- `headline`: "Four accounts, one quota each. Type a name in any terminal to start it." (number words one..ten, then digits; "One account. Type its name in any terminal to start it." for one).
- `CallView.detail`: "started 2m ago in ~/projects/portfolio" (`age`, cwd through `collapse_tilde`).
- `WarningView.text` per hot window: "has used 93% of its week. It resets in 21h 52m." (label "week" → "its week", other labels → "its 5 hr limit").
- `limits_updated`: "Last update 4m ago." from the newest `as_of`.
- `changes_view` color: hex of the first roster agent whose name appears as a whole word in the summary.
- `first_run_view`: one candidate per tool (Claude, Codex) whose standard folder is not on the roster; `suggested_name` "claude1"/"codex1" (same defaults as `roster init`); sign-in read with `sign_in_for` on a temporary adopted-default `Agent`; `prototype = detect_prototype(p).is_some()`.

- [ ] **Step 1: Write the failing tests** in `crates/roster-core/tests/view.rs`, building `Overview` values by hand with `now = 1_800_000_000`:
  - `calling_parent_and_working_child` — call agent1→agent3: agent1 `status_text == "calling agent3"`, both `live`, `calls[0].detail == "started 2m ago in ~/projects/portfolio"`.
  - `hot_single_window_is_coral` — agent3 week 93%, resets in 21h52m: `windows == [WindowView{label:"week",used_percent:93,hot:true}]`, `reset_text == Some("resets in 21h 52m")`, `reset_hot`, `tray_text == "93% week"`, `warnings[0].text == "has used 93% of its week. It resets in 21h 52m."`.
  - `two_windows_reset_line_names_the_last` — 5 hr 22%, week 61% → `reset_text == Some("week resets in 3d 16h")`, `!reset_hot`, `tray_text == "61% week"`.
  - `notes_when_no_limits` — needs sign in → "limits appear after sign in" and `tray_text == "sign in"`; Claude Off → "limits off", `claude_limits == Some(false)`; Codex → `claude_limits == None`.
  - `headline_counts_in_words` — 4 agents → starts "Four accounts"; 1 agent → "One account. Type its name in any terminal to start it."; 12 agents → starts "12 accounts".
  - `changes_newest_first_with_agent_color` — ops "Added boss (Codex)" and "Updated notes": first item is the newest; the boss op has orchid hex; "Updated notes" has `None`; "Added bossy" does not match boss.
  - `first_run_skips_tools_already_on_roster` — temp home with fake `claude` on PATH, roster already adopting `~/.claude`: no Claude candidate; Codex candidate `installed == false`.
  - `fixtures_match` — serializes a fixed sample (agent1 cobalt Claude 22%/61% calling agent3; agent2 marigold Claude needs sign in; agent3 jade Codex 93%; boss orchid Codex 8%/24% ready) plus a first-run and a changes sample with `serde_json::to_string_pretty`, compares to the three files under `app/src/lib/fixtures/`; when `UPDATE_FIXTURES=1` writes them instead.
- [ ] **Step 2: Run** `cargo test -p roster-core --test view` — Expected: FAIL, `unresolved import roster_core::view`.
- [ ] **Step 3: Implement** `view.rs` with the interfaces above; move `now`/`age` from `crates/roster-cli/src/render.rs` and re-export through the CLI call sites.
- [ ] **Step 4: Run** `UPDATE_FIXTURES=1 cargo test -p roster-core --test view fixtures_match`, inspect the three JSON files, then `cargo test --workspace` — Expected: PASS, 104 + new tests.
- [ ] **Step 5: Commit** `git commit -m "feat(core): view structs for the app window"`.

### Task 2: Adopt several logins in one change

**Files:** Modify `crates/roster-core/src/ops.rs`; Test `crates/roster-core/tests/ops.rs`

**Interfaces:**
- Produces: `pub fn plan_adopt_all(r: &Roster, p: &RosterPaths, ctx: &Ctx, picks: &[(Tool, String)]) -> Result<Plan, OpError>` — chains `plan_adopt` over the accumulating `roster_after`, merges steps (drops duplicate step texts, keeps one `NOTHING_ELSE` last) and actions (at most one `Action::PathSetup`), summary "Added claude1 and codex1 (existing logins)". Empty `picks` → `OpError::Refused("Pick at least one login to add.")`.

- [ ] **Step 1: Write failing tests** `adopt_all_is_one_change_and_one_undo` (both adopted, `apply` returns one op id, `Journal::undo(id)` leaves the roster empty) and `adopt_all_lists_path_step_once` (exactly one step containing "to PATH"; last step text is "Nothing else on your computer changes.").
- [ ] **Step 2: Run** `cargo test -p roster-core --test ops adopt_all` — Expected: FAIL, `cannot find function plan_adopt_all`.
- [ ] **Step 3: Implement** `plan_adopt_all` in `ops.rs`.
- [ ] **Step 4: Run** `cargo test --workspace` — Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "feat(core): adopt several logins in one change"`.

### Task 3: Desktop helpers — find `roster`, find the app, open a terminal

**Files:** Create `crates/roster-core/src/desktop.rs`, `crates/roster-core/tests/desktop.rs`; Modify `crates/roster-core/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub fn has_desktop(platform: Platform, env: &dyn Fn(&str) -> Option<String>) -> bool` (Linux needs `DISPLAY` or `WAYLAND_DISPLAY`; others true)
  - `pub fn roster_bin_candidates(p: &RosterPaths) -> Vec<PathBuf>` — Unix: `~/.local/bin/roster`, `~/.cargo/bin/roster`, `/opt/homebrew/bin/roster`, `/usr/local/bin/roster`, `/usr/bin/roster`; Windows: `%LOCALAPPDATA%\AgentRoster\roster.exe` (from `p.data_dir`'s parent), `~/.cargo/bin/roster.exe`.
  - `pub fn find_roster_bin(p: &RosterPaths, lookup: &dyn CommandLookup) -> Option<PathBuf>` — `lookup.find("roster")` first, then the first existing candidate.
  - `pub fn app_candidates(p: &RosterPaths, env: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf>` — `ROSTER_APP` first when set; macOS `~/Applications/Agent Roster.app`, `/Applications/Agent Roster.app`; Windows `%LOCALAPPDATA%\Agent Roster\agent-roster-app.exe`, `%ProgramFiles%\Agent Roster\agent-roster-app.exe`; Linux `/usr/bin/agent-roster-app`, `~/.local/bin/agent-roster-app`, `~/Applications/agent-roster.AppImage`.
  - `pub fn find_app(p, env) -> Option<PathBuf>` (first existing candidate)
  - `pub fn app_launch_command(platform: Platform, app: &Path) -> Vec<String>` — macOS `.app` → `["open", app]`; anything else → `[app]`.
  - `pub enum DesktopError { NoTerminal, Io(String) }` with `Display` ("No terminal app was found. Set TERMINAL to your terminal's command." / io text)
  - `pub fn terminal_command(platform: Platform, env: &dyn Fn(&str) -> Option<String>, lookup: &dyn CommandLookup, argv: &[String], scratch: &Path) -> Result<Vec<String>, DesktopError>` — macOS: writes `scratch/agent-roster-<n>.command` (`#!/bin/sh`, `exec` + each arg single-quoted with `'` escaped as `'\''`), mode 0755, returns `["open", file]`. Windows: `wt.exe` found → `["wt.exe", argv...]`, else `["cmd.exe", "/c", "start", "", "cmd.exe", "/k", argv...]`. Linux: first found of `$TERMINAL`, `x-terminal-emulator`, `gnome-terminal`, `konsole`, `xterm` → `[term, "-e", argv...]` (gnome-terminal uses `"--"` instead of `"-e"`); none → `NoTerminal`.

- [ ] **Step 1: Write failing tests** with a `FakeLookup(HashMap<&str, PathBuf>)` and temp dirs: `finds_roster_in_home_candidates` (no PATH hit, `~/.local/bin/roster` exists → found); `prefers_roster_on_path`; `roster_app_env_comes_first`; `mac_app_opens_with_open`; `mac_command_file_quotes_paths_with_spaces` (argv `["/Users/Jo Smith/.local/bin/roster","login","agent2"]` → file contains `exec '/Users/Jo Smith/.local/bin/roster' 'login' 'agent2'`, is executable, command is `["open", <file>]`); `windows_prefers_windows_terminal_then_cmd`; `linux_uses_terminal_env_then_fallbacks` (gnome-terminal gets `--`); `linux_without_terminal_errors`; `linux_without_display_has_no_desktop`.
- [ ] **Step 2: Run** `cargo test -p roster-core --test desktop` — Expected: FAIL, `unresolved import roster_core::desktop`.
- [ ] **Step 3: Implement** `desktop.rs` (pure functions; the only side effect is writing the `.command` file).
- [ ] **Step 4: Run** `cargo test --workspace` — Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "feat(core): find roster and the app, build terminal commands"`.

### Task 4: `roster ui` opens the app

**Files:** Modify `crates/roster-cli/src/main.rs` (the `Cmd::Ui` arm, remove `headless()` in favour of `desktop::has_desktop`); Test `crates/roster-cli/tests/cli.rs`

**Interfaces:** Consumes `desktop::{has_desktop, find_app, app_launch_command}`. The app is spawned detached (`std::process::Command::spawn`, not waited on); prints `Opening Agent Roster.`

- [ ] **Step 1: Write failing tests**: `ui_without_app_keeps_fallback_message` (temp HOME, no `ROSTER_APP` → stdout is exactly today's message, exit 0); `#[cfg(unix)] ui_opens_the_app_from_roster_app` (`ROSTER_APP` = a script that writes `$HOME/opened` → after the command, poll up to 2 s for the file; stdout contains "Opening Agent Roster."); `#[cfg(target_os = "linux")] ui_without_display_says_no_desktop` (env `DISPLAY`/`WAYLAND_DISPLAY` removed).
- [ ] **Step 2: Run** `cargo test -p agent-roster --test cli ui_` — Expected: `ui_opens_the_app_from_roster_app` FAILS (fallback printed).
- [ ] **Step 3: Implement** the `Ui` arm; `Login`'s device-auth check uses `!has_desktop(...)`.
- [ ] **Step 4: Run** `cargo test --workspace` — Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "feat(cli): roster ui opens the app when it is installed"`.

### Task 5: App crate, Backend and Tauri commands

**Files:**
- Create: `app/src-tauri/Cargo.toml`, `app/src-tauri/build.rs`, `app/src-tauri/tauri.conf.json`, `app/src-tauri/capabilities/default.json`, `app/src-tauri/src/main.rs`, `app/src-tauri/src/backend.rs`, `app/src-tauri/src/env.rs`, `app/src-tauri/src/commands.rs`, `app/src-tauri/icons/*` (from `cargo tauri icon`), `app/src-tauri/icons/tray-template.png`, `assets/icon/make-tray-template.py`, `app/.gitignore` (`node_modules`, `dist`)
- Modify: root `Cargo.toml` (`members += "app/src-tauri"`, workspace deps `tauri = { version = "2.12.5", features = ["tray-icon", "image-png"] }`, `tauri-build = "2.7.4"`), `.gitignore`
- Test: `app/src-tauri/src/backend.rs` (`#[cfg(test)] mod tests`), `app/src-tauri/src/env.rs`

**Interfaces:**
- Consumes: Task 1 views, Task 2 `plan_adopt_all`, Task 3 `find_roster_bin`/`terminal_command`, core `ops::*`, `import::plan_import`, `journal::Journal`, `status::overview`.
- Produces:
  - `env::login_path(shell: &str) -> Option<String>` runs `<shell> -ilc 'printf "__ROSTER_PATH__%s__ROSTER_PATH__" "$PATH"'` with a 3 s timeout; `env::parse_marked(output: &str) -> Option<String>` extracts between the markers. `main` sets the process `PATH` to it (Unix only, before any thread starts, in an `unsafe` block with a SAFETY comment), falling back to the current PATH.
  - `#[derive(Deserialize)] #[serde(tag = "op", rename_all = "kebab-case")] pub enum OpRequest { Add { name: String, tool: Tool, color: Option<AgentColor>, role: Option<String>, share: bool }, AdoptAll { picks: Vec<(Tool, String)> }, Import, EnableLimits { name: String }, DisableLimits { name: String } }`
  - `#[derive(Serialize)] pub struct StepView { kind: String /*"add"|"change"|"remove"|"note"*/, text: String }`, `pub struct PlanView { token: u64, summary: String, steps: Vec<StepView> }`
  - `pub struct Backend { p: RosterPaths, roster_bin: Option<PathBuf>, scratch: PathBuf, opener: Box<dyn Fn(&[String]) -> std::io::Result<()> + Send + Sync>, pending: Mutex<HashMap<u64, Pending>>, next: AtomicU64 }` where `Pending { plan: Plan, roster_sha: String }` (sha256 of `roster.toml` bytes, `""` when missing).
  - `impl Backend`: `new(p, roster_bin, scratch, opener)`, `overview(&self) -> Result<OverviewView, String>`, `first_run(&self) -> Result<FirstRunView, String>`, `preview(&self, req: OpRequest) -> Result<PlanView, String>`, `apply(&self, token: u64) -> Result<Option<u64>, String>` (removes the token whether it succeeds or not), `changes(&self) -> Result<Vec<ChangeView>, String>`, `undo(&self, id: u64) -> Result<(), String>`, `undo_all(&self) -> Result<usize, String>`, `sign_in(&self, name: &str) -> Result<(), String>` (opens `[roster_bin, "login", name]` through `terminal_command` + `opener`), `open_terminal_as(&self, name: &str) -> Result<(), String>` (`[roster_bin, "run", id]`).
  - Error copy: no binary → "Agent Roster's `roster` command wasn't found. Install it, then open the app again."; stale preview → "Your roster changed since this preview. Look at the new preview before applying."; unknown token → "That preview has expired. Look at it again."
  - `commands.rs`: one `#[tauri::command] fn` per Backend method (`overview, first_run, preview, apply, changes, undo, undo_all, sign_in, open_terminal_as`), each `State<'_, Backend>` → `Result<_, String>`; heavy calls use `async` commands + `tauri::async_runtime::spawn_blocking`.
- `build.rs`: if `../dist/index.html` is missing, create `../dist/` with a one-line placeholder `index.html` (so `cargo test --workspace` compiles on a fresh clone), then `tauri_build::build()`.
- `tauri.conf.json`: `productName "Agent Roster"`, `identifier "com.mahadihassan.agentroster"`, `build.frontendDist "../dist"`, `devUrl "http://localhost:1420"`, `beforeDevCommand "npm run dev"`, `beforeBuildCommand "npm run build"` (cwd `app/`), one window `main` titled "Agent Roster" 1080×640, min 900×560, `app.security.csp "default-src 'self'; style-src 'self' 'unsafe-inline'; font-src 'self'; img-src 'self' data:"`, `bundle.icon` from `cargo tauri icon` output.
- Tray template: `make-tray-template.py` (Pillow) draws icon-a's shapes as black-on-transparent — three rings on the right, the C-shaped cable from top ring to bottom ring — at 44×44, saved as `app/src-tauri/icons/tray-template.png`.

- [ ] **Step 1:** `cargo tauri icon assets/icon/icon-a.png -o app/src-tauri/icons` and `python3 assets/icon/make-tray-template.py`; look at the template PNG.
- [ ] **Step 2: Write failing tests** in `backend.rs` against a temp-home `RosterPaths::for_home` with a fake `codex` script on PATH and a fake `roster_bin`, and an `opener` that records argv: `preview_add_then_apply_records_one_change` (steps' texts equal `ops::plan_add(...).steps` texts; after apply, `changes()[0].summary` names the agent); `apply_refuses_when_roster_changed_since_preview` (write to `roster.toml` between preview and apply → stale-preview error, nothing applied); `apply_twice_says_expired`; `preview_refuses_without_roster_binary`; `bad_name_preview_shows_core_message` (name "1" → the `NameError` text); `undo_restores_and_lists_undone`; `sign_in_opens_terminal_with_roster_login` (recorded argv ends with `login <name>` on Linux/Windows, or is `open <file.command>` whose content has `'login' '<name>'` on macOS). In `env.rs`: `login_path_parses_marked_output` (noise around markers → the PATH between them; no markers → `None`).
- [ ] **Step 3: Run** `cargo test -p agent-roster-app` — Expected: FAIL to compile (`Backend` missing).
- [ ] **Step 4: Implement** the files above; `main.rs` builds `Backend` with `RosterPaths::detect()`, `find_roster_bin`, `std::env::temp_dir()`, an opener that spawns argv detached, `.manage(backend)`, `.invoke_handler(generate_handler![...])`.
- [ ] **Step 5: Run** `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings` — Expected: PASS, clean.
- [ ] **Step 6: Commit** `git commit -m "feat(app): Tauri crate with backend commands and icons"`.

### Task 6: Frontend shell and the Your agents screen

**Files:**
- Create: `app/package.json`, `app/package-lock.json`, `app/index.html`, `app/vite.config.ts`, `app/svelte.config.js`, `app/tsconfig.json`, `app/src/main.ts`, `app/src/app.css`, `app/src/App.svelte`, `app/src/lib/types.ts`, `app/src/lib/api.ts`, `app/src/lib/poll.ts`, `app/src/screens/Agents.svelte`, `app/src/components/AgentRow.svelte`, `app/src/components/Cable.svelte`, `app/src/components/SidePanel.svelte`, `app/src/test/setup.ts`, tests `app/src/components/AgentRow.test.ts`, `app/src/screens/Agents.test.ts`, `app/src/lib/poll.test.ts`

**Interfaces:**
- Consumes: Task 5 command names and the Task 1 fixtures.
- Produces:
  - `types.ts`: TS interfaces mirroring every Task 1 view and Task 5 `PlanView`/`StepView`/`OpRequest` with the same field names.
  - `api.ts`: `overview(): Promise<OverviewView>`, `firstRun()`, `preview(req: OpRequest): Promise<PlanView>`, `apply(token: number): Promise<number | null>`, `changes()`, `undo(id)`, `undoAll()`, `signIn(name)`, `openTerminalAs(name)` — each a one-line `invoke("<snake_case command>", {...})`.
  - `poll.ts`: `startPoll(fn: () => Promise<void>, ms: number): () => void` — never starts a call while the previous one is pending; returns a stop function. App polls `overview` every 10 s.
  - `App.svelte`: `screen` state `'agents' | 'add' | 'changes' | 'first-run'`; shows `first-run` when `overview.agents` is empty; listens for the Tauri event `navigate` (payload a screen name) from the tray.
- Layout copied from the mockup: grid `1fr 330px`; row grid `6px 1fr 210px 128px 44px`, height 76, gap 8, radius `12px 0 0 12px`; name 30px wide face in `color`; sub line `tool_label`, `email`, `default_label`; bars per `windows` (fill = agent color, coral when `hot`), `reset_text` (coral when `reset_hot`) or `limits_note`; status cell: `state == "sign-in"` → outlined **Sign in** button in the agent color calling `signIn(name)`; Claude agent with `claude_limits === false` and signed in → small **Turn on limits** button (opens the plan sheet from Task 7); otherwise dot + `status_text`. Jack lit when `live`. Header: "Your agents", `headline`, buttons **What changed** (secondary) and **Add agent** (primary). Side panel: "On the line now" (calls: `from_name` asked `to_name`, task, detail; "Nobody is on the line." when empty), "Type these in any terminal" (names in their colors, mono), "Limits" (`warnings`, then `limits_updated`, or "No agent is near its limit."), and the safety line "Your original Claude and Codex setup is untouched. **No hooks added.** Every change can be undone."
  - `Cable.svelte` props `calls: CallView[]`, `order: string[]` (agent ids in row order): one path per call `M0 y1 C 66 y1, 66 y2, 0 y2` with `y = index*84 + 38`, gradient from `from_color` to `to_color`, one white pulse path with class `pulse`, animation off under `prefers-reduced-motion`.

- [ ] **Step 1:** Scaffold `app/` (`npm init` + `npm install svelte@5 @sveltejs/vite-plugin-svelte@7 vite@8 @tauri-apps/api@2 @fontsource-variable/archivo @fontsource/jetbrains-mono` and dev `typescript@6 svelte-check@4 @tsconfig/svelte vitest@5 jsdom @testing-library/svelte @testing-library/jest-dom`); scripts `dev` (port 1420, strictPort), `build`, `test` (`vitest run`), `check` (`svelte-check --tsconfig ./tsconfig.json`). Vitest: `environment: 'jsdom'`, `svelteTesting()` plugin, `setupFiles: ['src/test/setup.ts']` which mocks `@tauri-apps/api/core` `invoke` and `@tauri-apps/api/event` `listen`.
- [ ] **Step 2: Write failing tests** using `overview.json`: `AgentRow`: `renders_name_in_agent_color` (agent1's name element style color `#4d7cff`), `hot_bar_is_coral` (agent3's bar and "93%" carry class `hot`), `sign_in_button_calls_sign_in` (agent2 → click **Sign in** → `invoke("sign_in", {name: "agent2"})`), `shows_reset_text` ("week resets in 3d 16h"). `Agents`: `shows_four_rows_and_side_panel` (4 names, "agent1 asked agent3", "No hooks added."), `cable_drawn_for_each_call` (one `path.pulse`). `poll`: `poll_skips_while_a_request_is_in_flight` (fake timers; a never-resolving fn is called once across 3 intervals).
- [ ] **Step 3: Run** `npm test` (in `app/`) — Expected: FAIL, components missing.
- [ ] **Step 4: Implement** the files; `app.css` defines the mockup tokens on `:root`, imports the bundled fonts, `body` background `var(--base)`.
- [ ] **Step 5: Run** `npm test && npm run check && npm run build` — Expected: tests PASS, `svelte-check found 0 errors`, `dist/` built.
- [ ] **Step 6: Commit** `git commit -m "feat(app): Your agents screen with live cable"`.

### Task 7: Plan sheet, Add agent and Claude limits on/off

**Files:** Create `app/src/components/PlanSheet.svelte`, `app/src/screens/AddAgent.svelte`, tests `app/src/components/PlanSheet.test.ts`, `app/src/screens/AddAgent.test.ts`; Modify `app/src/components/AgentRow.svelte`, `app/src/App.svelte`

**Interfaces:**
- `PlanSheet.svelte` props `{ plan: PlanView, confirmLabel: string, onDone: (opId: number | null) => void, onCancel: () => void }`: shows `summary` and each step text (mark `+ ~ -` colored by kind, like the CLI), **Cancel** and the confirm button calling `apply(plan.token)`; shows the backend error text on failure.
- `AddAgent.svelte`: big name input (46px wide face, in the chosen color, autofocus), Color swatches from the five `AgentColor` values (labels `cobalt marigold jade orchid coral`, hex from the overview fixture's palette list in `types.ts`, the only color constant in JS), Tool segmented control **Claude Code** / **Codex** / **Gemini soon** (disabled), **Share my skills** toggle with "Off. This account starts fully separate." / "On. Uses the skills of your main <tool> account.", optional "Role (optional)" input, the "What will happen" box = `preview({op:"add",...})` step texts refreshed 250 ms after each edit (errors from the backend shown in place of steps), footer **Cancel** / **Add and sign in** → `apply(token)` then `signIn(name)` then back to Your agents.
- Row: **Turn on limits** → `preview({op:"enable-limits", name})` in a PlanSheet; Claude agents with `claude_limits === true` show a small text button **Turn off limits** under the bars → `preview({op:"disable-limits", name})`.

- [ ] **Step 1: Write failing tests**: `add_preview_shows_backend_steps` (type "boss" → after debounce `invoke("preview", {req:{op:"add",name:"boss",tool:"codex",...}})`; the mocked steps' texts appear verbatim); `add_shows_backend_error` (mock rejects "Names can't start with a digit…" → text shown, **Add and sign in** disabled); `add_and_sign_in_applies_then_signs_in` (order of invokes: `preview`, `apply`, `sign_in`); `gemini_is_disabled`; `plan_sheet_apply_error_is_shown`; `turn_on_limits_opens_plan_sheet`.
- [ ] **Step 2: Run** `npm test` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `npm test && npm run check` — Expected: PASS, 0 errors.
- [ ] **Step 5: Commit** `git commit -m "feat(app): add agent and Claude limits with previews"`.

### Task 8: First run

**Files:** Create `app/src/screens/FirstRun.svelte`, `app/src/screens/FirstRun.test.ts`; Modify `app/src/App.svelte`

**Interfaces:** Uses `firstRun()`, `preview({op:"adopt-all", picks})`, `preview({op:"import"})`, `PlanSheet`.
- Title "You already have two agents." / "You already have one agent." / "No Claude Code or Codex login was found." from the count of `installed` candidates (copy only; the count is data). Sub: "We found these signed in on this computer. Give them names, or keep ours. Nothing changes until you continue."
- One row per candidate: suggested name in a cycling palette color (cobalt, jade), "`tool_label` `version`" + email or "not signed in yet", **Rename** turns the name into an input. Uninstalled candidates dimmed with "not installed". A static dimmed "Gemini · soon" row last.
- `prototype === true`: a panel "Your earlier agent-roster script's agents were found." with **Bring them in** → import preview in a PlanSheet; the adopt list stays as the alternative.
- **Continue** → adopt-all preview (installed candidates only) → PlanSheet → on done, back to Your agents. Disabled when no candidate is installed.

- [ ] **Step 1: Write failing tests** with `first-run.json`: `lists_candidates_with_versions`, `rename_changes_the_pick` (rename codex1→agent3 → preview req picks contain `["codex","agent3"]`), `prototype_offers_import` (invoke `preview` with `{op:"import"}`), `first_run_without_tools_disables_continue`.
- [ ] **Step 2: Run** `npm test` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `npm test && npm run check` — Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "feat(app): first run adopts logins or imports the script"`.

### Task 9: What changed

**Files:** Create `app/src/screens/Changes.svelte`, `app/src/screens/Changes.test.ts`; Modify `app/src/App.svelte`

**Interfaces:** Uses `changes()`, `undo(id)`, `undoAll()`.
- Title "Everything Agent Roster did", sub "Each change has a backup. Undo one, or undo all of them." Rows: dot in `color` (or `--mut`), `summary`, time from `at` via `Intl.DateTimeFormat` ("today at 13:42" style is presentation), **Undo** button (absent when `undone`, which shows "undone"). A fixed row "Hooks added to your tools" / "none, ever" (jade). Footer "Uninstalling restores all of this." + **Undo everything**.
- Undo and Undo everything first show a confirm line ("Undo change 3: Added boss (Codex)?" / "Undo every change Agent Roster made?") with **Undo** / **Cancel** — this is the preview for undo, same text as the CLI. Backend errors (conflict, later change) shown verbatim. List reloads after each undo.
- Empty: "Agent Roster hasn't changed anything yet."

- [ ] **Step 1: Write failing tests** with `changes.json`: `lists_changes_newest_first`, `undo_one_confirms_then_calls_undo` (`invoke("undo",{id:3})` only after confirm), `undo_error_is_shown_verbatim`, `undo_everything_confirms_then_calls_undo_all`, `hooks_row_says_none_ever`.
- [ ] **Step 2: Run** `npm test` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `npm test && npm run check` — Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "feat(app): what changed with undo one and undo all"`.

### Task 10: Menu bar / tray

**Files:** Create `app/src-tauri/src/tray.rs`; Modify `app/src-tauri/src/main.rs`

**Interfaces:**
- `pub struct TrayEntry { pub id: String, pub text: String, pub rgb: (u8, u8, u8) }`; `pub fn tray_entries(v: &OverviewView) -> Vec<TrayEntry>` — `id = "agent:<id>"`, `text = "<name>  —  <tray_text>"` (just the name when `tray_text` is empty).
- `pub fn dot_rgba(rgb: (u8,u8,u8), size: u32) -> Vec<u8>` — anti-aliased filled circle, transparent outside.
- `pub fn build_menu(app: &AppHandle, v: &OverviewView) -> tauri::Result<Menu<Wry>>`: agent `IconMenuItem`s (dot icon), separator, submenu **Open a terminal as…** (`term:<name>` per agent), **Add agent** (`add`), **Open Agent Roster** (`open`), separator, **Quit Agent Roster** (`quit`).
- Events: `agent:*` and `open` → show + focus `main`; `add` → show + emit `navigate` `"add"`; `term:<name>` → `Backend::open_terminal_as`; `quit` → `app.exit(0)`.
- Tray icon: macOS `icons/tray-template.png` with `icon_as_template(true)`; Windows/Linux the 32×32 app icon. Menu rebuilt by a background thread every 30 s and after each `overview` command.
- Main window `CloseRequested` → hide (prevent close); the app keeps running in the tray.

- [ ] **Step 1: Write failing tests**: `tray_entries_show_week_percent` (fixture overview → `"agent1  —  61% week"`, `"agent2  —  sign in"`, `"agent3  —  93% week"`), `dot_is_colored_in_the_middle_and_clear_in_the_corner`.
- [ ] **Step 2: Run** `cargo test -p agent-roster-app tray` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings` — Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "feat(app): menu bar and tray with week limits"`.

### Task 11: CI for the app

**Files:** Modify `.github/workflows/ci.yml`

- `test` job: on Linux first `sudo apt-get update && sudo apt-get install -y libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev build-essential`; then the existing fmt/clippy/test (now compiling the app crate on all three OSes).
- New `app` job, matrix macOS/Ubuntu/Windows: `actions/setup-node@v4` (node 24, npm cache on `app/package-lock.json`), in `app/`: `npm ci`, `npm test`, `npm run check`, `npm run build`.
- `musl` job unchanged (CLI only, D8).

- [ ] **Step 1:** Edit the workflow; validate syntax with `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/ci.yml'))"`.
- [ ] **Step 2: Commit** `git commit -m "ci: build and test the app on macOS, Windows and Linux"`. (CI runs on push of the branch's merge to main, Task 12.)

### Task 12: Build, run against the real roster, review, merge

- [ ] **Step 1:** `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `(cd app && npm test)`, `cargo tauri build --bundles app` (from `app/`) — print the tail of each; Expected: all pass, `target/release/bundle/macos/Agent Roster.app` exists.
- [ ] **Step 2:** Install the new CLI to `~/.local/bin/roster` (same path the owner's launchers use), copy the `.app` to `~/Applications/`, run `roster ui` — Expected: "Opening Agent Roster." and the window appears. Read-only: do not press any change button against the real roster.
- [ ] **Step 3:** Screenshot the main window (`screencapture -l <window id>`) to `docs/screenshots/app-window-real.png`; check it shows agent1–agent4 and agent3's Codex week bar.
- [ ] **Step 4:** Fresh reviewer subagent reviews `main..HEAD` against this plan and the spec; fix every Critical and Important finding with a test that fails first; record each in `docs/superpowers/plans/2026-11-09-plan-2-ledger.md`.
- [ ] **Step 5:** Merge to `main` (`--no-ff`), push, watch CI; `gh run view <id>` shows every job passed.
