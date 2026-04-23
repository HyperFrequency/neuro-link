---
title: PyO3 — subsystem index
parent: [[../index]]
tool: hftbacktest
last_updated: 2026-04-20
---

# PyO3/

The glue that crosses the Python↔Rust boundary. Source:
`py-hftbacktest/src/`. Built with `maturin 0.27.2` into a `_hftbacktest`
shared library; the Python package `hftbacktest/` imports from it.

## Leaves

- [[Maturin]] — build toolchain, `maturin develop --release` for
  iteration, `maturin build --release` + `pip install wheel` for
  distribution. Requires `rustc 1.90+`.
- [[PyClasses]] — the `#[pyclass]` and `#[pyfunction]` surface.
  Core entries: `BacktestAsset` (fluent builder), the
  `HashMapMarketDepthBacktest` / `ROIVectorMarketDepthBacktest`
  constructors that return an integer (`usize`) pointer to a
  heap-allocated Rust `Backtest<MD>`.
- [[NumbaSurface]] — Numba-compatible struct-ref types that let
  `@njit` strategies call methods on the Rust objects without
  falling back to the Python interpreter.

## Canonical wiki sections

`wiki.md#Architecture-Deep-Dive` (PyO3 section),
`wiki.md#Installation` (development from source),
assets `refgraph-pyo3-boundary.mmd`.

## Why raw `usize` pointers?

The Rust `Backtest<MD>` has a type parameter the Python API cannot
express. Exposing it as a typed `#[pyclass]` would require monomorphizing
the wrapper for every `MD`/`QM`/`LM`/`FM`/`AT` combination
— thousands of concrete types. Instead, the constructor leaks the
heap-allocated struct as a raw pointer; Python holds the `usize`
value; Numba's extension types provide a dispatch table keyed off the
pointer so method calls land on the correct Rust vtable. The
`build_asset!` macro generates these per-combination dispatchers at
compile time.

## Build metadata

- Maturin version: `0.27.2`
- PyO3: current stable, abi3-py311+ (works against Python 3.11, 3.12, 3.13)
- Minimum Rust: 1.90
- Features: standard depth + latency + queue + fee + exchange models
  enabled by default; `unstable_l3` gates MBO support.
