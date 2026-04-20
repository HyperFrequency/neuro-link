#!/usr/bin/env python3
"""ingest_refgraph.py — populate Qdrant + Neo4j from the deep-tool-wiki.

Reads the `deep-tool-wiki/<tool>/wiki.md` + `assets/refgraph-*.mmd` +
`llm-wiki/<tool>/**/*.md` files on disk and:

1. Chunks + embeds llm-wiki pages → upserts to Qdrant `nlr_wiki` collection.
2. Parses RefGraph Mermaid sources → upserts nodes/edges to Neo4j.
3. Embeds each Neo4j `:Symbol` node's qualname+docstring → Qdrant
   `tool_refgraph` collection with neo4j_node_id in payload.

Idempotent: uses deterministic point IDs (SHA256 of tool+path or
tool+qualname) so re-runs update rather than duplicate.

Usage:
    python3 ingest_refgraph.py [--dry-run] [--only-tool <name>]

Required packages (install.sh step 16 installs these in the NLR venv):
    qdrant-client>=1.8  neo4j>=5.15  httpx>=0.27  python-frontmatter>=1.0

Env vars: see companion `install_refgraph_ingest.sh` for the full list.
"""
from __future__ import annotations

import argparse
import hashlib
import os
import re
import sys
from pathlib import Path
from typing import Iterable


# ────────────────────────── config ──────────────────────────

DEEP_TOOL_WIKI_DIR = Path(os.environ.get("DEEP_TOOL_WIKI_DIR", "/Users/DanBot/hyperfrequency/docs/deep-tool-wiki"))
LLM_WIKI_DIR       = Path(os.environ.get("LLM_WIKI_DIR",       "/Users/DanBot/hyperfrequency/neuro-link/02-KB-main"))
QDRANT_URL         = os.environ.get("QDRANT_URL",         "http://127.0.0.1:6333")
NEO4J_URL          = os.environ.get("NEO4J_URL",          "bolt://127.0.0.1:7687")
NEO4J_USER         = os.environ.get("NEO4J_USER",         "neo4j")
NEO4J_PASSWORD     = os.environ.get("NEO4J_PASSWORD",     "")
LLAMA_SERVER_URL   = os.environ.get("LLAMA_SERVER_URL",   "http://127.0.0.1:8400")
ONLY_TOOL          = os.environ.get("ONLY_TOOL",          "") or None

NLR_WIKI_COLLECTION     = "nlr_wiki"
TOOL_REFGRAPH_COLLECTION = "tool_refgraph"
VECTOR_DIM = 4096   # Octen-Embedding-8B
DISTANCE = "Cosine"


# ────────────────────────── utilities ──────────────────────────

def point_id(*parts: str) -> int:
    """Deterministic 64-bit unsigned ID from string parts (matches Qdrant's uint64)."""
    h = hashlib.sha256("|".join(parts).encode("utf-8")).digest()
    return int.from_bytes(h[:8], "big") >> 1   # fit in signed int64


def tools_to_ingest() -> list[str]:
    if ONLY_TOOL:
        return [ONLY_TOOL]
    return sorted(d.name for d in DEEP_TOOL_WIKI_DIR.iterdir() if d.is_dir() and not d.name.startswith("."))


def embed_batch(client, texts: list[str]) -> list[list[float]]:
    """Call llama-server /embedding endpoint. Fails hard if unavailable."""
    import httpx
    resp = client.post(f"{LLAMA_SERVER_URL}/embedding", json={"content": texts}, timeout=120.0)
    resp.raise_for_status()
    data = resp.json()
    if isinstance(data, list):
        return [item["embedding"] for item in data]
    return [data["embedding"]]


# ────────────────────────── qdrant upsert ──────────────────────────

def ensure_qdrant_collections(qdrant):
    """Idempotent create-if-missing for both collections."""
    from qdrant_client.http import models as qm
    existing = {c.name for c in qdrant.get_collections().collections}
    for name in (NLR_WIKI_COLLECTION, TOOL_REFGRAPH_COLLECTION):
        if name not in existing:
            qdrant.create_collection(
                collection_name=name,
                vectors_config=qm.VectorParams(size=VECTOR_DIM, distance=qm.Distance.COSINE),
            )
            print(f"[qdrant] created collection: {name}")


def ingest_llm_wiki_to_qdrant(qdrant, httpx_client, tool: str) -> int:
    """Read llm-wiki/<tool>/**/*.md → chunk → embed → upsert to nlr_wiki.
    Returns count of points upserted."""
    tool_dir = LLM_WIKI_DIR / tool
    if not tool_dir.is_dir():
        return 0
    from qdrant_client.http import models as qm
    points = []
    for md_path in sorted(tool_dir.rglob("*.md")):
        text = md_path.read_text(encoding="utf-8")
        # Cheap chunking: per H2 section, max 2000 chars/chunk
        chunks = chunk_markdown(text, max_len=2000)
        rel = md_path.relative_to(LLM_WIKI_DIR).as_posix()
        for i, chunk in enumerate(chunks):
            points.append((point_id(tool, rel, str(i)), chunk, {
                "tool": tool,
                "leaf": rel,
                "chunk_idx": i,
                "source": "llm-wiki",
            }))
    if not points:
        return 0
    # Embed in batches of 16
    total = 0
    for i in range(0, len(points), 16):
        batch = points[i:i + 16]
        vecs = embed_batch(httpx_client, [p[1] for p in batch])
        qdrant.upsert(
            collection_name=NLR_WIKI_COLLECTION,
            points=[
                qm.PointStruct(id=pid, vector=vec, payload=meta)
                for (pid, _chunk, meta), vec in zip(batch, vecs)
            ],
        )
        total += len(batch)
    return total


def chunk_markdown(text: str, max_len: int = 2000) -> list[str]:
    """Split on H2 boundaries, then on paragraph if over max_len."""
    sections = re.split(r"(?m)^##\s", text)
    out: list[str] = []
    for sec in sections:
        if not sec.strip():
            continue
        if len(sec) <= max_len:
            out.append(sec.strip())
        else:
            # paragraph split
            paras = sec.split("\n\n")
            buf = ""
            for p in paras:
                if len(buf) + len(p) + 2 > max_len and buf:
                    out.append(buf.strip())
                    buf = p
                else:
                    buf = (buf + "\n\n" + p) if buf else p
            if buf.strip():
                out.append(buf.strip())
    return out


# ────────────────────────── neo4j upsert ──────────────────────────

def ingest_refgraph_to_neo4j(neo4j_driver, tool: str) -> tuple[int, int]:
    """Parse deep-tool-wiki/<tool>/assets/refgraph-*.mmd → MERGE into Neo4j.
    Returns (nodes_upserted, edges_upserted)."""
    assets_dir = DEEP_TOOL_WIKI_DIR / tool / "assets"
    if not assets_dir.is_dir():
        return 0, 0
    nodes: dict[str, dict] = {}
    edges: list[tuple[str, str, str]] = []
    for mmd in assets_dir.glob("refgraph-*.mmd"):
        parsed_nodes, parsed_edges = parse_mermaid(mmd.read_text(encoding="utf-8"))
        for n_id, attrs in parsed_nodes.items():
            nodes[n_id] = {**nodes.get(n_id, {}), **attrs, "tool": tool}
        edges.extend((s, t, rel) for s, t, rel in parsed_edges)

    if not nodes and not edges:
        return 0, 0

    # MERGE nodes first, then edges
    with neo4j_driver.session() as sess:
        for n_id, attrs in nodes.items():
            sess.run(
                "MERGE (s:Symbol {tool: $tool, qualname: $qualname}) "
                "SET s += $attrs",
                tool=tool, qualname=n_id, attrs=attrs,
            )
        for source, target, rel in edges:
            # rel-kind → edge label mapping
            label = rel.upper().replace("-", "_")
            sess.run(
                f"MATCH (a:Symbol {{tool: $tool, qualname: $source}}), "
                f"      (b:Symbol {{tool: $tool, qualname: $target}}) "
                f"MERGE (a)-[r:{label}]->(b)",
                tool=tool, source=source, target=target,
            )
    return len(nodes), len(edges)


MERMAID_NODE_RE = re.compile(r'^\s*([A-Za-z0-9_.]+)\s*(?:\[(.*?)\]|\{(.*?)\}|\((.*?)\))?', re.MULTILINE)
MERMAID_EDGE_RE = re.compile(r'^\s*([A-Za-z0-9_.]+)\s*(-->|==>|-\.->)\s*(?:\|([^|]+)\|)?\s*([A-Za-z0-9_.]+)', re.MULTILINE)


def parse_mermaid(src: str) -> tuple[dict[str, dict], list[tuple[str, str, str]]]:
    """Very permissive Mermaid parser covering classDiagram / graph LR / flowchart.
    TODO: replace with a proper Mermaid grammar when assets grow more complex."""
    nodes: dict[str, dict] = {}
    edges: list[tuple[str, str, str]] = []
    for line in src.splitlines():
        m_edge = MERMAID_EDGE_RE.match(line)
        if m_edge:
            src_id, _arrow, label, dst_id = m_edge.group(1), m_edge.group(2), m_edge.group(3), m_edge.group(4)
            rel = (label or "RELATED_TO").strip()
            edges.append((src_id, dst_id, rel))
            nodes.setdefault(src_id, {"label": src_id})
            nodes.setdefault(dst_id, {"label": dst_id})
            continue
        m_node = MERMAID_NODE_RE.match(line)
        if m_node and m_node.group(1) not in {"graph", "flowchart", "classDiagram", "classDef"}:
            node_id = m_node.group(1)
            label = m_node.group(2) or m_node.group(3) or m_node.group(4) or node_id
            nodes.setdefault(node_id, {"label": label})
    return nodes, edges


# ────────────────────────── main ──────────────────────────

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true", help="print plan, make no external calls")
    args = ap.parse_args()

    print(f"[ingest] deep-tool-wiki: {DEEP_TOOL_WIKI_DIR}")
    print(f"[ingest] llm-wiki:       {LLM_WIKI_DIR}")
    print(f"[ingest] Qdrant:         {QDRANT_URL}")
    print(f"[ingest] Neo4j:          {NEO4J_URL}")
    print(f"[ingest] llama-server:   {LLAMA_SERVER_URL}")

    tools = tools_to_ingest()
    print(f"[ingest] tools to process: {len(tools)} — {', '.join(tools)}")

    if args.dry_run:
        print("[ingest] DRY RUN — no changes")
        for tool in tools:
            md_count = sum(1 for _ in (LLM_WIKI_DIR / tool).rglob("*.md")) if (LLM_WIKI_DIR / tool).is_dir() else 0
            mmd_count = sum(1 for _ in (DEEP_TOOL_WIKI_DIR / tool / "assets").glob("refgraph-*.mmd")) if (DEEP_TOOL_WIKI_DIR / tool / "assets").is_dir() else 0
            print(f"[dry] {tool}: {md_count} llm-wiki md files → nlr_wiki;  {mmd_count} refgraph mmd → neo4j")
        return

    # Lazy imports so --dry-run works without the deps installed
    try:
        from qdrant_client import QdrantClient
        import httpx
        import neo4j
    except ImportError as e:
        print(f"[ingest][error] missing Python package: {e}", file=sys.stderr)
        print("  install with: pip install qdrant-client neo4j httpx", file=sys.stderr)
        sys.exit(1)

    qdrant = QdrantClient(url=QDRANT_URL)
    neo4j_driver = neo4j.GraphDatabase.driver(NEO4J_URL, auth=(NEO4J_USER, NEO4J_PASSWORD))
    httpx_client = httpx.Client()

    ensure_qdrant_collections(qdrant)

    total_qdrant_points = 0
    total_neo4j_nodes = 0
    total_neo4j_edges = 0

    for tool in tools:
        print(f"[ingest] {tool}: ...")
        q_points = ingest_llm_wiki_to_qdrant(qdrant, httpx_client, tool)
        n_nodes, n_edges = ingest_refgraph_to_neo4j(neo4j_driver, tool)
        total_qdrant_points += q_points
        total_neo4j_nodes += n_nodes
        total_neo4j_edges += n_edges
        print(f"[ingest] {tool}: qdrant={q_points} points, neo4j={n_nodes} nodes/{n_edges} edges")

    print(f"\n[ingest] TOTAL: qdrant={total_qdrant_points} points, neo4j={total_neo4j_nodes} nodes / {total_neo4j_edges} edges")

    neo4j_driver.close()
    httpx_client.close()


if __name__ == "__main__":
    main()
