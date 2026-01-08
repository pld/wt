import pytest
from pathlib import Path
import time
from wt.agent_spawner import AgentSpawner, AgentProcess, ProcessStatus


@pytest.fixture
def test_workspace(tmp_path):
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    (workspace / "test.txt").write_text("test content")
    return workspace


class TestAgentSpawner:
    def test_spawn_simple_command(self, test_workspace):
        spawner = AgentSpawner()

        process = spawner.spawn(
            task_id="test-task",
            command="echo 'hello world'",
            workspace=test_workspace
        )

        assert process.task_id == "test-task"
        assert process.status == ProcessStatus.RUNNING

    def test_wait_for_completion(self, test_workspace):
        spawner = AgentSpawner()

        process = spawner.spawn(
            task_id="test-task",
            command="echo 'hello'",
            workspace=test_workspace
        )

        result = spawner.wait(process, timeout=5)

        assert result.exit_code == 0
        assert result.status == ProcessStatus.COMPLETED
        assert "hello" in result.output

    def test_command_failure(self, test_workspace):
        spawner = AgentSpawner()

        process = spawner.spawn(
            task_id="test-task",
            command="exit 1",
            workspace=test_workspace
        )

        result = spawner.wait(process, timeout=5)

        assert result.exit_code == 1
        assert result.status == ProcessStatus.FAILED

    def test_multiple_concurrent_processes(self, test_workspace):
        spawner = AgentSpawner()

        process1 = spawner.spawn(
            task_id="task-1",
            command="sleep 0.1 && echo 'task 1'",
            workspace=test_workspace
        )

        process2 = spawner.spawn(
            task_id="task-2",
            command="sleep 0.1 && echo 'task 2'",
            workspace=test_workspace
        )

        processes = [process1, process2]

        for proc in processes:
            result = spawner.wait(proc, timeout=5)
            assert result.status == ProcessStatus.COMPLETED

    def test_get_process_status(self, test_workspace):
        spawner = AgentSpawner()

        process = spawner.spawn(
            task_id="test-task",
            command="sleep 1",
            workspace=test_workspace
        )

        status = spawner.get_status(process)
        assert status in [ProcessStatus.RUNNING, ProcessStatus.COMPLETED]

        spawner.wait(process, timeout=5)
        status = spawner.get_status(process)
        assert status == ProcessStatus.COMPLETED

    def test_terminate_process(self, test_workspace):
        spawner = AgentSpawner()

        process = spawner.spawn(
            task_id="test-task",
            command="sleep 10",
            workspace=test_workspace
        )

        time.sleep(0.1)
        spawner.terminate(process)

        status = spawner.get_status(process)
        assert status == ProcessStatus.TERMINATED

    def test_get_all_processes(self, test_workspace):
        spawner = AgentSpawner()

        spawner.spawn("task-1", "echo 'test1'", test_workspace)
        spawner.spawn("task-2", "echo 'test2'", test_workspace)

        processes = spawner.get_all_processes()

        assert len(processes) == 2
        task_ids = [p.task_id for p in processes]
        assert "task-1" in task_ids
        assert "task-2" in task_ids

    def test_get_process_by_task_id(self, test_workspace):
        spawner = AgentSpawner()

        spawner.spawn("task-1", "echo 'test'", test_workspace)

        process = spawner.get_process("task-1")

        assert process is not None
        assert process.task_id == "task-1"

    def test_timeout_handling(self, test_workspace):
        spawner = AgentSpawner()

        process = spawner.spawn(
            task_id="test-task",
            command="sleep 10",
            workspace=test_workspace
        )

        with pytest.raises(TimeoutError):
            spawner.wait(process, timeout=0.1)

    def test_working_directory(self, test_workspace):
        spawner = AgentSpawner()

        process = spawner.spawn(
            task_id="test-task",
            command="pwd",
            workspace=test_workspace
        )

        result = spawner.wait(process, timeout=5)

        assert str(test_workspace) in result.output
