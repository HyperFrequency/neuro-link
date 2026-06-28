#!/usr/bin/env node
"use strict";

const { runInstaller } = require("./_shared/install-lib.js");

runInstaller({
  displayName: "ForgeCode",
  binName: "neuro-link-forge-code",
  configCandidates: [".forge", ".forge-code"],
  hooksSubdir: "hooks",
  proxyPort: 7414,
});
