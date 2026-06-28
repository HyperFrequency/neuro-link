---
version: 1
auto_update_interval_days: 14
tools: []
---

# adjacent-tools-code-docs Configuration

Upstream tools and libraries your agents use frequently. Keeps documentation updated in `08-code-docs/common-tools/`.

## Registered Tools

| Tool | Repo URL | Context7 Name | Doc URL | Status | Last Updated |
|------|----------|---------------|---------|--------|--------------|
| nautilus-trader | nautechsystems/nautilus_trader | nautilus-trader | docs.nautilustrader.io | active | — |
| vectorbtpro | polakowo/vectorbt.pro | vectorbt | vectorbt.pro/docs | active | — |
| optuna | optuna/optuna | optuna | optuna.readthedocs.io | active | — |
| mlflow | mlflow/mlflow | mlflow | mlflow.org/docs | active | — |
| qlib | microsoft/qlib | qlib | qlib.readthedocs.io | active | — |
| featuretools | alteryx/featuretools | featuretools | featuretools.alteryx.com | active | — |
| tardis-python | tardis-dev/tardis-python | — | docs.tardis.dev | active | — |

## How It Works

The `neuro-scan` skill checks these tools at `auto_update_interval_days` intervals:
1. Fetch latest release/commit from the repo
2. Compare against last indexed version
3. If changed: re-ingest docs → update `08-code-docs/common-tools/<tool>/`
4. Propagate staleness to any wiki pages that reference this tool

This extends the deep-tool-wiki pattern to non-forked upstream tools.
