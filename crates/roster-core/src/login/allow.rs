//! Which pages the login window may show. Only https pages on the provider sites tested for
//! sign-in (and Google's own sign-in pages, for "Continue with Google"), and the tool's own
//! loopback callback. Hosts are compared exactly, after a real
//! URL parser has lowercased them and turned international names into punycode, so look-alike
//! names (`clаude.com` with a Cyrillic `а`), `user@host` tricks and odd ports never pass.

use crate::model::Tool;
use url::{Host, Url};

/// Provider hosts the login window may show (https, default port only): the ones the Phase 0
/// sign-ins on Windows went through. No wildcards: a host is added only once a real sign-in
/// has been seen to need it.
pub const PROVIDER_HOSTS: &[&str] = &[
    "claude.com",
    "claude.ai",
    "console.anthropic.com",
    "platform.claude.com",
    "auth.openai.com",
    "chatgpt.com",
];

/// Hosts a login page may open a popup on.
pub const POPUP_HOSTS: &[&str] = &["accounts.google.com"];

/// Where each tool's sign-in starts: the first page must be one of these.
fn first_hosts(tool: Tool) -> &'static [&'static str] {
    match tool {
        Tool::Claude => &["claude.com", "claude.ai"],
        Tool::Codex => &["auth.openai.com"],
    }
}

/// A URL with no user name or password in it, parsed. `None` for anything else.
fn parse(url: &str) -> Option<Url> {
    let u = Url::parse(url.trim()).ok()?;
    (u.username().is_empty() && u.password().is_none()).then_some(u)
}

/// An https page on one of `hosts`, on the default port.
fn https_on(u: &Url, hosts: &[&str]) -> bool {
    u.scheme() == "https"
        && u.port().is_none()
        && matches!(u.host(), Some(Host::Domain(h)) if hosts.contains(&h))
}

/// The tool's own loopback callback: `http://localhost:<port>/…` or `http://127.0.0.1:<port>/…`.
/// Returns the port.
fn loopback_port(u: &Url) -> Option<u16> {
    if u.scheme() != "http" {
        return None;
    }
    let loopback = match u.host()? {
        Host::Domain(h) => h == "localhost",
        Host::Ipv4(ip) => ip == std::net::Ipv4Addr::LOCALHOST,
        Host::Ipv6(_) => false,
    };
    if loopback { u.port() } else { None }
}

/// Google's own sign-in, which "Continue with Google" runs as pages in the window (seen in
/// the Phase 0 Codex sign-in): `accounts.google.com`, `accounts.youtube.com`, and Google's
/// country sign-in hosts `accounts.google.<cc>`, `accounts.google.com.<cc>` and
/// `accounts.google.co.<cc>`, where `<cc>` is two ASCII letters. Nothing else.
fn is_google_sign_in(host: &str) -> bool {
    if host == "accounts.google.com" || host == "accounts.youtube.com" {
        return true;
    }
    let Some(rest) = host.strip_prefix("accounts.google.") else {
        return false;
    };
    let cc = |s: &str| s.len() == 2 && s.bytes().all(|b| b.is_ascii_lowercase());
    match rest.split_once('.') {
        None => cc(rest),
        Some((second, top)) => matches!(second, "com" | "co") && cc(top),
    }
}

/// Is this page allowed in the login window? See the module docs.
pub fn allowed_in_window(url: &str) -> bool {
    parse(url).is_some_and(|u| {
        https_on(&u, PROVIDER_HOSTS)
            || loopback_port(&u).is_some()
            || (u.scheme() == "https"
                && u.port().is_none()
                && matches!(u.host(), Some(Host::Domain(h)) if is_google_sign_in(h)))
    })
}

/// Is this the tool's own callback on exactly `expected_port`? The window pins the port of
/// the login it shows (see `redirect_port`), so another local server can't be reached.
pub fn callback_matches(url: &str, expected_port: u16) -> bool {
    parse(url).is_some_and(|u| loopback_port(&u) == Some(expected_port))
}

/// Can a popup the login page opens be shown? Only Google sign-in (seen working in the
/// window's popup during the Phase 0 test); everything else goes to the browser.
pub fn allowed_popup(url: &str) -> bool {
    parse(url).is_some_and(|u| https_on(&u, POPUP_HOSTS))
}

/// Can a sign-in for `tool` start at this link? (https on the tool's own provider sign-in host.)
pub fn is_first_page(tool: Tool, url: &str) -> bool {
    parse(url).is_some_and(|u| https_on(&u, first_hosts(tool)))
}

/// The loopback port the provider will send the browser back to (the link's `redirect_uri`),
/// so the window can pin it with `callback_matches`. `None` when the link has no loopback
/// `redirect_uri`.
pub fn redirect_port(link: &str) -> Option<u16> {
    let u = parse(link)?;
    let redirect = u
        .query_pairs()
        .find(|(k, _)| k == "redirect_uri")
        .map(|(_, v)| v.into_owned())?;
    loopback_port(&parse(&redirect)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_table() {
        let table: &[(&str, bool)] = &[
            // Provider pages.
            (
                "https://claude.com/cai/oauth/authorize?code=true&client_id=x",
                true,
            ),
            ("https://claude.ai/login", true),
            ("https://CLAUDE.AI/login", true),
            ("https://claude.com:443/x", true),
            ("https://anthropic.com/", false),
            ("https://console.anthropic.com/oauth", true),
            ("https://platform.claude.com/oauth/code/callback", true),
            ("https://auth.openai.com/oauth/authorize?x=1", true),
            ("https://chatgpt.com/", true),
            ("https://openai.com/", false),
            ("https://auth.openai.com/log-in", true),
            ("https://claude.ai/oauth/authorize?x=1", true),
            // Loopback callbacks, any port.
            ("http://localhost:54321/callback?code=x&state=y", true),
            ("http://127.0.0.1:1455/auth/callback?code=x", true),
            ("http://LOCALHOST:8080/callback", true),
            // Not https, or not the default port.
            ("http://claude.com/login", false),
            ("https://claude.com:8443/login", false),
            ("ftp://claude.com/", false),
            ("javascript:alert(1)", false),
            ("data:text/html,hi", false),
            ("file:///C:/x.html", false),
            ("about:blank", false),
            ("", false),
            ("not a url", false),
            // Look-alikes and tricks.
            ("https://claude.com@evil.example.com/", false),
            ("https://user:pw@claude.com/", false),
            ("https://claude.com.evil.example.com/", false),
            ("https://evilclaude.com/", false),
            ("https://www.claude.com/", false),
            ("https://claude.com./", false),
            ("https://cl\u{0430}ude.com/", false),
            ("https://xn--claude-xyz.com/", false),
            ("https://login.microsoftonline.com/", false),
            ("https://appleid.apple.com/", false),
            ("https://127.0.0.1/", false),
            // Loopback must be plain http, a real loopback host, and name a port.
            ("https://localhost:1455/callback", false),
            ("http://localhost/callback", false),
            ("http://[::1]:1455/callback", false),
            ("http://localhost.evil.example.com:1455/", false),
            ("http://127.0.0.2:1455/", false),
            ("http://user@localhost:1455/", false),
            ("http://192.168.1.10:1455/", false),
        ];
        for (url, want) in table {
            assert_eq!(allowed_in_window(url), *want, "{url}");
        }
    }

    /// "Continue with Google" runs as top-level pages in the window (Phase 0, Codex).
    #[test]
    fn google_sign_in_pages_are_allowed_and_look_alikes_are_not() {
        let table: &[(&str, bool)] = &[
            (
                "https://accounts.google.com/o/oauth2/v2/auth?client_id=x",
                true,
            ),
            ("https://accounts.google.com/v3/signin/identifier", true),
            ("https://accounts.google.com/CheckCookie", true),
            ("https://accounts.youtube.com/accounts/SetSID", true),
            ("https://accounts.google.com.bd/accounts/SetSID", true),
            ("https://accounts.google.co.uk/accounts/SetSID", true),
            ("https://accounts.google.de/accounts/SetSID", true),
            ("https://ACCOUNTS.GOOGLE.COM.BD/x", true),
            ("https://accounts.google.com/signin/oauth/v3/consent", true),
            // Not Google's sign-in.
            ("http://accounts.google.com/", false),
            ("https://accounts.google.com:8443/", false),
            ("https://accounts.google.evil.com/", false),
            ("https://accounts.google.com.evil.com/", false),
            ("https://accounts.google.co.evil/", false),
            ("https://accounts.google.com.bd.evil.example.com/", false),
            ("https://accounts.google.bd.uk/", false),
            ("https://accounts.google.org.uk/", false),
            ("https://accounts.google.com.b/", false),
            ("https://accounts.google.com.bdx/", false),
            ("https://accounts.google.d3/", false),
            ("https://accounts.google.xn--p1ai/", false),
            ("https://accounts.google./", false),
            ("https://accounts.google/", false),
            ("https://accounts.googIe.com/", false),
            ("https://accounts-google.com/", false),
            ("https://evilaccounts.google.com/", false),
            ("https://mail.google.com/", false),
            ("https://google.com/", false),
            ("https://www.youtube.com/", false),
            ("https://accounts.youtube.com.bd/", false),
            ("https://accounts.google.com@evil.example.com/", false),
            ("https://login.microsoftonline.com/", false),
            ("https://appleid.apple.com/", false),
        ];
        for (url, want) in table {
            assert_eq!(allowed_in_window(url), *want, "{url}");
        }
    }

    #[test]
    fn popups_are_google_sign_in_only() {
        let table: &[(&str, bool)] = &[
            (
                "https://accounts.google.com/o/oauth2/v2/auth?client_id=x",
                true,
            ),
            ("https://ACCOUNTS.GOOGLE.COM/signin", true),
            ("http://accounts.google.com/o/oauth2/v2/auth", false),
            ("https://accounts.google.com:8443/", false),
            ("https://accounts.google.com@evil.example.com/", false),
            ("https://accounts.google.com.evil.example.com/", false),
            ("https://evil.example.com/accounts.google.com", false),
            ("https://google.com/", false),
            ("https://claude.com/", false),
            ("http://localhost:1455/callback", false),
            ("javascript:alert(1)", false),
        ];
        for (url, want) in table {
            assert_eq!(allowed_popup(url), *want, "{url}");
        }
    }

    #[test]
    fn callback_must_be_on_the_expected_port() {
        assert!(callback_matches(
            "http://127.0.0.1:1455/auth/callback?code=x",
            1455
        ));
        assert!(callback_matches("http://localhost:6000/callback", 6000));
        assert!(!callback_matches("http://localhost:6001/callback", 6000));
        assert!(!callback_matches("http://localhost/callback", 80));
        assert!(!callback_matches("https://claude.com/callback", 443));
        assert!(!callback_matches("http://evil.example.com:6000/", 6000));
    }

    #[test]
    fn first_page_is_the_tools_own_provider() {
        assert!(is_first_page(
            Tool::Claude,
            "https://claude.com/cai/oauth/authorize?x"
        ));
        assert!(is_first_page(Tool::Claude, "https://claude.ai/oauth"));
        assert!(!is_first_page(Tool::Claude, "https://auth.openai.com/x"));
        assert!(!is_first_page(
            Tool::Claude,
            "http://localhost:5000/callback"
        ));
        assert!(!is_first_page(
            Tool::Claude,
            "https://claude.com@evil.example.com/"
        ));
        assert!(is_first_page(
            Tool::Codex,
            "https://auth.openai.com/oauth/authorize?x"
        ));
        assert!(!is_first_page(Tool::Codex, "https://chatgpt.com/"));
        assert!(!is_first_page(Tool::Codex, "https://claude.com/"));
    }

    #[test]
    fn redirect_port_comes_from_a_loopback_redirect_uri() {
        assert_eq!(
            redirect_port(
                "https://auth.openai.com/oauth/authorize?response_type=code&redirect_uri=http%3A%2F%2F127.0.0.1%3A1455%2Fauth%2Fcallback&state=s"
            ),
            Some(1455)
        );
        assert_eq!(
            redirect_port(
                "https://claude.com/cai/oauth/authorize?redirect_uri=http%3A%2F%2Flocalhost%3A61234%2Fcallback"
            ),
            Some(61234)
        );
        assert_eq!(
            redirect_port(
                "https://claude.com/x?redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback"
            ),
            None
        );
        assert_eq!(redirect_port("https://claude.com/x"), None);
    }
}
