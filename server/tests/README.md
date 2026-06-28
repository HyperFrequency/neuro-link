# `server/tests/` — integration tests

## Default tests (no network, no external services)

```bash
cd server && cargo test --release --bin neuro-link
```

Runs the MCP stdio harness and tool contract suites:

- `integration_test.rs` — core MCP tool calls against a scratch `NLR_ROOT`.
- `feature_test.rs` — feature-flag / smoke checks.
- `pdf_ingest_integration.rs` (P17) — end-to-end `nlr_pdf_ingest` with a
  runtime-generated minimal 1-page PDF. Soft-skips if `poppler-utils`
  (`pdftotext`, `pdfinfo`) is not on `PATH`.

## Docker-backed tests (opt-in)

The `docker_tests` Cargo feature enables tests that spin up real containers:

- `graph_traverse_docker.rs` (P18) — starts a `neo4j:5` container on random
  host ports, seeds 5 triples via the Cypher HTTP API, and drives
  `nlr_graph_traverse` over MCP stdio. The container is torn down in a
  `Drop` impl even on panic.

Run with:

```bash
cargo test --release --bin neuro-link --features docker_tests -- --nocapture
```

Requires: `docker` on `PATH` and network egress to pull `neo4j:5` on first
run. Typical runtime: 30–60 s per test (container startup dominates).
