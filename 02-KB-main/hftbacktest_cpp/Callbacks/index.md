---
title: hftbacktest_cpp — Callbacks
parent: [[../index]]
subsystem: Callbacks
last_updated: 2026-04-20
---

# Callbacks

User strategy interface — a virtual base class. `include/callbacks.h` (10 lines).

## Leaves

- [[Callbacks]] — `class Callbacks { public: virtual void trade(Trade& info)
  {} };` — the entire interface.
- [[trade]]     — the single virtual method, with a default empty body.

## Usage pattern

```cpp
class MyStrategy : public Callbacks {
public:
    void trade(Trade& info) override {
        // react to trade print (delayed by latency from Engine)
    }
};

MyStrategy strat;
Engine engine(&strat, /*latency_ns=*/1'000'000);
engine.run("glbx-mdp3-20240102.mbo.csv", "4120818");
```

## Design notes

- **Non-pure virtual** — default empty body means subclasses don't *have* to
  override `trade`. Adding a pure virtual would break existing subclasses.
- **ABI hazard on extension** — adding a new virtual method changes the
  vtable layout. Recompile all subclass users.
- **Non-const `Trade&`** — a user can mutate `info`. No engine-side
  consequence; slightly surprising API. See [[../pitfalls]].

## Not-yet-present callbacks

Likely near-future additions (none currently present):

- `virtual void book_update(const Book&, uint64_t time)` — react to every
  book-state change (not just trades).
- `virtual void order_filled(uint64_t order_id, uint32_t qty, double price)`
  — would be needed once `Engine::run` market-op matching ships.
- `virtual void order_canceled(uint64_t order_id)` — symmetric.

## See also

- Canonical wiki section: `wiki.md` → "`Callbacks` — virtual-base strategy
  hook"
