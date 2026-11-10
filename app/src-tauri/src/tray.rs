//! Menu bar (macOS) and tray (Windows, Linux): every agent with its week limit, and a quick
//! way to open a terminal as any of them.

use crate::backend::Backend;
use roster_core::view::OverviewView;
use std::sync::Arc;
use tauri::image::Image;
use tauri::menu::{IconMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Runtime};

const TRAY_ID: &str = "main";
const DOT: u32 = 32;

#[derive(Debug, Clone, PartialEq)]
pub struct TrayEntry {
    pub id: String,
    pub text: String,
    pub rgb: (u8, u8, u8),
}

fn hex_rgb(hex: &str) -> (u8, u8, u8) {
    let h = hex.trim_start_matches('#');
    let v = |i: usize| {
        h.get(i..i + 2)
            .and_then(|s| u8::from_str_radix(s, 16).ok())
            .unwrap_or(0x88)
    };
    (v(0), v(2), v(4))
}

/// The window shows the stored colors with a palette that has no warning hues
/// (app/src/lib/colors.ts); the tray dots follow it so both agree.
fn shown_hex(hex: &str) -> &str {
    match hex.to_ascii_lowercase().as_str() {
        "#4d7cff" => "#4f7cff",
        "#f0b429" => "#3cc7e6",
        "#2fbf8f" => "#2ec4a0",
        "#c06bf0" => "#b86bff",
        "#ff7a6b" => "#ff6bc4",
        _ => hex,
    }
}

pub fn tray_entries(v: &OverviewView) -> Vec<TrayEntry> {
    v.agents
        .iter()
        .map(|a| TrayEntry {
            id: format!("agent:{}", a.id),
            text: if a.tray_text.is_empty() {
                a.name.clone()
            } else {
                format!("{}  —  {}", a.name, a.tray_text)
            },
            rgb: hex_rgb(shown_hex(&a.color)),
        })
        .collect()
}

/// A filled circle with soft edges, transparent around it.
pub fn dot_rgba(rgb: (u8, u8, u8), size: u32) -> Vec<u8> {
    let mut px = Vec::with_capacity((size * size * 4) as usize);
    let c = size as f32 / 2.0;
    let r = size as f32 * 0.32;
    for y in 0..size {
        for x in 0..size {
            let d = ((x as f32 + 0.5 - c).powi(2) + (y as f32 + 0.5 - c).powi(2)).sqrt();
            let a = (r + 0.5 - d).clamp(0.0, 1.0);
            px.extend_from_slice(&[rgb.0, rgb.1, rgb.2, (a * 255.0).round() as u8]);
        }
    }
    px
}

pub fn build_menu<R: Runtime>(app: &AppHandle<R>, v: &OverviewView) -> tauri::Result<Menu<R>> {
    let mut agents: Vec<Box<dyn IsMenuItem<R>>> = vec![];
    for e in tray_entries(v) {
        let icon = Image::new_owned(dot_rgba(e.rgb, DOT), DOT, DOT);
        agents.push(Box::new(IconMenuItem::with_id(
            app,
            &e.id,
            &e.text,
            true,
            Some(icon),
            None::<&str>,
        )?));
    }
    let terms: Vec<MenuItem<R>> = v
        .agents
        .iter()
        .map(|a| MenuItem::with_id(app, format!("term:{}", a.name), &a.name, true, None::<&str>))
        .collect::<tauri::Result<_>>()?;
    let term_refs: Vec<&dyn IsMenuItem<R>> =
        terms.iter().map(|m| m as &dyn IsMenuItem<R>).collect();
    let open_as = Submenu::with_id_and_items(
        app,
        "open-as",
        "Open a terminal as…",
        !terms.is_empty(),
        &term_refs,
    )?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let add = MenuItem::with_id(app, "add", "Add agent", true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open Agent Roster", true, None::<&str>)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Agent Roster", true, None::<&str>)?;
    let mut items: Vec<&dyn IsMenuItem<R>> = agents.iter().map(|b| b.as_ref()).collect();
    if !items.is_empty() {
        items.push(&sep1);
    }
    items.extend([&open_as as &dyn IsMenuItem<R>, &add, &open, &sep2, &quit]);
    Menu::with_items(app, &items)
}

/// Closing the window keeps the app in the menu bar / tray where a tray is always shown.
/// Linux desktops may show no tray (stock GNOME), so there closing quits.
pub fn hides_on_close(platform: roster_core::paths::Platform) -> bool {
    platform != roster_core::paths::Platform::Linux
}

/// Only the main window hides on close; a login window is really closed (which cancels its
/// login) and so is anything else.
pub fn hides_window_on_close(label: &str, platform: roster_core::paths::Platform) -> bool {
    label == "main" && hides_on_close(platform)
}

pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn on_menu<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match id {
        "quit" => app.exit(0),
        "add" => {
            show_main(app);
            let _ = app.emit_to("main", "navigate", "add");
        }
        "open" => show_main(app),
        _ if id.starts_with("agent:") => show_main(app),
        _ => {
            if let Some(name) = id.strip_prefix("term:") {
                let b = app.state::<Arc<Backend>>().inner().clone();
                let name = name.to_string();
                std::thread::spawn(move || {
                    let _ = b.open_terminal_as(&name, None);
                });
            }
        }
    }
}

/// Rebuilds the tray menu from a fresh view.
pub fn update<R: Runtime>(app: &AppHandle<R>, v: &OverviewView) {
    if let Some(tray) = app.tray_by_id(TRAY_ID)
        && let Ok(menu) = build_menu(app, v)
    {
        let _ = tray.set_menu(Some(menu));
    }
}

pub fn setup<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let empty = Menu::with_items(app, &[])?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Agent Roster")
        .menu(&empty)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, e| on_menu(app, e.id().as_ref()));
    if cfg!(target_os = "macos") {
        builder = builder
            .icon(Image::from_bytes(include_bytes!(
                "../icons/tray-template.png"
            ))?)
            .icon_as_template(true);
    } else if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    let handle = app.clone();
    std::thread::spawn(move || {
        loop {
            let b = handle.state::<Arc<Backend>>().inner().clone();
            if let Ok(v) = b.overview() {
                update(&handle, &v);
            }
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use roster_core::view::OverviewView;

    fn fixture() -> OverviewView {
        serde_json::from_str(include_str!("../../src/lib/fixtures/overview.json")).unwrap()
    }

    #[test]
    fn tray_entries_show_week_percent() {
        let e = tray_entries(&fixture());
        let texts: Vec<&str> = e.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "agent1  —  61% week",
                "agent2  —  sign in",
                "agent3  —  93% week",
                "boss  —  24% week"
            ]
        );
        assert_eq!(e[0].id, "agent:a1");
        assert_eq!(e[0].rgb, (0x4f, 0x7c, 0xff));
    }

    #[test]
    fn entry_without_tray_text_is_just_the_name() {
        let mut v = fixture();
        v.agents[3].tray_text.clear();
        assert_eq!(tray_entries(&v)[3].text, "boss");
    }

    #[test]
    fn closing_hides_only_where_a_tray_is_always_there() {
        use roster_core::paths::Platform;
        assert!(hides_on_close(Platform::Mac));
        assert!(hides_on_close(Platform::Windows));
        // Stock GNOME shows no tray icons, so a hidden window could not be brought back.
        assert!(!hides_on_close(Platform::Linux));
    }

    #[test]
    fn only_the_main_window_hides_on_close() {
        use roster_core::paths::Platform;
        assert!(hides_window_on_close("main", Platform::Windows));
        assert!(hides_window_on_close("main", Platform::Mac));
        assert!(!hides_window_on_close("main", Platform::Linux));
        assert!(!hides_window_on_close("login-1", Platform::Windows));
        assert!(!hides_window_on_close("login-1", Platform::Mac));
    }

    #[test]
    fn tray_dots_use_the_shown_palette() {
        assert_eq!(hex_rgb(shown_hex("#ff7a6b")), (0xff, 0x6b, 0xc4));
        assert_eq!(hex_rgb(shown_hex("#F0B429")), (0x3c, 0xc7, 0xe6));
        assert_eq!(shown_hex("#123456"), "#123456");
    }

    #[test]
    fn dot_is_colored_in_the_middle_and_clear_in_the_corner() {
        let px = dot_rgba((0x2f, 0xbf, 0x8f), 32);
        assert_eq!(px.len(), 32 * 32 * 4);
        let at = |x: usize, y: usize| &px[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4];
        assert_eq!(at(16, 16), &[0x2f, 0xbf, 0x8f, 255]);
        assert_eq!(at(0, 0)[3], 0);
        assert_eq!(at(31, 31)[3], 0);
    }
}
