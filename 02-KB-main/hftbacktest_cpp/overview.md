---
title: hftbacktest_cpp — overview
parent: [[index]]
tool: hftbacktest_cpp
last_updated: 2026-04-20
---

# hftbacktest_cpp — overview

`hftbacktest_cpp` is the HyperFrequency fork's **native C++17 port** of an
MBO-driven (market-by-order) backtesting engine aimed at CME futures strategies
running on **Databento** MBO CSV feeds. The codebase is deliberately compact —
roughly 440 lines across 5 headers (`types.h`, `book.h`, `engine.h`,
`callbacks.h`, `csv.h`) and 4 translation units (`book.cpp`, `engine.cpp`,
`csv.cpp`, `main.cpp`). It rebuilds the full limit order book via a
three-index design (`std::map<double, Limit*>` for asks ascending,
`std::map<double, Limit*, std::greater<>>` for bids, `std::unordered_map` for
order-id lookup, `std::unordered_multimap` keyed by price) and connects an
intrusive doubly-linked list of `Order*` nodes per `Limit` to preserve
price-time priority. The `Engine` class streams CSV rows, filters by
`instrument_id`, applies each action (`'A'` add / `'R'` reset / `'M'` modify /
`'C'` cancel / `'T'` trade print) to the book, and fires the user strategy's
virtual `Callbacks::trade(Trade&)` one **constant latency budget** after the
underlying feed event (simulating feed-to-strategy propagation delay).
`mkt_buy`/`mkt_sell` enqueue simulated market orders stamped with
`last_time + latency`, though their matching logic is **not yet implemented**
in this commit. The build system is a 10-line `CMakeLists.txt` with **zero
external dependencies** — no Boost, no Eigen, no pybind11, no asio, no fmt,
no BLAS — everything comes from the C++17 standard library. Python bindings
are listed as "coming" in the README but no `python/` directory, no pybind11
reference, and no `setup.py` exist yet. The design consciously avoids
templates and smart pointers: one concrete `Book` class, one concrete
`Engine` class, one virtual method (`Callbacks::trade`), raw `new`/`delete`
ownership. This makes the codebase faster to compile and easier to read than
the trait-generic Rust sibling `hftbacktest` (see [[../hftbacktest/index]]),
but the trade-off is lost extensibility — adding a new queue model, depth
backend, or latency model requires editing the core rather than dropping in
a new trait impl. Several known bugs and TODOs exist in the 2026-04-20 HEAD
(iterator-never-incremented in `cost_buy`/`cost_sell`, empty `'M'` op-matching
branch in `Engine::run`, `event.price` instead of `cb.price` in callback
dispatch, `size = event.side` char-to-uint32 coercion) — see
[[pitfalls]] for the full catalogue. Full canonical reference:
`hyperfrequency/docs/deep-tool-wiki/hftbacktest_cpp/wiki.md`
(~1000 lines with sitemap, five deep concept sections, five walkthroughs, full
API table, 16 pitfalls, performance characteristics table, and dual reasoning
ontology).
