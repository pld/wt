use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn ensure_worktrees_in_gitignore(repo_path: &Path) -> Result<()> {
    let gitignore_path = repo_path.join(".gitignore");
    let pattern = ".worktrees";

    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path)
            .context("Failed to read .gitignore")?;

        if content.lines().any(|line| line.trim() == pattern) {
            return Ok(());
        }
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&gitignore_path)
        .context("Failed to open .gitignore")?;

    writeln!(file, "{}", pattern)
        .context("Failed to write to .gitignore")?;

    Ok(())
}

pub fn check_not_in_worktree(path: &Path) -> Result<()> {
    let mut current = path;
    while let Some(parent) = current.parent() {
        if current.file_name().map(|n| n == ".worktrees").unwrap_or(false) {
            anyhow::bail!("Cannot create nested worktrees: already inside a .worktrees directory");
        }
        current = parent;
    }
    Ok(())
}

pub fn get_current_worktree_name(path: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(path)
        .output()
        .context("Failed to execute git rev-parse")?;

    if !output.status.success() {
        anyhow::bail!("Not a git repository");
    }

    let git_dir = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if let Some(pos) = git_dir.find("/.git/worktrees/") {
        let worktree_name = &git_dir[pos + "/.git/worktrees/".len()..];
        Ok(worktree_name.to_string())
    } else {
        Ok("main".to_string())
    }
}

#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub task_id: String,
    pub path: PathBuf,
    pub branch: String,
}

pub struct WorktreeManager {
    repo_path: PathBuf,
}

impl WorktreeManager {
    pub fn new(repo_path: PathBuf) -> Result<Self> {
        if !repo_path.join(".git").exists() {
            anyhow::bail!("Not a git repository: {:?}", repo_path);
        }
        Ok(Self { repo_path })
    }

    pub fn create_worktree(
        &self,
        task_id: &str,
        base_branch: &str,
        worktree_dir: &Path,
    ) -> Result<PathBuf> {
        let worktree_path = worktree_dir.join(task_id);

        if worktree_path.exists() {
            anyhow::bail!("Worktree path already exists: {:?}", worktree_path);
        }

        let output = Command::new("git")
            .args(&["worktree", "add", "-b", task_id])
            .arg(&worktree_path)
            .arg(base_branch)
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to execute git worktree add")?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to create worktree: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(worktree_path)
    }

    pub fn list_worktrees(&self) -> Result<Vec<WorktreeInfo>> {
        let output = Command::new("git")
            .args(&["worktree", "list", "--porcelain"])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to execute git worktree list")?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to list worktrees: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut worktrees = Vec::new();
        let mut current_worktree: Option<(PathBuf, Option<String>)> = None;

        for line in stdout.lines() {
            if line.starts_with("worktree ") {
                if let Some((path, branch)) = current_worktree.take() {
                    worktrees.push(self.parse_worktree_entry(path, branch));
                }
                let path = PathBuf::from(line.strip_prefix("worktree ").unwrap());
                current_worktree = Some((path, None));
            } else if line.starts_with("branch ") {
                if let Some((ref _path, ref mut branch)) = current_worktree {
                    let branch_name = line
                        .strip_prefix("branch ")
                        .unwrap()
                        .trim_start_matches("refs/heads/");
                    *branch = Some(branch_name.to_string());
                }
            }
        }

        if let Some((path, branch)) = current_worktree {
            worktrees.push(self.parse_worktree_entry(path, branch));
        }

        Ok(worktrees)
    }

    fn parse_worktree_entry(&self, path: PathBuf, branch: Option<String>) -> WorktreeInfo {
        let task_id = if path == self.repo_path {
            String::new()
        } else {
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string()
        };

        WorktreeInfo {
            task_id,
            path,
            branch: branch.unwrap_or_default(),
        }
    }

    pub fn remove_worktree(&self, task_id: &str, worktree_dir: &Path) -> Result<()> {
        let worktree_path = worktree_dir.join(task_id);

        if !worktree_path.exists() {
            anyhow::bail!("Worktree does not exist: {:?}", worktree_path);
        }

        let output = Command::new("git")
            .args(&["worktree", "remove"])
            .arg(&worktree_path)
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to execute git worktree remove")?;

        if !output.status.success() {
            let output_force = Command::new("git")
                .args(&["worktree", "remove", "--force"])
                .arg(&worktree_path)
                .current_dir(&self.repo_path)
                .output()
                .context("Failed to execute git worktree remove --force")?;

            if !output_force.status.success() {
                anyhow::bail!(
                    "Failed to remove worktree: {}",
                    String::from_utf8_lossy(&output_force.stderr)
                );
            }
        }

        Ok(())
    }

    pub fn worktree_exists(&self, task_id: &str, worktree_dir: &Path) -> bool {
        worktree_dir.join(task_id).exists()
    }

    pub fn get_worktree_info(&self, task_id: &str) -> Result<Option<WorktreeInfo>> {
        let worktrees = self.list_worktrees()?;
        Ok(worktrees.into_iter().find(|w| w.task_id == task_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_git_repo() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(&["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(&["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(&["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        fs::write(repo_path.join("README.md"), "# Test Repo\n").unwrap();

        Command::new("git")
            .args(&["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(&["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        temp_dir
    }

    #[test]
    fn test_create_worktree() {
        let repo = setup_git_repo();
        let worktree_dir = TempDir::new().unwrap();

        let manager = WorktreeManager::new(repo.path().to_path_buf()).unwrap();
        let worktree_path = manager
            .create_worktree("test-feature", "main", worktree_dir.path())
            .unwrap();

        assert!(worktree_path.exists());
        assert!(worktree_path.join("README.md").exists());
    }

    #[test]
    fn test_list_worktrees() {
        let repo = setup_git_repo();
        let worktree_dir = TempDir::new().unwrap();

        let manager = WorktreeManager::new(repo.path().to_path_buf()).unwrap();
        manager
            .create_worktree("feature-1", "main", worktree_dir.path())
            .unwrap();
        manager
            .create_worktree("feature-2", "main", worktree_dir.path())
            .unwrap();

        let worktrees = manager.list_worktrees().unwrap();

        let task_ids: Vec<String> = worktrees
            .iter()
            .filter(|w| !w.task_id.is_empty())
            .map(|w| w.task_id.clone())
            .collect();

        assert!(task_ids.contains(&"feature-1".to_string()));
        assert!(task_ids.contains(&"feature-2".to_string()));
    }

    #[test]
    fn test_remove_worktree() {
        let repo = setup_git_repo();
        let worktree_dir = TempDir::new().unwrap();

        let manager = WorktreeManager::new(repo.path().to_path_buf()).unwrap();
        let worktree_path = manager
            .create_worktree("test-feature", "main", worktree_dir.path())
            .unwrap();

        assert!(worktree_path.exists());

        manager
            .remove_worktree("test-feature", worktree_dir.path())
            .unwrap();

        assert!(!worktree_path.exists());
    }

    #[test]
    fn test_worktree_exists() {
        let repo = setup_git_repo();
        let worktree_dir = TempDir::new().unwrap();

        let manager = WorktreeManager::new(repo.path().to_path_buf()).unwrap();

        assert!(!manager.worktree_exists("test-feature", worktree_dir.path()));

        manager
            .create_worktree("test-feature", "main", worktree_dir.path())
            .unwrap();

        assert!(manager.worktree_exists("test-feature", worktree_dir.path()));
    }

    #[test]
    fn test_get_worktree_info() {
        let repo = setup_git_repo();
        let worktree_dir = TempDir::new().unwrap();

        let manager = WorktreeManager::new(repo.path().to_path_buf()).unwrap();
        manager
            .create_worktree("test-feature", "main", worktree_dir.path())
            .unwrap();

        let info = manager.get_worktree_info("test-feature").unwrap();

        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.task_id, "test-feature");
        assert!(info.branch.contains("test-feature") || info.branch.contains("main"));
    }

    #[test]
    fn test_create_duplicate_worktree_fails() {
        let repo = setup_git_repo();
        let worktree_dir = TempDir::new().unwrap();

        let manager = WorktreeManager::new(repo.path().to_path_buf()).unwrap();
        manager
            .create_worktree("test-feature", "main", worktree_dir.path())
            .unwrap();

        let result = manager.create_worktree("test-feature", "main", worktree_dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_nonexistent_worktree() {
        let repo = setup_git_repo();
        let worktree_dir = TempDir::new().unwrap();

        let manager = WorktreeManager::new(repo.path().to_path_buf()).unwrap();
        let result = manager.remove_worktree("nonexistent", worktree_dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_base_branch() {
        let repo = setup_git_repo();
        let worktree_dir = TempDir::new().unwrap();

        let manager = WorktreeManager::new(repo.path().to_path_buf()).unwrap();
        let result = manager.create_worktree("test-feature", "nonexistent-branch", worktree_dir.path());
        assert!(result.is_err());
    }
}
