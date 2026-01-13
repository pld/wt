use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn setup_git_repo() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
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
fn test_which_returns_main_in_main_repo() {
    use wt::worktree_manager::get_current_worktree_name;

    let repo = setup_git_repo();
    let result = get_current_worktree_name(repo.path()).unwrap();
    assert_eq!(result, "main");
}

#[test]
fn test_which_returns_worktree_name_in_worktree() {
    use wt::worktree_manager::get_current_worktree_name;

    let repo = setup_git_repo();
    let worktree_dir = TempDir::new().unwrap();
    let worktree_path = worktree_dir.path().join("feature-xyz");

    let output = Command::new("git")
        .args(["worktree", "add", "-b", "feature-xyz"])
        .arg(&worktree_path)
        .arg("main")
        .current_dir(repo.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "Failed to create worktree: {:?}", output);

    let result = get_current_worktree_name(&worktree_path).unwrap();
    assert_eq!(result, "feature-xyz");
}

#[test]
fn test_which_fails_outside_git_repo() {
    use wt::worktree_manager::get_current_worktree_name;

    let temp_dir = TempDir::new().unwrap();
    let result = get_current_worktree_name(temp_dir.path());
    assert!(result.is_err());
}

#[test]
fn test_ensure_worktrees_in_gitignore_creates_file() {
    use wt::worktree_manager::ensure_worktrees_in_gitignore;

    let repo = setup_git_repo();
    let gitignore_path = repo.path().join(".gitignore");
    let worktree_dir = repo.path().join(".worktrees");

    assert!(!gitignore_path.exists());

    ensure_worktrees_in_gitignore(repo.path(), &worktree_dir).unwrap();

    assert!(gitignore_path.exists());
    let content = fs::read_to_string(&gitignore_path).unwrap();
    assert!(content.contains(".worktrees"));
}

#[test]
fn test_ensure_worktrees_in_gitignore_appends_to_existing() {
    use wt::worktree_manager::ensure_worktrees_in_gitignore;

    let repo = setup_git_repo();
    let gitignore_path = repo.path().join(".gitignore");
    let worktree_dir = repo.path().join(".worktrees");

    fs::write(&gitignore_path, "node_modules\n").unwrap();

    ensure_worktrees_in_gitignore(repo.path(), &worktree_dir).unwrap();

    let content = fs::read_to_string(&gitignore_path).unwrap();
    assert!(content.contains("node_modules"));
    assert!(content.contains(".worktrees"));
}

#[test]
fn test_ensure_worktrees_in_gitignore_idempotent() {
    use wt::worktree_manager::ensure_worktrees_in_gitignore;

    let repo = setup_git_repo();
    let gitignore_path = repo.path().join(".gitignore");
    let worktree_dir = repo.path().join(".worktrees");

    ensure_worktrees_in_gitignore(repo.path(), &worktree_dir).unwrap();
    ensure_worktrees_in_gitignore(repo.path(), &worktree_dir).unwrap();
    ensure_worktrees_in_gitignore(repo.path(), &worktree_dir).unwrap();

    let content = fs::read_to_string(&gitignore_path).unwrap();
    let count = content.lines().filter(|l| l.trim() == ".worktrees").count();
    assert_eq!(count, 1);
}

#[test]
fn test_check_not_in_worktree_allows_normal_path() {
    use wt::worktree_manager::check_not_in_worktree;

    let temp_dir = TempDir::new().unwrap();
    let result = check_not_in_worktree(temp_dir.path());
    assert!(result.is_ok());
}

#[test]
fn test_check_not_in_worktree_rejects_worktrees_dir() {
    use wt::worktree_manager::check_not_in_worktree;

    let temp_dir = TempDir::new().unwrap();
    let worktrees_path = temp_dir.path().join(".worktrees").join("some-worktree");
    fs::create_dir_all(&worktrees_path).unwrap();

    let result = check_not_in_worktree(&worktrees_path);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("nested"));
}

