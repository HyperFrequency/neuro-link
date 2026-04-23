---
title: Persistence — catalog, backends, wranglers
parent: nautilus-trader/index
---

# Persistence (Python + Rust, dual-exposed)

## Leaves

- **ParquetDataCatalog** (`py+rust`) — partitioned Parquet store for ticks,
  bars, and books. Read/write via `.write_chunk(data)` and `.query(...)`.
  Built on Arrow + DataFusion.
- **StreamingFeatherWriter** (`py+rust`) — Arrow-native streaming writer
  for continuous tick logging.
- **Wranglers** (`py+rust`) — pandas DataFrame → Nautilus data:
  `QuoteTickDataWrangler`, `TradeTickDataWrangler`, `BarDataWrangler`,
  `OrderBookDeltaDataWrangler`, `OrderBookDepth10DataWrangler`.
- **RedisCacheDatabase** (`py+rust`) — Redis backend for `Cache`.
- **PostgresCacheDatabase** (`py+rust`) — Postgres backend for `Cache`.
- **RedisMessageBusDatabase** (`py+rust`) — durable MessageBus stream.

## Pitfalls

- Parquet schema is versioned with Nautilus; upgrading Nautilus may
  require rewriting a catalog. Pin the catalog-writer version or emit the
  schema version alongside.
- `DataBackendSession` holds a DataFusion context open — one session per
  long-running process is typical; close before exit to flush.

## Cross-links

- Canonical wiki §"Detailed Usage Guide 17. Persistence with Redis and PostgreSQL" + §"18. Persisting messages and streaming to external sinks"
- Rust source: `crates/persistence/`
