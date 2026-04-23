---
title: Signals subsystem
parent: [[../index]]
tool: vectorbtpro
subsystem: signals
last_updated: 2026-04-20
---

# Signals subsystem

Boolean-array primitives and signal-expression DSL. Every signals
operation operates on the full 2D array; path-dependent logic
(e.g. "exit N bars after entry") compiles to Numba.

## Leaves

- `[[SignalsAccessor]]` — `.vbt.signals` accessor on any boolean
  `Series`/`DataFrame`. Methods: `.clean(exits=...)`, `.generate_exits(...)`,
  `.generate_enex(...)`, `.first(...)`, `.last(...)`, `.partition_pos_rank(...)`.
- `[[SIG]]` — random signal generator `vbt.SIG.run(shape, prob=0.1)`.
- `[[STX]]` — stop exits: `vbt.STX.run(entries, close, sl_stop=0.02)`.
  Combines SL / TP / TSL / TTP in a single indicator.
- `[[OHLCSTX]]` — OHLC-aware stop exits that can trigger intrabar using
  the high/low of each bar (more realistic than close-only).
- `[[RAND]]` — random signal sampling.
- `[[BOLB]]` — Bollinger band breakout signals.
- `[[crossed_above]]` / `[[crossed_below]]` — predicate helpers on
  `Series`/`DataFrame` via `.vbt.crossed_above(other)`.
- `[[clean]]` — dedupe consecutive entries / resolve entry-exit conflicts.
- `[[chain]]` — chain multiple signal generators into a pipeline.
- `[[factory]]` — `SignalFactory` for building custom signal generators
  via `FactoryMode.Both|Chain|Entries|Exits`.
- `[[enums]]` — `StopType` (SL|TP|TSL|TTP|DT|TD), `FactoryMode`,
  `SignalRelation` (OneOne|OneMany|ManyOne|ManyMany|Chain|AnyChain).
- `[[nb]]` — NB kernels: `generate_nb`, `first_nb`, `clean_nb`.

## Pattern

```python
# Basic entries/exits
entries = close.vbt.crossed_above(upper_band)
exits = close.vbt.crossed_below(lower_band)
entries, exits = entries.vbt.signals.clean(exits)  # resolve conflicts

# With stop exits
stops = vbt.STX.run(
    entries,
    close, high, low,
    sl_stop=0.02,   # 2% SL
    tp_stop=0.05,   # 5% TP
    tsl_stop=0.03,  # 3% trailing stop
)
pf = vbt.Portfolio.from_signals(close, entries, stops.exits)
```

## See also

- Canonical wiki § Signals Deep Dive:
  `hyperfrequency/docs/deep-tool-wiki/vectorbtpro/wiki.md#signals-deep-dive`
- `[[../pitfalls#signal-hygiene]]` — NaN / clean / double-shift.
