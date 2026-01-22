use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub session: SessionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    #[serde(default = "default_panes")]
    pub panes: u8,
    #[serde(default = "default_agent_cmd")]
    pub agent_cmd: String,
    #[serde(default = "default_editor_cmd")]
    pub editor_cmd: String,
}

fn default_panes() -> u8 {
    2
}

fn default_agent_cmd() -> String {
    "claude".to_string()
}

fn default_editor_cmd() -> String {
    "nvim".to_string()
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            panes: default_panes(),
            agent_cmd: default_agent_cmd(),
            editor_cmd: default_editor_cmd(),
        }
    }
}

impl Config {
    /// Load config with precedence: .wt.toml > ~/.wt/config.toml > defaults
    pub fn load() -> Self {
        let mut config = Config::default();

        // Load global config from ~/.wt/config.toml
        if let Some(home) = dirs::home_dir() {
            let global_path = home.join(".wt").join("config.toml");
            if let Ok(contents) = std::fs::read_to_string(&global_path) {
                if let Ok(global_config) = toml::from_str::<Config>(&contents) {
                    config = global_config;
                }
            }
        }

        // Load repo-local config from .wt.toml (overrides global)
        if let Ok(contents) = std::fs::read_to_string(".wt.toml") {
            if let Ok(local_config) = toml::from_str::<Config>(&contents) {
                config.merge(local_config);
            }
        }

        config
    }

    /// Load config for a specific repo path
    pub fn load_for_repo(repo_path: &Path) -> Self {
        let mut config = Config::default();

        // Load global config from ~/.wt/config.toml
        if let Some(home) = dirs::home_dir() {
            let global_path = home.join(".wt").join("config.toml");
            if let Ok(contents) = std::fs::read_to_string(&global_path) {
                if let Ok(global_config) = toml::from_str::<Config>(&contents) {
                    config = global_config;
                }
            }
        }

        // Load repo-local config from .wt.toml (overrides global)
        let local_path = repo_path.join(".wt.toml");
        if let Ok(contents) = std::fs::read_to_string(&local_path) {
            if let Ok(local_config) = toml::from_str::<Config>(&contents) {
                config.merge(local_config);
            }
        }

        config
    }

    fn merge(&mut self, other: Config) {
        self.session = other.session;
    }

    /// Get effective pane count (flag override if provided)
    pub fn effective_panes(&self, flag_override: Option<u8>) -> u8 {
        flag_override.unwrap_or(self.session.panes).clamp(2, 3)
    }

    /// Ensure ~/.wt directory exists
    pub fn ensure_wt_dir() -> Result<std::path::PathBuf> {
        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        let wt_dir = home.join(".wt");
        std::fs::create_dir_all(&wt_dir)?;
        Ok(wt_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.session.panes, 2);
        assert_eq!(config.session.agent_cmd, "claude");
        assert_eq!(config.session.editor_cmd, "nvim");
    }

    #[test]
    fn test_effective_panes_clamp() {
        let config = Config::default();
        assert_eq!(config.effective_panes(Some(1)), 2);
        assert_eq!(config.effective_panes(Some(2)), 2);
        assert_eq!(config.effective_panes(Some(3)), 3);
        assert_eq!(config.effective_panes(Some(4)), 3);
        assert_eq!(config.effective_panes(None), 2);
    }

    #[test]
    fn test_parse_toml() {
        let toml_str = r#"
[session]
panes = 3
agent_cmd = "aider"
editor_cmd = "vim"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.session.panes, 3);
        assert_eq!(config.session.agent_cmd, "aider");
        assert_eq!(config.session.editor_cmd, "vim");
    }

    #[test]
    fn test_partial_toml() {
        let toml_str = r#"
[session]
panes = 3
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.session.panes, 3);
        assert_eq!(config.session.agent_cmd, "claude");
        assert_eq!(config.session.editor_cmd, "nvim");
    }
}
