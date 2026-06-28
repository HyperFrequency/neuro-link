# Rotating the Neo4j password

The runtime uses `NEO4J_PASSWORD` from `secrets/.env` to start and authenticate the `neo4j-nlr` container. Because the password is baked into the container's data volume on first boot, changing the env var alone is **not enough** — you must also tell the running Neo4j to change its internal password (or re-initialise the container from a fresh volume).

## Context: what happened pre-WAVE-C

Before this rotation, `docker-compose.yml` shipped with `NEO4J_AUTH: neo4j/neurolink1234` hardcoded. Any running `neo4j-nlr` container created from that compose file has the password `neurolink1234` baked into `/data` (persistent volume). Simply bumping `NEO4J_PASSWORD` in `secrets/.env` does nothing — Neo4j reads the auth only on first boot of an empty data dir.

## One-command rotation (recommended)

Run from the repo root. Replace `OLD_PASS` / `NEW_PASS` placeholders.

```bash
OLD_PASS="neurolink1234"
NEW_PASS="$(openssl rand -base64 32 | tr -dc 'A-Za-z0-9' | head -c 32)"

# 1. Rotate inside the live container using cypher-shell.
docker exec -i neo4j-nlr cypher-shell -u neo4j -p "$OLD_PASS" \
  "ALTER CURRENT USER SET PASSWORD FROM '$OLD_PASS' TO '$NEW_PASS';"

# 2. Persist the new password to secrets/.env (idempotent).
if grep -q '^NEO4J_PASSWORD=' secrets/.env 2>/dev/null; then
  # macOS: -i '' for no-backup in-place
  sed -i '' "s|^NEO4J_PASSWORD=.*$|NEO4J_PASSWORD=$NEW_PASS|" secrets/.env
else
  printf "\nNEO4J_PASSWORD=%s\n" "$NEW_PASS" >> secrets/.env
fi
chmod 600 secrets/.env

# 3. Verify from host.
docker exec -i neo4j-nlr cypher-shell -u neo4j -p "$NEW_PASS" "RETURN 1;"

echo "Rotated. New password stored in secrets/.env (mode 0600)."
echo "NEW_PASS=$NEW_PASS"
```

## Clean-slate alternative (nuke the volume)

If you don't need the existing graph data, the simplest path is to recreate the container from a new `NEO4J_PASSWORD`:

```bash
docker compose stop neo4j
docker compose rm -f neo4j
docker volume rm neuro-link_neo4j-data   # WARNING: destroys all graph data
# Generate a fresh password, persist, and recreate.
NEW_PASS="$(openssl rand -base64 32 | tr -dc 'A-Za-z0-9' | head -c 32)"
printf "NEO4J_PASSWORD=%s\n" "$NEW_PASS" >> secrets/.env
chmod 600 secrets/.env
docker compose up -d neo4j
```

## Update all callers that hold the old password

After rotation, update these references if they hold the old cleartext password:

- `secrets/.env` (done above)
- any exported env vars in your shell: `export NEO4J_PASSWORD=$NEW_PASS`
- `~/.claude.json` MCP server entries that pass `NEO4J_PASSWORD` explicitly (usually none — the Rust binary reads from `secrets/.env`)
- scripts that run `cypher-shell` directly (`ops/*.sh`, `scripts/*.sh`)

## Verify the rotation landed

```bash
# Server can still read the graph
docker exec neo4j-nlr cypher-shell -u neo4j -p "$NEW_PASS" \
  "MATCH (n) RETURN count(n);"

# Old password no longer works
docker exec neo4j-nlr cypher-shell -u neo4j -p "$OLD_PASS" \
  "RETURN 1;" 2>&1 | grep -i 'unauthorized'  # expect a match
```

## When to rotate

- Every 90 days for production / network-exposed deployments
- Immediately after any `~/.claude.json` / secrets/.env leak
- Immediately after the pre-WAVE-C default `neurolink1234` compose file has ever been used
- Before opening an ngrok tunnel for the first time on a given host
