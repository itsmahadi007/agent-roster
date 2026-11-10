# Agent page, roles, Good at, terminals, release — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Layout C (agent list + agent page), ready-made roles, "Good at" in the roster note, Open in a chosen terminal,
the credit line, and a GitHub release with Mac and Windows downloads that include the `roster` command.

**Architecture:** roster-core gains `Agent.good_at`, a `plan_set_good_at` op, a Good-at column and Leader sentence in
the note, terminal-app detection and a URL-open command. The Tauri backend exposes them. The Svelte app replaces the
rows/side panel/edit sheet with `AgentList` + `AgentPage`. `roster` ships inside the app bundle as a Tauri sidecar so a
teammate installs one thing. A `release.yml` workflow builds a universal Mac `.dmg` and a Windows setup on tag `v*`.

**Tech Stack:** Rust 2024 workspace, Tauri 2.12, Svelte 5, Vitest, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-11-10-agent-page-design.md` (binding spec: `2026-11-09-agent-roster-design.md`).

## Global Constraints

- Every roster/note/tool-config change: preview → token → apply in one Journal transaction, undoable.
- No hooks; never read credential files; never write through a link into another account; never move/delete an adopted folder.
- Every roster-core behavior change gets a test first. Checks before commit:
  `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, plus
  `cd app && npm test && npm run check`.
- Copy: plain short English, sentence case, no trailing period on labels; example.com in tests.
- Never type into another app. Open-and-copy is the fallback for terminals without a run-a-command form.
- No AI attribution in commits or release notes.

## Rulings made while planning (spec deviations)

- Owner's earlier rule "the note does not list what each tool is good at" is reversed by the owner's 2026-11-10 request
  ("what are the capabilities that our AI will know automatically"); binding spec §5.4 is amended in Task 1.
- The display-only `codex features list` check is not built this round (YAGNI); defaults are fixed per tool.
- "Runs it" terminals: macOS Terminal and iTerm (open the `.command` file with that app); Windows Terminal, PowerShell,
  Command Prompt. Every other detected app is open-and-copy (Ghostty's docs say its CLI launch isn't supported on macOS).
- The remembered terminal lives in the webview's localStorage (an app preference, per spec "My calls").
- `roster` ships as a Tauri `externalBin` sidecar; `roster_bin_candidates` also looks next to the running app.

## Review Focus

1. An agent whose role is a custom text that happens to be "leader" (any case) — Leader button lit, sentence lists it.
2. Good at items containing `|` or newlines — note table stays one row (same sanitising as role).
3. No terminal app detected at all — control turns into Copy command; nothing errors.
4. Selected agent removed or renamed while its page is open — selection moves to the neighbor / follows the id.
5. A teammate's machine without `roster` on PATH — the sidecar inside the app is found and used for wrappers.

---

### Task 1: Good at and Leader in roster-core

**Files:** Modify `crates/roster-core/src/model.rs`, `ops.rs`, `note.rs`, `view.rs`; `crates/roster-cli/src/main.rs`;
`docs/superpowers/specs/2026-11-09-agent-roster-design.md` (§5.4). Test: `crates/roster-core/src/note.rs` tests,
`crates/roster-core/tests/view.rs`, ops tests.

**Interfaces — Produces:**
- `Agent.good_at: Option<Vec<String>>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`).
- `pub fn default_good_at(tool: Tool) -> Vec<&'static str>`: Claude → `["coding", "reviewing code", "long tasks"]`,
  Codex → `["coding", "making images"]`.
- `pub fn good_at_of(a: &Agent) -> Vec<String>` (set list or defaults).
- `pub fn plan_set_good_at(r, p, ctx, name: &str, items: Option<Vec<String>>) -> Result<Plan, OpError>`; summary
  `"Set what {name} is good at"` / reset: `"Reset what {name} is good at"`; items trimmed, empties dropped, `|`→`/`.
- `AgentView.good_at: Vec<String>`, `AgentView.good_at_custom: bool`.
- CLI `roster good-at <name> [items…] [--reset]`.

- [ ] Step 1: tests. `note_lists_good_at_column`: header `| Name | Tool | Role | Good at |`, row
  `| boss | Claude Code | plans work | coding, reviewing code, long tasks |`. Rename the old test to
  `note_lists_names_tools_roles_and_good_at` and drop its `!contains("image")` line.
  `note_names_every_leader`: two agents with role "Leader" and "leader" → note contains
  "`boss` and `worker2` are leaders: give them big tasks; they split them and hand parts to the workers."; one leader →
  "`boss` is the leader: give it big tasks; it splits them and hands parts to the workers."; none → no "leader" text.
  `set_good_at_plans_and_undoes` (ops): set ["coding","tests"] then apply → model has Some; reset → None.
  view: `good_at` equals defaults and `good_at_custom == false` when None.
- [ ] Step 2: run, see them fail on assertions (add stubs first so they compile).
- [ ] Step 3: implement; amend binding spec §5.4 to say the note lists "Good at" (defaults per tool, editable).
- [ ] Step 4: `UPDATE_FIXTURES=1 cargo test -p roster-core --test view fixtures_match`, then full checks green.
- [ ] Step 5: commit `core: Good at per agent and a leader line in the roster note`.

### Task 2: Terminal apps and URL opening in roster-core

**Files:** Modify `crates/roster-core/src/desktop.rs`. Test in the same file.

**Interfaces — Produces:**
- `pub struct TerminalApp { pub id: String, pub label: String, pub runs: bool }` (Serialize).
- `pub fn find_terminals(platform: Platform, app_exists: &dyn Fn(&str) -> bool, lookup: &dyn CommandLookup) -> Vec<TerminalApp>`.
  macOS table (id = bundle name without `.app`, checked by `app_exists("<Name>.app")` in /Applications,
  ~/Applications, /System/Applications/Utilities): Terminal (runs), iTerm (runs), Warp, Ghostty, WezTerm, kitty,
  Alacritty, cmux, Hyper, Tabby, Visual Studio Code, Cursor, Windsurf, Zed. Windows: `wt` → "Windows Terminal" (runs),
  "PowerShell" (runs, always), "Command Prompt" (runs, always), `code` → "VS Code", `cursor` → "Cursor". Linux: today's
  list, all runs.
- `pub fn terminal_command_in(platform, app: Option<&str>, env, lookup, argv, scratch) -> Result<Vec<String>, DesktopError>`:
  None → today's `terminal_command`; Mac + runs app → `["open", "-a", <id>, <file.command>]`; Windows "PowerShell" →
  `["powershell.exe", "-NoExit", "-Command", <quoted argv>]` via `cmd /c start`; "Command Prompt" → today's cmd form.
- `pub fn app_open_command(platform, app_id: &str) -> Vec<String>`: Mac `["open", "-a", id]`; Windows `["cmd.exe","/c",<exe>]` (hidden; `start` would leave a `cmd /K` window open for `.cmd` launchers).
- `pub fn url_open_command(platform, url: &str) -> Vec<String>`: Mac `open`, Windows `cmd /c start "" url`, Linux `xdg-open`.

- [ ] Step 1: tests `finds_installed_mac_terminals_in_order` (Terminal first, Warp copy-mode), `windows_always_has_powershell_and_cmd`,
  `iterm_runs_the_command_file`, `url_open_per_platform`.
- [ ] Step 2: fail; Step 3: implement; Step 4: green; Step 5: commit `core: find terminal apps and open a URL`.

### Task 3: Backend commands

**Files:** Modify `app/src-tauri/src/backend.rs`, `commands.rs`, `main.rs`; test `backend_tests.rs`.

**Interfaces — Produces (Tauri commands):**
- `OpRequest::GoodAt { name: String, items: Option<Vec<String>> }` → `plan_set_good_at`.
- `terminals() -> Vec<TerminalApp>`.
- `open_terminal_as(name: String, app: Option<String>) -> Result<bool, String>`: true = it runs the agent;
  false = it opened the app, the page should say "copied".
- `sign_in(name, app: Option<String>) -> Result<bool, String>` same rule.
- `open_site()`: opens exactly `https://mahadihassan.com`.

- [ ] Steps: tests `good_at_previews_then_applies`, `open_in_copy_mode_app_opens_the_app_and_returns_false`,
  `open_site_opens_only_the_fixed_url` (recorded opener argv), fail, implement, green, commit `app backend: good at, terminals, site link`.

### Task 4: Layout C frontend

**Files:** Create `app/src/components/AgentList.svelte`, `AgentPage.svelte`, `TerminalButton.svelte`, `RolePicker.svelte`,
`GoodAt.svelte`, `Credit.svelte` (+ tests). Modify `App.svelte`, `lib/api.ts`, `lib/types.ts`, `screens/Help.svelte`,
`screens/AddAgent.svelte` (role picker). Delete `screens/Agents.svelte`, `components/AgentRow.svelte`, `Cable.svelte`,
`SidePanel.svelte`, `AgentSheet.svelte` and their tests (behaviors re-tested in the new components).

**Interfaces — Consumes:** Task 3 commands. **Produces:** `ROLE_PRESETS = ["Leader","Worker 1","Worker 2","Worker 3","Reviewer","Tester","Designer"]`.

- [ ] Tests (Vitest): list selects first agent; click/ArrowDown changes selection; page shows usage cards / "Shows after
  you use agent2" / Show usage; role preset click previews `{op:"role", role:"Leader"}` and Save role applies; custom
  "leader" lights Leader; good-at add/remove previews `{op:"good-at"}`; Reset to defaults sends `items:null`;
  terminal menu lists `terminals()` results, copy-mode click writes "agent2" to clipboard and says
  "agent2 is copied. Paste it in Warp and press Return."; no terminals → "Copy command"; "Or type agent2 in any
  terminal" + Copy; credit line text "Created by Mahadi Hassan · mahadihassan.com" and click calls `open_site`;
  remove selects the neighbor; Add agent/What changed/How it works open in the right pane.
- [ ] Fail, implement, green, `npm run check`, visual check in the browser mock (dark, light, 1 and 10 agents), commit
  `app: layout C — agent list and agent page`.

### Task 5: Ship roster inside the app, and the release workflow

**Files:** Modify `app/src-tauri/tauri.conf.json` (`bundle.externalBin: ["binaries/roster"]`), `crates/roster-core/src/desktop.rs`
(`roster_bin_candidates` adds the running exe's folder), `.github/workflows/package.yml` → `release.yml`, `.gitignore`
(`app/src-tauri/binaries/`), `README.md` (Download section), versions → `0.2.0` (Cargo workspace, app/package.json).

- [ ] Test `candidates_include_the_app_folder` (core). Fail, implement, green.
- [ ] `release.yml` on `push: tags: ['v*']` and `workflow_dispatch`: job `mac` (macos-latest): build `roster` for
  aarch64 + x86_64, `lipo` into `binaries/roster-universal-apple-darwin`, `cargo tauri build --target universal-apple-darwin --bundles dmg`;
  job `windows` (windows-latest): build roster, copy to `binaries/roster-x86_64-pc-windows-msvc.exe`, `cargo tauri build --bundles nsis`,
  silent-install smoke test (app + roster.exe present); job `release` creates the GitHub Release with the `.dmg`, the setup `.exe`, and plain `roster` binaries.
- [ ] Local check: `cargo tauri build --bundles app` with an aarch64 sidecar; the app finds its bundled roster.
- [ ] Commit `release: roster inside the app, Mac and Windows downloads on tag`.
- [ ] Merge to main, push, tag `v0.2.0`, push tag, watch the run, check the release assets.
