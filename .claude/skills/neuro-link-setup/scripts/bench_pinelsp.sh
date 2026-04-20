#!/usr/bin/env bash
# bench_pinelsp.sh — tree-sitter parse performance on representative files
#
# Reports: mean/p50/p95/p99/max parse time, throughput in bytes/sec, and
# incremental-vs-full reparse speedup. Uses folknor's pine-data/tests/
# corpus + a synthetic 500-line script for the "large file" number.
#
# Required: pinelsp installed at $PINELSP_INSTALL_DIR
# Optional: CORPUS_DIR (default: $PINELSP_INSTALL_DIR/pine-data/tests)

set -euo pipefail

: "${PINELSP_INSTALL_DIR:=$HOME/.local/share/neuro-link/pinelsp}"
: "${CORPUS_DIR:=$PINELSP_INSTALL_DIR/pine-data/tests}"
: "${WARMUP:=3}"
: "${RUNS:=20}"

if [ ! -d "$PINELSP_INSTALL_DIR" ]; then
  printf 'pinelsp not installed at %s\n' "$PINELSP_INSTALL_DIR" >&2
  exit 1
fi

cd "$PINELSP_INSTALL_DIR"

cat > /tmp/pinelsp-bench.mjs <<'EOF'
import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Parser, Language } from 'web-tree-sitter';
import { performance } from 'node:perf_hooks';

const here = path.dirname(fileURLToPath(import.meta.url));
const wasm = process.env.PINE_WASM ||
  path.resolve(process.cwd(), 'packages/tree-sitter-pine/tree-sitter-pine.wasm');
const corpus = process.env.CORPUS_DIR ||
  path.resolve(process.cwd(), 'pine-data/tests');
const warmup = Number(process.env.WARMUP || 3);
const runs   = Number(process.env.RUNS   || 20);

await Parser.init();
const pine = await Language.load(wasm);
const parser = new Parser();
parser.setLanguage(pine);

function* walk(dir) {
  if (!fs.existsSync(dir)) return;
  for (const ent of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, ent.name);
    if (ent.isDirectory()) yield* walk(p);
    else if (ent.isFile() && p.endsWith('.pine')) yield p;
  }
}

const files = [...walk(corpus)];
if (files.length === 0) {
  console.error(`no .pine files under ${corpus}`);
  process.exit(2);
}

function pct(arr, p) {
  const s = [...arr].sort((a, b) => a - b);
  return s[Math.min(s.length - 1, Math.floor(s.length * p / 100))];
}

// Full parse benchmark
let totalBytes = 0;
const durs = [];
for (let w = 0; w < warmup; w++) {
  for (const f of files) parser.parse(fs.readFileSync(f, 'utf8'));
}
for (let r = 0; r < runs; r++) {
  for (const f of files) {
    const src = fs.readFileSync(f, 'utf8');
    const t0 = performance.now();
    parser.parse(src);
    durs.push(performance.now() - t0);
    if (r === 0) totalBytes += src.length;
  }
}

console.log(JSON.stringify({
  corpus,
  file_count: files.length,
  total_bytes: totalBytes,
  runs,
  warmup,
  parse_ms: {
    mean: durs.reduce((a, b) => a + b, 0) / durs.length,
    p50: pct(durs, 50),
    p95: pct(durs, 95),
    p99: pct(durs, 99),
    max: Math.max(...durs),
  },
  throughput_mb_per_sec: (totalBytes / 1e6) / (durs.reduce((a, b) => a + b, 0) / durs.length / 1000),
}, null, 2));

// Incremental reparse speedup on the largest file
const largest = files.reduce((a, b) => fs.statSync(a).size > fs.statSync(b).size ? a : b);
const src = fs.readFileSync(largest, 'utf8');
const tree = parser.parse(src);
const incDurs = [];
for (let r = 0; r < runs; r++) {
  tree.edit({
    startIndex: src.length,
    oldEndIndex: src.length,
    newEndIndex: src.length + 1,
    startPosition: { row: 0, column: 0 },
    oldEndPosition: { row: 0, column: 0 },
    newEndPosition: { row: 0, column: 1 },
  });
  const t0 = performance.now();
  parser.parse(src + '\n', tree);
  incDurs.push(performance.now() - t0);
}
console.log(JSON.stringify({
  incremental_target: largest,
  size_bytes: src.length,
  incremental_parse_ms: {
    mean: incDurs.reduce((a, b) => a + b, 0) / incDurs.length,
    p50: pct(incDurs, 50),
    p95: pct(incDurs, 95),
  },
}, null, 2));
EOF

PINE_WASM="$PINELSP_INSTALL_DIR/packages/tree-sitter-pine/tree-sitter-pine.wasm" \
CORPUS_DIR="$CORPUS_DIR" \
WARMUP="$WARMUP" RUNS="$RUNS" \
node /tmp/pinelsp-bench.mjs
