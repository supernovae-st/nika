#!/usr/bin/env bash
# typescript-client.sh — capture the quick start of the TypeScript package
# (@supernovae-st/nika, repository supernovae-st/nika-client) as a Node app
# runs it, for the typescript-client clip. Every line the clip shows of the
# program, its output, the engine's check, the run's journal, the verifier
# and the TypeScript types is read from what this script writes.
#
# Usage · NIKA_CLIENT_DIR=/path/to/nika-client bash scripts/media/capture/typescript-client.sh
#   NIKA_CLIENT_DIR is a nika-client checkout with its dependencies
#   installed (npm ci) and its package built (dist/). This repository cannot
#   vendor the package: without the checkout the committed snapshot stays.
# Output · media/raw/typescript-client-*
#
# What it does, in a scratch project with a scratch HOME (so the signing key
# it mints never reaches a real HOME or keychain):
#   1. node_modules/@supernovae-st/nika links to the checkout, so the
#      README's `import … from '@supernovae-st/nika'` resolves to it, and
#      node_modules/.bin/nika is its CLI, as npm installs it;
#   2. hello.nika and demo.mts are the README's quick-start text, byte for
#      byte, and `./node_modules/.bin/nika key init` runs first (step 1);
#   3. `node demo.mts` runs; its stdout+stderr is the transcript;
#   4. the same engine then reads the one trace that run wrote: `nika trace
#      verify` (the verifier traceVerify() itself runs), and the journal is
#      kept as a chain (each line's kind and hash); `nika check` audits the
#      file;
#   5. TypeScript's language service types the program: its diagnostics,
#      the inlay types an editor shows after a declaration, a few hovers.
# The engine is the one the package bundles (what npm users get). Without a
# bundled payload the package is pointed at the `nika` on PATH through
# NIKA_BIN, and typescript-client-versions.json says so.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"

RAW="$ROOT/media/raw"
OUT="$RAW/typescript-client"

if [[ -z "${NIKA_CLIENT_DIR:-}" || ! -d "${NIKA_CLIENT_DIR}" ]]; then
  echo "NIKA_CLIENT_DIR unset or not a directory — keeping the committed typescript-client snapshot" >&2
  exit 0
fi
CLIENT="$(cd "$NIKA_CLIENT_DIR" && pwd)"
for f in package.json README.md dist/index.js dist/bin/nika.js; do
  [[ -f "$CLIENT/$f" ]] || {
    echo "FATAL: $CLIENT/$f missing — NIKA_CLIENT_DIR must be a built nika-client checkout (npm ci && npm run build)" >&2
    exit 1
  }
done
for d in node_modules/typescript node_modules/@types/node; do
  [[ -d "$CLIENT/$d" ]] || {
    echo "FATAL: $CLIENT/$d missing — the checkout needs its dependencies (npm ci)" >&2
    exit 1
  }
done
command -v node >/dev/null || {
  echo "node not found on PATH" >&2
  exit 1
}
mkdir -p "$RAW"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
APP="$WORK/app"
mkdir -p "$WORK/home" "$APP/node_modules/@supernovae-st" "$APP/node_modules/.bin"
ln -s "$CLIENT" "$APP/node_modules/@supernovae-st/nika"
ln -s ../@supernovae-st/nika/dist/bin/nika.js "$APP/node_modules/.bin/nika"

# The key and everything else the engine keeps per user stay in scratch:
# a scratch HOME, and file custody instead of an OS keychain.
export HOME="$WORK/home"
export NIKA_KEYCHAIN=off
unset NIKA_RUN_KEY_FILE NIKA_RUN_PUB_FILE NIKA_BIN

# ── the README's quick start, byte for byte ─────────────────────────────
node - "$CLIENT/README.md" "$APP" <<'NODE'
const fs = require('fs');
const [readme, app] = process.argv.slice(2);
const lines = fs.readFileSync(readme, 'utf8').split('\n');
// the first fenced block of `lang` after the line that starts with `step`
const block = (step, lang) => {
  const s = lines.findIndex(l => l.startsWith(step));
  if (s < 0) throw new Error(`README: no "${step}"`);
  const open = lines.findIndex((l, i) => i > s && l === '```' + lang);
  const close = lines.findIndex((l, i) => i > open && l === '```');
  if (open < 0 || close < 0) throw new Error(`README: no \`\`\`${lang} block after "${step}"`);
  return lines.slice(open + 1, close).join('\n') + '\n';
};
fs.writeFileSync(`${app}/hello.nika`, block('**2 · Write a workflow.**', 'yaml'));
fs.writeFileSync(`${app}/demo.mts`, block('**3 · Run it and verify it from TypeScript.**', 'ts'));
// what the README says the program prints (checked against the run below)
fs.writeFileSync(`${app}/.readme-output.txt`, block('node demo.mts', 'text'));
NODE

# ── the engine the package resolves ─────────────────────────────────────
# Asked of the package itself, never assumed: a local client names the
# engine it will spawn (bundled payload, or NIKA_BIN).
engine_of() {
  (cd "$APP" && node --input-type=module -e '
import { Nika, NikaEngineUnavailable } from "@supernovae-st/nika";
try {
  const engine = new Nika({ cwd: process.cwd() }).transport?.options?.engine;
  if (!engine || typeof engine.bin !== "string") throw new Error("the package did not name the engine it resolved");
  console.log(JSON.stringify(engine));
} catch (e) {
  if (e instanceof NikaEngineUnavailable) console.log(JSON.stringify({ unavailable: e.message }));
  else throw e;
}')
}
ENGINE_JSON="$(engine_of)"
if [[ "$ENGINE_JSON" == *'"unavailable"'* ]]; then
  command -v nika >/dev/null || {
    echo "FATAL: the checkout bundles no engine for this host and no nika is on PATH" >&2
    exit 1
  }
  export NIKA_BIN
  NIKA_BIN="$(command -v nika)"
  echo "no bundled engine for this host — pointing the package at $NIKA_BIN (NIKA_BIN)" >&2
  ENGINE_JSON="$(engine_of)"
fi
ENGINE="$(node -e 'process.stdout.write(JSON.parse(process.argv[1]).bin)' "$ENGINE_JSON")"
[[ -x "$ENGINE" ]] || {
  echo "FATAL: the resolved engine is not executable: $ENGINE" >&2
  exit 1
}

# ── step 1 · the signing key, then the program ──────────────────────────
cd "$APP"
./node_modules/.bin/nika key init >/dev/null
status=0
node demo.mts >output.txt 2>&1 || status=$?
cp output.txt "$OUT-output.txt"
cp demo.mts "$OUT-demo.mts"
cp hello.nika "$OUT-hello.nika"
if [[ "$status" -ne 0 ]]; then
  echo "FATAL: node demo.mts exited $status:" >&2
  cat output.txt >&2
  exit 1
fi
PROMPT="$(sed -n 's/^ *prompt: "\(.*\)"$/\1/p' hello.nika)"
grep -qxF "mock(echo) · $PROMPT" output.txt || {
  echo "FATAL: the output no longer prints the greeting (mock(echo) · $PROMPT)" >&2
  exit 1
}
grep -qxF "receipt verified" output.txt || {
  echo "FATAL: the output no longer says \"receipt verified\":" >&2
  cat output.txt >&2
  exit 1
}
cmp -s output.txt .readme-output.txt ||
  echo "note: the output differs from the README's documented output" >&2

# ── what the engine says about that run and that file ──────────────────
shopt -s nullglob
traces=(.nika/traces/*.ndjson)
shopt -u nullglob
[[ "${#traces[@]}" -eq 1 ]] || {
  echo "FATAL: expected the one trace the run wrote, found ${#traces[@]}" >&2
  exit 1
}
TRACE="${traces[0]}"
# the verifier traceVerify() runs first, with its own arguments
"$ENGINE" trace verify "$TRACE" --plain >"$OUT-verify.txt" 2>&1
"$ENGINE" check --color never hello.nika >"$OUT-check.txt" 2>&1
# the journal as a chain: each line's kind, the `chain` field it carries
# (the sha256 of the line before it, a genesis tag's hash first) and the
# sha256 of its own bytes; the last one is the head the verifier prints.
# No payloads, ids or timestamps.
# shellcheck disable=SC2016  # JavaScript template literals: no shell expansion wanted
node -e '
const fs = require("fs");
const crypto = require("crypto");
const sha = s => crypto.createHash("sha256").update(s, "utf8").digest("hex");
const lines = fs.readFileSync(process.argv[1], "utf8").split("\n").filter(l => l.trim());
const rows = lines.map(l => { const e = JSON.parse(l); return { kind: e.kind, chain: e.chain, hash: sha(l) }; });
rows.forEach((r, i) => {
  if (i && r.chain !== rows[i - 1].hash) throw new Error(`journal line ${i + 1} does not chain to line ${i}`);
});
process.stdout.write(JSON.stringify(rows, null, 1) + "\n");
' "$TRACE" >"$OUT-journal.json"
HEAD="$(sed -n 's/^OK — .* head \([0-9a-f]\{64\}\)$/\1/p' "$OUT-verify.txt")"
node -e '
const rows = require(process.argv[1]);
if (rows[rows.length - 1].hash !== process.argv[2]) throw new Error("the verified head is not the hash of the journal'"'"'s last line");
' "$OUT-journal.json" "$HEAD"

# ── the program's types, as TypeScript reports them ─────────────────────
node - "$CLIENT" "$APP" >"$OUT-types.json" <<'NODE'
const fs = require('fs');
const path = require('path');
const [client, app] = process.argv.slice(2);
const ts = require(path.join(client, 'node_modules/typescript'));
const file = path.join(app, 'demo.mts');
const src = fs.readFileSync(file, 'utf8');
// Node 22 runs .mts as an ES module: NodeNext, strict, with Node's types
const options = {
  target: ts.ScriptTarget.ES2022,
  module: ts.ModuleKind.NodeNext,
  moduleResolution: ts.ModuleResolutionKind.NodeNext,
  strict: true,
  noEmit: true,
  types: ['node'],
  typeRoots: [path.join(client, 'node_modules/@types')],
};
const host = {
  getScriptFileNames: () => [file],
  getScriptVersion: () => '1',
  getScriptSnapshot: f => (fs.existsSync(f) ? ts.ScriptSnapshot.fromString(fs.readFileSync(f, 'utf8')) : undefined),
  getCurrentDirectory: () => app,
  getCompilationSettings: () => options,
  getDefaultLibFileName: o => ts.getDefaultLibFilePath(o),
  fileExists: ts.sys.fileExists,
  readFile: ts.sys.readFile,
  readDirectory: ts.sys.readDirectory,
  directoryExists: ts.sys.directoryExists,
  getDirectories: ts.sys.getDirectories,
  realpath: ts.sys.realpath,
};
const ls = ts.createLanguageService(host, ts.createDocumentRegistry());
const sf = ts.createSourceFile(file, src, ts.ScriptTarget.ES2022);
const lineCol = pos => {
  const lc = ts.getLineAndCharacterOfPosition(sf, pos);
  return { line: lc.line + 1, column: lc.character + 1 };
};
const diagnostics = [...ls.getSyntacticDiagnostics(file), ...ls.getSemanticDiagnostics(file)]
  .map(d => ts.flattenDiagnosticMessageText(d.messageText, '\n'));
// the variable types an editor shows inline (inlay hints), as text
const inlayHints = ls.provideInlayHints(file, { start: 0, length: src.length }, {
  includeInlayVariableTypeHints: true,
  includeInlayParameterNameHints: 'none',
}).map(h => ({ ...lineCol(h.position), kind: h.kind, text: h.text ?? (h.displayParts || []).map(p => p.text).join('') }));
// the hover on `word` inside the first occurrence of `context`
const hover = (context, word) => {
  const at = src.indexOf(context);
  if (at < 0) throw new Error(`demo.mts no longer contains ${context}`);
  const pos = at + context.indexOf(word);
  const q = ls.getQuickInfoAtPosition(file, pos);
  if (!q) throw new Error(`TypeScript has no quick info for ${word} in ${context}`);
  return { ...lineCol(pos), word, quickInfo: ts.displayPartsToString(q.displayParts) };
};
process.stdout.write(JSON.stringify({
  typescript: ts.version,
  options: 'target ES2022 · module NodeNext · strict · types node',
  diagnostics,
  inlayHints,
  hovers: [
    hover('const result = await', 'result'),
    hover('result.outputs?.greeting', 'greeting'),
    hover('nika.traceVerify(', 'traceVerify'),
  ],
}, null, 1) + '\n');
NODE

# ── which package and which engine ran ─────────────────────────────────
node - "$CLIENT" "$ENGINE_JSON" "$TRACE" "$OUT-types.json" >"$OUT-versions.json" <<'NODE'
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { execFileSync } = require('child_process');
const [client, engineJson, trace, typesFile] = process.argv.slice(2);
const engine = JSON.parse(engineJson);
const pkg = JSON.parse(fs.readFileSync(path.join(client, 'package.json'), 'utf8'));
const identity = JSON.parse(execFileSync(engine.bin, ['--sdk-identity'], { encoding: 'utf8' }));
const started = JSON.parse(fs.readFileSync(trace, 'utf8').split('\n')[0]);
const ranWith = (started.fields || []).find(f => f.key === 'engine_version')?.value;
if (ranWith !== identity.engineVersion) throw new Error(`the run's journal names engine ${ranWith}, the resolved engine is ${identity.engineVersion}`);
const bundled = typeof engine.packageRoot === 'string';
const out = {
  package: { name: pkg.name, version: pkg.version },
  engine: {
    source: bundled ? 'bundled' : 'NIKA_BIN',
    ...(bundled ? {} : { note: 'no bundled engine for this host: NIKA_BIN pointed the package at the nika on PATH' }),
    version: identity.engineVersion,
    build: identity.buildSha,
    run_journal_says: ranWith,
  },
  node: process.version,
  typescript: JSON.parse(fs.readFileSync(typesFile, 'utf8')).typescript,
  command: 'node demo.mts',
  signing_key: 'nika key init in a scratch HOME (NIKA_KEYCHAIN=off: file custody)',
};
if (bundled) {
  const manifest = JSON.parse(fs.readFileSync(path.join(engine.packageRoot, 'package.json'), 'utf8'));
  const integrity = JSON.parse(fs.readFileSync(path.join(engine.packageRoot, 'INTEGRITY.json'), 'utf8'));
  const sha = crypto.createHash('sha256').update(fs.readFileSync(engine.bin)).digest('hex');
  out.engine.payload = { name: manifest.name, version: manifest.version, sha256: sha, sha256_matches_integrity: sha === integrity.executable?.sha256 };
  const source = path.join(engine.packageRoot, 'SOURCE.json');
  if (fs.existsSync(source)) {
    const s = JSON.parse(fs.readFileSync(source, 'utf8'));
    out.engine.payload.release = { tag: s.tag, commit: s.commit };
  }
}
process.stdout.write(JSON.stringify(out, null, 1) + '\n');
NODE

cd "$ROOT"
# no path of this machine may reach a transcript
if grep -l -e "$WORK" -e "$CLIENT" "$OUT"-* >/dev/null 2>&1; then
  echo "FATAL: a machine path leaked into a typescript-client capture" >&2
  exit 1
fi
# shellcheck disable=SC2016  # a JavaScript template literal: no shell expansion wanted
echo "typescript-client captured · $(node -e 'const v = require(process.argv[1]); console.log(`${v.package.name} ${v.package.version} · engine ${v.engine.version} (${v.engine.source})`)' "$OUT-versions.json")"
