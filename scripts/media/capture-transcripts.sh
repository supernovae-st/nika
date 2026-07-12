#!/usr/bin/env bash
# capture-transcripts.sh — refresh the REAL CLI transcripts that feed the
# motion scenes (scripts/media/motion/*.html). Every byte shown in a Nika
# media asset comes from these files: no fake commands, no fake output.
#
# Usage · bash scripts/media/capture-transcripts.sh
# Output · media/raw/*.txt + media/raw/transcripts.json (bundle for the renderer)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

RAW="media/raw"
FIX="scripts/media/fixtures"
mkdir -p "$RAW"

command -v nika >/dev/null || {
  echo "nika binary not found on PATH" >&2
  exit 1
}
nika --version | tee "$RAW/nika-version.txt"

# ── static-check-fix ────────────────────────────────────────────────────
# The broken fixture MUST fail (exit 2) · the fixed one MUST be clean.
nika check --color never "$FIX/broken-pr-review.nika.yaml" >"$RAW/check-broken.txt" 2>&1 && {
  echo "FATAL: broken fixture unexpectedly passed nika check" >&2
  exit 1
} || true
nika check --color never "$FIX/fixed-pr-review.nika.yaml" >"$RAW/check-fixed.txt" 2>&1

diff -u "$FIX/broken-pr-review.nika.yaml" "$FIX/fixed-pr-review.nika.yaml" \
  >"$RAW/fix-diff.txt" 2>&1 || true # diff exits 1 when files differ

# ── chat-to-workflow ────────────────────────────────────────────────────
nika check --color never "$FIX/meeting-actions.nika.yaml" >"$RAW/check-meeting.txt" 2>&1

# The run transcript uses a REAL local model. Only refresh when an Ollama
# server is reachable — otherwise keep the committed snapshot.
if curl -s --max-time 2 http://localhost:11434/api/tags >/dev/null 2>&1; then
  MODEL="ollama/llama3.2:3b"
  curl -s http://localhost:11434/api/generate \
    -d '{"model":"llama3.2:3b","prompt":"warm","stream":false}' >/dev/null || true
  rm -f action-items.json
  nika run --no-progress --color never --model "$MODEL" \
    "$FIX/meeting-actions.nika.yaml" >"$RAW/run-meeting.txt" 2>&1
  cp action-items.json "$RAW/action-items.json"
  rm -f action-items.json
else
  echo "ollama unreachable — keeping committed run-meeting.txt snapshot" >&2
fi

# ── dag-execution ───────────────────────────────────────────────────────
SHOWCASE="crates/nika-pack/pack/examples/showcase/t3-pr-review-fanout.nika.yaml"
nika inspect --format mermaid "$SHOWCASE" >"$RAW/graph-fanout.mmd" 2>&1
nika check --color never "$SHOWCASE" >"$RAW/check-fanout.txt" 2>&1

# ── permits-audit ───────────────────────────────────────────────────────
# The escaping fixture MUST fail (the boundary catches it) · the widened
# one MUST be clean with a HARD cost ceiling.
nika check --color never "$FIX/permits-escape.nika.yaml" >"$RAW/check-permits-escape.txt" 2>&1 && {
  echo "FATAL: permits-escape fixture unexpectedly passed nika check" >&2
  exit 1
} || true
nika check --color never "$FIX/permits-fits.nika.yaml" >"$RAW/check-permits-fits.txt" 2>&1
diff -u "$FIX/permits-escape.nika.yaml" "$FIX/permits-fits.nika.yaml" \
  >"$RAW/permits-fix-diff.txt" 2>&1 || true

# ── on-error-recover ────────────────────────────────────────────────────
# Deterministic + offline: the missing live-rates.json IS the failure; the
# cache task is the fallback. Run from a scratch dir so ./out starts clean.
RECOVER_TMP="$(mktemp -d)"
(
  cd "$RECOVER_TMP"
  nika run --no-progress --color never "$ROOT/$FIX/recover-fallback.nika.yaml" \
    >"$ROOT/$RAW/run-recover.txt" 2>&1
  cp out/rates.json "$ROOT/$RAW/recover-rates.json"
)
rm -rf "$RECOVER_TMP"

# ── bundle for the motion renderer ──────────────────────────────────────
node - <<'NODE'
const fs = require('fs');
const raw = 'media/raw';
const bundle = {};
for (const f of fs.readdirSync(raw)) {
  if (f === 'transcripts.json') continue;
  bundle[f] = fs.readFileSync(`${raw}/${f}`, 'utf8');
}
fs.writeFileSync(`${raw}/transcripts.json`, JSON.stringify(bundle, null, 2));
console.log(`bundled ${Object.keys(bundle).length} transcripts → ${raw}/transcripts.json`);
NODE

echo "transcripts refreshed."
