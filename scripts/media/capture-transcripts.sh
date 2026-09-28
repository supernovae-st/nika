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

# ── workflow-gallery ────────────────────────────────────────────────────
# The gallery bare `nika try` prints: the embedded path and jobs, each with
# its verb glyphs and the one line the CLI shows for it.
nika try --color never >"$RAW/try-gallery.txt" 2>&1

# ── full-loop ───────────────────────────────────────────────────────────
# The README's front door, captured as it runs: compile the offline
# `hello` skeleton, check it, run it (mock/echo: a rehearsal), verify the
# trace. Run from a scratch dir by relative names, so no path of this
# machine reaches a transcript.
LOOP_TMP="$(mktemp -d)"
(
  cd "$LOOP_TMP"
  nika compile --color never hello hello.nika >"$ROOT/$RAW/loop-compile.txt" 2>&1
  cp hello.nika "$ROOT/$RAW/loop-hello.nika"
  nika check --color never hello.nika >"$ROOT/$RAW/loop-check.txt" 2>&1
  nika run --no-progress --color never hello.nika >"$ROOT/$RAW/loop-run.txt" 2>&1
  nika trace verify --color never >"$ROOT/$RAW/loop-verify.txt" 2>&1
)
rm -rf "$LOOP_TMP"

# ── editor-diagnostics ──────────────────────────────────────────────────
# What the editor shows is what `nika lsp` publishes. The diagnostics are
# captured for the broken fixture, for the same file once the `asses`
# typo is fixed (one keystroke), and for the fixed fixture. Only the
# diagnostics are kept: the session itself carries this machine's paths.
LSP_TMP="$(mktemp -d)"
sed 's/tasks\.asses\./tasks.assess./' "$FIX/broken-pr-review.nika" >"$LSP_TMP/typo-fixed.nika"
node - "$FIX/broken-pr-review.nika" "$RAW/lsp-broken.json" \
  "$LSP_TMP/typo-fixed.nika" "$RAW/lsp-typo-fixed.json" \
  "$FIX/fixed-pr-review.nika" "$RAW/lsp-fixed.json" <<'NODE'
const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');
function diagnose(file) {
  return new Promise((resolve, reject) => {
    const abs = path.resolve(file), uri = `file://${abs}`;
    const lsp = spawn('nika', ['lsp'], { stdio: ['pipe', 'pipe', 'ignore'] });
    const send = m => { const s = JSON.stringify(m); lsp.stdin.write(`Content-Length: ${Buffer.byteLength(s)}\r\n\r\n${s}`); };
    const timer = setTimeout(() => { lsp.kill(); reject(new Error(`nika lsp published nothing for ${file}`)); }, 15000);
    let buf = Buffer.alloc(0);
    lsp.stdout.on('data', d => {
      buf = Buffer.concat([buf, d]);
      for (;;) {
        const h = buf.indexOf('\r\n\r\n');
        if (h < 0) return;
        const len = Number(buf.slice(0, h).toString().match(/Content-Length: (\d+)/i)[1]);
        if (buf.length < h + 4 + len) return;
        const msg = JSON.parse(buf.slice(h + 4, h + 4 + len).toString());
        buf = buf.slice(h + 4 + len);
        if (msg.method === 'textDocument/publishDiagnostics' && msg.params.uri === uri) {
          clearTimeout(timer);
          send({ jsonrpc: '2.0', id: 2, method: 'shutdown' });
          send({ jsonrpc: '2.0', method: 'exit' });
          resolve(msg.params.diagnostics);
        }
      }
    });
    send({ jsonrpc: '2.0', id: 1, method: 'initialize', params: { processId: null, rootUri: `file://${path.dirname(abs)}`, capabilities: {} } });
    send({ jsonrpc: '2.0', method: 'initialized', params: {} });
    send({ jsonrpc: '2.0', method: 'textDocument/didOpen', params: { textDocument: { uri, languageId: 'nika', version: 1, text: fs.readFileSync(abs, 'utf8') } } });
  });
}
(async () => {
  const args = process.argv.slice(2);
  for (let i = 0; i < args.length; i += 2) {
    fs.writeFileSync(args[i + 1], `${JSON.stringify(await diagnose(args[i]), null, 1)}\n`);
  }
})().catch(e => { console.error(e.message); process.exit(1); });
NODE
rm -rf "$LSP_TMP"

# ── clips that own their capture ────────────────────────────────────────
# A clip whose story takes several commands owns its capture script in
# scripts/media/capture/: offline, in its own scratch directory, writing
# media/raw/<clip>-*. trace-proof's hashes change on every capture, so
# refreshing it means re-rendering that clip.
bash scripts/media/capture/agent-plugin.sh
bash scripts/media/capture/spec-anatomy.sh
bash scripts/media/capture/trace-proof.sh
bash scripts/media/capture/first-session.sh
bash scripts/media/capture/cost-ceiling.sh
bash scripts/media/capture/approval-gate.sh
# pr-check-comment replays nika-action's own comment renderer. Without a
# checkout of that repository, keep the committed snapshot, as the
# nika-hero run does without an Ollama server.
if [ -f "${NIKA_ACTION:-../nika-action}/action.yml" ]; then
  bash scripts/media/capture/pr-check-comment.sh
else
  echo "no nika-action checkout (NIKA_ACTION) — keeping the committed pr-check-comment captures" >&2
fi
# typescript-client runs the TypeScript package's quick start from a built
# nika-client checkout named by NIKA_CLIENT_DIR. Without one it keeps the
# committed snapshot; a capture changes the run's hashes, so refreshing it
# means re-rendering that clip, as for trace-proof.
bash scripts/media/capture/typescript-client.sh

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
