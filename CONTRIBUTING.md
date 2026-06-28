# Contributing to neuro-link-recursive

## Development Setup

1. Clone the repo and run the init script:
   ```bash
   git clone https://github.com/HyperFrequency/neuro-link-recursive.git
   cd neuro-link-recursive
   bash scripts/init.sh
   ```

2. See [INSTALL.md](INSTALL.md) for full dependency installation (Rust, npm, Docker services, MCP servers, API keys).

3. Build the Rust server:
   ```bash
   cd server && cargo build && cd ..
   ```

## Running Tests

```bash
# All tests
make test

# Rust tests only
cd server && cargo test

# Lint everything
make lint
```

## Code Style

### Rust
- Follow standard Rust idioms. Run `cargo clippy` before committing.
- No warnings allowed in CI (`-D warnings`).

### Shell
- All hook scripts must pass [shellcheck](https://www.shellcheck.net/).
- Use `set -euo pipefail` at the top of every script.
- Use `#!/usr/bin/env bash` shebang.

### Markdown
- Config files in `config/` use YAML frontmatter + markdown body.
- Wiki pages follow `02-KB-main/schema.md` conventions.
- Skill definitions follow the `SKILL.md` format in `skills/`.

## Pull Request Process

1. Create a feature branch from `master`:
   ```bash
   git checkout -b feat/your-feature master
   ```

2. Make your changes. Ensure:
   - `make lint` passes with no errors
   - `make test` passes
   - New skills include a `SKILL.md`
   - New hooks are registered in `scripts/init.sh`
   - Config changes include YAML frontmatter

3. Commit with a clear, descriptive message. Prefix with category:
   - `feat:` new feature
   - `fix:` bug fix
   - `docs:` documentation only
   - `refactor:` code restructuring
   - `chore:` tooling, CI, deps

4. Open a PR against `master`. In the description:
   - Summarize what changed and why
   - List any new dependencies or config changes
   - Note if it requires re-running `init.sh`

5. CI must pass before merge. At least one review required.
