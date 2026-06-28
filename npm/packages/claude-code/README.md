# @neuro-link/claude-code

neuro-link helper for Claude Code. Installs the neuro-link hook scripts into `~/.claude/hooks/neuro-link/` and prints the environment variables you need to route Claude Code through the neuro-link proxy.

## Usage

```bash
npx @neuro-link/claude-code install     # default; runs on postinstall too
npx @neuro-link/claude-code status
npx @neuro-link/claude-code uninstall
```

After install, export:

```bash
export ANTHROPIC_BASE_URL="http://127.0.0.1:7414"
export ANTHROPIC_AUTH_TOKEN="neuro-link-proxy"
export NLR_ROOT="$HOME/Desktop/HyperFrequency/neuro-link"
```

Then start the proxy:

```bash
neuro-link mcp &
```

Restart Claude Code to pick up the new hooks.
