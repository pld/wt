import pytest
from pathlib import Path
import subprocess
from wt.merge_coordinator import MergeCoordinator, MergeStrategy, MergeResult, MergeStatus


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


class TestMergeCoordinator:
    def test_squash_merge_no_conflicts(self, git_repo, tmp_path):
        worktree_dir = tmp_path / "worktrees"
        worktree_dir.mkdir()

        subprocess.run(["git", "worktree", "add", "-b", "feat-test", str(worktree_dir / "feat-test"), "main"],
                      cwd=git_repo, check=True, capture_output=True)

        (worktree_dir / "feat-test" / "new_file.txt").write_text("new content")
        subprocess.run(["git", "add", "."], cwd=worktree_dir / "feat-test", check=True, capture_output=True)
        subprocess.run(["git", "commit", "-m", "Add new file"], cwd=worktree_dir / "feat-test",
                      check=True, capture_output=True)

        coordinator = MergeCoordinator(git_repo)
        result = coordinator.merge("feat-test", "main", MergeStrategy.SQUASH)

        assert result.status == MergeStatus.SUCCESS
        assert not result.has_conflicts

    def test_detect_merge_conflicts(self, git_repo, tmp_path):
        worktree_dir = tmp_path / "worktrees"
        worktree_dir.mkdir()

        (git_repo / "README.md").write_text("# Modified in main\n")
        subprocess.run(["git", "add", "."], cwd=git_repo, check=True, capture_output=True)
        subprocess.run(["git", "commit", "-m", "Modify README"], cwd=git_repo, check=True, capture_output=True)

        subprocess.run(["git", "worktree", "add", "-b", "feat-test", str(worktree_dir / "feat-test"), "main~1"],
                      cwd=git_repo, check=True, capture_output=True)

        (worktree_dir / "feat-test" / "README.md").write_text("# Modified in branch\n")
        subprocess.run(["git", "add", "."], cwd=worktree_dir / "feat-test", check=True, capture_output=True)
        subprocess.run(["git", "commit", "-m", "Modify README in branch"], cwd=worktree_dir / "feat-test",
                      check=True, capture_output=True)

        coordinator = MergeCoordinator(git_repo)
        has_conflicts = coordinator.check_for_conflicts("feat-test", "main")

        assert has_conflicts

    def test_rebase_merge(self, git_repo, tmp_path):
        worktree_dir = tmp_path / "worktrees"
        worktree_dir.mkdir()

        subprocess.run(["git", "worktree", "add", "-b", "feat-test", str(worktree_dir / "feat-test"), "main"],
                      cwd=git_repo, check=True, capture_output=True)

        (worktree_dir / "feat-test" / "new_file.txt").write_text("new content")
        subprocess.run(["git", "add", "."], cwd=worktree_dir / "feat-test", check=True, capture_output=True)
        subprocess.run(["git", "commit", "-m", "Add new file"], cwd=worktree_dir / "feat-test",
                      check=True, capture_output=True)

        coordinator = MergeCoordinator(git_repo)
        result = coordinator.merge("feat-test", "main", MergeStrategy.REBASE)

        assert result.status == MergeStatus.SUCCESS
        assert not result.has_conflicts

    def test_manual_merge_strategy(self, git_repo, tmp_path):
        worktree_dir = tmp_path / "worktrees"
        worktree_dir.mkdir()

        subprocess.run(["git", "worktree", "add", "-b", "feat-test", str(worktree_dir / "feat-test"), "main"],
                      cwd=git_repo, check=True, capture_output=True)

        (worktree_dir / "feat-test" / "new_file.txt").write_text("new content")
        subprocess.run(["git", "add", "."], cwd=worktree_dir / "feat-test", check=True, capture_output=True)
        subprocess.run(["git", "commit", "-m", "Add new file"], cwd=worktree_dir / "feat-test",
                      check=True, capture_output=True)

        coordinator = MergeCoordinator(git_repo)
        result = coordinator.merge("feat-test", "main", MergeStrategy.MANUAL)

        assert result.status == MergeStatus.MANUAL_REQUIRED
        assert "manual merge required" in result.message.lower()

    def test_get_merge_status(self, git_repo, tmp_path):
        worktree_dir = tmp_path / "worktrees"
        worktree_dir.mkdir()

        subprocess.run(["git", "worktree", "add", "-b", "feat-test", str(worktree_dir / "feat-test"), "main"],
                      cwd=git_repo, check=True, capture_output=True)

        coordinator = MergeCoordinator(git_repo)
        status = coordinator.get_merge_base("feat-test", "main")

        assert status is not None
