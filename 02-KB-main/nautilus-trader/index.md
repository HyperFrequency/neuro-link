---
title: nautilus-trader — llm-wiki navigation
tool: nautilus-trader
upstream: nautechsystems/nautilus_trader
fork: HyperFrequency/HF-nautilus_trader
canonical_wiki: ../../../docs/deep-tool-wiki/nautilus-trader/wiki.md
languages: [python, rust]
runtime_paths: [v1_cython, v2_rust, v2_pyo3]
last_updated: 2026-04-20
---

# nautilus-trader — llm-wiki

Tree-navigable offline context for NautilusTrader. Mirrors the sitemap
at the top of the canonical `wiki.md`. Each subsystem has its own index
page with API signatures, minimal examples, linked pitfalls, and
cross-links.

Language coverage is explicitly marked on every leaf:

- `(py)`    — Python / Cython only (v1 legacy path).
- `(rust)`  — Rust only (v2 Rust path; no Python runtime needed).
- `(py+rust)` — exposed in both languages via PyO3; same domain object.

## Sitemap

```
nautilus-trader/
├── [[overview]]                       — what it is, 3-paragraph brief
├── Core/
│   ├── [[Core/NautilusKernel]]        (py+rust) DI container
│   ├── [[Core/TradingNode]]           (py+rust) live runtime
│   ├── [[Core/BacktestNode]]          (py+rust) backtest orchestrator
│   ├── [[Core/BacktestEngine]]        (py+rust) single-run engine
│   ├── [[Core/Clock]]                 (py+rust) TestClock / LiveClock
│   ├── [[Core/Environment]]           (py+rust) enum — Backtest/Sandbox/Live
│   └── [[Core/Runtime-paths]]         — v1 Cython vs v2 Rust vs v2 PyO3
├── Model/
│   ├── [[Model/ValueTypes]]           (py+rust) Price / Quantity / Money / Currency
│   ├── [[Model/Identifiers]]          (py+rust) InstrumentId / Venue / *Id
│   ├── [[Model/Instruments]]          (py+rust) all instrument classes
│   ├── [[Model/Data]]                 (py+rust) QuoteTick / TradeTick / Bar / Book
│   ├── [[Model/Orders]]               (py+rust) all Order classes
│   ├── [[Model/Events]]               (py+rust) OrderFilled / PositionOpened / ...
│   ├── [[Model/OrderBook]]            (py+rust) L1 / L2 / L3 book
│   ├── [[Model/Greeks]]               (py+rust) OptionGreeks / OptionChainSlice
│   └── [[Model/DeFi]]                 (rust)   Block / PoolSwap / ChainId
├── MessageBus/
│   ├── [[MessageBus/Overview]]        (py+rust) pub/sub spine
│   ├── [[MessageBus/Topics]]          (py+rust) data.*, events.*, custom.*
│   └── [[MessageBus/RedisBackend]]    (py+rust) durable streaming
├── Cache/
│   ├── [[Cache/Overview]]             (py+rust) in-memory state store
│   ├── [[Cache/RedisBackend]]         (py+rust)
│   └── [[Cache/PostgresBackend]]      (py+rust)
├── Execution/
│   ├── [[Execution/ExecutionEngine]]  (py+rust) command → client
│   ├── [[Execution/MatchingEngine]]   (py+rust) L2/L3 matcher
│   ├── [[Execution/MatchingCore]]     (py+rust) TIF / OCO / brackets
│   ├── [[Execution/OrderEmulator]]    (py+rust) client-side STOP / MIT / TRAIL
│   ├── [[Execution/OrderManager]]     (py+rust) per-strategy tracking
│   ├── [[Execution/Algorithms]]       (py+rust) TWAP; ExecAlgorithm base
│   ├── [[Execution/Reports]]          (py+rust) FillReport / OrderStatusReport
│   └── [[Execution/Reconciliation]]   (py+rust) cache ↔ venue state
├── Adapters/
│   ├── [[Adapters/Binance]]           (py+rust)
│   ├── [[Adapters/Bybit]]             (py+rust)
│   ├── [[Adapters/BitMEX]]            (py+rust)
│   ├── [[Adapters/Deribit]]           (py+rust)
│   ├── [[Adapters/Hyperliquid]]       (py+rust)  HF-fork: fee-model helpers
│   ├── [[Adapters/dYdX]]              (py+rust)
│   ├── [[Adapters/OKX]]               (py+rust)
│   ├── [[Adapters/Kraken]]            (py+rust)
│   ├── [[Adapters/Coinbase]]          (rust)    v2 Rust only
│   ├── [[Adapters/Polymarket]]        (py+rust)
│   ├── [[Adapters/Betfair]]           (py+rust)
│   ├── [[Adapters/ArchitectAX]]       (py+rust)
│   ├── [[Adapters/Databento]]         (py+rust) historical+live
│   ├── [[Adapters/Tardis]]            (py+rust) historical crypto ticks
│   ├── [[Adapters/InteractiveBrokers]] (py)     v1-only
│   ├── [[Adapters/Sandbox]]           (py+rust) offline simulator
│   └── [[Adapters/Blockchain]]        (rust)   DeFi / EVM
├── Backtest/
│   ├── [[Backtest/BacktestEngine]]    (py+rust) low-level
│   ├── [[Backtest/BacktestNode]]      (py+rust) high-level orchestrator
│   ├── [[Backtest/SimulatedExchange]] (py+rust)
│   ├── [[Backtest/FillModel]]         (py+rust)
│   ├── [[Backtest/FeeModel]]          (py+rust) incl. HF Hyperliquid helper
│   ├── [[Backtest/LatencyModel]]      (py+rust)
│   └── [[Backtest/Results]]           (py)      BacktestResult / stats
├── Persistence/
│   ├── [[Persistence/ParquetDataCatalog]]    (py+rust)
│   ├── [[Persistence/Wranglers]]             (py+rust)
│   ├── [[Persistence/StreamingFeatherWriter]] (py+rust)
│   ├── [[Persistence/RedisCacheDatabase]]    (py+rust)
│   ├── [[Persistence/PostgresCacheDatabase]] (py+rust)
│   └── [[Persistence/RedisMessageBusDatabase]] (py+rust)
├── Indicators/
│   ├── [[Indicators/Averages]]        (py+rust) SMA / EMA / WMA / Hull / Wilder / VIDYA
│   ├── [[Indicators/Momentum]]        (py+rust) RSI / Stochastics / CCI / ROC / Bias
│   ├── [[Indicators/Volatility]]      (py+rust) ATR / BollingerBands / VR / Pressure
│   ├── [[Indicators/Volume]]          (py+rust) OBV / VWAP / KVO / CMF
│   ├── [[Indicators/Trend]]           (py+rust) MACD / ADX / Aroon / ArcherMAs
│   └── [[Indicators/Custom]]          (py+rust) subclass base / trait
├── [[FFI/PyO3-boundary]]              — FFI crossing map; read before tuning hot paths
├── [[FFI/Cython-layer]]               — v1 legacy bridge (still default today)
├── Live/
│   ├── [[Live/LiveNode]]              (rust)   pure-Rust live binary
│   ├── [[Live/Async-queues]]          (py)     asyncio queue glue
│   └── [[Live/Reconciliation]]        (py+rust)
├── Testing/
│   ├── [[Testing/Stubs]]              (py+rust) audusd_sim / btcusdt_sim
│   └── [[Testing/Mocks]]              (py)
├── Walkthroughs/
│   ├── [[Walkthroughs/Backtest-Python]]
│   ├── [[Walkthroughs/Backtest-Rust]]
│   ├── [[Walkthroughs/Live-Python]]
│   ├── [[Walkthroughs/Live-Rust]]
│   ├── [[Walkthroughs/Python-to-Rust-port]]
│   ├── [[Walkthroughs/Nautilus-vs-vectorbtpro]]
│   ├── [[Walkthroughs/Market-making]]
│   └── [[Walkthroughs/Optuna-sweep]]
└── [[pitfalls]]                        — consolidated per-subsystem gotchas
```

→ **Canonical wiki body:** `../../../docs/deep-tool-wiki/nautilus-trader/wiki.md`
→ **RefGraph:** `../../../docs/deep-tool-wiki/nautilus-trader/assets/refgraph-py-rust.mmd`
→ **PyO3 boundary graph:** `../../../docs/deep-tool-wiki/nautilus-trader/assets/refgraph-pyo3-boundary.mmd`

## Subsystem availability matrix

Subsystems that are fully dual-implemented vs. partial.

| Subsystem        | v1 (py/Cython) | v2 Rust | v2 PyO3 | Notes |
|------------------|----------------|---------|---------|-------|
| Kernel           | yes            | yes     | yes     | DI container |
| MessageBus       | yes            | yes     | yes     | Redis backend optional |
| Cache            | yes            | yes     | yes     | Redis / Postgres backends |
| DataEngine       | yes            | yes     | yes     |       |
| ExecutionEngine  | yes            | yes     | yes     |       |
| RiskEngine       | yes            | yes     | yes     |       |
| MatchingEngine   | yes            | yes     | yes     |       |
| BacktestNode     | yes            | yes     | yes     |       |
| LiveNode         | yes            | yes     | yes     |       |
| Portfolio        | yes            | yes     | yes     |       |
| Indicators       | yes            | yes     | yes     | ~30 pyo3-exposed classes |
| Controller       | yes            | —       | —       | v1 only |
| Tearsheets       | yes            | —       | —       | v1 only (Plotly) |
| Config serialize | yes            | —       | —       | v1 only |
| IB adapter       | yes            | —       | —       | v1 only |
| Coinbase adapter | —              | yes     | —       | Rust v2 only |
| Blockchain/DeFi  | —              | yes     | —       | Rust v2 only |

## Status

- [x] `index.md` (this page) — sitemap populated
- [x] canonical `wiki.md` has matching sitemap header
- [x] refgraph assets generated
- [ ] per-leaf deep pages — populate from canonical wiki sections on demand
- [ ] `pitfalls.md` — consolidate from canonical "Limitations and Pitfalls"

## See also

- InfraNodus graph: `deep-tool-wiki-nautilus-trader`
- HF fork skill: `_skills/trading/nautilus-trader/`
- Nautilus-admin (separate tool): `[[llm-wiki/nautilus-admin/index]]`
- Strategy porting skill: `_skills/trading/strategy-translator`
