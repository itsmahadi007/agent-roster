//! Launches the real app binary twice. Needs a desktop session and no other Agent Roster
//! running, so it is opt-in: `cargo test -p agent-roster-app --test single_instance -- --ignored`.

use std::process::{Child, Command};
use std::time::{Duration, Instant};

fn launch(home: &std::path::Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_agent-roster-app"))
        .env("HOME", home)
        .spawn()
        .expect("start the app")
}

fn exits_within(c: &mut Child, limit: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        if c.try_wait().expect("poll").is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

#[test]
#[ignore = "launches the app; run with --ignored on a desktop with Agent Roster closed"]
fn second_launch_hands_over_and_exits() {
    let home = tempfile::tempdir().unwrap();
    let mut first = launch(home.path());
    let first_died = exits_within(&mut first, Duration::from_secs(5));
    let mut second = launch(home.path());
    let second_exited = exits_within(&mut second, Duration::from_secs(10));
    let _ = first.kill();
    let _ = second.kill();
    let _ = first.wait();
    let _ = second.wait();
    assert!(!first_died, "the first launch should keep running");
    assert!(
        second_exited,
        "a second launch must hand over to the running app and exit"
    );
}
