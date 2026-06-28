#!/usr/bin/env node
"use strict";

const { runInstaller } = require("./_shared/install-lib.js");

runInstaller({
  displayName: "OpenClaw",
  binName: "neuro-link-openclaw",
  configCandidates: [".openclaw"],
  hooksSubdir: "hooks",
  proxyPort: 7414,
});
