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

## Owed before any public surface consumes this

1. **The differential gate.** A finding this crate emits must equal the
   binary's `check --json` finding for the same file, byte for byte, on the
   legs both run. The claim holds by construction today (same `SchemaError`,
   same `Display`, same `spec_code()`) — but *held by construction* is how
   drift starts. The gate runs both over the conformance fixture corpus and
   diffs the rows; it does not exist yet, and this crate must not be wired
   into nika.sh before it does.
2. **The remaining static legs.** COST · SECRETS · TYPES · TOOLS · ARGS ·
   SCHEMA · GATES · PERMITS · TRIFECTA are all pure (`nika-check`'s own
   dependency comments say so, and the wasm build proves them compilable) —
   the CLI's orchestration of them lives in `nika-cli` and must be extracted
   into a shared library seam rather than re-implemented here. Re-assembly in
   two places is the exact divergence class the check↔run oracle exists to
   kill.
3. **The 12-gate admission.** This crate rides a branch; joining `main` means
   the ceremony, like every crate before it.
