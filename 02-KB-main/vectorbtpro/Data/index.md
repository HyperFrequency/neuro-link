---
title: Data subsystem
parent: [[../index]]
tool: vectorbtpro
subsystem: data
last_updated: 2026-04-20
---

# Data subsystem

Unified ingestion layer. Every `*Data` class inherits from either
`CustomData` (synthetic / local) or `RemoteData` (API-backed). The base
`Data` class provides common methods: `.get()`, `.run()`, `.save()`,
`.load()`, `.update()`, `.resample()`.

## Leaves (remote / API-backed)

- `[[BinanceData]]` — Binance spot + futures via `python-binance`.
  Methods: `fetch_symbol`, `fetch_klines`. Supports WebSocket live feed
  in PRO.
- `[[CCXTData]]` — Any exchange supported by CCXT (100+ venues).
  Requires `ccxt` pip install.
- `[[YFData]]` — Yahoo Finance via `yfinance`. Stocks, ETFs, indices.
  Free, rate-limited.
- `[[PolygonData]]` (PRO only) — Polygon.io for US equities + options +
  crypto. API key required.
- `[[AlpacaData]]` — Alpaca brokerage data + paper/live trading bridge.
- `[[AVData]]` — Alpha Vantage for fundamental + forex.
- `[[TVData]]` (PRO only) — TradingView data via their private WebSocket.
  Uses `USER_AGENT` / `WS_TIMEOUT` constants from `data/custom/tv.py`.
- `[[BentoData]]` — Databento institutional market data.
- `[[FinPyData]]` — FinancialModelingPrep API.
- `[[NDLData]]` — Nasdaq Data Link (formerly Quandl).
- `[[RemoteData]]` — base class for all API-backed loaders.

## Leaves (local / file)

- `[[CSVData]]` — Read/write CSV.
- `[[ParquetData]]` — Apache Parquet (fast columnar).
- `[[FeatherData]]` — Arrow Feather format.
- `[[HDFData]]` — HDF5 via `h5py`.
- `[[DuckDBData]]` (PRO only) — DuckDB analytical queries.
- `[[SQLData]]` — SQLAlchemy / PostgreSQL / MySQL.
- `[[ArcticDBData]]` (PRO only) — Man Group's time-series DB.
- `[[LocalData]]` / `[[FileData]]` — generic file routing.

## Leaves (synthetic)

- `[[GBMData]]` — Geometric Brownian Motion.
- `[[GBMOHLCData]]` — GBM with OHLC bars.
- `[[RandomData]]` / `[[RandomOHLCData]]` — Uniform random.
- `[[SyntheticData]]` — base class.

## Leaves (other)

- `[[DataUpdater]]` — scheduled re-fetch loop.
- `[[DataFetcher]]` (PRO only) — async fetching, used with live portfolios.
- `[[saver]]` — persistence helpers.
- `[[decorators]]` — `@symbol_dict`, `@hybrid_method` for subclass
  configuration.

## Pattern

```python
data = vbt.BinanceData.fetch(
    ["BTCUSDT", "ETHUSDT"],
    start="2024-01-01",
    end="2024-12-31",
    timeframe="1h",
)
close = data.close  # DataFrame, columns = symbols
data.save("cache.parquet")  # Roundtrip via ParquetData
```

## See also

- Canonical wiki § Data:
  `hyperfrequency/docs/deep-tool-wiki/vectorbtpro/wiki.md#detailed-usage-guide`
- RefGraph: 20+ `*Data` classes cluster — the largest single subsystem.
