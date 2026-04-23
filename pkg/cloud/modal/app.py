"""Modal deployment for neuro-link + Nautilus + Optuna sweeps.

Deploy: `modal deploy pkg/cloud/modal/app.py`
Test:   `modal run pkg/cloud/modal/app.py::sweep --trials 10`
"""
from __future__ import annotations

import modal

app = modal.App("neuro-link")

image = (
    modal.Image.debian_slim(python_version="3.12")
    .apt_install("git", "build-essential", "libssl-dev")
    .pip_install(
        "uv",
        "nautilus-trader==1.225.0",
        "optuna==4.8.0",
        "optuna-dashboard==0.20.0",
        "ray[default,tune]==2.55.0",
        "sentence-transformers",
        "qdrant-client",
        "huggingface-hub",
    )
    .run_commands(
        "huggingface-cli download mradermacher/Octen-Embedding-8B-GGUF "
        "Octen-Embedding-8B.Q8_0.gguf --local-dir /root/models "
        "--local-dir-use-symlinks False",
        "huggingface-cli download ggml-org/Qwen3-Reranker-0.6B-Q8_0-GGUF "
        "qwen3-reranker-0.6b-q8_0.gguf --local-dir /root/.cache/qmd/models "
        "--local-dir-use-symlinks False",
        "huggingface-cli download tobil/qmd-query-expansion-1.7B-gguf "
        "qmd-query-expansion-1.7B-q4_k_m.gguf --local-dir /root/.cache/qmd/models "
        "--local-dir-use-symlinks False",
    )
)

volume = modal.Volume.from_name("nlr-data", create_if_missing=True)
secret = modal.Secret.from_name("nlr-secrets")


@app.function(image=image, gpu="A100", timeout=3600, secrets=[secret], volumes={"/data": volume})
def sweep(trials: int = 100, study_name: str = "nlr-default") -> dict:
    """Run an Optuna sweep of a Nautilus backtest on an A100."""
    import optuna
    from nautilus_trader.backtest.node import BacktestNode, BacktestRunConfig

    storage = f"sqlite:////data/optuna/{study_name}.db"
    study = optuna.create_study(
        study_name=study_name,
        storage=storage,
        direction="maximize",
        load_if_exists=True,
    )

    def objective(trial: optuna.Trial) -> float:
        fast = trial.suggest_int("fast_period", 5, 50)
        slow = trial.suggest_int("slow_period", 50, 200)
        # TODO: wire a real BacktestRunConfig with a HF-forked strategy
        return float(fast + slow)  # placeholder — replace with Sharpe ratio

    study.optimize(objective, n_trials=trials, n_jobs=1, show_progress_bar=False)
    return {
        "study_name": study_name,
        "n_trials": len(study.trials),
        "best_value": study.best_value,
        "best_params": study.best_params,
        "storage": storage,
    }


@app.local_entrypoint()
def main(trials: int = 10):
    result = sweep.remote(trials=trials)
    print(result)
