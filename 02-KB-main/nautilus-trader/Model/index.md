---
title: Model — domain types (value types, instruments, data, orders, events)
parent: nautilus-trader/index
---

# Model (Python + Rust, dual-exposed)

The domain model is invariant across all three runtime paths. Every Model
type is implemented in Rust (`crates/model`) and surfaced to Python via
PyO3. The Python-side Cython wrappers (v1) are thin; the underlying memory
is always Rust-owned.

## Leaves

- **Value types** — `Price`, `Quantity`, `Money`, `Currency`, `UUID4`.
  128-bit fixed-point on Linux/macOS (16 digits) / 64-bit (9 digits) on
  Windows. Compare as objects, not as Python floats.
- **Identifiers** — `InstrumentId` (= `Symbol.Venue`), `ClientOrderId`,
  `VenueOrderId`, `StrategyId`, `TraderId`, `AccountId`, `ComponentId`,
  `ExecAlgorithmId`, `OrderListId`, `TradeId`, `PositionId`.
- **Instruments** — 14 classes: `CurrencyPair`, `CryptoPerpetual`,
  `CryptoFuture`, `CryptoOption`, `FuturesContract`, `FuturesSpread`,
  `OptionContract`, `OptionSpread`, `Equity`, `Cfd`, `Commodity`,
  `BettingInstrument`, `BinaryOption`, `IndexInstrument`,
  `PerpetualContract`, `SyntheticInstrument`, `TokenizedAsset`.
- **Data** — `QuoteTick`, `TradeTick`, `Bar`, `BarType`, `BarSpecification`,
  `OrderBookDelta(s)`, `OrderBookDepth10`, `MarkPriceUpdate`,
  `IndexPriceUpdate`, `FundingRateUpdate`, `InstrumentStatus`,
  `InstrumentClose`, `CustomData`.
- **Orders** — `MarketOrder`, `LimitOrder`, `StopMarketOrder`,
  `StopLimitOrder`, `MarketIfTouchedOrder`, `LimitIfTouchedOrder`,
  `MarketToLimitOrder`, `TrailingStopMarketOrder`, `TrailingStopLimitOrder`.
  Constructed via `OrderFactory`; lifecycle managed by `OrderManager`.
- **Events** — `OrderSubmitted`, `OrderAccepted`, `OrderFilled`,
  `OrderCanceled`, `OrderExpired`, `OrderRejected`, `OrderDenied`,
  `OrderTriggered`, `OrderModifyRejected`, `OrderCancelRejected`,
  `OrderPendingCancel`, `OrderPendingUpdate`, `OrderEmulated`,
  `OrderReleased`, `OrderUpdated`, `PositionOpened`, `PositionChanged`,
  `PositionClosed`, `PositionAdjusted`, `AccountState`.
- **OrderBook** — `OrderBook`, `BookLevel`, `BookOrder`, `OwnOrderBook`.
  `BookType` = `L1` / `L2` / `L3`.
- **Greeks** — `OptionGreeks`, `OptionChainSlice`, `OptionStrikeData`,
  `StrikeRange`, `GreeksConvention` enum.
- **Enums** — `OrderSide`, `OrderType`, `TimeInForce`, `OmsType`,
  `PositionSide`, `PriceType`, `BookType`, `BarAggregation`, and ~20 more.
- **DeFi (Rust-only)** — `Block`, `PoolSwap`, `ChainId`. Requires the
  `defi` Cargo feature flag; implies `high-precision`.

## Pitfalls

- Comparing `Price("0.1")` to the Python float `0.1` → unreliable. Use
  `price.as_double()` or compare `Price` objects directly.
- Accessing PyO3 attributes in tight loops crosses the FFI boundary each
  time. Cache `price.as_double()` in a local variable inside strategy
  callbacks.
- `high-precision` must be consistent between Python wheels and any
  pure-Rust downstream crate consuming Nautilus types.

## Cross-links

- Canonical wiki §"Core Concepts"
- Rust source: `crates/model/src/{objects,identifiers,instruments,data,orders,events,orderbook}`
- PyO3 stub: `nautilus_trader/core/nautilus_pyo3.pyi` (≈10.4k lines)
