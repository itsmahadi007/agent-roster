//! Which names an agent may have.

use crate::model::Roster;
use std::fmt;
use std::path::{Path, PathBuf};

pub const LAUNCHER_MARKER: &str = "generated-by-agent-roster";
const RESERVED: [&str; 4] = ["roster", "claude", "codex", "gemini"];

/// Finds a command on PATH. Injected so tests never depend on the real machine.
pub trait CommandLookup {
    fn find(&self, name: &str) -> Option<PathBuf>;
}

/// The real PATH, excluding Agent Roster's own launcher folder.
pub struct PathLookup {
    pub exclude: Option<PathBuf>,
}

impl CommandLookup for PathLookup {
    fn find(&self, name: &str) -> Option<PathBuf> {
        let path = std::env::var_os("PATH")?;
        let dirs: Vec<PathBuf> = std::env::split_paths(&path)
            .filter(|d| self.exclude.as_deref() != Some(d.as_path()))
            .collect();
        let joined = std::env::join_paths(dirs).ok()?;
        which::which_in(name, Some(joined), std::env::current_dir().ok()?).ok()
    }
}

/// True when the file is a launcher Agent Roster wrote (marker on line 2).
pub fn is_our_shim(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| t.lines().nth(1).map(|l| l.contains(LAUNCHER_MARKER)))
        .unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameError {
    Empty,
    StartsWithDigit { suggestion: String },
    BadChars,
    TooLong,
    Taken(String),
    CommandExists(PathBuf),
    Reserved,
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NameError::Empty => write!(f, "Give the agent a name."),
            NameError::StartsWithDigit { suggestion } => {
                write!(
                    f,
                    "A name can't start with a number, because terminals can't run it. Try {suggestion}."
                )
            }
            NameError::BadChars => write!(f, "Use only letters, numbers, - and _ in a name."),
            NameError::TooLong => write!(f, "Keep the name to 32 characters or fewer."),
            NameError::Taken(n) => write!(f, "Another agent is already named {n}."),
            NameError::CommandExists(p) => {
                write!(
                    f,
                    "Your computer already has a command with that name ({}). Pick another name.",
                    p.display()
                )
            }
            NameError::Reserved => write!(f, "That name is reserved. Pick another name."),
        }
    }
}
impl std::error::Error for NameError {}

pub fn validate_name(
    name: &str,
    roster: &Roster,
    lookup: &dyn CommandLookup,
    ignore_id: Option<&str>,
) -> Result<(), NameError> {
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.len() > 32 {
        return Err(NameError::TooLong);
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(NameError::BadChars);
    }
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        return Err(NameError::StartsWithDigit {
            suggestion: format!("a{name}"),
        });
    }
    if name.starts_with('-') {
        return Err(NameError::BadChars);
    }
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(name)) {
        return Err(NameError::Reserved);
    }
    if let Some(a) = roster.by_name(name)
        && Some(a.id.as_str()) != ignore_id
    {
        return Err(NameError::Taken(a.name.clone()));
    }
    if let Some(found) = lookup.find(name)
        && !is_our_shim(&found)
    {
        return Err(NameError::CommandExists(found));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Roster;
    use std::path::PathBuf;

    struct NoCmds;
    impl CommandLookup for NoCmds {
        fn find(&self, _: &str) -> Option<PathBuf> {
            None
        }
    }
    struct FakeCmds(Vec<(String, PathBuf)>);
    impl CommandLookup for FakeCmds {
        fn find(&self, n: &str) -> Option<PathBuf> {
            self.0.iter().find(|(k, _)| k == n).map(|(_, p)| p.clone())
        }
    }

    fn empty() -> Roster {
        Roster::default()
    }

    fn roster_with(name: &str, id: &str) -> Roster {
        let mut r = Roster::default();
        r.agents.push(crate::model::Agent {
            id: id.into(),
            name: name.into(),
            tool: crate::model::Tool::Claude,
            config_dir: "~/.claude".into(),
            color: crate::model::AgentColor::Cobalt,
            role: None,
            default: true,
            share: false,
            limits: crate::model::LimitsSource::Off,
            advanced_links: vec![],
        });
        r
    }

    #[test]
    fn plain_names_ok() {
        for n in ["boss", "worker1", "worker-2", "a_1", "Agent3"] {
            assert!(validate_name(n, &empty(), &NoCmds, None).is_ok(), "{n}");
        }
    }

    #[test]
    fn digit_start_suggests() {
        assert_eq!(
            validate_name("1", &empty(), &NoCmds, None),
            Err(NameError::StartsWithDigit {
                suggestion: "a1".into()
            })
        );
    }

    #[test]
    fn bad_chars_and_length() {
        assert_eq!(
            validate_name("my agent", &empty(), &NoCmds, None),
            Err(NameError::BadChars)
        );
        assert_eq!(
            validate_name("", &empty(), &NoCmds, None),
            Err(NameError::Empty)
        );
        assert_eq!(
            validate_name(&"a".repeat(33), &empty(), &NoCmds, None),
            Err(NameError::TooLong)
        );
    }

    #[test]
    fn existing_command_refused() {
        let cmds = FakeCmds(vec![("ls".into(), PathBuf::from("/bin/ls"))]);
        assert!(matches!(
            validate_name("ls", &empty(), &cmds, None),
            Err(NameError::CommandExists(_))
        ));
    }

    #[test]
    fn our_shim_not_a_collision() {
        let d = tempfile::tempdir().unwrap();
        let shim = d.path().join("boss");
        std::fs::write(
            &shim,
            "#!/bin/sh\n# generated-by-agent-roster id=abc123\nexec x\n",
        )
        .unwrap();
        let cmds = FakeCmds(vec![("boss".into(), shim)]);
        assert!(validate_name("boss", &empty(), &cmds, None).is_ok());
    }

    #[test]
    fn taken_by_another_agent() {
        assert_eq!(
            validate_name("boss", &roster_with("boss", "id1"), &NoCmds, None),
            Err(NameError::Taken("boss".into()))
        );
    }

    #[test]
    fn rename_to_self_ok() {
        assert!(validate_name("boss", &roster_with("boss", "id1"), &NoCmds, Some("id1")).is_ok());
    }

    #[test]
    fn reserved() {
        assert_eq!(
            validate_name("claude", &empty(), &NoCmds, None),
            Err(NameError::Reserved)
        );
        assert_eq!(
            validate_name("roster", &empty(), &NoCmds, None),
            Err(NameError::Reserved)
        );
    }
}
