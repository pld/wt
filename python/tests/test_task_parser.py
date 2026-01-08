import pytest
from pathlib import Path
import tempfile
import yaml
from wt.task_parser import TaskConfig, Task, parse_tasks


class TestTaskParser:
    def test_parse_simple_task_config(self, tmp_path):
        config_path = tmp_path / "tasks.yaml"
        config_path.write_text("""
base_branch: main
worktree_dir: ../trees

tasks:
  - id: feat-auth
    prompt: "Implement OAuth2 authentication"
    agent: claude-code
        """)

        config = parse_tasks(config_path)

        assert config.base_branch == "main"
        assert config.worktree_dir == Path("../trees")
        assert len(config.tasks) == 1
        assert config.tasks[0].id == "feat-auth"
        assert config.tasks[0].prompt == "Implement OAuth2 authentication"
        assert config.tasks[0].agent == "claude-code"

    def test_parse_multiple_tasks(self, tmp_path):
        config_path = tmp_path / "tasks.yaml"
        config_path.write_text("""
base_branch: main
worktree_dir: ../trees

tasks:
  - id: feat-auth
    prompt: "Implement OAuth2 authentication"
    agent: claude-code

  - id: feat-payments
    prompt: "Add Stripe integration"
    agent: aider --yes
        """)

        config = parse_tasks(config_path)

        assert len(config.tasks) == 2
        assert config.tasks[1].id == "feat-payments"
        assert config.tasks[1].agent == "aider --yes"

    def test_parse_with_merge_strategy(self, tmp_path):
        config_path = tmp_path / "tasks.yaml"
        config_path.write_text("""
base_branch: main
worktree_dir: ../trees

tasks:
  - id: feat-auth
    prompt: "Implement OAuth2"
    agent: claude-code

merge_strategy: squash
        """)

        config = parse_tasks(config_path)

        assert config.merge_strategy == "squash"

    def test_parse_with_cleanup_option(self, tmp_path):
        config_path = tmp_path / "tasks.yaml"
        config_path.write_text("""
base_branch: main
worktree_dir: ../trees

tasks:
  - id: feat-auth
    prompt: "Implement OAuth2"
    agent: claude-code

cleanup: keep-on-error
        """)

        config = parse_tasks(config_path)

        assert config.cleanup == "keep-on-error"

    def test_parse_defaults(self, tmp_path):
        config_path = tmp_path / "tasks.yaml"
        config_path.write_text("""
base_branch: main

tasks:
  - id: feat-auth
    prompt: "Implement OAuth2"
    agent: claude-code
        """)

        config = parse_tasks(config_path)

        assert config.worktree_dir == Path("../wt-trees")
        assert config.merge_strategy == "squash"
        assert config.cleanup == "auto"

    def test_parse_missing_required_field(self, tmp_path):
        config_path = tmp_path / "tasks.yaml"
        config_path.write_text("""
tasks:
  - id: feat-auth
    prompt: "Implement OAuth2"
    agent: claude-code
        """)

        with pytest.raises(ValueError, match="base_branch"):
            parse_tasks(config_path)

    def test_parse_invalid_merge_strategy(self, tmp_path):
        config_path = tmp_path / "tasks.yaml"
        config_path.write_text("""
base_branch: main

tasks:
  - id: feat-auth
    prompt: "Implement OAuth2"
    agent: claude-code

merge_strategy: invalid
        """)

        with pytest.raises(ValueError, match="merge_strategy"):
            parse_tasks(config_path)

    def test_parse_task_missing_id(self, tmp_path):
        config_path = tmp_path / "tasks.yaml"
        config_path.write_text("""
base_branch: main

tasks:
  - prompt: "Implement OAuth2"
    agent: claude-code
        """)

        with pytest.raises(ValueError, match="id"):
            parse_tasks(config_path)

    def test_parse_nonexistent_file(self):
        with pytest.raises(FileNotFoundError):
            parse_tasks(Path("/nonexistent/tasks.yaml"))

    def test_task_config_to_dict(self):
        task = Task(
            id="feat-auth",
            prompt="Implement OAuth2",
            agent="claude-code"
        )
        config = TaskConfig(
            base_branch="main",
            tasks=[task],
            worktree_dir=Path("../trees"),
            merge_strategy="squash",
            cleanup="auto"
        )

        data = config.to_dict()

        assert data["base_branch"] == "main"
        assert data["merge_strategy"] == "squash"
        assert len(data["tasks"]) == 1
