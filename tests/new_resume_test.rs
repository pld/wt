use std::process::Command;
use tempfile::TempDir;
use wt::worktree_manager::WorktreeManager;

fn setup_git_repo() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    std::fs::write(repo_path.join("README.md"), "# Test Repo\n").unwrap();

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .output()
        .unwrap();

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    temp_dir
}

#[test]
fn test_create_worktree_fresh_succeeds() {
    let repo = setup_git_repo();
    let worktree_dir = TempDir::new().unwrap();

    let manager = WorktreeManager::new(repo.path().to_path_buf()).unwrap();
    let result = manager.create_worktree(
        "fresh-feature",
        "main",
        worktree_dir.path(),
        |_| unreachable!(),
    );

    assert!(result.is_ok(), "fresh create should succeed: {:?}", result);
    let path = result.unwrap();
    assert!(path.exists());
    assert!(manager.worktree_exists("fresh-feature"));
}

#[test]
fn test_create_worktree_already_registered_errors_at_manager_layer() {
    let repo = setup_git_repo();
    let worktree_dir = TempDir::new().unwrap();

    let manager = WorktreeManager::new(repo.path().to_path_buf()).unwrap();
    manager
        .create_worktree(
            "dup-feature",
            "main",
            worktree_dir.path(),
            |_| unreachable!(),
        )
        .unwrap();

    let result = manager.create_worktree(
        "dup-feature",
        "main",
        worktree_dir.path(),
        |_| unreachable!(),
    );

    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("already registered"),
        "expected 'already registered' in error: {msg}"
    );
}

#[test]
fn test_create_worktree_orphan_directory_errors() {
    let repo = setup_git_repo();
    let worktree_dir = TempDir::new().unwrap();

    let manager = WorktreeManager::new(repo.path().to_path_buf()).unwrap();
    let path = manager
        .create_worktree(
            "orphan-feature",
            "main",
            worktree_dir.path(),
            |_| unreachable!(),
        )
        .unwrap();

    // Deregister from git but leave the directory on disk
    Command::new("git")
        .args(["worktree", "remove", "--force", path.to_str().unwrap()])
        .current_dir(repo.path())
        .output()
        .unwrap();

    // Write a marker file to prove the directory is not cleaned up on error
    let marker = path.join("user_work.txt");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(&marker, "precious work").unwrap();

    let result = manager.create_worktree(
        "orphan-feature",
        "main",
        worktree_dir.path(),
        |_| unreachable!(),
    );

    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("not a registered worktree"),
        "expected actionable error, got: {msg}"
    );
    assert!(
        msg.contains("Remove the directory"),
        "expected removal hint in error: {msg}"
    );
    // Directory must not be deleted
    assert!(marker.exists(), "orphan directory must not be auto-cleaned");
}
