# agent-roster

Run several Claude Code and Codex accounts side by side on one Mac, each behind a name you choose.
Any running agent knows the roster, so "run this with `<name>`" just works.

## Install (one time)

```bash
mkdir -p ~/.local/bin
cp agent-roster ~/.local/bin/agent-roster && chmod +x ~/.local/bin/agent-roster
# make sure ~/.local/bin is on PATH (add to ~/.zshrc if not):  export PATH="$PATH:$HOME/.local/bin"
agent-roster install
```

Then open a NEW terminal and log the secondary accounts in once:

```bash
agent2          # Claude account 2 → type /login
agent4 login    # Codex account 2
```

## Pick your own names

```bash
agent-roster edit      # opens ~/.config/agent-roster/roster.conf
```

One line per account: `<name> <claude|codex> <config-dir> [# note]`

```
# name     tool    config-dir       note
mahadi     claude  ~/.claude        # default `claude`
boss       claude  ~/.claude-2
c1         codex   ~/.codex         # default `codex`
c2         codex   ~/.codex-2
```

Save, then `agent-roster install`. Old launchers are removed, new ones created, and the
roster the AI reads is rewritten. Names: letters, digits, `-` `_`, not starting with a digit,
and not clashing with an existing command.

## How it works

- Each name is a tiny script in `~/.local/bin` that sets `CLAUDE_CONFIG_DIR` (Claude) or
  `CODEX_HOME` (Codex) to that account's folder and launches the real CLI. Separate folder =
  separate login, Keychain entry, quota and history.
- Secondary folders get symlinks to the primary one's `skills`, `hooks`, `CLAUDE.md`,
  `settings.json` (Claude) and `config.toml`, `skills` (Codex), so your setup follows you.
- A roster block is written between markers into `~/.claude/CLAUDE.md` and `~/.codex/AGENTS.md`.
  It tells every agent what the names mean and how to call another one:
  Claude `<name> -p "<task>"`, Codex `<name> exec "<task>"`.
- Bare `claude` in a fresh terminal is the first Claude line; bare `codex` is the first Codex line.

## Rules of use

Multiple accounts for different purposes (personal vs. work) is fine under both Anthropic's and
OpenAI's terms. Rotating accounts to get around one workload's rate limit is not. This tool does
no rotation.

## Windows

Not supported by this script (bash + symlinks). Same idea works with PowerShell functions setting
`$env:CLAUDE_CONFIG_DIR` / `$env:CODEX_HOME`.
