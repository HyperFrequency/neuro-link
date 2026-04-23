---
title: Optimization subsystem
parent: [[../index]]
tool: vectorbtpro
subsystem: optimization
last_updated: 2026-04-20
---

# Optimization subsystem

Splitter / walk-forward / portfolio-weight optimization. These are all
PRO-only capabilities (public vectorbt has `rolling_split` but not
`Splitter` or `PortfolioOptimizer`).

## Leaves

- `[[Splitter]]` — unified cross-validation / walk-forward primitive.
  Single class supporting: fixed-window, expanding, rolling, train/test,
  train/valid/test, grouped, and arbitrary user-defined splits.
  Methods: `.from_rolling(...)`, `.from_expanding(...)`, `.from_ranges(...)`,
  `.apply(func, ...)`, `.stats()`.
- `[[CVSplitter]]` — cross-validation subclass with explicit fold
  semantics.
- `[[PortfolioOptimizer]]` — weight-based position sizing. Accepts
  expected-return estimates or signal strengths; outputs optimal weights
  subject to constraints. Integrates with `from_orders` via fractional
  sizes.
- `[[Pipeline]]` — composable sweep + simulation + analysis pipeline.
  Optuna / Ray Tune integration points for Bayesian hyperopt.
- `[[Parameterized]]` — base class for every parameter-aware object.
  Enables `.param_config`, sweep inspection, and reproducibility.
- `[[Param]]` — `vbt.Param([10, 20, 50], name='window')`. Marks a value
  as a sweep dimension; absorbed by `reshape_fns.broadcast()` during
  indicator construction.
- `[[Robustness]]` — robustness checks (noise injection, bootstrap
  confidence intervals, regime stability).
- `[[Chunker]]` — `chunked=True` kwarg. Splits large parameter sweeps
  into memory-bounded chunks executed sequentially or via Ray/Dask
  executor.

## Walk-forward pattern

```python
splitter = vbt.Splitter.from_rolling(
    close.index,
    length=252,      # 1 year train
    offset=63,       # 3 month step
    set_labels=["train", "test"],
    split=0.7,       # 70/30 train/test
)

def objective(close_train, close_test):
    # Fit on train, evaluate on test
    rsi = vbt.RSI.run(close_train, window=14)
    entries = rsi.rsi_crossed_below(30)
    exits = rsi.rsi_crossed_above(70)
    pf = vbt.Portfolio.from_signals(close_test, entries, exits)
    return pf.sharpe_ratio()

results = splitter.apply(objective)
```

## See also

- Canonical wiki § Optimization / Advanced Patterns:
  `hyperfrequency/docs/deep-tool-wiki/vectorbtpro/wiki.md#advanced-patterns`
- [[DeepTools/optuna]] — hyperparameter search integration.
- [[DeepTools/mlflow]] — experiment tracking for sweeps.
