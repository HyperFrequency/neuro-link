---
title: vectorbtpro — consolidated pitfalls
parent: [[index]]
tool: vectorbtpro
last_updated: 2026-04-20
---

# vectorbtpro — consolidated pitfalls

One-page catalogue of the recurring footguns, pulled from the canonical
wiki and the llms-full.txt corpus. Each item names the root cause, the
symptom, and the resolution.

## Fill-timing and bar reference

- **Signal-bar close vs next-bar open.** `Portfolio.from_signals` fills
  at the signal-bar CLOSE by default. Pine `process_orders_on_close=false`
  fills at next-bar OPEN. To match Pine exactly:
  `Portfolio.from_signals(..., price=open.shift(-1))`.
- **Look-ahead bias via `.rolling(min_periods=0)`.** Rolling windows
  with `min_periods=0` compute partial values on the first N-1 rows
  using future data for some aggregations. Always use `min_periods=N`
  (same as window) or explicitly `.dropna()` the first N-1 rows before
  building signals.
- **`.shift(-1)`** silently shifts future values backwards — only use
  when deliberately modeling next-bar fills; never in predicate
  construction.

## Enum and size-type semantics

- **`SizeType.Percent` vs `SizeType.TargetPercent`.** `Percent` adds
  that percentage of cash as the incremental position; `TargetPercent`
  rebalances to that percentage of total value. Mixing them up produces
  silent double-sizing.
- **`SizeType.Percent` / `Percent100` do not support position reversal**
  unless the position is closed first. Use `SizeType.Amount` or
  `SizeType.Value` when your strategy reverses between long and short.
- **Fractional vs percentage scale.** `TargetPercent` uses [0, 1];
  `TargetPercent100` uses [0, 100]. Pine `strategy.percent_of_equity`
  is in [0, 100], so the correct vbt equivalent when porting verbatim
  is `TargetPercent100`.
- **`size=np.inf` + `SizeType.Percent`** is a common typo. If you want
  "all available equity", use `size=1.0` with `TargetPercent`, or
  `size=np.inf` with `SizeType.Amount` (and the simulator will clamp
  to max affordable).
- **`direction` string aliases** are case-insensitive but must match
  `longonly` / `shortonly` / `both` exactly; typos like `"long"` silently
  fall through to the default.

## Signal hygiene

- **NaN propagation.** Indicators return NaN for the first `window-1`
  bars. Signals derived from them propagate NaN. Always
  `entries = entries.fillna(False)` before passing to `from_signals`.
- **Unclean conflicting signals.** If `entries & exits` both fire on
  the same bar, behavior depends on `conflict_mode`. Use
  `entries.vbt.signals.clean(exits)` to explicitly resolve conflicts
  before simulation, or set `conflict_mode='entry'|'exit'|'ignore'`.
- **Double-shifted crossovers.** If you precompute
  `upper_prev = upper.shift(1)`, do not shift again inside
  `close.vbt.crossed_above(upper_prev)` — the crossover predicate
  already references the previous bar internally.

## Indicator parity (Pine → vectorbtpro)

- **EMA `adjust` default.** `pandas.Series.ewm(span=N)` defaults to
  `adjust=True`; Pine `ta.ema` is `adjust=False`. Override explicitly
  via `vbt.MACD.run(..., adjust=False)` or use the Numba-level
  `ema_nb` which defaults to `adjust=False`.
- **`ta.stdev` is population (ddof=0);** pandas `.std()` defaults to
  sample (ddof=1). For Bollinger-band parity use
  `vbt.BBANDS.run(..., ddof=0)` or the NB-level `bbands_1d_nb`.
- **Wilder vs EMA smoothing.** `vbt.RSI` and `vbt.ATR` use Wilder
  smoothing (ewm alpha=1/N), matching Pine. `vbt.MACD` uses EMA
  (ewm span=N) — the two smoothing families are NOT interchangeable.

## MultiIndex and parameter sweeps

- **Level-name collisions after broadcast.** If two indicators both
  sweep `window`, the resulting MultiIndex has two `window` levels and
  `.xs('window', level=...)` is ambiguous. Rename via
  `vbt.Param([10,20], name='fast_window')` to disambiguate.
- **Memory blow-up.** A 10k-combination sweep on 10 years of hourly
  data with 5 assets → ~35 GB RAM just for float64 prices. Enable
  `chunked=True` for memory-bounded simulation.
- **`.stats()` on an unclosed sweep** returns per-column metrics; use
  `.groupby(level='param').mean()` to aggregate across a parameter
  dimension.

## Numba debugging

- **Cryptic tracebacks inside `@njit`.** Set `NUMBA_DISABLE_JIT=1` to
  disable JIT and get normal Python tracebacks. Be aware that custom
  `order_func_nb` context objects behave differently without JIT.
- **JIT compile time on first call.** `vbt.RSI.run(...)` takes 2-5s the
  first time due to Numba compilation; subsequent calls are fast.
  Persistent caches live under `__pycache__/*.nbi` and are invalidated
  by Numba version changes.
- **Numba version pinning.** `vectorbtpro` has strict Numba constraints
  (see `vectorbtpro._opt_deps`); upgrading outside the supported range
  silently breaks compiled kernels. Always pin in `requirements.txt`.

## Import-name differences

- **Public vs PRO import name.** Public library: `import vectorbt as vbt`;
  PRO: `import vectorbtpro as vbt`. Some class names, method signatures,
  and parameter defaults differ. Code written for public vectorbt may
  need adjustments on PRO.

See also:

- [[Portfolio]] — fill timing, conflict modes, size types
- [[Indicators]] — adjust, ddof, Wilder specifics
- [[Signals]] — clean / dedupe / chain primitives
- [[Optimization]] — Splitter / CVSplitter gotchas
