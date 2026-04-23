---
title: pine-script (v6) — consolidated pitfalls
parent: [[index]]
last_updated: 2026-04-20
upstream_refs:
  - /Users/DanBot/.claude/skills/strategy-translator/references/pinescript-v6.md
  - /Users/DanBot/hyperfrequency/docs/deep-tool-wiki/pine-script/wiki.md
  - https://www.tradingview.com/pine-script-docs/concepts/repainting/
  - https://www.tradingview.com/pine-script-docs/release-notes/
---

# pine-script v6 — consolidated pitfalls

The master long-form pitfall catalogue lives at
[`/Users/DanBot/.claude/skills/strategy-translator/references/pinescript-v6.md`](file:///Users/DanBot/.claude/skills/strategy-translator/references/pinescript-v6.md)
(Pitfalls 1–8, with full code + target-language fixes). This page
**mirrors** that index and **extends** it with v6-specific items discovered
during the upstream TradingView docs pass on 2026-04-20.

---

## Translator / numerics pitfalls (from strategy-translator reference)

Short index only — see the reference file for full fixes.

### {#wilder-vs-ema} Pitfall 1 — Wilder smoothing vs standard EMA

`ta.ema` is **standard EMA** (α = 2/(n+1)). `ta.rma` is **Wilder**
(α = 1/n). `ta.rsi` and `ta.atr` use Wilder internally via `ta.rma`.
Don't gloss as "EMA."

→ Full fix: reference file § Pitfall 1.

### {#biased-stdev} Pitfall 2 — Biased vs sample stdev

Pine's `ta.stdev` uses divisor **N** (biased / population). pandas'
`.std()` defaults to **N-1** (sample). Bollinger Bands come out silently
wider in pandas without `ddof=0`.

→ Full fix: reference file § Pitfall 2.

### {#lookahead} Pitfall 3 — Look-ahead in self-referential channels

Donchian / swing-high / chandelier use prior-bar extremes. Pine writes
`ta.highest(high, length)[1]`; pandas must use
`high.rolling(length).max().shift(1)`.

→ Full fix: reference file § Pitfall 3.

### {#crossover-lag} Pitfall 3b — Double-shifting pre-lagged references

When Pine writes `ta.crossover(close, upper[1])`, the `[1]` has already
applied the lag. The prior-bar half of the crossover check must compare
against the *same* pre-lagged series, not a further-shifted copy.

→ Full decision table: reference file § Pitfall 3b.

### Pitfall 4 — Crossover state requires prior bar

`ta.crossover(a, b)` fires only where `a` first exceeds `b`. In pandas:
`(a > b) & (a.shift(1) <= b.shift(1))`. In C++: update prior values
**after** the comparison.

### {#fill-timing} Pitfall 5 — Fill-timing convention

Default Pine strategy fills at **next bar's open**
(`process_orders_on_close = false`). `process_orders_on_close = true`
fills at signal-bar close. vectorbt `Portfolio.from_signals` defaults
match the open-fill convention. Always state explicitly.

### Pitfall 6 — RSI divide-by-zero

When avg loss is zero, Pine returns 100. Guard in any reimplementation.

### Pitfall 7 — MACD signal composition

Signal line is EMA of the **MACD line**, not of price. Feeding `close`
produces plausible-looking but wrong crossings.

### Pitfall 8 — Wilder seed

Wilder seeds recursion with a simple mean of the first `length` deltas
before switching to recursive smoothing. Library defaults may not match.

---

## v6-specific breaking-change pitfalls (extension)

### {#v6-a-integer-division} V6-A — `int / int` returns `int`

```pine
// v5: 3 / 2 = 1.5
// v6: 3 / 2 = 1
half = length / 2         // ← bug when length is int
half = length / 2.0       // ← correct
half = math.floor(length / 2.0)  // ← explicit floor
```

`ta.sma(close, length / 2)` with `length = 15` computed SMA(7), not
SMA(7.5). Some user-migrated v5 scripts silently miscalculate for months
before anyone notices.

### {#v6-b-short-circuit} V6-B — `and`/`or` short-circuits

```pine
// v6: request.security is skipped when barstate.isconfirmed is false
c = barstate.isconfirmed and request.security(syminfo.ticker, "60", close) > 100
```

Often desirable (fewer server calls) but changes behavior of v5 scripts
that relied on eager side-effects. Translation targets must mirror the
lazy semantics or explicitly evaluate both sides.

### {#v6-c-bool-no-na} V6-C — `bool` cannot be `na`

```pine
// v5: legal
bool flag = na
// v6: compile error — use a sentinel via a different type
int flagSet = na
x = na(flagSet) ? 0 : 1
```

### {#for-bound-reeval} V6-D — `for` loop bound re-evaluates per iteration

The `to` expression runs before every iteration in v6 (was once at
start in v5). Growing the array inside the loop creates an infinite
loop. Snapshot the bound first.

### {#qty-percent-initial} V6-E — `strategy.exit(qty_percent=X)` uses INITIAL pos

v6: `qty_percent = 50` always means 50% of the *original* entered size,
not 50% of what's left. Stop-ladder strategies must track remaining
size manually via `strategy.opentrades.size(0)`.

---

## Repaint / request.security pitfalls (upstream docs)

### {#security-lookahead} V6-F — `request.security` lookahead + `[1]` pair

```pine
// PEEKS THE FUTURE — TradingView rejects for publication
htf = request.security(syminfo.tickerid, "D", close, lookahead = barmerge.lookahead_on)

// NON-REPAINTING — prior daily close
htf = request.security(syminfo.tickerid, "D", close[1], lookahead = barmerge.lookahead_on)
```

The `[1]` and `lookahead_on` are **interdependent**: either alone is
wrong. Without `lookahead_on` and without `[1]` you get the current-bar
*confirmed* value on historical bars but the *live* value on realtime —
which re-classifies when the bar closes, causing "repaint."

### V6-P — `request.security_lower_tf` returns ARRAY

Unlike `request.security` which returns a scalar per bar,
`request.security_lower_tf` returns an `array<T>` with one entry per
lower-timeframe bar inside the chart bar. Iterating the array requires
`for v in arr` or `for [i, v] in arr`.

---

## Sizing / fills / risk pitfalls

### {#percent-of-equity-at-signal} V6-I — `percent_of_equity` uses equity AT SIGNAL

`default_qty_type = strategy.percent_of_equity, default_qty_value = 100`
sizes each entry at 100% of `strategy.equity` **at the signal bar** —
not 100% of initial capital. Profits compound, losses shrink the next
trade. Mapping to vectorbt: `size = 1.0, size_type = 'percent'` (0.26+).
Mapping to Nautilus: percentage of current account equity via
`current_equity() * 1.0`.

### {#trail-points-units} V6-G — `trail_points` / `trail_offset` are in TICKS

```pine
// Pine says "100 points" but means 100 TICKS
strategy.exit("x", trail_points = 100, trail_offset = 50)
```

On ES (`mintick = 0.25`): trail at 100 × 0.25 = 25 price points.
On EURUSD (`mintick = 0.00001`): trail at 100 × 0.00001 = 0.001.

When emitting Pine with a price-unit trail distance, divide by
`syminfo.mintick`.

### V6-H — `use_bar_magnifier` silently no-ops on free plan

Backtest results diverge between Premium author and free-plan reader.
Document the plan assumption.

### V6-Q — Default `pyramiding = 0` disallows adding to positions

Every `strategy.entry` call after the first in the same direction is
silently **ignored** until the position exits. Set `pyramiding = N` to
allow N concurrent same-direction entries. Contrast with
`strategy.order` (bypasses pyramiding entirely).

### V6-R — `strategy.close` and `strategy.close_all` obey `process_orders_on_close`

Even "market" closes wait until the next bar's open by default. Use
`process_orders_on_close = true` or `calc_on_every_tick = true` if you
need tighter close timing.

---

## Scope / var / varip pitfalls

### {#block-scope} V6-K — Block-scoped vars die at block end

```pine
// WRONG — compile error
if close > open
    signal = 1      // local to if-branch; not visible to plot()
plot(signal)

// CORRECT
signal = 0
if close > open
    signal := 1     // use := to reassign outer variable
plot(signal)
```

Every reassignment inside a block needs `:=`, not `=`.

### V6-J — `varip` survives rollback; `var` does not

Realtime-only distinction:

- `var x` — rolls back to last-confirmed-close value between ticks
- `varip x` — survives every tick

Use `varip` for tick counters, first-tick snapshots, order-flow
accumulators. Use `var` for everything else.

### V6-L — Built-in `time`, `hour`, `dayofweek` are SERIES

```pine
// Filters to Monday BARS, not "is today Monday"
mondayOnly = dayofweek == dayofweek.monday
```

Don't translate to Python as a single datetime check; iterate bar-wise.

---

## Indicator / ta.* pitfalls

### {#ta-in-conditional-scope} V6-O — `ta.*` in conditional scope gives wrong values

```pine
// WRONG — ta.rsi maintains state; skipping a bar corrupts state
if close > open
    rsi = ta.rsi(close, 14)
    plot(rsi)

// CORRECT — always compute, conditionally use
rsi = ta.rsi(close, 14)
if close > open
    plot(rsi)
```

v6 compiler warns. `pinelsp` currently does not (known limitation — see
`pinelsp/CLAUDE.md`).

### V6-S — `ta.highest(high, length)` includes the CURRENT bar

If you want the breakout *level* (high of the prior N bars, exclusive
of current), use `ta.highest(high, length)[1]`. Without the `[1]` a
current-bar high IS the breakout → signal never fires.

### V6-T — `nz(x, default)` vs `na(x)`

`nz(x)` returns `x` if not `na`, else `0` (or a passed default).
`na(x)` is a boolean test. Use `na(x)` for guards; use `nz(x)` for
"provide a default" in arithmetic contexts.

```pine
// Guard
if not na(rsi)
    plot(rsi)

// Default
safeClose = nz(close[5], close)
```

---

## Visual / alerting pitfalls

### V6-N — `alert()` vs `alertcondition()`

`alertcondition` creates a selectable option in the Create Alert dialog;
it does NOT fire alerts itself. `alert()` fires during execution.
Translations that emit "alert on cross" need to know which mechanism the
source uses.

### V6-M — `fill` color evaluated per realtime tick

`fill(p1, p2, color = expr)` recomputes `expr` on every realtime tick.
The visual flashes if the expression alternates bar-to-bar. For a more
stable visual, guard with `barstate.isconfirmed`.

---

## Additions beyond the strategy-translator reference

The items below were **added** in this pass (upstream docs on
2026-04-20) beyond Pitfalls 1–8 in the existing strategy-translator
reference file:

- **V6-A / int division** — translator reference mentions it once in
  target-specific C++ notes; now promoted to a top-level v6 breaking-
  change pitfall.
- **V6-B / short-circuit and/or** — not in the translator reference.
  Matters when translating v5 scripts whose logic relied on eager
  evaluation.
- **V6-C / bool cannot be `na`** — not in the translator reference.
- **V6-D / `for` bound re-eval** — not in the translator reference.
- **V6-E / `qty_percent` of initial pos** — not in the translator
  reference. Rewrites stop-ladder translations.
- **V6-F / lookahead + [1] pair** — mentioned in the reference in
  passing; expanded here with the "both or neither" rule.
- **V6-G / trail_points in TICKS** — not in the translator reference.
- **V6-H / use_bar_magnifier is Premium-only** — not in the translator
  reference.
- **V6-I / percent_of_equity at SIGNAL** — mentioned in the reference
  for vectorbt mapping; here re-stated as the primary Pine behavior.
- **V6-J / varip survives rollback** — not in the translator reference.
- **V6-K / block-scoped locals need `:=`** — not in the translator
  reference.
- **V6-L / built-in time/hour are SERIES** — not in the translator
  reference.
- **V6-M / `fill` color per-tick** — not in the translator reference.
- **V6-N / alert vs alertcondition** — not in the translator reference.
- **V6-O / `ta.*` in conditional scope** — not in the translator
  reference.
- **V6-P / `request.security_lower_tf` returns array** — not in the
  translator reference.
- **V6-Q / pyramiding = 0 silently ignores extra entries** — not in the
  translator reference.
- **V6-R / `strategy.close` obeys `process_orders_on_close`** — not in
  the translator reference.
- **V6-S / `ta.highest` includes current bar** — mentioned indirectly
  via Pitfall 3; here made explicit.
- **V6-T / `nz` vs `na` distinction** — not in the translator reference.

## See also

- Canonical wiki: `wiki.md` § Pitfall Catalog
- Long-form translator reference:
  `/Users/DanBot/.claude/skills/strategy-translator/references/pinescript-v6.md`
- Repainting upstream doc:
  <https://www.tradingview.com/pine-script-docs/concepts/repainting/>
- Release notes:
  <https://www.tradingview.com/pine-script-docs/release-notes/>
