//! Codex sign-in over `codex app-server` (JSON-RPC, one object per line, no `"jsonrpc"`).
//!
//! 1. `initialize` → `initialized`, then `account/login/start {"type":"chatgpt"}`.
//! 2. The reply holds `loginId` and `authUrl` (https on auth.openai.com): that is the `Link`.
//! 3. `account/login/completed {loginId, success, error}` arrives when the person finishes
//!    (or Codex gives up after ~10 minutes). `account/login/cancel {loginId}` stops it.
//!
//! App-server hosts the callback server on 127.0.0.1:1455 for the whole login, so it stays
//! alive until the end, and then its whole tree is stopped. `CodexLogin` is the pure state
//! machine (tested on lines Codex 0.167.0 printed, in `tests/fixtures/codex-login/`); `drive`
//! runs it over channels; `run` adds the process.
//!
//! What Codex says is never passed on: its error texts may hold details, so people get a
//! short fixed reason instead.

use super::{LoginEvent, allow, msg};
use crate::model::Tool;
use crate::usage::sys;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::Command;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

const INITIALIZE: i64 = 1;
const START: i64 = 2;
const CANCEL: i64 = 3;

/// The port Codex's own callback server always uses.
pub const CALLBACK_PORT: u16 = 1455;

/// How long Codex may take to hand over the link.
pub const LINK_TIMEOUT: Duration = Duration::from_secs(30);
/// Codex gives up after about 10 minutes; this is the backstop if it never says so.
pub const TOTAL_TIMEOUT: Duration = Duration::from_secs(11 * 60);

/// What one line from app-server leads to.
#[derive(Debug, Default, PartialEq)]
pub struct Step {
    /// Lines to send back, in order.
    pub send: Vec<String>,
    /// What to tell the app, if anything.
    pub event: Option<LoginEvent>,
}

/// One Codex sign-in, one line at a time, with no I/O.
#[derive(Debug, Default)]
pub struct CodexLogin {
    initialized: bool,
    login_id: Option<String>,
    linked: bool,
    canceling: bool,
    finished: bool,
}

impl CodexLogin {
    pub fn new() -> Self {
        CodexLogin::default()
    }

    /// The first line to send: `initialize`.
    pub fn opening(&self) -> String {
        json!({
            "id": INITIALIZE,
            "method": "initialize",
            "params": {
                "clientInfo": {
                    "name": "agent-roster",
                    "title": "Agent Roster",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "capabilities": null,
            },
        })
        .to_string()
    }

    /// A `Done` or `Failed` was given: nothing more will come.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// The link was handed over.
    pub fn has_link(&self) -> bool {
        self.linked
    }

    fn finish(&mut self, event: LoginEvent) -> Option<LoginEvent> {
        if self.finished {
            return None;
        }
        self.finished = true;
        Some(event)
    }

    fn fail(&mut self, why: &str) -> Option<LoginEvent> {
        self.finish(LoginEvent::Failed(why.into()))
    }

    /// The window was closed or Cancel clicked: the line to send (if a login was started)
    /// and the `Failed("canceled")` to give.
    pub fn cancel(&mut self) -> Step {
        self.stop(msg::CANCELED)
    }

    /// Ends the login with `why`, asking Codex to drop it when one was started.
    fn stop(&mut self, why: &str) -> Step {
        let send = match (&self.login_id, self.finished || self.canceling) {
            (Some(id), false) => vec![
                json!({"id": CANCEL, "method": "account/login/cancel",
                    "params": {"loginId": id}})
                .to_string(),
            ],
            _ => vec![],
        };
        self.canceling = true;
        Step {
            send,
            event: self.fail(why),
        }
    }

    /// App-server's output ended (it crashed or was stopped).
    pub fn on_closed(&mut self) -> Option<LoginEvent> {
        self.fail(msg::CODEX_STOPPED)
    }

    /// Too long with no link, or no end.
    pub fn on_timeout(&mut self) -> Step {
        self.stop(if self.linked {
            msg::TIMED_OUT
        } else {
            msg::CODEX_START_FAILED
        })
    }

    /// Takes one line app-server printed.
    pub fn on_line(&mut self, line: &str) -> Step {
        let Ok(Value::Object(m)) = serde_json::from_str::<Value>(line.trim()) else {
            return Step::default();
        };
        let id = m.get("id");
        if let Some(method) = m.get("method").and_then(Value::as_str) {
            if let Some(id) = id {
                // A request from the server needs an answer; we support none.
                return Step {
                    send: vec![
                        json!({"id": id, "error": {"code": -32601, "message": "not supported"}})
                            .to_string(),
                    ],
                    event: None,
                };
            }
            if method == "account/login/completed" {
                return Step {
                    send: vec![],
                    event: self.on_completed(m.get("params").unwrap_or(&Value::Null)),
                };
            }
            return Step::default();
        }
        let error = m.get("error");
        match id.and_then(Value::as_i64) {
            Some(INITIALIZE) if !self.initialized => {
                if error.is_some() {
                    return Step {
                        send: vec![],
                        event: self.fail(msg::CODEX_START_FAILED),
                    };
                }
                self.initialized = true;
                Step {
                    send: vec![
                        json!({"method": "initialized"}).to_string(),
                        json!({"id": START, "method": "account/login/start",
                            "params": {"type": "chatgpt"}})
                        .to_string(),
                    ],
                    event: None,
                }
            }
            Some(START) if self.login_id.is_none() && !self.finished => {
                self.on_started(m.get("result"), error)
            }
            _ => Step::default(),
        }
    }

    fn on_started(&mut self, result: Option<&Value>, error: Option<&Value>) -> Step {
        if let Some(e) = error {
            let text = e
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_ascii_lowercase();
            let port_busy = text.contains(&CALLBACK_PORT.to_string())
                || text.contains("address in use")
                || text.contains("address already in use")
                || text.contains("only one usage of each socket address");
            return self.stop(if port_busy {
                msg::PORT_BUSY
            } else {
                msg::CODEX_START_FAILED
            });
        }
        let result = result.unwrap_or(&Value::Null);
        let id = result.get("loginId").and_then(Value::as_str);
        let url = result.get("authUrl").and_then(Value::as_str);
        match (id, url) {
            (Some(id), Some(url)) if !id.is_empty() => {
                self.login_id = Some(id.to_string());
                if !allow::is_first_page(Tool::Codex, url) {
                    return self.stop(msg::BAD_LINK);
                }
                self.linked = true;
                Step {
                    send: vec![],
                    event: Some(LoginEvent::Link(url.to_string())),
                }
            }
            _ => self.stop(msg::CODEX_START_FAILED),
        }
    }

    fn on_completed(&mut self, params: &Value) -> Option<LoginEvent> {
        let ours = matches!(
            (&self.login_id, params.get("loginId").and_then(Value::as_str)),
            (Some(a), Some(b)) if a == b
        );
        if !ours {
            return None;
        }
        if params.get("success").and_then(Value::as_bool) == Some(true) {
            self.finish(LoginEvent::Done)
        } else {
            self.fail(msg::NOT_FINISHED)
        }
    }
}

/// What the login thread waits for.
#[derive(Debug)]
pub enum Input {
    /// One line app-server printed.
    Line(String),
    /// App-server's output ended.
    Closed,
    /// The person canceled.
    Cancel,
}

fn send(out: &mut dyn Write, lines: &[String]) -> bool {
    for l in lines {
        if out.write_all(l.as_bytes()).is_err() || out.write_all(b"\n").is_err() {
            return false;
        }
    }
    out.flush().is_ok()
}

/// Runs a `CodexLogin` over app-server's lines (`rx`) and stdin (`out`) until it finishes.
/// Gives exactly one `Link` (at most) and then exactly one `Done` or `Failed`.
pub fn drive(
    rx: &Receiver<Input>,
    out: &mut dyn Write,
    on_event: &dyn Fn(LoginEvent),
    link_timeout: Duration,
    total_timeout: Duration,
) {
    let mut m = CodexLogin::new();
    let started = Instant::now();
    let emit = |e: Option<LoginEvent>| {
        if let Some(e) = e {
            on_event(e);
        }
    };
    if !send(out, &[m.opening()]) {
        emit(m.on_closed());
        return;
    }
    while !m.is_finished() {
        let limit = if m.has_link() {
            total_timeout
        } else {
            link_timeout
        };
        let left = (started + limit).saturating_duration_since(Instant::now());
        let step = match rx.recv_timeout(left) {
            Ok(Input::Line(l)) => m.on_line(&l),
            Ok(Input::Cancel) => m.cancel(),
            Ok(Input::Closed) | Err(RecvTimeoutError::Disconnected) => Step {
                send: vec![],
                event: m.on_closed(),
            },
            Err(RecvTimeoutError::Timeout) => m.on_timeout(),
        };
        let ok = send(out, &step.send);
        emit(step.event);
        if !ok {
            emit(m.on_closed());
        }
    }
}

/// Starts `<tool> app-server` hidden for the agent and runs the login on a new thread. The
/// tree is always stopped at the end. `inputs` is how the caller cancels (send `Cancel`).
pub fn run(
    tool_path: &Path,
    env: &crate::run::EnvChanges,
    cwd: &Path,
    tx: Sender<Input>,
    rx: Receiver<Input>,
    on_event: Box<dyn Fn(LoginEvent) + Send + Sync>,
    on_end: Box<dyn FnOnce() + Send>,
) -> std::io::Result<()> {
    let mut cmd = Command::new(tool_path);
    cmd.arg("app-server").current_dir(cwd);
    for (k, v) in env {
        match v {
            Some(v) => cmd.env(k, v),
            None => cmd.env_remove(k),
        };
    }
    let mut child = sys::spawn_piped(&mut cmd)?;
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        sys::kill_tree(&mut child);
        return Err(std::io::Error::other("app-server has no pipes"));
    };
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).split(b'\n') {
            let Ok(line) = line else { break };
            if tx
                .send(Input::Line(String::from_utf8_lossy(&line).into_owned()))
                .is_err()
            {
                return;
            }
        }
        let _ = tx.send(Input::Closed);
    });
    std::thread::spawn(move || {
        drive(&rx, &mut stdin, &*on_event, LINK_TIMEOUT, TOTAL_TIMEOUT);
        // Let app-server take the cancel in before it is stopped.
        let _ = rx.recv_timeout(Duration::from_millis(300));
        drop(stdin);
        sys::kill_tree(&mut child);
        on_end();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::sync::mpsc::channel;

    fn fixture(name: &str) -> Vec<String> {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/codex-login")
            .join(name);
        std::fs::read_to_string(p)
            .unwrap()
            .lines()
            .map(String::from)
            .collect()
    }

    fn sent(lines: &[String]) -> Vec<Value> {
        lines
            .iter()
            .map(|l| {
                assert!(!l.contains('\n'));
                serde_json::from_str(l).unwrap()
            })
            .collect()
    }

    /// Feeds recorded lines in order; returns every event and every line sent.
    fn replay(lines: &[String]) -> (Vec<LoginEvent>, Vec<Value>) {
        let mut m = CodexLogin::new();
        let mut events = vec![];
        let mut out = vec![];
        for l in lines {
            let s = m.on_line(l);
            out.extend(sent(&s.send));
            events.extend(s.event);
        }
        (events, out)
    }

    const LINK: &str = "https://auth.openai.com/oauth/authorize?response_type=code&client_id=app_example&redirect_uri=http%3A%2F%2F127.0.0.1%3A1455%2Fauth%2Fcallback&state=example";

    #[test]
    fn opens_with_initialize() {
        let v = &sent(&[CodexLogin::new().opening()])[0];
        assert_eq!(v["method"], "initialize");
        assert_eq!(v["id"], 1);
        assert_eq!(v["params"]["clientInfo"]["name"], "agent-roster");
        assert!(v["params"]["capabilities"].is_null());
    }

    #[test]
    fn after_initialize_starts_a_chatgpt_login() {
        let lines = fixture("start.jsonl");
        let mut m = CodexLogin::new();
        let s = m.on_line(&lines[0]);
        let v = sent(&s.send);
        assert_eq!(v[0]["method"], "initialized");
        assert!(v[0].get("id").is_none());
        assert_eq!(v[1]["method"], "account/login/start");
        assert_eq!(v[1]["params"], json!({"type": "chatgpt"}));
        assert_eq!(s.event, None);
    }

    #[test]
    fn start_reply_gives_the_link() {
        let (events, _) = replay(&fixture("start.jsonl"));
        assert_eq!(events, vec![LoginEvent::Link(LINK.into())]);
    }

    #[test]
    fn completed_success_is_done() {
        let (events, _) = replay(&fixture("success.jsonl"));
        assert_eq!(
            events,
            vec![LoginEvent::Link(LINK.into()), LoginEvent::Done]
        );
    }

    #[test]
    fn completed_failure_is_failed_without_codexs_text() {
        let (events, _) = replay(&fixture("failure.jsonl"));
        assert_eq!(events.len(), 2);
        let LoginEvent::Failed(why) = &events[1] else {
            panic!("{events:?}")
        };
        assert_eq!(why, msg::NOT_FINISHED);
        assert!(!why.contains("Login server error"));
    }

    #[test]
    fn completion_of_another_login_is_ignored() {
        let mut m = CodexLogin::new();
        for l in fixture("start.jsonl") {
            m.on_line(&l);
        }
        let s = m.on_line(
            r#"{"method":"account/login/completed","params":{"loginId":"other","success":true,"error":null}}"#,
        );
        assert_eq!(s.event, None);
        assert!(!m.is_finished());
    }

    #[test]
    fn cancel_sends_login_cancel_and_fails_once() {
        let lines = fixture("cancel.jsonl");
        let mut m = CodexLogin::new();
        // Up to the start reply.
        for l in &lines[..3] {
            m.on_line(l);
        }
        let s = m.cancel();
        let v = sent(&s.send);
        assert_eq!(v[0]["method"], "account/login/cancel");
        assert_eq!(
            v[0]["params"]["loginId"],
            "c2d1063f-d234-4804-9c68-e0e63517882c"
        );
        assert_eq!(s.event, Some(LoginEvent::Failed("canceled".into())));
        // Codex's reply and its `completed {success:false}` add nothing.
        for l in &lines[3..] {
            assert_eq!(m.on_line(l).event, None, "{l}");
        }
        assert_eq!(m.cancel(), Step::default());
    }

    #[test]
    fn cancel_before_the_link_sends_nothing() {
        let mut m = CodexLogin::new();
        let s = m.cancel();
        assert!(s.send.is_empty());
        assert_eq!(s.event, Some(LoginEvent::Failed("canceled".into())));
    }

    #[test]
    fn port_busy_start_error_says_so() {
        let mut m = CodexLogin::new();
        m.on_line(&fixture("start.jsonl")[0]);
        let s = m.on_line(
            r#"{"id":2,"error":{"code":-32603,"message":"failed to start login server: Address already in use (os error 10048) on 127.0.0.1:1455"}}"#,
        );
        assert_eq!(s.event, Some(LoginEvent::Failed(msg::PORT_BUSY.into())));
    }

    #[test]
    fn other_start_error_is_generic() {
        let mut m = CodexLogin::new();
        m.on_line(&fixture("start.jsonl")[0]);
        let s = m.on_line(r#"{"id":2,"error":{"code":-32600,"message":"https://x?token=abc"}}"#);
        assert_eq!(
            s.event,
            Some(LoginEvent::Failed(msg::CODEX_START_FAILED.into()))
        );
    }

    #[test]
    fn a_link_off_the_provider_is_refused() {
        let mut m = CodexLogin::new();
        m.on_line(&fixture("start.jsonl")[0]);
        let s = m.on_line(
            r#"{"id":2,"result":{"type":"chatgpt","loginId":"L","authUrl":"https://auth.openai.com@evil.example.com/x"}}"#,
        );
        assert_eq!(s.event, Some(LoginEvent::Failed(msg::BAD_LINK.into())));
        assert_eq!(sent(&s.send)[0]["method"], "account/login/cancel");
        assert_eq!(sent(&s.send)[0]["params"]["loginId"], "L");
    }

    #[test]
    fn server_requests_are_refused_and_noise_skipped() {
        let mut m = CodexLogin::new();
        assert_eq!(m.on_line("WARNING: not json"), Step::default());
        let s = m.on_line(r#"{"id":"s1","method":"attestation/generate","params":{}}"#);
        assert_eq!(sent(&s.send)[0]["id"], "s1");
        assert!(sent(&s.send)[0]["error"].is_object());
    }

    #[test]
    fn closed_output_fails_once() {
        let mut m = CodexLogin::new();
        assert_eq!(
            m.on_closed(),
            Some(LoginEvent::Failed(msg::CODEX_STOPPED.into()))
        );
        assert_eq!(m.on_closed(), None);
    }

    // `drive`, over channels.

    fn events_of(
        feed: impl FnOnce(&Sender<Input>),
        link_timeout: Duration,
    ) -> (Vec<LoginEvent>, Vec<Value>) {
        // `tx` stays alive while `drive` runs, as the reader thread's would.
        let (tx, rx) = channel();
        feed(&tx);
        let got = Mutex::new(vec![]);
        let mut out: Vec<u8> = vec![];
        drive(
            &rx,
            &mut out,
            &|e| got.lock().unwrap().push(e),
            link_timeout,
            Duration::from_secs(5),
        );
        let lines: Vec<String> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(String::from)
            .collect();
        (got.into_inner().unwrap(), sent(&lines))
    }

    #[test]
    fn drive_runs_a_whole_login() {
        let (events, out) = events_of(
            |tx| {
                for l in fixture("success.jsonl") {
                    tx.send(Input::Line(l)).unwrap();
                }
            },
            Duration::from_secs(5),
        );
        assert_eq!(
            events,
            vec![LoginEvent::Link(LINK.into()), LoginEvent::Done]
        );
        let methods: Vec<_> = out.iter().map(|v| v["method"].clone()).collect();
        assert_eq!(
            methods,
            [
                json!("initialize"),
                json!("initialized"),
                json!("account/login/start")
            ]
        );
    }

    #[test]
    fn drive_cancel_after_the_link() {
        let (events, out) = events_of(
            |tx| {
                for l in fixture("start.jsonl") {
                    tx.send(Input::Line(l)).unwrap();
                }
                tx.send(Input::Cancel).unwrap();
            },
            Duration::from_secs(5),
        );
        assert_eq!(
            events,
            vec![
                LoginEvent::Link(LINK.into()),
                LoginEvent::Failed("canceled".into())
            ]
        );
        assert_eq!(out.last().unwrap()["method"], "account/login/cancel");
    }

    #[test]
    fn drive_without_a_link_times_out() {
        let (events, _) = events_of(|_tx| {}, Duration::from_millis(200));
        assert_eq!(
            events,
            vec![LoginEvent::Failed(msg::CODEX_START_FAILED.into())]
        );
    }

    #[test]
    fn drive_app_server_exits() {
        let (events, _) = events_of(
            |tx| {
                for l in fixture("start.jsonl") {
                    tx.send(Input::Line(l)).unwrap();
                }
                tx.send(Input::Closed).unwrap();
            },
            Duration::from_secs(5),
        );
        assert_eq!(
            events,
            vec![
                LoginEvent::Link(LINK.into()),
                LoginEvent::Failed(msg::CODEX_STOPPED.into())
            ]
        );
    }
}
