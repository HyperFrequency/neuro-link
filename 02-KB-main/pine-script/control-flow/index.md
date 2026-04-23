---
title: pine-script — Control-flow
parent: [[../index]]
last_updated: 2026-04-20
---

# pine-script — Control-flow

Pine's control flow is minimalist by design: `if/else`, `switch`, `for`,
`for...in`, `while`. No `break` from `if`; no `continue` modifiers beyond
the built-in statement.

## `if` / `else`

```pine
signal = 0
if close > open and volume > ta.sma(volume, 20)
    signal := 1
else if close < open
    signal := -1
plot(signal)
```

**Block-scope rule.** Variables *declared* inside an `if`-block are
local to that block. Variables *assigned* inside an `if`-block must be
declared in an outer scope and use `:=` reassignment. See
[[../pitfalls#block-scope]].

`if` is an expression: returns the last value of the branch taken.

```pine
direction = close > open ? "up" : close < open ? "down" : "flat"
// equivalent via if-as-expression:
direction2 = if close > open
    "up"
else if close < open
    "down"
else
    "flat"
```

## `switch`

```pine
tf = timeframe.period
multiplier = switch tf
    "1"   => 1.0
    "5"   => 5.0
    "15"  => 15.0
    "60"  => 60.0
    "D"   => 1440.0
    => na      // default case
```

Or condition-based (no discriminant):

```pine
regime = switch
    ta.rsi(close, 14) > 70 => "overbought"
    ta.rsi(close, 14) < 30 => "oversold"
    =>                       "neutral"
```

## `for`

```pine
sum = 0.0
for i = 0 to 9
    sum := sum + close[i]
avg10 = sum / 10.0
```

★**v6 change**: the `to` bound is re-evaluated **before every iteration**.
If the bound depends on an expression that grows (e.g.
`array.size(arr) - 1` where `arr` is being pushed to inside the loop),
the loop runs forever. Store the bound in a local first:

```pine
var arr = array.new_int()
upper = array.size(arr) - 1    // snapshot before loop
for i = 0 to upper
    ...
```

`by` step supported: `for i = 0 to 100 by 5`.

## `for...in`

```pine
arr = array.from(1, 2, 3, 4, 5)
sum = 0
for v in arr
    sum += v
// Indexed form:
for [i, v] in arr
    label.new(bar_index - i, v, str.tostring(v))
```

## `while`

```pine
i = 0
while i < 10 and array.get(queue, i) != targetValue
    i += 1
```

Budget-aware: each iteration counts against the script's scope budget.
Infinite loops are terminated by the runtime, not the compiler. Prefer
`for` with a snapshot bound where possible.

## `method`

v5/v6 method calls on UDTs:

```pine
type Ring
    array<float> buf
    int head = 0

method push(Ring self, float x) =>
    array.set(self.buf, self.head, x)
    self.head := (self.head + 1) % array.size(self.buf)

r = Ring.new(array.new_float(10, 0.0))
r.push(close)       // calls method push
```

## Tuple unpacking

```pine
[fast, slow, signal] = ta.macd(close, 12, 26, 9)
```

Tuple declarations cannot use keywords (`var`/`varip`/qualifiers) — each
element inherits its assigned type.

## See also

- [[../pitfalls#for-bound-reeval]]
- [[../pitfalls#block-scope]]
- Canonical: `wiki.md` § Mental Model / Pitfall Catalog
