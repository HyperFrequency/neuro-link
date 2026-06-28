"use strict";

// Shared install helper used by each @neuro-link/<harness> package.
// Each helper calls runInstaller(config) with a descriptor for its host CLI.

const fs = require("fs");
const os = require("os");
const path = require("path");

// Default location of the neuro-link-recursive workspace (hooks/*.sh live here).
// Override with NLR_ROOT. Falls back to the repo checkout used during development.
function resolveNlrRoot() {
  if (process.env.NLR_ROOT && fs.existsSync(process.env.NLR_ROOT)) {
    return process.env.NLR_ROOT;
  }
  const candidates = [
    path.join(os.homedir(), "Desktop", "HyperFrequency", "neuro-link"),
    path.join(os.homedir(), "neuro-link-recursive"),
    path.resolve(__dirname, "..", "..", ".."),
  ];
  for (const c of candidates) {
    if (fs.existsSync(path.join(c, "hooks")) && fs.existsSync(path.join(c, "02-KB-main"))) {
      return c;
    }
  }
  return candidates[0];
}

function log(msg) {
  process.stdout.write(`[neuro-link] ${msg}\n`);
}

function warn(msg) {
  process.stderr.write(`[neuro-link] ${msg}\n`);
}

function detectConfigDir(config) {
  const home = os.homedir();
  // Try each candidate path; pick the first that exists, else the preferred one.
  for (const rel of config.configCandidates) {
    const abs = path.join(home, rel);
    if (fs.existsSync(abs)) return abs;
  }
  return path.join(home, config.configCandidates[0]);
}

function installHooks(hooksSrcDir, hooksDestDir) {
  if (!fs.existsSync(hooksSrcDir)) {
    warn(`source hooks directory missing: ${hooksSrcDir}`);
    return [];
  }
  fs.mkdirSync(hooksDestDir, { recursive: true });

  const installed = [];
  for (const file of fs.readdirSync(hooksSrcDir)) {
    if (!file.endsWith(".sh")) continue;
    const src = path.join(hooksSrcDir, file);
    const dest = path.join(hooksDestDir, file);
    // Prefer a symlink (so hook updates propagate), fall back to a copy.
    try {
      if (fs.existsSync(dest) || fs.lstatSync(dest, { throwIfNoEntry: false })) {
        fs.unlinkSync(dest);
      }
    } catch {}
    try {
      fs.symlinkSync(src, dest);
    } catch {
      fs.copyFileSync(src, dest);
      try {
        fs.chmodSync(dest, 0o755);
      } catch {}
    }
    installed.push(path.basename(dest));
  }
  return installed;
}

function printEnvInstructions(config, nlrRoot) {
  const lines = [
    "",
    "=".repeat(64),
    `neuro-link helper for ${config.displayName} installed.`,
    "=".repeat(64),
    "",
    "Add these to your shell profile (~/.zshrc, ~/.bashrc, etc.):",
    "",
    `  export ANTHROPIC_BASE_URL="http://127.0.0.1:${config.proxyPort || 7414}"`,
    `  export NLR_ROOT="${nlrRoot}"`,
  ];
  if (config.extraEnv) {
    for (const [k, v] of Object.entries(config.extraEnv)) {
      lines.push(`  export ${k}="${v}"`);
    }
  }
  lines.push(
    "",
    "Then start the neuro-link proxy in another terminal:",
    "",
    "  neuro-link mcp &",
    "",
    "Restart your editor / harness to pick up the new hooks + env vars.",
    ""
  );
  process.stdout.write(lines.join("\n"));
}

function runInstaller(config) {
  const args = process.argv.slice(2);
  const cmd = args[0] || "install";

  if (cmd === "--help" || cmd === "-h" || cmd === "help") {
    process.stdout.write(
      `Usage: ${config.binName} [install|uninstall|status]\n\n` +
        `Installs neuro-link hooks into ${config.displayName}'s config directory\n` +
        "and prints the environment variables to export.\n"
    );
    return;
  }

  const nlrRoot = resolveNlrRoot();
  const hooksSrc = path.join(nlrRoot, "hooks");
  const hostConfigDir = detectConfigDir(config);
  const hooksDest = path.join(hostConfigDir, config.hooksSubdir || "hooks", "neuro-link");

  log(`NLR_ROOT = ${nlrRoot}`);
  log(`${config.displayName} config = ${hostConfigDir}`);

  if (cmd === "status") {
    const exists = fs.existsSync(hooksDest);
    log(`hooks installed: ${exists}`);
    if (exists) {
      const files = fs.readdirSync(hooksDest);
      for (const f of files) log(`  ${f}`);
    }
    return;
  }

  if (cmd === "uninstall") {
    if (fs.existsSync(hooksDest)) {
      fs.rmSync(hooksDest, { recursive: true, force: true });
      log(`removed ${hooksDest}`);
    } else {
      log("nothing to remove");
    }
    return;
  }

  // install (default)
  const installed = installHooks(hooksSrc, hooksDest);
  if (installed.length) {
    log(`installed ${installed.length} hook(s) into ${hooksDest}:`);
    for (const f of installed) log(`  ${f}`);
  } else {
    warn("no hooks installed (source directory empty or missing)");
  }
  printEnvInstructions(config, nlrRoot);
}

module.exports = { runInstaller, resolveNlrRoot };
