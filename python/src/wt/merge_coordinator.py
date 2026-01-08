from dataclasses import dataclass
from enum import Enum
from pathlib import Path
import subprocess
from typing import Optional


class MergeStrategy(Enum):
    SQUASH = "squash"
    REBASE = "rebase"
    MANUAL = "manual"


class MergeStatus(Enum):
    SUCCESS = "success"
    CONFLICTS = "conflicts"
    MANUAL_REQUIRED = "manual_required"
    FAILED = "failed"


@dataclass
class MergeResult:
    status: MergeStatus
    has_conflicts: bool
    message: str


class MergeCoordinator:
    def __init__(self, repo_path: Path):
        self.repo_path = Path(repo_path)
        if not (self.repo_path / ".git").exists():
            raise ValueError(f"Not a git repository: {repo_path}")

    def merge(self, branch: str, target_branch: str, strategy: MergeStrategy) -> MergeResult:
        if strategy == MergeStrategy.MANUAL:
            return MergeResult(
                status=MergeStatus.MANUAL_REQUIRED,
                has_conflicts=False,
                message="Manual merge required"
            )

        subprocess.run(
            ["git", "checkout", target_branch],
            cwd=self.repo_path,
            check=True,
            capture_output=True
        )

        if strategy == MergeStrategy.SQUASH:
            return self._squash_merge(branch, target_branch)
        elif strategy == MergeStrategy.REBASE:
            return self._rebase_merge(branch, target_branch)
        else:
            return MergeResult(
                status=MergeStatus.FAILED,
                has_conflicts=False,
                message=f"Unknown merge strategy: {strategy}"
            )

    def _squash_merge(self, branch: str, target_branch: str) -> MergeResult:
        result = subprocess.run(
            ["git", "merge", "--squash", branch],
            cwd=self.repo_path,
            capture_output=True,
            text=True
        )

        if result.returncode != 0:
            if "conflict" in result.stdout.lower() or "conflict" in result.stderr.lower():
                return MergeResult(
                    status=MergeStatus.CONFLICTS,
                    has_conflicts=True,
                    message=f"Merge conflicts detected: {result.stderr}"
                )
            return MergeResult(
                status=MergeStatus.FAILED,
                has_conflicts=False,
                message=f"Merge failed: {result.stderr}"
            )

        commit_result = subprocess.run(
            ["git", "commit", "-m", f"Merge {branch} (squashed)"],
            cwd=self.repo_path,
            capture_output=True,
            text=True
        )

        if commit_result.returncode != 0:
            return MergeResult(
                status=MergeStatus.FAILED,
                has_conflicts=False,
                message=f"Commit failed: {commit_result.stderr}"
            )

        return MergeResult(
            status=MergeStatus.SUCCESS,
            has_conflicts=False,
            message=f"Successfully merged {branch} into {target_branch}"
        )

    def _rebase_merge(self, branch: str, target_branch: str) -> MergeResult:
        result = subprocess.run(
            ["git", "merge", "--ff-only", branch],
            cwd=self.repo_path,
            capture_output=True,
            text=True
        )

        if result.returncode != 0:
            merge_result = subprocess.run(
                ["git", "merge", branch],
                cwd=self.repo_path,
                capture_output=True,
                text=True
            )

            if merge_result.returncode != 0:
                if "conflict" in merge_result.stdout.lower() or "conflict" in merge_result.stderr.lower():
                    return MergeResult(
                        status=MergeStatus.CONFLICTS,
                        has_conflicts=True,
                        message=f"Merge conflicts detected: {merge_result.stderr}"
                    )
                return MergeResult(
                    status=MergeStatus.FAILED,
                    has_conflicts=False,
                    message=f"Merge failed: {merge_result.stderr}"
                )

        return MergeResult(
            status=MergeStatus.SUCCESS,
            has_conflicts=False,
            message=f"Successfully merged {branch} into {target_branch}"
        )

    def check_for_conflicts(self, branch: str, target_branch: str) -> bool:
        result = subprocess.run(
            ["git", "merge-tree",
             self.get_merge_base(branch, target_branch),
             branch,
             target_branch],
            cwd=self.repo_path,
            capture_output=True,
            text=True
        )

        return "<<<<<<<" in result.stdout or "conflict" in result.stdout.lower()

    def get_merge_base(self, branch1: str, branch2: str) -> str:
        result = subprocess.run(
            ["git", "merge-base", branch1, branch2],
            cwd=self.repo_path,
            capture_output=True,
            text=True,
            check=True
        )

        return result.stdout.strip()
