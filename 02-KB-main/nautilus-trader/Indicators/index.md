---
title: Indicators — ~30 dual-exposed classes
parent: nautilus-trader/index
---

# Indicators (Python + Rust, dual-exposed)

All indicators are implemented in Rust (`crates/indicators`) and surfaced
via PyO3. The Python-side Cython v1 wrappers delegate to the same Rust
math. Strategies register indicators with `register_indicator_for_bars` /
`register_indicator_for_quote_ticks` / `register_indicator_for_trade_ticks`.

## Leaves

- **Averages** (`py+rust`) — `SimpleMovingAverage`,
  `ExponentialMovingAverage`, `HullMovingAverage`,
  `AdaptiveMovingAverage` (Kaufman), `WilderMovingAverage`,
  `DoubleExponentialMovingAverage`, `VariableIndexDynamicAverage` (VIDYA),
  `VolumeWeightedAveragePrice`, `LinearRegression`.
- **Momentum** (`py+rust`) — `RelativeStrengthIndex`, `Stochastics`,
  `RateOfChange`, `ChandeMomentumOscillator`, `Bias`, `EfficiencyRatio`,
  `RelativeVolatilityIndex`.
- **Volatility** (`py+rust`) — `AverageTrueRange`, `BollingerBands`,
  `VolatilityRatio`, `Pressure`.
- **Volume** (`py+rust`) — `VolumeWeightedAveragePrice`,
  `KlingerVolumeOscillator`, `OnBalanceVolume`, `ChaikinMoneyFlow`.
- **Trend** (`py+rust`) — `MovingAverageConvergenceDivergence`,
  `DirectionalMovement` (ADX family), `ArcherMovingAveragesTrends`,
  `AroonOscillator`, `Swings`, `PsychologicalLine`,
  `VerticalHorizontalFilter`.
- **Custom base** (`py+rust`) — subclass `Indicator` (Python) or
  implement the equivalent trait in Rust.

## Pitfalls

- `MovingAverageConvergenceDivergence` (MACD) uses EMA smoothing. Pandas
  default `adjust=True`; Pine `ta.ema` is `adjust=False`. For Pine parity,
  see `[[DeepTools/vectorbtpro]]` pitfalls.
- Indicator state is not reset by `engine.reset()`. `Strategy.on_reset`
  must explicitly call `.reset()` on each registered indicator.
- Warm-up: `MovingAverageConvergenceDivergence` needs
  `slow_period + signal_period` bars before `.initialized == True`.

## Cross-links

- Rust source: `crates/indicators/`
- Python source: `nautilus_trader/indicators/`
- Canonical wiki §"Indicator" and §"Detailed Usage Guide 3. Custom Indicator"
