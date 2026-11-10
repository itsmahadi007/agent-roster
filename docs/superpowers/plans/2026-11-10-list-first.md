# List-first window, weekly limit only — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the list the whole window (rows open in place), show only the weekly limit with its reset time, fix "resets in resetting now", and ship it as v0.3.0 through the app's own updater.

**Architecture:** roster-core gets two small helpers in `limits.rs` (`weekly`, `reset_phrase`) that every surface (view, CLI table, terminal banner, warnings, tray) uses, so "week only" and the reset wording live in one place. The Svelte app drops the two-pane grid: `AgentList` renders every row and, under the open one, `AgentDrawer` (today's `AgentPage`, minus its name header and usage cards). `App` owns which row is open and whether its drawer has a waiting change.

**Tech Stack:** Rust 2024 workspace (roster-core, roster-cli), Tauri 2 + Svelte 5 + vitest + @testing-library/svelte.

**Spec:** `docs/superpowers/specs/2026-11-10-list-first-design.md`. Visual: `docs/mockups/list-first.html`. Goal: `docs/goals/list-first.goal.txt`.

## Global Constraints

- Safety rules: preview before every change, every change journaled and undoable, no hooks, never write through a link into another account's files, never read credential files.
- Never apply a change to the owner's real agents while testing; fixtures or a temp HOME only.
- Never touch `~/Applications/Agent Roster.app` or `~/.local/bin/roster`; run the built app only from `target/`.
- example.com emails only. No AI attribution in commits.
- Existing tests change only where they assert a 5-hour window or the old reset wording.
- Checks before every commit: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`; app tasks also `cd app && npm test`.
- Copy: "resets in 2d 18h", "resetting now", "68% week", "not used yet", "sign in", "Apply or cancel this change first."

## Review Focus

1. Reset time already passed (agent3 today): every surface says "resetting now"; the warning says "It is resetting now." — pinned in Task 1.
2. A tool reports only a 5-hour window (no week): no bar, no crash; the row reads like "no numbers yet" — pinned in Task 2 (`only_five_hour_window_shows_nothing`).
3. Poll refresh while a drawer has a typed-but-unapplied rename: the drawer stays open and keeps the text — pinned in Task 4 (`open row and its edits survive a refresh`).
4. Clicking a different row while a change waits: no switch, nudge shown, the change survives — pinned in Task 4.
5. Removing the open agent: no drawer open afterwards, no error — pinned in Task 4.

---

### Task 1: One reset phrase everywhere ("resetting now" bug)

**Files:**
- Modify: `crates/roster-core/src/limits.rs` (add helper + unit test)
- Modify: `crates/roster-core/src/view.rs:205-222, 306-325`
- Modify: `crates/roster-cli/src/render.rs:72-100` (extract `window_line`), add `#[cfg(test)] mod tests`
- Modify: `crates/roster-core/src/run.rs:104-122` (extract `banner_limit`), test in its `mod tests`
- Test: `crates/roster-core/tests/view.rs`

**Interfaces:**
- Produces: `pub fn reset_phrase(resets_at: i64, now: i64) -> String` in `roster_core::limits` → `"resets in 21h 52m"` or `"resetting now"`.
- Produces (private): `render::window_line(w: &LimitWindow, color: AgentColor, now: i64) -> String`; `run::banner_limit(limits: Option<AgentLimits>, now: i64) -> String`.

- [ ] **Step 1: Failing tests**
  - limits.rs `reset_phrase_reads_well`: `reset_phrase(now + 78_745, now) == "resets in 21h 52m"`; `reset_phrase(now - 5, now) == "resetting now"`; `reset_phrase(now, now) == "resetting now"`.
  - tests/view.rs `past_reset_says_resetting_now`: sample with agent3's week `resets_at = NOW - 60` → `reset_text == Some("resetting now")`, warning text `"has used 93% of its week. It is resetting now."`.
  - render.rs `window_line_past_reset`: week window 94%, `resets_at = now - 1` → output contains `"resetting now"` and not `"resets in resetting"`.
  - run.rs `banner_past_reset`: `banner_limit(Some(limits with week 94%, resets_at now-1), now)` == `"  week 94%, resetting now"`; with future reset `now + 78_745` == `"  week 94%, resets in 21h 52m"`.
- [ ] **Step 2:** `cargo test --workspace` → the four new tests FAIL (missing fn / wrong text).
- [ ] **Step 3:** Implement `reset_phrase`; use it in view `reset_text`, warnings ("It is resetting now." when past, else "It resets in X."), `window_line`, `banner_limit`.
- [ ] **Step 4:** `cargo test --workspace` → PASS.
- [ ] **Step 5:** Commit `core: one reset phrase, "resetting now" instead of "resets in resetting now"`.

### Task 2: Weekly limit only in every view

**Files:**
- Modify: `crates/roster-core/src/limits.rs` (helper), `crates/roster-core/src/view.rs` (`agent_view` windows, `main_window` removed, warnings), `crates/roster-cli/src/render.rs` (only the week line), `crates/roster-core/src/run.rs` (`banner_limit` week only), `crates/roster-core/src/ops.rs:855` (copy)
- Test: `crates/roster-core/tests/view.rs`, `crates/roster-core/src/run.rs` tests, `crates/roster-cli/src/render.rs` tests, ops preview test if one asserts the copy

**Interfaces:**
- Produces: `pub fn weekly(windows: &[LimitWindow]) -> Option<&LimitWindow>` (label `"week"` only, no fallback).
- Consumes: `reset_phrase` from Task 1.

- [ ] **Step 1: Failing tests**
  - view.rs: rename `two_windows_reset_line_names_the_last` → `five_hour_window_is_ignored`: agent1 (5 hr 22% + week 61%) → `windows == [WindowView{label:"week",used_percent:61,hot:false}]`, `reset_text == Some("resets in 3d 16h")`, `tray_text == "61% week"`.
  - view.rs `only_five_hour_window_shows_nothing`: agent with only `win("5 hr", 95.0, 3600)` → `windows` empty, `reset_text None`, `tray_text == ""`, no warning for it, `limits_note == Some("usage shows after you use <name>")`.
  - run.rs `banner_ignores_five_hour`: limits with only a 5 hr window → `""`.
  - render.rs `overview_shows_week_only`: an `Overview` with one agent holding 5 hr + week → text contains `"week"` and not `"5 hr"` (build the Overview with the same helpers tests/view.rs uses, or the smallest literal).
  - ops: the enable-limits preview step text is `"Changes the status line in <path> so Agent Roster can read <name>'s weekly use."` (update the existing assertion if any; else add one in the ops test that previews enable-limits).
- [ ] **Step 2:** `cargo test --workspace` → new tests FAIL.
- [ ] **Step 3:** Implement `weekly`; view windows = `weekly(..)` cloned into a 0/1 vec; tray uses it; warnings loop only over `weekly`; render prints only the week line; banner only the week; ops copy. Statusline parsing untouched (its test that reads `"5 hr"` stays).
- [ ] **Step 4:** All checks pass, including windows clippy.
- [ ] **Step 5:** Commit `core: weekly limit only on every surface`.

### Task 3: Fixture, types and copy for week only

**Files:**
- Modify: `app/src/lib/fixtures/overview.json` (drop every `"5 hr"` window; `reset_text` without `"week "`), `app/src/screens/Help.svelte:48`, `app/src-tauri/tauri.conf.json` (`width` 760, `minWidth` 560; height unchanged), project `CLAUDE.md` (main window source of truth = `docs/mockups/list-first.html`; app-window.html kept for the other screens).
- Test: `app/src/screens` Help test if it asserts the old copy.

- [ ] Step 1: Help copy: "The bar shows how much of its weekly limit an account has used, and when it resets. It turns coral at 90%." (keep the rest of the paragraph's meaning).
- [ ] Step 2: `npm test` passes except tests Task 4 rewrites; commit with Task 4.

### Task 4: List-first window with rows that open in place

**Files:**
- Rename: `app/src/components/AgentPage.svelte` → `app/src/components/AgentDrawer.svelte` (and its test → `AgentDrawer.test.ts`)
- Modify: `app/src/components/AgentList.svelte`, `app/src/App.svelte`
- Test: `app/src/components/AgentList.test.ts`, `app/src/components/AgentDrawer.test.ts`, `app/src/App.test.ts`

**Interfaces:**
- `AgentDrawer` props: `{ agent: AgentView; palette: ColorView[]; terminals: TerminalApp[]; nudge: number; onApplied(notice: string, op: OpRequest["op"]): void; onDirty(dirty: boolean): void; onClose(): void }`. Removes: the `<h2>` name and both usage cards. Keeps: meta line (tool, email or "not signed in yet", plan, default label, status), TerminalButton, "Or type <name> in any terminal" + Copy, the usage line (Show usage / Hide usage), one no-numbers line when `agent.windows.length === 0` and not usageOff (`"Weekly use shows after you use <name>. Type <name> in a terminal and send one message."`, or `"Weekly use shows after sign in."` when signed out), the whole settings grid, Sign in again, Remove, the save bar. Calls `onDirty(req !== null)` whenever that changes. When `nudge` increases while `req` is set, the save bar shows `"Apply or cancel this change first."`. Escape: open menu → ignore; `req` → `reset()`; else `onClose()`. The save bar is `position: sticky; bottom: 0` inside the scrolling column.
- `AgentList` props: `{ view; open: string | null; palette; terminals; round: number; nudge: number; onToggle(id: string): void; onNavigate; onRefresh; onSite; onApplied; onDirty; onClose }`. Each row is a `<button class="item" aria-expanded={a.id === open}>`; right column: `"{pct}% week"` + `"resets in …"`/`"resetting now"` (from `reset_text`) + one bar; coral (`.hot`) on all three when `w.hot`; else `"sign in"` / `"not used yet"`; chevron `›` rotated 90° when open. Under the open row: `{#key `${open}:${round}`}<AgentDrawer …/>{/key}`. Arrows move focus (not open). Footer unchanged, in the same column.
- `App`: state `open: string | null = null`, `dirty = false`, `nudge = 0`. `toggle(id)`: if `dirty` → `nudge += 1`, return; else `open = open === id ? null : id`. After Add: open the new id (existing "fresh" logic). After remove: `open = null`. Rename keeps id. Body = one scroll column `max-width: 720px; margin: 0 auto`; Help/Changes/Add replace the list. Toast `left: 50%`.

- [ ] **Step 1: Failing tests**
  - AgentList: `"rows show weekly use and when it resets"` — agent1 row text contains `"61% week"` and `"resets in 3d 16h"`, agent3 row has `"93% week"`, `"resets in 21h 52m"`, and its `.pc`/reset elements have class `hot`; agent2 `"sign in"`; agent4 `"not used yet"`; no row contains `"5 hr"`.
  - AgentList: `"nothing is open at first; a click asks to open the row"` — with `open: null` no row has `aria-expanded="true"` and no `Name` field exists; click agent3 → `onToggle("a3")`; Enter on a focused row → `onToggle`.
  - AgentList: `"the open row shows its settings under it"` — `open: "a1"` → agent1 row `aria-expanded="true"`, a textbox with value `"agent1"` exists, a `radiogroup` named Color exists.
  - AgentList: arrow keys move focus between rows without calling `onToggle`.
  - AgentDrawer: port today's AgentPage tests; drop the `"5-hour use"` card assertions; add `"Escape cancels a waiting change first, then closes"` (type a new name → Escape → name back to `agent1`, `onClose` not called → Escape → `onClose` called once) and `"says to apply or cancel when nudged"`.
  - App: `"clicking a row opens it; clicking again closes it"`; `"another row can't open while a change waits"` (open agent1, type a name, click agent3 → still agent1's drawer, text "Apply or cancel this change first." shown, typed name kept); `"open row and its edits survive a refresh"` (open agent1, type, advance the 10 s poll → same drawer, same text); `"removing the open agent closes the drawer"` (replaces `"removing an agent opens the one above it"`); `"add agent opens the new agent's row"`; existing Show/Hide usage, rename, help, what-changed tests updated to open agent1's row first.
- [ ] **Step 2:** `cd app && npm test` → the new tests FAIL.
- [ ] **Step 3:** Implement per the Interfaces above. Styles follow the mockup: rows are bordered cards (`--raise`, 1px `--line`, radius 9px) with 8px gap; the open row joins its drawer (square bottom corners, drawer border `--line-2`).
- [ ] **Step 4:** `npm test` passes; `npx svelte-check` (if configured in package.json scripts) clean; Rust checks pass.
- [ ] **Step 5:** Commit `app: list-first window, rows open in place, weekly limit with reset time`.

### Task 5: Build, look, document

- [ ] Step 1: `cd app && cargo tauri build --bundles app` → `.app` in `target/release/bundle/macos/`.
- [ ] Step 2: Launch that `.app` from `target/` (real roster, look only). Screenshot (a) nothing open, (b) agent1 open (`screencapture -l <windowid>`), save to `docs/images/main-window.png` and `docs/images/agent-open.png` (replace the old main-window/agent-page images README uses). Compare with `docs/mockups/list-first.html`; fix clear mismatches.
- [ ] Step 3: README text for the list and agent page describes the list-first window and weekly limit. Commit `docs: screenshots and README for the list-first window`.

### Task 6: Review, merge, CI

- [ ] Step 1: Fresh reviewer subagent over `main..ui/list-first`. Fix every Critical/Important with a test that fails first.
- [ ] Step 2: All checks (incl. `--target x86_64-pc-windows-msvc` clippy, `npm test`). Merge `ui/list-first` into main (no fast-forward needed), push, wait for CI, print `gh run view`.

### Task 7: Release v0.3.0

- [ ] Step 1: Bump `0.2.1` → `0.3.0` in `Cargo.toml` (workspace), `Cargo.lock` (via `cargo check`), `app/package.json`, `app/package-lock.json`, and any other app-version spot `grep -rn "0\.2\.1"` finds (not dependency versions). Checks pass. Commit `v0.3.0`, push main.
- [ ] Step 2: `git tag v0.3.0 && git push origin v0.3.0`; watch the release workflow to green.
- [ ] Step 3: Print `gh release view v0.3.0`, the downloaded `latest.json` (version 0.3.0, darwin-aarch64 + darwin-x86_64 with signatures), `curl -sIL` 200 for both urls, the released mac CLI tarball's `roster --version` = 0.3.0, and `CFBundleShortVersionString` of `~/Applications/Agent Roster.app` still 0.2.1.
