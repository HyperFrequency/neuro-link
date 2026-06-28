#!/usr/bin/env node
"use strict";

const { runInstaller } = require("./_shared/install-lib.js");

runInstaller({
  displayName: "Claw-Code",
  binName: "neuro-link-claw-code",
  configCandidates: [".claw", ".claw-code"],
  hooksSubdir: "hooks",
  proxyPort: 7414,
});
