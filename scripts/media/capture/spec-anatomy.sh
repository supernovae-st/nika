#!/usr/bin/env bash
# spec-anatomy.sh — capture what the spec-anatomy clip draws: the anatomy
# of one .nika file (scripts/media/fixtures/ship-notes.nika), labelled in
# the language's own words as this binary embeds them.
#
#   the envelope keys and what each is for   · nika spec --schema
#   the verbs and their one-line semantic    · nika spec --canon
#   which task uses which verb, its colour   · nika inspect --format mermaid
#   the verdict on that exact file           · nika check
#
# Offline and idempotent. The fixture is checked by relative name from a
# scratch directory, so no path of this machine reaches a capture, and a
# fixture that stops passing `nika check` fails the capture.
#
# Usage · bash scripts/media/capture/spec-anatomy.sh
# Output · media/raw/spec-anatomy.nika (the file checked) and
#          media/raw/spec-anatomy-*.{txt,json,mmd}
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"

RAW="media/raw"
FIXTURE="scripts/media/fixtures/ship-notes.nika"
mkdir -p "$RAW"

command -v nika >/dev/null || {
  echo "nika binary not found on PATH" >&2
  exit 1
}
[[ -f "$FIXTURE" ]] || {
  echo "FATAL: fixture missing: $FIXTURE" >&2
  exit 1
}

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

nika --version >"$RAW/spec-anatomy-version.txt"

# The spec identity line the binary embeds (the language pack it speaks).
nika spec --color never >"$TMP/spec.txt"
sed -n '1p' "$TMP/spec.txt" >"$RAW/spec-anatomy-pack.txt"

# The envelope: the embedded workflow schema's top-level keys, in schema
# order, each with the description the schema gives it.
nika spec --schema --color never >"$TMP/schema.json"
node - "$TMP/schema.json" >"$RAW/spec-anatomy-envelope.json" <<'NODE'
const fs = require('fs');
const schema = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const keys = Object.entries(schema.properties || {}).map(([key, v]) => ({ key, description: v.description || '' }));
if (keys.length === 0) throw new Error('the embedded schema has no top-level keys');
const out = { source: 'nika spec --schema', required: schema.required || [], keys };
process.stdout.write(`${JSON.stringify(out, null, 1)}\n`);
NODE

# The verbs: the canon's verbs section, verbatim.
nika spec --canon --color never >"$TMP/canon.yaml"
awk '/^verbs:/ { on = 1 } on && /^$/ { exit } on' "$TMP/canon.yaml" >"$RAW/spec-anatomy-verbs.txt"
grep -q '^  count: ' "$RAW/spec-anatomy-verbs.txt" || {
  echo "FATAL: no verbs section in nika spec --canon" >&2
  exit 1
}

# The verdict and the task graph, on that exact file. The bytes checked
# are kept beside the verdict: the clip draws them, and refuses to render
# when the fixture no longer matches what was checked.
cp "$FIXTURE" "$TMP/"
cp "$FIXTURE" "$RAW/spec-anatomy.nika"
(
  cd "$TMP"
  if ! nika check --color never ship-notes.nika >"$ROOT/$RAW/spec-anatomy-check.txt" 2>&1; then
    echo "FATAL: ship-notes.nika no longer passes nika check" >&2
    exit 1
  fi
  nika inspect --format mermaid ship-notes.nika >"$ROOT/$RAW/spec-anatomy-graph.mmd" 2>&1
)
grep -q 'run ready ✔' "$RAW/spec-anatomy-check.txt" || {
  echo "FATAL: nika check no longer says ship-notes.nika is run ready" >&2
  exit 1
}

echo "spec-anatomy captured:" "$RAW"/spec-anatomy*
