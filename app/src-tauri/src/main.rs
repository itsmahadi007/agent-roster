#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backend;
mod commands;
mod env;
mod login_window;
mod sign_in;
mod tray;

use backend::{Backend, BackendConfig};
use roster_core::desktop::{bundled_roster, find_roster_bin};
use roster_core::names::PathLookup;
use roster_core::paths::RosterPaths;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tauri::{Emitter, Manager};

fn spawn_detached(argv: &[String]) -> std::io::Result<()> {
    let (prog, args) = argv
        .split_first()
        .ok_or_else(|| std::io::Error::other("empty command"))?;
    let mut c = Command::new(prog);
    // Windows openers run through `cmd.exe /c`: hide that cmd.exe. A terminal opened
    // with `start` still gets its own window.
    if is_cmd_c(prog, args) {
        roster_core::proc::hide_console(&mut c);
    }
    c.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

fn is_cmd_c(prog: &str, args: &[String]) -> bool {
    let name = std::path::Path::new(prog)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    name.eq_ignore_ascii_case("cmd.exe")
        && args.first().is_some_and(|a| a.eq_ignore_ascii_case("/c"))
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
    // The user's own `roster` first; otherwise the copy shipped inside this app.
    let roster_bin = find_roster_bin(&p, &lookup).or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|exe| bundled_roster(p.platform, &exe))
    });
    Backend::new(BackendConfig {
        roster_bin,
        scratch: std::env::temp_dir().join("agent-roster"),
        app_dirs: vec![
            PathBuf::from("/Applications"),
            p.home.join("Applications"),
            PathBuf::from("/System/Applications/Utilities"),
        ],
        path_var: std::env::var("PATH").unwrap_or_default(),
        lookup: Box::new(lookup),
        env: Box::new(|k: &str| std::env::var(k).ok()),
        opener: Box::new(spawn_detached),
        login: sign_in::real_starter(),
        p,
    })
}

/// Asks every tool for usage at start, every 5 minutes, soon after a row shows "checking"
/// (an agent never checked, or one seen signing in since its last check), and, for up to
/// 5 minutes after Sign in is clicked, until a check says that sign-in worked. One finished
/// check ends "checking", so nothing here repeats on its own. Runs on its own thread, never
/// the UI's; the window hears "usage-checked" when fresh answers are saved.
fn usage_loop(app: tauri::AppHandle, b: Arc<Backend>) {
    std::thread::spawn(move || {
        let mut next = std::time::Instant::now();
        loop {
            let due = std::time::Instant::now() >= next;
            if due {
                next = std::time::Instant::now() + backend::USAGE_EVERY;
            }
            if (due || b.take_wanted()) && b.refresh_usage().unwrap_or(false) {
                if let Ok(v) = b.overview() {
                    tray::update(&app, &v);
                }
                let _ = app.emit("usage-checked", ());
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
}

fn main() {
    #[cfg(unix)]
    use_login_path();
    let app = tauri::Builder::default()
        // A second launch (`roster ui`, Start menu, app menu) shows the running window instead.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app);
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(Arc::new(backend()))
        .setup(|app| {
            tray::setup(app.handle())?;
            std::thread::spawn(login_window::clean_old_data);
            app.state::<Arc<Backend>>()
                .set_login_ui(Arc::new(login_window::TauriLoginUi::new(
                    app.handle().clone(),
                )));
            usage_loop(
                app.handle().clone(),
                Arc::clone(app.state::<Arc<Backend>>().inner()),
            );
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the main window keeps Agent Roster in the menu bar / tray (not on
            // Linux). A login window really closes, which cancels its login.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && tray::hides_window_on_close(
                    window.label(),
                    roster_core::paths::Platform::current(),
                )
            {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::overview,
            commands::refresh_usage,
            commands::first_run,
            commands::preview,
            commands::apply,
            commands::changes,
            commands::undo,
            commands::undo_all,
            commands::sign_in,
            commands::sign_in_in_app,
            commands::settings,
            commands::set_sign_in_with,
            commands::open_terminal_as,
            commands::terminals,
            commands::open_site,
        ])
        .build(tauri::generate_context!())
        .expect("Agent Roster could not start");
    app.run(|handle, event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            tray::show_main(handle);
        }
        let _ = (handle, event);
    });
}

#[cfg(test)]
mod window_config_tests {
    /// Every platform's config opens the list-first window at the same size.
    #[test]
    fn every_platform_opens_the_list_first_window() {
        for (name, text) in [
            ("tauri.conf.json", include_str!("../tauri.conf.json")),
            (
                "tauri.macos.conf.json",
                include_str!("../tauri.macos.conf.json"),
            ),
            (
                "tauri.windows.conf.json",
                include_str!("../tauri.windows.conf.json"),
            ),
        ] {
            let v: serde_json::Value = serde_json::from_str(text).unwrap();
            let w = &v["app"]["windows"][0];
            assert_eq!(w["width"], 760, "{name}");
            assert_eq!(w["height"], 720, "{name}");
            assert_eq!(w["minWidth"], 560, "{name}");
        }
    }
}
