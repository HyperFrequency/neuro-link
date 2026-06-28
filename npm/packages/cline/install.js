#!/usr/bin/env node
"use strict";

const path = require("path");
const { runInstaller } = require("./_shared/install-lib.js");

runInstaller({
  displayName: "Cline",
  binName: "neuro-link-cline",
  // Cline is a VS Code extension; its per-user config lives under the VS Code global storage.
  configCandidates: [
    path.join("Library", "Application Support", "Code", "User", "globalStorage", "saoudrizwan.claude-dev"),
    path.join(".config", "Code", "User", "globalStorage", "saoudrizwan.claude-dev"),
    path.join("AppData", "Roaming", "Code", "User", "globalStorage", "saoudrizwan.claude-dev"),
    ".cline",
  ],
  hooksSubdir: "hooks",
  proxyPort: 7414,
});
