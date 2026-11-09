#![cfg(unix)]
use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const FAKE_CLAUDE: &str = r#"#!/bin/sh
if [ "$1" = "--version" ]; then echo "2.1.319 (Claude Code)"; exit 0; fi
if [ "$1" = "auth" ] && [ "$2" = "status" ]; then
  echo '{"loggedIn": true, "email": "me@example.com", "subscriptionType": "max"}'; exit 0
fi
echo "CLAUDE_CONFIG_DIR=${CLAUDE_CONFIG_DIR:-unset}"
echo "ROSTER_AGENT=$ROSTER_AGENT"
exit 7
"#;

const FAKE_CODEX: &str = r#"#!/bin/sh
if [ "$1" = "--version" ]; then echo "codex-cli 0.167.0"; exit 0; fi
if [ "$1" = "login" ] && [ "$2" = "status" ]; then echo "Logged in using ChatGPT"; exit 0; fi
echo "CODEX_HOME=${CODEX_HOME:-unset}"
echo "ROSTER_AGENT=$ROSTER_AGENT"
exit 7
"#;

struct Home {
    d: tempfile::TempDir,
    tools: PathBuf,
}

impl Home {
    fn new(with_claude: bool) -> Home {
        let d = tempfile::tempdir().unwrap();
        let tools = d.path().join("tools");
        fs::create_dir_all(&tools).unwrap();
        let put = |name: &str, body: &str| {
            let f = tools.join(name);
            fs::write(&f, body).unwrap();
            fs::set_permissions(&f, fs::Permissions::from_mode(0o755)).unwrap();
        };
        if with_claude {
            put("claude", FAKE_CLAUDE);
        }
        put("codex", FAKE_CODEX);
        fs::create_dir_all(d.path().join(".claude")).unwrap();
        fs::create_dir_all(d.path().join(".codex/sessions/2026/11/09")).unwrap();
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../roster-core/tests/fixtures/codex-rollout.jsonl"),
            d.path().join(".codex/sessions/2026/11/09/rollout-x.jsonl"),
        )
        .unwrap();
        Home { d, tools }
    }
    fn bin(&self) -> PathBuf {
        self.d.path().join(".agent-roster/bin")
    }
    fn path_var(&self) -> String {
        format!(
            "{}:{}:/usr/bin:/bin",
            self.bin().display(),
            self.tools.display()
        )
    }
    fn roster(&self) -> Command {
        let mut c = Command::cargo_bin("roster").unwrap();
        c.env_clear()
            .env("HOME", self.d.path())
            .env("PATH", self.path_var())
            .env("TERM", "dumb")
            .current_dir(self.d.path());
        c
    }
    fn init(&self) {
        self.roster().args(["init", "--yes"]).assert().success();
    }
}

#[test]
fn first_run_without_roster_suggests_init() {
    let h = Home::new(true);
    h.roster()
        .assert()
        .success()
        .stdout(contains("roster init"));
}

#[test]
fn init_adopts_installed_tools() {
    let h = Home::new(true);
    h.init();
    h.roster()
        .assert()
        .success()
        .stdout(contains("claude1"))
        .stdout(contains("codex1"));
}

#[test]
fn overview_lists_agents_plain_when_piped() {
    let h = Home::new(true);
    h.init();
    let out = h.roster().assert().success().get_output().stdout.clone();
    let text = String::from_utf8(out).unwrap();
    assert!(!text.contains('\u{1b}'), "no escape codes when piped");
    assert!(text.contains("Claude Code") && text.contains("Codex"));
    assert!(text.contains("93%") && text.contains("resets in"));
    assert!(text.contains("ready"));
}

#[test]
fn add_without_yes_in_pipe_exits_2() {
    let h = Home::new(true);
    h.init();
    h.roster()
        .args(["add", "boss", "--codex"])
        .assert()
        .code(2)
        .stderr(contains("Add --yes"));
}

#[test]
fn add_yes_then_shim_runs_fake_codex_with_env() {
    let h = Home::new(true);
    h.init();
    h.roster()
        .args(["add", "boss", "--codex", "--yes"])
        .assert()
        .success()
        .stdout(contains("Adds a boss command"));
    let json: serde_json::Value =
        serde_json::from_slice(&h.roster().args(["--json"]).output().unwrap().stdout).unwrap();
    let boss = json["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "boss")
        .unwrap()
        .clone();
    let id = boss["id"].as_str().unwrap().to_string();
    let out = std::process::Command::new(h.bin().join("boss"))
        .arg("exec")
        .arg("hello")
        .env_clear()
        .env("HOME", h.d.path())
        .env("PATH", h.path_var())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(7), "tool exit code passes through");
    assert!(
        text.contains(&format!(
            "CODEX_HOME={}",
            h.d.path()
                .join(".agent-roster/accounts")
                .join(&id)
                .display()
        )),
        "{text}"
    );
    assert!(text.contains(&format!("ROSTER_AGENT={id}")));
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains('\u{1b}'),
        "no banner when piped"
    );
}

#[test]
fn adopted_default_runs_with_dir_unset() {
    let h = Home::new(true);
    h.init();
    let out = std::process::Command::new(h.bin().join("claude1"))
        .env_clear()
        .env("HOME", h.d.path())
        .env("PATH", h.path_var())
        .env("CLAUDE_CONFIG_DIR", "/should/be/removed")
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("CLAUDE_CONFIG_DIR=unset"));
}

#[test]
fn json_overview_shape() {
    let h = Home::new(true);
    h.init();
    let json: serde_json::Value =
        serde_json::from_slice(&h.roster().args(["--json"]).output().unwrap().stdout).unwrap();
    let a = &json["agents"][1];
    assert_eq!(a["name"], "codex1");
    assert_eq!(a["tool"], "codex");
    assert_eq!(a["status"], "ready");
    assert_eq!(a["limits"]["windows"][0]["used_percent"], 93.0);
    assert!(json["calls"].is_array());
}

#[test]
fn no_color_env_respected() {
    let h = Home::new(true);
    h.init();
    let out = h
        .roster()
        .env("NO_COLOR", "1")
        .env("CLICOLOR_FORCE", "1")
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&out.stdout).contains('\u{1b}'));
}

#[test]
fn missing_claude_shows_not_installed_and_exits_0() {
    let h = Home::new(false);
    h.roster()
        .args(["add", "worker", "--claude", "--yes"])
        .assert()
        .success();
    h.roster()
        .assert()
        .success()
        .stdout(contains("not installed"));
}

#[test]
fn changes_and_undo() {
    let h = Home::new(true);
    h.init();
    h.roster()
        .args(["add", "boss", "--codex", "--yes"])
        .assert()
        .success();
    h.roster()
        .args(["changes"])
        .assert()
        .success()
        .stdout(contains("Added boss"));
    h.roster()
        .args(["undo", "--last", "--yes"])
        .assert()
        .success();
    assert!(!h.bin().join("boss").exists());
}

#[test]
fn undo_all_then_uninstall_leaves_home_clean() {
    let h = Home::new(true);
    h.init();
    h.roster()
        .args(["add", "boss", "--codex", "--yes"])
        .assert()
        .success();
    h.roster().args(["uninstall", "--yes"]).assert().success();
    let data = h.d.path().join(".agent-roster");
    let left: Vec<String> = fs::read_dir(&data)
        .map(|r| {
            r.flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    assert!(left.iter().all(|n| n == "accounts"), "{left:?}");
    assert!(!h.d.path().join(".config/agent-roster/roster.toml").exists());
    assert!(
        !fs::read_to_string(h.d.path().join(".claude/CLAUDE.md"))
            .unwrap_or_default()
            .contains("agent-roster:begin")
    );
}

#[test]
fn bad_name_explains() {
    let h = Home::new(true);
    h.init();
    h.roster()
        .args(["add", "1", "--codex", "--yes"])
        .assert()
        .code(1)
        .stderr(contains("Try a1"));
}
