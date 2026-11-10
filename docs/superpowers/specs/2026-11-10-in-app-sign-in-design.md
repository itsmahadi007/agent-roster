# In-app sign-in: brainstorm and plan

Status: draft for review (2026-11-10). Owner request: "All the login, sign in, then two-factor
authentication, all should be done in this Agent Roster. ... this will be an option. Some users
might want to open it from the browser."

## 1. What we want

Clicking **Sign in** on an agent opens an **Agent Roster window** that shows the real Claude or
ChatGPT/OpenAI login page. The person types their email, password, email code and 2FA code there.
No default browser, and no terminal window either. When the login finishes, the window closes
by itself and the agent's row fills in (usage shows within seconds, as in 0.3.4).

It is an **option**: *Sign in with: My browser* (today's behaviour) or *Agent Roster window*.
Each login window also has a "Use my browser instead" link. **Which one is the default is the
owner's decision** (see §7): the review recommends the browser by default, as RFC 8252 advises
for native apps, with the Agent Roster window switched on for people who want it (Sam).

## 2. What we proved today (spike, Windows, throwaway empty account folders)

| | How Agent Roster gets the login link | Where the login completes | Default browser opened? |
|---|---|---|---|
| Codex 0.167.0 | `codex app-server` → `account/login/start {type:"chatgpt"}` returns `{loginId, authUrl}` (host `auth.openai.com`); completion comes as the `account/login/completed` notification | `http://127.0.0.1:1455/auth/callback` (Codex's own local server) | No |
| Claude Code 2.1.322 | `claude auth login` with `BROWSER` set to a helper: the helper receives the link (host `claude.com`) | `http://localhost:<port>/callback` (Claude Code's own local server) | No (the helper got it) |

Both flows are OAuth with PKCE: the tool keeps the secret half (verifier), the login page sends
the person back to the tool's own local server, and the tool writes its own login. **Agent Roster
does not collect a password, a code or a token**: it only shows the pages. (An app that hosts a
web view *could* read what is typed, which is why RFC 8252 prefers the system browser; our promise
is "never collects", kept auditable by a small login-window module with no scripts and no IPC.) This keeps the project rule
"never read credential files" and adds: never read or log login URLs' query strings, never inject
script into login pages.

Not proven yet (Phase 0 below): that the login pages fully work inside the system webview
(WebView2 on Windows, WKWebView on macOS) with email codes, 2FA, Cloudflare checks and passkeys.

## 3. Options considered

**A. Agent Roster login window (recommended).** A Tauri webview window pointed at the tool's
auth link. Private (incognito) session per login, so no cookies are kept and the next login
starts clean.
- + Exactly what the owner asked. No browser, no terminal.
- + **Fixes a real multi-account trap**: in the person's own browser, an already signed-in
  claude.ai / chatgpt.com account is picked automatically, so `claude_2` silently gets
  `claude1`'s account. A fresh private window per agent always asks.
- − "Continue with Google" is blocked by Google inside embedded webviews ("This browser or app
  may not be secure"). Apple / Microsoft sign-in and passkeys may also fail in a webview,
  especially on macOS. These people must use email + password/code, or the browser option.
- − An embedded page has no browser address bar, so people can't check the site themselves.
  We show a read-only address bar with a lock and the real host, always.

**B. Device-code login** (`codex login --device-auth`: show a code, the person enters it at a
web page). Still needs a web page, and Claude Code has no device flow. Not a fit for "everything
in Agent Roster"; keep as a later fallback for Codex only.

**C. Browser (today).** Stays as the option and as the escape hatch from A.

**Not doing:** our own login form that sends the person's password to Anthropic/OpenAI. That
would make Agent Roster handle passwords and 2FA codes and break the tools' own login. Never.

## 4. How option A works

### Codex
1. App starts `codex app-server` for the agent (its `CODEX_HOME`, hidden window, process tree in
   a Job Object as today) and keeps it alive during the login.
2. `account/login/start {type:"chatgpt"}` → `authUrl`, `loginId`.
3. Open the login window at `authUrl`.
4. Wait for `account/login/completed {loginId, success, error}` (or the window being closed →
   `account/login/cancel`). Close the window, stop app-server, run a usage check.
5. Port 1455 is fixed by Codex: if another Codex login is running, start fails → show
   "Another Codex sign-in is open. Finish or close it first."

### Claude Code
1. App starts `claude auth login --claudeai` hidden, with the agent's env (`CLAUDE_CONFIG_DIR`)
   and `BROWSER=<roster.exe>` plus a one-time login channel: a named pipe / Unix socket the app
   creates (current user only), a random nonce, a short expiry, one message, bounded size.
2. `roster.exe`, when started with that channel, the right nonce and a single argument that is a
   `https://claude.com/...` (or `claude.ai`) link, sends only the link and exits. Anything else is
   refused. No file paths come from the environment. Never run elevated.
   `BROWSER` is not an official Claude Code API: if no link arrives within ~10 s, or Claude asks
   for a pasted code, fall back to the visible terminal login (never collect a pasted code).
3. App reads the link, opens the login window.
4. `claude auth login` exits 0 after the callback → close the window, run a usage check. Window
   closed early → kill the process tree.
5. SSO (`--sso`) and Console (`--console`) logins follow the same path later.

### The login window
- Title "Sign in codex_2 · Codex"; agent colour dot; read-only address bar showing the current
  host with a lock (and a warning style if not https, except the localhost callback).
- Private session per window: incognito plus its own storage folder, deleted after; tested with
  two logins at once, close/reopen and a crash (incognito alone is not proof).
- Navigation: the first page must be the expected provider origin; after that only tested
  origins (claude.com, claude.ai, anthropic.com, auth.openai.com, chatgpt.com, openai.com and
  what Phase 0 shows they need), plus the tool's exact loopback callback (host, port and path).
  Anything else (Google, Microsoft, Apple, company SSO) → "This sign-in needs your browser" and
  one click to restart the login in the browser (fresh login, no cookies carried over).
- The address bar is drawn by Agent Roster outside the page, shows the punycode host for IDNs,
  no fullscreen, no certificate-error bypass. The remote page gets zero Tauri capabilities and
  no IPC, before and after redirects.
- Closing the window really destroys it (today `main.rs` hides every closing window: limit that
  to `main`) and cancels the login (Codex `account/login/cancel`, Claude process tree killed).
- Footer: "Having trouble? Use my browser instead" (re-runs the same login in the default
  browser) and "Cancel".
- No JavaScript injected into pages; no reading page content; URLs never logged.

### The option
- Settings: *Sign in with* — Agent Roster window (default) · My browser.
- `roster login <name>` in a terminal keeps using the browser (it is the terminal path).
- The drawer's Sign in button and Add agent's sign-in step follow the setting.

## 5. Plan

**Phase 0: spike (½ day).** A throwaway Tauri window, Windows first:
claude.com login with email code; auth.openai.com with password + TOTP 2FA; one Cloudflare
check; "Continue with Google" (expect blocked: confirm the message); a passkey; window closed
mid-login (tool processes gone?); two logins in a row land in two different accounts. Record
results. Stop or change the plan if email code or 2FA fails.

**Phase 1: the login window and its safety rules, browser fallback first.** Window module
(origin allowlist, exact loopback match, address bar, no capabilities, real close, private
storage), the setting, "Use my browser instead", cancel. Failure-path tests first.

**Phase 1b: Codex in-app sign-in.** A dedicated login state machine over app-server (the usage
`Exchange` drops notifications): `account/login/start` → `authUrl`, `account/login/completed`,
`account/login/cancel`; completion/cancel races; one Codex login at a time; port 1455 busy →
clear message + device-code login as fallback; never kill an unknown listener. Confirm the
signed-in account afterwards through Codex's own `account/read`. Job Object assigned before the
child can start its own children (spawn suspended).

**Phase 2: Claude in-app sign-in.** `BROWSER` bridge in `roster.exe`, hidden `claude auth login`
runner, same window. Tests with a fake `claude` that calls `$BROWSER`.

**Phase 3: option + polish.** Settings toggle, "Use my browser instead", errors (port busy,
Google blocked hint, network), Help text, spec §6.x, README.

**Phase 4: verify on this PC and on the Mac, then release 0.4.0.** Sign in a fresh Claude and a
fresh Codex agent fully inside Agent Roster (owner types credentials + 2FA), 0 console windows,
no browser process started, usage shows after sign-in, cancel leaves no process behind,
Defender stays quiet. **Real macOS sign-in test before the release** (WKWebView differs; consider
`ASWebAuthenticationSession` there, which is the system's own sign-in sheet).

Cut from 0.4.0: Console and SSO logins in the window (browser only), exhaustive trivial-state
tests. Redact URLs, RPC errors and child output everywhere (logs, errors, crash reports).

## 7. Review (codex_1, gpt-6.1-sol, high effort, read-only) and the one decision

Blockers it raised, now in the plan above: embedded OAuth is discouraged by RFC 8252 (promise
"never collects", not "can't see"); Google/Microsoft/Apple/passkeys need the browser; "https
anywhere" would allow convincing phishing inside our window (now an origin allowlist + browser
fallback); the env-path bridge could be abused (now a one-time authenticated channel).
Important: dedicated Codex login state machine, port 1455, `BROWSER` is not an official API
(terminal fallback), login windows must really close, incognito must be tested, macOS must be
tested before release. It confirmed the Codex method names against the official app-server docs.

**Decided (owner, 2026-11-10):** default sign-in = **My browser**; *Agent Roster window* is one
click away in Settings (Sam switches it on). Phase 0 approved to start now.

## 6. Risks and open questions

1. Google/Apple sign-in inside the window (likely blocked) → clear hint + browser fallback.
2. Passkeys in WebView2 (probably work, Windows Hello) and WKWebView (likely not without
   entitlements) → test in Phase 0.
3. Cloudflare or bot checks treating the webview as a bot → test in Phase 0.
4. Terms: we show the providers' own pages unmodified; no automation of the login.
5. Defender: one more hidden child process (`claude auth login`) and a URL bridge in roster.exe;
   keep it small and stick to what 0.3.4 already does. Code signing would help most.
6. Should Agent Roster also offer sign-in during *Add agent* (one step instead of two)?
