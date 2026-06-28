#!/usr/bin/env python3
"""
Create the `math_symbols` Qdrant collection.

- Vector size 4096 (matches Octen-Embedding-8B used by the Rust server).
- Distance: Cosine.
- Payload schema indexes canonical `srepr` so symbolic lookups are O(log n).
- Idempotent: if collection exists we only ensure the payload index.
- Non-brittle: if qdrant is unreachable we log + exit 0.
"""

from __future__ import annotations

import argparse
import logging
import sys
from typing import Optional

LOG = logging.getLogger("qdrant_index")

COLLECTION_NAME = "math_symbols"
VECTOR_SIZE = 4096


def _get_client(host: str, port: int):
    try:
        from qdrant_client import QdrantClient  # type: ignore
    except ImportError:
        LOG.error("qdrant-client not installed; pip install qdrant-client")
        return None
    try:
        return QdrantClient(host=host, port=port, timeout=5.0)
    except Exception as exc:
        LOG.warning("could not connect to qdrant at %s:%d (%s)", host, port, exc)
        return None


def ensure_collection(host: str = "localhost", port: int = 6333) -> int:
    client = _get_client(host, port)
    if client is None:
        LOG.warning("qdrant unavailable; skipping schema setup (non-fatal)")
        return 0

    try:
        from qdrant_client.http import models as qm  # type: ignore
    except Exception as exc:  # pragma: no cover
        LOG.warning("qdrant-client http models unavailable: %s", exc)
        return 0

    try:
        existing = {c.name for c in client.get_collections().collections}
    except Exception as exc:
        LOG.warning("qdrant get_collections failed (%s); non-fatal", exc)
        return 0

    if COLLECTION_NAME in existing:
        LOG.info("collection %r already exists; ensuring payload index", COLLECTION_NAME)
    else:
        try:
            client.create_collection(
                collection_name=COLLECTION_NAME,
                vectors_config=qm.VectorParams(
                    size=VECTOR_SIZE,
                    distance=qm.Distance.COSINE,
                ),
            )
            LOG.info(
                "created collection %r (size=%d, distance=Cosine)",
                COLLECTION_NAME,
                VECTOR_SIZE,
            )
        except Exception as exc:
            LOG.warning("create_collection failed (%s); non-fatal", exc)
            return 0

    # Index canonical srepr for fast exact-match lookup.
    try:
        client.create_payload_index(
            collection_name=COLLECTION_NAME,
            field_name="canonical_srepr",
            field_schema=qm.PayloadSchemaType.KEYWORD,
        )
        LOG.info("payload index on 'canonical_srepr' ensured")
    except Exception as exc:
        LOG.info("payload index creation skipped/already exists: %s", exc)

    return 0


def _parse_args() -> argparse.Namespace:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--host", default="localhost")
    ap.add_argument("--port", type=int, default=6333)
    ap.add_argument("--verbose", action="store_true")
    return ap.parse_args()


def main() -> int:
    args = _parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(levelname)s %(name)s %(message)s",
    )
    return ensure_collection(args.host, args.port)


if __name__ == "__main__":
    sys.exit(main())
