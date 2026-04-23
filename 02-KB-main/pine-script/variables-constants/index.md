---
title: pine-script — Variables and Constants
parent: [[../index]]
last_updated: 2026-04-20
---

# pine-script — Variables and Constants

Built-in series variables (series float unless noted) and namespace
constants.

## Price

| Name | Type | Meaning |
|---|---|---|
| `open` | series float | Bar open |
| `high` | series float | Bar high |
| `low`  | series float | Bar low |
| `close`| series float | Bar close |
| `volume` | series float | Bar volume |
| `hl2`  | series float | `(high + low) / 2` |
| `hlc3` | series float | `(high + low + close) / 3` |
| `ohlc4`| series float | `(open + high + low + close) / 4` |
| `bid`, `ask` | series float | ★v6, **"1T" timeframe only** (tick) |

## Time

| Name | Type | Meaning |
|---|---|---|
| `time` | series int | Bar open time (Unix ms) |
| `time_close` | series int | Bar close time (Unix ms) |
| `timenow` | series int | Current wall-clock (Unix ms) |
| `year`, `month`, `weekofyear`, `dayofmonth`, `dayofweek`, `hour`, `minute`, `second` | series int | Current bar time components |

`dayofweek.sunday … .saturday` constants exist for comparisons.

## Bar state (`barstate.*`)

`isfirst`, `islast`, `ishistory`, `isrealtime`, `isnew`, `isconfirmed`,
`islastconfirmedhistory`. All `series bool`. Use `barstate.isconfirmed`
to guard logic that should only run once per confirmed close.

## Session (`session.*`)

`ismarket`, `isfirstbar`, `islastbar`, `isfirstbar_regular`,
`islastbar_regular`, `ispremarket`, `ispostmarket`. `series bool`.

## Syminfo (`syminfo.*`)

`tickerid` (prefix:ticker), `prefix`, `ticker`, `mintick`, `currency`,
`type`, `basecurrency`, `pointvalue`, `volumetype`, `root`, `session`,
`timezone`, `description`, `country`, `industry`, `sector`, `employees`,
`shares_outstanding`, `target_price_*`, `recommendations_*`.

★v6 additions: `syminfo.isin`, `syminfo.mincontract`,
`syminfo.main_tickerid`, `syminfo.current_contract` (continuous futures
underlying).

## Timeframe (`timeframe.*`)

`period`, `multiplier`, `isdaily`, `isweekly`, `ismonthly`,
`isintraday`, `isminutes`, `isseconds`, `isticks`. ★v6:
`timeframe.main_period` (main chart timeframe when called from
`request.security`).

## Strategy state (in strategy scripts only)

`strategy.position_size`, `.position_avg_price`,
`.equity`, `.initial_capital`, `.openprofit`,
`.netprofit`, `.grossprofit`, `.grossloss`,
`.wintrades`, `.losstrades`, `.eventrades`,
`.max_drawdown`, `.max_runup`, `.closedtrades`, `.opentrades` (counts),
`.position_entry_name`.

## Built-in constants (subset)

- `na` — the missing-value sentinel
- `true` / `false`
- `math.pi`, `math.e`, `math.phi`, `math.rphi`
- Colors: `color.red`, `.blue`, `.green`, `.orange`, `.yellow`,
  `.purple`, `.fuchsia`, `.aqua`, `.lime`, `.teal`, `.navy`, `.maroon`,
  `.olive`, `.silver`, `.gray`, `.white`, `.black`
- Line styles: `line.style_solid`, `.style_dashed`, `.style_dotted`,
  `.style_arrow_*`
- Label styles: `label.style_label_left`, `.style_label_right`, ...
- Shape styles: `shape.arrowup`, `.arrowdown`, `.circle`, `.cross`,
  `.diamond`, `.flag`, `.labeldown`, `.labelup`, `.square`, `.triangledown`,
  `.triangleup`, `.xcross`
- Plot locations: `location.abovebar`, `.belowbar`, `.top`, `.bottom`,
  `.absolute`
- Barmerge: `barmerge.gaps_off`, `.gaps_on`, `.lookahead_off`,
  `.lookahead_on`
- Commission: `strategy.commission.percent`, `.cash_per_order`,
  `.cash_per_contract`
- Qty type: `strategy.fixed`, `.cash`, `.percent_of_equity`
- Direction: `strategy.long`, `.short`, `strategy.direction.all`,
  `.long`, `.short`
- OCA: `strategy.oca.none`, `.reduce`, `.cancel`

## See also

- [[../builtins/index]]
- Canonical: `wiki.md` § API Surface / Mental Model
