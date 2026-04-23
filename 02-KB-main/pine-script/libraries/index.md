---
title: pine-script — Libraries
parent: [[../index]]
last_updated: 2026-04-20
---

# pine-script — Libraries

Libraries are reusable Pine modules. Once published on TradingView with
a version, they can be `import`ed by other scripts.

## Declaration

```pine
//@version=6
//@description Math extras for trading scripts
library("MathExtras", overlay = false)
```

## Exporting

Only `export`-marked functions, methods, and UDTs are visible to
importers:

```pine
// @function   EWMA with explicit alpha
// @param src  Input series
// @param alpha  Smoothing factor (0..1)
// @returns    series float
export myEwma(series float src, simple float alpha) =>
    var float state = na
    state := na(state) ? src : alpha * src + (1 - alpha) * state
    state

// @type  Bracket-order container
// @field entry Entry price
// @field stop  Stop-loss price
// @field take  Take-profit price
export type Bracket
    float entry = na
    float stop  = na
    float take  = na
```

Annotations (`@function`, `@param`, `@returns`, `@type`, `@field`) feed
the editor's hover/intellisense and the published library's doc page.

## Importing

```pine
//@version=6
indicator("User", overlay = true)
import TradingView/ta/8 as tvta     // stdlib-ish published example
import MyUsername/MathExtras/1 as mx

x = mx.myEwma(close, 0.2)
b = mx.Bracket.new(entry = close, stop = close * 0.99, take = close * 1.02)
plot(x)
```

Import form: `import <author>/<library-name>/<version> as <alias>`. The
version must be a published integer; versions are immutable, so bumping
is the only way to ship fixes.

## Versioning rules

1. Every published library has a monotonically increasing integer
   version.
2. A published version is **immutable** — edits require a new version.
3. Importers pin a specific version (`MathExtras/1`, `MathExtras/2`).
   Bumping requires changing the import line.
4. Private libraries can be referenced by your own scripts only; public
   libraries are searchable by everyone.

## See also

- [[../annotations/index]] — `@function` / `@param` / `@returns`
- Canonical: `wiki.md` § Script Types / `library()`
- Community examples: Pine Coders at
  <https://www.tradingview.com/u/PineCoders/#published-scripts>
