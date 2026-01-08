from dataclasses import dataclass, field
from pathlib import Path
from enum import Enum
import subprocess
import time
from typing import Optional, List, Dict


class ProcessStatus(Enum):
    RUNNING = "running"
    COMPLETED = "completed"
    FAILED = "failed"
    TERMINATED = "terminated"


@dataclass
class AgentProcess:
    task_id: str
    command: str
    workspace: Path
    pid: Optional[int] = None
    status: ProcessStatus = ProcessStatus.RUNNING
    exit_code: Optional[int] = None
    output: str = ""
    _process: Optional[subprocess.Popen] = field(default=None, repr=False)


class AgentSpawner:
    def __init__(self):
        self._processes: Dict[str, AgentProcess] = {}

    def spawn(self, task_id: str, command: str, workspace: Path) -> AgentProcess:
        process = subprocess.Popen(
            command,
            shell=True,
            cwd=workspace,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True
        )

        agent_process = AgentProcess(
            task_id=task_id,
            command=command,
            workspace=workspace,
            pid=process.pid,
            _process=process
        )

        self._processes[task_id] = agent_process
        return agent_process

    def wait(self, agent_process: AgentProcess, timeout: Optional[float] = None) -> AgentProcess:
        if agent_process._process is None:
            raise ValueError("Process not initialized")

        try:
            stdout, _ = agent_process._process.communicate(timeout=timeout)
            agent_process.output = stdout
            agent_process.exit_code = agent_process._process.returncode

            if agent_process.exit_code == 0:
                agent_process.status = ProcessStatus.COMPLETED
            else:
                agent_process.status = ProcessStatus.FAILED

        except subprocess.TimeoutExpired:
            raise TimeoutError(f"Process {agent_process.task_id} timed out after {timeout} seconds")

        return agent_process

    def get_status(self, agent_process: AgentProcess) -> ProcessStatus:
        if agent_process._process is None:
            return ProcessStatus.FAILED

        poll_result = agent_process._process.poll()

        if poll_result is None:
            return ProcessStatus.RUNNING
        elif agent_process.status == ProcessStatus.TERMINATED:
            return ProcessStatus.TERMINATED
        elif poll_result == 0:
            agent_process.status = ProcessStatus.COMPLETED
            agent_process.exit_code = poll_result
            return ProcessStatus.COMPLETED
        else:
            agent_process.status = ProcessStatus.FAILED
            agent_process.exit_code = poll_result
            return ProcessStatus.FAILED

    def terminate(self, agent_process: AgentProcess):
        if agent_process._process is None:
            return

        agent_process._process.terminate()
        try:
            agent_process._process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            agent_process._process.kill()

        agent_process.status = ProcessStatus.TERMINATED

    def get_all_processes(self) -> List[AgentProcess]:
        return list(self._processes.values())

    def get_process(self, task_id: str) -> Optional[AgentProcess]:
        return self._processes.get(task_id)
