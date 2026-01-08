from dataclasses import dataclass, asdict
from pathlib import Path
from typing import List
import yaml


@dataclass
class Task:
    id: str
    prompt: str
    agent: str

    def to_dict(self):
        return asdict(self)


@dataclass
class TaskConfig:
    base_branch: str
    tasks: List[Task]
    worktree_dir: Path = Path("../wt-trees")
    merge_strategy: str = "squash"
    cleanup: str = "auto"

    def to_dict(self):
        data = asdict(self)
        data['worktree_dir'] = str(self.worktree_dir)
        data['tasks'] = [task.to_dict() for task in self.tasks]
        return data


def parse_tasks(config_path: Path) -> TaskConfig:
    if not config_path.exists():
        raise FileNotFoundError(f"Configuration file not found: {config_path}")

    with open(config_path, 'r') as f:
        data = yaml.safe_load(f)

    if not data:
        raise ValueError("Empty configuration file")

    if 'base_branch' not in data:
        raise ValueError("Missing required field: base_branch")

    if 'tasks' not in data or not data['tasks']:
        raise ValueError("Missing or empty tasks list")

    tasks = []
    for task_data in data['tasks']:
        if 'id' not in task_data:
            raise ValueError("Task missing required field: id")
        if 'prompt' not in task_data:
            raise ValueError("Task missing required field: prompt")
        if 'agent' not in task_data:
            raise ValueError("Task missing required field: agent")

        tasks.append(Task(
            id=task_data['id'],
            prompt=task_data['prompt'],
            agent=task_data['agent']
        ))

    merge_strategy = data.get('merge_strategy', 'squash')
    valid_strategies = ['squash', 'rebase', 'manual']
    if merge_strategy not in valid_strategies:
        raise ValueError(
            f"Invalid merge_strategy: {merge_strategy}. "
            f"Must be one of: {', '.join(valid_strategies)}"
        )

    cleanup = data.get('cleanup', 'auto')
    valid_cleanup_options = ['auto', 'manual', 'keep-on-error']
    if cleanup not in valid_cleanup_options:
        raise ValueError(
            f"Invalid cleanup option: {cleanup}. "
            f"Must be one of: {', '.join(valid_cleanup_options)}"
        )

    worktree_dir = Path(data.get('worktree_dir', '../wt-trees'))

    return TaskConfig(
        base_branch=data['base_branch'],
        tasks=tasks,
        worktree_dir=worktree_dir,
        merge_strategy=merge_strategy,
        cleanup=cleanup
    )
