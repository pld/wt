use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Task {
    pub id: String,
    pub prompt: String,
    pub agent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskConfig {
    pub base_branch: String,
    pub tasks: Vec<Task>,
    #[serde(default = "default_worktree_dir")]
    pub worktree_dir: PathBuf,
    #[serde(default = "default_merge_strategy")]
    pub merge_strategy: String,
    #[serde(default = "default_cleanup")]
    pub cleanup: String,
}

fn default_worktree_dir() -> PathBuf {
    PathBuf::from("../wt-trees")
}

fn default_merge_strategy() -> String {
    "squash".to_string()
}

fn default_cleanup() -> String {
    "auto".to_string()
}

impl TaskConfig {
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .context(format!("Failed to read config file: {:?}", path))?;

        let config: TaskConfig = serde_yaml::from_str(&contents)
            .context("Failed to parse YAML")?;

        Self::validate(&config)?;

        Ok(config)
    }

    fn validate(config: &TaskConfig) -> Result<()> {
        if config.tasks.is_empty() {
            anyhow::bail!("No tasks defined in configuration");
        }

        let valid_strategies = ["squash", "rebase", "manual"];
        if !valid_strategies.contains(&config.merge_strategy.as_str()) {
            anyhow::bail!(
                "Invalid merge_strategy: {}. Must be one of: {}",
                config.merge_strategy,
                valid_strategies.join(", ")
            );
        }

        let valid_cleanup = ["auto", "manual", "keep-on-error"];
        if !valid_cleanup.contains(&config.cleanup.as_str()) {
            anyhow::bail!(
                "Invalid cleanup option: {}. Must be one of: {}",
                config.cleanup,
                valid_cleanup.join(", ")
            );
        }

        for task in &config.tasks {
            if task.id.is_empty() {
                anyhow::bail!("Task missing required field: id");
            }
            if task.prompt.is_empty() {
                anyhow::bail!("Task missing required field: prompt");
            }
            if task.agent.is_empty() {
                anyhow::bail!("Task missing required field: agent");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_parse_simple_task_config() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"
base_branch: main
worktree_dir: ../trees

tasks:
  - id: feat-auth
    prompt: "Implement OAuth2 authentication"
    agent: claude-code
        "#).unwrap();

        let config = TaskConfig::from_file(&file.path().to_path_buf()).unwrap();

        assert_eq!(config.base_branch, "main");
        assert_eq!(config.worktree_dir, PathBuf::from("../trees"));
        assert_eq!(config.tasks.len(), 1);
        assert_eq!(config.tasks[0].id, "feat-auth");
        assert_eq!(config.tasks[0].prompt, "Implement OAuth2 authentication");
        assert_eq!(config.tasks[0].agent, "claude-code");
    }

    #[test]
    fn test_parse_multiple_tasks() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"
base_branch: main
worktree_dir: ../trees

tasks:
  - id: feat-auth
    prompt: "Implement OAuth2 authentication"
    agent: claude-code

  - id: feat-payments
    prompt: "Add Stripe integration"
    agent: aider --yes
        "#).unwrap();

        let config = TaskConfig::from_file(&file.path().to_path_buf()).unwrap();

        assert_eq!(config.tasks.len(), 2);
        assert_eq!(config.tasks[1].id, "feat-payments");
        assert_eq!(config.tasks[1].agent, "aider --yes");
    }

    #[test]
    fn test_parse_with_merge_strategy() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"
base_branch: main

tasks:
  - id: feat-auth
    prompt: "Implement OAuth2"
    agent: claude-code

merge_strategy: squash
        "#).unwrap();

        let config = TaskConfig::from_file(&file.path().to_path_buf()).unwrap();
        assert_eq!(config.merge_strategy, "squash");
    }

    #[test]
    fn test_parse_defaults() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"
base_branch: main

tasks:
  - id: feat-auth
    prompt: "Implement OAuth2"
    agent: claude-code
        "#).unwrap();

        let config = TaskConfig::from_file(&file.path().to_path_buf()).unwrap();

        assert_eq!(config.worktree_dir, PathBuf::from("../wt-trees"));
        assert_eq!(config.merge_strategy, "squash");
        assert_eq!(config.cleanup, "auto");
    }

    #[test]
    fn test_parse_invalid_merge_strategy() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"
base_branch: main

tasks:
  - id: feat-auth
    prompt: "Implement OAuth2"
    agent: claude-code

merge_strategy: invalid
        "#).unwrap();

        let result = TaskConfig::from_file(&file.path().to_path_buf());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("merge_strategy"));
    }

    #[test]
    fn test_parse_task_missing_id() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"
base_branch: main

tasks:
  - prompt: "Implement OAuth2"
    agent: claude-code
        "#).unwrap();

        let result = TaskConfig::from_file(&file.path().to_path_buf());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_nonexistent_file() {
        let result = TaskConfig::from_file(&PathBuf::from("/nonexistent/tasks.yaml"));
        assert!(result.is_err());
    }
}
