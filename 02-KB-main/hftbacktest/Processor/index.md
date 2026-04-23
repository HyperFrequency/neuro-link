---
title: Processor — subsystem index
parent: [[../index]]
tool: hftbacktest
last_updated: 2026-04-20
---

# Processor/

Two traits split the simulation into a local view (what the strategy sees,
subject to feed latency) and an exchange view (the ground-truth book,
running the fill simulator). Source: `hftbacktest/src/backtest/proc/`.

## Leaves

- [[LocalProcessor]] — trait; stock impl `Local<MD,FM,AT>` holds the
  local `MarketDepth`, the `OrderBus<LM>`, and the `StateValues`.
- [[ExchangeProcessor]] — trait; applies `QueueModel` and issues fills.
- [[NoPartialFillExchange]] — default exchange sim; rejects FOK/IOC/MARKET.
- [[PartialFillExchange]] — supports FOK/IOC/MARKET with partial fills.
- [[L3Processors]] — `L3Local` + `L3NoPartialFillExchange` for MBO feeds
  with `L3FIFOQueueModel`.

## Canonical wiki sections

`wiki.md#Architecture-Deep-Dive` (Processor Architecture),
`wiki.md#Exchange-Models-in-Detail`.

## When does `LocalProcessor` suffice vs needing a custom impl?

The stock `Local<MD,FM,AT>` handles: local book updates, OrderBus
routing, `State` accounting (position/balance/fees), and
`clear_inactive_orders`. It's generic over every model trait, so you
swap behavior by changing type parameters on `BacktestAsset`, not by
implementing the trait.

Manual `impl LocalProcessor` is only needed for: custom order-book
invariants (e.g., maintaining a derived top-N summary), extra
bookkeeping the strategy needs on every event, or a non-`Event` feed
format. None of these apply to standard HFT research; use the stock
impl.

## When does `ExchangeProcessor` suffice vs needing a custom impl?

`NoPartialFillExchange` + any `QueueModel` covers maker-only market
making and simple taker strategies. Use `PartialFillExchange` when
taker orders need FOK/IOC/MARKET semantics with partial fills.

Manual `impl ExchangeProcessor` is needed for: exotic matching
(periodic auctions, hidden liquidity, iceberg orders), venue-specific
post-only rules beyond `GTX`, or fill models that depend on more than
`QueueModel::is_filled`.
