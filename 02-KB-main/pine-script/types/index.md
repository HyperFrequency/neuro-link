---
title: pine-script — Types
parent: [[../index]]
last_updated: 2026-04-20
---

# pine-script — Types

## Primitives (5)

| Type | Literal | Notes |
|---|---|---|
| `int` | `42`, `-7` | Whole numbers. ★v6: `int / int` returns `int` (silent truncation bug). |
| `float` | `1.5`, `math.pi` | IEEE-754 doubles under the hood. |
| `bool` | `true`, `false` | ★v6: strictly `true`/`false`; `na` bool is a type error. |
| `string` | `"text"` | ★v6: max length 40,960 (was 4,096 in v5). |
| `color` | `#RRGGBB`, `#RRGGBBAA`, `color.red` | 24-bit RGB + optional transparency. |

## Forms (qualifiers)

Order: `const < input < simple < series`.

- **`const`** — known at compile time. `const int N = 14`.
- **`input`** — set by user in settings; static thereafter.
- **`simple`** — computed on the first bar; static.
- **`series`** — can change every bar. Default for most expressions.

Assignment compatibility: weaker qualifiers accepted where stronger
expected, never vice versa. `plot(color = ...)` requires `series color`
and accepts anything; `request.security(timeframe = ...)` historically
required `simple string` but ★v6 now accepts `series string` (dynamic
requests).

## Collections

- `array<T>` — 1D, homogeneous. `array.new_int()`, `array.new_float()`, etc.
  ★v6: negative indices wrap (e.g. `array.get(a, -1)` is last).
- `matrix<T>` — 2D, homogeneous. `matrix.new<float>(3, 3, 0.0)`.
- `map<K, V>` — dict. `map.new<string, float>()`.

## User-defined types (UDTs)

```pine
//@version=6
type Bracket
    float entry = na
    float stop  = na
    float take  = na

b = Bracket.new(entry = close, stop = close * 0.99, take = close * 1.02)
```

Fields can have default values. Add methods via `method` keyword:

```pine
method distance(Bracket self) =>
    self.take - self.entry
```

## Drawing objects

`line`, `label`, `box`, `table`, `polyline`, `linefill` are **first-class
series types** that persist across bars (subject to script-level
`max_lines_count`, `max_labels_count`, `max_boxes_count`, etc.). They
must be created via `*.new()` and deleted via `*.delete()` when no
longer needed to avoid resource exhaustion.

## See also

- [[../operators-keywords/index]] — operators on each type
- [[../pitfalls#v6-a-integer-division]]
- [[../pitfalls#v6-c-bool-no-na]]
- Canonical: `wiki.md` § Types, Mental Model
