---
title: Backtest — engine, node, models
parent: nautilus-trader/index
---

# Backtest (Python + Rust, dual-exposed)

## Leaves

- **BacktestEngine** (`py+rust`) — low-level. Add venues, instruments,
  data, strategies manually; call `.run()`. All data in memory.
- **BacktestNode** (`py+rust`) — high-level orchestrator. Takes a
  `list[BacktestRunConfig]`; supports parallel runs and catalog-backed
  streaming.
- **SimulatedExchange** (`py+rust`) — the in-process venue. Routes orders
  through `MatchingEngine` with configurable `FillModel`, `FeeModel`,
  `LatencyModel`, and `BookType` (L1/L2/L3).
- **FillModel** (`py+rust`) — determines whether a resting order fills on
  a given market update. Default: optimistic (fill at mid if price crosses).
  Use `PartialFillModel` for more realistic crypto simulation.
- **FeeModel** (`py+rust`) — maker/taker bps or flat-rate fees. HF fork
  ships a `configure_hyperliquid_fee_model` helper.
- **LatencyModel** (`py+rust`) — adds deterministic delay between
  `submit_order` and venue receipt.
- **Results** (`py`) — `BacktestResult` with `stats_pnls`, `stats_returns`,
  per-venue `Account.balances`, and order / fill frames.

## Run patterns

```python
# BacktestEngine (low-level)
engine = BacktestEngine(config)
engine.add_venue(...); engine.add_instrument(...); engine.add_data(...)
engine.add_strategy(MyStrategy(MyConfig(...)))
engine.run()

# BacktestNode (high-level, data from catalog)
node = BacktestNode(configs=[BacktestRunConfig(...)])
results = node.run()
```

## Pitfalls

- `engine.reset()` requires `on_reset()` to explicitly clear custom
  state (indicator buffers, counters). Missing resets cause second-run
  divergence.
- Streaming mode (`BacktestNode` + catalog) requires the `streaming`
  Cargo feature in Rust contexts.
- `FillModel` default is optimistic; use a queue-position-aware model
  (`hftbacktest`-calibrated) for market-making studies.

## Cross-links

- Canonical wiki §"Detailed Usage Guide 1. Simple backtest" + §"Detailed Usage Guide 4. Multi-instrument backtest"
- Rust source: `crates/backtest/`
- `[[Walkthroughs/Backtest-Python]]`, `[[Walkthroughs/Backtest-Rust]]`
