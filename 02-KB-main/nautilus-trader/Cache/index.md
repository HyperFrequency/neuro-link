---
title: Cache — state store
parent: nautilus-trader/index
---

# Cache (Python + Rust, dual-exposed)

The cache is the central read-through state store. Before any strategy
handler fires, the cache is already updated with the triggering event, so
`self.cache.quote_tick(id)` inside `on_quote_tick` returns the tick that
just arrived.

## Contents

- Last `QuoteTick` / `TradeTick` / `Bar` per instrument (or per bar type)
- `OrderBook` per instrument (L1 / L2 / L3)
- All open, closed, and emulated `Order`s
- All open and closed `Position`s
- `AccountState` per venue
- `Instrument` definitions
- `CurrencyPair` cache
- `Strategy` and `Actor` registrations
- `OrderList` (brackets, OCO sets)

## Backends

- **In-memory** (default)
- **Redis** — `RedisCacheDatabase`. Durable, crash-recoverable state; can
  bootstrap a fresh process from persistent Redis keys.
- **Postgres** — `PostgresCacheDatabase`. Relational queries, long-term
  analytics.

## Leaves

- **Overview** — this page
- **RedisBackend** — durable state in Redis
- **PostgresBackend** — durable state in Postgres

## Pitfalls

- In backtests, do not assume the cache is pre-populated with "historical"
  data. Only the streaming events you have subscribed to will be present.
  For warm-up use `request_bars(start, end)` + `on_historical_data`.
- Redis-backed cache state must match the engine version; schema changes
  between Nautilus versions can require a flush.

## Cross-links

- Rust source: `crates/common/src/cache/`
- Canonical wiki §"`Cache`"
- Persistence backends: `[[Persistence/RedisCacheDatabase]]` / `[[Persistence/PostgresCacheDatabase]]`
