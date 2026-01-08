from dataclasses import dataclass
from pathlib import Path
import subprocess
from typing import List, Optional


@dataclass
class WorktreeInfo:
    task_id: str
    path: Path
    branch: str


class WorktreeManager:
    def __init__(self, repo_path: Path):
        self.repo_path = Path(repo_path)
        if not (self.repo_path / ".git").exists():
            raise ValueError(f"Not a git repository: {repo_path}")

    def create_worktree(self, task_id: str, base_branch: str, worktree_dir: Path) -> Path:
        worktree_path = worktree_dir / task_id

        if worktree_path.exists():
            raise ValueError(f"Worktree path already exists: {worktree_path}")

        result = subprocess.run(
            ["git", "worktree", "add", "-b", task_id, str(worktree_path), base_branch],
            cwd=self.repo_path,
            capture_output=True,
            text=True
        )

        if result.returncode != 0:
            raise RuntimeError(f"Failed to create worktree: {result.stderr}")

        return worktree_path

    def list_worktrees(self) -> List[WorktreeInfo]:
        result = subprocess.run(
            ["git", "worktree", "list", "--porcelain"],
            cwd=self.repo_path,
            capture_output=True,
            text=True,
            check=True
        )

        worktrees = []
        current_worktree = {}

        for line in result.stdout.split("\n"):
            if line.startswith("worktree "):
                if current_worktree:
                    worktrees.append(self._parse_worktree_entry(current_worktree))
                current_worktree = {"path": line.split(" ", 1)[1]}
            elif line.startswith("branch "):
                current_worktree["branch"] = line.split(" ", 1)[1].replace("refs/heads/", "")
            elif line.startswith("HEAD "):
                current_worktree["head"] = line.split(" ", 1)[1]

        if current_worktree:
            worktrees.append(self._parse_worktree_entry(current_worktree))

        return worktrees

    def _parse_worktree_entry(self, entry: dict) -> WorktreeInfo:
        path = Path(entry["path"])
        branch = entry.get("branch", "")
        task_id = path.name if path != self.repo_path else ""

        return WorktreeInfo(
            task_id=task_id,
            path=path,
            branch=branch
        )

    def remove_worktree(self, task_id: str, worktree_dir: Path):
        worktree_path = worktree_dir / task_id

        if not worktree_path.exists():
            raise ValueError(f"Worktree does not exist: {worktree_path}")

        result = subprocess.run(
            ["git", "worktree", "remove", str(worktree_path)],
            cwd=self.repo_path,
            capture_output=True,
            text=True
        )

        if result.returncode != 0:
            result_force = subprocess.run(
                ["git", "worktree", "remove", "--force", str(worktree_path)],
                cwd=self.repo_path,
                capture_output=True,
                text=True
            )
            if result_force.returncode != 0:
                raise RuntimeError(f"Failed to remove worktree: {result_force.stderr}")

    def worktree_exists(self, task_id: str, worktree_dir: Path) -> bool:
        worktree_path = worktree_dir / task_id
        return worktree_path.exists()

    def get_worktree_info(self, task_id: str, worktree_dir: Path) -> Optional[WorktreeInfo]:
        worktrees = self.list_worktrees()

        for worktree in worktrees:
            if worktree.task_id == task_id:
                return worktree

        return None
