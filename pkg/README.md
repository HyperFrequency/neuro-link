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

Each target, when green, produces `pkg/.proof/<target>.ready.json`:

```json
{
  "target": "macos-arm64",
  "version": "0.x.y",
  "artifact": "dist/neuro-link-0.x.y-arm64.pkg",
  "sha256": "...",
  "smoke_tests": [{"name": "cli_help", "exit_code": 0, "stdout_head": "..."}],
  "models_present": ["Octen-Embedding-8B.Q8_0.gguf", "qwen3-reranker-0.6b-q8_0.gguf", "qmd-query-expansion-1.7B-q4_k_m.gguf"],
  "dashboards_reachable": {"optuna-dashboard": "http://localhost:8080", "vbtpro-dashboard": "http://localhost:8501"},
  "timestamp": "<ISO8601>"
}
```

Absence of a `.ready.json` file means the target is NOT green. The skill refuses to stamp READY-TO-USE without every expected `.ready.json` present.

## Model triple (required on every target)

See `.batch-runs/20260422-hf-nq-deployable-a7c3/research/api-pins.md` §Model triple for canonical identifiers.

- Octen-Embedding-8B (Q8_0 GGUF, 4096-dim; BF16 safetensors variant available for FP16 cast)
- Qwen3-Reranker-0.6B (Q8_0 GGUF, stored in `~/.cache/qmd/models/`)
- qmd-query-expansion-1.7B (Q4_K_M GGUF, stored in `~/.cache/qmd/models/`)

Every installer MUST pre-download or fetch-on-first-run. No manual steps.

## Dashboards (required on every target)

- **optuna-dashboard** — `pip install optuna-dashboard`, launch `optuna-dashboard sqlite:///<study.db>` on :8080. Installer adds a launcher unit (systemd on linux, LaunchAgent on macOS, modal-scheduled on cloud).
- **vectorbtpro dashboard** — no first-party dashboard; default = scaffold a minimal Plotly Dash app over `vbt.Portfolio` on :8501. Confirm this interpretation with user before shipping.

## Dependency hell policy

Single Python runtime per target — `uv` managed venv with `uv.lock` cross-platform TOML resolution. Installers call `uv sync --frozen --all-extras`. No fallback to `pip install -r`. System deps audited via `brew`/`apt`/`yum`/`pacman` with pinned versions in `pkg/<target>/system-deps.txt`.

## Scope

This `pkg/` tree does NOT contain:
- Secrets — `.env` files stay on host; installers consume `$NLR_SECRETS_FILE` env var.
- Runtime state — JSONL logs under `state/` are append-only per host.
- Model weights in git — `.gitignore` excludes `models/` and `~/.cache/qmd/`.

See `pkg/macos/README.md`, `pkg/linux-x86_64/README.md`, etc. for per-target specifics.
