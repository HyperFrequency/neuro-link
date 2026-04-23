---
title: QueueModel — subsystem index
parent: [[../index]]
tool: hftbacktest
last_updated: 2026-04-20
---

# QueueModel/

Queue-position-aware fill models. The exchange-view processor asks the
model "is this resting order filled yet?" on every depth/trade event;
the model maintains `qty_ahead` and `qty_behind` and returns `true` once
`qty_ahead <= 0`.

## Leaves

- [[ProbQueueModel]] — parametric over a probability function `F`
  (`PowerProbQueueFunc`, `LogProbQueueFunc`, `IdentityProbQueueFunc`);
  attributes depth decreases partially to cancellations ahead of your
  order using `F(qty_ahead / total_qty)`.
- [[RiskAdverseQueueModel]] — conservative; only trades advance
  position (no credit for cancellations).
- [[L3FIFOQueueModel]] — exact per-order FIFO simulation, requires MBO
  feed with `order_id` field populated.

## Canonical wiki sections

`wiki.md#ProbQueueModel`, `wiki.md#LogProbQueueModel`,
`wiki.md#PowerProbQueueModel`, `wiki.md#RiskAdverseQueueModel`,
`wiki.md#Queue-Model-Mathematics`,
`wiki.md#Worked-Example:-Calibrating-Queue-Model-Exponent`.

## Choosing a model

- Maximally conservative baseline (no trade data? use this anyway to
  avoid false fills): `RiskAdverseQueueModel`.
- Default for L2 crypto perpetuals: `PowerProbQueueModel` with
  `.power_prob_queue_model3(3.0)` (i.e., `n = 3`).
- Thin inside, thick deeper books (FX-like): `LogProbQueueModel`.
- Availability of L3 MBO data (Databento, Nasdaq ITCH, Tardis L3):
  `L3FIFOQueueModel` — highest fidelity.
