# nika-check-wasm

The static half of `nika check`, compiled to the browser.

The spec reserved this seat before the crate existed — conformance Level 2
names « custom engines for specialized environments (embedded · **WASM** ·
custom LLM gateway) » (`spec/07-conformance.md`). This crate is the checker
taking it: the same parser (`nika-schema`), the same judgment hub
(`nika-check`), the same error voice (`SchemaError::spec_code()` + `Display`),
for the legs a browser genuinely has.

## What it covers, and how you know

The verdict names its own coverage **in-band**:

```json
{ "report_version": 1, "wasm": true, "legs": ["PARSE", "CONFORM"],
  "clean": false, "findings": [ { "kind": "parse", "gate": "PARSE", "…": "…" } ] }
```

`wasm: true` + the closed `legs:` list exist so no consumer can mistake the
browser half for the full binary. What WASM cannot do — read a filesystem,
resolve an installed model, launch a process — is exactly what the report's
environmental legs must never claim in a browser. This is the same honesty
line the website applies when its captures drop `kind: inputs` findings:
they describe the machine, not the file.

## Use from JavaScript

The npm package is `@supernovae-st/nika-check-wasm` — the `--target web`
build, published by the engine's release workflow with npm provenance
(the same tarball is attached to the GitHub release with its sha256, so
the registry bytes are verifiable against the release asset).

```js
import init, { check, engine_version } from '@supernovae-st/nika-check-wasm'

await init() // browser: fetches the .wasm beside the glue
const verdict = JSON.parse(check('nika: hello\n'))
verdict.wasm // true — the browser half names itself
verdict.legs // ["PARSE", "CONFORM"] — closed, in-band coverage
verdict.findings // the engine's own rows (code · gate · message · line/col in CHARS)
```

Under Vite, exclude the package from dependency pre-bundling — esbuild's
prebundle rewrites `import.meta.url` and loses the `.wasm` beside the
glue (the one recurring integration bug for web-target builds):

```js
// vite.config.js
export default { optimizeDeps: { exclude: ['@supernovae-st/nika-check-wasm'] } }
```

Node ≥ 18 runs the same artifact — hand it the bytes (Node's `fetch`
refuses `file://`, so the glue's self-loading path is browser-only):

```js
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import init, { check } from '@supernovae-st/nika-check-wasm'

const wasm = import.meta.resolve('@supernovae-st/nika-check-wasm/nika_check_wasm_bg.wasm')
await init({ module_or_path: readFileSync(fileURLToPath(wasm)) })
```

**Scale, measured** (Node, the shipped artifact, 2026-07-28): a 623 KB
workflow of 20 000 tasks checks in ~69 ms; a 4 MB single-string value in
~13 ms. Depth-shaped hostility is refused at the source instead (jq
nesting over 128, bounded suggestion budget) — the profile is
`panic = "abort"`, so the contract is: a refusal is a finding, a trap is
a bug, and a trapped instance is never reused.

## License

[AGPL-3.0-or-later](https://www.gnu.org/licenses/agpl-3.0.html) — the
engine's license; compiling to wasm changes nothing. Using it in your
application makes the AGPL's terms apply to the combined work, including
the network-use provision (section 13). Every published byte is built
from this crate and its workspace at the release tag, and the provenance
attestation binds the npm artifact to that exact commit.

## Build

`./build-wasm.sh` is the canonical recipe — native tests, the
`wasm-release` profile build (the workspace `--release` keeps debuginfo a
served artifact must not carry), `wasm-bindgen` at the lock's minor, and
the `wasm-opt -Oz` pass that takes the artifact 4.6M → 3.1M. An earlier
version of this section hand-listed three commands that had drifted from
the script (wrong profile, wrong path, no wasm-opt) — the Gate-11 review
caught it, and a pointer cannot drift.

## The differential gate (`tests/differential.rs`)

Two legs, two divergence classes, both proven over the whole conformance
corpus (125 fixtures):

- **Leg A · assembly vs library** — always on. `check()` re-run beside the
  same `parse` + `analyze` calls it wraps; every row byte-compared (code ·
  message · gate · span). Catches the only thing this crate can get wrong:
  its own assembly.
- **Leg B · rows vs the CLI** — `NIKA_DIFF_CLI=1` (builds the full CLI, so
  env-gated; CI should wire it). The SAME TREE's `nika-cli` run over every
  fixture, rows filtered to the shared legs (PARSE · CONFORM), compared on
  (gate, code, message). First real run went red on a wrong bin-target name
  and green after — the gate has been seen to bite.

## Owed before any public surface consumes this

1. **The remaining static legs.** COST · SECRETS · TYPES · TOOLS · ARGS ·
   SCHEMA · GATES · PERMITS · TRIFECTA are all pure (`nika-check`'s own
   dependency comments say so, and the wasm build proves them compilable) —
   the CLI's orchestration of them lives in `nika-cli` and must be extracted
   into a shared library seam rather than re-implemented here. Re-assembly in
   two places is the exact divergence class the check↔run oracle exists to
   kill.
2. ~~**The 12-gate admission.**~~ Done — ADR-107 (accepted 2026-07-28), the
   full dossier gate-by-gate, including the three-leg adversarial review
   that found and fixed a real engine bug (the jq nesting bomb) before any
   browser ever saw it.
