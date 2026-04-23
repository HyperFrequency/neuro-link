---
title: Recorder — subsystem index
parent: [[../index]]
tool: hftbacktest
last_updated: 2026-04-20
---

# Recorder/

Periodic snapshotting of per-asset state into numpy buffers for
post-run analysis. Lives in the Python layer
(`python/hftbacktest/recorder.py`), drives into the Rust
`BacktestRecorder` via PyO3.

## Leaves

- [[Recorder]] — base class; call `recorder.record(hbt)` at each
  strategy iteration (or on a coarser schedule) to capture
  `StateValues` + timestamps.
- [[LinearInverseRecord]] — `LinearAssetRecord` for quote-denominated
  PnL, `InverseAssetRecord` for base-denominated. Determines how
  position × (exit − entry) is converted to realized currency.

## Canonical wiki sections

`wiki.md#Recorder`, `wiki.md#BotStatePnL`, `wiki.md#Analysis`.

## Pattern

```python
from hftbacktest.recorder import LinearAssetRecord
rec = LinearAssetRecord(asset_no=0, contract_size=1.0)

while hbt.elapse(10_000_000) == 0:
    # ... strategy logic ...
    rec.record(hbt)

# Post-run
df = rec.to_pandas()  # timestamp, position, balance, fee, trade_qty, …
```
