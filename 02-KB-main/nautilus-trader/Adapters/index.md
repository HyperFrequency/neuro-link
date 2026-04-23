---
title: Adapters — venue clients
parent: nautilus-trader/index
---

# Adapters (per-venue data + execution clients)

Each adapter provides a `DataClient` and an `ExecutionClient` for a venue.
Most are dual-implemented (Cython/Python v1 plus Rust v2); a few are
single-path.

## Dual-language adapters (py+rust)

| Adapter             | Notes |
|---------------------|-------|
| `Binance`           | spot + USDT/COIN futures; `BinanceAccountType` enum. |
| `Bybit`             | v5 REST + WS; spot/linear/inverse/options. |
| `BitMEX`            | XBT / USDT perpetuals. |
| `Deribit`           | options + perpetuals; `OptionGreeks` support. |
| `Hyperliquid`       | native HL perps. **HF-fork:** fee-model helpers. |
| `dYdX`              | v4 on-chain perps; gRPC + ZK. |
| `OKX`               | unified account + USDT swap. |
| `Kraken`            | spot + futures; FOK + LimitIfTouched. |
| `Polymarket`        | prediction markets; Rust instrument provider. |
| `Betfair`           | exchange-style betting; streaming. |
| `Architect AX`      | institutional OEMS. |
| `Databento`         | historical + live; DBN binary decoded in Rust. |
| `Tardis`            | historical crypto ticks + live. |
| `Sandbox`           | offline simulator (uses `SimulatedExchange`). |

## Python-only (v1 legacy)

- `Interactive Brokers` — requires TWS / IB Gateway. Install via
  `pip install "nautilus_trader[ib,docker]"`.

## Rust-only (v2 Rust path)

- `Coinbase` — INTX advanced trade API, v2 Rust crate.
- `Blockchain` — DeFi / EVM; DEX pool data; blockchain-adapter crate.

## HF-fork enhancements

The `HyperFrequency/HF-nautilus_trader` fork adds
`configure_hyperliquid_fee_model(maker_bps, taker_bps, ...)` on top of the
standard Hyperliquid adapter, plus `FUNDING_RATE_8H` as a first-class
custom data type for perp PnL simulation across overnight holds.

## Cross-links

- Canonical wiki §"Adapters and Integrations"
- Rust source: `crates/adapters/<venue>/`
- Python source: `nautilus_trader/adapters/<venue>/`
