---
title: Live — subsystem index
parent: [[../index]]
tool: hftbacktest
last_updated: 2026-04-20
---

# Live/

Live-trading runtime. Rust-only at the outer boundary: the `LiveBot` trait
mirrors the shape of `Backtest<MD>` so the **same strategy crate compiles
against both**. Exchange connectors are separate processes that exchange
messages with the bot over Iceoryx2 zero-copy shared-memory IPC. Source:
`hftbacktest/src/live/`.

## Leaves

- [[LiveBot]] — trait aligning with `Backtest` shape.
- [[Iceoryx2]] — zero-copy shared-memory transport; roudi daemon +
  publisher/subscriber services.
- [[Connectors]] — `binance_futures`, `bybit`, and `hyperliquid` (WIP).

## Canonical wiki sections

`wiki.md#Supported-Exchanges`, `wiki.md#Architecture-Deep-Dive` (live
subsection), `wiki.md#Tutorials-Index` (Live trading tutorial).

## Python vs Rust at the live boundary

Backtesting exposes a clean Python + Numba strategy surface. **Live
trading is Rust-only at the outer layer** — the `LiveBot` is spawned from
Rust and communicates with connectors via Iceoryx2. Strategy code that
exercises only the shared shape (`bot.elapse`, `bot.depth`, `bot.orders`,
`bot.submit_*`, `bot.wait_order_response`) compiles unchanged against
either engine; the Python/Numba path is for research, the Rust path for
deployment.
