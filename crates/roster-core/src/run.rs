//! Starting an agent: the environment it runs with, and `roster run`.

use crate::model::{Agent, Roster};
use crate::paths::RosterPaths;

pub type EnvChanges = Vec<(String, Option<String>)>;

/// The environment changes for running `agent`. `None` means "remove this variable".
pub fn build_env(agent: &Agent, r: &Roster, p: &RosterPaths, parent_agent: Option<&str>) -> EnvChanges {
    let dir = if r.is_adopted_default(agent, p) {
        None
    } else {
        Some(r.config_dir_of(agent, p).display().to_string())
    };
    vec![
        (agent.tool.dir_var().to_string(), dir),
        ("CLAUDECODE".to_string(), None),
        ("ROSTER_AGENT".to_string(), Some(agent.id.clone())),
        ("ROSTER_PARENT".to_string(), parent_agent.map(String::from)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Roster, Tool};
    use crate::note::tests::agent;
    use crate::paths::{Platform, RosterPaths};

    fn setup() -> (tempfile::TempDir, RosterPaths, Roster) {
        let d = tempfile::tempdir().unwrap();
        let p = RosterPaths::for_home(d.path(), Platform::Mac, &|_| None);
        let r = Roster {
            version: 1,
            agents: vec![
                agent("k1", "worker1", Tool::Claude, "~/.agent-roster/accounts/k1", None),
                agent("c3", "agent3", Tool::Codex, "~/.codex", None),
            ],
            statusline: vec![],
        };
        (d, p, r)
    }

    #[test]
    fn env_secondary_sets_dir() {
        let (_d, p, r) = setup();
        let e = build_env(&r.agents[0], &r, &p, None);
        let dir = p.home.join(".agent-roster/accounts/k1").display().to_string();
        assert!(e.contains(&("CLAUDE_CONFIG_DIR".into(), Some(dir))));
        assert!(e.contains(&("CLAUDECODE".into(), None)));
        assert!(e.contains(&("ROSTER_AGENT".into(), Some("k1".into()))));
    }

    #[test]
    fn env_adopted_default_unsets_dir() {
        let (_d, p, r) = setup();
        assert!(build_env(&r.agents[1], &r, &p, None).contains(&("CODEX_HOME".into(), None)));
    }

    #[test]
    fn env_records_parent() {
        let (_d, p, r) = setup();
        assert!(build_env(&r.agents[1], &r, &p, Some("k1")).contains(&("ROSTER_PARENT".into(), Some("k1".into()))));
        assert!(build_env(&r.agents[1], &r, &p, None).contains(&("ROSTER_PARENT".into(), None)));
    }
}
