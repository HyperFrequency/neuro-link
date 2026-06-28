#!/usr/bin/env node
"use strict";

// Postinstall: place a platform-specific neuro-link binary at
// native/<platform>-<arch>/neuro-link so the bin wrapper can exec it.
//
// Resolution order:
//   1. Optional platform package (@neuro-link/<platform>-<arch>) — silently noop if already populated
//   2. Local dev fallback: ~/Desktop/HyperFrequency/neuro-link/server/target/release/neuro-link
//      and the sibling ../server/target/release/neuro-link when installed from the repo
//   3. Pre-existing binary on PATH — noop (wrapper will pick it up)
//   4. TODO: download prebuilt binary from GitHub releases

const fs = require("fs");
const os = require("os");
const path = require("path");
const { execFileSync } = require("child_process");

// TODO(phase-e): wire this to the real GitHub release URL once CI publishes assets.
const RELEASE_URL_TEMPLATE =
  "https://github.com/HyperFrequency/neuro-link-recursive/releases/download/v{version}/neuro-link-{platform}-{arch}{ext}";

const PKG_ROOT = path.resolve(__dirname, "..");

function platformKey() {
  return `${process.platform}-${process.arch}`;
}

function binaryFilename() {
  return process.platform === "win32" ? "neuro-link.exe" : "neuro-link";
}

function log(msg) {
  process.stdout.write(`[neuro-link postinstall] ${msg}\n`);
}

function warn(msg) {
  process.stderr.write(`[neuro-link postinstall] ${msg}\n`);
}

function supported() {
  const supported = new Set(["darwin-arm64", "darwin-x64", "linux-x64", "win32-x64"]);
  return supported.has(platformKey());
}

function targetPath() {
  return path.join(PKG_ROOT, "native", platformKey(), binaryFilename());
}

function copyBinary(src, dest) {
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.copyFileSync(src, dest);
  try {
    fs.chmodSync(dest, 0o755);
  } catch {}
}

function tryPlatformPackage() {
  const pkgName = `@neuro-link/${platformKey()}`;
  try {
    const entry = require.resolve(`${pkgName}/${binaryFilename()}`, { paths: [PKG_ROOT] });
    if (fs.existsSync(entry)) {
      copyBinary(entry, targetPath());
      log(`installed binary from ${pkgName}`);
      return true;
    }
  } catch {}
  return false;
}

function tryDevFallbacks() {
  const bin = binaryFilename();
  const candidates = [
    // When installed from the local repo (npm/ -> ../server/target/release/)
    path.resolve(PKG_ROOT, "..", "server", "target", "release", bin),
    path.resolve(PKG_ROOT, "..", "server", "target", "debug", bin),
    // Developer machine absolute path (documented in requirements)
    path.join(os.homedir(), "Desktop", "HyperFrequency", "neuro-link", "server", "target", "release", bin),
  ];

  for (const src of candidates) {
    if (fs.existsSync(src)) {
      copyBinary(src, targetPath());
      log(`installed dev binary from ${src}`);
      return true;
    }
  }
  return false;
}

function tryPathBinary() {
  const bin = binaryFilename();
  try {
    const which = process.platform === "win32" ? "where" : "which";
    const out = execFileSync(which, [bin], { encoding: "utf8", stdio: ["pipe", "pipe", "pipe"] });
    const found = out.split(/\r?\n/).find(Boolean);
    if (found && fs.existsSync(found)) {
      log(`found existing ${bin} on PATH at ${found} — wrapper will use it`);
      return true;
    }
  } catch {}
  return false;
}

function main() {
  if (process.env.NEURO_LINK_SKIP_POSTINSTALL === "1") {
    log("NEURO_LINK_SKIP_POSTINSTALL=1 — skipping");
    return;
  }

  if (!supported()) {
    warn(
      `unsupported platform ${platformKey()}; neuro-link supports darwin-arm64, darwin-x64, linux-x64, win32-x64. ` +
        "The CLI may still run if a neuro-link binary is on PATH or NEURO_LINK_BINARY is set."
    );
    return;
  }

  if (fs.existsSync(targetPath())) {
    log(`binary already present at native/${platformKey()}/${binaryFilename()} — skipping`);
    return;
  }

  if (tryPlatformPackage()) return;
  if (tryDevFallbacks()) return;
  if (tryPathBinary()) return;

  // TODO(phase-e): download from RELEASE_URL_TEMPLATE. For now, be soft-fail so
  // `npm install` never breaks — the wrapper reports a helpful error at runtime
  // and points the user at `cargo build --release` or `NEURO_LINK_BINARY`.
  warn(
    `no binary installed for ${platformKey()}. ` +
      "Build from source (`cd server && cargo build --release`) or set NEURO_LINK_BINARY. " +
      `(TODO: implement GitHub release download from ${RELEASE_URL_TEMPLATE})`
  );
}

try {
  main();
} catch (err) {
  // Never fail npm install because of postinstall.
  warn(`postinstall error: ${err && err.message ? err.message : err}`);
}
