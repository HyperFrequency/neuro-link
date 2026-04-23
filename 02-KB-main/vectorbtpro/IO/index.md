---
title: IO subsystem
parent: [[../index]]
tool: vectorbtpro
subsystem: io
last_updated: 2026-04-20
---

# IO subsystem

Persistence and format bridges. Portfolios, Data objects, and
IndicatorFactory outputs all implement `.save()` / `.load()` via the
`Pickleable` base, plus dedicated format handlers for interoperability.

## Leaves

- `[[Pickle]]` — `pf.save('file.pkl')` / `Portfolio.load('file.pkl')`.
  Uses `dill` for closure / lambda serialization. Default format.
- `[[Parquet]]` — `ParquetData` class. Columnar, fast, compressed.
  Best for multi-asset OHLCV storage.
- `[[HDF5]]` — `HDFData` class. Hierarchical groups; good for multi-
  symbol multi-timeframe archives. Requires `h5py`.
- `[[Feather]]` — `FeatherData` class. Arrow-backed, very fast
  read/write for DataFrame-sized data.
- `[[CSV]]` — `CSVData` class. Plain text; slowest but most portable.
- `[[DuckDB]]` (PRO) — `DuckDBData`. Analytical SQL over local / S3
  files without a server.
- `[[SQL]]` — `SQLData`. SQLAlchemy-backed Postgres / MySQL / SQLite.
- `[[ArcticDB]]` (PRO) — Man Group's time-series DB. Best for very large
  (TB-scale) tick data.
- `[[TOML / YAML / JSON]]` — `Pickleable.decode_toml` / `.encode_yaml`
  / `.decode_config` helpers. Used for config round-trips.

## Pattern

```python
# Roundtrip a fetched Data bundle
data = vbt.BinanceData.fetch(["BTCUSDT", "ETHUSDT"], start="2024-01-01")
data.to_parquet("crypto_2024.parquet")

# Later
data = vbt.ParquetData.load("crypto_2024.parquet")

# Or save the Portfolio itself
pf.save("my_portfolio.pkl")
pf = vbt.Portfolio.load("my_portfolio.pkl")
```

## See also

- Canonical wiki § Detailed Usage Guide:
  `hyperfrequency/docs/deep-tool-wiki/vectorbtpro/wiki.md#detailed-usage-guide`
- `Pickleable` class appears in the RefGraph top-30 via its
  `encode_*` / `decode_*` methods (many inherited helpers).
