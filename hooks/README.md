# Hooks

In-repo hook scripts for neuro-link-recursive. These are the canonical source;
the harness-global `~/.claude/hooks/` directory should either **copy** or
**symlink** to the scripts in this directory, depending on install mode.

## Layout

| Path                          | Purpose                                           |
|-------------------------------|---------------------------------------------------|
| `auto-rag-inject.sh`          | UserPromptSubmit — inject wiki context into prompts |
| `harness-bridge-check.sh`     | health probe for the external-harness bridge      |
| `neuro-grade.sh`              | grade sessions after Stop                         |
| `neuro-log-tool-use.sh`       | append tool-use events to `state/tool_use.jsonl`  |
| `neuro-task-check.sh`         | check queue status after tool use                 |
| `session-end.sh`              | append session summary to `state/session_log.jsonl` |
| `trace_logger.py`             | helper library used by the above                  |
| `clients/claude-code/`        | per-client pre/post/session hooks (see `clients/README.md`) |

## Install modes (for the user-global `~/.claude/settings.json`)

The global settings file references hooks at absolute paths such as
`~/.claude/hooks/<name>.sh`. Two install modes exist:

### 1. Copy (default)

The legacy pattern. A separate copy of each hook lives in
`~/.claude/hooks/`. Edits made in this repo do **not** propagate until the
user re-copies. This is the default behavior of `scripts/install-hooks.sh`.

### 2. Symlink (de-shadow, recommended for active development)

Run:

```bash
./scripts/install-hooks.sh claude-code --link-project
```

This creates symlinks:

```
~/.claude/hooks/auto-rag-inject.sh      -> <repo>/hooks/auto-rag-inject.sh
~/.claude/hooks/harness-bridge-check.sh -> <repo>/hooks/harness-bridge-check.sh
~/.claude/hooks/neuro-grade.sh          -> <repo>/hooks/neuro-grade.sh
~/.claude/hooks/neuro-log-tool-use.sh   -> <repo>/hooks/neuro-log-tool-use.sh
~/.claude/hooks/neuro-task-check.sh     -> <repo>/hooks/neuro-task-check.sh
...
```

for every `*.sh` at the top level of this directory. Edits in-repo take
effect immediately for the next hook firing. Idempotent: re-running is safe,
pre-existing non-symlink files are backed up timestamped before being
replaced.

The flag is only valid for `claude-code` (the other clients have been
removed — see `clients/README.md`).

## HyperFrequency-prefix hooks (`hf-*`) scoping

The user-global `~/.claude/settings.json` may register HyperFrequency hooks
such as `hf-docs-trigger.sh`, `hf-fork-trigger.sh`,
`hf-fork-auto-ingest.sh`, `doc-sync-on-push.sh`. Those hooks live in the
user global (`~/.claude/hooks/`) and are **not** shipped in this repo.

They fire on every event by default, which adds latency for non-HF
projects. The recommended pattern is an env-var guard at the top of each
`hf-*` hook:

```bash
#!/usr/bin/env bash
if [[ "${HF_HOOKS_ENABLED:-0}" != "1" ]]; then
  exit 0
fi
# ... existing hook body ...
```

With that guard the hooks no-op unless the user explicitly opts in with
`HF_HOOKS_ENABLED=1` (either in their shell env or in `settings.json` via
`"env": { "HF_HOOKS_ENABLED": "1" }` for HF-scoped projects).

**Do not edit the user's `~/.claude/hooks/*.sh` or
`~/.claude/settings.json` from this repo** — document the pattern here and
let the user apply it on their side.

## `session-end.sh` — Stop / SessionEnd registration

`hooks/session-end.sh` is a repo-shipped Stop hook. It reads the Claude
Code Stop-event JSON from stdin and appends one JSON line per session to
`$NLR_ROOT/state/session_log.jsonl` with:

```
{ "ts": <ms>, "session_id": "...", "duration_ms": <ms|null>,
  "tool_call_count": <n|null> }
```

Register it in `~/.claude/settings.json`:

```json
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          { "type": "command",
            "command": "$NLR_ROOT/hooks/session-end.sh" }
        ]
      }
    ]
  }
}
```

The hook is fail-soft: missing `$NLR_ROOT`, missing
`$NLR_ROOT/state/`, or a malformed payload all degrade to a no-op (exit 0)
so it never blocks the session from ending.
