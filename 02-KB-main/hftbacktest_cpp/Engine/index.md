---
title: hftbacktest_cpp — Engine
parent: [[../index]]
subsystem: Engine
last_updated: 2026-04-20
---

# Engine

Backtest driver. Single concrete class `Engine` in `include/engine.h` +
`src/engine.cpp` (83 LOC). Owns a `Book` for the duration of `run`, holds a
non-owning `Callbacks*`, and buffers pending trade callbacks (`cbs`) and
pending market ops (`ops`).

## Leaves

- [[Engine]]        — class declaration + constructor.
- [[run]]           — `int run(const std::string& file, const std::string&
  instr_id)`; streaming CSV loop with single-instrument filter and
  latency-gated callback fire.
- [[mkt_buy_sell]]  — `uint64_t mkt_buy(uint32_t)` / `mkt_sell(uint32_t)` —
  enqueue a simulated market op; **matching logic is a TODO** in upstream,
  so these calls currently never produce fills. See [[../pitfalls]].
- [[latency_model]] — a single `uint64_t latency` field (constant). No
  `LatencyModel<T>` abstraction; to get variable or interpolated latency,
  patch `Engine`.

## Event loop shape

```text
for each CSV row:
    filter by instrument_id
    parse to Event
    last_time = event.time
    book.apply(event)
    for each cb in cbs:
        if (event.time - cb.time > latency): fire callback
        else: break
    if event.action == 'T': cbs.push_back(event)
    if flags & 128 (end-of-batch flush):
        for each op in ops:
            if op.time <= event.time: break
            if op.action == 'M': /* TODO: match against book */
```

## See also

- Canonical wiki section: `wiki.md` → "`Engine` — streaming replay + latency-
  budget callback dispatch"
- Pitfalls: market-op-todo, callback-field-bug, unbounded cbs buffer —
  see [[../pitfalls]]
