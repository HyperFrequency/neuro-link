"""Shared fixtures for neuro-link-recursive test suite."""

from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path
from typing import Generator

import pytest

NLR_REPO = Path(__file__).resolve().parent.parent
DEFAULT_BINARY = NLR_REPO / "server" / "target" / "release" / "nlr"


def _mcp_binary() -> str:
    return os.environ.get("NLR_MCP_BINARY", str(DEFAULT_BINARY))


@pytest.fixture()
def nlr_root(tmp_path: Path) -> Path:
    """Create a temp NLR_ROOT with the minimum directory/file structure."""
    dirs = [
        "00-raw",
        "01-sorted/books",
        "01-sorted/arxiv",
        "01-sorted/medium",
        "01-sorted/huggingface",
        "01-sorted/github",
        "01-sorted/docs",
        "02-KB-main",
        "03-ontology-main/domain",
        "03-ontology-main/agent",
        "03-ontology-main/workflow",
        "04-KB-agents-workflows",
        "05-insights-gaps",
        "05-self-improvement-HITL/models",
        "05-self-improvement-HITL/hyperparameters",
        "05-self-improvement-HITL/prompts",
        "05-self-improvement-HITL/features",
        "05-self-improvement-HITL/code-changes",
        "05-self-improvement-HITL/services-integrations",
        "06-self-improvement-recursive/harness-to-harness-comms",
        "06-progress-reports",
        "07-neuro-link-task",
        "08-code-docs/my-repos",
        "08-code-docs/common-tools",
        "08-code-docs/my-forks",
        "09-business-docs",
        "config",
        "state",
        "secrets",
        "scripts",
        "skills/neuro-link",
        "hooks",
    ]
    for d in dirs:
        (tmp_path / d).mkdir(parents=True, exist_ok=True)

    # CLAUDE.md
    (tmp_path / "CLAUDE.md").write_text("# neuro-link-recursive\nTest scaffold.\n")

    # schema.md
    (tmp_path / "02-KB-main" / "schema.md").write_text(
        "---\ntitle: Schema\n---\n# Wiki Schema\nConventions here.\n"
    )
    (tmp_path / "02-KB-main" / "index.md").write_text("# Index\n")
    (tmp_path / "02-KB-main" / "log.md").write_text("# Mutation Log\n")

    # Master config
    (tmp_path / "config" / "neuro-link.md").write_text(
        "---\nversion: 1\nauto_rag: true\n---\n# Master Config\n"
    )

    # Harness config
    (tmp_path / "config" / "harness-harness-comms.md").write_text(
        "---\nversion: 1\nenabled: false\nbridge_mode: mcp2cli\n---\n# Harness Comms\n"
    )

    # State files
    (tmp_path / "state" / "heartbeat.json").write_text(
        json.dumps({"status": "ok", "last_check": "2026-01-01T00:00:00Z", "errors": []})
    )
    (tmp_path / "state" / "session_log.jsonl").write_text("")
    (tmp_path / "state" / "score_history.jsonl").write_text("")
    (tmp_path / "state" / "deviation_log.jsonl").write_text("")

    return tmp_path


class MCPClient:
    """Helper to spawn the Rust MCP server and send/receive JSON-RPC messages."""

    def __init__(self, nlr_root: Path):
        binary = _mcp_binary()
        if not Path(binary).exists():
            pytest.skip(f"MCP binary not found: {binary}")
        self.proc = subprocess.Popen(
            [binary],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env={**os.environ, "NLR_ROOT": str(nlr_root)},
            text=True,
        )
        self._id = 0

    def send(self, method: str, params: dict | None = None) -> dict:
        self._id += 1
        msg = {"jsonrpc": "2.0", "method": method, "id": self._id}
        if params is not None:
            msg["params"] = params
        line = json.dumps(msg) + "\n"
        self.proc.stdin.write(line)
        self.proc.stdin.flush()
        resp_line = self.proc.stdout.readline()
        if not resp_line:
            raise RuntimeError("MCP server closed stdout")
        return json.loads(resp_line)

    def call_tool(self, name: str, arguments: dict | None = None) -> dict:
        params = {"name": name}
        if arguments is not None:
            params["arguments"] = arguments
        return self.send("tools/call", params)

    def tool_text(self, name: str, arguments: dict | None = None) -> str:
        resp = self.call_tool(name, arguments)
        content = resp.get("result", {}).get("content", [{}])
        return content[0].get("text", "") if content else ""

    def close(self):
        if self.proc.poll() is None:
            self.proc.stdin.close()
            self.proc.wait(timeout=5)

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


@pytest.fixture()
def mcp(nlr_root: Path) -> Generator[MCPClient, None, None]:
    """Spawn an MCP server against the temp NLR_ROOT and yield a client."""
    client = MCPClient(nlr_root)
    # Send initialize
    client.send("initialize")
    yield client
    client.close()
