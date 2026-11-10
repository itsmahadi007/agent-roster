//! The Claude link bridge: how `claude auth login` hands its sign-in link back to Agent Roster.
//!
//! Claude Code opens the link by running `$BROWSER <link>`. Agent Roster sets `BROWSER` to
//! `roster.exe`, plus a one-time channel to send the link over:
//!
//! - Windows: a named pipe `\\.\pipe\agent-roster-login-<random>`, created first-instance
//!   (no one can have made it before us), one instance, remote clients rejected, and a DACL
//!   that lets only the current user open it.
//! - macOS and Linux: a socket `login.sock` in a new `0700` folder `agent-roster-login-<random>`
//!   in the temp folder, so only the current user can reach it.
//!
//! A random nonce rides along in the environment, the channel expires after `EXPIRY`, and it
//! takes exactly one message of at most `MAX_MESSAGE` bytes: `{"nonce": …, "url": …}`. The
//! message counts only with the right nonce and a link that `is_first_page(Claude)` accepts.
//! Nothing here takes a file path to write to from the environment, and the channel name
//! must have the exact shape above before `roster.exe` connects to it.

use super::allow::is_first_page;
use crate::model::Tool;
use serde_json::{Value, json};
use std::ffi::OsString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Names the channel for `roster.exe`.
pub const CHANNEL_VAR: &str = "ROSTER_LOGIN_CHANNEL";
/// The one-time nonce `roster.exe` must send back.
pub const NONCE_VAR: &str = "ROSTER_LOGIN_NONCE";
/// The largest message the channel takes.
pub const MAX_MESSAGE: usize = 8 * 1024;
/// How long a channel waits for its one message.
pub const EXPIRY: Duration = Duration::from_secs(120);

const PREFIX: &str = "agent-roster-login-";

/// 16 random bytes as 32 lowercase hex characters.
fn random_hex() -> String {
    let bytes: [u8; 16] = rand::random();
    hex::encode(bytes)
}

fn is_token(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Equal without stopping at the first difference.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The message `roster.exe` sends.
pub fn message(nonce: &str, url: &str) -> Vec<u8> {
    json!({"nonce": nonce, "url": url}).to_string().into_bytes()
}

/// The link in a message, when it is small enough, carries `nonce`, and is a Claude sign-in
/// link. Anything else: `None`.
pub fn parse_message(bytes: &[u8], nonce: &str) -> Option<String> {
    if bytes.len() > MAX_MESSAGE {
        return None;
    }
    let Ok(Value::Object(m)) = serde_json::from_slice::<Value>(bytes) else {
        return None;
    };
    if m.len() != 2 {
        return None;
    }
    let sent = m.get("nonce")?.as_str()?;
    let url = m.get("url")?.as_str()?;
    (same(sent.as_bytes(), nonce.as_bytes()) && is_first_page(Tool::Claude, url))
        .then(|| url.to_string())
}

/// Does `name` have the exact shape of a channel `LinkChannel::create` makes?
pub fn valid_channel_name(name: &str) -> bool {
    #[cfg(windows)]
    {
        name.strip_prefix(r"\\.\pipe\")
            .and_then(|n| n.strip_prefix(PREFIX))
            .is_some_and(is_token)
    }
    #[cfg(unix)]
    {
        let p = std::path::Path::new(name);
        p.is_absolute()
            && p.file_name().is_some_and(|f| f == "login.sock")
            && p.parent()
                .and_then(|d| d.file_name())
                .and_then(|d| d.to_str())
                .and_then(|d| d.strip_prefix(PREFIX))
                .is_some_and(is_token)
    }
}

/// Sends the link to the channel (what `roster.exe` does as `BROWSER`).
pub fn send_link(channel: &str, nonce: &str, url: &str) -> std::io::Result<()> {
    if !valid_channel_name(channel) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a sign-in channel",
        ));
    }
    sys::send(channel, &message(nonce, url))
}

/// What `roster.exe` does first thing. `None`: not started as the sign-in `BROWSER` (neither
/// variable set), carry on as usual. `Some(code)`: exit with `code` and do nothing else.
/// Exits 0 only after sending a Claude sign-in link given as the one and only argument.
pub fn browser_handoff(env: &dyn Fn(&str) -> Option<OsString>, args: &[OsString]) -> Option<u8> {
    let channel = env(CHANNEL_VAR);
    let nonce = env(NONCE_VAR);
    if channel.is_none() && nonce.is_none() {
        return None;
    }
    let sent = (|| {
        let channel = channel?.into_string().ok()?;
        let nonce = nonce?.into_string().ok()?;
        let [url] = args else { return None };
        let url = url.to_str()?;
        (is_token(&nonce) && is_first_page(Tool::Claude, url)).then_some(())?;
        send_link(&channel, &nonce, url).ok()
    })();
    Some(if sent.is_some() { 0 } else { 2 })
}

/// A one-time channel waiting for the sign-in link. Closed (and on Unix removed) when dropped.
pub struct LinkChannel {
    name: String,
    nonce: String,
    inner: sys::Listener,
}

impl LinkChannel {
    pub fn create() -> std::io::Result<Self> {
        let (name, inner) = sys::Listener::create(&random_hex())?;
        Ok(LinkChannel {
            name,
            nonce: random_hex(),
            inner,
        })
    }

    /// For `CHANNEL_VAR`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// For `NONCE_VAR`.
    pub fn nonce(&self) -> &str {
        &self.nonce
    }

    /// Waits for the one message until `deadline` (or until `stop` is set) and returns its
    /// link when it is valid. Takes one message only: the channel is closed afterwards.
    pub fn receive(self, deadline: Instant, stop: &AtomicBool) -> Option<String> {
        let give_up = || stop.load(Ordering::SeqCst) || Instant::now() >= deadline;
        let bytes = self.inner.receive_one(&give_up)?;
        parse_message(&bytes, &self.nonce)
    }
}

#[cfg(windows)]
mod sys {
    use super::{MAX_MESSAGE, PREFIX};
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::windows::fs::OpenOptionsExt;
    use std::ptr::null;
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, GetLastError, HANDLE,
        INVALID_HANDLE_VALUE, LocalFree, WAIT_OBJECT_0,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
        TokenUser,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, PIPE_ACCESS_INBOUND, ReadFile,
        SECURITY_IDENTIFICATION,
    };
    use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_MESSAGE, PIPE_REJECT_REMOTE_CLIENTS,
        PIPE_TYPE_MESSAGE, PIPE_WAIT,
    };
    use windows_sys::Win32::System::Threading::{
        CreateEventW, GetCurrentProcess, OpenProcessToken, WaitForSingleObject,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// The current user's SID as a string (`S-1-5-21-…`).
    fn user_sid() -> std::io::Result<String> {
        // SAFETY: Win32 calls on our own process token; buffers are sized as Windows asks,
        // and every handle and allocation is released before returning.
        unsafe {
            let mut token: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err(std::io::Error::last_os_error());
            }
            let mut len = 0u32;
            GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut len);
            // u64 storage keeps the TOKEN_USER properly aligned.
            let mut buf = vec![0u64; (len as usize).div_ceil(8).max(1)];
            let ok = GetTokenInformation(token, TokenUser, buf.as_mut_ptr().cast(), len, &mut len);
            CloseHandle(token);
            if ok == 0 {
                return Err(std::io::Error::last_os_error());
            }
            let user = &*(buf.as_ptr() as *const TOKEN_USER);
            let mut text: *mut u16 = std::ptr::null_mut();
            if ConvertSidToStringSidW(user.User.Sid, &mut text) == 0 {
                return Err(std::io::Error::last_os_error());
            }
            let n = (0..).take_while(|&i| *text.add(i) != 0).count();
            let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, n));
            LocalFree(text.cast());
            Ok(sid)
        }
    }

    /// The pipe's handle and the event its overlapped calls signal.
    pub struct Listener {
        pipe: HANDLE,
        event: HANDLE,
    }

    // SAFETY: the handles are only used through `&self`/`self` by one thread at a time.
    unsafe impl Send for Listener {}

    impl Drop for Listener {
        fn drop(&mut self) {
            // SAFETY: both handles were opened in `create` and are closed once, here.
            unsafe {
                CloseHandle(self.pipe);
                CloseHandle(self.event);
            }
        }
    }

    impl Listener {
        pub fn create(id: &str) -> std::io::Result<(String, Listener)> {
            let name = format!(r"\\.\pipe\{PREFIX}{id}");
            // Only this user, full access; nobody else (protected: nothing inherited).
            let sddl = wide(&format!("D:P(A;;GA;;;{})", user_sid()?));
            // SAFETY: Win32 calls with valid, NUL-terminated strings; the descriptor is freed
            // after the pipe is made, and every handle is owned by the returned `Listener`.
            unsafe {
                let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
                if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut sd,
                    std::ptr::null_mut(),
                ) == 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                let sa = SECURITY_ATTRIBUTES {
                    nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                    lpSecurityDescriptor: sd,
                    bInheritHandle: 0,
                };
                let pipe = CreateNamedPipeW(
                    wide(&name).as_ptr(),
                    PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
                    PIPE_TYPE_MESSAGE
                        | PIPE_READMODE_MESSAGE
                        | PIPE_WAIT
                        | PIPE_REJECT_REMOTE_CLIENTS,
                    1,
                    0,
                    MAX_MESSAGE as u32,
                    0,
                    &sa,
                );
                let err = std::io::Error::last_os_error();
                LocalFree(sd);
                if pipe == INVALID_HANDLE_VALUE {
                    return Err(err);
                }
                let event = CreateEventW(null(), 1, 0, null());
                if event.is_null() {
                    let err = std::io::Error::last_os_error();
                    CloseHandle(pipe);
                    return Err(err);
                }
                Ok((name, Listener { pipe, event }))
            }
        }

        /// Waits for an overlapped call to finish, or cancels it once `give_up` says so.
        /// Returns the bytes moved, or `None` when it failed or was canceled.
        ///
        /// SAFETY: `ov` must be the OVERLAPPED of a call just started on `self.pipe`.
        unsafe fn finish(&self, ov: &mut OVERLAPPED, give_up: &dyn Fn() -> bool) -> Option<u32> {
            unsafe {
                while WaitForSingleObject(self.event, 50) != WAIT_OBJECT_0 {
                    if give_up() {
                        CancelIoEx(self.pipe, ov);
                        let mut n = 0;
                        GetOverlappedResult(self.pipe, ov, &mut n, 1);
                        return None;
                    }
                }
                let mut n = 0;
                (GetOverlappedResult(self.pipe, ov, &mut n, 0) != 0).then_some(n)
            }
        }

        /// Takes one client and one message from it, at most `MAX_MESSAGE` bytes.
        pub fn receive_one(self, give_up: &dyn Fn() -> bool) -> Option<Vec<u8>> {
            // SAFETY: overlapped calls on our own pipe; `ov` and `buf` outlive each call
            // because `finish` waits for (or cancels and then waits for) its end.
            unsafe {
                let mut ov: OVERLAPPED = std::mem::zeroed();
                ov.hEvent = self.event;
                if ConnectNamedPipe(self.pipe, &mut ov) == 0 {
                    match GetLastError() {
                        ERROR_PIPE_CONNECTED => {}
                        ERROR_IO_PENDING => {
                            self.finish(&mut ov, give_up)?;
                        }
                        _ => return None,
                    }
                }
                // One byte more than allowed, so a message that is too big is seen.
                let mut buf = vec![0u8; MAX_MESSAGE + 1];
                let mut ov: OVERLAPPED = std::mem::zeroed();
                ov.hEvent = self.event;
                let mut n = 0u32;
                let read = ReadFile(
                    self.pipe,
                    buf.as_mut_ptr(),
                    buf.len() as u32,
                    &mut n,
                    &mut ov,
                );
                if read == 0 {
                    if GetLastError() != ERROR_IO_PENDING {
                        // Includes ERROR_MORE_DATA: too big.
                        return None;
                    }
                    n = self.finish(&mut ov, give_up)?;
                } else {
                    let mut done = 0;
                    if GetOverlappedResult(self.pipe, &ov, &mut done, 1) != 0 {
                        n = done;
                    }
                }
                buf.truncate(n as usize);
                Some(buf)
            }
        }
    }

    /// Opens the pipe for writing (identification only: the server can't act as us) and
    /// writes the message in one go.
    pub fn send(channel: &str, msg: &[u8]) -> std::io::Result<()> {
        let mut pipe = OpenOptions::new()
            .write(true)
            .security_qos_flags(SECURITY_IDENTIFICATION)
            .open(channel)?;
        pipe.write_all(msg)?;
        pipe.flush()
    }
}

#[cfg(unix)]
mod sys {
    use super::{MAX_MESSAGE, PREFIX};
    use std::io::{Read, Write};
    use std::os::unix::fs::DirBuilderExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    /// The socket and its private folder, removed when dropped.
    pub struct Listener {
        dir: PathBuf,
        listener: UnixListener,
    }

    impl Drop for Listener {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(self.dir.join("login.sock"));
            let _ = std::fs::remove_dir(&self.dir);
        }
    }

    impl Listener {
        pub fn create(id: &str) -> std::io::Result<(String, Listener)> {
            // Socket paths are limited to about 100 bytes (104 on macOS), and macOS's
            // per-user temp folder is long: use /tmp then (the 0700 folder still protects it).
            let name = format!("{PREFIX}{id}");
            let mut dir = std::env::temp_dir().join(&name);
            if dir.join("login.sock").as_os_str().len() > 100 {
                dir = PathBuf::from("/tmp").join(&name);
            }
            // A new folder only this user can enter; fails if the name is taken.
            std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
            let path = dir.join("login.sock");
            let listener = match UnixListener::bind(&path) {
                Ok(l) => l,
                Err(e) => {
                    let _ = std::fs::remove_dir(&dir);
                    return Err(e);
                }
            };
            listener.set_nonblocking(true)?;
            let name = path.to_string_lossy().into_owned();
            Ok((name, Listener { dir, listener }))
        }

        pub fn receive_one(self, give_up: &dyn Fn() -> bool) -> Option<Vec<u8>> {
            let stream = loop {
                match self.listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if give_up() {
                            return None;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Err(_) => return None,
                }
            };
            // Read without blocking, giving up after 5 s or on `give_up`. No socket options:
            // macOS refuses them (EINVAL) once the sender has shut its end, which `roster`
            // does right after writing. On macOS the accepted socket inherits non-blocking
            // from the listener, on Linux it doesn't: set it either way.
            stream.set_nonblocking(true).ok()?;
            let read_until = Instant::now() + Duration::from_secs(5);
            // One byte more than allowed, so a message that is too big is seen.
            let mut stream = stream.take(MAX_MESSAGE as u64 + 1);
            let mut buf = vec![];
            let mut chunk = [0u8; 4096];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => return Some(buf),
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if give_up() || Instant::now() >= read_until {
                            return None;
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => return None,
                }
            }
        }
    }

    pub fn send(channel: &str, msg: &[u8]) -> std::io::Result<()> {
        let mut s = UnixStream::connect(channel)?;
        s.write_all(msg)?;
        s.shutdown(std::net::Shutdown::Write)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINK: &str = "https://claude.com/cai/oauth/authorize?code=true&client_id=x&redirect_uri=http%3A%2F%2Flocalhost%3A50123%2Fcallback";
    const NONCE: &str = "0123456789abcdef0123456789abcdef";

    fn far() -> Instant {
        Instant::now() + Duration::from_secs(10)
    }

    #[test]
    fn message_table() {
        let ok = message(NONCE, LINK);
        assert_eq!(parse_message(&ok, NONCE).as_deref(), Some(LINK));
        let other_nonce = "f".repeat(32);
        let table: Vec<(Vec<u8>, &str)> =
            vec![
            (ok.clone(), &other_nonce),
            (message(NONCE, "https://evil.example.com/"), NONCE),
            (message(NONCE, "https://auth.openai.com/oauth/authorize"), NONCE),
            (message(NONCE, "http://localhost:5000/callback"), NONCE),
            (message(NONCE, "https://claude.com@evil.example.com/"), NONCE),
            (message("", LINK), NONCE),
            (b"not json".to_vec(), NONCE),
            (b"[1,2]".to_vec(), NONCE),
            (
                json!({"nonce": NONCE, "url": LINK, "extra": 1})
                    .to_string()
                    .into_bytes(),
                NONCE,
            ),
            (json!({"nonce": NONCE}).to_string().into_bytes(), NONCE),
            (
                json!({"nonce": NONCE, "url": format!("{LINK}&pad={}", "a".repeat(MAX_MESSAGE))})
                    .to_string()
                    .into_bytes(),
                NONCE,
            ),
        ];
        for (bytes, nonce) in table {
            assert_eq!(
                parse_message(&bytes, nonce),
                None,
                "{}",
                String::from_utf8_lossy(&bytes[..bytes.len().min(120)])
            );
        }
    }

    #[test]
    fn channel_names_must_have_the_exact_shape() {
        let ch = LinkChannel::create().unwrap();
        assert!(valid_channel_name(ch.name()), "{}", ch.name());
        assert!(is_token(ch.nonce()));
        for bad in [
            "",
            "C:\\Windows\\System32\\config",
            "/etc/passwd",
            r"\\.\pipe\agent-roster-login-short",
            r"\\server\pipe\agent-roster-login-0123456789abcdef0123456789abcdef",
            r"\\.\pipe\other-0123456789abcdef0123456789abcdef",
            "/tmp/agent-roster-login-0123456789abcdef0123456789abcdef/other.sock",
            "/tmp/other/login.sock",
            "agent-roster-login-0123456789abcdef0123456789abcdef/login.sock",
        ] {
            assert!(!valid_channel_name(bad), "{bad}");
        }
    }

    #[test]
    fn a_link_sent_with_the_nonce_arrives() {
        let ch = LinkChannel::create().unwrap();
        let (name, nonce) = (ch.name().to_string(), ch.nonce().to_string());
        let t = std::thread::spawn(move || ch.receive(far(), &AtomicBool::new(false)));
        std::thread::sleep(Duration::from_millis(100));
        send_link(&name, &nonce, LINK).unwrap();
        assert_eq!(t.join().unwrap().as_deref(), Some(LINK));
    }

    /// `roster` sends, shuts its end and exits before the app accepts: the link must still be
    /// read (macOS refuses socket options on such a connection).
    #[test]
    fn a_link_sent_and_closed_before_the_receive_arrives() {
        let ch = LinkChannel::create().unwrap();
        send_link(ch.name(), ch.nonce(), LINK).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            ch.receive(far(), &AtomicBool::new(false)).as_deref(),
            Some(LINK)
        );
    }

    /// A client that connects and never sends doesn't hold the channel past `stop`.
    #[cfg(unix)]
    #[test]
    fn a_silent_client_is_dropped_on_stop() {
        let ch = LinkChannel::create().unwrap();
        let _client = std::os::unix::net::UnixStream::connect(ch.name()).unwrap();
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let s = stop.clone();
        let t = std::thread::spawn(move || ch.receive(far(), &s));
        std::thread::sleep(Duration::from_millis(200));
        stop.store(true, Ordering::SeqCst);
        let t0 = Instant::now();
        assert_eq!(t.join().unwrap(), None);
        assert!(t0.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_wrong_nonce_is_refused_and_the_channel_takes_one_message() {
        let ch = LinkChannel::create().unwrap();
        let name = ch.name().to_string();
        let nonce = ch.nonce().to_string();
        let t = std::thread::spawn(move || ch.receive(far(), &AtomicBool::new(false)));
        std::thread::sleep(Duration::from_millis(100));
        send_link(&name, &"0".repeat(32), LINK).unwrap();
        assert_eq!(t.join().unwrap(), None);
        // Closed now: the right nonce comes too late.
        assert!(send_link(&name, &nonce, LINK).is_err());
    }

    #[test]
    fn too_big_a_message_is_refused() {
        let ch = LinkChannel::create().unwrap();
        let name = ch.name().to_string();
        let nonce = ch.nonce().to_string();
        let t = std::thread::spawn(move || ch.receive(far(), &AtomicBool::new(false)));
        std::thread::sleep(Duration::from_millis(100));
        let big = format!("{LINK}&pad={}", "a".repeat(MAX_MESSAGE));
        let _ = sys::send(&name, &message(&nonce, &big));
        assert_eq!(t.join().unwrap(), None);
    }

    #[test]
    fn the_channel_expires_and_stops() {
        let ch = LinkChannel::create().unwrap();
        let t0 = Instant::now();
        assert_eq!(
            ch.receive(
                Instant::now() + Duration::from_millis(200),
                &AtomicBool::new(false)
            ),
            None
        );
        assert!(t0.elapsed() < Duration::from_secs(3));
        let ch = LinkChannel::create().unwrap();
        let stop = AtomicBool::new(true);
        assert_eq!(ch.receive(far(), &stop), None);
    }

    #[cfg(windows)]
    #[test]
    fn a_pipe_name_already_taken_is_not_reused() {
        let (name, _first) = sys::Listener::create(&"a".repeat(32)).unwrap();
        assert!(name.ends_with(&"a".repeat(32)));
        assert!(sys::Listener::create(&"a".repeat(32)).is_err());
    }

    fn env_of(vars: Vec<(&'static str, String)>) -> impl Fn(&str) -> Option<OsString> {
        move |k| {
            vars.iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| OsString::from(v))
        }
    }

    fn args(a: &[&str]) -> Vec<OsString> {
        a.iter().map(OsString::from).collect()
    }

    #[test]
    fn handoff_is_skipped_without_the_variables() {
        assert_eq!(browser_handoff(&|_| None, &args(&["--help"])), None);
    }

    #[test]
    fn handoff_refuses_anything_but_one_claude_link() {
        let ch = LinkChannel::create().unwrap();
        let env = env_of(vec![
            (CHANNEL_VAR, ch.name().to_string()),
            (NONCE_VAR, ch.nonce().to_string()),
        ]);
        for a in [
            vec![],
            vec!["list"],
            vec![LINK, LINK],
            vec!["https://evil.example.com/"],
            vec!["https://auth.openai.com/oauth/authorize"],
            vec!["--json"],
        ] {
            assert_eq!(browser_handoff(&env, &args(&a)), Some(2), "{a:?}");
        }
        // Only one of the two variables: still refused, never the normal CLI.
        let half = env_of(vec![(CHANNEL_VAR, ch.name().to_string())]);
        assert_eq!(browser_handoff(&half, &args(&[LINK])), Some(2));
        let half = env_of(vec![(NONCE_VAR, ch.nonce().to_string())]);
        assert_eq!(browser_handoff(&half, &args(&[LINK])), Some(2));
        // A channel that isn't ours.
        let other = env_of(vec![
            (CHANNEL_VAR, "/etc/passwd".to_string()),
            (NONCE_VAR, ch.nonce().to_string()),
        ]);
        assert_eq!(browser_handoff(&other, &args(&[LINK])), Some(2));
        // Nothing reached the channel.
        assert_eq!(
            ch.receive(
                Instant::now() + Duration::from_millis(200),
                &AtomicBool::new(false)
            ),
            None
        );
    }

    #[test]
    fn handoff_sends_one_claude_link() {
        let ch = LinkChannel::create().unwrap();
        let env = env_of(vec![
            (CHANNEL_VAR, ch.name().to_string()),
            (NONCE_VAR, ch.nonce().to_string()),
        ]);
        let t = std::thread::spawn(move || ch.receive(far(), &AtomicBool::new(false)));
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(browser_handoff(&env, &args(&[LINK])), Some(0));
        assert_eq!(t.join().unwrap().as_deref(), Some(LINK));
    }
}
