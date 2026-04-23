---
title: hftbacktest — overview
parent: [[index]]
tool: hftbacktest
last_updated: 2026-04-20
---

# hftbacktest — overview

hftbacktest is a tick-level backtesting and live-trading framework purpose-built
for high-frequency and market-making strategies on crypto perpetuals. The
architecture is **Rust-native end-to-end**: the `hftbacktest` crate owns the
nanosecond-precision event loop (`Backtest<MD>::goto`), the order book backends
(`HashMapMarketDepth`, `ROIVectorMarketDepth`, `BTreeMarketDepth`,
`FusedHashMapMarketDepth`), the dual-processor simulation (`LocalProcessor`
applies feed latency and holds strategy-visible state; `ExchangeProcessor`
holds the true book plus `QueueModel` and runs fill checks), and the
trait-generic model families (`QueueModel`, `LatencyModel`, `FeeModel`,
`AssetType`). The sibling `py-hftbacktest` crate uses **PyO3** to expose the
Rust structs to CPython and is packaged into a `_hftbacktest` wheel with
**maturin 0.27.2**; the resulting shared library allows user strategies
written as Python **Numba `@njit`** functions to call directly into Rust
through raw `usize` pointers and Numba-compatible struct surfaces, yielding
near-zero interpreter overhead on the hot path. For live trading the same
strategy shape runs inside a `LiveBot` (Rust-only at the outer layer) that
talks to Binance Futures and Bybit connectors via **Iceoryx2** zero-copy
shared-memory IPC; there is no C++ anywhere in the codebase. Full canonical
reference: `hyperfrequency/docs/deep-tool-wiki/hftbacktest/wiki.md`
(~2,000 lines with sitemap, mental-model diagram, deep concept glossary,
four walkthroughs, full API surface, queue-model mathematics, dual reasoning
ontology, and RefGraph `.mmd` renders).
