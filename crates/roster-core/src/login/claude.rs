//! Claude Code sign-in: `claude auth login --claudeai`, hidden, with `BROWSER` set to
//! `roster.exe` and a one-time link channel (see `bridge`). Claude Code runs its own callback
//! server and exits 0 once the login is saved: that exit is the only `Done` (its success page,
//! platform.claude.com, proves nothing). Its output is read and thrown away, never logged: it
//! holds a "paste code" fallback link, and a pasted code is never collected.

use super::bridge::{self, CHANNEL_VAR, LinkChannel, NONCE_VAR};
use super::{LoginEvent, allow, msg};
use crate::model::Tool;
use crate::usage::sys;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

/// How long Claude Code may take to hand over the link.
pub const LINK_TIMEOUT: Duration = Duration::from_secs(15);
/// The longest a sign-in may stay open.
pub const TOTAL_TIMEOUT: Duration = Duration::from_secs(15 * 60);

const POLL: Duration = Duration::from_millis(100);

/// What the login thread waits for.
#[derive(Debug)]
pub enum Input {
    /// The bridge got a valid link.
    Link(String),
    /// The bridge closed without one (expired, or a bad message).
    NoLink,
    /// The person canceled.
    Cancel,
}

/// Runs the login until it ends: `exited` says whether Claude Code has exited and how
/// (`Some(true)`: exit 0). Gives at most one `Link`, then exactly one `Done` or `Failed`.
pub fn drive(
    rx: &Receiver<Input>,
    exited: &mut dyn FnMut() -> Option<bool>,
    on_event: &dyn Fn(LoginEvent),
    link_timeout: Duration,
    total_timeout: Duration,
) {
    let started = Instant::now();
    let mut linked = false;
    let fail = |why: &str| on_event(LoginEvent::Failed(why.into()));
    loop {
        match rx.recv_timeout(POLL) {
            Ok(Input::Link(url)) if !linked => {
                if !allow::is_first_page(Tool::Claude, &url) {
                    return fail(msg::BAD_LINK);
                }
                linked = true;
                on_event(LoginEvent::Link(url));
                continue;
            }
            Ok(Input::NoLink) if !linked => return fail(msg::NO_CLAUDE_LINK),
            Ok(Input::Cancel) => return fail(msg::CANCELED),
            Ok(_) => continue,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return fail(msg::CANCELED),
        }
        match exited() {
            Some(true) if linked => return on_event(LoginEvent::Done),
            Some(true) => return fail(msg::NO_CLAUDE_LINK),
            Some(false) if linked => return fail(msg::CLAUDE_FAILED),
            Some(false) => return fail(msg::CLAUDE_START_FAILED),
            None => {}
        }
        let waited = started.elapsed();
        if !linked && waited >= link_timeout {
            return fail(msg::NO_CLAUDE_LINK);
        }
        if waited >= total_timeout {
            return fail(msg::TIMED_OUT);
        }
    }
}

/// `BROWSER` for Claude Code: the plain path. Claude Code starts it without a shell, so a
/// path with spaces works as is (checked with Claude Code 2.1.322 on Windows).
pub fn browser_value(roster_bin: &Path) -> std::ffi::OsString {
    roster_bin.as_os_str().to_owned()
}

/// Reads a pipe to the end and throws it away.
fn drain(r: Option<impl Read + Send + 'static>) {
    if let Some(mut r) = r {
        std::thread::spawn(move || {
            let _ = std::io::copy(&mut r, &mut std::io::sink());
        });
    }
}

/// Starts `<tool> auth login --claudeai` hidden for the agent and runs the login on a new
/// thread. The tree is always stopped at the end. Send `Input::Cancel` on `tx` to cancel.
#[allow(clippy::too_many_arguments)]
pub fn run(
    tool_path: &Path,
    env: &crate::run::EnvChanges,
    cwd: &Path,
    roster_bin: &Path,
    tx: Sender<Input>,
    rx: Receiver<Input>,
    on_event: Box<dyn Fn(LoginEvent) + Send + Sync>,
    on_end: Box<dyn FnOnce() + Send>,
) -> std::io::Result<()> {
    let channel = LinkChannel::create()?;
    let mut cmd = Command::new(tool_path);
    cmd.args(["auth", "login", "--claudeai"]).current_dir(cwd);
    for (k, v) in env {
        match v {
            Some(v) => cmd.env(k, v),
            None => cmd.env_remove(k),
        };
    }
    cmd.env("BROWSER", browser_value(roster_bin))
        .env(CHANNEL_VAR, channel.name())
        .env(NONCE_VAR, channel.nonce())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = sys::spawn_contained(&mut cmd)?;
    drain(child.stdout.take());
    drain(child.stderr.take());
    let stop = Arc::new(AtomicBool::new(false));
    let bridge_stop = stop.clone();
    std::thread::spawn(move || {
        let deadline = Instant::now() + bridge::EXPIRY;
        let got = channel.receive(deadline, &bridge_stop);
        if !bridge_stop.load(Ordering::SeqCst) {
            let _ = tx.send(got.map_or(Input::NoLink, Input::Link));
        }
    });
    std::thread::spawn(move || {
        // Peek, never reap: `kill_tree` must still find Claude Code's group afterwards.
        let mut exited = || match sys::peek_exit(&mut child) {
            Ok(Some(status)) => Some(status.success()),
            Ok(None) => None,
            Err(_) => Some(false),
        };
        drive(&rx, &mut exited, &*on_event, LINK_TIMEOUT, TOTAL_TIMEOUT);
        stop.store(true, Ordering::SeqCst);
        sys::kill_tree(&mut child);
        on_end();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::mpsc::channel;

    const LINK: &str = "https://claude.com/cai/oauth/authorize?code=true&redirect_uri=http%3A%2F%2Flocalhost%3A50123%2Fcallback";

    /// Runs `drive` with `inputs` queued and `exit` reported after `exit_after` polls.
    fn run_drive(
        inputs: Vec<Input>,
        exit: Option<bool>,
        exit_after: usize,
        link_timeout: Duration,
    ) -> Vec<LoginEvent> {
        let (tx, rx) = channel();
        for i in inputs {
            tx.send(i).unwrap();
        }
        let got = Mutex::new(vec![]);
        let mut polls = 0;
        let mut exited = || {
            polls += 1;
            if polls > exit_after { exit } else { None }
        };
        drive(
            &rx,
            &mut exited,
            &|e| got.lock().unwrap().push(e),
            link_timeout,
            Duration::from_secs(5),
        );
        drop(tx);
        got.into_inner().unwrap()
    }

    const LONG: Duration = Duration::from_secs(5);

    #[test]
    fn link_then_exit_0_is_done() {
        assert_eq!(
            run_drive(vec![Input::Link(LINK.into())], Some(true), 2, LONG),
            vec![LoginEvent::Link(LINK.into()), LoginEvent::Done]
        );
    }

    #[test]
    fn link_then_a_failed_exit_is_failed() {
        assert_eq!(
            run_drive(vec![Input::Link(LINK.into())], Some(false), 2, LONG),
            vec![
                LoginEvent::Link(LINK.into()),
                LoginEvent::Failed(msg::CLAUDE_FAILED.into())
            ]
        );
    }

    #[test]
    fn no_link_in_time_is_failed() {
        assert_eq!(
            run_drive(vec![], None, 0, Duration::from_millis(300)),
            vec![LoginEvent::Failed(msg::NO_CLAUDE_LINK.into())]
        );
    }

    #[test]
    fn a_closed_bridge_is_failed() {
        assert_eq!(
            run_drive(vec![Input::NoLink], None, 0, LONG),
            vec![LoginEvent::Failed(msg::NO_CLAUDE_LINK.into())]
        );
    }

    #[test]
    fn exit_0_without_a_link_is_not_done() {
        assert_eq!(
            run_drive(vec![], Some(true), 1, LONG),
            vec![LoginEvent::Failed(msg::NO_CLAUDE_LINK.into())]
        );
    }

    #[test]
    fn cancel_after_the_link() {
        assert_eq!(
            run_drive(vec![Input::Link(LINK.into()), Input::Cancel], None, 0, LONG),
            vec![
                LoginEvent::Link(LINK.into()),
                LoginEvent::Failed("canceled".into())
            ]
        );
    }

    #[test]
    fn a_link_off_claude_is_refused() {
        assert_eq!(
            run_drive(
                vec![Input::Link("https://evil.example.com/".into())],
                None,
                0,
                LONG
            ),
            vec![LoginEvent::Failed(msg::BAD_LINK.into())]
        );
    }

    #[test]
    fn a_second_link_is_ignored() {
        assert_eq!(
            run_drive(
                vec![Input::Link(LINK.into()), Input::Link(LINK.into())],
                Some(true),
                3,
                LONG
            ),
            vec![LoginEvent::Link(LINK.into()), LoginEvent::Done]
        );
    }

    /// Claude Code exits by itself (here at once, with no link) and leaves a helper running
    /// in its group: the end of the login must still stop it.
    #[cfg(unix)]
    #[test]
    fn what_claude_leaves_running_is_stopped_after_it_exits() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let tool = d.path().join("claude");
        std::fs::write(
            &tool,
            "#!/bin/sh
( while [ ! -e stop ]; do echo x >> out; sleep 0.1; done ) &
sleep 0.5
exit 0
",
        )
        .unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        struct Stop(std::path::PathBuf);
        impl Drop for Stop {
            fn drop(&mut self) {
                let _ = std::fs::write(&self.0, "");
            }
        }
        let _stop = Stop(d.path().join("stop"));
        let (tx, rx) = channel();
        let (end_tx, end_rx) = channel();
        let events = Arc::new(Mutex::new(vec![]));
        let seen = events.clone();
        run(
            &tool,
            &vec![],
            d.path(),
            Path::new("/bin/true"),
            tx,
            rx,
            Box::new(move |e| seen.lock().unwrap().push(e)),
            Box::new(move || {
                let _ = end_tx.send(());
            }),
        )
        .unwrap();
        end_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the login ended");
        assert_eq!(
            *events.lock().unwrap(),
            [LoginEvent::Failed(msg::NO_CLAUDE_LINK.into())]
        );
        let size = || {
            std::fs::metadata(d.path().join("out"))
                .map(|m| m.len())
                .unwrap_or(0)
        };
        std::thread::sleep(Duration::from_millis(300));
        let before = size();
        assert!(before > 0, "the helper never started");
        std::thread::sleep(Duration::from_millis(1000));
        assert_eq!(size(), before, "the helper is still running");
    }

    #[test]
    fn browser_is_the_plain_path_even_with_spaces() {
        let p = Path::new(r"C:\Program Files\Agent Roster\roster.exe");
        assert_eq!(browser_value(p), p.as_os_str());
    }
}
