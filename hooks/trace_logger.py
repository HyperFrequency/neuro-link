#!/usr/bin/env python3
"""Raw-trace logger for neuro-link-recursive (F2).

Invoked via Claude Code PostToolUse and Stop hooks. Reads the hook payload
from stdin and appends a scrubbed, gzipped NDJSON line to:

    $NLR_ROOT/state/traces/<session_id>/<ts>.jsonl.gz

On every invocation it also prunes files older than 30 days in the same
session directory. Exits 0 unconditionally so hooks never block.
"""

from __future__ import annotations

import gzip
import json
import os
import re
import sys
import time
from pathlib import Path

# Secret-scrub patterns (order does not matter; all applied).
_SECRET_PATTERNS = [
    re.compile(r"NLR_API_TOKEN=\S+"),
    re.compile(r"sk-[a-zA-Z0-9]{20,}"),
    re.compile(r"ghp_\S+"),
    re.compile(r"Bearer \S+"),
]
_REDACTED = "***REDACTED***"
_TTL_SECONDS = 30 * 24 * 60 * 60  # 30 days


def scrub(value):
    """Recursively redact known secret patterns in strings."""
    if isinstance(value, str):
        out = value
        for pat in _SECRET_PATTERNS:
            out = pat.sub(_REDACTED, out)
        return out
    if isinstance(value, list):
        return [scrub(v) for v in value]
    if isinstance(value, dict):
        return {k: scrub(v) for k, v in value.items()}
    return value


def _resolve_nlr_root() -> Path | None:
    env = os.environ.get("NLR_ROOT")
    if env:
        return Path(env)
    persisted = Path.home() / ".claude" / "state" / "nlr_root"
    if persisted.is_file():
        try:
            return Path(persisted.read_text().strip())
        except OSError:
            return None
    return None


def _prune(session_dir: Path) -> None:
    cutoff = time.time() - _TTL_SECONDS
    try:
        for entry in session_dir.iterdir():
            if not entry.is_file():
                continue
            try:
                if entry.stat().st_mtime < cutoff:
                    entry.unlink()
            except OSError:
                continue
    except OSError:
        return


def _write_trace(root: Path, payload: dict) -> None:
    session_id = (
        payload.get("session_id")
        or payload.get("sessionId")
        or "unknown"
    )
    # Defensively coerce & sanitise (no path traversal).
    session_id = re.sub(r"[^A-Za-z0-9_\-]", "_", str(session_id))[:128] or "unknown"

    session_dir = root / "state" / "traces" / session_id
    session_dir.mkdir(parents=True, exist_ok=True)

    _prune(session_dir)

    scrubbed = scrub(payload)
    record = {
        "ts": time.time(),
        "hook_event_name": scrubbed.get("hook_event_name")
        or scrubbed.get("hookEventName"),
        "tool_name": scrubbed.get("tool_name") or scrubbed.get("toolName"),
        "tool_input": scrubbed.get("tool_input") or scrubbed.get("toolInput"),
        "tool_response": scrubbed.get("tool_response")
        or scrubbed.get("toolResponse"),
        "stop_hook_active": scrubbed.get("stop_hook_active"),
        "session_id": session_id,
    }

    # Use fractional seconds + pid to avoid collisions across concurrent hooks.
    fname = f"{int(record['ts'] * 1000)}-{os.getpid()}.jsonl.gz"
    target = session_dir / fname
    line = (json.dumps(record, ensure_ascii=False) + "\n").encode("utf-8")
    with gzip.open(target, "wb") as fh:
        fh.write(line)


def main() -> int:
    raw = sys.stdin.read() if not sys.stdin.isatty() else ""
    if not raw.strip():
        return 0
    try:
        payload = json.loads(raw)
    except (json.JSONDecodeError, ValueError):
        return 0
    if not isinstance(payload, dict):
        return 0

    root = _resolve_nlr_root()
    if root is None:
        return 0
    try:
        _write_trace(root, payload)
    except OSError:
        return 0
    return 0


if __name__ == "__main__":
    # Minimal built-in self-test for scrub regex.
    if len(sys.argv) > 1 and sys.argv[1] == "--self-test":
        cases = [
            ("NLR_API_TOKEN=abc123deadbeef", _REDACTED),
            ("sk-" + "A" * 25, _REDACTED),
            ("ghp_abcdef012345", _REDACTED),
            ("Authorization: Bearer xyz.jwt.payload", f"Authorization: {_REDACTED}"),
            ("no secrets here", "no secrets here"),
        ]
        failed = 0
        for inp, expect in cases:
            got = scrub(inp)
            if got != expect:
                print(f"FAIL: scrub({inp!r}) -> {got!r} != {expect!r}", file=sys.stderr)
                failed += 1
        nested = scrub({"a": "NLR_API_TOKEN=zzz", "b": ["Bearer secret"]})
        assert nested["a"] == _REDACTED, nested
        assert nested["b"] == [_REDACTED], nested
        if failed:
            sys.exit(1)
        print("trace_logger self-test OK")
        sys.exit(0)
    sys.exit(main())
