#!/usr/bin/env node
"use strict";

const { runInstaller } = require("./_shared/install-lib.js");

runInstaller({
  displayName: "Claude Code",
  binName: "neuro-link-claude-code",
  // ~/.claude is the canonical user config for Claude Code.
  configCandidates: [".claude"],
  hooksSubdir: "hooks",
  proxyPort: 7414,
  extraEnv: {
    ANTHROPIC_AUTH_TOKEN: "neuro-link-proxy",
  },
});
