---
title: Portfolio subsystem
parent: [[../index]]
tool: vectorbtpro
subsystem: portfolio
last_updated: 2026-04-20
---

# Portfolio subsystem

Central simulation object. Every `Portfolio.from_*` constructor builds
the portfolio by dispatching to a Numba kernel in `portfolio/nb.py`.
After construction, the portfolio exposes computed properties and
records (orders, trades, positions, logs).

## Leaves

- `[[from_signals]]` — signal-driven simulation. Fills at signal-bar
  CLOSE by default. Supports `sl_stop`, `tp_stop`, `tsl_stop`, `dt_stop`,
  and price-based stops via `stop_entry_price` / `stop_exit_price`.
  Conflict resolution via `conflict_mode` (`Entry|Exit|Adjacent|Opposite|Ignore`).
- `[[from_orders]]` — order-driven; each row is an explicit order with
  `size`, `price`, `direction`. Supports fractional sizing via all
  `SizeType` variants.
- `[[from_order_func]]` — path-dependent, callable-per-bar. Slowest but
  most flexible. The `order_func_nb` receives a `Context` named tuple
  and returns an `Order` named tuple. Used for custom execution logic
  (iceberg orders, custom slippage models, regime-dependent sizing).
- `[[from_holding]]` — buy-and-hold reference. Used as the benchmark
  column in `pf.stats(agg_func=None)` comparisons.
- `[[stats]]` — DataFrame of Sharpe, Sortino, Calmar, max drawdown,
  win rate, profit factor, expectancy (~30 metrics).
- `[[plot]]` — Plotly chart of equity, drawdowns, annotated trade
  markers. Methods: `pf.plot()`, `pf.plot_cumulative_returns()`,
  `pf.plot_drawdowns()`, `pf.plot_trades()`.
- `[[trades]]` — `EntryTrades` / `ExitTrades` / `Trades` / `Positions`
  record accessors. `pf.trades.readable` returns a human-readable
  DataFrame with entry/exit timestamps, PnL, duration.
- `[[orders]]` — `Orders` records with fill price, fee, size, side.
- `[[logs]]` — `Logs` records for debugging (`log=True` during sim).
- `[[enums]]` — `SizeType`, `Direction`, `ConflictMode`, `StopType`,
  `OrderType`, `OrderSide`, `OrderStatus`, `PriceType`, etc. (see
  `pitfalls.md` for the Pine ↔ vbt mapping).
- `[[preparers]]` — `FSPreparer` (from-signals), `FOPreparer`
  (from-orders), `FOFPreparer` (from-order-func), `FDOFPreparer`
  (from-def-order-func). Internal; translate Python kwargs to
  jitted arrays.
- `[[nb]]` — Numba kernels: `simulate_nb`, `execute_order_nb`,
  `from_signals_nb`, `from_orders_nb`. Hot path.

## See also

- Canonical wiki § Portfolio Deep Dive:
  `hyperfrequency/docs/deep-tool-wiki/vectorbtpro/wiki.md#portfolio-deep-dive`
- RefGraph: `Portfolio` has 540 edges — largest hub in the codebase.
