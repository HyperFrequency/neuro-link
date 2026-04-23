---
title: hftbacktest_cpp — consolidated pitfalls
parent: [[index]]
tool: hftbacktest_cpp
last_updated: 2026-04-20
---

# hftbacktest_cpp — consolidated pitfalls

One-page catalogue of recurring C++-specific footguns, known upstream bugs, and
design-level fragilities. Each item names the root cause, the symptom, and the
resolution. Anchored to the 2026-04-20 HEAD of the cloned `master` branch —
some items may be fixed upstream after that date. Cross-reference against the
canonical wiki's `## Pitfalls` section
(`hyperfrequency/docs/deep-tool-wiki/hftbacktest_cpp/wiki.md`).

## Known bugs in upstream (2026-04-20)

### `Book::cost_buy` — iterator never advanced

```cpp
double Book::cost_buy(uint32_t size) const {
    double cost = 0;
    const auto it = asks.begin();          // const → cannot increment
    while (size > 0 && it != asks.end()) {
        uint32_t qty = std::min(size, it->second->size);
        cost += qty * it->first;
        size -= qty;
    }
    return cost;
}
```

The iterator is `const auto`; the loop body never increments it. Impact:
infinite loop (if `size > top_level_size`) or wrong cost (multiplies additional
fills by the best price forever). **Fix:** `auto it = asks.begin();` + add
`++it;` inside the loop.

### `Book::cost_sell` — references `asks.end()` while walking `bids`

```cpp
const auto it = bids.begin();
while (size > 0 && it != asks.end()) { ... }   // typo: asks → bids
```

Same iterator bug as `cost_buy` plus a cross-side termination check that
compares a `bids` iterator against `asks.end()`. Behavior is implementation-
defined. **Fix:** `it != bids.end()` + increment.

### `Engine::run` — market-op matching is a TODO

```cpp
for (const auto& op : ops) {
    if (op.time <= event.time) break;
    if (op.action == 'M') {
        // EMPTY BODY — market orders never fill
    }
}
```

Calling `Engine::mkt_buy` / `Engine::mkt_sell` enqueues the op correctly but
the matching block is unimplemented. Impact: strategies that depend on market-
order fills observe **zero fills**. **Fix:** walk `book.asks` / `book.bids`
top-down, subtract qty, fire a `fill` callback (which also doesn't exist yet
— see next item).

### `Engine::run` — callback dispatch reads wrong fields

```cpp
if (cb.action == 'T') {
    Trade info;
    info.price = event.price;    // bug: should be cb.price
    info.side  = event.side;     // bug: should be cb.side
    info.time  = event.time;     // bug: should be cb.time
    info.size  = event.side;     // bug: size = side char; also should be cb.size
    callback->trade(info);
}
```

All four field assignments use the **current** `event` rather than the
**delayed** `cb` — defeating the whole point of the latency buffer. Worst bug:
`info.size = event.side` assigns a `char` (`'B'`/`'S'`/`'A'` ≈ 65–83) to a
`uint32_t`. **Fix:** use `cb.price` / `cb.side` / `cb.time` / `cb.size`.

### `Book::modify` — `size >= order->size` branch loses priority on equality

```cpp
if (event.price != order->price || event.size >= order->size) {
    delete_order(order->id); add(event);
}
```

A size-equal modify (exchange echoing current state) triggers a full
delete+re-add. **Fix:** change to strict `>`, or add an early-return if
`(event.price == order->price && event.size == order->size)`.

## C++-specific footguns

### Template instantiation cost — N/A today, relevant if porting grows

The 2026-04-20 codebase uses **zero templates** (other than stdlib container
parameters). If a future patch introduces `Book<QueueModel>` or
`Engine<LatencyModel>`, watch for:

- Compile-time blow-up if instantiated in many translation units.
- Debug symbol size (especially with `-g` + `-O0`).
- Link-time errors from explicit-vs-implicit instantiation.

Keep template code in headers (required for implicit instantiation) or explicitly
instantiate in one `.cpp` with `template class Foo<Bar>;` to localize cost.

### Header-only vs compiled translation units

All class definitions live in `include/*.h`; all implementations live in
`src/*.cpp`. That's the "compiled TUs" discipline. The risk is **ODR
(one-definition-rule) violation** if any `.cpp` accidentally defines a non-
inline member function already declared in a header. The current code avoids
this, but adding an `inline` helper in a header without the `inline` keyword
silently violates ODR and produces linker errors only in release builds (where
inlining doesn't hide the duplicate definition).

**Rule:** free functions in headers must be `inline`, `static`, or template.
Member functions defined inline inside the class body are implicitly inline.

### Undefined behavior traps

- **`std::map::at` on missing key throws** — the codebase uses `orders.at(id)`
  without try/catch. A malformed feed row with an unknown order_id terminates
  the backtest with `std::out_of_range`. Either wrap in `orders.find` + `!=
  orders.end()` guards, or catch at a higher level.
- **Dereferencing `asks.begin()` on an empty map** is UB. `cost_buy` uses
  `asks.begin()` without checking; the loop guard `it != asks.end()` saves it
  in practice, but if the iterator bug is fixed without also adding an empty-
  book early-return, the resulting `it->second` deref is still UB on first
  entry.
- **Slicing `Order → Event` on copy** — `Order` publicly inherits `Event`, so
  `Event e = some_order;` silently drops `next`/`prev`/`parent`. Harmless today
  (no such copy exists in the code), but worth watching.
- **`std::stoul(str)` on empty or non-numeric string** throws `std::invalid_argument`.
  The CSV parser trusts Databento format; a tampered CSV row crashes the run.

### Memory ordering — no concurrency today, will matter with live-trading

The code is single-threaded. `Book` and `Engine` have no atomics, no
`std::mutex`, no `std::memory_order`. If the port grows a concurrent feed
handler (a common extension when adding live support), every field access
on shared book state becomes a data race without explicit synchronization.

### C++ version requirements

- `std::map<double, Limit*, std::greater<>>` — the bare `<>` trailing-bracket
  shorthand for template argument deduction requires **C++17** or later.
- `std::ifstream` + `std::getline` behavior is consistent across C++11+.
- No `std::filesystem`, no `std::format`, no `std::span` — no C++20 features.

**MSVC:** requires `/std:c++17` flag. The shipped `CMakeLists.txt` handles this
correctly via `set(CMAKE_CXX_STANDARD 17)` + `set(CMAKE_CXX_STANDARD_REQUIRED
True)`.

### Platform compatibility

- **Tested:** implicitly on macOS / Linux (the `.vscode/` shipped in the repo
  targets clang).
- **Untested:** Windows (MSVC and MinGW). Should work — no POSIX APIs, no `<unistd.h>`,
  no `fork` / `exec`. Line-ending differences in CSV files (CRLF vs LF) might
  slip through `std::getline` incorrectly on Windows — strip the trailing `\r`
  defensively.
- **Endianness:** binary never written; no concern.
- **Apple Silicon:** no ARM-specific code; `-O3` autovectorizes into NEON
  where profitable.

### Static vs dynamic linkage

The shipped target is an executable (`add_executable(main ...)`), not a library.
To use the code as a library in another project, you'd need to change to
`add_library(hftbacktest_cpp STATIC ...)` or `SHARED`. Gotchas with each:

- **STATIC:** no symbol-export concerns; the consumer must recompile when
  headers change. Preferred default.
- **SHARED / DLL:** on Windows requires `__declspec(dllexport)` /
  `__declspec(dllimport)` annotations on every public class; hidden-by-default
  symbol visibility on Linux/macOS (`-fvisibility=hidden`) requires explicit
  `__attribute__((visibility("default")))`. The current headers have none
  of this — a naive DLL build would miss all exports.
- **Header-only variant:** not possible with the current `.cpp`-split design
  without moving implementations into headers (and dealing with ODR).

### Raw `new`/`delete` ownership

`Book` allocates `Order*` and `Limit*` with `new`, tracks them in maps, and
frees them in `delete_order` / `clear` / `~Book`. Edge cases:

- **Exception escape from `Book::add`** between `new Order` and the successful
  `orders.emplace` → leaks one `Order*`. `std::bad_alloc` from `new` is the
  only realistic throw.
- **`clear()` under reentry** (e.g., if a callback indirectly triggers another
  `apply('R')`) is safe because `clear` erases all containers before returning
  — but the pattern is fragile.
- **Fix:** wrap allocation in `std::unique_ptr<Order>` inside a function-scope
  local; release into the map on successful emplace. ~3-line change per site.

## Data / feed pitfalls

### Databento MBO assumption

The parser assumes column names `ts_event`, `action`, `side`, `price`, `size`,
`order_id`, `instrument_id`, `flags`. A rename upstream (Databento has shipped
schema migrations historically) silently breaks `encode_event` with an
`std::out_of_range` from `row.at("...")`. **Fix:** pin Databento schema version
in a comment; assert on parse.

### `flags & 128` — end-of-batch flush

The engine only processes pending market ops when the Databento flush bit
(`0x80`) is set. Two risks:

- Databento occasionally packs multiple atomic updates with **no** flush bit
  set (malformed feed or converter bug), and ops back up indefinitely.
- A different feed provider without that bit breaks the engine's flush-
  boundary logic entirely.

**Fix:** add a watchdog — if `ops` grows beyond N entries without being
processed, log a warning.

### `double` price bit-equality assumption

`std::map<double, Limit*>` uses operator< on doubles. Two events supposedly
at the same price but with different float encodings will fragment the book.
Databento is safe, but any pandas round-trip (e.g., `df.to_csv` at default
precision) corrupts this invariant. **Fix:** read prices as integer nanounits
and convert on display only, or use `std::map<int64_t, Limit*>` keyed by
tick count.

### `Event::side` — `'B'`/`'A'` vs `'B'`/`'S'` inconsistency

Databento uses `'B'` bid / `'A'` ask. `Book::delete_order` correctly tests
`order->side == 'B'`. `Engine::mkt_sell` sets `op.side = 'S'`. A user who
writes `if (event.side == 'S')` gets perpetually false for real feed events.
**Fix:** normalize at parse time, or document the mapping table in a header
comment and in the README.

## Engineering fragilities

### No tests, no CI

No `tests/` directory, no `ctest` target, no GitHub Actions workflow. Bugs
found in this document were discovered by reading. **Fix:** add doctest or
Catch2 (both header-only); start with `test_book.cpp` — apply canned A/M/C
sequences, assert book state.

### `main.cpp` hard-codes CSV path and instrument ID

```cpp
engine.run("glbx-mdp3-20240102.mbo.csv", "4120818");
```

No CLI parsing, no env-var override, no config file. **Fix:** `argv[1]` +
`argv[2]` in `main`.

### CSV parser has no quote/escape handling

`parse_line` splits on every comma. Databento MBO has no string columns, so
the parser is safe *today*. Switching to a different data source (e.g., ICE,
LSE, OPRA) that uses quoted strings will silently mis-split. **Fix:** drop in
a small header-only CSV library (nanocsv, csv-parser) before supporting new
feeds.

### `std::map` is not cache-friendly

Red-black trees chase pointers; each level traversal is a cache miss on
typical L2/L3. On HFT workloads a `boost::container::flat_map` or
hand-rolled sorted vector is 2-5× faster for BBO lookup. The Rust sibling
offers `ROIVectorMarketDepth` for exactly this reason. **Fix (when perf
matters):** prototype a flat-map `Book2` + benchmark.

### No `-march=native`, no LTO

The shipped `CMAKE_CXX_FLAGS "-O3"` leaves 10-20% performance on the table.
**Fix (optional):**

```cmake
target_compile_options(main PRIVATE -O3 -march=native -flto)
target_link_options(main PRIVATE -flto)
```

Caveat: `-march=native` produces a non-portable binary.

### `Engine` keeps `cbs` and `ops` unbounded

`std::vector<Event> cbs` accumulates every trade-print for the entire run
and is never drained. Memory cost is fine for a one-day backtest (~GB), but
cache pressure is measurable. **Fix:** drain fired entries on each flush
boundary.

### `Callbacks::trade(Trade&)` is a non-const reference

```cpp
virtual void trade(Trade& info) {}
```

A user strategy can mutate `info` without consequence (the engine doesn't
read it back), but the API is surprising. **Fix:** `virtual void
trade(const Trade& info)`. ABI-breaking; bump a version number.

### No README build/platform guidance

The shipped README is 16 lines: "fast, written in C++", "Python bindings
coming", plus a motivation paragraph. No build commands, no platform table,
no example strategy. **Fix:** one-paragraph "Building from source" + one-
paragraph "Platforms" + a minimal `BookTopPrinter` strategy example.

## Cross-library hazards

### **Not ABI-compatible with `hftbacktest` (Rust)**

The two codebases share a README-level intent but no actual interop:

- Different data formats (Databento CSV vs tardis-machine NPZ).
- Different `Event` struct shapes (6 fields vs 8).
- No shared header or `cbindgen` bridge.
- No queue-model / latency-model trait equivalence.

Strategies ported from Rust → C++ or vice versa are **manual rewrites**.

### Python access today requires process invocation

Until Python bindings ship, the only way to drive the C++ engine from Python
is `subprocess.run([./main, ...])` + parsing `stdout`. No in-process
interop; no zero-copy data hand-off; no Numba fast-path like the Rust side
has. Plan accordingly.

## See also

- Canonical wiki pitfalls section:
  `hyperfrequency/docs/deep-tool-wiki/hftbacktest_cpp/wiki.md#pitfalls`
- Rust-side pitfalls: [[../hftbacktest/pitfalls]]
- Deep-concepts context in this vault: [[overview]], [[index]]
