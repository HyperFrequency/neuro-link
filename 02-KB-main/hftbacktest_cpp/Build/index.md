---
title: hftbacktest_cpp — Build
parent: [[../index]]
subsystem: Build
last_updated: 2026-04-20
---

# Build

CMake-driven build system. Single source file of truth is `CMakeLists.txt` at
the repo root — 10 lines total.

## Leaves

- [[CMakeLists]] — `cmake_minimum_required >= 3.10`, C++17 standard required,
  `-O3` optimization, single `add_executable(main ...)` target, no
  `find_package` calls, no third-party dependency resolution.

## Build commands

```bash
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build -j
./build/main     # runs against hard-coded CSV + instrument_id in main.cpp
```

## Notes

- No library target — consumers can't link against it without editing the
  `CMakeLists.txt` to add `add_library(hftbacktest_cpp STATIC ...)`.
- No install rules, no CPack, no test target.
- The executable is named `main` (a dubious default — a real release should
  rename to `hftbacktest_cpp` or `hftb_cpp`).

## See also

- Canonical wiki section: `wiki.md` → "Build system — minimal CMake"
- Pitfalls: no `-march=native`, no LTO — see [[../pitfalls]]
