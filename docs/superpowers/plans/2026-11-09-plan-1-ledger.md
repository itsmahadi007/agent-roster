# SDD ledger — plan: docs/superpowers/plans/2026-11-09-plan-1-core-cli.md
Spec: docs/superpowers/specs/2026-11-09-agent-roster-design.md
Setup: Ruling: work on branch feat/core-cli in place, no separate worktree — repo is brand new with only docs on main, a branch isolates fully — cost if wrong: none, main untouched.
Pre-flight:
- T4 produces Txn{write_file,remove_file,ensure_dir,link_dir,replace_link_with_copy}; T5/T6/T10/T12 consume the same names — consistent.
- T7 sign_in_for consumes run::build_env, but build_env is produced in T9 — forward dependency. Ruling: create run.rs with build_env in T7 (with its T9 tests), T9 adds run_agent/session — cost if wrong: none, same code earlier.
- T7 produces CommandRunner; T11 handle_statusline consumes it — consistent.
- T8 read_claude_limits reads limits_dir/<id>.json; T11 writes it — format fixed in T11: {"at":i64,"windows":[{"label","used_percent","resets_at"}]} written via AgentLimits serde — consistent once both use AgentLimits.
- T10 apply consumes write_launchers(T6), write_notes(T5), apply_share(T10 own) — consistent.
Task 1: complete (commits 5200252..1662662, tests: cargo test -p roster-core paths → test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s)
Task 2: complete (commits 1662662..7e2c794, tests: cargo test -p roster-core → test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s)
Task 3: complete (commits 7e2c794..2169448, tests: cargo test -p roster-core --lib → test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s)
Task 4: complete (commits 2169448..b0275ea, tests: cargo test -p roster-core --lib → test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 4: Ruling: added write_file refusing symlinked paths (IsLink) and LinkRemoved/LinkReplaced change kinds beyond the plan's list — needed so writes never follow a link into the primary's files (Review Focus 1) — cost if wrong: callers must replace links first, which they do
Task 5: complete (commits b0275ea..39c868e, tests: cargo test -p roster-core --lib → test result: ok. 36 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 5: Ruling: render_note takes (roster, me) not (roster, paths) and each note says "You are `<name>`" — each agent should know its own identity to delegate correctly — cost if wrong: one parameter
Task 6: complete (commits 39c868e..9812670, tests: cargo test -p roster-core --lib → test result: ok. 43 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 6: Ruling: path_setup_needed returns a PathSetup value (rc files + Windows PATH) instead of Vec<Step> — Step lives in ops.rs (Task 10), which turns PathSetup into preview steps; avoids a module cycle — cost if wrong: none
Task 7: complete (commits 9812670..32fd03a, tests: cargo test -p roster-core --lib → test result: ok. 52 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 7: Ruling: sign_in_for takes the resolved tool path as a parameter (plan had none) — keeps PATH lookup out of a function tests call with fakes — cost if wrong: one argument
Task 8: complete (commits 32fd03a..ed578ed, tests: cargo test -p roster-core --lib → test result: ok. 58 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 9: complete (commits ed578ed..377b09a, tests: cargo test -p roster-core --lib → test result: ok. 63 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 9: Ruling: split run_agent into prepare_run (pure, unit-tested) + exec_run; on macOS/Linux exec_run replaces the process (same pid) instead of spawn+wait+signal handling — exec keeps job control and Ctrl-C exactly like running the tool directly, and the pid-keyed record dies with the tool — cost if wrong: a record lingers until the next live check deletes it.
Task 9: Ruling: the end-to-end "piped output has no banner, exit code passes through" test moves to Task 13 (assert_cmd on the real binary), because exec cannot run inside a unit test process — cost if wrong: none, covered there.
Task 10: complete (commits 377b09a..df94c3c, tests: cargo test -p roster-core --test ops → test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s)
Task 10: Ruling: added plan_adopt (puts ~/.claude or ~/.codex on the roster in place) — spec §6.1 first run needs it and no task named it — cost if wrong: none.
Task 10: Ruling: plan_* take a Ctx {lookup, path_var, roster_bin} instead of separate lookup params; apply returns Option<u64> (None when nothing changed) — keeps PATH detection testable and avoids empty journal entries — cost if wrong: signature only.
Task 10: Ruling: deleting a login folder moves it into the journal backups (new Change::DirMoved) instead of deleting — keeps "every change can be undone" true even for remove --delete-login-folder — cost if wrong: disk space until uninstall.
Task 11: complete (commits df94c3c..a0ea349, tests: cargo test -p roster-core --test statusline → test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 11: Ruling: handle_statusline takes a shell closure (cmd, stdin) -> stdout instead of CommandRunner, since the chained status line needs stdin; real_shell is the production closure — cost if wrong: none
Task 12: complete (commits a0ea349..f27dd6f, tests: cargo test -p roster-core --test import → test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s)
Task 12: Ruling: a linked CLAUDE.md/AGENTS.md in a prototype folder is removed (undoable) rather than replaced with a copy, because sharing then adds an @import of the main file — a copy would duplicate the owner's global rules — cost if wrong: the file starts with only the roster note and import line.
Task 12: Ruling: prototype notes like "default `claude`" are not turned into roles — they are annotations, not roles — cost if wrong: owner sets roles with `roster role`.
Task 12: found and fixed a journal bug (undo pre-check ignored files freed earlier in the same undo) — test import::old_launchers_removed_with_backup_and_undo_restores_them RED→GREEN, plus journal::more_tests::undo_remove_then_recreate_in_one_op.
Task 13: complete (commits f27dd6f..9707333, tests: cargo test --workspace → test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s)
Task 13: Ruling: added `roster init` (adopts installed tools as claude1/codex1, names overridable) and `undo --last` — first run needs a way to adopt existing logins, and --last is the common undo — cost if wrong: extra commands.
Task 13: Ruling: overview gathering lives in a new core module status.rs (parallel sign-in checks) so the app (Plan 2) reuses it — cost if wrong: none.
Task 13: Ruling: agent states are ready / sign in / working / not installed; the mockup's "idle" is folded into "ready" — no reliable signal separates them — cost if wrong: one label.
Task 13: Ruling: change history shows relative ages ("12m ago") instead of clock times — std has no local timezone, and a tz crate isn't worth it for v0.1 — cost if wrong: cosmetic.
Task 14: partial — CI workflow committed; Linux suite passed in Docker (rust:1, non-root); release binary built (2.2 MB) and installed to ~/.local/bin/roster; import preview printed correctly. Applying the import was blocked by the session's auto-mode safety check (edits shell rc + global CLAUDE.md/AGENTS.md); left for the owner to run. Steps 3-5 pending.
Final review: fresh reviewer (fable). Ready to merge: with fixes. 1 Critical, 7 Important, 7 Minor.
Final: fixed Windows user PATH written as REG_SZ — journal::review_tests::user_path_keeps_expand_type (cfg(windows); runs in CI, compile-verified locally), suite 104/104
Final: fixed launchers baking in the canonicalized binary path — cli::launchers_use_the_path_roster_was_started_from RED→GREEN, suite 104/104
Final: fixed commit marking committed before append — journal::review_tests::failed_append_rolls_back_changes RED→GREEN, suite 104/104
Final: fixed non-LIFO undo blaming the user — journal::review_tests::undo_older_op_names_the_later_change RED→GREEN, suite 104/104
Final: fixed dotfile symlinks replaced by copies — ops::dotfile_links_are_written_through_not_replaced RED→GREEN, suite 104/104
Final: fixed overview hang on a stuck tool — detect::timeout_tests::stuck_command_times_out RED→GREEN, suite 104/104
Final: fixed partial undo failure wedging the op — journal::review_tests::partial_undo_failure_can_be_retried RED→GREEN, suite 104/104
Final: fixed import preview omitting the instruction-link replacement — import::preview_names_the_replaced_instruction_link RED→GREEN, suite 104/104
Final: minor (deferred): roster.toml written with plain fs::write inside the journal, not tmp+rename
Final: minor (deferred): names that collide with shell builtins (test, time, cd) pass validation
Final: minor (deferred): session records linger after exec; pid reuse can show a false "working"
Final: minor (deferred): Windows PowerShell profile path assumes PS7 and non-OneDrive Documents; PATH dedupe case-sensitive; LinkRemoved undo may yield a copy; exit codes clamped to 255
Final: minor (deferred): plan_remove leaves share symlinks in the kept folder
Final: minor (deferred): fish default-function file left empty instead of removed
Final: minor (deferred): `roster statusline` prints nothing if roster.toml fails to load
Final: Ruling: per-change undo is effectively newest-first when changes share files (every op rewrites roster.toml and notes); the error now names which change to undo first — cost if wrong: users undo in order, which is the safe order anyway.
Task 14: complete — owner ran roster import (change 1); live: roster overview shows agent1-4, agent3 week 94%; agent1 -p → OK1, agent3 exec → OK3; CI green on macOS/Ubuntu/Windows/musl (run 42586919676).
