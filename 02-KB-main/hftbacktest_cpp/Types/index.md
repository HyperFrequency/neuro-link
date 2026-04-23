---
title: hftbacktest_cpp — Types
parent: [[../index]]
subsystem: Types
last_updated: 2026-04-20
---

# Types

POD (plain-old-data) struct definitions for all feed-event, order-book, and
trade-tape entities. All four structs live in a single header, `include/types.h`.

## Leaves

- [[Event]]   — `{time, action, side, price, size, id}`; 6 fields; the canonical
  feed-row representation.
- [[Order]]   — `: public Event` + `{next, prev, parent}`; resting limit order;
  node in an intrusive doubly-linked list per `Limit`.
- [[Limit]]   — `{price, side, num, size, head, tail}`; price-level aggregate
  holding the FIFO queue of orders at that level.
- [[Trade]]   — `{time, price, size, side}`; trade-tape row handed to
  `Callbacks::trade`.

## Design notes

- **All POD** — no constructors, no destructors, no virtual methods.
- **Raw pointers** — `Order::next/prev/parent` and `Limit::head/tail` are raw
  pointers owned by the enclosing `Book`. No smart pointers anywhere.
- **`Order : Event` public inheritance** — slicing hazard exists in principle
  (`Event e = an_order;` drops DLL fields) but no such copy exists in the code.
- **`double` prices** — bit-exact equality required in `std::map` keys; see
  pitfall in [[../pitfalls]] about float round-tripping.

## See also

- Canonical wiki section: `wiki.md` → "Event / Order / Limit / Trade — the POD
  struct quartet"
