---
title: hftbacktest — llm-wiki navigation
tool: hftbacktest
upstream: nkaz001/hftbacktest
fork: HyperFrequency/hftbacktest
canonical_wiki: ../../../hyperfrequency/docs/deep-tool-wiki/hftbacktest/wiki.md
last_updated: 2026-04-20
---

# hftbacktest — llm-wiki

Tree-navigable offline context for hftbacktest. Mirrors the sitemap at
the top of the canonical `wiki.md`; each leaf links to a per-page deep
stub. The runtime is **Rust core + PyO3 bindings + Numba-jitted user
strategies** — no C++ exists in the codebase.

## Sitemap

```text
hftbacktest/
├── Backtest/                                (rust) event-driven simulator
│   ├── [[Backtest/Backtest]]                — top-level engine; goto<WAIT_NEXT_FEED>
│   ├── [[Backtest/BacktestAsset]]           — per-asset builder (macro-expanded)
│   ├── [[Backtest/EventSet]]                — global ns-precision priority queue
│   ├── [[Backtest/OrderBus]]                — FIFO bus; applies LatencyModel
│   └── [[Backtest/MultiAsset]]              — multi-asset, multi-exchange interleaving
├── Depth/                                   (rust) MarketDepth backends
│   ├── [[Depth/HashMapMarketDepth]]         — O(1) map, O(n) best scan
│   ├── [[Depth/ROIVectorMarketDepth]]       — O(1) everywhere inside ROI
│   ├── [[Depth/BTreeMarketDepth]]           — ordered iteration
│   ├── [[Depth/FusedHashMapMarketDepth]]    — per-level ts; fuses L2+BBO
│   └── [[Depth/snapshots]]                  — initial_snapshot + ApplySnapshot
├── Live/                                    (rust) live-trading runtime
│   ├── [[Live/LiveBot]]                     — trait sharing shape with Backtest
│   ├── [[Live/Iceoryx2]]                    — zero-copy shared-memory IPC
│   └── [[Live/Connectors]]                  — binance_futures, bybit, hyperliquid(WIP)
├── Types/                                   (rust) canonical types
│   ├── [[Types/Event]]                      — 8-field struct; event_dtype
│   ├── [[Types/Order]]                      — lifecycle + OrdStatus
│   ├── [[Types/TimeInForce]]                — GTC/GTX/FOK/IOC
│   └── [[Types/Constants]]                  — EXCH|LOCAL|BUY|SELL|DEPTH|TRADE|…
├── Processor/                               (rust) local + exchange views
│   ├── [[Processor/LocalProcessor]]         — local-view book + OrderBus + State
│   ├── [[Processor/ExchangeProcessor]]      — fill sim + QueueModel + OrderBus
│   ├── [[Processor/NoPartialFillExchange]]  — default exchange sim
│   ├── [[Processor/PartialFillExchange]]    — FOK/IOC/MARKET support
│   └── [[Processor/L3Processors]]           — MBO per-order queue sim
├── LatencyModel/                            (rust) LatencyModel trait family
│   ├── [[LatencyModel/ConstantLatency]]     — fixed ns entry/response
│   ├── [[LatencyModel/IntpOrderLatency]]    — interp NPZ of req/exch/resp
│   └── [[LatencyModel/FeedLatencyAdjustment]] — preprocessor on local_ts
├── QueueModel/                              (rust) QueueModel trait family
│   ├── [[QueueModel/ProbQueueModel]]        — parametric Power/Log/Identity
│   ├── [[QueueModel/RiskAdverseQueueModel]] — trade-only; conservative
│   └── [[QueueModel/L3FIFOQueueModel]]      — per-order FIFO for MBO
├── FillModel/                               (rust) exchange-side fill logic
│   └── [[FillModel/QueueToFill]]            — is_filled() + qty_ahead semantics
├── PyO3/                                    (py+rust) py-hftbacktest crate
│   ├── [[PyO3/Maturin]]                     — maturin 0.27.2 wheel build
│   ├── [[PyO3/PyClasses]]                   — #[pyclass] surface; raw usize ptrs
│   └── [[PyO3/NumbaSurface]]                — struct-ref types consumed under @njit
├── Recorder/                                (py) state capture + asset records
│   ├── [[Recorder/Recorder]]                — periodic StateValues snapshots
│   └── [[Recorder/LinearInverseRecord]]     — per-asset-type PnL accounting
└── Analysis/                                (py) stats + reporting
    ├── [[Analysis/Stats]]                   — Sharpe/Sortino/MDD/ROMDD
    └── [[Analysis/EquityCurve]]             — post-run array shape
```

→ **Canonical wiki body:** `hyperfrequency/docs/deep-tool-wiki/hftbacktest/wiki.md`
→ **RefGraphs:** `hyperfrequency/docs/deep-tool-wiki/hftbacktest/assets/refgraph-rust-crates.mmd`, `refgraph-pyo3-boundary.mmd`
→ **InfraNodus graph:** `deep-tool-wiki-hftbacktest`
→ **Consolidated pitfalls:** [[pitfalls]]
→ **Overview:** [[overview]]

## Status

- [x] `index.md` (this page) — sitemap populated
- [x] `overview.md` — populated
- [x] `pitfalls.md` — populated
- [x] Subsystem indexes: `Backtest/`, `Depth/`, `Live/`, `Types/`,
      `Processor/`, `LatencyModel/`, `QueueModel/`, `FillModel/`,
      `PyO3/`, `Recorder/`, `Analysis/` — placeholder index pages
      pending per-leaf expansion

## See also

- Canonical deep-tool-wiki: `../../hyperfrequency/docs/deep-tool-wiki/hftbacktest/wiki.md`
- Cross-tool links: [[../nautilus-trader/index]] (production exec),
  [[../vectorbtpro/index]] (bar-level backtesting),
  [[../tardis-python/index]] (NPZ data converter pipeline)
