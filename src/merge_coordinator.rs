use anyhow::{Context, Result};
use std::process::Command;

#[derive(Debug, Clone, Copy)]
pub enum MergeStrategy {
    Squash,
    Rebase,
    Manual,
}

impl MergeStrategy {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "squash" => Some(MergeStrategy::Squash),
            "rebase" => Some(MergeStrategy::Rebase),
            "manual" => Some(MergeStrategy::Manual),
            _ => None,
        }
    }
}

pub struct MergeCoordinator {
    repo_path: std::path::PathBuf,
}

impl MergeCoordinator {
    pub fn new(repo_path: std::path::PathBuf) -> Result<Self> {
        if !repo_path.join(".git").exists() {
            anyhow::bail!("Not a git repository: {:?}", repo_path);
        }
        Ok(Self { repo_path })
    }

    pub fn merge(&self, branch: &str, target_branch: &str, strategy: MergeStrategy) -> Result<()> {
        if matches!(strategy, MergeStrategy::Manual) {
            println!("Manual merge required for branch: {}", branch);
            return Ok(());
        }

        Command::new("git")
            .args(&["checkout", target_branch])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to checkout target branch")?;

        match strategy {
            MergeStrategy::Squash => {
                let output = Command::new("git")
                    .args(&["merge", "--squash", branch])
                    .current_dir(&self.repo_path)
                    .output()?;

                if !output.status.success() {
                    anyhow::bail!("Merge failed: {}", String::from_utf8_lossy(&output.stderr));
                }

                Command::new("git")
                    .args(&["commit", "-m", &format!("Merge {} (squashed)", branch)])
                    .current_dir(&self.repo_path)
                    .output()?;
            }
            MergeStrategy::Rebase => {
                let output = Command::new("git")
                    .args(&["merge", branch])
                    .current_dir(&self.repo_path)
                    .output()?;

                if !output.status.success() {
                    anyhow::bail!("Merge failed: {}", String::from_utf8_lossy(&output.stderr));
                }
            }
            _ => {}
        }

        Ok(())
    }
}
