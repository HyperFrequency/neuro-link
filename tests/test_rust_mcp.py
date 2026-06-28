"""Tests for the Rust MCP server (30 tests).

Each test spawns the binary with a temp NLR_ROOT, sends JSON-RPC via stdin,
and asserts on the response.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from conftest import MCPClient


# ---------------------------------------------------------------------------
# 1. Initialize
# ---------------------------------------------------------------------------

def test_initialize_response(mcp: MCPClient, nlr_root: Path):
    """initialize returns protocolVersion and serverInfo."""
    # The conftest already called initialize; send again to get a fresh response.
    resp = mcp.send("initialize")
    result = resp["result"]
    assert result["protocolVersion"] == "2024-11-05"
    assert result["serverInfo"]["name"] == "neuro-link-recursive"
    assert "version" in result["serverInfo"]


# ---------------------------------------------------------------------------
# 2-4. Tools list
# ---------------------------------------------------------------------------

def test_tools_list_returns_23_tools(mcp: MCPClient, nlr_root: Path):
    resp = mcp.send("tools/list")
    tools = resp["result"]["tools"]
    assert len(tools) == 23, f"Expected 23 tools, got {len(tools)}: {[t['name'] for t in tools]}"


def test_tools_list_has_required_fields(mcp: MCPClient, nlr_root: Path):
    resp = mcp.send("tools/list")
    for tool in resp["result"]["tools"]:
        assert "name" in tool, f"Tool missing 'name': {tool}"
        assert "description" in tool, f"Tool {tool.get('name')} missing 'description'"
        assert "inputSchema" in tool, f"Tool {tool.get('name')} missing 'inputSchema'"
        schema = tool["inputSchema"]
        assert schema.get("type") == "object", f"Tool {tool['name']} inputSchema type != object"


# ---------------------------------------------------------------------------
# 5-11. Wiki tools
# ---------------------------------------------------------------------------

def test_wiki_create_and_read(mcp: MCPClient, nlr_root: Path):
    text = mcp.tool_text("nlr_wiki_create", {
        "path": "test-page.md", "title": "Test Page", "content": "Hello world"
    })
    assert "Created" in text
    read = mcp.tool_text("nlr_wiki_read", {"path": "test-page.md"})
    assert "Hello world" in read
    assert "title: Test Page" in read


def test_wiki_create_computes_sha256(mcp: MCPClient, nlr_root: Path):
    mcp.call_tool("nlr_wiki_create", {
        "path": "sha-test.md", "title": "SHA", "content": "deterministic"
    })
    page = (nlr_root / "02-KB-main" / "sha-test.md").read_text()
    import hashlib
    expected = hashlib.sha256(b"deterministic").hexdigest()
    assert expected in page


def test_wiki_update_replace(mcp: MCPClient, nlr_root: Path):
    mcp.call_tool("nlr_wiki_create", {
        "path": "upd.md", "title": "Upd", "content": "original"
    })
    mcp.call_tool("nlr_wiki_update", {
        "path": "upd.md", "content": "replaced", "append": False
    })
    content = (nlr_root / "02-KB-main" / "upd.md").read_text()
    assert "replaced" in content
    assert "original" not in content


def test_wiki_update_append(mcp: MCPClient, nlr_root: Path):
    mcp.call_tool("nlr_wiki_create", {
        "path": "app.md", "title": "App", "content": "first"
    })
    mcp.call_tool("nlr_wiki_update", {
        "path": "app.md", "content": "second", "append": True
    })
    content = (nlr_root / "02-KB-main" / "app.md").read_text()
    assert "first" in content
    assert "second" in content


def test_wiki_list_empty(mcp: MCPClient, nlr_root: Path):
    text = mcp.tool_text("nlr_wiki_list")
    pages = json.loads(text)
    assert isinstance(pages, list)
    assert len(pages) == 0


def test_wiki_list_after_create(mcp: MCPClient, nlr_root: Path):
    mcp.call_tool("nlr_wiki_create", {
        "path": "listed.md", "title": "Listed", "content": "body"
    })
    text = mcp.tool_text("nlr_wiki_list")
    pages = json.loads(text)
    assert len(pages) == 1
    assert pages[0]["title"] == "Listed"


def test_wiki_search_finds_match(mcp: MCPClient, nlr_root: Path):
    mcp.call_tool("nlr_wiki_create", {
        "path": "microstructure.md",
        "title": "Market Microstructure",
        "content": "Order book dynamics and price formation.",
    })
    text = mcp.tool_text("nlr_wiki_search", {"query": "microstructure"})
    hits = json.loads(text)
    assert len(hits) >= 1
    assert any("microstructure" in h.get("path", "").lower() for h in hits)


def test_wiki_search_no_match(mcp: MCPClient, nlr_root: Path):
    text = mcp.tool_text("nlr_wiki_search", {"query": "xyznonexistent123"})
    hits = json.loads(text)
    assert len(hits) == 0


# ---------------------------------------------------------------------------
# 12-14. RAG tools
# ---------------------------------------------------------------------------

def test_rag_query_keyword_match(mcp: MCPClient, nlr_root: Path):
    mcp.call_tool("nlr_wiki_create", {
        "path": "rag-target.md",
        "title": "Reinforcement Learning",
        "content": "Policy gradient methods optimize expected return.",
    })
    text = mcp.tool_text("nlr_rag_query", {"query": "policy gradient optimization"})
    results = json.loads(text)
    assert len(results) >= 1


def test_rag_query_no_match(mcp: MCPClient, nlr_root: Path):
    text = mcp.tool_text("nlr_rag_query", {"query": "zzz_nomatch_zzz"})
    results = json.loads(text)
    assert len(results) == 0


def test_rag_rebuild_index(mcp: MCPClient, nlr_root: Path):
    mcp.call_tool("nlr_wiki_create", {
        "path": "idx-page.md", "title": "Index Test", "content": "content for indexing"
    })
    text = mcp.tool_text("nlr_rag_rebuild_index")
    assert "Index rebuilt" in text
    idx_file = nlr_root / "state" / "auto-rag-index.json"
    assert idx_file.exists()
    idx = json.loads(idx_file.read_text())
    assert "keywords" in idx
    assert "pages" in idx


# ---------------------------------------------------------------------------
# 15-17. Ontology tools
# ---------------------------------------------------------------------------

def test_ontology_generate_creates_scaffold(mcp: MCPClient, nlr_root: Path):
    text = mcp.tool_text("nlr_ontology_generate", {
        "text": "Market microstructure studies order flow.",
        "name": "microstructure",
        "type": "domain",
    })
    assert "scaffold created" in text.lower()
    ont_dir = nlr_root / "03-ontology-main" / "domain" / "microstructure"
    assert (ont_dir / "summary.md").exists()
    assert (ont_dir / "detailed.md").exists()
    assert (ont_dir / "metadata.json").exists()
    assert (ont_dir / "input.md").exists()


def test_ontology_query_not_found(mcp: MCPClient, nlr_root: Path):
    resp = mcp.call_tool("nlr_ontology_query", {"name": "nonexistent"})
    content = resp["result"]["content"][0]["text"]
    assert "Error" in content or "not found" in content.lower()


def test_ontology_gaps_empty(mcp: MCPClient, nlr_root: Path):
    text = mcp.tool_text("nlr_ontology_gaps")
    data = json.loads(text)
    assert "total_ontologies" in data


# ---------------------------------------------------------------------------
# 18-20. Ingest tools
# ---------------------------------------------------------------------------

def test_ingest_creates_raw_file(mcp: MCPClient, nlr_root: Path):
    text = mcp.tool_text("nlr_ingest", {
        "slug": "test-article",
        "content": "Article body text",
        "url": "https://example.com/article",
        "source_type": "web",
    })
    data = json.loads(text)
    assert data["status"] == "ingested"
    assert (nlr_root / "00-raw" / "test-article" / "source.md").exists()
    assert (nlr_root / "00-raw" / "test-article" / "metadata.json").exists()


def test_ingest_dedup_detects_duplicate(mcp: MCPClient, nlr_root: Path):
    mcp.call_tool("nlr_ingest", {"slug": "first", "content": "same content"})
    text = mcp.tool_text("nlr_ingest_dedup", {"content": "same content"})
    data = json.loads(text)
    assert data["duplicate"] is True


def test_ingest_classify_sorts_to_domain(mcp: MCPClient, nlr_root: Path):
    mcp.call_tool("nlr_ingest", {"slug": "classify-me", "content": "Domain test body"})
    text = mcp.tool_text("nlr_ingest_classify", {"slug": "classify-me", "domain": "arxiv"})
    assert "arxiv" in text.lower()
    assert (nlr_root / "01-sorted" / "arxiv" / "classify-me.md").exists()


# ---------------------------------------------------------------------------
# 21-23. Task tools
# ---------------------------------------------------------------------------

def test_task_create_writes_file(mcp: MCPClient, nlr_root: Path):
    text = mcp.tool_text("nlr_task_create", {
        "title": "Ingest NautilusTrader docs",
        "type": "ingest",
        "priority": 1,
        "body": "Scrape release notes.",
    })
    assert "Created" in text
    files = list((nlr_root / "07-neuro-link-task").glob("*.md"))
    assert len(files) == 1
    content = files[0].read_text()
    assert "status: pending" in content
    assert "priority: 1" in content


def test_task_list_pending(mcp: MCPClient, nlr_root: Path):
    mcp.call_tool("nlr_task_create", {"title": "Task A", "type": "curate", "priority": 2})
    text = mcp.tool_text("nlr_task_list", {"status_filter": "pending"})
    tasks = json.loads(text)
    assert len(tasks) == 1


def test_task_update_status(mcp: MCPClient, nlr_root: Path):
    mcp.call_tool("nlr_task_create", {"title": "Updatable", "type": "scan"})
    files = list((nlr_root / "07-neuro-link-task").glob("*.md"))
    fname = files[0].name
    text = mcp.tool_text("nlr_task_update", {"filename": fname, "status": "completed"})
    assert "completed" in text.lower()
    content = (nlr_root / "07-neuro-link-task" / fname).read_text()
    assert "status: completed" in content


# ---------------------------------------------------------------------------
# 24-25. Harness tools
# ---------------------------------------------------------------------------

def test_harness_list_reads_config(mcp: MCPClient, nlr_root: Path):
    text = mcp.tool_text("nlr_harness_list")
    data = json.loads(text)
    assert "version" in data


def test_harness_dispatch_writes_message(mcp: MCPClient, nlr_root: Path):
    text = mcp.tool_text("nlr_harness_dispatch", {
        "to": "k-dense", "task": "Review paper", "priority": 1
    })
    assert "Dispatched" in text
    comms = nlr_root / "06-self-improvement-recursive" / "harness-to-harness-comms"
    files = list(comms.glob("*.json"))
    assert len(files) == 1
    msg = json.loads(files[0].read_text())
    assert msg["to"] == "k-dense"
    assert msg["task"] == "Review paper"


# ---------------------------------------------------------------------------
# 26-28. Scan tools
# ---------------------------------------------------------------------------

def test_scan_health_ok(mcp: MCPClient, nlr_root: Path):
    text = mcp.tool_text("nlr_scan_health")
    data = json.loads(text)
    assert data["status"] == "ok"
    assert data["errors"] == []


def test_scan_health_missing_dir(nlr_root: Path):
    import shutil
    shutil.rmtree(nlr_root / "02-KB-main")
    with MCPClient(nlr_root) as client:
        client.send("initialize")
        text = client.tool_text("nlr_scan_health")
        data = json.loads(text)
        assert data["status"] == "error"
        assert any("02-KB-main" in e for e in data["errors"])


def test_scan_staleness_empty(mcp: MCPClient, nlr_root: Path):
    text = mcp.tool_text("nlr_scan_staleness", {"threshold_days": 30})
    stale = json.loads(text)
    assert isinstance(stale, list)
    assert len(stale) == 0


# ---------------------------------------------------------------------------
# 29-30. State & config tools
# ---------------------------------------------------------------------------

def test_state_heartbeat_read(mcp: MCPClient, nlr_root: Path):
    text = mcp.tool_text("nlr_state_heartbeat", {"action": "read"})
    data = json.loads(text)
    assert data["status"] == "ok"


def test_state_log_appends(mcp: MCPClient, nlr_root: Path):
    mcp.call_tool("nlr_state_log", {"tool": "test-tool", "exit_code": 0})
    log = (nlr_root / "state" / "session_log.jsonl").read_text().strip()
    assert log  # not empty
    entry = json.loads(log.splitlines()[-1])
    assert entry["tool"] == "test-tool"
    assert entry["success"] is True


def test_config_read_frontmatter(mcp: MCPClient, nlr_root: Path):
    text = mcp.tool_text("nlr_config_read", {"name": "neuro-link"})
    data = json.loads(text)
    assert data["version"] == "1"
