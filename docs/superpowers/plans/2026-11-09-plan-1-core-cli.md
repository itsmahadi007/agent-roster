# Agent Roster — Plan 1: Core + CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A complete, headless `roster` CLI on macOS, Windows and Linux, built on a `roster-core`
library, that replaces the bash prototype on the owner's Mac.

**Architecture:** Cargo workspace. `roster-core` holds all logic behind a `RosterPaths` value
(every path derives from it, so tests run in a temp HOME). Every mutation is planned as a `Plan`
of human-readable steps, then applied through a `Journal` transaction that backs up each file and
can undo it. `roster-cli` is a thin clap layer over core. The app (Plan 2) will call the same core.

**Tech Stack:** Rust 2024 edition (stable toolchain, run `rustup update` first), clap 4, serde +
toml + serde_json, sha2, which, anstream/anstyle, time, tempfile + assert_cmd + predicates (tests),
winreg (Windows only).

**Spec:** `docs/superpowers/specs/2026-11-09-agent-roster-design.md` (approved v3)

**Plan series:** Plan 0 spike (`2026-11-09-plan-0-spike.md`) runs first or alongside Task 1-5.
Plan 2 (Tauri app + tray) and Plan 3 (CI, packaging, install scripts, landing page on
agentroster.mahadihassan.com) are written after this plan lands, against its real interfaces.

## Global Constraints

- Platforms: macOS (arm64, x86_64), Windows 10/11 (x86_64, arm64), Linux (x86_64, arm64; musl CLI).
- Package/crate name `agent-roster`; binary name `roster`; core crate `roster-core`.
- Never open token files (`auth.json`, `.credentials.json`) or keychain/credential-manager secrets.
- Never install hooks into any tool. Never shadow the real `claude`/`codex` binaries.
- Never move an adopted config folder (`~/.claude`, `~/.codex`, prototype `~/.claude-2` etc.).
- Every file change outside Agent Roster's own folders goes through `Journal` and appears in a `Plan` preview first.
- Marked-section markers, exactly: `<!-- agent-roster:begin -->` and `<!-- agent-roster:end -->`.
- Shell rc markers, exactly: `# >>> agent-roster >>>` and `# <<< agent-roster <<<`.
- Launcher marker on line 2, exactly: `# generated-by-agent-roster id=<id>` (`rem` instead of `#` in `.cmd`).
- Launcher folder is **appended** to PATH, never prepended.
- Name rule: `^[A-Za-z_][A-Za-z0-9_-]{0,31}$`, not an existing command on PATH (our own shims excepted).
- Colors (name → hex): cobalt `#4d7cff`, marigold `#f0b429`, jade `#2fbf8f`, orchid `#c06bf0`, coral `#ff7a6b`. 90% or more used renders coral.
- Copy: plain verbs, sentence case, no exclamation marks. `NO_COLOR` and non-TTY disable color.
- No account rotation logic anywhere.

## Review Focus

1. **Prototype symlinks:** `~/.claude-2/CLAUDE.md` and `settings.json` are symlinks to the primary's files. Writing a note or a statusLine through them would edit the primary. Expect: real file replaces a linked `CLAUDE.md`/`AGENTS.md` before writing; statusLine is written once per real settings file. Tests in Task 5 and Task 11.
2. **User edits after we wrote:** the user edits `~/.claude/CLAUDE.md` outside our block, then runs `roster undo`. Expect: undo of that op refuses with a conflict naming the file, does not overwrite. Test in Task 4.
3. **Rename while an agent runs:** `roster rename worker1 designer` while `worker1` is open. Expect: launcher swapped, running session unaffected, live record still resolves by id. Test in Task 10.
4. **Non-interactive delegation output:** `agent3 exec "..."` captured by another agent. Expect: no banner, no title escape, exit code of the tool passed through. Test in Task 9.
5. **Missing or unreadable sources:** no Codex sessions yet, malformed jsonl line, `claude` not installed, `claude auth status` fails. Expect: overview still renders, field shows "no data yet" / "not installed", exit 0. Tests in Tasks 7, 8, 13.

---

## File Structure

```
Cargo.toml                          workspace
crates/roster-core/src/
  lib.rs            re-exports
  paths.rs          RosterPaths, Platform, tilde expand/collapse
  model.rs          Roster, Agent, Tool, AgentColor, LimitsSource; load/save/validate
  names.rs          validate_name, NameError, CommandLookup trait
  journal.rs        Journal, Txn, Op, Change, undo/undo_all, Conflict
  note.rs           render_note, upsert_block, remove_block
  launcher.rs       shim_text, write_launchers, ensure_path_setup
  detect.rs         ToolInfo, find_tool, parse_claude_status, parse_codex_status, CommandRunner trait
  limits.rs         AgentLimits, LimitWindow, read_codex_limits, read_claude_limits, format_reset
  session.rs        SessionRecord, start/finish/live_calls, task_from_args
  run.rs            build_env, run_agent
  statusline.rs     handle_statusline
  share.rs          apply_share
  import.rs         plan_import (bash prototype)
  ops.rs            Plan, Step, plan_* functions, apply
crates/roster-core/tests/fixtures/   codex rollout jsonl, claude status json, prototype roster.conf
crates/roster-cli/src/
  main.rs           clap definitions, dispatch
  render.rs         overview, bars, colors, json output
  prompt.rs         confirm(plan) with --yes
crates/roster-cli/tests/cli.rs      assert_cmd end-to-end in temp HOME
```

---

### Task 1: Workspace and `RosterPaths`

**Files:** Create `Cargo.toml`, `crates/roster-core/{Cargo.toml,src/lib.rs,src/paths.rs}`, `crates/roster-cli/{Cargo.toml,src/main.rs}`, `rust-toolchain.toml` (channel = "stable"), `LICENSE` (MIT, "Mahadi Hassan"), `README.md` (one paragraph from spec §1).

**Interfaces — Produces:**
- `enum Platform { Mac, Linux, Windows }` with `Platform::current()`.
- `struct RosterPaths { home, config_dir, data_dir, platform }` with fields `roster_file`, `bin_dir`, `accounts_dir`, `journal_dir`, `run_dir`, `limits_dir` as methods returning `PathBuf`.
- `RosterPaths::for_home(home: &Path, platform: Platform, env: &dyn Fn(&str) -> Option<String>) -> RosterPaths`; `RosterPaths::detect() -> RosterPaths`.
- `fn expand_tilde(p: &str, home: &Path) -> PathBuf`; `fn collapse_tilde(p: &Path, home: &Path) -> String`.

- [ ] **Step 1: Failing tests in `paths.rs`**
```rust
#[test] fn unix_layout() {
    let p = RosterPaths::for_home(Path::new("/h"), Platform::Mac, &|_| None);
    assert_eq!(p.roster_file(), PathBuf::from("/h/.config/agent-roster/roster.toml"));
    assert_eq!(p.bin_dir(), PathBuf::from("/h/.agent-roster/bin"));
    assert_eq!(p.accounts_dir(), PathBuf::from("/h/.agent-roster/accounts"));
}
#[test] fn xdg_config_respected() {
    let p = RosterPaths::for_home(Path::new("/h"), Platform::Linux, &|k| (k=="XDG_CONFIG_HOME").then(|| "/x".into()));
    assert_eq!(p.roster_file(), PathBuf::from("/x/agent-roster/roster.toml"));
}
#[test] fn windows_layout() {
    let env = |k: &str| match k { "APPDATA" => Some(r"C:\U\R".into()), "LOCALAPPDATA" => Some(r"C:\U\L".into()), _ => None };
    let p = RosterPaths::for_home(Path::new(r"C:\U"), Platform::Windows, &env);
    assert_eq!(p.roster_file(), PathBuf::from(r"C:\U\R\AgentRoster\roster.toml"));
    assert_eq!(p.bin_dir(), PathBuf::from(r"C:\U\L\AgentRoster\bin"));
}
#[test] fn tilde_round_trip() {
    assert_eq!(expand_tilde("~/.claude", Path::new("/h")), PathBuf::from("/h/.claude"));
    assert_eq!(collapse_tilde(Path::new("/h/.codex-2"), Path::new("/h")), "~/.codex-2");
}
```
- [ ] **Step 2:** `cargo test -p roster-core paths` → FAIL (not defined).
- [ ] **Step 3:** Implement. `data_dir` is `~/.agent-roster` on Mac/Linux and `%LOCALAPPDATA%\AgentRoster` on Windows; `config_dir` is `$XDG_CONFIG_HOME/agent-roster` or `~/.config/agent-roster`, Windows `%APPDATA%\AgentRoster`. `limits_dir = data_dir/limits`.
- [ ] **Step 4:** `cargo test -p roster-core paths` → PASS. `cargo run -p agent-roster -- --version` prints the version.
- [ ] **Step 5:** `git commit -m "feat(core): workspace and RosterPaths"`

### Task 2: Roster model

**Files:** Create `crates/roster-core/src/model.rs`.

**Interfaces — Produces:**
```rust
enum Tool { Claude, Codex }                       // serde lowercase
enum AgentColor { Cobalt, Marigold, Jade, Orchid, Coral }  // serde lowercase; fn hex(&self) -> &'static str
enum LimitsSource { CodexSessions, ClaudeStatusline, Off } // serde kebab-case
struct Agent { id: String, name: String, tool: Tool, config_dir: String, color: AgentColor,
               role: Option<String>, default: bool, share: bool, limits: LimitsSource,
               advanced_links: Vec<String> }
struct StatuslineChain { settings: String, chain: Option<String> }
struct Roster { version: u32, agents: Vec<Agent>, statusline: Vec<StatuslineChain> }
impl Roster { fn load(p: &RosterPaths) -> Result<Roster, ModelError>;   // missing file -> empty roster
              fn save(&self, p: &RosterPaths) -> Result<(), ModelError>; // atomic write (tmp + rename)
              fn validate(&self) -> Result<(), ModelError>;
              fn by_name(&self, n: &str) -> Option<&Agent>; fn by_id(&self, id: &str) -> Option<&Agent>;
              fn config_dir_of(&self, a: &Agent, p: &RosterPaths) -> PathBuf;
              fn is_adopted_default(&self, a: &Agent, p: &RosterPaths) -> bool; } // dir == ~/.claude or ~/.codex
fn new_id() -> String   // 6 lowercase hex chars, random
```
TOML layout exactly as spec §5.2 (`version = 1`, `[[agent]]`, plus `[[statusline]]`).

- [ ] **Step 1: Failing tests**
```rust
#[test] fn round_trip() { /* build Roster with 2 agents, save to temp RosterPaths, load, assert_eq */ }
#[test] fn missing_file_is_empty() { assert!(Roster::load(&tmp_paths()).unwrap().agents.is_empty()); }
#[test] fn rejects_duplicate_names() { assert!(matches!(r_with_names(&["boss","boss"]).validate(), Err(ModelError::DuplicateName(n)) if n=="boss")); }
#[test] fn rejects_two_defaults_per_tool() { assert!(matches!(two_claude_defaults().validate(), Err(ModelError::TwoDefaults(Tool::Claude)))); }
#[test] fn rejects_unknown_version() { /* write "version = 2" */ assert!(matches!(Roster::load(&p), Err(ModelError::UnsupportedVersion(2)))); }
#[test] fn color_hex() { assert_eq!(AgentColor::Cobalt.hex(), "#4d7cff"); assert_eq!(AgentColor::Coral.hex(), "#ff7a6b"); }
```
- [ ] **Step 2:** run → FAIL. **Step 3:** implement. **Step 4:** run → PASS.
- [ ] **Step 5:** `git commit -m "feat(core): roster model"`

### Task 3: Name rules

**Files:** Create `crates/roster-core/src/names.rs`.

**Interfaces — Produces:** `trait CommandLookup { fn find(&self, name: &str) -> Option<PathBuf>; }` with `PathLookup` (real PATH via `which`); `fn is_our_shim(path: &Path) -> bool` (line 2 starts with the launcher marker); `fn validate_name(name: &str, roster: &Roster, lookup: &dyn CommandLookup, ignore_id: Option<&str>) -> Result<(), NameError>`; `enum NameError { Empty, StartsWithDigit { suggestion: String }, BadChars, TooLong, Taken(String), CommandExists(PathBuf), Reserved }`. Reserved: `roster`, `claude`, `codex`, `gemini`.

- [ ] **Step 1: Failing tests**
```rust
#[test] fn plain_names_ok() { for n in ["boss","worker1","worker-2","a_1"] { assert!(validate_name(n,&empty(),&NoCmds,None).is_ok()); } }
#[test] fn digit_start_suggests() { assert_eq!(validate_name("1",&empty(),&NoCmds,None), Err(NameError::StartsWithDigit{suggestion:"a1".into()})); }
#[test] fn existing_command_refused() { assert!(matches!(validate_name("ls",&empty(),&FakeCmds(&["ls"]),None), Err(NameError::CommandExists(_)))); }
#[test] fn our_shim_not_a_collision() { /* FakeCmds returns a temp file whose line 2 is the marker */ assert!(validate_name("boss",&empty(),&shim_lookup,None).is_ok()); }
#[test] fn rename_to_self_ok() { assert!(validate_name("boss",&roster_with("boss","id1"),&NoCmds,Some("id1")).is_ok()); }
#[test] fn reserved() { assert_eq!(validate_name("claude",&empty(),&NoCmds,None), Err(NameError::Reserved)); }
```
- [ ] **Step 2-4:** FAIL → implement → PASS.
- [ ] **Step 5:** `git commit -m "feat(core): name rules"`

### Task 4: Journal (backup, undo, conflicts)

**Files:** Create `crates/roster-core/src/journal.rs`.

**Interfaces — Produces:**
```rust
struct Journal { /* data_dir/journal: journal.jsonl + backups/<op>/<n> */ }
impl Journal { fn open(p: &RosterPaths) -> Result<Journal>; fn begin(&mut self, summary: &str) -> Txn<'_>;
               fn ops(&self) -> &[Op]; fn undo(&mut self, op_id: u64) -> Result<(), UndoError>;
               fn undo_all(&mut self) -> Result<Vec<u64>, UndoError>; }   // newest first; stops at first conflict
impl Txn<'_> { fn write_file(&mut self, path: &Path, bytes: &[u8]) -> Result<()>;
               fn remove_file(&mut self, path: &Path) -> Result<()>;
               fn ensure_dir(&mut self, path: &Path) -> Result<()>;
               fn link_dir(&mut self, link: &Path, target: &Path) -> Result<()>; // symlink; junction on Windows
               fn replace_link_with_copy(&mut self, path: &Path) -> Result<()>;  // symlinked file -> real file with target's bytes
               fn set_windows_user_path(&mut self, value: &str) -> Result<()>;   // cfg(windows); backup old value
               fn commit(self) -> Result<u64>; }   // dropping without commit rolls back already-applied changes
struct Op { id: u64, at: i64, summary: String, changes: Vec<Change>, undone: bool }
enum Change { Created { path, after_sha256 }, Modified { path, backup, after_sha256 },
              Removed { path, backup }, DirCreated { path }, Linked { link, target }, UserPath { before } }
enum UndoError { Conflict { path: PathBuf }, AlreadyUndone(u64), NotFound(u64), Io(..) }
```
Undo of `Created`/`Modified` refuses with `Conflict` if the file's current sha256 differs from `after_sha256`. `DirCreated` is removed only if empty.

- [ ] **Step 1: Failing tests** (all in temp dirs)
```rust
#[test] fn write_then_undo_restores_original() { /* file "A" -> txn write "B" -> commit -> undo -> reads "A" */ }
#[test] fn created_file_undo_deletes_it() { }
#[test] fn conflict_when_user_edited_after() { /* write "B", commit, user writes "C", undo -> Err(Conflict{path}); file still "C" */ }
#[test] fn undo_all_newest_first() { /* op1 writes x, op2 writes y; undo_all returns [2,1] */ }
#[test] fn uncommitted_txn_rolls_back_on_drop() { }
#[test] fn replace_link_with_copy_keeps_target_untouched() { /* link -> target "T"; replace; write link "N"; target still "T" */ }
#[test] fn journal_persists_across_open() { }
```
- [ ] **Step 2-4:** FAIL → implement → PASS.
- [ ] **Step 5:** `git commit -m "feat(core): journal with backups, undo and conflict detection"`

### Task 5: Roster note

**Files:** Create `crates/roster-core/src/note.rs`.

**Interfaces — Consumes:** `Roster`, `Agent`, `RosterPaths`. **Produces:** `fn render_note(r: &Roster, p: &RosterPaths) -> String` (the full block including both markers); `fn upsert_block(existing: &str, block: &str) -> String`; `fn remove_block(existing: &str) -> String`; `fn note_targets(r: &Roster, p: &RosterPaths) -> Vec<PathBuf>` (adopted Claude → `~/.claude/CLAUDE.md`, adopted Codex → `~/.codex/AGENTS.md`, secondary → `<config_dir>/CLAUDE.md` or `AGENTS.md`); `fn write_notes(txn: &mut Txn, r: &Roster, p: &RosterPaths) -> Result<()>` (calls `replace_link_with_copy` first when a target is a symlink).

Note content (spec §5.4): heading `## Agent roster`, table `| Name | Tool | Role |`, then rules: how to call (`<name> -p "<task>"`, add `--permission-mode acceptEdits` to edit; `<name> exec "<task>"`, add `--full-auto` to edit), never call bare `claude`/`codex` from inside a session, pass a self-contained task and relay the result, check identity with `claude auth status` / `codex login status`. No tool-capability claims.

- [ ] **Step 1: Failing tests**
```rust
#[test] fn inserts_once_and_is_idempotent() { let a = upsert_block("# Rules\n", &blk()); assert_eq!(upsert_block(&a, &blk()), a); assert_eq!(a.matches("<!-- agent-roster:begin -->").count(), 1); }
#[test] fn preserves_text_outside_block() { /* text before and after survives a replace */ }
#[test] fn replaces_prototype_block() { /* fixture with the prototype's old block -> new block, old rows gone */ }
#[test] fn note_lists_names_tools_roles_and_no_capabilities() { let n = render_note(&roster_boss_worker(), &p); assert!(n.contains("| boss | Claude Code | plans work |")); assert!(!n.to_lowercase().contains("image")); }
#[test] fn linked_note_file_is_replaced_not_followed() { /* secondary CLAUDE.md symlink -> primary; write_notes; primary unchanged except its own block */ }
#[test] fn remove_block_leaves_rest() { }
```
- [ ] **Step 2-4:** FAIL → implement → PASS.
- [ ] **Step 5:** `git commit -m "feat(core): roster note render and marked-block upsert"`

### Task 6: Launchers and PATH setup

**Files:** Create `crates/roster-core/src/launcher.rs`.

**Interfaces — Produces:** `fn shim_text(platform: Platform, id: &str, roster_bin: &Path) -> Vec<(String /*file suffix*/, String /*content*/)>` (Unix: one extension-less sh file; Windows: `<name>.cmd` plus an extension-less sh file for Git Bash); `fn write_launchers(txn: &mut Txn, r: &Roster, p: &RosterPaths, roster_bin: &Path) -> Result<()>` (writes one per agent, removes stale files in `bin_dir` that carry our marker but no longer match an agent); `fn path_has_bin(p: &RosterPaths, path_var: &str) -> bool`; `fn plan_path_setup(p: &RosterPaths, shell_rcs: &[PathBuf]) -> Option<Vec<Step>>`; `fn apply_path_setup(txn: &mut Txn, ...)`.

Unix shim exact text:
```sh
#!/bin/sh
# generated-by-agent-roster id=<id>
exec "<roster_bin>" run <id> -- "$@"
```
Windows `.cmd` exact text:
```bat
@echo off
rem generated-by-agent-roster id=<id>
"<roster_bin>" run <id> -- %*
```
rc block (zsh `~/.zshrc`, bash `~/.bashrc`, fish `~/.config/fish/conf.d/agent-roster.fish`):
```sh
# >>> agent-roster >>>
export PATH="$PATH:$HOME/.agent-roster/bin"
# <<< agent-roster <<<
```
(fish: `fish_add_path --append $HOME/.agent-roster/bin` inside the markers). Windows: append `bin_dir` to HKCU `Environment\Path` via `Txn::set_windows_user_path`, then broadcast `WM_SETTINGCHANGE`.

- [ ] **Step 1: Failing tests**
```rust
#[test] fn unix_shim_exact() { assert_eq!(shim_text(Platform::Mac,"a1b2c3",Path::new("/opt/homebrew/bin/roster"))[0].1, "#!/bin/sh\n# generated-by-agent-roster id=a1b2c3\nexec \"/opt/homebrew/bin/roster\" run a1b2c3 -- \"$@\"\n"); }
#[test] fn windows_shim_has_cmd_and_sh() { let s = shim_text(Platform::Windows,"x",Path::new(r"C:\r.exe")); assert!(s.iter().any(|(suf,_)| suf==".cmd")); assert_eq!(s.len(),2); }
#[test] fn stale_launcher_removed_foreign_file_kept() { /* bin has our marker file "old" and a user file "mine"; write_launchers keeps "mine", removes "old" */ }
#[test] fn rc_block_appended_once() { /* apply twice -> one block; PATH line appends */ }
#[test] fn no_setup_when_on_path() { assert!(plan_path_setup(&p, &[]).is_none() /* with PATH containing bin_dir */); }
```
- [ ] **Step 2-4:** FAIL → implement → PASS (Unix shims chmod 0755).
- [ ] **Step 5:** `git commit -m "feat(core): launchers and PATH setup"`

### Task 7: Tool detection and sign-in status

**Files:** Create `crates/roster-core/src/detect.rs`, fixtures `tests/fixtures/claude-auth-status.json`, `codex-login-status.txt`.

**Interfaces — Produces:** `trait CommandRunner { fn run(&self, prog: &Path, args: &[&str], env: &[(String, Option<String>)]) -> Result<Output>; }` (`None` value = unset) with `RealRunner`; `struct ToolInfo { tool: Tool, path: Option<PathBuf>, version: Option<String> }`; `fn find_tool(tool: Tool, path_var: &str, exclude: &Path) -> Option<PathBuf>` (skips our `bin_dir`); `struct SignIn { signed_in: bool, email: Option<String>, plan: Option<String> }`; `fn parse_claude_status(json: &str) -> SignIn` (keys `loggedIn`, `email`, `subscriptionType`); `fn parse_codex_status(stdout: &str, success: bool) -> SignIn` (signed in when success and stdout starts with `Logged in`); `fn sign_in_for(agent: &Agent, r: &Roster, p: &RosterPaths, runner: &dyn CommandRunner) -> SignIn` (runs `claude auth status --json` or `codex login status` with the agent's env from `run::build_env`).

- [ ] **Step 1: Failing tests**
```rust
#[test] fn claude_status_signed_in() { let s = parse_claude_status(include_str!("../tests/fixtures/claude-auth-status.json")); assert!(s.signed_in); assert_eq!(s.email.as_deref(), Some("someone@example.com")); assert_eq!(s.plan.as_deref(), Some("max")); }
#[test] fn claude_status_garbage_is_signed_out() { assert!(!parse_claude_status("not json").signed_in); }
#[test] fn codex_status() { assert!(parse_codex_status("Logged in using ChatGPT\n", true).signed_in); assert!(!parse_codex_status("Not logged in\n", false).signed_in); }
#[test] fn find_tool_skips_our_bin() { /* PATH = ourbin:realbin, both contain "claude"; returns realbin/claude */ }
#[test] fn sign_in_uses_agent_env() { /* FakeRunner records env; for secondary Claude agent CLAUDE_CONFIG_DIR is Some(dir); for adopted default it is None (unset) */ }
```
- [ ] **Step 2-4:** FAIL → implement → PASS.
- [ ] **Step 5:** `git commit -m "feat(core): tool detection and sign-in status"`

### Task 8: Limits

**Files:** Create `crates/roster-core/src/limits.rs`, fixtures `codex-rollout.jsonl` (three lines: a non-token event, a `token_count` with older limits, a `token_count` with `primary.used_percent = 93.0, window_minutes = 10080, resets_at = 1794287948, plan_type = "pro"`), `codex-rollout-bad.jsonl` (one malformed line then a good one).

**Interfaces — Produces:** `struct LimitWindow { label: String, used_percent: f64, resets_at: i64 }`; `struct AgentLimits { as_of: i64, plan: Option<String>, windows: Vec<LimitWindow> }`; `fn window_label(minutes: i64) -> String` (300 → "5 hr", 10080 → "week", else "<n> hr"/"<n> min"); `fn read_codex_limits(codex_home: &Path) -> Option<AgentLimits>` (newest `sessions/**/*.jsonl` by mtime; last line with `type == "event_msg"`, `payload.type == "token_count"`, `payload.rate_limits`; `as_of` = file mtime; skips malformed lines); `fn read_claude_limits(p: &RosterPaths, agent_id: &str) -> Option<AgentLimits>` (reads `limits_dir/<id>.json` written by Task 11); `fn format_reset(resets_at: i64, now: i64) -> String`.

- [ ] **Step 1: Failing tests**
```rust
#[test] fn codex_last_token_count_wins() { let l = read_codex_limits(&home_with_fixture("codex-rollout.jsonl")).unwrap(); assert_eq!(l.windows, vec![LimitWindow{label:"week".into(), used_percent:93.0, resets_at:1794287948}]); assert_eq!(l.plan.as_deref(), Some("pro")); }
#[test] fn codex_skips_malformed_lines() { assert!(read_codex_limits(&home_with_fixture("codex-rollout-bad.jsonl")).is_some()); }
#[test] fn codex_no_sessions_is_none() { assert!(read_codex_limits(&empty_dir()).is_none()); }
#[test] fn labels() { assert_eq!(window_label(300),"5 hr"); assert_eq!(window_label(10080),"week"); }
#[test] fn reset_format() { let now=1_000_000; assert_eq!(format_reset(now+318_797,now),"3d 16h"); assert_eq!(format_reset(now+78_745,now),"21h 52m"); assert_eq!(format_reset(now+600,now),"10m"); assert_eq!(format_reset(now-5,now),"resetting now"); }
```
- [ ] **Step 2-4:** FAIL → implement → PASS.
- [ ] **Step 5:** `git commit -m "feat(core): limits readers and reset formatting"`

### Task 9: `roster run` and session records

**Files:** Create `crates/roster-core/src/run.rs`, `crates/roster-core/src/session.rs`.

**Interfaces — Consumes:** `Roster`, `RosterPaths`, `find_tool`. **Produces:** `fn build_env(agent: &Agent, r: &Roster, p: &RosterPaths, parent_agent: Option<&str>) -> Vec<(String, Option<String>)>` — sets `CLAUDE_CONFIG_DIR` / `CODEX_HOME` to the agent's dir, or unsets it (`None`) for an adopted default; always unsets `CLAUDECODE`; sets `ROSTER_AGENT=<id>`; sets `ROSTER_PARENT=<parent id>` when given. `fn task_from_args(tool: Tool, args: &[String]) -> Option<String>` (Claude: value after `-p`/`--print`; Codex: first non-flag after `exec`; truncated to 120 chars). `struct SessionRecord { pid: u32, agent_id, parent_id: Option<String>, cwd, task: Option<String>, started_at: i64 }`; `fn start(p, rec) -> Result<PathBuf>`, `fn finish(path)`, `fn live_calls(p) -> Vec<SessionRecord>` (records with a parent whose pid is still alive; stale files deleted). `fn run_agent(id: &str, args: Vec<String>, p: &RosterPaths) -> Result<i32>`: resolve agent, `find_tool` (exclude `bin_dir`), banner + title only when stdin and stdout are TTYs (banner to stderr: name in agent color, tool, email if cached, week %/reset if known), spawn with inherited stdio, ignore SIGINT/SIGQUIT (Unix) or console Ctrl-C (Windows) in the parent **after** spawn, wait, write `finish`, return the child's code (Unix killed-by-signal → 128 + signal).

- [ ] **Step 1: Failing tests**
```rust
#[test] fn env_secondary_sets_dir() { let e = build_env(&worker_claude("~/.agent-roster/accounts/k1"), &r, &p, None); assert!(e.contains(&("CLAUDE_CONFIG_DIR".into(), Some(p.home.join(".agent-roster/accounts/k1").display().to_string())))); assert!(e.contains(&("CLAUDECODE".into(), None))); }
#[test] fn env_adopted_default_unsets_dir() { let e = build_env(&adopted_codex(), &r, &p, None); assert!(e.contains(&("CODEX_HOME".into(), None))); }
#[test] fn env_records_parent() { assert!(build_env(&a, &r, &p, Some("a1b2c3")).contains(&("ROSTER_PARENT".into(), Some("a1b2c3".into())))); }
#[test] fn task_extraction() { assert_eq!(task_from_args(Tool::Claude, &s(&["-p","write tests"])), Some("write tests".into())); assert_eq!(task_from_args(Tool::Codex, &s(&["exec","--full-auto","make hero image"])), Some("make hero image".into())); }
#[test] fn live_calls_drop_dead_pids() { }
// integration (tests/run.rs): fake `codex` script on a temp PATH that prints $CODEX_HOME and $ROSTER_AGENT and exits 7
#[test] fn run_passes_env_and_exit_code_without_banner_when_piped() { /* output == "<dir>\n<id>\n", no ESC bytes, code 7 */ }
```
- [ ] **Step 2-4:** FAIL → implement → PASS.
- [ ] **Step 5:** `git commit -m "feat(core): roster run with env, session records and clean piped output"`

### Task 10: Operations layer (plan, preview, apply)

**Files:** Create `crates/roster-core/src/ops.rs`, `crates/roster-core/src/share.rs`.

**Interfaces — Consumes:** everything above. **Produces:**
```rust
struct Step { kind: StepKind /* Add, Change, Remove, Note */, text: String }   // text is the user-facing preview line
struct Plan { summary: String, steps: Vec<Step>, roster_after: Roster, /* private: actions */ }
struct AddRequest { name: String, tool: Tool, color: Option<AgentColor>, role: Option<String>, share: bool }
fn plan_add(r: &Roster, p: &RosterPaths, lookup: &dyn CommandLookup, req: AddRequest) -> Result<Plan, OpError>;
fn plan_rename(r, p, lookup, name: &str, new_name: &str) -> Result<Plan, OpError>;
fn plan_remove(r, p, name: &str, delete_login_folder: bool) -> Result<Plan, OpError>;
fn plan_set_color(..), plan_set_role(..), plan_set_share(.., on: bool), plan_set_default(.., name);
fn apply(plan: Plan, p: &RosterPaths, journal: &mut Journal, roster_bin: &Path) -> Result<u64 /*op id*/, OpError>;
```
`apply` always: saves `roster_after`, rewrites launchers (Task 6), rewrites notes (Task 5), applies sharing (`share::apply_share`: Claude links `skills/agents/commands` + `@import` line for the primary `CLAUDE.md`; Codex links `skills` + copies primary `AGENTS.md` above the note with a `<!-- agent-roster:copied-from ... -->` marker), all in one journal transaction. New secondary folder: `accounts_dir/<id>`. Default color for a new agent: first color not yet used, in the order cobalt, marigold, jade, orchid, coral. `plan_set_default` on a secondary agent adds a marked rc block with `claude() { <name> "$@"; }` (zsh/bash), the fish equivalent, and a PowerShell profile function; interactive shells only (spec D5).

Preview text for add must read exactly (spec mockup):
```
Adds a <name> command for every terminal.
Creates a private folder for this account.
Tells your other agents that <name> exists.
Nothing else on your computer changes.
```

- [ ] **Step 1: Failing tests** (integration in temp HOME, `tests/ops.rs`)
```rust
#[test] fn add_creates_folder_shim_note_and_one_op() { /* after apply: accounts/<id> exists, bin/boss exists, ~/.claude/CLAUDE.md has "| boss |", journal.ops().len()==1 */ }
#[test] fn add_preview_text_exact() { assert_eq!(plan.steps.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), vec!["Adds a boss command for every terminal.","Creates a private folder for this account.","Tells your other agents that boss exists.","Nothing else on your computer changes."]); }
#[test] fn rename_swaps_launcher_keeps_folder() { /* bin/worker1 gone, bin/designer exists, accounts/<id> unchanged, live session record with that id still listed */ }
#[test] fn remove_keeps_login_folder_by_default() { }
#[test] fn share_on_links_skills_not_settings() { /* skills is link, settings.json absent */ }
#[test] fn undo_add_restores_everything() { /* undo op -> no shim, note row gone, CLAUDE.md byte-equal to before */ }
#[test] fn default_secondary_writes_interactive_function_only() { /* rc contains "claude() { worker1" inside markers; no file named claude in bin_dir */ }
```
- [ ] **Step 2-4:** FAIL → implement → PASS.
- [ ] **Step 5:** `git commit -m "feat(core): planned, previewed, journaled operations"`

### Task 11: Claude limits via status line (opt-in)

**Files:** Create `crates/roster-core/src/statusline.rs`; add `plan_enable_claude_limits` / `plan_disable_claude_limits` to `ops.rs`.

**Interfaces — Produces:** `fn plan_enable_claude_limits(r, p, name) -> Result<Plan, OpError>` (resolves the agent's `settings.json` **real path**; if another agent already uses that real path, reuses its chain entry; stores the existing `statusLine.command` as `chain`; sets `statusLine = {"type":"command","command":"<roster_bin> statusline"}`; sets `limits = claude-statusline` on every agent sharing that file); `fn handle_statusline(stdin_json: &str, env: &dyn Fn(&str)->Option<String>, r: &Roster, p: &RosterPaths, runner: &dyn CommandRunner) -> String` — agent resolved by `ROSTER_AGENT`, else by `CLAUDE_CONFIG_DIR` matching `config_dir`, else the adopted Claude default; writes `limits_dir/<id>.json` from `rate_limits.five_hour` and `rate_limits.seven_day` (`used_percentage`, `resets_at`); runs `chain` through the shell with the same stdin and returns its stdout unchanged; with no chain returns the agent name.

- [ ] **Step 1: Failing tests**
```rust
#[test] fn writes_limits_file_from_rate_limits() { /* stdin {"rate_limits":{"seven_day":{"used_percentage":61,"resets_at":1794528000}}} -> read_claude_limits gives week 61.0 */ }
#[test] fn chains_existing_statusline_output_unchanged() { /* chain = "printf hello"; returns "hello" */ }
#[test] fn shared_settings_written_once() { /* two agents whose settings.json resolve to one real file: one statusLine, one chain, both limits=claude-statusline */ }
#[test] fn disable_restores_original_statusline() { }
#[test] fn resolves_agent_by_env_then_dir_then_default() { }
```
- [ ] **Step 2-4:** FAIL → implement → PASS.
- [ ] **Step 5:** `git commit -m "feat(core): opt-in Claude limits through a chained status line"`

### Task 12: Import the bash prototype

**Files:** Create `crates/roster-core/src/import.rs`, fixture `tests/fixtures/prototype/roster.conf` (the owner's default four lines).

**Interfaces — Produces:** `fn detect_prototype(p: &RosterPaths) -> Option<PathBuf>`; `fn plan_import(p: &RosterPaths, lookup: &dyn CommandLookup) -> Result<Plan, OpError>`. Mapping (spec §6.1): each line → `Agent` keeping name, tool, order and `config_dir` **in place**; colors in order cobalt, marigold, jade, orchid; note text → `role`; first Claude/Codex line with an adopted dir → `default = true`. Links found in a secondary dir: `skills` → `share = true`; `settings.json`, `hooks`, `config.toml` → kept as is and listed in `advanced_links`; `CLAUDE.md`/`AGENTS.md` links → `replace_link_with_copy`. Removes files in `~/.local/bin` whose line 2 is `# generated-by-agent-roster` (prototype marker, no id) and the prototype script `~/.local/bin/agent-roster`, both backed up by the journal. Renames `roster.conf` to `roster.conf.imported`. One journal op.

- [ ] **Step 1: Failing tests**
```rust
#[test] fn imports_four_agents_in_place() { /* names agent1..agent4, config_dirs "~/.claude","~/.claude-2","~/.codex","~/.codex-2", defaults agent1 and agent3 */ }
#[test] fn settings_link_kept_claude_md_link_replaced() { /* ~/.claude-2/settings.json still a link; ~/.claude-2/CLAUDE.md now a regular file */ }
#[test] fn old_launchers_removed_with_backup_and_undo_restores_them() { }
```
- [ ] **Step 2-4:** FAIL → implement → PASS.
- [ ] **Step 5:** `git commit -m "feat(core): import the bash prototype in place"`

### Task 13: CLI

**Files:** `crates/roster-cli/src/{main.rs,render.rs,prompt.rs}`, `crates/roster-cli/tests/cli.rs`.

**Interfaces — Consumes:** all core functions. Commands (spec §6.5): `roster` (overview), `add <name> --claude|--codex [--color C] [--role R] [--share] [--yes]`, `rename <old> <new>`, `color <name> <C>`, `role <name> <text>`, `share <name> on|off`, `default <name>`, `remove <name> [--delete-login-folder]`, `login <name> [--device]`, `limits [enable|disable <name>]`, `changes`, `undo <op> | --all`, `import`, `doctor`, `ui`, `uninstall`, hidden `run <id> -- <args>` and `statusline`. Mutating commands print the plan's steps and ask `Continue? [Y/n]` unless `--yes`; non-TTY without `--yes` exits 2 with "Add --yes to run without a prompt." Read commands accept `--json`. First run with no roster prints the detected tools and suggests `roster import` (prototype found) or `roster add`.

`login`: Claude runs `claude auth login` through `run_agent`; Codex runs `codex login`, with `--device-auth` when `--device` is passed or no display is detected (Linux with neither `DISPLAY` nor `WAYLAND_DISPLAY`). `ui`: if the app is not installed, prints "The app window isn't installed. Everything works from these commands." and exits 0. `uninstall`: `undo_all`, then removes `bin_dir`, `run_dir`, `limits_dir`, `journal_dir`, roster file; keeps `accounts_dir` unless `--delete-login-folders`. `doctor`: checks tools found, `bin_dir` on PATH, every agent has a launcher, notes present, statusline chain valid; prints fixes.

Overview layout follows `docs/mockups/terminal.png`: header `Agent Roster  <n> agents` and right-aligned `limits as of <age> ago`; per agent: name (agent color, bold), tool, status dot (`● ready` jade, `● sign in` marigold, `● working` agent color, `● idle` dim), then one row per limit window: label, 10-cell bar (`█` used in agent color, coral when ≥ 90, `░` rest), percent, `resets in <format_reset>`; then live calls as `<parent> ━━━━━━━━━━━━━━━▶ <child>   "<task>"  <age>`; then the hint line `roster add <name>    roster limits    roster ui    roster changes`.

- [ ] **Step 1: Failing tests** (`assert_cmd`, `HOME` and `PATH` pointed at temp dirs with fake `claude`/`codex` scripts)
```rust
#[test] fn overview_lists_agents_plain_when_piped() { /* no ESC bytes; contains "agent1", "Claude Code", "93%", "resets in" */ }
#[test] fn no_color_env_respected() { }
#[test] fn add_without_yes_in_pipe_exits_2() { cmd.args(["add","boss","--codex"]).assert().code(2).stderr(contains("Add --yes")); }
#[test] fn add_yes_then_shim_runs_fake_codex_with_env() { /* run temp bin/boss; fake codex prints CODEX_HOME = accounts/<id> */ }
#[test] fn json_overview_shape() { /* keys: agents[].name, tool, status, limits.windows[].used_percent */ }
#[test] fn missing_claude_shows_not_installed_and_exits_0() { }
#[test] fn undo_all_then_uninstall_leaves_home_clean() { /* only accounts_dir remains */ }
```
- [ ] **Step 2-4:** FAIL → implement → PASS. Also `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` clean.
- [ ] **Step 5:** `git commit -m "feat(cli): roster command with overview, preview prompts and json"`

### Task 14: CI and replace the prototype on the owner's Mac

**Files:** Create `.github/workflows/ci.yml` (matrix: macos-latest, ubuntu-latest, windows-latest; `cargo test --workspace`, clippy, fmt; musl build check on ubuntu).

- [ ] **Step 1:** Push is **not** part of this task (no remote yet). Run the workflow locally where possible: `cargo test --workspace` on macOS; `docker run --rm -v $PWD:/w -w /w rust:1 cargo test --workspace` for Linux. Expected: all PASS.
- [ ] **Step 2:** `cargo build --release -p agent-roster`; copy `target/release/roster` to `~/.local/bin/roster` (already on the owner's PATH). The owner asked to use it right away (2026-11-09), so this runs without another prompt; it is undoable.
- [ ] **Step 3:** `roster import` → review the printed plan → confirm. Expected: four agents, defaults agent1/agent3, one journal op.
- [ ] **Step 4:** In a new cmux pane: `agent1 -p "Reply with exactly: OK1"` → `OK1`; `agent3 exec --skip-git-repo-check "Reply with exactly: OK3"` → `OK3`; inside an `agent1` session ask "run this with agent3: reply OK" → agent3 answers; `roster` shows the live call while it runs and agent3's week %.
- [ ] **Step 5:** `roster changes` lists the import op; `roster undo <op>` restores the prototype exactly (check `~/.local/bin/agent1` is back), then `roster import` again.
- [ ] **Step 6:** `git commit -m "ci: test matrix; verified prototype replacement on macOS"`
