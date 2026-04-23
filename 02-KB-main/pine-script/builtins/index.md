---
title: pine-script — Built-ins (per namespace)
parent: [[../index]]
last_updated: 2026-04-20
source: /Users/DanBot/hyperfrequency/pinelsp/pine-data/v6/functions.ts (457 v6 funcs)
---

# pine-script — Built-ins

457 functions across 22 namespaces. Counts are from the scraped v6
snapshot; the official language reference
(<https://www.tradingview.com/pine-script-reference/v6/>) is the source
of truth for exact signatures.

## Namespaces (sorted by function count)

| Namespace | Count | Focus |
|---|---|---|
| [[ta]]        | 59 | Technical-analysis indicators |
| [[array]]     | 54 | 1D dynamic arrays |
| [[matrix]]    | 48 | 2D linear algebra |
| [[strategy]]  | 47 | Orders, positions, risk, trade inspection |
| box           | 29 | Drawing boxes |
| [[math]]      | 23 | Numeric primitives |
| table         | 22 | On-chart tables |
| label         | 21 | Drawing labels |
| [[string]]    | 18 | `str.*`: format, tostring, split |
| [[input]]     | 13 | User-configurable settings |
| line          | 11 | Drawing lines |
| [[map]]       | 10 | Key-value dicts |
| [[request]]   | 10 | HTF, financial, economic, footprint(v6), seed |
| ticker        | 9  | Custom tickerid construction |
| color         | 7  | RGB + gradient |
| chart         | 5  | Chart points |
| linefill      | 3  | Fills between lines |
| log           | 3  | Pine Logs panel |
| [[time]]      | —  | `time()`/`timestamp()` + vars; see timeframe too |
| timeframe     | 3  | `change`, `from_seconds`, `in_seconds` |
| polyline      | 2  | Multi-segment lines |
| syminfo       | 2  | `prefix`, `ticker` (many more as variables) |
| runtime       | 1  | `runtime.error` |
| top-level     | ~90 unique names | `plot`, `indicator`, `strategy`, `library`, `alert`, `nz`, `na`, ... |

## Per-namespace quick reference

### `ta.*` — Technical Analysis

Most common: `rsi`, `ema`, `sma`, `rma`, `wma`, `atr`, `bb`, `macd`,
`stoch`, `crossover`, `crossunder`, `highest`, `lowest`, `stdev`,
`variance`, `correlation`, `change`, `cum`, `cog`, `vwap`, `vwma`,
`supertrend`, `sar`, `cci`, `mfi`, `tsi`, `linreg`, `pivot_point_low`,
`pivot_point_high`, `kc`, `falling`, `rising`, `percentrank`, `dmi`,
`mom`, `median`, `mode`, `cross`, `swma`.

**Critical**: `ta.rsi` / `ta.atr` use Wilder (`ta.rma`, α=1/n), NOT
EMA. See [[../pitfalls#wilder-vs-ema]].

**`ta.stdev`** is **biased** (divisor N). See
[[../pitfalls#biased-stdev]].

### `strategy.*`

See [[../strategy/index]] — full coverage of declaration params, entry,
exit, close, order, risk, state, opentrades, closedtrades.

### `request.*`

- `security(symbol, timeframe, expr, gaps, lookahead, ignore_invalid_symbol)`
- `security_lower_tf(symbol, timeframe, expr, ignore_invalid_symbol)` → array
- `financial(symbol, field, period, gaps, ignore_invalid_symbol)`
- `dividends` / `splits` / `earnings` / `economic` / `currency_rate`
- `footprint(symbol, timeframe, ...)` ★v6 new
- `seed(source, symbol, expression, ignore_invalid_symbol)` — read from a
  user-maintained GitHub repo of custom CSV data

★v6: all `request.*` accept `series string` for symbol/timeframe and can
be called inside conditional/loop scopes.

### `array.*` — selected

`new_int`/`new_float`/`new_bool`/`new_string`/`new_color`/`from`,
`push`/`pop`/`shift`/`unshift`, `get`/`set`/`insert`/`remove`,
`first`/`last`/`size`/`slice`/`sort`/`reverse`/`concat`/`copy`/`fill`/`join`,
`includes`/`indexof`/`lastindexof`, `sum`/`avg`/`median`/`mode`/`stdev`/
`variance`/`min`/`max`/`range`/`abs`/`standardize`,
`percentile_linear_interpolation`, `percentile_nearest_rank`,
`percentrank`, `covariance`, `binary_search`, `clear`.

### `matrix.*` — selected

`new`, `get`/`set`, `row`/`col`, `add_row`/`add_col`, `remove_row`/`remove_col`,
`swap_rows`/`swap_columns`, `fill`/`reshape`, `mult`/`inv`/`transpose`/`pinv`,
`det`/`trace`/`rank`/`pow`, `eigenvalues`/`eigenvectors`, `kron`,
`is_identity`/`is_square`/`is_symmetric`/`is_triangular`.

### `map.*`

`new`, `put`/`get`/`remove`/`clear`, `keys`/`values`/`size`/`contains`/`copy`.

### `math.*`

`abs`, `acos`/`asin`/`atan`, `avg`, `ceil`/`floor`, `cos`/`sin`/`tan`,
`exp`/`log`/`log10`, `max`/`min`, `pow`/`sqrt`, `random`,
`round`/`round_to_mintick`, `sign`, `sum`, `todegrees`/`toradians`.

### `str.*`

`contains`, `endswith`/`startswith`, `split`, `format`/`format_time`,
`length`, `lower`/`upper`, `match`, `pos`, `replace`/`replace_all`,
`substring`, `tonumber`/`tostring`, `trim`, `repeat`.

### `input.*`

`input` (polymorphic — type inferred from `defval`), `.bool`, `.int`,
`.float`, `.string`, `.color`, `.source`, `.symbol`, `.timeframe`,
`.session`, `.time`, `.price`, `.text_area`. ★v6: `active` param for
runtime-conditional editability.

### `time.*` and `timeframe.*`

Bare functions: `time(timeframe, session, timezone)`,
`timestamp(year, month, day, hour, minute, second)`.
Variables: `time`, `time_close`, `timenow`, `year`, `month`, `weekofyear`,
`dayofmonth`, `dayofweek`, `hour`, `minute`, `second`.
`timeframe.*`: `period`, `multiplier`, `main_period` (v6), `isdaily`,
`isweekly`, `isminutes`, `isseconds`, `change(timeframe)`,
`in_seconds(timeframe)`, `from_seconds(secs)`.

### `syminfo.*`

Functions: `prefix`, `ticker`.
Variables: `syminfo.tickerid`, `syminfo.mintick`, `syminfo.currency`,
`syminfo.type`, `syminfo.basecurrency`, `syminfo.pointvalue`,
`syminfo.volumetype`, `syminfo.root`, `syminfo.session`,
`syminfo.isin` ★v6, `syminfo.mincontract` ★v6,
`syminfo.main_tickerid` ★v6, `syminfo.current_contract` ★v6.

### `color.*`

Functions: `rgb(r,g,b,transp)`, `new(color, transp)`, `from_gradient`,
`r(color)`, `g(color)`, `b(color)`, `t(color)`.
Constants: `color.red`, `.blue`, `.green`, `.orange`, `.yellow`,
`.purple`, `.fuchsia`, `.aqua`, `.lime`, `.teal`, `.navy`, `.maroon`,
`.olive`, `.silver`, `.gray`, `.white`, `.black`.

### Drawing object namespaces

Each has `new`, `delete`, `copy`, and setters/getters. See
<https://www.tradingview.com/pine-script-reference/v6/#fun_line>
etc. Watch script-level counters: `max_lines_count = 500` (default),
`max_labels_count = 50`, `max_boxes_count = 50`, set on `indicator()` /
`strategy()`. Exceeding the count causes oldest drawings to be deleted.

## See also

- [[../strategy/index]] — deeper on strategy.*
- [[../pitfalls]]
- Canonical: `wiki.md` § API Surface by Namespace
- Scraped snapshot: `/Users/DanBot/hyperfrequency/pinelsp/pine-data/v6/functions.ts`
