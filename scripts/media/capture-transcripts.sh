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
if nika check --color never "$FIX/broken-pr-review.nika" >"$RAW/check-broken.txt" 2>&1; then
  echo "FATAL: broken fixture unexpectedly passed nika check" >&2
  exit 1
fi
nika check --color never "$FIX/fixed-pr-review.nika" >"$RAW/check-fixed.txt" 2>&1

diff -u "$FIX/broken-pr-review.nika" "$FIX/fixed-pr-review.nika" \
  >"$RAW/fix-diff.txt" 2>&1 || true # diff exits 1 when files differ

# ── chat-to-workflow · nika-hero ────────────────────────────────────────
MODEL="ollama/llama3.2:3b"
nika check --color never "$FIX/meeting-actions.nika" >"$RAW/check-meeting.txt" 2>&1
# The same audit under the model the run below uses. check never dials a
# server, so this one refreshes even where the run cannot.
nika check --color never --model "$MODEL" "$FIX/meeting-actions.nika" \
  >"$RAW/check-meeting-ollama.txt" 2>&1

# The run transcript uses a REAL local model. Only refresh when an Ollama
# server is reachable — otherwise keep the committed snapshot.
if curl -s --max-time 2 http://localhost:11434/api/tags >/dev/null 2>&1; then
  curl -s http://localhost:11434/api/generate \
    -d '{"model":"llama3.2:3b","prompt":"warm","stream":false}' >/dev/null || true
  rm -f action-items.json
  nika run --no-progress --color never --model "$MODEL" \
    "$FIX/meeting-actions.nika" >"$RAW/run-meeting.txt" 2>&1
  cp action-items.json "$RAW/action-items.json"
  rm -f action-items.json
else
  echo "ollama unreachable — keeping committed run-meeting.txt snapshot" >&2
fi

# ── dag-execution ───────────────────────────────────────────────────────
SHOWCASE="crates/nika-pack/pack/examples/pr-review-fanout.nika"
# The path is asserted, not assumed: every capture below redirects stderr INTO
# its artifact, so a missing input does not fail the script — it writes the
# reader an error message where a diagram belongs. This one had been pointing
# at a `showcase/t3-` layout the pack flattened away, and both artifacts
# carried "cannot read ..." instead of a DAG (measured 2026-07-30).
[[ -f "$SHOWCASE" ]] || {
  echo "FATAL: showcase example missing: $SHOWCASE" >&2
  exit 1
}
nika inspect --format mermaid "$SHOWCASE" >"$RAW/graph-fanout.mmd" 2>&1
nika check --color never "$SHOWCASE" >"$RAW/check-fanout.txt" 2>&1

# ── permits-audit ───────────────────────────────────────────────────────
# The escaping fixture MUST fail (the boundary catches it) · the widened
# one MUST be clean with a HARD cost ceiling.
if nika check --color never "$FIX/permits-escape.nika" >"$RAW/check-permits-escape.txt" 2>&1; then
  echo "FATAL: permits-escape fixture unexpectedly passed nika check" >&2
  exit 1
fi
nika check --color never "$FIX/permits-fits.nika" >"$RAW/check-permits-fits.txt" 2>&1
diff -u "$FIX/permits-escape.nika" "$FIX/permits-fits.nika" \
  >"$RAW/permits-fix-diff.txt" 2>&1 || true

# ── on-error-recover ────────────────────────────────────────────────────
# Deterministic + offline: the missing live-rates.json IS the failure; the
# cache task is the fallback. Run from a scratch dir so ./out starts clean.
RECOVER_TMP="$(mktemp -d)"
cp "$FIX/recover-fallback.nika" "$RECOVER_TMP/"
(
  cd "$RECOVER_TMP"
  # Run it by relative name: the CLI echoes the path it was given, and a
  # transcript must not carry this machine's checkout location.
  nika run --no-progress --color never recover-fallback.nika \
    >"$ROOT/$RAW/run-recover.txt" 2>&1
  cp out/rates.json "$ROOT/$RAW/recover-rates.json"
  # The failure the recover absorbed, as the run's trace records it: the
  # task_recovered event's own fields (its ids, times and chain differ on
  # every run, so they stay out of the transcript).
  node -e '
const fs = require("fs");
for (const f of process.argv.slice(1)) {
  for (const l of fs.readFileSync(f, "utf8").split("\n")) {
    if (!l.trim()) continue;
    const e = JSON.parse(l);
    if (e.kind === "task_recovered") console.log(JSON.stringify({ kind: e.kind, fields: e.fields }));
  }
}' .nika/traces/*.ndjson >"$ROOT/$RAW/recover-event.json"
)
rm -rf "$RECOVER_TMP"
nika explain --color never NIKA-EXEC-001 >"$RAW/explain-exec-001.txt" 2>&1

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
