import pytest
from pathlib import Path
import subprocess
from wt.cleanup import CleanupOrchestrator, CleanupMode
from wt.worktree_manager import WorktreeManager


@pytest.fixture
def git_repo(tmp_path):
    repo_path = tmp_path / "test_repo"
    repo_path.mkdir()
    subprocess.run(["git", "init"], cwd=repo_path, check=True, capture_output=True)
    subprocess.run(["git", "config", "user.email", "test@example.com"], cwd=repo_path, check=True, capture_output=True)
    subprocess.run(["git", "config", "user.name", "Test User"], cwd=repo_path, check=True, capture_output=True)

    (repo_path / "README.md").write_text("# Test Repo\n")
    subprocess.run(["git", "add", "."], cwd=repo_path, check=True, capture_output=True)
    subprocess.run(["git", "commit", "-m", "Initial commit"], cwd=repo_path, check=True, capture_output=True)

    return repo_path


class TestCleanupOrchestrator:
    def test_auto_cleanup_successful_task(self, git_repo, tmp_path):
        worktree_dir = tmp_path / "worktrees"
        worktree_dir.mkdir()

        manager = WorktreeManager(git_repo)
        worktree_path = manager.create_worktree("test-task", "main", worktree_dir)

        cleanup = CleanupOrchestrator(manager)
        cleanup.cleanup_worktree("test-task", worktree_dir, CleanupMode.AUTO, task_failed=False)

        assert not worktree_path.exists()

    def test_auto_cleanup_failed_task(self, git_repo, tmp_path):
        worktree_dir = tmp_path / "worktrees"
        worktree_dir.mkdir()

        manager = WorktreeManager(git_repo)
        worktree_path = manager.create_worktree("test-task", "main", worktree_dir)

        cleanup = CleanupOrchestrator(manager)
        cleanup.cleanup_worktree("test-task", worktree_dir, CleanupMode.AUTO, task_failed=True)

        assert not worktree_path.exists()

    def test_keep_on_error_mode_successful_task(self, git_repo, tmp_path):
        worktree_dir = tmp_path / "worktrees"
        worktree_dir.mkdir()

        manager = WorktreeManager(git_repo)
        worktree_path = manager.create_worktree("test-task", "main", worktree_dir)

        cleanup = CleanupOrchestrator(manager)
        cleanup.cleanup_worktree("test-task", worktree_dir, CleanupMode.KEEP_ON_ERROR, task_failed=False)

        assert not worktree_path.exists()

    def test_keep_on_error_mode_failed_task(self, git_repo, tmp_path):
        worktree_dir = tmp_path / "worktrees"
        worktree_dir.mkdir()

        manager = WorktreeManager(git_repo)
        worktree_path = manager.create_worktree("test-task", "main", worktree_dir)

        cleanup = CleanupOrchestrator(manager)
        cleanup.cleanup_worktree("test-task", worktree_dir, CleanupMode.KEEP_ON_ERROR, task_failed=True)

        assert worktree_path.exists()

    def test_manual_cleanup_mode(self, git_repo, tmp_path):
        worktree_dir = tmp_path / "worktrees"
        worktree_dir.mkdir()

        manager = WorktreeManager(git_repo)
        worktree_path = manager.create_worktree("test-task", "main", worktree_dir)

        cleanup = CleanupOrchestrator(manager)
        cleanup.cleanup_worktree("test-task", worktree_dir, CleanupMode.MANUAL, task_failed=False)

        assert worktree_path.exists()

    def test_cleanup_all_worktrees(self, git_repo, tmp_path):
        worktree_dir = tmp_path / "worktrees"
        worktree_dir.mkdir()

        manager = WorktreeManager(git_repo)
        manager.create_worktree("task-1", "main", worktree_dir)
        manager.create_worktree("task-2", "main", worktree_dir)

        cleanup = CleanupOrchestrator(manager)
        task_statuses = {
            "task-1": False,
            "task-2": False
        }

        cleanup.cleanup_all(worktree_dir, CleanupMode.AUTO, task_statuses)

        assert not (worktree_dir / "task-1").exists()
        assert not (worktree_dir / "task-2").exists()

    def test_cleanup_all_with_mixed_statuses(self, git_repo, tmp_path):
        worktree_dir = tmp_path / "worktrees"
        worktree_dir.mkdir()

        manager = WorktreeManager(git_repo)
        manager.create_worktree("task-1", "main", worktree_dir)
        manager.create_worktree("task-2", "main", worktree_dir)

        cleanup = CleanupOrchestrator(manager)
        task_statuses = {
            "task-1": False,
            "task-2": True
        }

        cleanup.cleanup_all(worktree_dir, CleanupMode.KEEP_ON_ERROR, task_statuses)

        assert not (worktree_dir / "task-1").exists()
        assert (worktree_dir / "task-2").exists()
