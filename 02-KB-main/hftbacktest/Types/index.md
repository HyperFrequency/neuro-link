---
title: Types — subsystem index
parent: [[../index]]
tool: hftbacktest
last_updated: 2026-04-20
---

# Types/

Canonical types crossing every module. Lives in `hftbacktest/src/types/`
and re-exported from the `prelude`.

## Leaves

- [[Event]] — 8-field struct `(ev, exch_ts, local_ts, px, qty, order_id,
  ival, fval)`; the NPZ `event_dtype`.
- [[Order]] — lifecycle struct with `order_id`, `price_tick`, `qty`,
  `leaves_qty`, `status`, `cancellable`, `exec_qty`.
- [[TimeInForce]] — `GTC` / `GTX` (post-only) / `FOK` / `IOC`.
- [[Constants]] — event flags (`EXCH_EVENT`, `LOCAL_EVENT`, `BUY_EVENT`,
  `SELL_EVENT`, `DEPTH_EVENT`, `TRADE_EVENT`, `DEPTH_SNAPSHOT_EVENT`,
  `DEPTH_CLEAR_EVENT`).

## Canonical wiki sections

`wiki.md#Tick`, `wiki.md#Order`, `wiki.md#TimeInForce`,
`wiki.md#Data-Format-Reference`.
