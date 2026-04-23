---
title: FFI — Cython layer + PyO3 boundary
parent: nautilus-trader/index
---

# FFI (cross-cutting)

Two separate FFI layers connect Rust to Python.

## Cython layer (v1 legacy)

- Lives in `nautilus_trader/**/*.pxd` and `*.pyx`.
- Uses `nautilus_trader/core/rust/*.pxd` declarations backed by a
  cbindgen-generated `model.h`.
- Each Rust struct has a hand-written Cython wrapper that exposes a
  Python-friendly class.
- Still the default runtime today for most user-facing classes.

## PyO3 boundary (v2 path)

- Lives in `crates/pyo3/src/lib.rs` (`#[pymodule]`) plus per-crate
  `src/python/*.rs` files with `#[pyclass]` / `#[pyfunction]` impls.
- Exposed to Python as `nautilus_trader.core.nautilus_pyo3`.
- Canonical inventory: `nautilus_trader/core/nautilus_pyo3.pyi` (≈10.4k
  lines of type stubs).
- Memory model: Rust owns; Python holds `Py<T>`. Every attribute access
  is one FFI crossing — cache derived values in strategy hot loops.

## Cross-links

- Refgraph: `../../docs/deep-tool-wiki/nautilus-trader/assets/refgraph-pyo3-boundary.mmd`
- Canonical wiki §"FFI" implicit in §"Installation" + §"Cython performance optimizations"
