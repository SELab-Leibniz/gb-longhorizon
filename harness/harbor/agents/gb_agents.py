"""Harbor agents for the gb-longhorizon task.

    harbor run -p harness/harbor/task --agent-import-path gb_agents:ICodeAgent -m deepseek/deepseek-flash \
        --ae DEEPSEEK_API_KEY=${DEEPSEEK_API_KEY} -y
    harbor run -p harness/harbor/task --agent-import-path gb_agents:JiuwenSwarmAgent -m deepseek/deepseek-flash \
        --ae API_KEY=${DEEPSEEK_API_KEY} -y

Both agents are pre-installed in the task image (environment/Dockerfile),
because the container has no network at run time except the model API.
`install()` therefore only checks they are there. `run()` writes Harbor's
instruction to /work/TASK.md and starts the adapter loop from
harness/agents/<name>/launch.* (baked into the image at /opt/gb-agents/),
bounded by `run_seconds` so it ends before Harbor's agent timeout.

Agent kwargs (--ak key=value):
  run_seconds     wall-clock budget for the adapter loop (default 172500 ≈ 47h55m)
  chaos_after_sec kill the in-flight agent process once, this many seconds in
                  (default 72000 = hour 20; 0 disables)
  model           model id passed to the agent (default from -m)
"""
from __future__ import annotations

import shlex
from pathlib import Path

from harbor.agents.installed.base import BaseInstalledAgent
from harbor.environments.base import BaseEnvironment
from harbor.models.agent.context import AgentContext

PROXY_ENV = {
    "HTTPS_PROXY": "http://egress:8888",
    "HTTP_PROXY": "http://egress:8888",
    "NO_PROXY": "localhost,127.0.0.1",
}


class _GbAgent(BaseInstalledAgent):
    ADAPTER_DIR = "/opt/gb-agents"

    def __init__(self, logs_dir: Path, model_name: str | None = None, run_seconds: int = 172_500,
                 chaos_after_sec: int = 0, model: str | None = None, **kwargs):
        super().__init__(logs_dir=logs_dir, model_name=model_name, **kwargs)
        self.run_seconds = int(run_seconds)
        self.chaos_after_sec = int(chaos_after_sec)
        self.model = model or self._parsed_model_name or "deepseek-flash"

    def version(self) -> str | None:
        return None

    async def _write_task(self, environment: BaseEnvironment, instruction: str) -> None:
        quoted = shlex.quote(instruction)
        await self.exec_as_root(
            environment,
            command=(f"cd /work && printf '%s\\n' {quoted} > TASK.md && "
                     "git add TASK.md QUESTIONS.md && git -c user.email=scaffold@example.invalid -c user.name=Scaffold "
                     "commit -q -m 'Add task brief and questions channel' || true"),
        )

    def _common_env(self) -> dict[str, str]:
        return {
            **PROXY_ENV,
            "GB_ARM": self.name(),
            "GB_TRAJECTORY_DIR": "/logs/agent/trajectory",
            "GB_CHAOS_AFTER_SEC": str(self.chaos_after_sec),
        }

    async def _run_adapter(self, environment: BaseEnvironment, cmd: str, env: dict[str, str]) -> None:
        # `timeout` ends the loop before Harbor's own timeout; `|| true` keeps
        # the exit code zero so the trial records a normal completion.
        await self.exec_as_root(
            environment,
            command=(f"cd /work && mkdir -p /logs/agent/trajectory && "
                     f"(timeout -s TERM {self.run_seconds} {cmd} 2>&1 | stdbuf -oL tee /logs/agent/{self.name()}.txt) || true"),
            env=env,
            cwd="/work",
        )


class ICodeAgent(_GbAgent):
    @staticmethod
    def name() -> str:
        return "icode"

    async def install(self, environment: BaseEnvironment) -> None:
        await self.exec_as_root(environment, "test -x /opt/icode/.venv/bin/icode && /opt/icode/.venv/bin/icode --help > /dev/null")

    async def run(self, instruction: str, environment: BaseEnvironment, context: AgentContext) -> None:
        await self._write_task(environment, instruction)
        env = {
            **self._common_env(),
            "ICODE_DIR": "/opt/icode",
            "ICODE_PROVIDER": self._get_env("ICODE_PROVIDER") or "deepseek-openai",
            "ICODE_MODEL": self.model,
        }
        for k in ("DEEPSEEK_API_KEY", "DEEPSEEK_BASE_URL", "ICODE_BASE_URL", "OPENAI_API_KEY"):
            v = self._get_env(k)
            if v:
                env[k] = v
        await self._run_adapter(environment, f"bash {self.ADAPTER_DIR}/icode/launch.sh", env)


class JiuwenSwarmAgent(_GbAgent):
    @staticmethod
    def name() -> str:
        return "jiuwenswarm"

    async def install(self, environment: BaseEnvironment) -> None:
        await self.exec_as_root(environment, "test -x /opt/jiuwenswarm/.venv/bin/jiuwenswarm-process")

    async def run(self, instruction: str, environment: BaseEnvironment, context: AgentContext) -> None:
        await self._write_task(environment, instruction)
        env = {
            **self._common_env(),
            "JW_DIR": "/opt/jiuwenswarm",
            "MODEL_NAME": self.model,
            "API_BASE": self._get_env("API_BASE") or "https://api.deepseek.com",
            "MODEL_PROVIDER": self._get_env("MODEL_PROVIDER") or "OpenAI",
            "ENDPOINT_PROFILE": self._get_env("ENDPOINT_PROFILE") or "deepseek",
        }
        key = self._get_env("API_KEY") or self._get_env("DEEPSEEK_API_KEY")
        if key:
            env["API_KEY"] = key
        await self._run_adapter(environment, f"/opt/jiuwenswarm/.venv/bin/python {self.ADAPTER_DIR}/jiuwenswarm/launch.py", env)
