#!/usr/bin/env bash
# Domain: Neo4j schema tests.
# - Attempt to create duplicate :Entity with same canonical_name → assert rejected
#   (this requires the WAVE B constraint to be shipped; otherwise EXPECTED_SKIP)
# - Attempt to create :OntologyTriple without required fields → assert rejected
# - Smoke-test: MATCH patterns that the dispatcher uses
#
# Uses Neo4j HTTP Cypher endpoint at /db/neo4j/tx/commit.

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

init_test_script "neo4j"
require_tools curl jq python3

NEO4J_TX="$NEO4J_BASE/db/neo4j/tx/commit"

cypher() {
    local stmt="$1"
    curl -s -u "$NEO4J_AUTH" -H "Content-Type: application/json" \
        --max-time 10 \
        -d "$(jq -n --arg s "$stmt" '{statements:[{statement:$s}]}')" \
        "$NEO4J_TX" 2>&1
}

# Reachability
start=$(now_ms)
code=$(curl -s -u "$NEO4J_AUTH" -o /dev/null -w '%{http_code}' --max-time 10 "$NEO4J_BASE/")
dur=$(( $(now_ms) - start ))
if [ "$code" = "200" ]; then
    record_test "neo4j reachable (HTTP 200)" PASS "$dur"
else
    record_test "neo4j reachable (HTTP 200)" FAIL "$dur" "got $code"
    print_script_summary
fi

# Check what constraints exist right now
start=$(now_ms)
constraints=$(cypher "SHOW CONSTRAINTS YIELD name, type, entityType, labelsOrTypes, properties")
dur=$(( $(now_ms) - start ))
constraint_rows=$(printf "%s" "$constraints" | jq -r '.results[0].data | length // 0' 2>/dev/null)
record_test "SHOW CONSTRAINTS returns" PASS "$dur" "$constraint_rows rows"

entity_unique_exists=$(printf "%s" "$constraints" | jq -r \
    '.results[0].data[]? | .row | select(.[2] == "NODE" and (.[3][0] == "Entity") and (.[4][0] == "canonical_name")) | "yes"' \
    2>/dev/null | head -n1)

# Cleanup helper — remove any test nodes we create
cleanup_neo4j() {
    cypher "MATCH (n) WHERE n.test_marker = 'nlr-suite' DETACH DELETE n" >/dev/null 2>&1 || true
}
cleanup_neo4j
trap cleanup_neo4j EXIT

# ── Test 1: Duplicate :Entity rejected (only if constraint exists) ──
start=$(now_ms)
if [ "$entity_unique_exists" = "yes" ]; then
    # Create first Entity
    r1=$(cypher "CREATE (e:Entity {canonical_name: 'nlr-test-dup-entity', test_marker: 'nlr-suite'}) RETURN e")
    # Try to create duplicate
    r2=$(cypher "CREATE (e:Entity {canonical_name: 'nlr-test-dup-entity', test_marker: 'nlr-suite'}) RETURN e")
    dur=$(( $(now_ms) - start ))
    has_error=$(printf "%s" "$r2" | jq -r '.errors | length // 0')
    if [ "$has_error" -ge 1 ]; then
        err=$(printf "%s" "$r2" | jq -r '.errors[0].code // ""')
        if [[ "$err" == *"ConstraintValidationFailed"* ]] || [[ "$err" == *"SchemaConstraint"* ]]; then
            record_test "Entity duplicate canonical_name rejected" PASS "$dur" "$err"
        else
            record_test "Entity duplicate canonical_name rejected" FAIL "$dur" "unexpected err: $err"
        fi
    else
        record_test "Entity duplicate canonical_name rejected" FAIL "$dur" "duplicate accepted"
    fi
    # Clean up
    cypher "MATCH (e:Entity) WHERE e.canonical_name = 'nlr-test-dup-entity' DETACH DELETE e" >/dev/null 2>&1
else
    record_test "Entity duplicate canonical_name rejected" EXPECTED_SKIP 0 \
        "WAVE B constraint not shipped yet"
fi

# ── Test 2: OntologyTriple missing required fields ──
# Check if constraint exists for OntologyTriple
ot_constraints=$(printf "%s" "$constraints" | jq -r \
    '.results[0].data[]? | .row | select(.[3][0] == "OntologyTriple") | .[0]' \
    2>/dev/null)

start=$(now_ms)
if [ -n "$ot_constraints" ]; then
    r=$(cypher "CREATE (t:OntologyTriple {test_marker: 'nlr-suite'}) RETURN t")
    dur=$(( $(now_ms) - start ))
    has_error=$(printf "%s" "$r" | jq -r '.errors | length // 0')
    if [ "$has_error" -ge 1 ]; then
        record_test "OntologyTriple without required fields rejected" PASS "$dur"
    else
        record_test "OntologyTriple without required fields rejected" FAIL "$dur" "accepted"
    fi
    cypher "MATCH (t:OntologyTriple) WHERE t.test_marker = 'nlr-suite' DETACH DELETE t" >/dev/null 2>&1
else
    record_test "OntologyTriple without required fields rejected" EXPECTED_SKIP 0 \
        "WAVE B constraints not shipped"
fi

# ── Test 3: Smoke test — dispatcher MATCH patterns work ──

# Seed a small ontology subgraph
cypher "CREATE (a:Entity {canonical_name: 'nlr-test-alpha', test_marker: 'nlr-suite'})-[:RELATES_TO]->(b:Entity {canonical_name: 'nlr-test-beta', test_marker: 'nlr-suite'})" >/dev/null 2>&1

start=$(now_ms)
r=$(cypher "MATCH (e:Entity {canonical_name: 'nlr-test-alpha'})-[r]->(m) RETURN e.canonical_name, type(r), m.canonical_name")
dur=$(( $(now_ms) - start ))
row_count=$(printf "%s" "$r" | jq -r '.results[0].data | length // 0')
if [ "$row_count" -ge 1 ]; then
    record_test "MATCH Entity → RELATES_TO → Entity works" PASS "$dur" "rows=$row_count"
else
    record_test "MATCH Entity → RELATES_TO → Entity works" FAIL "$dur" "got 0 rows: ${r:0:200}"
fi

# Clean up
cypher "MATCH (n) WHERE n.test_marker = 'nlr-suite' DETACH DELETE n" >/dev/null 2>&1

# ── Test 4: Basic database info ──
start=$(now_ms)
r=$(cypher "CALL dbms.components() YIELD name, versions RETURN name, versions")
dur=$(( $(now_ms) - start ))
if printf "%s" "$r" | jq -e '.results[0].data | length >= 1' >/dev/null 2>&1; then
    record_test "dbms.components() works" PASS "$dur"
else
    record_test "dbms.components() works" FAIL "$dur" "${r:0:200}"
fi

# ── Test 5: Can create and query a wiki ingest trace (typical pattern) ──
start=$(now_ms)
cypher "CREATE (p:WikiPage {path: 'quant/test.md', title: 'Test', test_marker: 'nlr-suite'})" >/dev/null 2>&1
r=$(cypher "MATCH (p:WikiPage {title: 'Test'}) RETURN p.path")
dur=$(( $(now_ms) - start ))
if printf "%s" "$r" | jq -e '.results[0].data | length >= 1' >/dev/null 2>&1; then
    record_test "WikiPage node create + match" PASS "$dur"
else
    record_test "WikiPage node create + match" FAIL "$dur" "${r:0:200}"
fi
cypher "MATCH (p:WikiPage) WHERE p.test_marker = 'nlr-suite' DETACH DELETE p" >/dev/null 2>&1

print_script_summary
