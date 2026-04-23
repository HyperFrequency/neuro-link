---
title: pine-script (v6) — llm-wiki navigation
tool: pine-script
version: v6
upstream: TradingView (closed-source DSL)
local_tooling: folknor/pine-tools fork at /Users/DanBot/hyperfrequency/pinelsp/
canonical_wiki: ../../../docs/deep-tool-wiki/pine-script/wiki.md
last_updated: 2026-04-20
---

# pine-script v6 — llm-wiki

Tree-navigable offline context for Pine Script v6. Mirrors the sitemap at
the top of `docs/deep-tool-wiki/pine-script/wiki.md`. Each leaf links to
a per-page deep-content stub.

## Sitemap

```
pine-script/
├── Types/
│   ├── [[types/primitives]]            — int, float, bool, string, color
│   ├── [[types/forms]]                 — const < input < simple < series
│   ├── [[types/collections]]           — array<T>, matrix<T>, map<K,V>
│   ├── [[types/user-defined]]          — `type` keyword + methods
│   └── [[types/drawing-objects]]       — line, label, box, table, polyline, linefill
├── Operators-keywords/
│   ├── [[operators-keywords/arithmetic]]   — + - * / %  (★v6: int/int returns int)
│   ├── [[operators-keywords/comparison]]   — < <= == != > >=
│   ├── [[operators-keywords/logical]]      — and or not  (★v6: short-circuits)
│   ├── [[operators-keywords/ternary]]      — cond ? a : b
│   ├── [[operators-keywords/history]]      — [n] series lag
│   └── [[operators-keywords/assignment]]   — = := += -= *= /= %=
├── Control-flow/
│   ├── [[control-flow/if-else]]        — block-scoped locals
│   ├── [[control-flow/switch]]         — match-style on series values
│   ├── [[control-flow/for]]            — ★v6: `to` bound re-eval per iter
│   ├── [[control-flow/for-in]]         — iterate arrays
│   └── [[control-flow/while]]          — use sparingly; scope-count budget
├── Variables-constants/
│   ├── [[variables-constants/price]]       — open, high, low, close, volume, hl2, hlc3, ohlc4
│   ├── [[variables-constants/time]]        — time, time_close, hour, minute, year, month, dayofweek
│   ├── [[variables-constants/barstate]]    — isfirst, islast, ishistory, isrealtime, isnew, isconfirmed
│   ├── [[variables-constants/syminfo]]     — ticker, mintick, currency, type, isin(v6)
│   ├── [[variables-constants/session]]     — ismarket, ispremarket, ispostmarket
│   └── [[variables-constants/strategy]]    — position_size, equity, openprofit, netprofit
├── Annotations/
│   ├── [[annotations/version]]         — //@version=6
│   ├── [[annotations/description]]     — //@description for libraries
│   ├── [[annotations/param]]           — //@param doc
│   ├── [[annotations/returns]]         — //@returns doc
│   ├── [[annotations/variable]]        — //@variable doc
│   └── [[annotations/field]]           — //@field doc (UDT fields)
├── Builtins/
│   ├── [[builtins/ta]]                 — 59 funcs: rsi, ema, rma, atr, bb, macd, stoch, crossover
│   ├── [[builtins/math]]               — 23 funcs: abs, log, sqrt, round, sum, sin/cos, random
│   ├── [[builtins/array]]              — 54 funcs (★v6 negative indices)
│   ├── [[builtins/matrix]]             — 48 funcs: mult, inv, transpose, det, eigenvalues
│   ├── [[builtins/map]]                — 10 funcs: put/get/remove/keys/values
│   ├── [[builtins/string]]             — 18 funcs under str.*: format, tostring, split, match
│   ├── [[builtins/strategy]]           — 47 funcs under strategy.*
│   ├── [[builtins/request]]            — 10 funcs (★v6: dynamic strings + footprint)
│   ├── [[builtins/syminfo]]            — 2 funcs + ~20 vars (★v6: isin, mincontract)
│   ├── [[builtins/time]]               — time(), timestamp() + 15 vars
│   ├── [[builtins/timeframe]]          — change, from_seconds, in_seconds + period/multiplier
│   ├── [[builtins/input]]              — 13 input.* forms
│   ├── [[builtins/color]]              — rgb, new, from_gradient + constants
│   ├── [[builtins/ticker]]             — new, modify, heikinashi, renko, kagi, pnf, linebreak
│   ├── [[builtins/chart]]              — point.new, is_standard
│   ├── [[builtins/log]]                — info/warning/error (Pine Logs panel)
│   ├── [[builtins/runtime]]            — runtime.error
│   └── [[builtins/drawing]]            — line/label/box/table/polyline/linefill
├── Strategy/
│   ├── [[strategy/declaration]]        — strategy() params: qty type, process_orders_on_close, ...
│   ├── [[strategy/entry]]              — strategy.entry (respects pyramiding, reverses)
│   ├── [[strategy/exit]]               — strategy.exit (★trail_points in TICKS; ★v6 qty_percent of INITIAL)
│   ├── [[strategy/close]]              — strategy.close, close_all
│   ├── [[strategy/order]]              — strategy.order (raw, bypasses pyramiding)
│   ├── [[strategy/risk]]               — strategy.risk.* (drawdown, cons-loss, intraday caps)
│   ├── [[strategy/state]]              — position_size, equity, openprofit, netprofit
│   └── [[strategy/trades]]             — opentrades.*, closedtrades.* inspection
├── Indicators/
│   ├── [[indicators/declaration]]      — indicator() params: overlay, scale, max_bars_back
│   ├── [[indicators/plotting]]         — plot, plotshape, plotchar, plotbar, plotcandle, hline, fill
│   └── [[indicators/alerts]]           — alertcondition vs alert, freq param
├── Libraries/
│   ├── [[libraries/declaration]]       — library() + export keyword
│   ├── [[libraries/import]]            — import User/Lib/Version as alias
│   └── [[libraries/versioning]]        — immutable versions; bump on change
└── pitfalls.md                          — [[pitfalls]] consolidated
```

→ **Canonical deep wiki:** `/Users/DanBot/hyperfrequency/docs/deep-tool-wiki/pine-script/wiki.md`
→ **RefGraph assets:** `../docs/deep-tool-wiki/pine-script/assets/refgraph-{builtins,strategy}.mmd`
→ **Translation pitfalls (long-form):** `/Users/DanBot/.claude/skills/strategy-translator/references/pinescript-v6.md`
→ **Local LSP + validator:** `/Users/DanBot/hyperfrequency/pinelsp/` (branch `feat/tree-sitter-pine`)
→ **Awesome-pinescript index:** `/Library/Obsidian-Vault/Auto-Quant/_Awesome-Resources/awesome-pinescript.md`

## Leaf page template

Each leaf file under this tree follows:

```
# <subsystem>/<leaf>

## Signature
<primary signature(s), series/simple qualifiers>

## Minimal example
<smallest runnable Pine snippet>

## v6 changes
<what changed from v5, if anything>

## Pitfalls
- [[../pitfalls#<slug>]]

## See also
- [[<sibling>]]
- Canonical: wiki.md § <anchor>
```
