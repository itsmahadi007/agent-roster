# Agent Roster — project instructions

Give each Claude Code / Codex account a name, run it from any terminal, let agents call each other by name.
Rust workspace: `crates/roster-core` (all logic) + `crates/roster-cli` (binary `roster`). Public repo: github.com/itsmahadi007/agent-roster.

## Read first
- Spec (binding): `docs/superpowers/specs/2026-11-09-agent-roster-design.md`
- Plans: `docs/superpowers/plans/` (Plans 1 and 2 done, each with a ledger of every ruling; Plan 0 spike pending)
- Visual source of truth for the app: `docs/mockups/app-window.html` and `docs/mockups/terminal.html`
- App: `app/` (Tauri 2 + Svelte 5). `cd app && npm test`, `cargo tauri build --bundles app`. Done goal: `docs/goals/plan-2-app-window.goal.txt`
- App icon: `assets/icon/icon-a.png` (chosen; generated with Codex)

## Owner's rules
- NEVER add Co-Authored-By, "Generated with", or any AI attribution to commits or PRs.
- Approved work: move fast, little narration, one report at the end. Run commands yourself; don't hand the owner commands to type unless a permission check blocks you.
- Reply in plain short English. No Bangla script in chat.
- Default to the latest stable version of every dependency and toolchain.
- Never push secrets; never put real emails or names of other people in docs, mockups or tests (use example.com).

## Safety rules (from the spec, non-negotiable)
- Every change is planned, previewed, applied in one `Journal` transaction, and undoable.
- No hooks into any tool, ever. Never read credential files (`auth.json`, `.credentials.json`, keychain secrets).
- Never write through a link into another account's files; user-owned dotfiles (rc files, main CLAUDE.md/AGENTS.md) are written through their link.
- Never move or delete an adopted folder (`~/.claude`, `~/.codex`, `~/.claude-2`, `~/.codex-2`).
- The owner's real roster (agent1-agent4) is live on this Mac. Ask before anything destructive to it.

## Checks before any commit
`cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Windows code: `cargo clippy --workspace --all-targets --target x86_64-pc-windows-msvc -- -D warnings`
