---
title: Backtest — subsystem index
parent: [[../index]]
tool: hftbacktest
last_updated: 2026-04-20
---

# Backtest/

Rust-core event-driven simulator. Top-level engine is `Backtest<MD>`
(`hftbacktest/src/backtest/mod.rs`). Driven by `EventSet`, a ns-precision
priority queue over `(asset_no, event_type, timestamp)` tuples. Each asset
owns one `LocalProcessor` and one `ExchangeProcessor`; strategy code
(Python `@njit`) calls into this layer exclusively.

## Leaves

- [[Backtest]] — top-level `Backtest<MD>` with `goto<WAIT_NEXT_FEED>(ts)`.
- [[BacktestAsset]] — fluent builder; macro `build_asset!` expands per
  `(LatencyModel, QueueModel, AssetType, FeeModel, MD)` combo.
- [[EventSet]] — the ns-precision global priority queue; owns event ordering.
- [[OrderBus]] — FIFO channel that applies `LatencyModel::entry` /
  `::response`.
- [[MultiAsset]] — multi-asset and multi-exchange topology notes.

## Canonical wiki sections

`wiki.md#HftBacktest`, `wiki.md#MultiAssetMultiExchangeBacktest`,
`wiki.md#Architecture-Deep-Dive`.

## When is the `Processor` trait manually implemented?

The stock `Local<MD,FM,AT>` `impl LocalProcessor` and
`NoPartialFillExchange<QM,LM,FM,AT>` / `PartialFillExchange` `impl
ExchangeProcessor` cover **every canonical use case** — they are generic
over all four model traits (`QueueModel`, `LatencyModel`, `FeeModel`,
`AssetType`) via the `build_asset!` macro. Manually implementing
`LocalProcessor` or `ExchangeProcessor` is only required when:

1. Simulating a venue-specific matching rule that neither
   `NoPartialFillExchange` nor `PartialFillExchange` captures (e.g., hidden
   order types, periodic batch auctions, price-time-pro-rata matching).
2. Integrating a fill-model variant beyond the queue model (e.g., custom
   slippage on taker fills that depends on book shape beyond what
   `QueueModel::is_filled` sees).
3. Driving the engine off a non-`Event` input (e.g., an in-memory stream).

For 99% of HFT/market-making research, `Local` + `NoPartialFillExchange` +
a `build_asset!` combo is sufficient. `LocalProcessor` as a standalone
trait is most useful as an extension point, not a daily-driver entry.
