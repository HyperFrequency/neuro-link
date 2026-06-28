# @neuro-link/cline

neuro-link helper for Cline (VS Code extension). Installs neuro-link hook scripts into the Cline global-storage directory and prints the environment variables to route Cline through the neuro-link proxy.

## Usage

```bash
npx @neuro-link/cline install
npx @neuro-link/cline status
npx @neuro-link/cline uninstall
```

After install, export:

```bash
export ANTHROPIC_BASE_URL="http://127.0.0.1:7414"
export NLR_ROOT="$HOME/Desktop/HyperFrequency/neuro-link"
```

Restart VS Code to pick up the new env vars.
