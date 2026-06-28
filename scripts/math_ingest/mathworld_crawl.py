#!/usr/bin/env python3
"""
Polite MathWorld crawler.

- Reads topic slugs (one per line) from topics.txt.
- Fetches https://mathworld.wolfram.com/<slug>.html at 1 req / 3s (hard floor).
- Caches raw HTML to state/mirrors/mathworld/<slug>.html; skips cached entries.
- Sends a descriptive User-Agent and obeys robots.txt via urllib.robotparser.
- No concurrency.
"""

from __future__ import annotations

import argparse
import logging
import sys
import time
import urllib.robotparser as robotparser
from pathlib import Path
from typing import Iterable, List

import requests

USER_AGENT = (
    "neuro-link-recursive/0.2 "
    "(https://github.com/HyperFrequency/neuro-link-recursive)"
)
BASE_URL = "https://mathworld.wolfram.com"
MIN_INTERVAL_SECONDS = 3.0  # hard floor, no concurrency
REQUEST_TIMEOUT = 30
ROBOTS_URL = f"{BASE_URL}/robots.txt"

LOG = logging.getLogger("mathworld_crawl")


def _repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def _load_topics(path: Path) -> List[str]:
    lines = [ln.strip() for ln in path.read_text().splitlines()]
    return [ln for ln in lines if ln and not ln.startswith("#")]


def _build_robot_parser(session: requests.Session) -> robotparser.RobotFileParser:
    rp = robotparser.RobotFileParser()
    rp.set_url(ROBOTS_URL)
    try:
        resp = session.get(ROBOTS_URL, timeout=REQUEST_TIMEOUT)
        if resp.status_code == 200:
            rp.parse(resp.text.splitlines())
        else:
            LOG.warning(
                "robots.txt returned HTTP %s; defaulting to allow",
                resp.status_code,
            )
            rp.parse(["User-agent: *", "Allow: /"])
    except requests.RequestException as exc:
        LOG.warning("could not fetch robots.txt (%s); defaulting to allow", exc)
        rp.parse(["User-agent: *", "Allow: /"])
    return rp


def _fetch_with_rate_limit(
    session: requests.Session,
    url: str,
    last_fetch_ts: List[float],
) -> requests.Response:
    # Enforce 1 req / MIN_INTERVAL_SECONDS even across multiple calls.
    if last_fetch_ts:
        elapsed = time.time() - last_fetch_ts[0]
        if elapsed < MIN_INTERVAL_SECONDS:
            time.sleep(MIN_INTERVAL_SECONDS - elapsed)
    resp = session.get(url, timeout=REQUEST_TIMEOUT)
    last_fetch_ts.clear()
    last_fetch_ts.append(time.time())
    return resp


def crawl(
    topics: Iterable[str],
    cache_dir: Path,
    session: requests.Session | None = None,
    robot_parser: robotparser.RobotFileParser | None = None,
) -> dict:
    """Download each topic's MathWorld page. Returns stats dict.

    Cached pages (files that already exist) are skipped -- zero HTTP calls.
    """
    cache_dir.mkdir(parents=True, exist_ok=True)
    session = session or requests.Session()
    session.headers.update({"User-Agent": USER_AGENT})
    robot_parser = robot_parser or _build_robot_parser(session)

    stats = {"cached": 0, "fetched": 0, "skipped_disallow": 0, "errors": 0}
    last_fetch_ts: List[float] = []

    for slug in topics:
        out_path = cache_dir / f"{slug}.html"
        if out_path.exists():
            LOG.info("cache hit: %s", slug)
            stats["cached"] += 1
            continue

        url = f"{BASE_URL}/{slug}.html"
        if not robot_parser.can_fetch(USER_AGENT, url):
            LOG.warning("robots.txt disallows %s; skipping", url)
            stats["skipped_disallow"] += 1
            continue

        try:
            resp = _fetch_with_rate_limit(session, url, last_fetch_ts)
        except requests.RequestException as exc:
            LOG.error("fetch failed for %s: %s", slug, exc)
            stats["errors"] += 1
            continue

        if resp.status_code != 200:
            LOG.error("HTTP %s for %s", resp.status_code, url)
            stats["errors"] += 1
            continue

        out_path.write_bytes(resp.content)
        LOG.info("fetched %s -> %s (%d bytes)", slug, out_path, len(resp.content))
        stats["fetched"] += 1

    return stats


def _parse_args() -> argparse.Namespace:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--topics",
        default=str(Path(__file__).with_name("topics.txt")),
        help="path to topics.txt (one slug per line)",
    )
    ap.add_argument(
        "--cache-dir",
        default=str(_repo_root() / "state" / "mirrors" / "mathworld"),
        help="where to persist <slug>.html files",
    )
    ap.add_argument(
        "--only",
        nargs="*",
        default=None,
        help="restrict to these slugs (useful for smoke tests)",
    )
    ap.add_argument(
        "--verbose",
        action="store_true",
        help="debug logging",
    )
    return ap.parse_args()


def main() -> int:
    args = _parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(levelname)s %(name)s %(message)s",
    )

    topics_path = Path(args.topics)
    topics = _load_topics(topics_path)
    if args.only:
        allow = set(args.only)
        topics = [t for t in topics if t in allow]
        if not topics:
            LOG.error("no matching topics after --only filter")
            return 2

    cache_dir = Path(args.cache_dir)
    stats = crawl(topics, cache_dir)
    LOG.info("crawl stats: %s", stats)
    return 0


if __name__ == "__main__":
    sys.exit(main())
