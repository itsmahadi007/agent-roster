# Agent Roster

Give each of your AI coding accounts a name you choose (`boss`, `worker1`, `agent3`). Type that
name in any terminal to open Claude Code or Codex on that account, see every account's login,
limits and reset times in one place, and let any agent hand work to another by name. Works on
macOS, Windows and Linux. Never installs hooks, previews every change, and can undo all of them.

## Download

Get the latest version from [Releases](https://github.com/itsmahadi007/agent-roster/releases/latest).

- **Mac:** download `Agent Roster_<version>_universal.dmg`, open it and drag Agent Roster to
  Applications. The app isn't signed by Apple yet, so the first time macOS blocks it: open
  System Settings > Privacy & Security, scroll down, and click **Open Anyway** next to Agent
  Roster. Or run `xattr -dr com.apple.quarantine "/Applications/Agent Roster.app"` once.
- **Windows:** download `Agent Roster_<version>_x64-setup.exe` and run it. Windows SmartScreen may
  say it protected your PC: click **More info**, then **Run anyway**.

The `roster` command comes inside the app, so there's nothing else to install. You need Claude
Code or Codex installed and signed in once.

To use `roster` in a terminal too, download the plain `roster` file for your system from the same
release and put it somewhere on your PATH.

Created by Mahadi Hassan · [mahadihassan.com](https://mahadihassan.com)

Design: `docs/superpowers/specs/2026-11-09-agent-roster-design.md`.
