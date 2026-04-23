---
title: pine-script — Strategy
parent: [[../index]]
last_updated: 2026-04-20
---

# pine-script — Strategy

The `strategy.*` namespace turns an indicator-like script into a
simulated trading engine. 47 functions + several dozen state variables.

## Declaration

```pine
//@version=6
strategy(
  title = "Example",
  shorttitle = "EX",
  overlay = true,
  default_qty_type = strategy.percent_of_equity,
  default_qty_value = 100,
  pyramiding = 0,
  commission_type = strategy.commission.percent,
  commission_value = 0.075,
  slippage = 2,
  process_orders_on_close = false,
  calc_on_every_tick = false,
  calc_on_order_fills = false,
  use_bar_magnifier = true,   // Premium only
  initial_capital = 1_000_000,
  margin_long = 100,
  margin_short = 100,
  fill_orders_on_standard_ohlc = false
)
```

**Fill timing.** Default: orders placed this bar fill at *next* bar's
open. With `process_orders_on_close = true`: fill at *this* bar's close.
See [[../pitfalls#fill-timing]].

**Sizing modes (`default_qty_type`).**

- `strategy.fixed` — `default_qty_value` is contracts/shares.
- `strategy.cash` — `default_qty_value` is dollars (or base currency).
- `strategy.percent_of_equity` — `default_qty_value` is a percentage of
  `strategy.equity` at the **signal bar**. See
  [[../pitfalls#percent-of-equity-at-signal]].

## Entry / Exit / Close / Order

### `strategy.entry(id, direction, qty, limit, stop, oca_name, oca_type, comment, alert_message)`

- Respects pyramiding.
- **Reverses** if called in opposite direction while a position exists
  (unless `strategy.risk.allow_entry_in` restricts).
- `direction ∈ {strategy.long, strategy.short}`.

### `strategy.exit(id, from_entry, qty, qty_percent, profit, limit, loss, stop, trail_price, trail_points, trail_offset, oca_name, ...)`

- Persists across bars if `from_entry` is omitted (waits for a position).
- **`trail_points` / `trail_offset` are in TICKS.** Multiply by
  `syminfo.mintick` to reason about price distances. See
  [[../pitfalls#trail-points-units]].
- ★v6: `qty_percent` is percentage of **initial** position size, not
  remaining. See [[../pitfalls#qty-percent-initial]].
- ★v6: both absolute and relative parameters evaluate; whichever
  triggers first wins (v5 prioritized absolute).

### `strategy.close(id, comment, qty, qty_percent, alert_message)` / `strategy.close_all(...)`

Market order on next available tick (respects `process_orders_on_close`).

### `strategy.order(id, direction, qty, limit, stop, oca_name, oca_type, comment, alert_message)`

Low-level: bypasses pyramiding, no auto-reversal. Use for net-position
engines where you control everything.

### `strategy.cancel(id)` / `strategy.cancel_all()`

Cancel pending orders by id.

## Risk controls (`strategy.risk.*`)

| Function | Effect |
|---|---|
| `allow_entry_in(strategy.direction.all | long | short | short)` | Restrict entry direction; overrides auto-reversal |
| `max_cons_loss_days(count, alert_message)` | Pause trading after N consecutive losing days |
| `max_drawdown(value, type)` | Close all + halt when DD exceeds threshold |
| `max_intraday_filled_orders(count)` | Cap fills per day |
| `max_intraday_loss(value, type)` | Halt intraday after loss threshold |
| `max_position_size(contracts)` | Cap position size |

These are declared at top level; TradingView's engine enforces them.

## State queries

Variables (series-qualified, updated each bar):

- `strategy.position_size` — signed contracts held (negative = short)
- `strategy.position_avg_price` — weighted-avg entry price
- `strategy.equity` — current equity including open PnL
- `strategy.openprofit` — unrealized PnL
- `strategy.netprofit` / `strategy.grossprofit` / `strategy.grossloss`
- `strategy.wintrades` / `.losstrades` / `.eventrades` — counts
- `strategy.max_drawdown` / `.max_runup`
- `strategy.initial_capital` — starting balance
- `strategy.closedtrades` / `.opentrades` — COUNTS (not arrays)

## Trade inspection

```pine
// Profit of the most recently closed trade:
lastProfit = strategy.closedtrades.profit(strategy.closedtrades - 1)

// Entry price of the first open trade:
openEntry = strategy.opentrades.entry_price(0)
```

Available fields: `size`, `entry_price`, `entry_bar_index`,
`entry_time`, `entry_comment`, `entry_id`, `exit_*` (closed only),
`profit`, `profit_percent`, `max_drawdown`, `max_drawdown_percent`,
`max_runup`, `max_runup_percent`, `commission`.

## Currency conversion (★v6-new)

- `strategy.convert_to_account(value)` — convert to account currency
- `strategy.convert_to_symbol(value)` — convert from account to symbol

## Minimal working strategy

```pine
//@version=6
strategy("EMA crossover", overlay = true,
  default_qty_type = strategy.percent_of_equity,
  default_qty_value = 100,
  commission_type = strategy.commission.percent,
  commission_value = 0.075,
  slippage = 2)

fastLen = input.int(9,  "Fast")
slowLen = input.int(21, "Slow")

fast = ta.ema(close, fastLen)
slow = ta.ema(close, slowLen)

if ta.crossover(fast, slow)
    strategy.entry("L", strategy.long)
if ta.crossunder(fast, slow)
    strategy.entry("S", strategy.short)

plot(fast, color = color.blue)
plot(slow, color = color.orange)
```

## See also

- [[../pitfalls]] — trail_points/qty_percent/percent_of_equity/fill-timing
- [[../indicators/index]]
- Canonical: `wiki.md` § Script Types / Pitfall Catalog
- Translation reference: `/Users/DanBot/.claude/skills/strategy-translator/references/pinescript-v6.md`
