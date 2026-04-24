# Vault migration (2026-04-24)

Moves the canonical vault root from `02-KB-main/` to `vaults/`. Fully
backward-compatible: existing content under `02-KB-main/` keeps working
until you migrate it.

## TL;DR

- New code in `server/src/embed.rs`: walks the list
  `DEFAULT_VAULT_DIRS = ["vaults", "02-KB-main"]`.
- If both directories contain markdown, both get indexed into `nlr_wiki`.
- To migrate, move files into `vaults/` when convenient. No rush.

## What changed

Before:

```rust
let kb = root.join("02-KB-main");
for entry in WalkDir::new(&kb) { ... }
```

After:

```rust
pub const DEFAULT_VAULT_DIRS: &[&str] = &["vaults", "02-KB-main"];

for vault_name in DEFAULT_VAULT_DIRS {
    let vault_root = root.join(vault_name);
    if !vault_root.is_dir() { continue; }
    for entry in WalkDir::new(&vault_root) { ... }
}
```

A missing directory is a silent no-op — it is safe for a fresh clone to
have only `vaults/` and no `02-KB-main/`, or vice versa.

## What stays the same

- **Skip list** — `schema.md`, `index.md`, `log.md` are still skipped.
- **Qdrant collection** — still `nlr_wiki`, 4096-dim, cosine.
- **Payload schema** — still `{ path, preview }`.
- **Env vars** — `EMBEDDING_API_URL`, `EMBEDDING_MODEL` unchanged.
- **Call signature** — `embed_wiki(root, qdrant_url, recreate)` unchanged.

## Path semantics

The payload's `path` field is **relative to the vault root that found
the file**, not relative to the overall repo. Examples:

| Filesystem path                         | Payload `path` field  |
|-----------------------------------------|-----------------------|
| `02-KB-main/legacy.md`                  | `legacy.md`           |
| `vaults/papers/kyle-1985/notes.md`      | `papers/kyle-1985/notes.md` |
| `vaults/02-KB-main/moved-legacy.md`     | `02-KB-main/moved-legacy.md` |

So if you migrate `02-KB-main/legacy.md` to `vaults/02-KB-main/legacy.md`,
the path changes from `legacy.md` to `02-KB-main/legacy.md`. Update any
dashboards or bookmarks that reference these paths accordingly.

## Migration checklist

When you're ready to move content:

1. `mkdir -p vaults/02-KB-main && mv 02-KB-main/* vaults/02-KB-main/`
2. Recreate the Qdrant collection to drop stale points:
   `curl -X DELETE 'http://localhost:6333/collections/nlr_wiki'`
3. Re-embed: `neuro-link embed --recreate`
4. Verify: `neuro-link search 'a query you know lives in that file'`

Duplicates across `vaults/` and `02-KB-main/` are indexed twice. Run the
migration end-to-end to avoid stale dupes; otherwise they're harmless
but add noise to RAG retrieval.

## Why this now

From the user: "the neuro-link repo should have a /vaults folder and
that is what the local rag pipeline should point towards". This
centralizes vault content outside the number-prefixed legacy layout
(`01-raw/`, `02-KB-main/`, etc.) so that:

- Users can point `vaults/` at an Obsidian mirror, Zotero export, or
  deep-tool-wiki submodule without renaming anything.
- New subdirs (`vaults/papers/`, `vaults/personal/`, etc.) are the
  natural way to organize diverse content without touching the
  number-prefixed convention.
- The old convention keeps working — no forced migration.

See `vaults/README.md` for the suggested layout.
