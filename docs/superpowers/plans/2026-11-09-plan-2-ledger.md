# SDD ledger — plan: docs/superpowers/plans/2026-11-09-plan-2-app-window.md
Spec: docs/superpowers/specs/2026-11-09-agent-roster-design.md (§4–§6.7); mockup docs/mockups/app-window.html
Setup: Ruling: branch feat/app-window in place, no worktree — same as Plan 1, main untouched — cost if wrong: none.
Pre-flight:
- T1 overview_view/first_run_view/changes_view → T5 Backend consumes same names — consistent.
- T2 plan_adopt_all(&[(Tool, String)]) → T5 OpRequest::AdoptAll{picks: Vec<(Tool,String)>} — consistent.
- T3 find_roster_bin/terminal_command → T5; has_desktop/find_app/app_launch_command → T4 — consistent.
- T5 command names → T6 api.ts invoke names — consistent.
- T1 tray_text → T10 tray_entries; T5 open_terminal_as(name) → T10 term:<name> — consistent.
- T7 swatch hex "from the overview fixture's palette list" but T1 OverviewView has no palette — Ruling: add `palette: Vec<{label, hex}>` to OverviewView from AgentColor::ORDER, so JS holds no color constants — cost if wrong: one field.
Task 1: Ruling: AgentView gains `plan` (sign-in plan, else the limits plan) — spec §6.1 says Codex rows show the plan when no email — cost if wrong: one unused field
Task 1: complete (commits 4fb37cf..d119117, tests: cargo test --workspace → test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s)
Task 1: note: full suite 114 passed 0 failed (task-done tail shows the doc-test line)
Task 2: complete (commits d119117..000e756, tests: .superpowers/sdd/2026-11-09-plan-2-app-window/check.sh → suite: 117 passed, 0 failed)
Task 3: complete (commits 000e756..e5fb379, tests: .superpowers/sdd/2026-11-09-plan-2-app-window/check.sh → suite: 127 passed, 0 failed)
Task 4: complete (commits e5fb379..d0679c2, tests: .superpowers/sdd/2026-11-09-plan-2-app-window/check.sh → suite: 129 passed, 0 failed)
Task 4: note: ui_without_display_says_no_desktop is cfg(linux), runs in CI only
Task 5: Ruling: Backend takes a BackendConfig with injected lookup, env and opener (plan listed only p, roster_bin, scratch, opener) — tests need fake PATH lookup and TERMINAL without touching the process env — cost if wrong: none.
Task 5: Ruling: removed the android/ios icon folders `cargo tauri icon` generates — desktop-only app — cost if wrong: re-run cargo tauri icon.
Task 5: Ruling: test sources live in backend_tests.rs (#[path]) to keep backend.rs focused — cost if wrong: none.
Task 5: complete (commits d0679c2..ecf3a1c, tests: .superpowers/sdd/2026-11-09-plan-2-app-window/check.sh → suite: 139 passed, 0 failed)
Task 6: Ruling: "Turn on limits" sits in the limits column next to "limits off" (not the status cell), and "Turn off limits" is a hover link after the reset line — the 128px status cell must keep the agent's state visible — cost if wrong: CSS placement.
Task 6: Ruling: sub line shows "not signed in yet" for agents needing sign in, else email, else plan — matches mockup row agent2 — cost if wrong: copy.
Task 6: complete (commits ecf3a1c..b679a27, tests: sh -c 'cd app && npm test && npm run check' → 1794214897233 COMPLETED 294 FILES 0 ERRORS 0 WARNINGS 0 FILES_WITH_PROBLEMS)
Task 6: Ruling: window 1200x660 (min 1000x560) instead of 1080x640 — at 1080 the sub line truncated 'default for claude'; checked visually in Chrome with fixture data — cost if wrong: one config value. Note: screencapture is blocked (no Screen Recording permission for this shell); visual checks use the Vite dev server + fixture mock in Chrome.
Task 7: Ruling: OverviewView gains `next_color` (Roster::next_color) so Add agent preselects the color the CLI would pick without JS logic — test next_color_is_the_one_add_would_pick — cost if wrong: one field.
Task 7: Ruling: Add agent and the sheets render inside the main window (screen state + modal), not separate OS windows as drawn in the mockup — one window keeps tray "Add agent" simple — cost if wrong: layout only.
Task 7: Ruling: if sign-in fails to open after a successful add, the app returns to Your agents with a notice naming the agent (the add is already journaled) — cost if wrong: copy.
Task 7: complete (commits 025914a..4e458dc, tests: sh -c '.superpowers/sdd/2026-11-09-plan-2-app-window/check.sh && cd app && npm test' →    Duration  2.09s (environment 44%, tests 25%, setup 16%, transform 13%, import 3%))
Task 8: Ruling: candidate names are shown in palette[i] (the colors adopt will assign to an empty roster); sub copy drops 'signed in' when a found login isn't signed in — cost if wrong: cosmetic.
Task 8: complete (commits 4e458dc..af615a2, tests: sh -c 'cd app && npm test && npm run check' → 1794215659757 COMPLETED 302 FILES 0 ERRORS 0 WARNINGS 0 FILES_WITH_PROBLEMS)
Task 9: Ruling: undo confirm button reads 'Yes, undo' (plan said Undo) — avoids two buttons named Undo on screen — cost if wrong: copy.
Task 9: complete (commits af615a2..79b137c, tests: sh -c 'cd app && npm test && npm run check' → 1794215718068 COMPLETED 305 FILES 0 ERRORS 0 WARNINGS 0 FILES_WITH_PROBLEMS)
Task 10: Ruling: view structs also derive Deserialize so the app's Rust tests read the same fixture JSON — cost if wrong: none.
Task 10: Ruling: agent rows in the tray show the window (they are the status list); terminals open from the "Open a terminal as…" submenu — mockup lists agents then that item — cost if wrong: one click more.
Task 10: Ruling: macOS Dock click (RunEvent::Reopen) shows the hidden window again — closing hides to the menu bar — cost if wrong: none.
Task 10: complete (commits 79b137c..afeef8e, tests: .superpowers/sdd/2026-11-09-plan-2-app-window/check.sh → suite: 143 passed, 0 failed)
Task 11: Ruling: CI is first run on a draft PR from feat/app-window (push to the owner's public repo, inside the goal's merge-and-push scope) so Windows/Linux failures surface before merge; local Windows cross-clippy of the app crate can't run (tauri-build needs the Windows resource compiler) — cost if wrong: one extra PR.
Task 11: complete (commit efdcbdb, workflow parsed: jobs test/app/musl; CI result recorded in Task 12)
Task 12: real app launched via `roster ui` from ~/Applications/Agent Roster.app (13.32 MiB); owner took the window screenshot (this shell lacks Screen Recording): /Users/mahadihassan/Desktop/Screenshot 2026-11-09 at 3.23.01 PM.png — shows agent1-agent4 and agent3 Codex week 94% coral. Ruling: screenshot not committed — it shows the owner's real email and the repo is public (CLAUDE.md: no real emails in docs) — cost if wrong: owner copies it in.
Final review: fresh reviewer (fable). Ready to merge: with fixes. 0 Critical, 4 Important, 7 Minor.
Final: fixed preview hashed the roster after planning (race let a stale plan apply) — backend::tests::preview_taken_while_roster_changes_is_stale RED→GREEN, suite 147/147 + 43/43
Final: fixed login-shell PATH broken for fish ("$PATH" space-joined) — env::tests::login_path_works_for_fish RED→GREEN, suite 147/147
Final: fixed stale/used preview was a dead end in sheets and Add agent — PlanSheet "fetches a new preview when the roster changed…" and AddAgent "looks again after a stale apply…" RED→GREEN, 43/43
Final: fixed second launch on Windows/Linux started a second app + tray, and a hidden window on tray-less Linux was unreachable — tray::tests::closing_hides_only_where_a_tray_is_always_there RED→GREEN (close rule); single-instance via tauri-plugin-single-instance 2.5.5, suite 147/147
Final: Ruling: the single-instance half of finding 4 has no failing test — the plugin's cross-process IPC can't run in a unit test; CI compiles it on all three OSes — cost if wrong: a second instance on Windows/Linux, as before the fix.
Final: re-graded Minor→Important and fixed: TERMINAL=gnome-terminal got -e, so Sign in failed — desktop::terminal_env_naming_gnome_terminal_gets_its_own_flag RED→GREEN
Final: re-graded Minor→Important and fixed: two identical live calls threw each_key_duplicate in the side panel — Agents "two identical calls both show" RED→GREEN
Final: minor (deferred): window poll (10s) and tray thread (30s) can run overview at the same time; tray menu rebuilt on every window poll
Final: minor (deferred): tray "Open a terminal as…" drops errors (no roster binary / no terminal) silently
Final: minor (deferred): .command files in $TMPDIR/agent-roster are never cleaned up
Final: minor (deferred): Windows Terminal closes the tab when `roster login` exits, hiding its last message
Final: minor (deferred): sign-in error in a row is truncated to 120px with a tooltip; headline says "Zero accounts" for an empty roster (unreachable)
Final: Ruling (declined-to-judge list): window size, in-window sheets, limits-button placement, "Yes, undo" — prior rulings stand; Windows/Linux runtime of tray and terminals is unverified beyond CI compile + unit tests — left to the Plan 0 spike — cost if wrong: platform bugs found in the spike.
Task 12: complete — cargo test 147/147, clippy clean, npm test 43/43, svelte-check 0 errors, cargo tauri build → Agent Roster.app 13.46 MiB; roster ui opens the installed app, second roster ui keeps one process; merged to main (e09ee9d), CI run 42594072505 green on all 7 jobs.
