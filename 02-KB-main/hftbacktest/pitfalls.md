---
title: hftbacktest — consolidated pitfalls
parent: [[index]]
tool: hftbacktest
last_updated: 2026-04-20
---

# hftbacktest — consolidated pitfalls

One-page catalogue of recurring footguns, pulled from the canonical wiki
`Limitations and Pitfalls` section, the upstream docs site, and the
HyperFrequency fork's field notes. Each item names the root cause, the
symptom, and the resolution.

## Queue model choice

- **`RiskAdverseQueueModel` with no trade data.** Without `TRADE_EVENT`
  rows the queue advances never, so resting orders never fill. Symptom:
  zero fills + empty trade log. Fix: include trade ticks in the NPZ; or
  switch to `ProbQueueModel` which advances on depth decreases too.
- **`PowerProbQueueModel` exponent untuned.** Default `n = 3.0` may
  over- or under-predict fills by 2-5x on venues with atypical
  cancellation patterns. Symptom: sim vs live fill rate drift. Fix:
  calibrate by replaying a live day through both; tune `n` until
  simulated fill count matches within ±10% at matched price levels.
- **Using `ProbQueueModel` on L3 data.** L3 MBO feeds carry exact
  per-order queue positions; the probabilistic model discards that
  information. Fix: use `L3FIFOQueueModel` with `L3Local` + `L3NoPartialFillExchange`.

## MBO vs MBP data selection

- **L2 (MBP) vs L3 (MBO) mismatch.** Configuring `L3FIFOQueueModel`
  with L2 data will panic on the first `add_order` call — L2 feeds
  don't carry `order_id`. Fix: match queue model to feed type (L2 →
  Prob/RiskAdverse; L3 → L3FIFO).
- **ROI bounds too narrow.** `ROIVectorMarketDepthBacktest(assets,
  roi_lb, roi_ub)` silently drops depth updates outside the tick
  window. Symptom: stale "outside" levels + missed fills when price
  walks. Fix: size ROI to cover your order band *plus* expected price
  drift over the backtest horizon (e.g., `±3σ * sqrt(horizon) / tick_size`).
- **Missing initial snapshot.** Starting with an empty book means the
  first N events populate the book rather than execute against it.
  Symptom: low fill count in the first minutes. Fix: supply
  `.initial_snapshot('eod-snapshot.npz')` or discard the first N
  events' worth of metrics.

## Latency calibration

- **`ConstantLatency` for production-grade simulation.** Constants miss
  intraday latency spikes at open/close and during volatility events.
  Symptom: backtest looks great, live underperforms. Fix: record live
  `req_ts/exch_ts/resp_ts` tuples into an NPZ and use
  `IntpOrderLatency([npz_files])`.
- **Collection-site vs deployment-site skew.** If you collected data
  in one datacenter but deploy in another, `local_ts` reflects the
  collector's latency. Fix: run `FeedLatencyAdjustment` with an offset
  measured as the round-trip difference between the two sites.
- **Forward vs backward latency adjustment sign confusion.** A
  *positive* offset simulates a *worse* (further) co-location than the
  collector. Symptom: fills arrive before your orders in the timeline
  when the sign is flipped. Fix: verify direction with a known
  baseline day.

## Numba type-annotation requirements

- **Missing `@njit(cache=True)` on strategy.** Without `@njit`, every
  call into Rust pays Python dispatch overhead; backtest runs 10-50x
  slower. Fix: annotate the strategy function; ensure all locals are
  Numba-typed (int64, float64, structured arrays).
- **OrderDict iteration with Python `for`.** Numba does not support
  Python `dict` iteration over the Rust-backed `OrderDict`. Fix: use
  `orders = hbt.orders(asset_no); vals = orders.values(); while
  vals.has_next(): order = vals.get()`.
- **Storing Rust-pointer objects in Python lists.** Inside `@njit`,
  you cannot put `hbt` or `depth` into generic Python containers. Fix:
  keep them as local variables or Numba-typed tuples.
- **Forgetting `hbt.close()`.** Rust-heap backtest objects are not
  garbage-collected by Python. Symptom: leaked memory across
  parameter sweeps. Fix: always call `hbt.close()` at the end of the
  strategy, ideally in a `finally`.

## Iceoryx2 shared-memory lifecycle (live)

- **Services not started before LiveBot init.** The Iceoryx2 roudi
  daemon must be running and the exchange-connector service must be
  advertised before the bot's subscriber attaches. Symptom:
  `SubscriberCreateError::ServiceDoesNotExist`. Fix: launch the
  connector first; verify with `iox2 services list`.
- **Stale shared-memory segments from previous runs.** Unclean shutdown
  leaves segments behind; next connector start may fail with
  `ServiceAlreadyExists`. Fix: `iox2 services cleanup` or delete
  `/dev/shm/iox2_*`.
- **Message-type mismatch across crate versions.** Both processes must
  link the same `hftbacktest-live` version; a mismatched serde shape
  silently corrupts orders. Fix: version-pin connector and bot in
  workspace `Cargo.toml`.

## Rust feature-flag choices

- **`--features unstable_l3` not enabled when using L3 API.** The
  `hftbacktest` crate gates MBO simulation behind a cargo feature.
  Symptom: `L3FIFOQueueModel` not found at compile time. Fix:
  `maturin develop --release --features unstable_l3`.
- **`default-features = false` in consumer crate.** Strips the
  standard depth backends. Symptom: `HashMapMarketDepth` missing. Fix:
  re-enable `depth` feature or drop the override.
- **Mixing `hftbacktest` crate versions across workspace members.**
  Breaks trait object layouts; produces confusing "trait not
  implemented" errors. Fix: declare the dep once at workspace root
  and use `workspace = true` in members.

## Order management

- **Not calling `hbt.clear_inactive_orders(asset_no)` each tick.** The
  OrderDict grows unboundedly with filled/canceled/rejected entries.
  Symptom: iteration time grows linearly through the day; OOM on
  multi-day runs. Fix: call at the top of every loop iteration.
- **Reusing `order_id = price_tick` across grids at the same price.**
  The "one order per tick" idiom breaks when multiple strategies grid
  at the same level. Fix: `order_id = price_tick * 10 + side` or use
  a monotonically-incrementing counter.
- **Submitting without `GTX` for market making.** A standard `GTC`
  can cross the book and pay taker fees. Symptom: unexpected maker→
  taker conversions in fee accounting. Fix: always use `GTX`
  (post-only) for maker quotes.
- **Expecting `wait_order_response(timeout_ns=0)` to be
  non-blocking.** Zero-timeout is immediate-fail, not poll. Symptom:
  strategy never sees acks. Fix: use a realistic timeout like 1 ms
  (`1_000_000 ns`) or loop with `elapse` + status checks.
