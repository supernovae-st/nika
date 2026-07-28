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

## Build

```sh
# native tests (the row shape · the spec codes · the verbatim voice)
cargo test -p nika-check-wasm --lib

# the browser target (the getrandom 0.3 backend needs the cfg at build time)
RUSTFLAGS='--cfg getrandom_backend="wasm_js"' \
  cargo build -p nika-check-wasm --target wasm32-unknown-unknown --release

# bindings (wasm-bindgen-cli · matches the wasm-bindgen minor in Cargo.lock)
wasm-bindgen --target web --out-dir pkg \
  target/wasm32-unknown-unknown/release/nika_check_wasm.wasm
```

`./build-wasm.sh` runs the three in order and refuses politely when a tool is
absent.

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
3. **The 12-gate admission.** This crate rides a branch; joining `main` means
   the ceremony, like every crate before it.
