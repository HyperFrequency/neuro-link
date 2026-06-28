# obsidian-copilot attribution

Upstream: https://github.com/logancyang/obsidian-copilot (AGPL-3.0-only)

No code from obsidian-copilot is adapted in this PR. This directory
exists so that when Phase 6 (chat panel UI) lands — which *will* adapt
patterns from `src/components/Chat/ChatInput.tsx` and sibling files —
the attribution + license copy is already in place.

Any file in this plugin that contains adapted obsidian-copilot code
MUST carry an `SPDX-License-Identifier: AGPL-3.0-only` header and
preserve the upstream copyright notice. Files that reuse only broad
design ideas (without verbatim code) do not need the header.

See `.planning/2026-04-18-turbovault-qmd-rebuild/20-obsidian-chat-panel.md`
for the Phase 6/7 design including the specific files to adapt.
