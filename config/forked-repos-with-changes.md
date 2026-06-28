---
version: 1
auto_diff_interval_days: 7
forks: []
---

# forked-repos-with-changes Configuration

Tracks HyperFrequency forks and their divergence from upstream. Generates comprehensive fork-specific wiki pages.

## Registered Forks

| Fork | Upstream | Divergence | Status | Last Diff |
|------|----------|-----------|--------|-----------|
| HyperFrequency/optuna | optuna/optuna | — | active | — |
| HyperFrequency/nautilus-trader | nautechsystems/nautilus_trader | — | active | — |
| HyperFrequency/vectorbtpro | polakowo/vectorbt.pro | — | active | — |
| HyperFrequency/mlflow | mlflow/mlflow | — | active | — |
| HyperFrequency/h2o-3 | h2oai/h2o-3 | — | active | — |
| HyperFrequency/tardis-python | tardis-dev/tardis-python | — | active | — |
| HyperFrequency/qlib | microsoft/qlib | — | active | — |
| HyperFrequency/featuretools | alteryx/featuretools | — | active | — |
| HyperFrequency/tsfel | fraunhoferportugal/tsfel | — | active | — |
| HyperFrequency/hftbacktest | nkaz001/hftbacktest | — | active | — |
| HyperFrequency/hyper-stats | — | — | active | — |
| HyperFrequency/xfeat | pfnet-research/xfeat | — | active | — |
| HyperFrequency/alpha-factory | — | — | active | — |

## How It Works

At `auto_diff_interval_days` intervals:
1. Fetch latest upstream commit for each fork
2. Compute diff between fork HEAD and upstream HEAD
3. Update `Divergence` column with summary (e.g., "+45 files, -12 files, 3 new modules")
4. Generate fork-specific documentation in `08-code-docs/my-forks/<tool>/`
5. Documentation covers: what changed, why (from commit messages), fork-specific API differences

This is the evolution of the deep-tool-wiki system — instead of just documenting tools, it tracks the semantic understanding gap between upstream and fork.
