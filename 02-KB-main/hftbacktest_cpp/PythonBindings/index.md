---
title: hftbacktest_cpp — PythonBindings
parent: [[../index]]
subsystem: PythonBindings
status: planned
last_updated: 2026-04-20
---

# PythonBindings

**Status: not yet implemented.** The README lists "Python bindings for strategy
research (coming)" under Features, but no `python/` directory, no `pyproject.toml`,
no `setup.py`, and no pybind11 / nanobind references exist in the 2026-04-20 HEAD.

## Leaves

- [[roadmap]] — speculative plan for the likely binding path.

## Most likely future shape

- **Binding framework:** `pybind11` (header-only, CMake-friendly, de-facto
  standard) or `nanobind` (pybind11 successor; smaller code, faster compile,
  Python 3.8+).
- **Target surface:** `Engine` (run / mkt_buy / mkt_sell), `Callbacks` via a
  py-subclass trampoline so a Python subclass of `Callbacks` can override
  `trade`, and the four POD structs (`Event`, `Order`, `Limit`, `Trade`).
- **Build:** CMake + `pybind11_add_module(_hftbacktest_cpp ...)` or a
  `scikit-build-core` config to produce a wheel.

## Contrast with Rust sibling

The Rust `hftbacktest` exposes a `py-hftbacktest` crate via PyO3 and packages
it with **maturin 0.27.2** into a `_hftbacktest` wheel. Python strategies
there run as Numba `@njit` functions that call directly into Rust through raw
`usize` pointers. See [[../../hftbacktest/PyO3/index]].

The C++ port would **not** be able to use Numba. The closest equivalent would
be:

- Wrap `Engine` with pybind11 `py::class_<Engine>`.
- Provide a trampoline `PyCallbacks : public Callbacks { trade() override {
  PYBIND11_OVERRIDE(...); }};` so Python subclasses can override virtuals.
- Accept the GIL cost per callback — Python strategies would be slower than
  the Rust+Numba path.

## How to drive today (workaround)

```python
import subprocess
result = subprocess.run(
    ["./build/main"],
    capture_output=True, text=True,
    cwd="path/to/hftbacktest_cpp"
)
# parse result.stdout
```

The engine prints trade prices to stdout in the reference `main.cpp`. A proper
pipeline would require modifying `main.cpp` to emit structured output (JSON,
CSV, or protobuf).

## See also

- Canonical wiki section: `wiki.md` → "Python bindings — "coming""
- Rust-side bindings: [[../../hftbacktest/PyO3/index]]
