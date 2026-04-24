#!/usr/bin/env bash
# UserPromptSubmit hook: detect mentions of HyperFrequency forks or their upstream libraries
# and inject a reminder to use the docs-dual-lookup skill (Context7 + Auggie).
#
# Reads JSON event from stdin, writes a JSON response to stdout if matched.

set -euo pipefail

# Read the prompt from stdin JSON.
input="$(cat)"
prompt="$(printf '%s' "$input" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d.get("prompt",""))' 2>/dev/null || echo "")"

if [[ -z "$prompt" ]]; then
  exit 0
fi

# Lowercase for matching.
lc="$(printf '%s' "$prompt" | tr '[:upper:]' '[:lower:]')"

# HyperFrequency forks + their upstream library names + common tooling.
# Keep this list in sync with `gh repo list HyperFrequency`.
keywords=(
  hyperfrequency
  "gh repo fork"
  forge-code nautilus_prediction nautilus_llm_trader claw-code-parity auto-brain
  nautilus_admin genetics_trading obsidian-neural-composer codebuff
  mlflow h2o-3 tabpfn flaml vectorbt quantlib tardis-python tardis-machine
  optuna tlob autogluon chronos-forecasting qlib featuretools cryptofeed
  tsfel finrl hyper-stats hftbacktest tsfresh automl-agent mother
  pycaret caafe hmmlearn coml xfeat automl-papers adanet deeplob
  hf-automl rl-algorithms automl-gs nautilus_trader nautilus-trader
)

matched=""
for kw in "${keywords[@]}"; do
  if [[ "$lc" == *"$kw"* ]]; then
    matched="$kw"
    break
  fi
done

if [[ -z "$matched" ]]; then
  exit 0
fi

# Inject a system reminder via additionalContext.
cat <<EOF
{
  "hookSpecificOutput": {
    "hookEventName": "UserPromptSubmit",
    "additionalContext": "AUTO-TRIGGER: This prompt mentions a HyperFrequency fork or its upstream library ('${matched}'). Before recommending APIs, signatures, or code patterns from any third-party library here, you MUST invoke the docs-dual-lookup skill to query both Context7 and Auggie in parallel. Synthesize the answers and cite which sources confirmed each claim. If creating a fork (gh repo fork ...), first run docs-dual-lookup on the upstream library to confirm current API surface and avoid forking against stale assumptions."
  }
}
EOF
