# Usage and account health without a terminal — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Agent Roster finds every agent's weekly usage, reset time and account health by itself, with no terminal and no status line needed first.

**Architecture:** `roster-core::usage` asks the tools: `claude.rs` (`claude auth status --json` + `claude -p /usage`), `codex.rs` (`codex app-server`, `account/rateLimits/read`), shared `Health`/`Check`/`SavedHealth` and `refresh_all` in `mod.rs`, platform code in `windows.rs` / `unix.rs`. Each check is saved (`limits/<id>.json`, `limits/<id>.health.json`); the overview, the tray and `roster` only read what is saved. `view::shown_health` turns live sign-in plus the saved check into one word per row.

**Tech Stack:** Rust 2024 workspace, jiff for reset times, Tauri 2 + Svelte 5 + vitest.

**Spec:** `docs/superpowers/specs/2026-11-09-agent-roster-design.md` §6.6 (rewritten). Goal: `docs/goals/auto-usage.goal.txt`.

## Global Constraints

- Never a model call to read usage: a `/usage` reply with turns or cost marks that Claude Code version unsupported.
- Never read credential files; never change the real claude1 agent; tests use temp dirs and recorded fixtures (example.com only).
- Every new background process is hidden on Windows (`proc::hide_console`; `RealRunner` already does).
- Checks: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, `cd app && npm test && npm run check`.

## Tasks

- [x] **Claude check** (`usage/claude.rs`): `claude_health(json) -> Option<Health>` (None = unreadable, not signed out), `UsageReply::NoPlan`, `check_claude(...) -> Check`. Fixtures recorded 2026-11-10 with Claude Code 2.1.322 and an empty `CLAUDE_CONFIG_DIR`: `claude-auth-status-signed-out.json`, `claude-auth-status-api-key.json` (fake `ANTHROPIC_API_KEY`), `claude-usage-{signed-out,api-key,subscription}.json` (all `num_turns: 0`, `total_cost_usd: 0`).
- [x] **Shared saving** (`usage/mod.rs`): `Check::settled` (week at 100% → limit reached), `SavedHealth { health, checked_at, known }`, `next_health`, `record_check` (never replaces numbers with nothing or older ones), `usage_runner` (15 s limit, empty working folder), `refresh_all`.
- [x] **Overview** (`status.rs`): `AgentStatus.health`; `limits_for` reads saved numbers for every tool, newest `as_of` wins for Codex; `detect::ask_sign_in` returns None when the tool can't be asked, and the overview then keeps what the last check knew.
- [x] **Rows** (`view.rs`): `Shown`, `shown_health`, `usage_note`, `as_of_text`; `AgentView.health` and `AgentView.as_of`; tray words follow. CLI `render.rs` uses the same words; `roster limits` runs `refresh_all` first.
- [x] **App**: `Backend::refresh_usage` behind `UsageGate` (one at a time, 30 s gap, never-checked agents skip the gap); `refresh_usage` command; a usage thread checks on start, every 5 minutes, and when the overview shows "checking"; the window listens for `usage-checked`. Refresh calls `refresh_usage`; the 10 s poll calls only `overview`.
- [x] **UI**: row words (checking, no subscription, limit reached in coral, as of), drawer Sign in button, "Agent Roster asks … itself" text, Hide usage only while the status line is on, Help text.
- [x] **Docs**: spec §6.6, README usage section, this plan.
- [ ] **Codex check** (`usage/codex.rs`, `usage/windows.rs`, `usage/unix.rs`): built in parallel on branch `usage-codex`; `refresh_all` already calls `codex::fetch_codex_usage(agent, r, p, tool_path) -> Check`.

## Ledger of rulings

| # | Ruling | Why |
|---|---|---|
| 1 | Health comes from `claude auth status --json` first; `/usage` is asked only for a signed-in account with a plan. | Saves a 2 s call per signed-out or API-key agent; `auth status` is the tool's own answer. |
| 2 | `authMethod` "claude.ai" with no `subscriptionType` (or "free"), `api_key`, Bedrock/Vertex → no subscription. No `authMethod` at all (older versions) → let `/usage` decide (`Total cost:` only → no subscription). | Matches the recorded outputs; older versions aren't called "no subscription" on a guess. |
| 3 | An unreadable `auth status` reply, a timeout or a crash is Unreachable, never signed out; the 10 s overview then keeps the last known sign-in. | Goal: never flip a signed-in agent to "sign in". |
| 4 | A signed-in `/usage` reply with no week line and no cost summary is Unreachable. | It means Claude Code couldn't fetch the numbers (offline); keep the last ones. |
| 5 | Limit reached = week rounds to 100%, or the tool said so and no newer numbers or passed reset overrule it. | Codex can report a reached limit while the week is below 100%. |
| 6 | Saved health keeps `known`, the newest health a check did learn, next to the newest result. | An unreachable check must not erase "no subscription" or "ok". |
| 7 | "checking" shows only when no check has finished for that agent (or the last one predates its sign-in or install) and there are no numbers; numbers seen before any check still show. | Never "not used yet" when the truth is unknown; never hide true numbers. |
| 8 | No subscription hides old numbers. | Without a plan there is no weekly limit; old numbers would mislead. |
| 9 | `LimitsSource` no longer gates reading numbers (Codex `Off` included); for Claude it only means "our status line is installed". | Numbers come from the tools themselves now. |
| 10 | The 5-minute timer lives in Rust (a thread), not in the webview. | The tray stays current while the window is hidden. |
| 11 | The 10 s poll never runs `/usage`; when a row says "checking" it only sets a flag the usage thread reads (still 30 s throttled). | A new sign-in shows numbers within seconds without the poll doing the slow work. |
| 12 | `refresh_claude_usage` (from 96b3e75) is replaced by `refresh_all`. | One entry point for the app and the CLI. |
| 13 | Usage checks run with a 15 s limit in an empty folder under `limits/`. | `--setting-sources local` would otherwise read a project's local settings from the working folder. |
| 14 | "Show usage" leaves the drawer; `roster limits enable` stays for people who want the status line. | Numbers no longer need it. |
