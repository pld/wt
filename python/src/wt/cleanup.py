from enum import Enum
from pathlib import Path
from typing import Dict
from wt.worktree_manager import WorktreeManager


class CleanupMode(Enum):
    AUTO = "auto"
    MANUAL = "manual"
    KEEP_ON_ERROR = "keep-on-error"


class CleanupOrchestrator:
    def __init__(self, worktree_manager: WorktreeManager):
        self.worktree_manager = worktree_manager

    def cleanup_worktree(
        self,
        task_id: str,
        worktree_dir: Path,
        mode: CleanupMode,
        task_failed: bool
    ):
        if mode == CleanupMode.MANUAL:
            return

        if mode == CleanupMode.KEEP_ON_ERROR and task_failed:
            return

        if self.worktree_manager.worktree_exists(task_id, worktree_dir):
            self.worktree_manager.remove_worktree(task_id, worktree_dir)

    def cleanup_all(
        self,
        worktree_dir: Path,
        mode: CleanupMode,
        task_statuses: Dict[str, bool]
    ):
        for task_id, failed in task_statuses.items():
            self.cleanup_worktree(task_id, worktree_dir, mode, failed)
