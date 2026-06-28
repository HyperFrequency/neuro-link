# Client-specific hooks

Only `claude-code/` is active. The other client directories (`cline`,
`forge-code`, `claw-code`, `openclaw`) were removed in the polish-a pass:
their scripts were stubs that never fired in any shipped configuration
and there was no user of the install path.

## Layout

```
clients/
  _lib/common.sh          shared shell helpers: nlr_post_event, token
  claude-code/
    pre_tool.sh           PreToolUse hook
    post_tool.sh          PostToolUse hook
    user_prompt.sh        UserPromptSubmit hook
    session_start.sh      SessionStart hook
    session_end.sh        SessionEnd / Stop hook
```

Every hook sources `_lib/common.sh`, reads the Claude Code event JSON from
stdin, and POSTs an envelope to the local neuro-link HTTP server
(`/api/v1/hooks/event`) — the server deduplicates, annotates, and appends
to `state/hooks_log.jsonl`. All hooks are fire-and-forget (backgrounded) so
they never add perceptible latency to the user's Claude Code session.

## Register in `~/.claude/settings.json`

```json
{
  "hooks": {
    "PreToolUse": [
      { "hooks": [
        { "type": "command",
          "command": "<repo>/hooks/clients/claude-code/pre_tool.sh" }
      ] }
    ],
    "PostToolUse": [
      { "hooks": [
        { "type": "command",
          "command": "<repo>/hooks/clients/claude-code/post_tool.sh" }
      ] }
    ],
    "UserPromptSubmit": [
      { "hooks": [
        { "type": "command",
          "command": "<repo>/hooks/clients/claude-code/user_prompt.sh" }
      ] }
    ],
    "SessionStart": [
      { "hooks": [
        { "type": "command",
          "command": "<repo>/hooks/clients/claude-code/session_start.sh" }
      ] }
    ],
    "SessionEnd": [
      { "hooks": [
        { "type": "command",
          "command": "<repo>/hooks/clients/claude-code/session_end.sh" }
      ] }
    ]
  }
}
```

Or just run `scripts/install-hooks.sh claude-code` to have the installer
insert the blocks idempotently.

## Pattern for new clients

If another CLI client needs equivalent event-capture hooks:

1. Add a sibling directory `clients/<client>/` with the same five scripts.
2. Each script should source `_lib/common.sh` (adjust the relative path as
   needed) and call `nlr_post_event <event_name> <client_name> [session_id]`.
3. Update `scripts/install-hooks.sh` to learn the new client's settings
   location and formatter.
4. Document the registration snippet here.

The `_lib/common.sh` contract:

- `nlr_post_event <event> <client> [session_id]` — reads stdin, appends
  metadata, POSTs to `${NLR_HOOKS_URL:-http://localhost:8080/api/v1/hooks/event}`,
  exits silently on failure.
- Auth is read from `$NLR_API_TOKEN` or `$NLR_ROOT/secrets/.env`.

Keep new client hooks thin: if the logic grows, put it in
`clients/_lib/` and call it from the client-specific scripts.
