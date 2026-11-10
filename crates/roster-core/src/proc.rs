//! Spawning helpers shared by the CLI and the app.

use std::process::Command;

/// Stops Windows from opening a console window for a background child.
///
/// The app is a GUI program with no console, so every console program it starts
/// (`claude`, `codex`, `cmd.exe`) would otherwise get a new, visible console window
/// that flashes up and closes. Use it only for children whose output is piped or
/// discarded, never for one the person is meant to type into.
pub fn hide_console(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
