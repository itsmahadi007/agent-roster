## Install with one command

**Mac or Linux** (Terminal):

```sh
curl -fsSL https://raw.githubusercontent.com/itsmahadi007/agent-roster/main/install.sh | sh
```

**Windows** (PowerShell):

```powershell
irm https://raw.githubusercontent.com/itsmahadi007/agent-roster/main/install.ps1 | iex
```

The command picks the right download for your computer, installs the app and the `roster` command, and opens Agent Roster.

## Or download the app yourself

| Your computer | Download |
|---|---|
| Mac with Apple chip (M1 and later) | [Agent-Roster-mac-apple-silicon.dmg](https://github.com/itsmahadi007/agent-roster/releases/download/v{{VERSION}}/Agent-Roster-mac-apple-silicon.dmg) |
| Mac with Intel chip | [Agent-Roster-mac-intel.dmg](https://github.com/itsmahadi007/agent-roster/releases/download/v{{VERSION}}/Agent-Roster-mac-intel.dmg) |
| Windows (most PCs) | [Agent-Roster-windows-x64-setup.exe](https://github.com/itsmahadi007/agent-roster/releases/download/v{{VERSION}}/Agent-Roster-windows-x64-setup.exe) |
| Windows on ARM | [Agent-Roster-windows-arm64-setup.exe](https://github.com/itsmahadi007/agent-roster/releases/download/v{{VERSION}}/Agent-Roster-windows-arm64-setup.exe) |
| Linux (any distribution) | [Agent-Roster-linux-x64.AppImage](https://github.com/itsmahadi007/agent-roster/releases/download/v{{VERSION}}/Agent-Roster-linux-x64.AppImage) |
| Linux (Ubuntu, Debian) | [Agent-Roster-linux-x64.deb](https://github.com/itsmahadi007/agent-roster/releases/download/v{{VERSION}}/Agent-Roster-linux-x64.deb) |

Not sure which Mac you have? Apple menu > About This Mac: "Chip: Apple M…" means Apple chip, "Processor: Intel" means Intel.

**First launch of a downloaded file:** the app isn't signed by Apple or Microsoft yet.

- **Mac:** drag Agent Roster to Applications and open it. If macOS blocks it, go to System Settings > Privacy & Security and click **Open Anyway**.
- **Windows:** if SmartScreen appears, click **More info** > **Run anyway**.

The install commands above skip both steps.

## Already installed?

Agent Roster 0.2.1 and later check for new versions on their own and offer **Update now**.

## Other files

- `roster-*`: the `roster` command on its own, for use in a terminal. It already comes inside the app.
- `*.app.tar.gz`, `*.sig`, `latest.json`: used by the app's automatic updates. You don't need to download these.

Created by Mahadi Hassan · https://mahadihassan.com
