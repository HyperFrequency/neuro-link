---
title: vectorbtpro — overview
parent: [[index]]
tool: vectorbtpro
last_updated: 2026-04-20
---

# vectorbtpro — overview

vectorbtpro (VectorBT® PRO) is a high-performance Python backtesting and
quantitative research library that treats every strategy configuration as a
column in a 2D array, enabling thousands of parameter combinations to be
evaluated simultaneously through vectorized NumPy and Numba JIT operations.
The central abstraction is the [[Portfolio]] object, constructed via
`Portfolio.from_signals` / `from_orders` / `from_order_func` / `from_holding`;
these class methods delegate to Numba kernels (`simulate_nb` in
`portfolio/nb.py`) that iterate bar-by-bar across every column in one
compiled pass. Pandas integration is handled via the `ArrayWrapper` +
`.vbt` accessor pattern so that `pf.returns`, `rsi.rsi_above(70)`, and
`mask.vbt.signals.generate_exits(...)` all return properly labelled
`Series`/`DataFrame` objects. vectorbtpro extends the Apache-licensed
public `vectorbt` with PRO-only capabilities: `PortfolioOptimizer`,
`Splitter`/`CVSplitter`, chunked execution, extended `Data` sources
(Polygon, TV, HDF, Parquet, DuckDB, SQL, ArcticDB), Signal expression
DSLs, and an MCP server (`vectorbtpro.mcp_server`) that exposes the
knowledge base to LLM tooling. The HyperFrequency fork pins upstream
PRO commits and patches for compatibility with the HF Nautilus / Optuna
/ MLflow stack. Full canonical reference:
`hyperfrequency/docs/deep-tool-wiki/vectorbtpro/wiki.md` (2,300+ lines
with API tables, Numba kernel walkthroughs, RefGraph visualizations,
and a Pine → vectorbtpro translator cheat sheet).
