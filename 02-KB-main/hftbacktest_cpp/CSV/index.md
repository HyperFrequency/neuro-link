---
title: hftbacktest_cpp — CSV
parent: [[../index]]
subsystem: CSV
last_updated: 2026-04-20
---

# CSV

Databento MBO CSV parser. Three free functions in `include/csv.h` +
`src/csv.cpp` (44 LOC). Hand-rolled, no third-party library.

## Leaves

- [[parse_header]] — `const std::vector<std::string> parse_header(const
  std::string& line)` — split first line on commas, return column names.
- [[parse_line]]   — `const std::map<std::string, std::string>
  parse_line(const std::string& line, const std::vector<std::string>&
  header)` — per-row column-name → value map.
- [[encode_event]] — `Event encode_event(const std::map<std::string,
  std::string>& row)` — extract `ts_event` / `action` / `side` / `price` /
  `size` / `order_id` into a typed `Event`.

## Assumed columns

`ts_event`, `action`, `side`, `price`, `size`, `order_id`, `instrument_id`,
`flags` — all expected by name. A Databento schema rename silently breaks
the parser with `std::out_of_range` from `row.at(...)`. See [[../pitfalls]].

## Naïve parsing

`parse_line` splits on every `,` without handling quotes or escapes. Safe for
Databento MBO (no string columns) but unsafe for any feed with quoted text
columns. See [[../pitfalls]].

## Performance

`parse_line` is likely the **hot path** in the backtest — it builds a
`std::map<std::string, std::string>` per row, which means N allocator calls
per row. A vectorized `std::array<std::string_view, N>` indexed by column
position would be 5-10× faster.

## See also

- Canonical wiki section: `wiki.md` → "CSV parsing — hand-rolled, no external
  library"
- Performance discussion: `wiki.md` → "Performance characteristics"
