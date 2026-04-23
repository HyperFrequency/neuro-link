---
title: Execution — engine, matching, emulation, algorithms
parent: nautilus-trader/index
---

# Execution (Python + Rust, dual-exposed)

The execution layer handles the full order lifecycle: from `submit_order`
through matching, through fills, into `Position` updates. The same
primitives back both simulated (backtest) and real (live) execution.

## Leaves

- **ExecutionEngine** (`py+rust`) — routes `SubmitOrder` / `ModifyOrder` /
  `CancelOrder` commands to the right `ExecutionClient`. Generates
  `OrderSubmitted` event before the command leaves the process; receives
  venue responses and publishes `OrderAccepted` / `OrderRejected` /
  `OrderFilled`.
- **MatchingEngine** (`py+rust`) — the L2/L3 book matcher. Used by
  `SimulatedExchange` in backtest and, under `SandboxExecutionClient`,
  in paper trading. Supports FIFO, Pro-Rata, and custom match models.
- **MatchingCore** (`py+rust`) — the order-type state machine underlying
  the matcher. Handles `GTC` / `GTD` / `IOC` / `FOK` semantics, OCO
  linking, bracket parent/child order relationships.
- **OrderEmulator** (`py+rust`) — client-side emulation for order types
  that a venue does not support natively (typical for STOP, MIT, TRAILING
  on crypto spot venues). When the trigger fires, emulator submits the
  underlying MARKET or LIMIT to the venue.
- **OrderManager** (`py+rust`) — per-strategy tracking of
  submitted / working / closed orders. Provides `Strategy.orders` /
  `.orders_working` / `.orders_closed`.
- **Algorithms** (`py+rust`) — `ExecAlgorithm` base + built-in `TWAP`.
  Spawn child orders over a schedule; link as contingent orders.
- **Reports** (`py+rust`) — `FillReport`, `OrderStatusReport`,
  `PositionStatusReport`, `ExecutionMassStatus`, `OrderSnapshot`,
  `PositionSnapshot`. Used in reconciliation.
- **Reconciliation** (`py+rust`) — at live startup, compare internal cache
  state vs. venue mass status; emit synthetic events to close any gaps.

## Pitfalls

- `OrderEmulator` runs client-side. If the process crashes between trigger
  fire and child submission, the emulated order is lost. For critical
  stops, prefer venue-native where available.
- `MatchingCore` OCO behaviour requires explicit `contingency_type=OCO`
  on the `OrderList`; otherwise siblings are independent.

## Cross-links

- Rust source: `crates/execution/`
- Canonical wiki §"`ExecutionEngine`" and §"Order lifecycle deep dive"
- Execution algorithms: §"Execution algorithms"
