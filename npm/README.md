# neuro-link

Unified context, memory & behavior control plane for AI agent harnesses. Hybrid RAG + LLM-Wiki system (Karpathy pattern) with auto-curation, reasoning ontologies, and recursive self-improvement.

Ships a native Rust binary plus per-CLI helper packages that wire hooks + environment variables into Claude Code, Cline, ForgeCode, Claw-Code, and OpenClaw.

## Install

```bash
npm i -g neuro-link
```

Or run on-demand:

```bash
npx neuro-link init
```

The `postinstall` step downloads a prebuilt binary for your platform (darwin-arm64, darwin-x64, linux-x64, win32-x64). If you're running from the repo, it uses the local `server/target/release/neuro-link` build as a fallback.

If no binary can be fetched, build it yourself:

```bash
cd server && cargo build --release
```

## Quick start

```bash
neuro-link init              # Scaffold the directory tree, install skills & hooks
neuro-link status            # Check Rust server, Python, Qdrant, Neo4j, hooks, skills
neuro-link tasks             # List the task queue
neuro-link mcp               # Run as an MCP server over stdio
```

Both `neuro-link` and the short alias `nlr` are installed.

## Per-CLI helpers

Install the helper for your harness to register hooks and print the env vars you need:

```bash
npx @neuro-link/claude-code install
npx @neuro-link/cline install
npx @neuro-link/forge-code install
npx @neuro-link/claw-code install
npx @neuro-link/openclaw install
```

Each helper detects the host CLI's config directory, copies/symlinks the neuro-link hook scripts into it, and prints the `ANTHROPIC_BASE_URL` (and related) values to export.

## Environment

| Variable | Purpose |
|---|---|
| `NEURO_LINK_BINARY` | Absolute path override for the native binary. |
| `NEURO_LINK_SKIP_POSTINSTALL` | Set to `1` to skip the postinstall binary resolution. |
| `NLR_ROOT` | Root of your neuro-link-recursive workspace (used by hooks and the MCP server). |
| `ANTHROPIC_BASE_URL` | Set by helper packages so harnesses route through the neuro-link proxy. |

## Platform binaries

The package ships as a thin wrapper. The real binary lives either in a platform-specific optional dependency (`@neuro-link/darwin-arm64`, etc.) or is downloaded by `scripts/postinstall.js` into `native/<platform>-<arch>/neuro-link`.

## Repository

https://github.com/HyperFrequency/neuro-link-recursive
