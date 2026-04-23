---
title: FillModel — subsystem index
parent: [[../index]]
tool: hftbacktest
last_updated: 2026-04-20
---

# FillModel/

hftbacktest does not expose an independent `FillModel` trait — fills are
a joint responsibility of the `ExchangeProcessor` (which handles taker
crossings and order acceptance) and the `QueueModel` (which decides
whether resting maker orders have worked through the queue).

## Leaves

- [[QueueToFill]] — how `ExchangeProcessor::process` delegates the
  maker-fill decision to `QueueModel::is_filled`, and how taker fills
  cross the spread.

## Canonical wiki sections

`wiki.md#Exchange-Models-in-Detail`,
`wiki.md#Queue-Model-Mathematics`.

## Mental model

For a resting BID at `price_tick`, on each market event at that price:

1. If `TRADE_EVENT` and `SELL_EVENT` (sell crossing): `qty_ahead -= trade_qty`.
2. If `DEPTH_EVENT` at same tick and total depth decreased: the queue
   model attributes a fraction `F(x)` of the decrease to cancellations
   ahead of you, reducing `qty_ahead` by `decrease * (1 - F(x))` where
   `x = qty_ahead / total_qty`.
3. When `qty_ahead <= 0`, the order is filled at its resting price.
   `ExchangeProcessor` then emits a local-bound fill event with
   response latency applied by `OrderBus`.

Taker fills (MARKET / IOC / FOK crossing the spread) bypass
`QueueModel` entirely and execute instantly against the book inside
`PartialFillExchange`.
