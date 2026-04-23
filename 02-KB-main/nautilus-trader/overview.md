---
title: nautilus-trader — overview
tool: nautilus-trader
parent: index
---

# nautilus-trader — overview

NautilusTrader is an open-source, production-grade, Rust-native engine for
multi-asset, multi-venue algorithmic trading. It combines tick-level
backtesting, a deterministic matching engine, and a live trading runtime
that reuses the exact same strategy class without modification. Python
serves as the control plane; Rust (plus Cython in v1) handles the hot path.

## Three runtime paths, one domain model

| Path        | User code writes | Engine runs in | Best for |
|-------------|------------------|-----------------|----------|
| v1 legacy   | Python (Cython wrappers available) | Rust via Cython/C-ABI FFI | Full feature parity today; IB, tearsheets, config serialization |
| v2 Rust     | Rust             | Rust             | Latency-sensitive HFT; standalone binaries; no Python runtime |
| v2 PyO3     | Python           | Rust             | Python authoring + Rust engine performance; "native" strategies |

All three share the same `Price` / `Quantity` / `Order` / `Instrument` /
`OrderBook` types, so the domain model is invariant across paths.

## Why it matters for HyperFrequency

1. **Research-to-live parity.** The same `Strategy` subclass (or Rust
   trait impl) runs both in `BacktestNode` and on `TradingNode` / `LiveNode`.
   Only the `Clock` and venue adapter swap; the matching engine, message
   bus, cache, risk engine, and portfolio are identical instances.
2. **Tick-level realism.** Unlike vectorbtpro's bar-vectorized model,
   NautilusTrader processes one event at a time with an L2/L3 order book
   matcher. Partial fills, queue-position effects, and OCO/bracket
   behaviour are simulated faithfully.
3. **Multi-venue / multi-asset.** Separate `ExecutionClient` per venue;
   routing is implicit via `InstrumentId.venue`. A single strategy can
   quote on Binance, hedge on Bybit, and settle futures on Deribit.
4. **Fixed-point determinism.** `Price` and `Quantity` use 128-bit
   fixed-point on Linux/macOS (16 digits) / 64-bit on Windows (9 digits).
   Backtests are byte-reproducible across runs.
5. **Rust-optional.** The majority of HyperFrequency strategies author in
   Python first (v1) and migrate to Rust only when the latency budget
   demands it. The PyO3 boundary (`nautilus_trader.core.nautilus_pyo3`)
   plus `add_native_strategy` enable incremental migration.

## What it is not

- Not a vectorized backtester. For rapid parameter sweeps over large
  historical ranges, use `[[DeepTools/vectorbtpro]]` first, then port
  top-K candidates into NautilusTrader for realistic validation.
- Not a turnkey trading bot. There is no built-in UI, Telegram bridge,
  or "strategy marketplace"; it is a framework, not an application.
- Not a vendor-hosted service. There is no QuantConnect-like cloud; you
  run it on your own infrastructure.

## Canonical sources

- Upstream repo: [github.com/nautechsystems/nautilus_trader](https://github.com/nautechsystems/nautilus_trader)
- Docs site: [nautilustrader.io/docs/latest](https://nautilustrader.io/docs/latest/)
- Canonical wiki: `../../../docs/deep-tool-wiki/nautilus-trader/wiki.md`
- HF fork: `HyperFrequency/HF-nautilus_trader` (adds Hyperliquid fee model
  helpers and `FUNDING_RATE_8H` custom data type).
