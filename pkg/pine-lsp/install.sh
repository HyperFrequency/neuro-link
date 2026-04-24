#!/usr/bin/env bash
# install.sh — build + install pine-lsp (folknor TypeScript + zelosleone Rust variants)
#
# Locates the pinelsp submodule at $NEURO_QUANT_ROOT/pinelsp (parent monorepo).
# Installs two binaries:
#   pine-lsp        — folknor's TS implementation at ./dist/packages/lsp/bin/pine-lsp.js
#   pine-lsp-rust   — zelosleone's Rust implementation (optional; --no-rust to skip)
#
# Both are symlinked into $BIN_DIR (default: ~/.local/bin). After install,
# `pine-lsp --version` (and `pine-lsp-rust --version` if built) must succeed.
#
# Env:
#   BIN_DIR          — where to symlink CLIs (default: $HOME/.local/bin)
#   NEURO_QUANT_ROOT — monorepo root containing pinelsp/ submodule
#                      (default: auto-detect two levels up from this script)
#   SKIP_RUST=1      — skip building the Rust variant
#   SKIP_FOLKNOR=1   — skip building the folknor TS variant

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# pkg/pine-lsp/ → pkg/ → neuro-link/ → neuro-quant/  (the monorepo root)
NEURO_QUANT_ROOT="${NEURO_QUANT_ROOT:-$(cd "$SCRIPT_DIR/../../.." && pwd)}"
PINELSP_DIR="$NEURO_QUANT_ROOT/pinelsp"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
SKIP_RUST="${SKIP_RUST:-0}"
SKIP_FOLKNOR="${SKIP_FOLKNOR:-0}"

log() { printf "[pine-lsp/install] %s\n" "$*" >&2; }

# --- Pre-flight ---
if [[ ! -d "$PINELSP_DIR" ]]; then
  log "ERROR: pinelsp submodule not found at $PINELSP_DIR"
  log "       git submodule update --init pinelsp (from $NEURO_QUANT_ROOT)"
  exit 1
fi

mkdir -p "$BIN_DIR"

# --- 1. folknor TypeScript LSP (pnpm) ---
if [[ "$SKIP_FOLKNOR" != "1" ]]; then
  if ! command -v pnpm >/dev/null 2>&1; then
    log "ERROR: pnpm not on PATH. npm install -g pnpm@10"
    exit 1
  fi

  log "Building folknor TypeScript pine-lsp (pnpm install && pnpm run build)"
  (
    cd "$PINELSP_DIR"
    pnpm install --frozen-lockfile
    pnpm run build
  )

  TS_BIN="$PINELSP_DIR/dist/packages/lsp/bin/pine-lsp.js"
  if [[ ! -f "$TS_BIN" ]]; then
    log "ERROR: expected build output missing: $TS_BIN"
    exit 1
  fi

  # Ensure shebang on the JS entry (pnpm build should add it, but verify).
  # The symlink + chmod exists on the built file itself; we only wrap it
  # with a `node` invocation if the JS is not already executable.
  if [[ ! -x "$TS_BIN" ]]; then
    chmod +x "$TS_BIN" || true
  fi

  # Write a small shim so `pine-lsp --version` runs node for us.
  SHIM="$BIN_DIR/pine-lsp"
  cat > "$SHIM" <<SHIM_EOF
#!/usr/bin/env bash
exec node "$TS_BIN" "\$@"
SHIM_EOF
  chmod +x "$SHIM"
  log "  installed shim: $SHIM -> node $TS_BIN"
else
  log "SKIP_FOLKNOR=1 — skipping TypeScript build"
fi

# --- 2. zelosleone Rust LSP ---
if [[ "$SKIP_RUST" != "1" ]]; then
  RUST_DIR="$PINELSP_DIR/zelosleone-pine-rust"
  if [[ ! -d "$RUST_DIR" ]]; then
    log "  zelosleone-pine-rust/ not present in pinelsp/ — skipping Rust variant"
  else
    if ! command -v cargo >/dev/null 2>&1; then
      log "ERROR: cargo not on PATH. install Rust: https://rustup.rs/"
      exit 1
    fi

    log "Building zelosleone Rust pine-lsp (cargo build --release)"
    (
      cd "$RUST_DIR"
      cargo build --release
    )

    # Crate name from Cargo.toml: pinescript-vsc-server-rust
    RUST_BIN="$RUST_DIR/target/release/pinescript-vsc-server-rust"
    if [[ ! -x "$RUST_BIN" ]]; then
      log "ERROR: expected Rust binary missing: $RUST_BIN"
      exit 1
    fi

    RUST_LINK="$BIN_DIR/pine-lsp-rust"
    ln -sf "$RUST_BIN" "$RUST_LINK"
    log "  installed symlink: $RUST_LINK -> $RUST_BIN"
  fi
else
  log "SKIP_RUST=1 — skipping Rust build"
fi

# --- 3. Verify ---
log "Verifying installs"
FAIL=0

if [[ "$SKIP_FOLKNOR" != "1" ]]; then
  if "$BIN_DIR/pine-lsp" --version >/dev/null 2>&1; then
    log "  PASS: pine-lsp --version"
  elif "$BIN_DIR/pine-lsp" --help >/dev/null 2>&1; then
    # Some LSP servers don't support --version; --help is an acceptable proxy.
    log "  PASS: pine-lsp --help (no --version but --help works)"
  else
    log "  FAIL: pine-lsp does not respond to --version or --help"
    FAIL=1
  fi
fi

if [[ "$SKIP_RUST" != "1" && -x "$BIN_DIR/pine-lsp-rust" ]]; then
  if "$BIN_DIR/pine-lsp-rust" --version >/dev/null 2>&1; then
    log "  PASS: pine-lsp-rust --version"
  elif "$BIN_DIR/pine-lsp-rust" --help >/dev/null 2>&1; then
    log "  PASS: pine-lsp-rust --help"
  else
    log "  WARN: pine-lsp-rust built but does not respond to --version/--help (LSP servers often don't)"
  fi
fi

if [[ "$FAIL" == "1" ]]; then
  exit 1
fi

log "DONE. Add $BIN_DIR to PATH if not already: export PATH=\"$BIN_DIR:\$PATH\""
