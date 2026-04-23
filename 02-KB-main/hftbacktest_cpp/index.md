---
title: hftbacktest_cpp — llm-wiki navigation
tool: hftbacktest_cpp
upstream: HyperFrequency/hftbacktest_cpp
fork: HyperFrequency/hftbacktest_cpp
language: C++17
sibling: hftbacktest
canonical_wiki: ../../../hyperfrequency/docs/deep-tool-wiki/hftbacktest_cpp/wiki.md
last_updated: 2026-04-20
---

# hftbacktest_cpp — llm-wiki

Tree-navigable offline context for **hftbacktest_cpp**, the HyperFrequency native
C++17 port of an MBO-driven CME-futures backtesting engine. Distinct from
the Rust sibling [[../hftbacktest/index]] (which is Rust + PyO3 + Numba).

The C++ port is a deliberately **minimal, non-templated** codebase: 5 headers,
4 translation units, ~440 lines of source, zero external dependencies beyond the
C++17 standard library.

## Sitemap

```text
hftbacktest_cpp/
├── Build/                                     (cmake) plain CMake >= 3.10
│   └── [[Build/CMakeLists]]                   — cpp17, -O3, single executable
├── Types/                                     (header-only POD structs, types.h)
│   ├── [[Types/Event]]                        — time, action, side, price, size, id
│   ├── [[Types/Order]]                        — inherits Event; intrusive DLL node
│   ├── [[Types/Limit]]                        — price level aggregate
│   └── [[Types/Trade]]                        — trade tape row for callbacks
├── Book/                                      (cpp) order book reconstruction
│   ├── [[Book/Book]]                          — concrete class; 3-index design
│   ├── [[Book/apply]]                         — action dispatch (A/R/M/C)
│   ├── [[Book/add]]                           — FIFO insertion
│   ├── [[Book/modify]]                        — priority-preserve vs lose
│   ├── [[Book/cancel]]                        — partial vs full
│   ├── [[Book/clear]]                         — hard reset
│   └── [[Book/cost_buy_sell]]                 — walk-the-book cost (BUGGY)
├── Engine/                                    (cpp) backtest loop
│   ├── [[Engine/Engine]]                      — constructor + run driver
│   ├── [[Engine/run]]                         — CSV stream + latency-gated cb
│   ├── [[Engine/mkt_buy_sell]]                — enqueue market op (MATCH TODO)
│   └── [[Engine/latency_model]]               — single constant-latency field
├── Callbacks/                                 (header) virtual base interface
│   ├── [[Callbacks/Callbacks]]                — subclass to implement
│   └── [[Callbacks/trade]]                    — virtual void trade(Trade&)
├── CSV/                                       (cpp) Databento MBO parsing
│   ├── [[CSV/parse_header]]                   — first-line columns
│   ├── [[CSV/parse_line]]                     — map&lt;col,val&gt; per row
│   └── [[CSV/encode_event]]                   — map → typed Event
└── PythonBindings/                            (PLANNED — not yet present)
    └── [[PythonBindings/roadmap]]             — probable pybind11 path
```

→ **Canonical wiki body:** `hyperfrequency/docs/deep-tool-wiki/hftbacktest_cpp/wiki.md`
→ **RefGraphs:** `hyperfrequency/docs/deep-tool-wiki/hftbacktest_cpp/assets/refgraph-cpp-headers.mmd`, `refgraph-templated-classes.mmd`
→ **InfraNodus graph:** `deep-tool-wiki-hftbacktest_cpp` (pending pipeline run)
→ **Consolidated pitfalls:** [[pitfalls]]
→ **Overview:** [[overview]]
→ **Rust sibling:** [[../hftbacktest/index]]

## Status

- [x] `index.md` (this page) — sitemap populated
- [x] `overview.md` — populated
- [x] `pitfalls.md` — populated (C++-specific, incl. known upstream bugs)
- [x] Subsystem indexes — placeholder stubs at `Build/`, `Types/`, `Book/`,
      `Engine/`, `Callbacks/`, `CSV/`, `PythonBindings/`
- [ ] Per-leaf expansion (each sitemap leaf gets its own `.md` with
      signature + minimal example + linked pitfalls)

## See also

- Canonical deep-tool-wiki: `../../hyperfrequency/docs/deep-tool-wiki/hftbacktest_cpp/wiki.md`
- Rust sibling: [[../hftbacktest/index]] (for the trait-generic design that
  this C++ port does *not* mirror)
- Cross-tool: [[../nautilus-trader/index]] (production event-driven exec),
  [[../tardis-python/index]] (NPZ feed pipeline — irrelevant to this C++
  port which uses Databento CSV)
