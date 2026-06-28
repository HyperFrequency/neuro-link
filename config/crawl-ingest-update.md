---
version: 1
dedup: true
dedup_method: sha256
auto_classify: true
auto_curate: true
llm_assisted_source_discovery: true
extraction_strategies:
  url: firecrawl
  git_repo: shallow_clone_docs
  pdf: markitdown
  local_file: copy
  obsidian_note: turbovault_read
  youtube: transcript_extract
classification_rules:
  - pattern: "arxiv.org"
    domain: arxiv
  - pattern: "medium.com"
    domain: medium
  - pattern: "huggingface.co"
    domain: huggingface
  - pattern: "github.com"
    domain: github
  - pattern: ".pdf"
    domain: docs
  - pattern: "youtube.com|youtu.be"
    domain: docs
default_domain: docs
---

# crawl-ingest-update Configuration

Defines what to ingest, how to extract, and how to classify.

## Source Tables

Edit these tables to define your ingestion sources. The job-scanner will process them.

### Table 1 — Direct Ingest (Markdown + Images)

| Source Path | Domain | Status | Last Ingested |
|-------------|--------|--------|---------------|
| | | | |

### Table 2 — Single Page Crawl

| URL | Domain | Status | Last Crawled |
|-----|--------|--------|--------------|
| | | | |

### Table 3 — Full Site Crawl

| Base URL | Domain | Max Depth | Status | Last Crawled |
|----------|--------|-----------|--------|--------------|
| | | | | |

### Table 4 — Monitored Sites (Auto-Update)

| Base URL | Domain | Check Interval | Status | Last Crawled | Last Checked |
|----------|--------|----------------|--------|--------------|--------------|
| | | | | | |

## Extraction Strategies

| Source Type | Strategy | Tool | Notes |
|-------------|----------|------|-------|
| URL | Firecrawl MCP | `firecrawl` | Respects robots.txt, handles JS rendering |
| Git repo | Shallow clone + extract docs/ README | `bash` | Only clones docs, not full history |
| PDF | Markitdown conversion | `markitdown` skill | OCR support for scanned PDFs |
| Local file | Direct copy | `cp` | Preserves original in 00-raw/ |
| Obsidian note | TurboVault read | `turbovault` | Reads from configured vault |
| YouTube | Transcript extraction | `firecrawl` or `yt-dlp` | Extracts auto-captions |

## Deduplication

When `dedup: true`, each ingested file gets a SHA256 hash stored in `00-raw/.hashes`. If a file with the same hash already exists, it's skipped.

## Classification

Raw files are classified into `01-sorted/<domain>/` based on `classification_rules`. Rules are checked in order; first match wins. If no rule matches, the file goes to `01-sorted/<default_domain>/`.

## LLM-Assisted Source Discovery

When `llm_assisted_source_discovery: true`, after ingesting a source, the LLM analyzes it for references to other sources worth ingesting. Discovered sources are added to Table 2 or Table 4 as pending entries.
