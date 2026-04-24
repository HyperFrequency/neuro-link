# vaults/ — unified vault root for the local RAG pipeline

**As of 2026-04-24, `neuro-link/vaults/` is the canonical entry point for
all vault content consumed by the local RAG pipeline.** Previously the
pipeline scanned only `02-KB-main/`. Going forward, `embed_wiki` scans
`vaults/` first (walking all subdirectories) and falls back to
`02-KB-main/` only for backward compat.

## Convention

Drop vault content into one of the subdirectories below. The pipeline
will discover `.md` files anywhere under `vaults/` — nested directory
depth is fine. Structure it however you like; the only reserved names
are the skip-list (`schema.md`, `index.md`, `log.md`) that `embed_wiki`
ignores.

Suggested layout:

```
vaults/
├── 02-KB-main/        # legacy wiki pages — existing synthesized content
├── deep-tool-wiki/    # submodule mirror of HyperFrequency/deep-tool-wiki
├── papers/            # foundational papers (per A6 ingest)
│   ├── avellaneda-stoikov-2008/
│   ├── kyle-1985/
│   └── almgren-chriss-2001/
├── personal/          # your own notes
└── imported/          # Obsidian vault mirrors, Zotero exports, etc.
```

## Migration

Existing content under `02-KB-main/` continues to work unchanged. Move
it into `vaults/02-KB-main/` on your own schedule — no code relies on
the old path being empty. See `docs/vault-migration.md` for details.

## Rules

- Files under `01-raw/` stay where they are (immutable source material).
- Files under `02-KB-main/` remain readable by the legacy path until
  you migrate them. Both paths are indexed together; nothing breaks.
- Subdirs named `schema.md`, `index.md`, or `log.md` are skipped by
  `embed_wiki` regardless of location (see `server/src/embed.rs`).
- Secrets and private drafts should go under `secrets/` (gitignored),
  not here.

## Runtime behavior

`neuro-link`'s `embed_wiki` function (`server/src/embed.rs`) walks the
vault directories in order defined by `DEFAULT_VAULT_DIRS`. Each markdown
file is embedded into Qdrant collection `nlr_wiki` with payload fields
`path` (relative to the scanned root) and `preview` (first 500 chars).

The `path` field preserves the directory prefix, so a file at
`vaults/papers/kyle-1985/notes.md` shows up as `papers/kyle-1985/notes.md`
in search results, while `02-KB-main/legacy-entry.md` shows up as
`legacy-entry.md`. This disambiguates sources across the two roots.
