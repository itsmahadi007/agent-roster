# List-first window, weekly limit only

Date: 2026-11-10. Replaces **Layout C** (list on the left, the agent's page on the right) from
`2026-11-10-agent-page-design.md`. Everything else in that spec (ready-made roles, Good at, Open in any terminal)
stays. Visual source of truth: `docs/mockups/list-first.html`, option B "click opens the row in place", chosen by the
owner in the visual companion on 2026-11-10.

## Why

The owner noticed the list showed "% week" but not **when** the limit resets, and that knowing when an agent is
usable again matters as much as how much is used. The agent page also showed every setting all the time, which made
the window busy. The owner wants the list to be the whole window, with settings appearing only after a row is clicked.

Separately: Claude Code and Codex no longer have a 5-hour limit. Only the weekly limit matters.

Success: open the app, see every agent's weekly use and its reset time at a glance; click one agent to change it;
the other agents stay visible while you do.

## Goals

1. **One column, no right pane.** The window opens about 760 px wide (minWidth about 560). The list is centered,
   max width about 720 px.
2. **Each row**: color square, the name in its color, "role · tool · status", the weekly % ("68% week"), one bar and
   the reset time ("resets in 2d 18h"). At 90% or more the %, the bar and the reset text turn coral. Rows with no
   numbers keep today's words ("not used yet", "sign in"). A chevron on the right turns when the row is open.
3. **Row opens in place.** Nothing is open at start. Click, Enter or Space opens a drawer right under the row with
   everything the agent page had: the email / plan / "default for" / status line, Open terminal with its terminal
   picker, "type <name> in any terminal" with Copy, Show/Hide usage, Name, Color, Role, Good at, Skills (share),
   Default, Sign in again, Remove, and the same preview-then-apply save bar. One row open at a time. Clicking the
   open row, or Escape, closes it.
4. **Unsaved edits are never dropped silently.** Escape keeps today's guard: an open menu closes first; then a
   waiting change is cancelled (the drawer stays open); only an Escape with nothing waiting closes the drawer.
   Clicking the open row, or another row, while a change is waiting does not close or switch; the save bar says
   "Apply or cancel this change first."
5. Up/Down arrows move focus between rows. The open row is tracked by id, so it stays open across the 10-second poll
   and across a rename. After Add agent, the new agent's row is open. After Remove, nothing is open.
6. **Help, What changed and Add agent fill the window**, each with its existing back link. First run is unchanged. The
   notice toast is centered in the window (it was offset by the 320 px sidebar).
7. **Weekly limit only.** No 5-hour bar or card anywhere: app, tray, `roster limits`, the terminal banner. roster-core
   keeps parsing any window a tool reports (a 5-hour one too) without error; the views just show the week. The view's
   `reset_text` loses the "week " prefix. Help and the status-line preview text talk about weekly use only.
8. **Bug fix:** a reset time that has passed reads "resetting now", never "resets in resetting now", in the view,
   the CLI table and the terminal banner. One helper in `limits.rs` builds the phrase for all three.

## Non-goals

- No change to how limits are read, to the journal, to any operation, or to the tray menu's structure.
- No new settings. No change to Add agent, First run, What changed or Help beyond the wording in goal 7.

## Decisions (owner's)

- Option B over option A (a separate page with a back link).
- Weekly limit only.
- Released as v0.3.0 and installed by the owner through the app's own "Check for updates".

## Builder's calls

- Window width ~760 px with the list centered (the owner didn't pick between narrow and wide).
- The drawer drops the two usage cards: the row already shows the weekly %, bar and reset. When there are no numbers,
  the drawer shows one line saying why and what to do (the same words the cards used).
- The list footer (warnings, who is calling whom, Refresh, How it works, What changed, Updates, credit) moves under
  the list in the same column.
- The save bar sticks to the bottom of the window while its drawer is open, so Apply is always reachable.
