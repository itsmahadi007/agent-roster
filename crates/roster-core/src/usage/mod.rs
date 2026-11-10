//! Asks each tool itself for an agent's usage, limits and account health, with no terminal
//! and no status line: Claude Code through `claude -p /usage`, Codex through
//! `codex app-server` (`account/rateLimits/read`). Agent Roster never opens a credential file.
//!
//! Parsing is shared by every platform. How a tool is found, spawned and stopped differs:
//! that lives in `windows.rs` and `unix.rs`, and `sys` picks the right one at build time.

pub mod claude;
pub mod codex;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

/// The platform-specific half: finding a tool, spawning it hidden, stopping its whole tree.
pub mod sys {
    #[cfg(unix)]
    pub use super::unix::*;
    #[cfg(windows)]
    pub use super::windows::*;
}

pub use claude::{USAGE_ARGS, UsageReply, fetch_claude_usage, parse_reset, parse_usage_reply};

use crate::limits::AgentLimits;
use serde::{Deserialize, Serialize};

/// What the last check learned about an account, beyond its numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Health {
    /// Signed in with a plan; the numbers (if any) are fresh.
    Ok,
    /// Not signed in, or the sign-in expired: the agent needs `roster login <name>`.
    SignedOut,
    /// Signed in, but no subscription (none, ended, or an API key without plan limits).
    NoSubscription,
    /// A limit is used up (100%, or the tool says so). The window says when it resets.
    LimitReached,
    /// The tool is not on this computer.
    NotInstalled,
    /// The tool could not be asked (offline, timed out, crashed). Keep the last numbers.
    Unreachable,
}

/// One check of one agent.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub health: Health,
    /// Fresh numbers. `None` when the check got none (keep showing the saved ones).
    pub limits: Option<AgentLimits>,
}

impl Check {
    pub fn health(health: Health) -> Self {
        Check {
            health,
            limits: None,
        }
    }
}
