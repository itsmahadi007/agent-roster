use roster_core::desktop::{
    DesktopError, TerminalApp, app_candidates, app_launch_command, app_open_command,
    bundled_roster, find_app, find_roster_bin, find_terminals, has_desktop,
    is_temporary_app_location, terminal_command, terminal_command_in, url_open_command,
};
use roster_core::names::CommandLookup;
use roster_core::paths::{Platform, RosterPaths};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

struct FakeLookup(HashMap<&'static str, PathBuf>);
impl CommandLookup for FakeLookup {
    fn find(&self, name: &str) -> Option<PathBuf> {
        self.0.get(name).cloned()
    }
}
fn none() -> FakeLookup {
    FakeLookup(HashMap::new())
}
fn no_env(_: &str) -> Option<String> {
    None
}
fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn touch(p: &Path) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, "x").unwrap();
}

#[test]
fn finds_roster_in_home_candidates() {
    let d = tempfile::tempdir().unwrap();
    let p = RosterPaths::for_home(d.path(), Platform::Mac, &no_env);
    assert_eq!(find_roster_bin(&p, &none()), None);
    let bin = d.path().join(".local/bin/roster");
    touch(&bin);
    assert_eq!(find_roster_bin(&p, &none()), Some(bin));
}

#[test]
fn prefers_roster_on_path() {
    let d = tempfile::tempdir().unwrap();
    let p = RosterPaths::for_home(d.path(), Platform::Mac, &no_env);
    touch(&d.path().join(".local/bin/roster"));
    let on_path = PathBuf::from("/opt/homebrew/bin/roster");
    let lookup = FakeLookup(HashMap::from([("roster", on_path.clone())]));
    assert_eq!(find_roster_bin(&p, &lookup), Some(on_path));
}

#[test]
fn roster_app_env_comes_first() {
    let d = tempfile::tempdir().unwrap();
    let p = RosterPaths::for_home(d.path(), Platform::Mac, &no_env);
    let fake = d.path().join("fake-app");
    let env = |k: &str| (k == "ROSTER_APP").then(|| fake.display().to_string());
    let c = app_candidates(&p, &env);
    assert_eq!(c[0], fake);
    assert!(c.contains(&d.path().join("Applications/Agent Roster.app")));
    assert!(c.contains(&PathBuf::from("/Applications/Agent Roster.app")));
    assert_eq!(find_app(&p, &env), None);
    touch(&fake);
    assert_eq!(find_app(&p, &env), Some(fake));
}

#[test]
fn app_candidates_per_platform() {
    let d = tempfile::tempdir().unwrap();
    let env = |k: &str| match k {
        "LOCALAPPDATA" => Some("C:\\Users\\you\\AppData\\Local".to_string()),
        "ProgramFiles" => Some("C:\\Program Files".to_string()),
        _ => None,
    };
    let w = RosterPaths::for_home(d.path(), Platform::Windows, &env);
    let c = app_candidates(&w, &env);
    assert!(
        c.contains(
            &PathBuf::from("C:\\Users\\you\\AppData\\Local")
                .join("Agent Roster")
                .join("agent-roster-app.exe")
        )
    );
    let l = RosterPaths::for_home(d.path(), Platform::Linux, &no_env);
    let c = app_candidates(&l, &no_env);
    assert!(c.contains(&PathBuf::from("/usr/bin/agent-roster-app")));
}

#[test]
fn mac_app_opens_with_open() {
    assert_eq!(
        app_launch_command(Platform::Mac, Path::new("/Applications/Agent Roster.app")),
        s(&["open", "/Applications/Agent Roster.app"])
    );
    assert_eq!(
        app_launch_command(Platform::Linux, Path::new("/usr/bin/agent-roster-app")),
        s(&["/usr/bin/agent-roster-app"])
    );
}

#[test]
fn mac_command_file_quotes_paths_with_spaces() {
    let d = tempfile::tempdir().unwrap();
    let argv = s(&["/Users/Jo Smith/.local/bin/roster", "login", "it's"]);
    let cmd = terminal_command(Platform::Mac, &no_env, &none(), &argv, d.path()).unwrap();
    assert_eq!(cmd[0], "open");
    assert_eq!(cmd.len(), 2);
    let file = PathBuf::from(&cmd[1]);
    assert!(file.starts_with(d.path()));
    assert!(file.extension().is_some_and(|e| e == "command"));
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.starts_with("#!/bin/sh\n"));
    assert!(
        text.contains("exec '/Users/Jo Smith/.local/bin/roster' 'login' 'it'\\''s'"),
        "{text}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111);
    }
    let again = terminal_command(Platform::Mac, &no_env, &none(), &argv, d.path()).unwrap();
    assert_ne!(again[1], cmd[1], "each call gets its own file");
}

#[test]
fn windows_prefers_windows_terminal_then_cmd() {
    let d = tempfile::tempdir().unwrap();
    let argv = s(&["C:\\roster.exe", "login", "agent2"]);
    let wt = PathBuf::from("C:\\wt.exe");
    let lookup = FakeLookup(HashMap::from([("wt", wt.clone())]));
    let cmd = terminal_command(Platform::Windows, &no_env, &lookup, &argv, d.path()).unwrap();
    assert_eq!(cmd, s(&["C:\\wt.exe", "C:\\roster.exe", "login", "agent2"]));
    let cmd = terminal_command(Platform::Windows, &no_env, &none(), &argv, d.path()).unwrap();
    assert_eq!(
        cmd,
        s(&[
            "cmd.exe",
            "/c",
            "start",
            "",
            "cmd.exe",
            "/k",
            "C:\\roster.exe",
            "login",
            "agent2"
        ])
    );
}

#[test]
fn linux_uses_terminal_env_then_fallbacks() {
    let d = tempfile::tempdir().unwrap();
    let argv = s(&["/usr/bin/roster", "login", "agent2"]);
    let env = |k: &str| (k == "TERMINAL").then(|| "alacritty".to_string());
    let cmd = terminal_command(Platform::Linux, &env, &none(), &argv, d.path()).unwrap();
    assert_eq!(
        cmd,
        s(&["alacritty", "-e", "/usr/bin/roster", "login", "agent2"])
    );
    let gnome = FakeLookup(HashMap::from([(
        "gnome-terminal",
        PathBuf::from("/usr/bin/gnome-terminal"),
    )]));
    let cmd = terminal_command(Platform::Linux, &no_env, &gnome, &argv, d.path()).unwrap();
    assert_eq!(
        cmd,
        s(&[
            "/usr/bin/gnome-terminal",
            "--",
            "/usr/bin/roster",
            "login",
            "agent2"
        ])
    );
    let both = FakeLookup(HashMap::from([
        ("gnome-terminal", PathBuf::from("/usr/bin/gnome-terminal")),
        (
            "x-terminal-emulator",
            PathBuf::from("/usr/bin/x-terminal-emulator"),
        ),
    ]));
    let cmd = terminal_command(Platform::Linux, &no_env, &both, &argv, d.path()).unwrap();
    assert_eq!(cmd[0], "/usr/bin/x-terminal-emulator");
    assert_eq!(cmd[1], "-e");
}

#[test]
fn linux_without_terminal_errors() {
    let d = tempfile::tempdir().unwrap();
    let argv = s(&["/usr/bin/roster", "login", "agent2"]);
    let err = terminal_command(Platform::Linux, &no_env, &none(), &argv, d.path()).unwrap_err();
    assert!(matches!(err, DesktopError::NoTerminal));
    assert_eq!(
        err.to_string(),
        "No terminal app was found. Set TERMINAL to your terminal's command."
    );
}

#[test]
fn linux_without_display_has_no_desktop() {
    assert!(!has_desktop(Platform::Linux, &no_env));
    assert!(has_desktop(Platform::Linux, &|k: &str| {
        (k == "WAYLAND_DISPLAY").then(|| "wayland-0".to_string())
    }));
    assert!(!has_desktop(Platform::Linux, &|k: &str| {
        (k == "DISPLAY").then(String::new)
    }));
    assert!(has_desktop(Platform::Mac, &no_env));
    assert!(has_desktop(Platform::Windows, &no_env));
}

#[test]
fn terminal_env_naming_gnome_terminal_gets_its_own_flag() {
    let d = tempfile::tempdir().unwrap();
    let argv = s(&["/usr/bin/roster", "login", "agent2"]);
    let env = |k: &str| (k == "TERMINAL").then(|| "/usr/bin/gnome-terminal".to_string());
    let cmd = terminal_command(Platform::Linux, &env, &none(), &argv, d.path()).unwrap();
    assert_eq!(
        cmd,
        s(&[
            "/usr/bin/gnome-terminal",
            "--",
            "/usr/bin/roster",
            "login",
            "agent2"
        ])
    );
}

fn t(id: &str, label: &str, runs: bool) -> TerminalApp {
    TerminalApp {
        id: id.into(),
        label: label.into(),
        runs,
    }
}

#[test]
fn finds_installed_mac_terminals_in_order() {
    let have = [
        "Terminal.app",
        "Warp.app",
        "cmux.app",
        "Visual Studio Code.app",
    ];
    let found = find_terminals(Platform::Mac, &|b| have.contains(&b), &none());
    assert_eq!(
        found,
        [
            t("Terminal", "Terminal", true),
            t("Warp", "Warp", false),
            t("cmux", "cmux", false),
            t("Visual Studio Code", "VS Code", false),
        ]
    );
}

#[test]
fn iterm_runs_the_command_file() {
    let d = tempfile::tempdir().unwrap();
    let argv = s(&["/usr/local/bin/roster", "run", "a1"]);
    let cmd = terminal_command_in(
        Platform::Mac,
        Some("iTerm"),
        &no_env,
        &none(),
        &argv,
        d.path(),
    )
    .unwrap();
    assert_eq!(cmd[..3], s(&["open", "-a", "iTerm"]));
    assert!(cmd[3].ends_with(".command"));
    let def = terminal_command_in(Platform::Mac, None, &no_env, &none(), &argv, d.path()).unwrap();
    assert_eq!(def[0], "open");
    assert_eq!(def.len(), 2);
}

#[test]
fn windows_always_has_powershell_and_cmd() {
    let lookup = FakeLookup(HashMap::from([
        ("wt", PathBuf::from("C:\\wt.exe")),
        ("code", PathBuf::from("C:\\code.cmd")),
    ]));
    let found = find_terminals(Platform::Windows, &|_| false, &lookup);
    assert_eq!(
        found,
        [
            t("wt", "Windows Terminal", true),
            t("powershell", "PowerShell", true),
            t("cmd", "Command Prompt", true),
            t("code", "VS Code", false),
        ]
    );
    let d = tempfile::tempdir().unwrap();
    let argv = s(&["C:\\Roster Files\\roster.exe", "run", "a1"]);
    let ps = terminal_command_in(
        Platform::Windows,
        Some("powershell"),
        &no_env,
        &lookup,
        &argv,
        d.path(),
    )
    .unwrap();
    assert_eq!(
        ps,
        s(&[
            "cmd.exe",
            "/c",
            "start",
            "",
            "powershell.exe",
            "-NoExit",
            "-Command",
            "& 'C:\\Roster Files\\roster.exe' 'run' 'a1'"
        ])
    );
}

#[test]
fn opening_an_app_and_a_url_per_platform() {
    assert_eq!(
        app_open_command(Platform::Mac, "Warp"),
        s(&["open", "-a", "Warp"])
    );
    assert_eq!(
        app_open_command(Platform::Windows, "code"),
        s(&["cmd.exe", "/c", "code"])
    );
    let url = "https://mahadihassan.com";
    assert_eq!(url_open_command(Platform::Mac, url), s(&["open", url]));
    assert_eq!(
        url_open_command(Platform::Windows, url),
        s(&["cmd.exe", "/c", "start", "", url])
    );
    assert_eq!(
        url_open_command(Platform::Linux, url),
        s(&["xdg-open", url])
    );
}

#[test]
fn bundled_roster_sits_next_to_the_app() {
    let d = tempfile::tempdir().unwrap();
    let exe = d.path().join("agent-roster-app");
    assert_eq!(bundled_roster(Platform::Mac, &exe), None);
    touch(&d.path().join("roster"));
    assert_eq!(
        bundled_roster(Platform::Mac, &exe),
        Some(d.path().join("roster"))
    );
    touch(&d.path().join("roster.exe"));
    assert_eq!(
        bundled_roster(Platform::Windows, &d.path().join("agent-roster-app.exe")),
        Some(d.path().join("roster.exe"))
    );
}

#[test]
fn bundled_roster_is_not_used_from_a_disk_image_or_a_moved_copy() {
    let d = tempfile::tempdir().unwrap();
    let bin = d.path().join("AppTranslocation/X1/d");
    touch(&bin.join("roster"));
    assert_eq!(
        bundled_roster(Platform::Mac, &bin.join("agent-roster-app")),
        None
    );
    assert!(is_temporary_app_location(Path::new(
        "/Volumes/Agent Roster/Agent Roster.app/Contents/MacOS/agent-roster-app"
    )));
    assert!(!is_temporary_app_location(Path::new(
        "/Applications/Agent Roster.app/Contents/MacOS/agent-roster-app"
    )));
}
