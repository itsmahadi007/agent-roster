# Agent Roster

Give each of your AI coding accounts a name you choose (`boss`, `worker1`, `agent3`). Type that
name in any terminal to open Claude Code or Codex on that account, see every account's login,
limits and reset times in one place, and let any agent hand work to another by name. Works on
macOS, Windows and Linux. Never installs hooks, previews every change, and can undo all of them.

## Install

**Mac or Linux** (Terminal):

```sh
curl -fsSL https://raw.githubusercontent.com/itsmahadi007/agent-roster/main/install.sh | sh
```

**Windows** (PowerShell):

```powershell
irm https://raw.githubusercontent.com/itsmahadi007/agent-roster/main/install.ps1 | iex
```

The command picks the right download for your computer, installs the app and the `roster` command,
and opens Agent Roster. Run it again any time to reinstall.

Or download the file yourself from [Releases](https://github.com/itsmahadi007/agent-roster/releases/latest):
Mac Apple chip or Intel (`.dmg`), Windows x64 or ARM (`-setup.exe`), Linux (`.AppImage` or `.deb`).
The apps aren't signed by Apple or Microsoft yet: on a Mac use System Settings > Privacy & Security >
**Open Anyway** the first time; on Windows click **More info** > **Run anyway**. The install commands
skip this.

Once installed, Agent Roster checks for new versions by itself and offers **Update now**. You need
Claude Code or Codex installed and signed in once.

Created by Mahadi Hassan · [mahadihassan.com](https://mahadihassan.com)

Design: `docs/superpowers/specs/2026-11-09-agent-roster-design.md`.
