"""Ray Tune + Optuna sweep over a Nautilus backtest.

Submit: RAY_ADDRESS=http://$HEAD:8265 ray job submit --working-dir . -- python pkg/cloud/ray/job.py
"""
from __future__ import annotations

import ray
from ray import tune, train
from ray.tune.search.optuna import OptunaSearch


def trainable(config: dict) -> None:
    # TODO: wire a Nautilus BacktestNode with params from config.
    fast = config["fast_period"]
    slow = config["slow_period"]
    sharpe = (slow - fast) / 10.0  # placeholder metric
    train.report({"sharpe": sharpe})


def main(num_samples: int = 100) -> None:
    ray.init(address="auto")

    param_space = {
        "fast_period": tune.randint(5, 50),
        "slow_period": tune.randint(50, 200),
    }

    tuner = tune.Tuner(
        trainable,
        param_space=param_space,
        tune_config=tune.TuneConfig(
            num_samples=num_samples,
            search_alg=OptunaSearch(metric="sharpe", mode="max"),
            max_concurrent_trials=8,
        ),
        run_config=train.RunConfig(
            name="nlr-sweep",
            storage_path="/mnt/cluster_storage/nlr",
        ),
    )

    results = tuner.fit()
    print("best:", results.get_best_result(metric="sharpe", mode="max").config)


if __name__ == "__main__":
    main()
