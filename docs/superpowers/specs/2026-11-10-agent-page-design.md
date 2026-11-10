# Agent page, ready-made roles, "Good at", and Open in any terminal

Date: 2026-11-10. Builds on the binding spec `2026-11-09-agent-roster-design.md` and the app as it stands on branch
`ui/unslop-polish` (edit sheet, own top bar, light theme, Show usage wording).

## Why

The owner finds the current list "a little immature": you can only open an agent's settings by clicking its name, the
settings live in a pop-up, the role is a blank box, and nothing tells an agent what the other agents are good at.
Opening a terminal only uses the default terminal, and it isn't obvious that typing the agent's name in any terminal
does the same thing.

Success: a new user opens the app, clicks an agent anywhere, sees its usage and every setting on one page, picks a
role with one click, and starts the agent in the terminal app they like, or learns the one command to type.

## Goals

1. **Layout C: list on the left, the agent's page on the right.** Chosen by the owner in the visual companion
   (`.superpowers/brainstorm/63736-1794271494/content/settings-v2.html`, option C).
2. **Ready-made roles**: one click sets the role.
3. **"Good at"**: each agent carries a short list of what it is good at, filled in from its tool, editable, and written
   into the roster note every agent reads, so agents can pick who to hand work to.
4. **Open in any terminal**: choose among the terminal apps actually installed; when none can be driven, show the
   command to type, with a Copy button.
5. Make it obvious everywhere that typing the agent's name in any terminal starts it.

## Non-goals

- Self-healing (an agent fixing Agent Roster's own problems). Agreed order: roles, then "Good at", then self-healing as
  its own design later.
- Changing what a role *does*. A role is text other agents read; Leader does not get extra powers in the app.
- New colors, new tools (Gemini stays "soon").

## Anti-goals (fails even if it works)

- Any change applied without the preview-then-Save step. Every setting still goes through `preview` → token → `apply`
  in one Journal transaction and stays undoable.
- Claiming a capability the installed tool doesn't have.
- Typing into, or injecting keystrokes into, someone else's terminal app.

## The design

### 1. Layout (window 1200×660, min 1000×560)

- **Left column, 320 px, `--well` background**: header "Your agents" + count + **Add agent** (primary, small).
  One item per agent, 58 px tall: color square, name (16 px, agent color, font C), second line "Role · Tool", right side
  "67% week" with a thin bar, or "not used yet" when there are no numbers. Hot (≥90%) shows coral.
  The whole item is a button; the selected item has the raised surface and a hairline. Up/Down arrow keys move the
  selection. The list scrolls on its own when there are many agents.
  Bottom of the column, separated by a hairline: the limit warning(s), "On the line now" (the live call, or "Nobody is on
  the line"), **Refresh**, and links **How it works** · **What changed**. Last line, quiet (12 px, `--dim`): "Created by
  Mahadi Hassan · mahadihassan.com". The site part is a link to `https://mahadihassan.com`, opened in the default
  browser by the backend (a fixed URL, not one the page can pass in). The same line closes the How it works page.
- **Right: the agent's page** (scrolls on its own; the save bar never scrolls away):
  - Top: big name (34 px, agent color), meta line (tool, account email or plan, default label, status dot + text), and
    on the right the **Open in terminal** control (§4) and a **⋯** menu (Open in terminal…, Sign in again, Show/Hide
    usage, Make default, Remove…).
  - A one-line callout under the top: "Or type **agent2** in any terminal" with a **Copy** button. (Goal 5.)
  - Two usage cards side by side: 5-hour use and Weekly use (percent, bar, reset). With no numbers: "Shows after you
    use agent2" and "Type agent2 in a terminal and send one message". Claude Code with usage hidden: "Usage isn't shown
    yet" + **Show usage** button.
  - Settings, label column 84 px: **Name** (input), **Color** (swatches), **Role** (§2), **Good at** (§3), **Skills**
    (Share my skills switch), **Default** (text or **Make default for claude/codex**), **Sign in again**, **Remove** (with
    "its login folder stays" hint). Same hints as today's Edit window.
  - **Save bar** pinned to the bottom of the page, shown only while a change waits: bold "what" line, the plan's steps
    as the detail, **Cancel** and the action button (Rename / Change color / Save role / Save "Good at" / Turn on
    sharing / Make default / Remove). One change at a time, exactly like today's AgentSheet.
- First launch with agents: the first agent is selected. After Add agent: the new agent is selected. After Remove: the
  agent above it is selected. With zero agents: First run, unchanged.
- The pop-up AgentSheet goes away; its logic moves into the page. PageHead screens (Add agent, What changed, How it
  works) open in the right pane, with the list staying on the left.
- The jacks and cables are retired with this layout. Calls show in the list footer and on the calling agent's page
  ("calling agent3").

### 2. Ready-made roles

Buttons: **Leader**, **Worker 1**, **Worker 2**, **Worker 3**, **Reviewer**, **Tester**, **Designer**, **Custom…**.
Clicking one queues a role change (Save bar appears). Custom… shows the text input (prefilled with the current custom
role). The button matching the current role is shown selected. Same buttons in Add agent.
The roster note gets one extra sentence when a Leader exists: "<name> is the leader: give it big tasks; it splits them
and hands parts to the workers." (Owner said "go with your recommendations"; this is that recommendation.)

### 3. "Good at"

- Model: `Agent.good_at: Option<Vec<String>>` (serde default, skipped when None). None means "use the tool's
  defaults"; Some(list) is what the person set (an empty list is allowed and means "say nothing").
- Defaults live in roster-core, one table:
  - Claude Code: "coding", "reviewing code", "long tasks".
  - Codex: "coding", plus "making images" only when `codex features list` reports `image_generation` enabled for that
    account's config. Checked once per overview refresh at most, via the existing `CommandRunner`, never by reading
    credential files. (Verified 2026-11-10 on codex-cli 0.167.0: `image_generation  stable  true`.)
- UI: tags with an × on each and "+ add" (inline input, Enter adds). Hint: "Filled in from <Tool>. Other agents read
  it too. Edit it if you know better." A **Reset to defaults** link when Some(list) differs from the defaults.
- New op `plan_set_good_at(r, p, ctx, name, Option<Vec<String>>)`, previewed and undoable; rewrites the notes.
- Roster note table becomes `| Name | Tool | Role | Good at |`. CLI: `roster good-at <name> <items…>` and
  `roster good-at <name> --reset`, matching the other setters.

### 4. Open in terminal

- Control: a split button. Main part "Open in <Default>" runs the agent in the default terminal (today's behavior).
  The arrow opens a menu of the terminal apps found on this computer, the default first, then a line "Or copy the
  command".
- Detection (roster-core, pure functions over a lookup so it is testable):
  - macOS: look for app bundles in /Applications and ~/Applications. Known entries and how each runs a command:
    - Terminal, iTerm: open the generated `.command` file with that app (`open -a <App> file.command`). Runs it.
    - Ghostty, WezTerm, kitty, Alacritty: their documented "run this command" launch form. Runs it.
    - Warp, cmux, VS Code, Cursor, any other found terminal: **open the app and copy the command**; the page then says
      "agent2 is copied. Paste it in <App> and press Return." (We never type into another app.)
  - Windows: Windows Terminal (`wt`), PowerShell, Command Prompt run it; VS Code/Cursor open-and-copy.
  - Linux: today's list ($TERMINAL, x-terminal-emulator, gnome-terminal, konsole, xterm).
  - The exact launch form for each "runs it" entry is confirmed against that app's docs during planning; any entry that
    can't be confirmed falls back to open-and-copy.
- The choice is remembered per computer (app-local setting, not the roster) and becomes the main button's target.
- Nothing found: the control becomes "Copy command" and the callout reads "Open any terminal, type agent2 and press
  Return." The same applies to Sign in again (copy `roster login agent2`).

## Constraints

- Safety rules from the binding spec, unchanged: preview → apply → Journal → undo; no hooks; never read credential
  files; never write through a link into another account; never move or delete an adopted folder.
- Every roster-core behavior change has a test first (owner's rule).
- Copy uses plain short English, sentence case, no trailing periods on labels.
- Light and dark themes both work; Windows draws its own window buttons, Mac keeps its own.

## Decided by the owner

- Layout C (list + agent page), over A (refined rows) and B (table).
- Ready-made roles, a "Good at" line filled from the tool, Open in a chosen terminal with a copy fallback.
- Order: roles → "Good at" → self-healing later.

## My calls (owner can override)

- Role button set and the Leader sentence in the note.
- Retiring the jacks and cables with layout C.
- Default "Good at" lists above.
- Open-and-copy for apps without a documented run-a-command launch form.
- Roles are not unique: two agents may both be Leader. The role is saved as the plain label ("Leader"); a typed role
  matching a label (any case) lights that button. The note sentence lists every leader ("agent1 and agent2 are
  leaders: …").
- "Good at" defaults are fixed per tool, worked out when a plan is made, with no command run: Codex includes "making
  images" (stable and on in codex-cli 0.167.0). The app runs `codex features list` only for display, and if it reports
  image generation off for that account, the page shows a quiet "image generation is off in this Codex" next to the
  tag. The note never changes without a preview.
- The remembered terminal is an app preference (a small file in the app's data folder, written when picked), not a
  change to anyone's setup, so it skips preview, Journal and undo. Everything that touches the roster, notes or tool
  configs still goes through them.

## Left to the builder

- Component split, exact spacing within the 4 px grid, keyboard focus order, and test names.
