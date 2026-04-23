---
title: hftbacktest_cpp — Book
parent: [[../index]]
subsystem: Book
last_updated: 2026-04-20
---

# Book

Order-book reconstruction. Single concrete class `Book` in `include/book.h` and
`src/book.cpp` (170 LOC). Three-index design with an intrusive doubly-linked
list per price level.

## Leaves

- [[Book]]        — class declaration; the public surface.
- [[apply]]       — `bool apply(const Event&)`; action-dispatch entry point.
- [[add]]         — `void add(const Event&)`; FIFO insertion at price level.
- [[modify]]      — `void modify(const Event&)`; priority-preserve vs lose rules.
- [[cancel]]      — `void cancel(const Event&)`; partial vs full branch.
- [[clear]]       — `void clear()`; full reset on `'R'` snapshot event.
- [[cost_buy_sell]] — `double cost_buy(uint32_t) const` / `cost_sell(...)` — walk
  the book to estimate cash cost. **BUGGY in upstream** (iterator not
  advanced) — see [[../pitfalls]].

## Three-index layout

```cpp
std::map<double, Limit*>                  asks;    // ascending
std::map<double, Limit*, std::greater<>>  bids;    // descending
std::unordered_map<uint64_t, Order*>      orders;  // id → Order
std::unordered_multimap<double, Limit*>   limits;  // price → Limit (bid & ask)
```

- `asks`/`bids` — ordered red-black trees for BBO and side iteration.
- `orders` — O(1) lookup by `order_id` for `modify` and `cancel`.
- `limits` — multimap because a bid and ask can share a price on snapshot
  replay.

## See also

- Canonical wiki section: `wiki.md` → "`Book` — map-of-map-of-linked-list"
- Pitfalls: cost_buy/cost_sell iterator bugs, modify-equality-bug, raw-
  new/delete-leak — see [[../pitfalls]]
