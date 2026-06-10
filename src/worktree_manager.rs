use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

fn sanitize_for_path(name: &str) -> String {
    name.replace('/', "--")
}

struct ParsedWorktreeEntry {
    path: PathBuf,
    branch: Option<String>,
}

struct ParsingWorktree {
    path: PathBuf,
    branch: Option<String>,
    bare: bool,
}

fn flush_worktree(current: &mut Option<ParsingWorktree>, entries: &mut Vec<ParsedWorktreeEntry>) {
    if let Some(wt) = current.take() {
        if !wt.bare {
            entries.push(ParsedWorktreeEntry {
                path: wt.path,
                branch: wt.branch,
            });
        }
    }
}

fn parse_worktree_porcelain(stdout: &str) -> Vec<ParsedWorktreeEntry> {
    let mut entries = Vec::new();
    let mut current: Option<ParsingWorktree> = None;

    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            flush_worktree(&mut current, &mut entries);
            current = Some(ParsingWorktree {
                path: PathBuf::from(rest),
                branch: None,
                bare: false,
            });
        } else if let Some(rest) = line.strip_prefix("branch ") {
            if let Some(wt) = current.as_mut() {
                wt.branch = Some(rest.trim_start_matches("refs/heads/").to_string());
            }
        } else if line.trim() == "bare" {
            if let Some(wt) = current.as_mut() {
                wt.bare = true;
            }
        }
    }
    flush_worktree(&mut current, &mut entries);
    entries
}

fn unsanitize_from_path(name: &str) -> String {
    name.replace("--", "/")
}

fn parse_wt_copy_paths(repo_path: &Path) -> Vec<PathBuf> {
    let gitignore_path = repo_path.join(".gitignore");
    let Ok(content) = fs::read_to_string(&gitignore_path) else {
        return Vec::new();
    };

    let mut paths = Vec::new();
    let mut in_wt_copy_section = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "# wt copy" {
            in_wt_copy_section = true;
            continue;
        }
        if in_wt_copy_section {
            if trimmed.starts_with('#') || trimmed.is_empty() {
                break;
            }
            paths.push(PathBuf::from(trimmed));
        }
    }

    paths
}

fn symlink_wt_copy_files(repo_path: &Path, worktree_path: &Path) {
    for rel_path in parse_wt_copy_paths(repo_path) {
        let src = repo_path.join(&rel_path);
        let dst = worktree_path.join(&rel_path);

        if !src.exists() {
            continue;
        }

        // Create parent directories if needed
        if let Some(parent) = dst.parent() {
            let _ = fs::create_dir_all(parent);
        }

        // Create symlink (Unix)
        #[cfg(unix)]
        {
            let _ = std::os::unix::fs::symlink(&src, &dst);
        }
    }
}

pub fn ensure_worktrees_in_gitignore(repo_path: &Path, worktree_dir: &Path) -> Result<()> {
    let gitignore_path = repo_path.join(".gitignore");

    // Get the directory name relative to repo root for gitignore
    let pattern = worktree_dir
        .strip_prefix(repo_path)
        .ok()
        .and_then(|p| p.to_str())
        .unwrap_or(".worktrees");

    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path).context("Failed to read .gitignore")?;

        if content.lines().any(|line| line.trim() == pattern) {
            return Ok(());
        }

        let mut file = OpenOptions::new()
            .append(true)
            .open(&gitignore_path)
            .context("Failed to open .gitignore")?;

        if !content.is_empty() && !content.ends_with('\n') {
            file.write_all(b"\n")
                .context("Failed to write newline to .gitignore")?;
        }

        writeln!(file, "{}", pattern).context("Failed to write to .gitignore")?;
        return Ok(());
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&gitignore_path)
        .context("Failed to open .gitignore")?;

    writeln!(file, "{}", pattern).context("Failed to write to .gitignore")?;

    Ok(())
}

pub fn check_not_in_worktree(path: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(path)
        .output();

    let Ok(output) = output else {
        return Ok(());
    };
    if !output.status.success() {
        return Ok(());
    }

    let git_dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
    // A linked worktree's --git-dir always ends with /worktrees/<name>.
    // Using a suffix check avoids false positives from user paths that
    // happen to contain "/worktrees/" as a non-git directory component.
    let is_linked = git_dir
        .rsplit_once("/worktrees/")
        .map(|(_, name)| !name.is_empty() && !name.contains('/'))
        .unwrap_or(false);
    if is_linked {
        anyhow::bail!("Cannot create nested worktrees: already inside a linked worktree");
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

    // A linked worktree's --git-dir is always <common-dir>/worktrees/<name>.
    if let Some((_, name)) = git_dir.rsplit_once("/worktrees/") {
        if !name.is_empty() && !name.contains('/') {
            return Ok(name.to_string());
        }
    }
    Ok("main".to_string())
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
        let output = Command::new("git")
            .args(["rev-parse", "--git-common-dir"])
            .current_dir(&repo_path)
            .output()
            .context("Failed to execute git rev-parse")?;
        if !output.status.success() {
            anyhow::bail!("Not a git repository: {:?}", repo_path);
        }
        Ok(Self { repo_path })
    }

    pub fn create_worktree(
        &self,
        task_id: &str,
        base_branch: &str,
        worktree_dir: &Path,
        select_remote_branch: impl FnOnce(&[String]) -> Result<String>,
    ) -> Result<PathBuf> {
        // Sanitize for filesystem (/ -> --) but keep original for git
        let safe_name = sanitize_for_path(task_id);
        let worktree_path = worktree_dir.join(&safe_name);

        if worktree_path.exists() {
            let registered = self
                .list_worktrees()?
                .into_iter()
                .any(|w| w.path == worktree_path);
            if registered {
                anyhow::bail!(
                    "Worktree '{}' is already registered at {:?}",
                    task_id,
                    worktree_path
                );
            }
            anyhow::bail!(
                "Path {:?} exists but is not a registered worktree.\n\
                 Remove the directory before retrying.",
                worktree_path
            );
        }

        let mut upstream_branch: Option<String> = None;
        let output = if self.local_branch_exists(task_id) {
            // Local branch exists, just check it out
            Command::new("git")
                .args(["worktree", "add"])
                .arg(&worktree_path)
                .arg(task_id)
                .current_dir(&self.repo_path)
                .output()
                .context("Failed to execute git worktree add")?
        } else {
            let remote_branches = self.remote_branch_candidates(task_id)?;
            match remote_branches.as_slice() {
                [] => Command::new("git")
                    .args(["worktree", "add", "-b", task_id])
                    .arg(&worktree_path)
                    .arg(base_branch)
                    .current_dir(&self.repo_path)
                    .output()
                    .context("Failed to execute git worktree add")?,
                [remote_branch] => {
                    upstream_branch = Some(remote_branch.clone());
                    Command::new("git")
                        .args(["worktree", "add", "-b", task_id])
                        .arg(&worktree_path)
                        .arg(remote_branch)
                        .current_dir(&self.repo_path)
                        .output()
                        .context("Failed to execute git worktree add")?
                }

                _ => {
                    let remote_branch = select_remote_branch(&remote_branches)?;
                    upstream_branch = Some(remote_branch.clone());
                    Command::new("git")
                        .args(["worktree", "add", "-b", task_id])
                        .arg(&worktree_path)
                        .arg(&remote_branch)
                        .current_dir(&self.repo_path)
                        .output()
                        .context("Failed to execute git worktree add")?
                }
            }
        };

        if !output.status.success() {
            anyhow::bail!(
                "Failed to create worktree: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        if let Some(remote_branch) = upstream_branch {
            if let Some(remote_name) = remote_branch.split('/').next() {
                if self.remote_exists(remote_name) {
                    let output = Command::new("git")
                        .args(["branch", "--set-upstream-to", &remote_branch, task_id])
                        .current_dir(&self.repo_path)
                        .output()
                        .context("Failed to set branch upstream")?;

                    if !output.status.success() {
                        anyhow::bail!(
                            "Failed to set branch upstream: {}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                    }
                }
            }
        }

        // Set up autoSetupRemote so `git push` works without -u origin HEAD
        // (avoids "upstream is gone" warning before first push)
        Command::new("git")
            .args(["config", "push.autoSetupRemote", "true"])
            .current_dir(&worktree_path)
            .output()
            .ok();

        // Symlink files from `# wt copy` section in .gitignore
        symlink_wt_copy_files(&self.repo_path, &worktree_path);

        Ok(worktree_path)
    }

    fn local_branch_exists(&self, branch: &str) -> bool {
        Command::new("git")
            .args([
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{}", branch),
            ])
            .current_dir(&self.repo_path)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn remote_branch_candidates(&self, branch: &str) -> Result<Vec<String>> {
        let output = Command::new("git")
            .args(["for-each-ref", "--format=%(refname:short)", "refs/remotes"])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to execute git for-each-ref")?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to list remote branches: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let mut candidates: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|refname| !refname.is_empty() && !refname.ends_with("/HEAD"))
            .filter(|refname| {
                refname
                    .rsplit_once('/')
                    .map(|(_, leaf)| leaf == branch)
                    .unwrap_or(false)
            })
            .map(str::to_string)
            .collect();
        candidates.sort();

        Ok(candidates)
    }

    fn remote_exists(&self, remote: &str) -> bool {
        Command::new("git")
            .args(["config", "--get", &format!("remote.{}.url", remote)])
            .current_dir(&self.repo_path)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    pub fn list_worktrees(&self) -> Result<Vec<WorktreeInfo>> {
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to execute git worktree list")?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to list worktrees: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(
            parse_worktree_porcelain(&String::from_utf8_lossy(&output.stdout))
                .into_iter()
                .map(|entry| self.parse_worktree_entry(entry.path, entry.branch))
                .collect(),
        )
    }

    fn parse_worktree_entry(&self, path: PathBuf, branch: Option<String>) -> WorktreeInfo {
        let task_id = if path == self.repo_path {
            String::new()
        } else {
            let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // Convert filesystem name back to original (-- -> /)
            unsanitize_from_path(dir_name)
        };

        WorktreeInfo {
            task_id,
            path,
            branch: branch.unwrap_or_default(),
        }
    }

    pub fn remove_worktree(&self, task_id: &str) -> Result<()> {
        // Look up the actual path from git
        let wt_info = self
            .get_worktree_info(task_id)?
            .ok_or_else(|| anyhow::anyhow!("Worktree '{}' not found", task_id))?;

        // If path doesn't exist on disk, just prune stale entries
        if !wt_info.path.exists() {
            Command::new("git")
                .args(["worktree", "prune"])
                .current_dir(&self.repo_path)
                .output()
                .context("Failed to prune stale worktrees")?;
            return Ok(());
        }

        let output = Command::new("git")
            .args(["worktree", "remove"])
            .arg(&wt_info.path)
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to execute git worktree remove")?;

        if !output.status.success() {
            let output_force = Command::new("git")
                .args(["worktree", "remove", "--force"])
                .arg(&wt_info.path)
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

    pub fn worktree_exists(&self, task_id: &str) -> bool {
        self.get_worktree_info(task_id)
            .map(|info| info.is_some())
            .unwrap_or(false)
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
            .args(&["init", "-b", "main"])
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
            .create_worktree(
                "test-feature",
                "main",
                worktree_dir.path(),
                |_| unreachable!(),
            )
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
            .create_worktree("feature-1", "main", worktree_dir.path(), |_| unreachable!())
            .unwrap();
        manager
            .create_worktree("feature-2", "main", worktree_dir.path(), |_| unreachable!())
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
            .create_worktree(
                "test-feature",
                "main",
                worktree_dir.path(),
                |_| unreachable!(),
            )
            .unwrap();

        assert!(worktree_path.exists());

        manager.remove_worktree("test-feature").unwrap();

        assert!(!worktree_path.exists());
    }

    #[test]
    fn test_worktree_exists() {
        let repo = setup_git_repo();
        let worktree_dir = TempDir::new().unwrap();

        let manager = WorktreeManager::new(repo.path().to_path_buf()).unwrap();

        assert!(!manager.worktree_exists("test-feature"));

        manager
            .create_worktree(
                "test-feature",
                "main",
                worktree_dir.path(),
                |_| unreachable!(),
            )
            .unwrap();

        assert!(manager.worktree_exists("test-feature"));
    }

    #[test]
    fn test_get_worktree_info() {
        let repo = setup_git_repo();
        let worktree_dir = TempDir::new().unwrap();

        let manager = WorktreeManager::new(repo.path().to_path_buf()).unwrap();
        manager
            .create_worktree(
                "test-feature",
                "main",
                worktree_dir.path(),
                |_| unreachable!(),
            )
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
            .create_worktree(
                "test-feature",
                "main",
                worktree_dir.path(),
                |_| unreachable!(),
            )
            .unwrap();

        let result = manager.create_worktree(
            "test-feature",
            "main",
            worktree_dir.path(),
            |_| unreachable!(),
        );
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("already registered"));
    }

    #[test]
    fn test_remove_nonexistent_worktree() {
        let repo = setup_git_repo();

        let manager = WorktreeManager::new(repo.path().to_path_buf()).unwrap();
        let result = manager.remove_worktree("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_base_branch() {
        let repo = setup_git_repo();
        let worktree_dir = TempDir::new().unwrap();

        let manager = WorktreeManager::new(repo.path().to_path_buf()).unwrap();
        let result = manager.create_worktree(
            "test-feature",
            "nonexistent-branch",
            worktree_dir.path(),
            |_| unreachable!(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_create_worktree_for_existing_branch() {
        let repo = setup_git_repo();
        let worktree_dir = TempDir::new().unwrap();

        // Create a branch first
        Command::new("git")
            .args(["branch", "existing-feature"])
            .current_dir(repo.path())
            .output()
            .unwrap();

        let manager = WorktreeManager::new(repo.path().to_path_buf()).unwrap();
        let worktree_path = manager
            .create_worktree(
                "existing-feature",
                "main",
                worktree_dir.path(),
                |_| unreachable!(),
            )
            .unwrap();

        assert!(worktree_path.exists());
        assert!(worktree_path.join("README.md").exists());

        // Verify we're on the existing branch
        let output = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&worktree_path)
            .output()
            .unwrap();
        let branch = String::from_utf8_lossy(&output.stdout);
        assert_eq!(branch.trim(), "existing-feature");
    }

    #[test]
    fn test_create_worktree_for_remote_branch() {
        let repo = setup_git_repo();
        let worktree_dir = TempDir::new().unwrap();

        let head = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(repo.path())
            .output()
            .unwrap();
        let commit = String::from_utf8_lossy(&head.stdout).trim().to_string();

        Command::new("git")
            .args(["update-ref", "refs/remotes/origin/remote-feature", &commit])
            .current_dir(repo.path())
            .output()
            .unwrap();

        let manager = WorktreeManager::new(repo.path().to_path_buf()).unwrap();
        let worktree_path = manager
            .create_worktree(
                "remote-feature",
                "main",
                worktree_dir.path(),
                |_| unreachable!(),
            )
            .unwrap();

        assert!(worktree_path.exists());

        let output = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&worktree_path)
            .output()
            .unwrap();
        let branch = String::from_utf8_lossy(&output.stdout);
        assert_eq!(branch.trim(), "remote-feature");
    }

    #[test]
    fn test_create_worktree_prompts_for_ambiguous_remote_branch() {
        let repo = setup_git_repo();
        let worktree_dir = TempDir::new().unwrap();

        let head = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(repo.path())
            .output()
            .unwrap();
        let commit = String::from_utf8_lossy(&head.stdout).trim().to_string();

        Command::new("git")
            .args(["update-ref", "refs/remotes/origin/shared-feature", &commit])
            .current_dir(repo.path())
            .output()
            .unwrap();
        Command::new("git")
            .args([
                "update-ref",
                "refs/remotes/upstream/shared-feature",
                &commit,
            ])
            .current_dir(repo.path())
            .output()
            .unwrap();

        let manager = WorktreeManager::new(repo.path().to_path_buf()).unwrap();
        let mut seen_candidates = Vec::new();
        let worktree_path = manager
            .create_worktree("shared-feature", "main", worktree_dir.path(), |remotes| {
                seen_candidates = remotes.to_vec();
                Ok(remotes[1].clone())
            })
            .unwrap();

        assert_eq!(
            seen_candidates,
            vec![
                "origin/shared-feature".to_string(),
                "upstream/shared-feature".to_string(),
            ]
        );

        let output = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&worktree_path)
            .output()
            .unwrap();
        let branch = String::from_utf8_lossy(&output.stdout);
        assert_eq!(branch.trim(), "shared-feature");
    }

    #[test]
    fn test_branch_name_with_slashes() {
        let repo = setup_git_repo();
        let worktree_dir = TempDir::new().unwrap();

        let manager = WorktreeManager::new(repo.path().to_path_buf()).unwrap();

        // Create worktree with slash in name
        let worktree_path = manager
            .create_worktree(
                "feature/auth",
                "main",
                worktree_dir.path(),
                |_| unreachable!(),
            )
            .unwrap();

        // Directory should use sanitized name (-- instead of /)
        assert!(worktree_path.exists());
        assert!(worktree_path.ends_with("feature--auth"));

        // Listing should return original name with slashes
        let worktrees = manager.list_worktrees().unwrap();
        let wt = worktrees.iter().find(|w| w.task_id == "feature/auth");
        assert!(wt.is_some(), "Should find worktree by original name");

        // get_worktree_info should work with original name
        let info = manager.get_worktree_info("feature/auth").unwrap();
        assert!(info.is_some());
        assert_eq!(info.unwrap().task_id, "feature/auth");

        // Remove should work with original name
        manager.remove_worktree("feature/auth").unwrap();
        assert!(!worktree_path.exists());
    }

    #[test]
    fn test_parse_porcelain_drops_bare_entries() {
        let porcelain = "\
worktree /home/user/projects/todolist
bare

worktree /home/user/projects/todolist/feat
HEAD abc123
branch refs/heads/feat
";
        let entries = parse_worktree_porcelain(porcelain);
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].path,
            PathBuf::from("/home/user/projects/todolist/feat")
        );
        assert_eq!(entries[0].branch.as_deref(), Some("feat"));
    }

    #[test]
    fn test_parse_porcelain_normal_repo() {
        let porcelain = "\
worktree /home/user/projects/todolist
HEAD abc123
branch refs/heads/main

worktree /home/user/projects/todolist/.worktrees/feat
HEAD def456
branch refs/heads/feat
";
        let entries = parse_worktree_porcelain(porcelain);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].branch.as_deref(), Some("main"));
        assert_eq!(entries[1].branch.as_deref(), Some("feat"));
    }
}
