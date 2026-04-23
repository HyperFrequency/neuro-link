---
title: Live — live node, async queues, reconciliation
parent: nautilus-trader/index
---

# Live (Python + Rust, dual-exposed)

- **LiveNode (Rust)** — pure-Rust live runtime binary. `LiveNode::builder()`
  pattern for data + exec clients; `node.run().await?`.
- **TradingNode (Python)** — Python wrapper on top of `NautilusKernel` +
  `LiveDataEngine` / `LiveExecutionEngine` / `LiveRiskEngine`. asyncio
  queues drain WebSocket messages from tokio tasks.
- **Reconciliation** — at startup, `LiveExecutionEngine` compares internal
  cache state vs. venue mass status; emits synthetic events to close gaps.

## Lifecycle (Python)

```python
async with TradingNode(config) as node:
    await node.run_async()
```

## Pitfalls

- Blocking calls (`requests.get`, `time.sleep`, heavy computation) inside
  strategy callbacks stall the asyncio event loop. Use
  `await loop.run_in_executor(None, fn)` or `self.create_task(...)`.
- Rust `LiveNode` requires the `nautilus-live` crate and at least one
  adapter (e.g., `nautilus-okx`) at compile time.

## Cross-links

- Canonical wiki §"`TradingNode`" and §"Detailed Usage Guide 5. Live trading on Binance"
- Rust source: `crates/live/`
