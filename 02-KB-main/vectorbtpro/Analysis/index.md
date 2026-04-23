---
title: Analysis subsystem
parent: [[../index]]
tool: vectorbtpro
subsystem: analysis
last_updated: 2026-04-20
---

# Analysis subsystem

Post-simulation metrics, plots, and record accessors. Every analysis
method is available both via `Portfolio.*` and via the relevant `.vbt.*`
accessor on standalone arrays.

## Leaves

- `[[stats]]` — `pf.stats()` returns a DataFrame of ~30 metrics per
  column: Start/End, Period, Start Value, End Value, Total Return,
  Benchmark Return, Max Drawdown, Sharpe, Sortino, Calmar, Omega,
  Profit Factor, Expectancy, Win Rate, Avg/Max Trade PnL, Trade
  Duration, etc. Accepts `metrics=[...]`, `settings={...}`,
  `agg_func=...` to customize.
- `[[plot]]` — `pf.plot()` renders interactive Plotly equity +
  drawdown + trade markers. Sub-plots:
  `pf.plot_cumulative_returns()`, `pf.plot_drawdowns()`,
  `pf.plot_trade_pnl()`, `pf.plot_orders()`.
- `[[trades]]` — `EntryTrades` / `ExitTrades` / `Trades` / `Positions`.
  `pf.trades.readable` is a human-friendly DataFrame. Metrics:
  `.pnl`, `.duration`, `.return_`, `.winning.count()`, `.losing.count()`.
- `[[orders]]` — `Orders` records. `pf.orders.records_readable` shows
  timestamp, price, size, side, fee.
- `[[logs]]` — `Logs` records. Only populated when `log=True` during
  simulation. Columns: `idx`, `col`, `group`, `cash_before`,
  `position_before`, `cash_after`, `position_after`, `order_result`.
- `[[returns]]` — `pf.returns` yields a `ReturnsAccessor`-wrapped
  series. Methods: `.sharpe_ratio()`, `.sortino_ratio()`,
  `.calmar_ratio()`, `.omega_ratio()`, `.value_at_risk()`,
  `.conditional_value_at_risk()`, `.deflated_sharpe_ratio()` (PRO).
- `[[drawdowns]]` — `Drawdowns` records via `pf.drawdowns`. Columns:
  `start_idx`, `valley_idx`, `end_idx`, `valley_val`, `recovery_duration`.
- `[[tearsheet]]` — quantstats-style HTML export via
  `pf.returns.stats()` + custom HTML template. PRO adds native
  tearsheet generator integrated with the `knowledge` subsystem.
- `[[ranges]]` — `Ranges` / `PatternRanges` for analyzing
  range-structured events (e.g. winning streaks, breakout durations).
- `[[records]]` — base `Records` / `MappedArray` operations:
  `.mask`, `.overlaps`, `.to_pd()`, `.readable`.

## Aggregation across sweeps

```python
pf = vbt.Portfolio.from_signals(close, entries, exits)
# pf.stats() returns one row per column (strategy variant)
summary = pf.stats(agg_func='mean')     # aggregate metrics across columns
per_param = pf.stats(group_by='window') # aggregate by param-level
```

## See also

- Canonical wiki § Returns Analysis / Drawdowns:
  `hyperfrequency/docs/deep-tool-wiki/vectorbtpro/wiki.md#returns-analysis`
- [[DeepTools/hyper-stats]] — bootstrap confidence intervals, regime tests.
