---
title: Depth — subsystem index
parent: [[../index]]
tool: hftbacktest
last_updated: 2026-04-20
---

# Depth/

MarketDepth backends. All implement the `MarketDepth` trait (best bid/ask,
tick/lot size); L2 and L3 sub-traits add update/add/delete operations.
Source: `hftbacktest/src/depth/`.

## Leaves

- [[HashMapMarketDepth]] — default; O(1) per-level, O(n) best-bid/ask scan.
- [[ROIVectorMarketDepth]] — region-of-interest vector; O(1) everywhere
  inside the configured tick bounds; silently drops updates outside.
- [[BTreeMarketDepth]] — sorted tree; O(log n); ordered iteration of levels.
- [[FusedHashMapMarketDepth]] — per-level timestamps; fuses L2 depth +
  book-ticker streams via `BboBuilder`.
- [[snapshots]] — `DEPTH_SNAPSHOT_EVENT` + `DEPTH_CLEAR_EVENT`, plus
  `ApplySnapshot` trait.

## Canonical wiki sections

`wiki.md#MarketDepth`, `wiki.md#MarketDepthBuilder`, `wiki.md#BboBuilder`,
`wiki.md#Snapshot`.

## Choosing the backend

- Default: `HashMapMarketDepth` (via `HashMapMarketDepthBacktest(assets)`).
- High-frequency market making with quotes near mid: `ROIVectorMarketDepth`
  (via `ROIVectorMarketDepthBacktest(assets, roi_lb, roi_ub)`).
- Need ordered iteration over all levels (e.g., depth-weighted metrics
  beyond best): `BTreeMarketDepth`.
- Combining Binance `depth@100ms` with `bookTicker` in one stream:
  `FusedHashMapMarketDepth` (selected automatically when fused data is
  present).
