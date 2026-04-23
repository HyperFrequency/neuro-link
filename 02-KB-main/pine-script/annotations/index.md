---
title: pine-script — Annotations
parent: [[../index]]
last_updated: 2026-04-20
---

# pine-script — Annotations

Pine uses `//@` comment-prefix annotations for compiler directives and
documentation. The compiler parses them from leading comments only.

## Compiler directives

| Annotation | Where | Effect |
|---|---|---|
| `//@version=6` | First line of script (usually) | Target language version. **Required.** |

## Documentation annotations (for libraries + editor hover)

| Annotation | Scope | Purpose |
|---|---|---|
| `//@description` | Library header | Free-text library summary |
| `//@function` | Before `export function()` | Describes the function |
| `//@param <name>` | Before `export function()` | Describes a parameter |
| `//@returns` | Before `export function()` | Describes the return value |
| `//@variable <name>` | Before a var declaration | Describes a top-level variable |
| `//@type` | Before `export type` | Describes a UDT |
| `//@field <name>` | Inside UDT block | Describes a UDT field |
| `//@method` | Before `export method` | Describes a method |
| `//@strategy_alert_message` | Before `strategy.entry/exit` | Default alert message placeholder |
| `//@enum` | (planned) | Enum documentation |

## Example

```pine
//@version=6
//@description Helpers for bracket orders
library("BracketHelpers", overlay = true)

//@type Bracket container
//@field entry Entry price
//@field stop  Stop-loss price
//@field take  Take-profit price
export type Bracket
    float entry = na
    float stop  = na
    float take  = na

//@function Place a bracketed long
//@param symbol Symbol to trade
//@param b       Bracket parameters
//@returns       void
export placeLong(simple string symbol, Bracket b) =>
    strategy.entry("L", strategy.long, limit = b.entry)
    strategy.exit("X", from_entry = "L", stop = b.stop, limit = b.take)
```

## See also

- [[../libraries/index]] — how annotations feed library docs
- Canonical: `wiki.md` § Script Types
