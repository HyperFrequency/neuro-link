# neuro-link `pkg/` — platform installers + cloud deploys

This tree is the **single source of truth** for how neuro-link ships. Every path below is driven by `pkg/Makefile`. Every artifact produced has a SHA-pinned proof file in `pkg/.proof/` written by the `monorepo-deploy` skill (see `~/.claude/skills/monorepo-deploy/`).

```
pkg/
├── Makefile              # top-level driver — `make all` → every target green
├── macos/                # pkgbuild + productbuild + notarytool (arm64)
├── linux-x86_64/         # fpm → .deb + .rpm + AppImage
├── linux-aarch64/        # fpm → .deb + .rpm + AppImage
├── docker/               # docker compose (dev + prod profiles)
└── cloud/
    ├── modal/            # modal.App Python deploy
    ├── lambda/           # Lambda Labs REST API wrapper + terraform
    └── ray/              # ray up cluster YAML + job submit
```

## Build contract — READY-TO-USE proof per target

Each target, when green, produces `pkg/.proof/<target>.ready.json`. The concrete
schema varies by target type (tarball vs docker image vs cloud deploy), but
every per-target proof includes at minimum: `target`, `state`, `version`,
`target_arch`, `smoke_tests[]`, `timestamp`. Examples:

Tarball targets (macos-arm64, linux-x86_64, linux-aarch64) also emit
`artifact` (path to `dist/<name>.tar.gz`), `sha256`, and a `binary_*` group
(`binary_arch`, `binary_sha256`, `binary_file`).

Docker target emits `artifact: "docker://<tag>"`, `sha256: <image_id>`, plus
`image`, `image_size_bytes`, `image_arch`, `built_from`.

Cloud targets (modal/lambda/ray) emit `state:"ready"` with an `artifact` URL
when creds are present, or `state:"skipped_no_creds"` with a `creds_probe`
block (boolean-only, never reads values) when creds are absent — per
fork-F2 honest-deferred policy.

`pkg/.proof/ALL.ready.json` is the aggregate. It has two green flags:
- `green`: all 7 targets `"ready"` (strict — requires cloud creds)
- `green_excluding_skipped`: all non-skipped targets `"ready"` (deferred OK)

The aggregator exits 0 iff `green_excluding_skipped: true`.

## Model triple (required on every target)

The model manifest lives at `<target-staging>/models-manifest/manifest.json`
inside each installer. Canonical identifiers:

- **Octen-Embedding-8B** — `mradermacher/Octen-Embedding-8B-GGUF`, file
  `Octen-Embedding-8B.f16.gguf`, dest `~/neuro-link-models/`. 4096-dim;
  BF16 safetensors variant available for FP16 cast.
- **Qwen3-Reranker-0.6B** — `ggml-org/Qwen3-Reranker-0.6B-Q8_0-GGUF`, file
  `qwen3-reranker-0.6b-q8_0.gguf`, dest `~/.cache/qmd/models/`.
- **qmd-query-expansion-1.7B** — `tobil/qmd-query-expansion-1.7B-gguf`,
  file `qmd-query-expansion-1.7B-q4_k_m.gguf`, dest `~/.cache/qmd/models/`.

Every installer stages the manifest.json; per-target `postinstall.sh`
scripts fetch-on-first-run. No manual steps.

## Dashboards (optional, not part of the build-contract proof)

Dashboards (optuna-dashboard on :8080, Plotly-Dash vbtpro overview on :8501)
are brought up by the Phase 6 shakedown step, not by `make all`. Health
checks on them are shakedown concerns, not READY-TO-USE proof fields.

## Dependency hell policy

Single Python runtime per target — `uv` managed venv with `uv.lock` cross-platform TOML resolution. Installers call `uv sync --frozen --all-extras`. No fallback to `pip install -r`. System deps audited via `brew`/`apt`/`yum`/`pacman` with pinned versions in `pkg/<target>/system-deps.txt`.

## Scope

This `pkg/` tree does NOT contain:
- Secrets — `.env` files stay on host; installers consume `$NLR_SECRETS_FILE` env var.
- Runtime state — JSONL logs under `state/` are append-only per host.
- Model weights in git — `.gitignore` excludes `models/` and `~/.cache/qmd/`.

See `pkg/macos/README.md`, `pkg/linux-x86_64/README.md`, etc. for per-target specifics.
