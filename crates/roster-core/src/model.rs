//! The roster file: who the agents are.

use crate::paths::{RosterPaths, expand_tilde};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Claude,
    Codex,
}

impl Tool {
    pub fn display(&self) -> &'static str {
        match self {
            Tool::Claude => "Claude Code",
            Tool::Codex => "Codex",
        }
    }
    pub fn binary(&self) -> &'static str {
        match self {
            Tool::Claude => "claude",
            Tool::Codex => "codex",
        }
    }
    /// The environment variable that points the tool at a config folder.
    pub fn dir_var(&self) -> &'static str {
        match self {
            Tool::Claude => "CLAUDE_CONFIG_DIR",
            Tool::Codex => "CODEX_HOME",
        }
    }
    /// The folder the tool uses when the variable is unset.
    pub fn standard_dir(&self) -> &'static str {
        match self {
            Tool::Claude => "~/.claude",
            Tool::Codex => "~/.codex",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentColor {
    Cobalt,
    Marigold,
    Jade,
    Orchid,
    Coral,
}

impl AgentColor {
    pub const ORDER: [AgentColor; 5] =
        [AgentColor::Cobalt, AgentColor::Marigold, AgentColor::Jade, AgentColor::Orchid, AgentColor::Coral];

    pub fn label(&self) -> &'static str {
        match self {
            AgentColor::Cobalt => "cobalt",
            AgentColor::Marigold => "marigold",
            AgentColor::Jade => "jade",
            AgentColor::Orchid => "orchid",
            AgentColor::Coral => "coral",
        }
    }

    pub fn hex(&self) -> &'static str {
        match self {
            AgentColor::Cobalt => "#4d7cff",
            AgentColor::Marigold => "#f0b429",
            AgentColor::Jade => "#2fbf8f",
            AgentColor::Orchid => "#c06bf0",
            AgentColor::Coral => "#ff7a6b",
        }
    }
    pub fn rgb(&self) -> (u8, u8, u8) {
        let h = &self.hex()[1..];
        let v = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0);
        (v(0), v(2), v(4))
    }
    pub fn parse(s: &str) -> Option<AgentColor> {
        match s.to_ascii_lowercase().as_str() {
            "cobalt" | "blue" => Some(AgentColor::Cobalt),
            "marigold" | "yellow" | "amber" => Some(AgentColor::Marigold),
            "jade" | "green" => Some(AgentColor::Jade),
            "orchid" | "purple" | "violet" => Some(AgentColor::Orchid),
            "coral" | "red" | "orange" => Some(AgentColor::Coral),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LimitsSource {
    CodexSessions,
    ClaudeStatusline,
    Off,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub tool: Tool,
    pub config_dir: String,
    pub color: AgentColor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub share: bool,
    pub limits: LimitsSource,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub advanced_links: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatuslineChain {
    pub settings: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Roster {
    pub version: u32,
    #[serde(default, rename = "agent")]
    pub agents: Vec<Agent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub statusline: Vec<StatuslineChain>,
}

impl Default for Roster {
    fn default() -> Self {
        Roster { version: 1, agents: vec![], statusline: vec![] }
    }
}

#[derive(Debug)]
pub enum ModelError {
    Io(std::io::Error),
    Parse(String),
    UnsupportedVersion(u32),
    DuplicateName(String),
    DuplicateId(String),
    TwoDefaults(Tool),
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelError::Io(e) => write!(f, "could not read or write the roster file: {e}"),
            ModelError::Parse(e) => write!(f, "the roster file is not valid: {e}"),
            ModelError::UnsupportedVersion(v) => {
                write!(f, "the roster file is version {v}, which this version of roster can't read. Update roster.")
            }
            ModelError::DuplicateName(n) => write!(f, "two agents are named {n}"),
            ModelError::DuplicateId(i) => write!(f, "two agents share the id {i}"),
            ModelError::TwoDefaults(t) => write!(f, "two {} agents are marked default", t.display()),
        }
    }
}
impl std::error::Error for ModelError {}
impl From<std::io::Error> for ModelError {
    fn from(e: std::io::Error) -> Self {
        ModelError::Io(e)
    }
}

#[derive(Deserialize)]
struct VersionProbe {
    version: u32,
}

impl Roster {
    pub fn load(p: &RosterPaths) -> Result<Roster, ModelError> {
        let path = p.roster_file();
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Roster::default()),
            Err(e) => return Err(e.into()),
        };
        let probe: VersionProbe = toml::from_str(&text).map_err(|e| ModelError::Parse(e.to_string()))?;
        if probe.version != 1 {
            return Err(ModelError::UnsupportedVersion(probe.version));
        }
        let r: Roster = toml::from_str(&text).map_err(|e| ModelError::Parse(e.to_string()))?;
        r.validate()?;
        Ok(r)
    }

    pub fn save(&self, p: &RosterPaths) -> Result<(), ModelError> {
        self.validate()?;
        let text = toml::to_string_pretty(self).map_err(|e| ModelError::Parse(e.to_string()))?;
        let path = p.roster_file();
        std::fs::create_dir_all(path.parent().expect("roster file has a parent"))?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn to_toml_string(&self) -> Result<String, ModelError> {
        self.validate()?;
        toml::to_string_pretty(self).map_err(|e| ModelError::Parse(e.to_string()))
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        let mut names = std::collections::HashSet::new();
        let mut ids = std::collections::HashSet::new();
        for a in &self.agents {
            if !names.insert(a.name.to_ascii_lowercase()) {
                return Err(ModelError::DuplicateName(a.name.clone()));
            }
            if !ids.insert(a.id.clone()) {
                return Err(ModelError::DuplicateId(a.id.clone()));
            }
        }
        for tool in [Tool::Claude, Tool::Codex] {
            if self.agents.iter().filter(|a| a.tool == tool && a.default).count() > 1 {
                return Err(ModelError::TwoDefaults(tool));
            }
        }
        Ok(())
    }

    pub fn by_name(&self, n: &str) -> Option<&Agent> {
        self.agents.iter().find(|a| a.name.eq_ignore_ascii_case(n))
    }
    pub fn by_id(&self, id: &str) -> Option<&Agent> {
        self.agents.iter().find(|a| a.id == id)
    }
    pub fn config_dir_of(&self, a: &Agent, p: &RosterPaths) -> PathBuf {
        expand_tilde(&a.config_dir, &p.home)
    }
    /// True when the agent uses the tool's standard folder, so the env var must be unset.
    pub fn is_adopted_default(&self, a: &Agent, p: &RosterPaths) -> bool {
        self.config_dir_of(a, p) == expand_tilde(a.tool.standard_dir(), &p.home)
    }
    /// First color not used yet, in the fixed order.
    pub fn next_color(&self) -> AgentColor {
        AgentColor::ORDER
            .into_iter()
            .find(|c| !self.agents.iter().any(|a| a.color == *c))
            .unwrap_or(AgentColor::ORDER[self.agents.len() % 5])
    }
}

pub fn new_id() -> String {
    let n: u32 = rand::random();
    format!("{:06x}", n & 0x00ff_ffff)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::{Platform, RosterPaths};

    fn tmp() -> (tempfile::TempDir, RosterPaths) {
        let d = tempfile::tempdir().unwrap();
        let p = RosterPaths::for_home(d.path(), Platform::Mac, &|_| None);
        (d, p)
    }

    fn agent(id: &str, name: &str, tool: Tool, default: bool) -> Agent {
        Agent {
            id: id.into(),
            name: name.into(),
            tool,
            config_dir: format!("~/.agent-roster/accounts/{id}"),
            color: AgentColor::Cobalt,
            role: None,
            default,
            share: false,
            limits: LimitsSource::Off,
            advanced_links: vec![],
        }
    }

    fn roster(agents: Vec<Agent>) -> Roster {
        Roster { version: 1, agents, statusline: vec![] }
    }

    #[test]
    fn round_trip() {
        let (_d, p) = tmp();
        let mut a = agent("a1b2c3", "boss", Tool::Claude, true);
        a.config_dir = "~/.claude".into();
        a.role = Some("plans work".into());
        a.limits = LimitsSource::ClaudeStatusline;
        let r = roster(vec![a, agent("d4e5f6", "worker2", Tool::Codex, false)]);
        r.save(&p).unwrap();
        assert_eq!(Roster::load(&p).unwrap(), r);
    }

    #[test]
    fn missing_file_is_empty() {
        let (_d, p) = tmp();
        assert!(Roster::load(&p).unwrap().agents.is_empty());
    }

    #[test]
    fn rejects_duplicate_names() {
        let r = roster(vec![agent("a", "boss", Tool::Claude, false), agent("b", "boss", Tool::Codex, false)]);
        assert!(matches!(r.validate(), Err(ModelError::DuplicateName(n)) if n == "boss"));
    }

    #[test]
    fn rejects_two_defaults_per_tool() {
        let r = roster(vec![agent("a", "x", Tool::Claude, true), agent("b", "y", Tool::Claude, true)]);
        assert!(matches!(r.validate(), Err(ModelError::TwoDefaults(Tool::Claude))));
    }

    #[test]
    fn rejects_unknown_version() {
        let (_d, p) = tmp();
        std::fs::create_dir_all(&p.config_dir).unwrap();
        std::fs::write(p.roster_file(), "version = 2\n").unwrap();
        assert!(matches!(Roster::load(&p), Err(ModelError::UnsupportedVersion(2))));
    }

    #[test]
    fn color_hex() {
        assert_eq!(AgentColor::Cobalt.hex(), "#4d7cff");
        assert_eq!(AgentColor::Coral.hex(), "#ff7a6b");
    }

    #[test]
    fn toml_uses_spec_layout() {
        let (_d, p) = tmp();
        roster(vec![agent("a1b2c3", "boss", Tool::Codex, false)]).save(&p).unwrap();
        let text = std::fs::read_to_string(p.roster_file()).unwrap();
        assert!(text.contains("version = 1"));
        assert!(text.contains("[[agent]]"));
        assert!(text.contains("tool = \"codex\""));
        assert!(text.contains("limits = \"off\""));
    }
}
