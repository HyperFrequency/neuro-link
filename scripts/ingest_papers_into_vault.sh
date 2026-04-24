#!/usr/bin/env bash
# ingest_papers_into_vault.sh — bootstrap the 4 foundational market-making
# papers into vaults/papers/<slug>/ and feed them to the pdf-ingest pipeline.
#
# Paper manifest (name, slug, open-access URL, fallback notes). URLs that
# are gated or unstable are listed as PLACEHOLDER — the script emits a
# stub README.md with retrieval instructions instead of fabricating content.
#
# On success: vaults/papers/<slug>/{source.pdf,README.md} exist, and
# nlr_pdf_ingest has been invoked on each PDF, which populates
# 01-raw/<slug>/, 01-sorted/<domain>/<slug>.md, and enqueues for curation.
#
# Env:
#   NLR_BIN         — path to the neuro-link binary (default: auto-detect)
#   NLR_ROOT        — vault root (default: repo root)
#   SKIP_INGEST=1   — download only; don't call nlr_pdf_ingest
#   SKIP_DOWNLOAD=1 — skip downloads; just ingest whatever's already on disk

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NLR_ROOT="${NLR_ROOT:-$(cd "$SCRIPT_DIR/.." && pwd)}"
NLR_BIN="${NLR_BIN:-$NLR_ROOT/server/target/release/neuro-link}"
PAPERS_DIR="$NLR_ROOT/vaults/papers"
SKIP_INGEST="${SKIP_INGEST:-0}"
SKIP_DOWNLOAD="${SKIP_DOWNLOAD:-0}"

log() { printf "[papers-ingest] %s\n" "$*" >&2; }

mkdir -p "$PAPERS_DIR"

# Paper manifest. Format: slug | title | url | kind
#
# kind:
#   OPEN      — direct PDF download via curl; script fetches source.pdf.
#   PAYWALLED — writes a placeholder README; user fetches manually.
#
# URLs selected as best-known open-access mirrors as of 2026-04-24. If a
# URL returns a non-PDF body or 404, the script falls back to placeholder
# mode for that paper (and logs the failure) — it never fabricates.
PAPERS=(
  "avellaneda-stoikov-2008|High-frequency trading in a limit order book|https://www.math.nyu.edu/~avellane/HighFrequencyTrading.pdf|OPEN"
  "almgren-chriss-2001|Optimal Execution of Portfolio Transactions|https://www.ram-ahluwalia.com/wp-content/uploads/2020/05/optliq.pdf|OPEN"
  "foucault-kadan-kandel-2005|Limit Order Book as a Market for Liquidity|https://papers.ssrn.com/sol3/Delivery.cfm/SSRN_ID347440_code020814670.pdf?abstractid=347440&mirid=1|OPEN"
  "kyle-1985|Continuous Auctions and Insider Trading|https://www.jstor.org/stable/1913210|PAYWALLED"
)

download_paper() {
  local slug="$1" title="$2" url="$3" kind="$4"
  local paper_dir="$PAPERS_DIR/$slug"
  mkdir -p "$paper_dir"
  local pdf_path="$paper_dir/source.pdf"

  if [[ -f "$pdf_path" ]]; then
    log "  $slug: source.pdf already present — skip download"
    return 0
  fi

  case "$kind" in
    OPEN)
      log "  $slug: downloading from $url"
      if curl -sSL --max-time 60 -o "$pdf_path.tmp" "$url"; then
        # Verify we got a PDF (first bytes should be %PDF-).
        if head -c 5 "$pdf_path.tmp" | grep -q '%PDF-'; then
          mv "$pdf_path.tmp" "$pdf_path"
          log "    OK: $(stat -f%z "$pdf_path" 2>/dev/null || stat -c%s "$pdf_path") bytes"
        else
          log "    FAIL: response is not a PDF (HTML error page or gated landing?)"
          rm -f "$pdf_path.tmp"
          write_placeholder "$paper_dir" "$slug" "$title" "$url" "Download returned non-PDF content. Fetch manually from the paper URL or author's page."
        fi
      else
        log "    FAIL: curl returned non-zero"
        rm -f "$pdf_path.tmp"
        write_placeholder "$paper_dir" "$slug" "$title" "$url" "curl download failed (network error or 404). Retry manually or fetch from an alternate mirror."
      fi
      ;;
    PAYWALLED)
      log "  $slug: PAYWALLED — emitting placeholder (user fetches manually)"
      write_placeholder "$paper_dir" "$slug" "$title" "$url" "This paper is behind a paywall (JSTOR). Fetch the PDF manually through your institutional access and save as source.pdf alongside this README."
      ;;
    *)
      log "  $slug: unknown kind '$kind' — skip"
      ;;
  esac
}

write_placeholder() {
  local paper_dir="$1" slug="$2" title="$3" url="$4" reason="$5"
  cat > "$paper_dir/README.md" <<README
---
title: "$title"
slug: $slug
status: placeholder
source_url: $url
reason: "$reason"
added: $(date -u +%Y-%m-%d)
---

# $title

**Status: placeholder.** No PDF fetched yet.

## Why this file exists

$reason

## Fetching manually

Download the PDF and save as \`source.pdf\` in this directory:

\`\`\`
vaults/papers/$slug/source.pdf
\`\`\`

Then re-run the ingest script with \`SKIP_DOWNLOAD=1\` to index without
re-attempting the download.

## Source URL (may be gated)

$url
README
}

ingest_paper() {
  local slug="$1"
  local pdf_path="$PAPERS_DIR/$slug/source.pdf"
  if [[ ! -f "$pdf_path" ]]; then
    log "  $slug: no source.pdf — skipping ingest"
    return 0
  fi
  if [[ ! -x "$NLR_BIN" ]]; then
    log "  $slug: NLR_BIN not executable at $NLR_BIN — skipping ingest"
    log "    build via: cd server && cargo build --release"
    return 0
  fi
  log "  $slug: invoking nlr_pdf_ingest"
  # The neuro-link binary exposes pdf ingest via the MCP stdio transport;
  # here we call it via the one-shot cli variant if present.
  if "$NLR_BIN" ingest-pdf --help >/dev/null 2>&1; then
    "$NLR_BIN" ingest-pdf --path "$pdf_path" --target-domain quant 2>&1 | tail -5 || \
      log "    ingest returned non-zero (check logs)"
  else
    log "    NLR_BIN doesn't expose 'ingest-pdf' CLI subcommand (MCP-only)."
    log "    The MCP tool nlr_pdf_ingest is available through Claude Code; invoke manually."
  fi
}

# --- Main loop ---
log "Papers dir: $PAPERS_DIR"
log "NLR_ROOT:   $NLR_ROOT"
log "NLR_BIN:    $NLR_BIN"

for row in "${PAPERS[@]}"; do
  IFS='|' read -r slug title url kind <<<"$row"
  log "=== $slug ($title) ==="
  if [[ "$SKIP_DOWNLOAD" != "1" ]]; then
    download_paper "$slug" "$title" "$url" "$kind"
  fi
  if [[ "$SKIP_INGEST" != "1" ]]; then
    ingest_paper "$slug"
  fi
done

log "DONE."
log "Review: ls -la $PAPERS_DIR/*/"
log "Re-embed after ingest: $NLR_BIN embed   (or: nlr_rag_rebuild_index via MCP)"
