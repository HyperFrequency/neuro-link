---
title: pine-script — Operators and Keywords
parent: [[../index]]
last_updated: 2026-04-20
---

# pine-script — Operators and Keywords

## Operators

| Category | Operators | Notes |
|---|---|---|
| Arithmetic | `+ - * / %` | ★v6: `int / int` returns `int`. For float division, coerce one operand: `a / 1.0`. `+` concatenates strings too. |
| Comparison | `< <= == != > >=` | Return `bool`. ★v6: strict equality on `bool` is compile error if either side could be `na` (bool cannot be `na`). |
| Logical | `and or not` | ★v6: **short-circuit** on `and`/`or`. Second operand may not execute. |
| Ternary | `cond ? a : b` | No block scope; expressions only. Both branches must have compatible types. |
| History | `series[n]` | `close[1]` = previous bar's close. Access depth bounded by `max_bars_back`. Only valid on `series`-qualified values. |
| Assignment | `=` | Declaration with inferred/annotated type. |
| Reassignment | `:=` | Reassign an existing variable. Using `=` on an existing name shadows → compile warning. |
| Compound | `+= -= *= /= %=` | Reassignment with arithmetic. |

### Precedence (high → low)

1. `[]` (history-reference)
2. Unary `+ - not`
3. `* / %`
4. `+ -`
5. `< <= > >=`
6. `== !=`
7. `and`
8. `or`
9. `?:` (ternary)

## Keywords (reserved)

Parser-level (not configurable via `pine-data/v6/keywords.ts`):

`if`, `else`, `for`, `for...in`, `while`, `var`, `varip`, `return`,
`import`, `export`, `method`, `type`, `enum` (recent), `true`, `false`,
`na`, `and`, `or`, `not`.

## Script-type functions (special top-level only)

`indicator()`, `strategy()`, `library()` — called at most once per script,
must come before any non-comment code (other than `//@version`).

## Type keywords

`int`, `float`, `bool`, `string`, `color`, `array`, `matrix`, `map`,
`line`, `label`, `box`, `table`, `polyline`, `linefill`, `chart.point`.

## Qualifiers

`const`, `simple`, `series`. `input` is implicit (set by an
`input.*()` function call, not as a keyword prefix).

## See also

- [[../pitfalls]]
- [[../control-flow/index]]
- Canonical: `wiki.md` § Types / Operators
