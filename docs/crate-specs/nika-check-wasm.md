# Crate spec — `nika-check-wasm` (WIP · pre-admission)

| | |
|---|---|
| Status | **WIP** (branch `feat/check-wasm`) — pre-admission; this sheet is Gate 1 of the ceremony, written before the vote, and the readiness table lives in ADR-107 (proposed). |
| Layer | **L4** — a transport surface (wasm-bindgen) over the L0 judgment crates, the same seat `nika-lsp`/`nika-mcp` hold: it exposes, it never judges. Deps `nika-schema` (parse · `SchemaError::spec_code()`/`Display` — the one error voice) + `nika-check` (`analyze` · `ERROR_DOCS_BASE` · `REPORT_VERSION`) + `serde_json` + `wasm-bindgen`. |
| Design | The **static half of `nika check`, compiled to the browser** — the seat the spec reserved (`07-conformance.md` Level 2 · « custom engines for specialized environments (embedded · WASM) »). `check(source) -> String` returns the verdict as JSON: findings in the CLI's own `--json` row shape (kind · gate · severity · message · code · docs_url · span), plus `line`/`col` in CHARACTERS computed beside the source (`line_col` · a JS consumer must never re-derive them from the byte span — the two arithmetics disagree the moment a multi-byte character sits left of the caret). Coverage is IN-BAND: every verdict carries `wasm: true` and the closed `legs: ["PARSE", "CONFORM"]`, so the browser half can never claim the binary's reach. |
| Determinism | Same file, same verdict — the linked getrandom/uuid `js` doors exist for graph unification only; no judgment path consumes entropy. |
| Name | `nika-check-wasm` — the crate it compiles (`nika-check`) plus the target that changes everything about what it may claim. |
| LOC | ~234 LOC src (`scripts/crate-metrics.sh --loc nika-check-wasm` · ±15% band per vector 6). |
| Deps | `nika-schema`, `nika-check`, `serde_json`, `wasm-bindgen`; wasm32-only feature doors `uuid/js`, `getrandom` 0.2+0.3. dev: `proptest`. |
| Publish | `false` — ships as a built pkg (wasm-bindgen `--target web`), vendored by consumers with a PROVENANCE.json (nika.sh `src/lib/check-wasm/` is the first). |

## 1 · Why this crate exists

The playground problem: nika.sh's `/play` judged files with a 13-code
TypeScript approximation of a 96-code checker, under a site law that forbids
hand-written verdicts. The honest fix is not a better port — it is the real
parser and the real analyzer, compiled, with their coverage stated in-band.
One crate, zero I/O, zero clock, zero process: what WASM cannot do is exactly
what the report's environmental legs must never claim in a browser.

## 2 · The gates that hold it (differential, both proven red first)

- **Leg A** (`tests/differential.rs` · always on): `check()`'s assembly
  re-run beside the same `parse` + `analyze` calls it wraps, every row
  byte-compared (code · message · gate · span) over the 125-fixture
  conformance corpus.
- **Leg B** (`NIKA_DIFF_CLI=1`): rows vs the SAME TREE's `nika-cli check
  --json` on the shared legs, compared on (gate, code, message).
- **Consumer side** (nika.sh `check-wasm-oracle.test.ts`): the vendored
  artifact re-judges the served hero twins against the CLI-captured truth —
  message byte-equal, line/col landing on the same square from two
  independent arithmetics.

## 3 · Known residue (owned)

- The artifact links the judgment hub's heavy corners (`jaq` · `jsonschema`)
  into a parse+conform build — 3.1 M raw after `wasm-opt -Oz`. The named
  diet is feature-gating them out of `nika-check`; engine surgery, a later
  wave.
- The remaining static legs (COST · SECRETS · TYPES · TOOLS · ARGS · SCHEMA ·
  GATES · PERMITS · TRIFECTA) are pure and wasm-compilable, but their
  orchestration lives in `nika-cli` and must be EXTRACTED into a shared seam,
  never re-assembled here — re-assembly in two places is the divergence class
  the check↔run oracle exists to kill, and leg B is its tripwire.
