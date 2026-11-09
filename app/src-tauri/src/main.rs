#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backend;
mod commands;
mod env;

use backend::{Backend, BackendConfig};
use roster_core::desktop::find_roster_bin;
use roster_core::names::PathLookup;
use roster_core::paths::RosterPaths;
use std::process::{Command, Stdio};
use std::sync::Arc;

fn spawn_detached(argv: &[String]) -> std::io::Result<()> {
    let (prog, args) = argv
        .split_first()
        .ok_or_else(|| std::io::Error::other("empty command"))?;
    Command::new(prog)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(unix)]
fn use_login_path() {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/bin/zsh".into()
        } else {
            "/bin/bash".into()
        }
    });
    if let Some(path) = env::login_path(&shell) {
        // SAFETY: called first thing in main, before Tauri or any other thread exists.
        unsafe { std::env::set_var("PATH", path) };
    }
}

fn backend() -> Backend {
    let p = RosterPaths::detect();
    let lookup = PathLookup {
        exclude: Some(p.bin_dir()),
    };
    let roster_bin = find_roster_bin(&p, &lookup);
    Backend::new(BackendConfig {
        roster_bin,
        scratch: std::env::temp_dir().join("agent-roster"),
        path_var: std::env::var("PATH").unwrap_or_default(),
        lookup: Box::new(lookup),
        env: Box::new(|k: &str| std::env::var(k).ok()),
        opener: Box::new(spawn_detached),
        p,
    })
}

fn main() {
    #[cfg(unix)]
    use_login_path();
    tauri::Builder::default()
        .manage(Arc::new(backend()))
        .invoke_handler(tauri::generate_handler![
            commands::overview,
            commands::first_run,
            commands::preview,
            commands::apply,
            commands::changes,
            commands::undo,
            commands::undo_all,
            commands::sign_in,
            commands::open_terminal_as,
        ])
        .run(tauri::generate_context!())
        .expect("Agent Roster could not start");
}
