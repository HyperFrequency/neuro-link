# `test-harness/` — neuro-link bench simulator

A self-contained bench harness that simulates real terminal sessions against the installed neuro-link stack. Modeled on the `strategy-translator` skill's bench pattern: fixtures → scenarios → runner → reports.

**Scope:** NOT the laptop's running stack. This harness spins up a separate throwaway runtime (per-scenario), exercises neuro-link end-to-end, captures every I/O + memory/state touch, and emits a report the monorepo-deploy skill consumes.

> **Current status (batch run 20260422-hf-nq-deployable-a7c3 / U30):**
> Only `README.md` and `run.py` are shipped on this feature branch. `scenarios/`, `conftest.py`, `fixtures/`, `state/`, `reports/` are scaffolded on-demand by later units (U44/U45 shakedown-author). `run.py` exits **non-zero (code 3)** when invoked with an empty `scenarios/` directory — silent PASS on no-scenarios is banned per the run's evidence bar.

```
test-harness/
├── README.md                           # this file — contract + how to run
├── run.py                              # entry point: `python run.py --scenario all`
├── conftest.py                         # pytest fixtures (one runtime per scenario)
├── scenarios/
│   ├── 01_rag_canonical_hit.py         # query → nlr_wiki returns a canonical chunk
│   ├── 02_rag_navigation_hit.py        # query → nlr_wiki returns a navigation breadcrumb
│   ├── 03_pine_v6_diagnostics.py       # serena-pine-mod diagnostics byte-diff vs goldens
│   ├── 04_litellm_proxy_reasoning.py   # LiteLLM + OpenRouter proxy returns 200 with reasoning
│   ├── 05_ingest_wiki_to_qdrant.py     # cold ingest: deep-tool-wiki → Qdrant count > 0
│   ├── 06_nautilus_optuna_sweep.py     # 10-trial Optuna sweep on Nautilus backtest
│   ├── 07_dashboards_reachable.py      # curl -sf :8080 (optuna)
│   ├── 08_memory_state_persistence.py  # JSONL log append + replay in fresh runtime
│   └── 09_hot_reload_no_compaction.py  # session survives a /compact-equivalent context reset
├── state/                              # append-only per-scenario JSONL logs
│   ├── session_log.jsonl               # one event per sim-user action
│   └── score_history.jsonl             # scenario pass/fail + timing history
├── fixtures/
│   ├── pine/                           # golden Pine v6 snippets + diagnostics
│   ├── strategies/                     # canonical Nautilus strategies for sweep
│   └── rag/                            # query/expected-hit pairs for scenarios 01–02
└── reports/
    └── <iso-ts>-<scenario>.md          # captured stdout + stderr + timings + pass/fail
```

## Running

```bash
# Full suite — one runtime per scenario, cold-started
python test-harness/run.py --scenario all --runtime docker-compose

# Single scenario, host runtime (fast feedback)
python test-harness/run.py --scenario 03_pine_v6_diagnostics --runtime host

# Against an installed package (validates the .pkg / .deb / cloud deploy)
python test-harness/run.py --scenario all --runtime installed --target macos-arm64
```

`--runtime` values:
- `docker-compose` — uses `pkg/docker/compose.yaml` (dev profile).
- `host` — current Python env. Fastest; less isolation.
- `installed` — assumes neuro-link is installed via `pkg/<target>/`; runs scenarios against the CLI + daemon in place.

## Memory test (scenario 08)

Scenario 08 specifically exercises the memory persistence contract:
1. Write 50 events to `state/session_log.jsonl` via the neuro-link CLI.
2. Kill the running daemon.
3. Start a fresh daemon in a clean process tree.
4. Replay the last 50 events via `nlr_sessions_parse`.
5. Assert that event ordering, timestamps, and payload SHAs match byte-for-byte.

Pass criteria:
- Zero dropped events.
- Zero reordered events.
- `score_history.jsonl` rollup matches the replay.

## Report schema

Each scenario emits `reports/<iso-ts>-<scenario>.md`:

```markdown
# <scenario_name> — <ISO8601>
**Runtime:** <docker-compose|host|installed>
**Target:** <target-id or N/A>
**Duration:** <seconds>
**Result:** PASS | FAIL

## stdout (head 500 lines)
<captured stdout>

## stderr (head 500 lines)
<captured stderr>

## state writes
<append-only log entries produced>

## assertions
- assertion_1: PASS — <evidence>
- assertion_2: FAIL — <diff>
```

## Integration with monorepo-deploy

`monorepo-deploy`'s Phase 7 shakedown calls `python test-harness/run.py --scenario all --runtime installed --target <t>` AFTER the target's smoke produces a `.ready.json`. The test-harness reports roll up into the shakedown cycle's report. Two consecutive zero-fail cycles = triple-gate item (b).

## Not in scope for this harness

- Load / perf benchmarks (that's a separate `bench/` dir — not yet authored).
- Fuzz testing of exchange adapters (separate `fuzz/` dir).
- Web UI E2E (not applicable; neuro-link has no web UI beyond optuna-dashboard).
