//! Asks Codex itself for an agent's limits: `codex app-server` speaks JSON-RPC over stdio.
//! After the `initialize` handshake, `account/rateLimits/read` returns the same numbers the
//! Codex status line shows, fetched with the agent's own login.
//!
//! STUB: the signature is final; the body is being written.

use super::{Check, Health};
use crate::model::{Agent, Roster};
use crate::paths::RosterPaths;
use std::path::Path;

/// Asks one Codex agent (its CODEX_HOME comes from `run::build_env`) for its limits and
/// account health. Never blocks longer than its timeout; never leaves a process behind.
pub fn fetch_codex_usage(
    _agent: &Agent,
    _r: &Roster,
    _p: &RosterPaths,
    _tool_path: &Path,
) -> Check {
    Check::health(Health::Unreachable)
}
