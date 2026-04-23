---
title: Indicators subsystem
parent: [[../index]]
tool: vectorbtpro
subsystem: indicators
last_updated: 2026-04-20
---

# Indicators subsystem

The `IndicatorFactory` constructs array-oriented technical indicators
that support parameter sweeps via `vbt.Param(...)`. Every indicator
returns an object whose output columns are a broadcast product of
input columns × parameter combinations.

## Leaves

- `[[IndicatorFactory]]` — the DSL builder. Four construction modes:
  - `from_expr(...)` — string expression like `"close > upper"`.
  - `from_apply_func(...)` — per-column Python callable wrapped in Numba.
  - `from_custom_func(...)` — full control over the kernel.
  - `from_talib(...)` / `from_pandas_ta(...)` — wrap TA-Lib / pandas-ta
    indicators with vectorbtpro parameterization.
- `[[RSI]]` — Wilder-smoothed RSI. Matches Pine `ta.rsi` 1:1. Parameters:
  `window` (sweep-safe).
- `[[BBANDS]]` — Bollinger bands with SMA basis. Defaults to `ddof=0`
  at the NB level to match Pine `ta.stdev`. Parameters: `window`,
  `alpha` (std multiplier).
- `[[ATR]]` — Wilder-smoothed ATR. Matches Pine `ta.atr`. Parameters:
  `window`.
- `[[MACD]]` — EMA-of-EMA MACD. Pass `adjust=False` for Pine parity
  (default is `adjust=True` to match pandas).
- `[[MA]]` — Moving average with configurable windowing: SMA, EMA,
  WMA, VWMA. Parameter: `window`, `wtype` (enum: `Simple|Exp|Weighted|Wilder|Vidya`).
- `[[STOCH]]` — Stochastic oscillator (`%K`, `%D`).
- `[[ADX]]` — Average directional index + DI+/DI-. Parameters: `window`.
- `[[OBV]]` — On-balance volume.
- `[[custom]]` — custom indicators (ADX, ADL, ATR, BBANDS, BOLB, HURST,
  KAMA, OLS, PATSIM, PIVOT, PIVOTINFO, PROBS, RANDX, RSI, SIGDET, SMI,
  STCH, STOCH, SUPERTREND, VIDYA, VWAP, VWMA, WAE, etc.).
- `[[nb]]` — NB-level kernels: `rsi_nb`, `atr_nb`, `bbands_1d_nb`,
  `macd_nb`. Defaults chosen for Pine parity (`adjust=False`, `ddof=0`).
- `[[configs]]` — `vbt.IndicatorFactory.list_builtin_indicators()`.
- `[[expr]]` — expression-language utilities for `from_expr`.

## Parameter sweep idiom

```python
rsi = vbt.RSI.run(close, window=vbt.Param([10, 14, 20, 30]))
# rsi.rsi is a DataFrame with 4x the input columns (one per window)
entries = rsi.rsi_crossed_below(30)
exits = rsi.rsi_crossed_above(70)
pf = vbt.Portfolio.from_signals(close, entries, exits)
pf.sharpe_ratio()  # one row per window value
```

## See also

- Canonical wiki § Indicators:
  `hyperfrequency/docs/deep-tool-wiki/vectorbtpro/wiki.md#core-concepts`
- `[[../pitfalls#indicator-parity]]` for adjust / ddof / Wilder
  gotchas.
