---
title: MessageBus — pub/sub spine
parent: nautilus-trader/index
---

# MessageBus (Python + Rust, dual-exposed)

Every cross-component piece of information — market data, order events,
timer firings, custom signals — is a message on the bus. Components
subscribe and publish rather than calling each other directly.

## Topics

- `data.quotes.{instrument_id}` — QuoteTick
- `data.trades.{instrument_id}` — TradeTick
- `data.bars.{bar_type}` — Bar
- `data.book.deltas.{instrument_id}` — OrderBookDelta(s)
- `events.order.{order_id}` — OrderFilled / OrderAccepted / ...
- `events.position.{position_id}` — PositionOpened / PositionClosed / ...
- `custom.{topic}` — user-defined
- `system.*` — internal lifecycle

## API shape

```python
self.msgbus.subscribe("data.bars.*", handler=self._on_bar)
self.msgbus.publish("custom.my_signal", payload=my_signal)
```

## Redis backend

`MessageBusConfig(database=DatabaseConfig(type="redis", ...), stream_per_topic=True, autotrim_mins=60)`
streams every `BusMessage` to Redis. External consumers can replay the
stream for live monitoring, post-crash recovery, or cross-process fanout.

## Leaves

- **Overview** — this page
- **Topics** — full topic string reference
- **RedisBackend** — `RedisMessageBusDatabase`

## Cross-links

- Rust source: `crates/common/src/msgbus/`
- Canonical wiki §"`MessageBus`" + §"Message bus pub/sub"
- See also: `[[Cache]]` (observes the same topics)
