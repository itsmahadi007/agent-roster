#!/bin/sh
# Installs or updates Agent Roster on macOS or Linux:
#   curl -fsSL https://raw.githubusercontent.com/itsmahadi007/agent-roster/main/install.sh | sh
# It downloads the latest release, installs the app and the `roster` command, and opens the app.
set -eu

REPO="itsmahadi007/agent-roster"
BASE="https://github.com/$REPO/releases/latest/download"
BIN="$HOME/.local/bin"

say() { printf '%s\n' "$*"; }
fail() { say "Agent Roster: $*" >&2; exit 1; }

command -v curl >/dev/null 2>&1 || fail "curl is needed to download Agent Roster."
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$BIN"

case "$(uname -s)" in
  Darwin)
    case "$(uname -m)" in
      arm64) slug=mac-apple-silicon ;;
      x86_64) slug=mac-intel ;;
      *) fail "this Mac's chip ($(uname -m)) isn't supported." ;;
    esac
    say "Downloading Agent Roster for $slug..."
    curl -fL --progress-bar "$BASE/Agent-Roster-$slug.dmg" -o "$tmp/agent-roster.dmg"
    mnt="$tmp/mnt"
    mkdir -p "$mnt"
    hdiutil attach -nobrowse -readonly -mountpoint "$mnt" "$tmp/agent-roster.dmg" >/dev/null
    # Update an existing copy where it is; otherwise /Applications, or ~/Applications if that isn't writable.
    if [ -d "$HOME/Applications/Agent Roster.app" ]; then
      dest="$HOME/Applications"
    elif [ -w /Applications ]; then
      dest=/Applications
    else
      dest="$HOME/Applications"
      mkdir -p "$dest"
    fi
    app="$dest/Agent Roster.app"
    pkill -f "Agent Roster.app/Contents/MacOS/agent-roster-app" 2>/dev/null || true
    rm -rf "$app"
    cp -R "$mnt/Agent Roster.app" "$dest/"
    hdiutil detach -quiet "$mnt" || true
    xattr -dr com.apple.quarantine "$app" 2>/dev/null || true
    ln -sf "$app/Contents/MacOS/roster" "$BIN/roster"
    say "Installed $app"
    open "$app"
    ;;
  Linux)
    [ "$(uname -m)" = "x86_64" ] || fail "only 64-bit Intel/AMD Linux is supported for now."
    slug=linux-x64
    say "Downloading Agent Roster for Linux..."
    mkdir -p "$HOME/Applications"
    img="$HOME/Applications/agent-roster.AppImage"
    curl -fL --progress-bar "$BASE/Agent-Roster-$slug.AppImage" -o "$tmp/agent-roster.AppImage"
    curl -fsSL "$BASE/roster-$slug.tar.gz" -o "$tmp/roster.tar.gz"
    pkill -f "agent-roster.AppImage" 2>/dev/null || true
    mv "$tmp/agent-roster.AppImage" "$img"
    chmod +x "$img"
    tar xzf "$tmp/roster.tar.gz" -C "$BIN" roster
    chmod +x "$BIN/roster"
    apps="$HOME/.local/share/applications"
    mkdir -p "$apps"
    cat > "$apps/agent-roster.desktop" <<EOF
[Desktop Entry]
Name=Agent Roster
Comment=Name your AI coding accounts
Exec=$img
Terminal=false
Type=Application
Categories=Development;
EOF
    say "Installed $img"
    (nohup "$img" >/dev/null 2>&1 &) || true
    ;;
  *)
    fail "this script is for macOS and Linux. On Windows, run in PowerShell: irm https://raw.githubusercontent.com/$REPO/main/install.ps1 | iex"
    ;;
esac

case ":$PATH:" in
  *":$BIN:"*) say "The roster command is ready: type roster in a new terminal." ;;
  *) say "The roster command is in $BIN. Add it to your PATH to use roster in a terminal." ;;
esac
say "Agent Roster is open. It checks for updates by itself."
