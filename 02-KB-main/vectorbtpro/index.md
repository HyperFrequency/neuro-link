---
title: vectorbtpro — llm-wiki navigation
tool: vectorbtpro
upstream: polakowo/vectorbt (public) + polakowo/vectorbt.pro (paid, private)
fork: HyperFrequency/vectorbt.pro
canonical_wiki: ../../../hyperfrequency/docs/deep-tool-wiki/vectorbtpro/wiki.md
last_updated: 2026-04-20
---

# vectorbtpro — llm-wiki

Tree-navigable offline context for vectorbtpro. This mirrors the sitemap
at the top of the canonical `wiki.md`; each leaf links to a per-page
deep-content stub.

## Sitemap

```
vectorbtpro/
├── Portfolio/
│   ├── [[Portfolio/from_signals]]       — signal-driven, fills at signal-bar CLOSE by default
│   ├── [[Portfolio/from_orders]]        — order-driven, explicit price/size per order
│   ├── [[Portfolio/from_order_func]]    — callable-per-bar, path-dependent state machine
│   └── [[Portfolio/from_holding]]       — buy-and-hold reference
├── Indicators/
│   ├── [[Indicators/IndicatorFactory]]  — from_expr, from_apply_func, @p_/@in_/@out_ prefixes
│   ├── [[Indicators/RSI]]               — Wilder-smoothed, matches Pine ta.rsi 1:1
│   ├── [[Indicators/BBANDS]]            — SMA basis; ddof=0 via std kwarg override
│   ├── [[Indicators/ATR]]               — Wilder-smoothed
│   ├── [[Indicators/MACD]]              — EMA-of-EMA; pass adjust=False for Pine parity
│   └── [[Indicators/NB-level]]          — bbands_1d_nb / atr_nb / rsi_nb with default adjust=False/ddof=0
├── Data/
│   ├── [[Data/BinanceData]]
│   ├── [[Data/CCXTData]]
│   ├── [[Data/YFData]]
│   ├── [[Data/PolygonData]]
│   └── [[Data/Custom]]                  — subclass Data for proprietary feeds
├── Signals/
│   ├── [[Signals/Generators]]           — vbt.SIG, vbt.STX, vbt.OHLCSTX
│   ├── [[Signals/crossed_above_below]]  — predicate helpers
│   └── [[Signals/clean]]                — dedupe / reset / chain
├── Optimization/
│   ├── [[Optimization/PortfolioOptimizer]]
│   ├── [[Optimization/Splitter]]
│   ├── [[Optimization/CVSplitter]]
│   └── [[Optimization/Walk-forward]]
├── Analysis/
│   ├── [[Analysis/stats]]               — Portfolio.stats()
│   ├── [[Analysis/plot]]                — Portfolio.plot()
│   ├── [[Analysis/trades]]              — pf.trades.readable, pf.entries / exits
│   └── [[Analysis/tearsheet]]           — quantstats-style HTML export
├── IO/
│   ├── [[IO/Parquet]]
│   ├── [[IO/HDF5]]
│   ├── [[IO/Pickle]]
│   └── [[IO/Database]]                  — DuckDB / Postgres
└── MCP/
    ├── [[MCP/mcp_server]]               — vectorbtpro.mcp_server stdio MCP
    ├── [[MCP/knowledge]]                — knowledge subpackage (AssetFunc / search corpus)
    └── [[MCP/chat]]                     — CLI chat commands
```

→ **Canonical wiki body:** `hyperfrequency/docs/deep-tool-wiki/vectorbtpro/wiki.md`

## Pitfalls (consolidated)

See `[[pitfalls]]` for the full per-leaf pitfall list. Top recurring ones:

- **Fill-timing drift.** `Portfolio.from_signals` defaults to signal-bar
  CLOSE. Pine `process_orders_on_close=false` is next-bar OPEN. Use
  `price=open.shift(-1)` to match Pine, or document the residual bias.
- **Sizing idiom.** Pine `strategy.percent_of_equity=100` →
  `size=1.0, size_type='percent'`. Never `np.inf + size_type='percent'`.
- **`.ewm()` `adjust` default mismatch.** Pandas / vbt.MACD default
  `adjust=True`; Pine `ta.ema` is `adjust=False`. Override explicitly.
- **ddof mismatch.** `ta.stdev` is population (ddof=0); pandas default
  is sample (ddof=1). For BBANDS-parity use `ddof=0`.
- **Wilder vs EMA.** `vbt.RSI` / `vbt.ATR` use Wilder (ewm alpha=1/N),
  matching Pine. `vbt.MACD` uses EMA (ewm span=N) — different.
- **Double-shifted crossover** with pre-shifted bands. If `upper_prev =
  upper.shift(1)`, do not shift `upper_prev` again inside the crossover
  predicate.

## Status

- [x] `index.md` (this page) — sitemap populated
- [ ] `overview.md` — pending
- [ ] `Portfolio/`, `Indicators/`, `Data/`, `Signals/`, `Optimization/`,
      `Analysis/`, `IO/`, `MCP/` — leaf pages pending
- [ ] `pitfalls.md` — consolidate the per-leaf pitfalls into one page

## See also

- Canonical wiki: `../../hyperfrequency/docs/deep-tool-wiki/vectorbtpro/wiki.md`
- InfraNodus graph: `deep-tool-wiki-vectorbtpro`
- MCP server: registered as `pinelsp` in container `.claude.json`;
  activate locally via `python -m vectorbtpro.mcp_server`
- Live pvt URL (rotates): `~/.claude/skills/vectorbt/scripts/get-pvt-url.sh`
