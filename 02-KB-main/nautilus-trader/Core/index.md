---
title: Core — kernel / nodes / clock / environment
parent: nautilus-trader/index
---

# Core (Python + Rust)

The kernel is the DI container. It wires MessageBus, Cache, Clock, DataEngine,
ExecutionEngine, RiskEngine, and Portfolio in strict dependency order, then
hands references to every actor and strategy.

## Leaves

- **NautilusKernel** (`py+rust`) — shared by all three paths. Constructor
  is driven by `NautilusKernelConfig`. See canonical wiki §"Mental model".
- **TradingNode** (`py+rust`) — live runtime. Wraps the kernel and adds
  `LiveDataEngine`, `LiveExecutionEngine`, `LiveRiskEngine`, plus asyncio
  queues and OS signal handling. Supports `async with node: await
  node.run_async()`.
- **BacktestNode** (`py+rust`) — high-level. Accepts
  `list[BacktestRunConfig]`. Drives `BacktestEngine` instances and
  optionally streams data from `ParquetDataCatalog`.
- **BacktestEngine** (`py+rust`) — low-level. Manually add venues,
  instruments, data, and strategies; call `.run()`.
- **Clock** (`py+rust`) — `TestClock` (synthetic) / `LiveClock` (system).
  Strategies only interact with the interface — same code in both paths.
- **Environment** (`py+rust`) — enum `Backtest`, `Sandbox`, `Live`.
- **FiniteStateMachine** (`rust`) — core FSM guard; prevents invalid
  state transitions (e.g., submitting orders while `STOPPED`).

## Cross-links

- Boundary details: `[[FFI/PyO3-boundary]]`
- Backtest vs live: `[[Backtest/BacktestNode]]` vs `[[Live/LiveNode]]`
- Canonical: §"Core Concepts" in `wiki.md`
