---
title: Analysis — subsystem index
parent: [[../index]]
tool: hftbacktest
last_updated: 2026-04-20
---

# Analysis/

Post-run performance metrics. Python-only — `hftbacktest.stats.Stats`
consumes recorded `StateValues` snapshots and computes standard
quant tearsheet numbers.

## Leaves

- [[Stats]] — wraps a DataFrame of recorded state; exposes `Ret`,
  `AnnualRet`, `SR` (Sharpe), `Sortino`, `MaxDrawdown`,
  `ReturnOverMDD`, `ReturnOverTrade`, `NumberOfTrades`,
  `DailyNumberOfTrades`, `TradingVolume`, `DailyTradingVolume`,
  `TradingValue`, `MaxPositionValue`, `MeanPositionValue`,
  `MedianPositionValue`, `MaxLeverage`.
- [[EquityCurve]] — how to go from `recorder.df` to an equity-vs-time
  array for plotting or Sortino-over-window calculations.

## Canonical wiki sections

`wiki.md#BotStatePnL`, `wiki.md#Detailed-Usage-Guide` (Analysis
subsection).

## Minimal usage

```python
from hftbacktest.stats import Stats

stats = Stats(recorder.df, risk_free=0.0)
print(stats.sr(), stats.sortino(), stats.max_drawdown())
print(stats.to_dataframe())  # full metric table
```
