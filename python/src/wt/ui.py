from enum import Enum
from typing import List, Dict
from rich.console import Console
from rich.table import Table
from rich.live import Live
from wt.agent_spawner import ProcessStatus


class TaskState(Enum):
    PENDING = "pending"
    RUNNING = "running"
    COMPLETED = "completed"
    FAILED = "failed"

    @staticmethod
    def from_process_status(status: ProcessStatus) -> "TaskState":
        mapping = {
            ProcessStatus.RUNNING: TaskState.RUNNING,
            ProcessStatus.COMPLETED: TaskState.COMPLETED,
            ProcessStatus.FAILED: TaskState.FAILED,
            ProcessStatus.TERMINATED: TaskState.FAILED,
        }
        return mapping[status]


class ProgressUI:
    def __init__(self):
        self.console = Console()
        self._tasks: Dict[str, Dict] = {}

    def add_task(self, task_id: str, description: str):
        self._tasks[task_id] = {
            "task_id": task_id,
            "description": description,
            "state": TaskState.PENDING
        }

    def update_task_status(self, task_id: str, state: TaskState):
        if task_id in self._tasks:
            self._tasks[task_id]["state"] = state

    def get_tasks(self) -> List[Dict]:
        return list(self._tasks.values())

    def get_summary(self) -> Dict[str, int]:
        summary = {
            "total": len(self._tasks),
            "completed": 0,
            "running": 0,
            "pending": 0,
            "failed": 0
        }

        for task in self._tasks.values():
            state = task["state"]
            if state == TaskState.COMPLETED:
                summary["completed"] += 1
            elif state == TaskState.RUNNING:
                summary["running"] += 1
            elif state == TaskState.PENDING:
                summary["pending"] += 1
            elif state == TaskState.FAILED:
                summary["failed"] += 1

        return summary

    def all_tasks_completed(self) -> bool:
        if not self._tasks:
            return False
        return all(t["state"] == TaskState.COMPLETED for t in self._tasks.values())

    def has_failed_tasks(self) -> bool:
        return any(t["state"] == TaskState.FAILED for t in self._tasks.values())

    def render_table(self) -> Table:
        table = Table(title="Task Progress")

        table.add_column("Task ID", style="cyan")
        table.add_column("Description", style="white")
        table.add_column("Status", justify="center")

        for task in self._tasks.values():
            status_style = self._get_status_style(task["state"])
            table.add_row(
                task["task_id"],
                task["description"],
                f"[{status_style}]{task['state'].value}[/{status_style}]"
            )

        return table

    def _get_status_style(self, state: TaskState) -> str:
        styles = {
            TaskState.PENDING: "yellow",
            TaskState.RUNNING: "blue",
            TaskState.COMPLETED: "green",
            TaskState.FAILED: "red"
        }
        return styles.get(state, "white")

    def display(self):
        self.console.print(self.render_table())

    def display_summary(self):
        summary = self.get_summary()
        self.console.print(f"\nTotal: {summary['total']} | "
                          f"Completed: [green]{summary['completed']}[/green] | "
                          f"Running: [blue]{summary['running']}[/blue] | "
                          f"Pending: [yellow]{summary['pending']}[/yellow] | "
                          f"Failed: [red]{summary['failed']}[/red]")
