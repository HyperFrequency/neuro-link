---
title: llm-wiki — offline deep-tool-wiki navigation
layer: kb-root
last_updated: 2026-04-20
---

# llm-wiki — offline deep-tool-wiki

Tree-navigable offline context for every HyperFrequency-indexed tool.

The **canonical wiki body** (1500–2200 lines per tool, full prose + walkthroughs +
dual reasoning ontology) lives at
`/Users/DanBot/hyperfrequency/docs/deep-tool-wiki/<tool>/wiki.md`. This vault
(`02-KB-main/`) is the **tree-navigable companion**: each tool has a subdirectory;
each sitemap leaf has its own short page with API signature(s), minimal example,
and linked pitfalls.

Navigate by tree. Every leaf links back to the canonical wiki section it expands.

## Indexed tools

```
llm-wiki/
├── alpha-factory/          → [[alpha-factory/index]]
├── dask-distributed/       → [[dask-distributed/index]]
├── featuretools/           → [[featuretools/index]]
├── gnu-parallel/           → [[gnu-parallel/index]]
├── h2o-3/                  → [[h2o-3/index]]
├── hftbacktest/            → [[hftbacktest/index]]
├── hftbacktest_cpp/        → [[hftbacktest_cpp/index]]
├── hyper-stats/            → [[hyper-stats/index]]
├── mlflow/                 → [[mlflow/index]]
├── nautilus-admin/         → [[nautilus-admin/index]]
├── nautilus-trader/        → [[nautilus-trader/index]]
├── optuna/                 → [[optuna/index]]
├── pine-script/            → [[pine-script/index]]
├── postgresql-optuna/      → [[postgresql-optuna/index]]
├── qlib/                   → [[qlib/index]]
├── ray-distributed/        → [[ray-distributed/index]]
├── tardis-python/          → [[tardis-python/index]]
├── tsfel/                  → [[tsfel/index]]
├── vectorbtpro/            → [[vectorbtpro/index]]
└── xfeat/                  → [[xfeat/index]]
```

## How the tree is built

Each tool's `index.md` mirrors the sitemap at the top of that tool's canonical
`wiki.md`. The sitemap is derived from the upstream repo's package/module
structure — not from prose. That guarantees the tree reflects the tool's real
shape.

Each leaf page follows this template:

```
# <subsystem>/<leaf>

## Signature
<primary API signature(s)>

## Minimal example
<smallest runnable snippet>

## Pitfalls
- [[../pitfalls#<slug>]]
- ...

## See also
- [[<neighbor-leaf>]]
- [[../../<related-tool>/<related-leaf>]]
- Canonical wiki section: `deep-tool-wiki/<tool>/wiki.md#<anchor>`
```

## Layers

- `index.md` (this file) — vault root; lists every indexed tool
- `<tool>/index.md` — tree nav for that tool (mirrors wiki sitemap)
- `<tool>/overview.md` — one-paragraph "what it is"
- `<tool>/<subsystem>/<leaf>.md` — per-leaf deep content
- `<tool>/pitfalls.md` — consolidated per-tool pitfall list

## Status

- [x] `index.md` (this page) — 2026-04-20
- [x] `vectorbtpro/index.md` — demonstration tool; full tree populated
- [x] `nautilus-trader/index.md` — dual-language (py+rust) tree populated
      (11 subsystem indexes: Core, Model, MessageBus, Cache, Execution,
      Adapters, Backtest, Persistence, Indicators, Live, FFI)
- [x] `pine-script/index.md` — v6 DSL tree populated (9 subsystem
      indexes: Types, Builtins, Strategy, Indicators, Libraries,
      Annotations, Variables-Constants, Operators-Keywords, Control-flow
      + consolidated pitfalls.md)
- [x] `hftbacktest/index.md` — Rust+PyO3+Numba tree populated (11 subsystem
      indexes: Backtest, Depth, Live, Types, Processor, LatencyModel,
      QueueModel, FillModel, PyO3, Recorder, Analysis + overview.md +
      consolidated pitfalls.md; canonical wiki enriched with sitemap-first
      layout + Rust-crate & PyO3-boundary RefGraph `.mmd` assets)
- [x] `hftbacktest_cpp/index.md` — C++17 port; distinct from the Rust
      sibling above. 7 subsystem indexes populated (Build, Types, Book,
      Engine, Callbacks, CSV, PythonBindings (planned)) + overview.md +
      consolidated pitfalls.md catalogue (incl. known upstream bugs in
      `cost_buy`/`cost_sell` iterator, `Engine::run` market-op matching
      TODO, and callback field-name mix-ups); canonical wiki includes
      sitemap-first layout + cpp-headers & templated-classes RefGraph
      `.mmd` assets
- [ ] Remaining 13 tools — scaffolds pending

Retrofit path: run `~/.claude/scripts/deep-tool-wiki.py sitemap --tool <T>`
(once written) to generate `02-KB-main/<T>/index.md` from that tool's
canonical `wiki.md` sitemap header. Leaf pages are authored by the
Sonnet+Opus pipeline the same way the wiki bodies are.

## Related

- Canonical wiki repo: `HyperFrequency/deep-tool-wiki`
- Per-tool reasoning ontologies: inside each `wiki.md`
- InfraNodus persistent graphs: `deep-tool-wiki-<tool>`
- Obsidian curated cross-link pages: `Auto-Quant/DeepTools/<tool>`
