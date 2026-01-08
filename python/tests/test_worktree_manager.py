import pytest
from pathlib import Path
import subprocess
from wt.worktree_manager import WorktreeManager, WorktreeInfo


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


class TestWorktreeManager:
    def test_create_worktree(self, git_repo, tmp_path):
        manager = WorktreeManager(git_repo)
        worktree_dir = tmp_path / "worktrees"
        worktree_dir.mkdir()

        worktree_path = manager.create_worktree("test-feature", "main", worktree_dir)

        assert worktree_path.exists()
        assert (worktree_path / "README.md").exists()
        assert worktree_path.name == "test-feature"

    def test_create_worktree_from_branch(self, git_repo, tmp_path):
        subprocess.run(["git", "checkout", "-b", "develop"], cwd=git_repo, check=True, capture_output=True)
        subprocess.run(["git", "checkout", "main"], cwd=git_repo, check=True, capture_output=True)

        manager = WorktreeManager(git_repo)
        worktree_dir = tmp_path / "worktrees"
        worktree_dir.mkdir()

        worktree_path = manager.create_worktree("test-feature", "develop", worktree_dir)

        assert worktree_path.exists()

    def test_list_worktrees(self, git_repo, tmp_path):
        manager = WorktreeManager(git_repo)
        worktree_dir = tmp_path / "worktrees"
        worktree_dir.mkdir()

        manager.create_worktree("feature-1", "main", worktree_dir)
        manager.create_worktree("feature-2", "main", worktree_dir)

        worktrees = manager.list_worktrees()

        assert len(worktrees) >= 2
        task_ids = [w.task_id for w in worktrees if w.task_id]
        assert "feature-1" in task_ids
        assert "feature-2" in task_ids

    def test_remove_worktree(self, git_repo, tmp_path):
        manager = WorktreeManager(git_repo)
        worktree_dir = tmp_path / "worktrees"
        worktree_dir.mkdir()

        worktree_path = manager.create_worktree("test-feature", "main", worktree_dir)
        assert worktree_path.exists()

        manager.remove_worktree("test-feature", worktree_dir)

        assert not worktree_path.exists()

    def test_worktree_exists(self, git_repo, tmp_path):
        manager = WorktreeManager(git_repo)
        worktree_dir = tmp_path / "worktrees"
        worktree_dir.mkdir()

        assert not manager.worktree_exists("test-feature", worktree_dir)

        manager.create_worktree("test-feature", "main", worktree_dir)

        assert manager.worktree_exists("test-feature", worktree_dir)

    def test_get_worktree_info(self, git_repo, tmp_path):
        manager = WorktreeManager(git_repo)
        worktree_dir = tmp_path / "worktrees"
        worktree_dir.mkdir()

        worktree_path = manager.create_worktree("test-feature", "main", worktree_dir)

        info = manager.get_worktree_info("test-feature", worktree_dir)

        assert info is not None
        assert info.task_id == "test-feature"
        assert info.path == worktree_path
        assert "main" in info.branch or "test-feature" in info.branch

    def test_create_duplicate_worktree_fails(self, git_repo, tmp_path):
        manager = WorktreeManager(git_repo)
        worktree_dir = tmp_path / "worktrees"
        worktree_dir.mkdir()

        manager.create_worktree("test-feature", "main", worktree_dir)

        with pytest.raises(Exception):
            manager.create_worktree("test-feature", "main", worktree_dir)

    def test_remove_nonexistent_worktree(self, git_repo, tmp_path):
        manager = WorktreeManager(git_repo)
        worktree_dir = tmp_path / "worktrees"
        worktree_dir.mkdir()

        with pytest.raises(Exception):
            manager.remove_worktree("nonexistent", worktree_dir)

    def test_invalid_base_branch(self, git_repo, tmp_path):
        manager = WorktreeManager(git_repo)
        worktree_dir = tmp_path / "worktrees"
        worktree_dir.mkdir()

        with pytest.raises(Exception):
            manager.create_worktree("test-feature", "nonexistent-branch", worktree_dir)
