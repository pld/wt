import pytest
from wt.ui import ProgressUI, TaskState
from wt.agent_spawner import ProcessStatus


class TestProgressUI:
    def test_create_ui(self):
        ui = ProgressUI()
        assert ui is not None

    def test_add_task(self):
        ui = ProgressUI()

        ui.add_task("feat-auth", "Implement OAuth2")

        tasks = ui.get_tasks()
        assert len(tasks) == 1
        assert tasks[0]["task_id"] == "feat-auth"
        assert tasks[0]["description"] == "Implement OAuth2"
        assert tasks[0]["state"] == TaskState.PENDING

    def test_update_task_status(self):
        ui = ProgressUI()
        ui.add_task("feat-auth", "Implement OAuth2")

        ui.update_task_status("feat-auth", TaskState.RUNNING)

        tasks = ui.get_tasks()
        assert tasks[0]["state"] == TaskState.RUNNING

    def test_update_multiple_tasks(self):
        ui = ProgressUI()
        ui.add_task("feat-auth", "Implement OAuth2")
        ui.add_task("feat-payments", "Add Stripe")

        ui.update_task_status("feat-auth", TaskState.RUNNING)
        ui.update_task_status("feat-payments", TaskState.COMPLETED)

        tasks = ui.get_tasks()
        task_dict = {t["task_id"]: t for t in tasks}

        assert task_dict["feat-auth"]["state"] == TaskState.RUNNING
        assert task_dict["feat-payments"]["state"] == TaskState.COMPLETED

    def test_task_state_from_process_status(self):
        assert TaskState.from_process_status(ProcessStatus.RUNNING) == TaskState.RUNNING
        assert TaskState.from_process_status(ProcessStatus.COMPLETED) == TaskState.COMPLETED
        assert TaskState.from_process_status(ProcessStatus.FAILED) == TaskState.FAILED
        assert TaskState.from_process_status(ProcessStatus.TERMINATED) == TaskState.FAILED

    def test_get_summary(self):
        ui = ProgressUI()
        ui.add_task("task-1", "Task 1")
        ui.add_task("task-2", "Task 2")
        ui.add_task("task-3", "Task 3")

        ui.update_task_status("task-1", TaskState.COMPLETED)
        ui.update_task_status("task-2", TaskState.RUNNING)

        summary = ui.get_summary()

        assert summary["total"] == 3
        assert summary["completed"] == 1
        assert summary["running"] == 1
        assert summary["pending"] == 1
        assert summary["failed"] == 0

    def test_all_tasks_completed(self):
        ui = ProgressUI()
        ui.add_task("task-1", "Task 1")
        ui.add_task("task-2", "Task 2")

        assert not ui.all_tasks_completed()

        ui.update_task_status("task-1", TaskState.COMPLETED)
        assert not ui.all_tasks_completed()

        ui.update_task_status("task-2", TaskState.COMPLETED)
        assert ui.all_tasks_completed()

    def test_has_failed_tasks(self):
        ui = ProgressUI()
        ui.add_task("task-1", "Task 1")
        ui.add_task("task-2", "Task 2")

        assert not ui.has_failed_tasks()

        ui.update_task_status("task-1", TaskState.FAILED)
        assert ui.has_failed_tasks()
