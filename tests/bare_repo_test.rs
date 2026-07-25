use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;
use wt::worktree_manager::{check_not_in_worktree, get_current_worktree_name, WorktreeManager};

/// Set up a bare repo at `<parent>/.git` with a `main` branch holding one
/// commit. Returns the `TempDir` (which is the parent dir) and the path
/// to the bare git dir.
fn setup_bare_repo() -> (TempDir, PathBuf) {
    let parent = TempDir::new().unwrap();
    let bare_dir = parent.path().join(".git");

    Command::new("git")
        .args(["init", "--bare", "-b", "main"])
        .arg(&bare_dir)
        .output()
        .unwrap();

    // Configure identity for commits via plumbing.
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(&bare_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&bare_dir)
        .output()
        .unwrap();

    // Build an initial commit via plumbing (no working tree available).
    let empty_tree_sha = run_git_capture(
        &bare_dir,
        &["mktree"],
        "", // empty stdin → empty tree
    );
    let commit_sha = run_git_capture(
        &bare_dir,
        &["commit-tree", &empty_tree_sha, "-m", "initial"],
        "",
    );

    let status = Command::new("git")
        .args(["update-ref", "refs/heads/main", &commit_sha])
        .current_dir(&bare_dir)
        .status()
        .unwrap();
    assert!(status.success());

    (parent, bare_dir)
}

fn run_git_capture(cwd: &Path, args: &[&str], stdin: &str) -> String {
    use std::io::Write;
    let mut child = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn test_worktree_manager_new_succeeds_on_bare() {
    let (_parent, bare) = setup_bare_repo();
    let manager = WorktreeManager::new(bare.clone());
    assert!(
        manager.is_ok(),
        "WorktreeManager::new on bare should succeed"
    );
}

#[test]
fn test_create_and_list_worktree_on_bare() {
    let (parent, bare) = setup_bare_repo();
    let manager = WorktreeManager::new(bare.clone()).unwrap();

    let path = manager
        .create_worktree("feat", "main", parent.path(), |_| unreachable!())
        .unwrap();

    assert_eq!(path, parent.path().join("feat"));
    assert!(path.exists());

    let worktrees = manager.list_worktrees().unwrap();
    let names: Vec<_> = worktrees
        .iter()
        .map(|w| w.task_id.clone())
        .filter(|n| !n.is_empty())
        .collect();
    assert_eq!(names, vec!["feat"]);
}

#[test]
fn test_get_current_worktree_name_on_linked_bare() {
    let (parent, bare) = setup_bare_repo();
    let manager = WorktreeManager::new(bare.clone()).unwrap();

    let path = manager
        .create_worktree("feat", "main", parent.path(), |_| unreachable!())
        .unwrap();

    let name = get_current_worktree_name(&path).unwrap();
    assert_eq!(name, "feat");
}

#[test]
fn test_check_not_in_worktree_for_bare() {
    let (parent, bare) = setup_bare_repo();
    let manager = WorktreeManager::new(bare.clone()).unwrap();

    let linked = manager
        .create_worktree("feat", "main", parent.path(), |_| unreachable!())
        .unwrap();

    assert!(check_not_in_worktree(&linked).is_err());
    assert!(check_not_in_worktree(&bare).is_ok());
    assert!(check_not_in_worktree(parent.path()).is_ok());
}

#[test]
fn test_remove_worktree_on_bare() {
    let (parent, bare) = setup_bare_repo();
    let manager = WorktreeManager::new(bare.clone()).unwrap();

    let path = manager
        .create_worktree("feat", "main", parent.path(), |_| unreachable!())
        .unwrap();
    assert!(path.exists());

    manager.remove_worktree("feat").unwrap();
    assert!(!path.exists());
}

#[test]
fn test_no_gitignore_or_worktrees_dir_created_for_bare() {
    let (parent, bare) = setup_bare_repo();
    let manager = WorktreeManager::new(bare.clone()).unwrap();

    manager
        .create_worktree("feat", "main", parent.path(), |_| unreachable!())
        .unwrap();

    assert!(!parent.path().join(".worktrees").exists());
    assert!(!parent.path().join(".gitignore").exists());
}
