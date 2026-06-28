#!/usr/bin/env node
"use strict";

// Thin wrapper that execs the platform-specific neuro-link binary.
// The binary is resolved in this order:
//   1. Platform package at native/<platform>-<arch>/neuro-link (installed by postinstall)
//   2. NEURO_LINK_BINARY env var
//   3. Local cargo release/debug build (dev fallback)
//   4. `neuro-link` on PATH

const { spawnSync } = require("child_process");
const path = require("path");
const fs = require("fs");
const os = require("os");

function platformKey() {
  const p = process.platform;
  const a = process.arch;
  return `${p}-${a}`;
}

function binaryFilename() {
  return process.platform === "win32" ? "neuro-link.exe" : "neuro-link";
}

function resolveBinary() {
  const pkgRoot = path.resolve(__dirname, "..");
  const bin = binaryFilename();

  // 1. Bundled platform-specific binary (written by postinstall)
  const bundled = path.join(pkgRoot, "native", platformKey(), bin);
  if (fs.existsSync(bundled)) return bundled;

  // 2. Explicit override
  if (process.env.NEURO_LINK_BINARY && fs.existsSync(process.env.NEURO_LINK_BINARY)) {
    return process.env.NEURO_LINK_BINARY;
  }

  // 3. Dev fallback: cargo release / debug under ../server/target
  const devRelease = path.resolve(pkgRoot, "..", "server", "target", "release", bin);
  if (fs.existsSync(devRelease)) return devRelease;
  const devDebug = path.resolve(pkgRoot, "..", "server", "target", "debug", bin);
  if (fs.existsSync(devDebug)) return devDebug;

  // 4. On PATH
  const pathEnv = process.env.PATH || "";
  const sep = process.platform === "win32" ? ";" : ":";
  for (const dir of pathEnv.split(sep)) {
    if (!dir) continue;
    const candidate = path.join(dir, bin);
    try {
      fs.accessSync(candidate, fs.constants.X_OK);
      return candidate;
    } catch {}
  }

  return null;
}

function main() {
  const binary = resolveBinary();
  if (!binary) {
    console.error(
      "neuro-link: could not locate the platform binary.\n" +
        `Platform: ${platformKey()}\n` +
        "Tried:\n" +
        `  - ${path.resolve(__dirname, "..", "native", platformKey(), binaryFilename())}\n` +
        "  - $NEURO_LINK_BINARY\n" +
        "  - ../server/target/release/neuro-link (dev fallback)\n" +
        "  - neuro-link on PATH\n\n" +
        "Fixes:\n" +
        "  1. Reinstall: npm i -g neuro-link\n" +
        "  2. Build from source: cd server && cargo build --release\n" +
        "  3. Point NEURO_LINK_BINARY at an existing binary"
    );
    process.exit(127);
  }

  const args = process.argv.slice(2);
  const result = spawnSync(binary, args, {
    stdio: "inherit",
    env: process.env,
    windowsHide: false,
  });

  if (result.error) {
    console.error(`neuro-link: failed to execute ${binary}: ${result.error.message}`);
    process.exit(1);
  }
  if (result.signal) {
    process.kill(process.pid, result.signal);
    return;
  }
  process.exit(result.status == null ? 1 : result.status);
}

main();
