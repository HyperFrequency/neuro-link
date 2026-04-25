#!/usr/bin/env python3
"""pkg/scripts/verify.py — install-completeness gate.

Runs per-component checks against a fresh install and emits
`pkg/.proof/INSTALL_COMPLETE.ready.json` with pass/fail per component
plus a top-level `green` boolean. Exit 0 iff every required check passes
(or is deliberately skipped via env).

Skipping rules:
  - NLR_VERIFY_SKIP=comp1,comp2  — skip listed checks (mark as 'skipped', not fail)
  - NLR_VERIFY_NEO4J_PASS        — required for the Neo4j HTTP check
  - NLR_VERIFY_OFFLINE=1         — skip llama-server + Neo4j + Qdrant probes

The gate is intentionally strict: a check's "failure" here means the
install did NOT reach parity with the user's existing box. That's the
whole point of A8.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[2]
PROOF_DIR = REPO_ROOT / "pkg" / ".proof"
HOME = Path(os.environ["HOME"])

SKIP = set(filter(None, os.environ.get("NLR_VERIFY_SKIP", "").split(",")))
OFFLINE = os.environ.get("NLR_VERIFY_OFFLINE") == "1"

# Gate-11 CC3: split REQUIRED vs OPTIONAL to match install-mirror.sh's
# BB1 contract. serena is high-value but installed separately via pipx
# (not bootstrapped by `make all`), so treat its absence as skipped,
# not failed. Missing REQUIRED servers remain a hard fail.
REQUIRED_MCP_SERVERS = ["neuro-link-http", "neuro-link-recursive"]
OPTIONAL_MCP_SERVERS = ["serena"]
# Gate-3 M3 fix: must match settings.template.json hook references. The
# prior list omitted neuro-task-check.sh which settings.template invokes
# on PreToolUse — completeness gate passed even if that file was missing.
REQUIRED_HOOKS = [
    "auto-rag-inject.sh",
    "doc-sync-on-push.sh",
    "harness-bridge-check.sh",
    "hf-docs-trigger.sh",
    "hf-fork-auto-ingest.sh",
    "hf-fork-trigger.sh",
    "inception-clear-relay.sh",
    "neuro-grade.sh",
    "neuro-log-tool-use.sh",
    "neuro-task-check.sh",
]
WARM_LLAMA_PORTS = [8400, 8401, 8402]


def _run(cmd: list[str], timeout: float = 10.0) -> tuple[int, str]:
    """Run cmd, return (rc, combined-stdout+stderr). Never raises."""
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        return r.returncode, (r.stdout or "") + (r.stderr or "")
    except FileNotFoundError:
        return 127, f"not found: {cmd[0]}"
    except subprocess.TimeoutExpired:
        return 124, f"timeout after {timeout}s"
    except Exception as e:  # pragma: no cover
        return 1, str(e)


def _http_ok(url: str, timeout: float = 3.0) -> tuple[bool, str]:
    """GET url; return (ok, note)."""
    req = urllib.request.Request(url)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return 200 <= resp.status < 400, f"HTTP {resp.status}"
    except Exception as e:
        return False, f"{type(e).__name__}: {e}"


def _skipped(name: str, reason: str) -> dict[str, Any]:
    return {"name": name, "status": "skipped", "reason": reason}


def _pass(name: str, note: str = "") -> dict[str, Any]:
    return {"name": name, "status": "pass", "note": note}


def _fail(name: str, reason: str) -> dict[str, Any]:
    return {"name": name, "status": "fail", "reason": reason}


# --- Checks --------------------------------------------------------------


def check_pyright() -> dict[str, Any]:
    if "pyright" in SKIP:
        return _skipped("pyright", "NLR_VERIFY_SKIP")
    if shutil.which("pyright") is None:
        return _fail("pyright", "not on PATH (pip install pyright in the pkg venv)")
    rc, out = _run(["pyright", "--version"])
    if rc == 0:
        return _pass("pyright", out.strip().splitlines()[0] if out else "")
    return _fail("pyright", f"--version exit {rc}: {out[:200]}")


def check_multilspy() -> dict[str, Any]:
    if "multilspy" in SKIP:
        return _skipped("multilspy", "NLR_VERIFY_SKIP")
    # multilspy is a library, not a CLI; import-check it from the harness venv
    # by invoking python -c "import multilspy".
    python = shutil.which("python3") or shutil.which("python")
    if python is None:
        return _fail("multilspy", "no python3 on PATH")
    rc, out = _run([python, "-c", "import multilspy; print(multilspy.__name__)"])
    if rc == 0 and "multilspy" in out:
        return _pass("multilspy", "importable from harness venv")
    return _fail("multilspy", f"import failed exit {rc}: {out[:200]}")


def _validate_mcp_entry(entry: dict[str, Any]) -> str | None:
    """Mirror of install-mirror.sh's is_mcp_entry_valid — STRUCTURAL
    check only. Validates shape + .command resolves to an executable
    (stdio) or .url is a well-formed http(s) URL (http/sse). Does NOT
    probe reachability (no HTTP fetch, no stdio exec). Returns None on
    success, or a short reason string on failure. Transport inferred:
    explicit .type wins, else string .url implies http, else stdio."""
    if not isinstance(entry, dict):
        return "entry is not a JSON object"
    transport = entry.get("type")
    if not isinstance(transport, str):
        transport = "http" if isinstance(entry.get("url"), str) else "stdio"
    if transport == "stdio":
        cmd = entry.get("command")
        if not isinstance(cmd, str) or not cmd:
            return "stdio entry missing non-empty .command"
        # Expand $HOME / ${HOME} / leading ~ for the executable probe
        expanded = cmd.replace("${HOME}", str(HOME)).replace("$HOME", str(HOME))
        if expanded.startswith("~"):
            expanded = str(HOME) + expanded[1:]
        if expanded.startswith("/"):
            if not Path(expanded).exists():
                return f"stdio .command {expanded} does not exist"
            if not os.access(expanded, os.X_OK):
                return f"stdio .command {expanded} not executable"
        else:
            if not shutil.which(expanded):
                return f"stdio .command {cmd!r} not on PATH"
        return None
    if transport in ("http", "sse"):
        url = entry.get("url")
        if not isinstance(url, str) or not url:
            return "http/sse entry missing non-empty .url"
        if not url.startswith(("http://", "https://")):
            return f"http/sse .url {url!r} does not start with http(s)://"
        return None
    return f"unknown transport type: {transport!r}"


def check_mcp_servers() -> dict[str, Any]:
    if "mcp-servers" in SKIP:
        return _skipped("mcp-servers", "NLR_VERIFY_SKIP")
    claude_json = HOME / ".claude.json"
    if not claude_json.is_file():
        return _fail("mcp-servers", f"{claude_json} does not exist")
    try:
        data = json.loads(claude_json.read_text())
    except Exception as e:
        return _fail("mcp-servers", f"invalid JSON: {e}")
    servers = data.get("mcpServers", {}) or {}
    missing_required = [s for s in REQUIRED_MCP_SERVERS if s not in servers]
    if missing_required:
        return _fail("mcp-servers", f"missing required: {','.join(missing_required)}")
    # Gate-12 EE2 + Gate-14 GG2 + Gate-15 HH1: structural validation of
    # REQUIRED entries (shape + .command executable for stdio + .url
    # well-formed for http/sse), PLUS a short-timeout reachability
    # probe for http/sse transports. The probe closes the "dead
    # listener / wrong port" hole that pure metadata checks missed.
    # stdio entries are NOT auto-started (too variable across servers);
    # their operational correctness surfaces at Claude's first tool
    # call instead. NLR_VERIFY_OFFLINE=1 skips the network probe, so
    # `make all`→`make proof` still runs offline.
    malformed_required: list[str] = []
    for srv in REQUIRED_MCP_SERVERS:
        reason = _validate_mcp_entry(servers[srv])
        if reason is not None:
            malformed_required.append(f"{srv}({reason})")
    if malformed_required:
        return _fail("mcp-servers", f"required registered but invalid: {'; '.join(malformed_required)}")

    # Gate-16 II1 + Gate-17 JJ1: protocol-aware MCP probe.
    #   - Send the entry's configured headers (so misconfigured
    #     ${NLR_API_TOKEN} surfaces as 401/403 from the real endpoint).
    #   - Disable urllib's automatic redirect following — a misrouted
    #     /mcp URL that 302s to a dashboard/login would otherwise end
    #     up 200 OK and pass; we now treat 3xx as "wrong endpoint".
    #   - POST a JSON-RPC `initialize` request and require the
    #     response body to be a JSON object with .jsonrpc=="2.0" plus
    #     either .result or .error. That proves the listener actually
    #     speaks the MCP protocol, not just HTTP.
    def _resolve_header_value(raw: str) -> str:
        return os.path.expandvars(raw)

    class _NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, req, fp, code, msg, headers, newurl):  # type: ignore[override]
            return None  # tells urllib to NOT follow

    _no_redirect_opener = urllib.request.build_opener(_NoRedirect)

    unreachable_required: list[str] = []
    if not OFFLINE:
        init_payload = json.dumps({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "neuro-link-verify.py", "version": "0.1"},
            },
        }).encode("utf-8")
        for srv in REQUIRED_MCP_SERVERS:
            entry = servers[srv]
            transport = entry.get("type") or ("http" if isinstance(entry.get("url"), str) else "stdio")
            if transport not in ("http", "sse"):
                continue
            url = entry.get("url", "")
            headers = {k: _resolve_header_value(v) for k, v in (entry.get("headers") or {}).items() if isinstance(v, str)}
            headers["Content-Type"] = "application/json"
            headers["Accept"] = "application/json, text/event-stream"
            try:
                req = urllib.request.Request(url, data=init_payload, headers=headers, method="POST")
                with _no_redirect_opener.open(req, timeout=3.0) as resp:
                    body = resp.read(8192)
                    code = resp.status
            except urllib.error.HTTPError as e:
                # 3xx (redirect) lands here because we disabled following.
                # Read the body and try to surface a JSON-RPC error message
                # when the upstream returns a structured error with a
                # non-2xx status (Gate-19 LL2 fix — prior code discarded
                # the body and only logged the status code).
                code = e.code
                err_msg: str | None = None
                try:
                    err_body = e.read(8192).decode("utf-8", errors="replace")
                    err_payload = json.loads(err_body) if err_body.strip().startswith("{") else None
                    if isinstance(err_payload, dict):
                        err_obj = err_payload.get("error")
                        if isinstance(err_obj, dict):
                            err_msg = str(err_obj.get("message", ""))[:80]
                except Exception:
                    pass
                if err_msg:
                    unreachable_required.append(f"{srv}(HTTP {code}: {err_msg})")
                else:
                    unreachable_required.append(f"{srv}(HTTP {code})")
                continue
            except Exception as e:
                unreachable_required.append(f"{srv}({type(e).__name__})")
                continue
            if code >= 400:
                unreachable_required.append(f"{srv}(HTTP {code})")
                continue
            # 2xx — verify MCP-shape body. Streamable-HTTP returns a
            # JSON object directly; SSE-transport returns a `data: {...}`
            # event line. Accept either.
            text = body.decode("utf-8", errors="replace").strip()
            mcp_payload: dict[str, Any] | None = None
            try:
                mcp_payload = json.loads(text) if text.startswith("{") else None
            except (json.JSONDecodeError, ValueError):
                mcp_payload = None
            if mcp_payload is None and "data:" in text:
                # SSE shape: scan for first `data: {...}` line
                for line in text.splitlines():
                    if line.startswith("data:"):
                        candidate = line[len("data:"):].strip()
                        try:
                            mcp_payload = json.loads(candidate)
                            break
                        except (json.JSONDecodeError, ValueError):
                            continue
            if not isinstance(mcp_payload, dict):
                unreachable_required.append(f"{srv}(non-JSON body)")
                continue
            if mcp_payload.get("jsonrpc") != "2.0":
                unreachable_required.append(f"{srv}(missing jsonrpc:2.0)")
                continue
            # Gate-18 KK1: require a successful MCP `initialize`
            # response shape — `.result.protocolVersion` AND
            # `.result.serverInfo`. A `.error` body proves nothing
            # because any unrelated JSON-RPC server returns one for
            # an unrecognized method; only the result shape is
            # diagnostic of an MCP implementation answering.
            if "error" in mcp_payload:
                err = mcp_payload.get("error") or {}
                err_msg = err.get("message", "") if isinstance(err, dict) else str(err)
                unreachable_required.append(f"{srv}(JSON-RPC error: {err_msg[:80]})")
                continue
            result = mcp_payload.get("result")
            if not isinstance(result, dict):
                unreachable_required.append(f"{srv}(missing initialize.result object)")
                continue
            # Gate-19 LL1: enforce TYPES, not just key presence.
            # protocolVersion must be a non-empty string; serverInfo
            # must be an object (presence-only let `protocolVersion:
            # null` or `serverInfo: "x"` slip past).
            pv = result.get("protocolVersion")
            si = result.get("serverInfo")
            shape_problems: list[str] = []
            if not isinstance(pv, str) or not pv:
                shape_problems.append("protocolVersion not a non-empty string")
            if not isinstance(si, dict):
                shape_problems.append("serverInfo not an object")
            if shape_problems:
                unreachable_required.append(
                    f"{srv}(initialize.result shape: {'; '.join(shape_problems)})"
                )
    if unreachable_required:
        return _fail(
            "mcp-servers",
            f"required http/sse listeners unreachable: {'; '.join(unreachable_required)} (NLR_VERIFY_OFFLINE=1 to skip)",
        )

    missing_optional = [s for s in OPTIONAL_MCP_SERVERS if s not in servers]
    malformed_optional: list[str] = []
    for srv in OPTIONAL_MCP_SERVERS:
        if srv in servers:
            reason = _validate_mcp_entry(servers[srv])
            if reason is not None:
                malformed_optional.append(f"{srv}({reason})")
    probe_note = "struct + http probe" if not OFFLINE else "struct only (OFFLINE)"
    note = f"required {probe_note} OK: {','.join(sorted(s for s in servers if s in REQUIRED_MCP_SERVERS))}"
    if missing_optional:
        note += f"; optional absent: {','.join(missing_optional)}"
    if malformed_optional:
        note += f"; optional malformed: {','.join(malformed_optional)}"
    return _pass("mcp-servers", note)


def check_hooks() -> dict[str, Any]:
    if "hooks" in SKIP:
        return _skipped("hooks", "NLR_VERIFY_SKIP")
    hooks_dir = HOME / ".claude" / "hooks"
    if not hooks_dir.is_dir():
        return _fail("hooks", f"{hooks_dir} missing")
    present = {p.name for p in hooks_dir.iterdir() if p.suffix == ".sh"}
    missing = [h for h in REQUIRED_HOOKS if h not in present]
    if missing:
        return _fail("hooks", f"{len(missing)} hook(s) missing: {','.join(missing)}")
    return _pass("hooks", f"{len(REQUIRED_HOOKS)}/{len(REQUIRED_HOOKS)} hooks present")


def check_qmd() -> dict[str, Any]:
    if "qmd" in SKIP:
        return _skipped("qmd", "NLR_VERIFY_SKIP")
    if shutil.which("qmd") is None:
        return _fail("qmd", "not on PATH (pip install qmd)")
    rc, out = _run(["qmd", "collection", "list"], timeout=15)
    if rc == 0:
        return _pass("qmd", (out.strip().splitlines() or ["(no output)"])[0][:120])
    return _fail("qmd", f"collection list exit {rc}: {out[:200]}")


def check_llama_servers() -> dict[str, Any]:
    if "llama-servers" in SKIP or OFFLINE:
        return _skipped("llama-servers", "NLR_VERIFY_OFFLINE or SKIP")
    reachable = []
    down = []
    for port in WARM_LLAMA_PORTS:
        ok, _ = _http_ok(f"http://127.0.0.1:{port}/v1/models", timeout=2.0)
        (reachable if ok else down).append(port)
    if not reachable:
        # Non-fatal but loud — user may have them intentionally stopped.
        return {
            "name": "llama-servers",
            "status": "warn",
            "reason": f"no warm llama-servers reachable on {WARM_LLAMA_PORTS}; expected at least 1",
            "reachable": reachable,
            "down": down,
        }
    return {
        "name": "llama-servers",
        "status": "pass",
        "note": f"{len(reachable)}/{len(WARM_LLAMA_PORTS)} reachable",
        "reachable": reachable,
        "down": down,
    }


def check_neo4j() -> dict[str, Any]:
    if "neo4j" in SKIP or OFFLINE:
        return _skipped("neo4j", "NLR_VERIFY_OFFLINE or SKIP")
    ok, note = _http_ok("http://127.0.0.1:7474/", timeout=3.0)
    if not ok:
        return _fail("neo4j", f"7474 probe: {note}")
    # Optional: count tools if password provided.
    pw = os.environ.get("NLR_VERIFY_NEO4J_PASS")
    if not pw:
        return _pass("neo4j", f"HTTP port up (no NLR_VERIFY_NEO4J_PASS for content check)")
    user = os.environ.get("NEO4J_USER", "neo4j")
    import base64

    auth = base64.b64encode(f"{user}:{pw}".encode()).decode()
    req = urllib.request.Request(
        "http://127.0.0.1:7474/db/neo4j/tx/commit",
        data=json.dumps({"statements": [{"statement": "MATCH (t:Tool) RETURN count(t) AS n"}]}).encode(),
        headers={"Content-Type": "application/json", "Authorization": f"Basic {auth}"},
    )
    try:
        with urllib.request.urlopen(req, timeout=5) as resp:
            body = json.loads(resp.read())
        n = body.get("results", [{}])[0].get("data", [{}])[0].get("row", [0])[0]
        if n >= 1:
            return _pass("neo4j", f"{n} Tool node(s) present")
        return _fail("neo4j", "0 Tool nodes — run scripts/ingest_deep_tool_wiki_into_neo4j.sh")
    except Exception as e:
        return _fail("neo4j", f"query failed: {e}")


def check_qdrant() -> dict[str, Any]:
    if "qdrant" in SKIP or OFFLINE:
        return _skipped("qdrant", "NLR_VERIFY_OFFLINE or SKIP")
    ok, note = _http_ok("http://127.0.0.1:6333/collections/nlr_wiki", timeout=3.0)
    if not ok:
        return _fail("qdrant", f"collection probe: {note}")
    # Fetch point count
    try:
        req = urllib.request.Request(
            "http://127.0.0.1:6333/collections/nlr_wiki",
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req, timeout=3) as resp:
            body = json.loads(resp.read())
        count = body.get("result", {}).get("points_count", 0)
        status = body.get("result", {}).get("status", "unknown")
        if count >= 25 and status == "green":
            return _pass("qdrant", f"nlr_wiki: {count} points, status={status}")
        return _fail(
            "qdrant",
            f"nlr_wiki has {count} points (need >=25), status={status}",
        )
    except Exception as e:
        return _fail("qdrant", f"query failed: {e}")


def check_vaults_dir() -> dict[str, Any]:
    if "vaults" in SKIP:
        return _skipped("vaults", "NLR_VERIFY_SKIP")
    vaults_dir = REPO_ROOT / "vaults"
    if not vaults_dir.is_dir():
        return _fail("vaults", f"{vaults_dir} does not exist — A5 not applied?")
    readme = vaults_dir / "README.md"
    if not readme.is_file():
        return _fail("vaults", f"{readme} missing")
    return _pass("vaults", f"{vaults_dir} + README.md present")


def _detect_host_arch() -> str:
    """Return the *hardware* arch — arm64, x86_64, or unknown.

    `platform.machine()` reports the Python interpreter's arch, which on
    Apple Silicon under Rosetta translation lies (returns x86_64 on arm64
    hardware). `sysctl hw.optional.arm64` is hardware-state, not
    process-state — unforgeable by Rosetta. Fall back to uname(1) for
    Linux/BSD.
    """
    if sys.platform == "darwin":
        rc, out = _run(["sysctl", "-n", "hw.optional.arm64"])
        if rc == 0 and out.strip() == "1":
            return "arm64"
        if rc == 0 and out.strip() == "0":
            return "x86_64"
    rc, out = _run(["uname", "-m"])
    if rc != 0:
        return "unknown"
    m = out.strip()
    if m in ("arm64", "aarch64"):
        return "arm64"
    if m in ("x86_64", "amd64"):
        return "x86_64"
    return "unknown"


def _resolve_serena_bin() -> tuple[Path | None, str]:
    """Read mcpServers.serena.command from ~/.claude.json and return the
    resolved binary path. Returns (None, reason) on any failure."""
    claude_json = HOME / ".claude.json"
    if not claude_json.is_file():
        return None, f"{claude_json} missing — install_mcp_servers.sh did not run"
    try:
        cfg = json.loads(claude_json.read_text())
    except (OSError, json.JSONDecodeError) as e:
        return None, f"could not read {claude_json}: {e}"
    serena = (cfg.get("mcpServers") or {}).get("serena")
    if not isinstance(serena, dict):
        return None, "mcpServers.serena absent or not an object in ~/.claude.json"
    cmd = serena.get("command")
    if not isinstance(cmd, str) or not cmd:
        return None, f"mcpServers.serena.command absent or empty: {serena!r}"
    expanded = cmd.replace("${HOME}", str(HOME)).replace("$HOME", str(HOME))
    if expanded.startswith("~"):
        expanded = str(HOME) + expanded[1:]
    if expanded.startswith("/"):
        path = Path(expanded)
    else:
        which = shutil.which(expanded)
        if not which:
            return None, f"mcpServers.serena.command {expanded!r} not on PATH"
        path = Path(which)
    if not path.is_file():
        return None, f"serena binary {path} (from ~/.claude.json) missing"
    if not os.access(path, os.X_OK):
        return None, f"serena binary {path} (from ~/.claude.json) is not executable (chmod +x or reinstall)"
    return path, str(path)


def check_serena_arch() -> dict[str, Any]:
    if "serena-arch" in SKIP:
        return _skipped("serena-arch", "NLR_VERIFY_SKIP")
    # Gate-11 CC3 + Gate-13 FF1: serena is an OPTIONAL MCP — matching
    # install-mirror.sh BB1's contract. If the key isn't registered,
    # skip. If the key IS registered but the binary can't be resolved
    # (stale entry from a prior uninstall, wrong path, etc.), ALSO skip
    # with a warn note — install-mirror.sh treats that case as "warn
    # only" for optional servers, so the gate must match or retries on
    # non-pristine hosts go red even when the installer said OK.
    # Returning _skipped (not _fail) keeps the optional contract intact
    # while surfacing the stale entry in the gate note.
    claude_json = HOME / ".claude.json"
    if claude_json.is_file():
        try:
            cfg = json.loads(claude_json.read_text())
        except (OSError, json.JSONDecodeError):
            cfg = {}
        if "serena" not in (cfg.get("mcpServers") or {}):
            return _skipped("serena-arch", "serena not registered (optional MCP)")
    serena_bin, why = _resolve_serena_bin()
    if serena_bin is None:
        return _skipped("serena-arch", f"optional serena unresolvable: {why}")

    # ~/.local/bin/serena-hooks is usually a pip wrapper script — running
    # file(1) directly tells you "ASCII text". Resolve symlinks, then
    # follow shebangs up to two hops so we reach the Python interpreter
    # the wrapper actually launches. That's the binary whose arch matters.
    target = serena_bin.resolve()
    trail: list[str] = [str(target)]
    for _ in range(2):
        try:
            head = target.read_bytes()[:256]
        except OSError:
            break
        if not head.startswith(b"#!"):
            break
        shebang = head.split(b"\n", 1)[0][2:].decode("utf-8", "replace").strip()
        parts = shebang.split()
        if not parts:
            break
        if parts[0].rsplit("/", 1)[-1] == "env" and len(parts) > 1:
            resolved = shutil.which(parts[1])
        else:
            resolved = parts[0]
        if not resolved:
            break
        nxt = Path(resolved).resolve()
        if nxt == target:
            break
        target = nxt
        trail.append(str(target))

    rc, out = _run(["file", str(target)])
    if rc != 0:
        return _fail("serena-arch", f"file(1) exit {rc} on {target}: {out[:200]}")

    host_arch = _detect_host_arch()
    trail_s = " -> ".join(trail)

    if "arm64" in out:
        return _pass("serena-arch", f"arm64 native ({trail_s})")
    if "x86_64" in out and "arm64" not in out:
        return _fail(
            "serena-arch",
            f"x86_64-only binary at {target} (Rosetta) — reinstall with arch -arm64 python: {out[:160]}",
        )

    # Unknown signature. On arm64 *hardware* the rag stack leans on Metal
    # via an arm64-native interpreter; ambiguity here almost always means
    # a misinstall (uv's cpython-3.11 is x86_64 on this host). Hard-fail
    # when the hardware is arm64 — note that we resolve host arch from
    # sysctl, NOT platform.machine(), so a Rosetta-translated Python
    # cannot bypass this branch.
    if host_arch == "arm64":
        return _fail(
            "serena-arch",
            f"unknown arch on arm64 host — {trail_s}: {out[:160]}",
        )
    return {
        "name": "serena-arch",
        "status": "warn",
        "note": f"unknown arch on {host_arch}: {out[:120]}",
    }


# --- Main ----------------------------------------------------------------


CHECKS = [
    check_pyright,
    check_multilspy,
    check_mcp_servers,
    check_hooks,
    check_qmd,
    check_llama_servers,
    check_neo4j,
    check_qdrant,
    check_vaults_dir,
    check_serena_arch,
]


def main() -> int:
    results = []
    for fn in CHECKS:
        try:
            results.append(fn())
        except Exception as e:  # pragma: no cover
            results.append(_fail(fn.__name__, f"unexpected exception: {e}"))

    # green: zero fails (warns and skips are allowed).
    statuses = [r["status"] for r in results]
    fails = sum(1 for s in statuses if s == "fail")
    warns = sum(1 for s in statuses if s == "warn")
    skips = sum(1 for s in statuses if s == "skipped")
    passes = sum(1 for s in statuses if s == "pass")
    green = fails == 0

    PROOF_DIR.mkdir(parents=True, exist_ok=True)
    # Gate-20 MM2: emit `state` alongside `green` so the same
    # INSTALL_COMPLETE.ready.json file is consumable by both verify.py's
    # own consumers (which use `green`) AND aggregate_proof.py
    # (which keys off `state` for the per-target ship aggregator). The
    # state mapping mirrors aggregate_proof's vocabulary:
    #   green && fails==0   → "ready"
    #   not-green           → "fail"
    out = {
        "target": "INSTALL_COMPLETE",
        "state": "ready" if green else "fail",
        "generated_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "run_id": os.environ.get("NLR_RUN_ID", "unknown-run"),
        "green": green,
        "summary": {"pass": passes, "fail": fails, "warn": warns, "skipped": skips},
        "checks": results,
    }
    proof_path = PROOF_DIR / "INSTALL_COMPLETE.ready.json"
    proof_path.write_text(json.dumps(out, indent=2) + "\n")

    # Human-readable summary to stderr
    print(f"INSTALL_COMPLETE: green={green} | pass={passes} fail={fails} warn={warns} skipped={skips}", file=sys.stderr)
    for r in results:
        line = f"  [{r['status']:<7}] {r['name']}"
        if r.get("note"):
            line += f" — {r['note']}"
        if r.get("reason"):
            line += f" — {r['reason']}"
        print(line, file=sys.stderr)
    print(f"Proof: {proof_path}", file=sys.stderr)

    return 0 if green else 1


if __name__ == "__main__":
    sys.exit(main())
