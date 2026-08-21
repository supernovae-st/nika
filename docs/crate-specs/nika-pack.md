# Crate spec — `nika-pack`

| | |
|---|---|
| Status | **ADMITTED 2026-06-11** (`5f37637c3` · via PR from `feat/nika-pack`) · embedded language pack — spec snapshot + manifest hashes |
| Layer | L0 — pure data · zero I/O · zero async · no error enum (total accessors) |
| Design | `include_dir!` snapshot of the spec pack + total accessor functions |
| LOC budget | ≤300 src (actual ~170) |
| Crate version | tracks workspace · embedded `pack_version()` tracks the SPEC (`0.1.0-draft`) |
| License | `AGPL-3.0-or-later` (the embedded pack files are Apache-2.0 · spec repo) |
| Publish | `false` |
| NIKA codes | none — accessors return `Option`/defaults · integrity is enforced by tests |

---

## 1. Purpose

The binary carries the language artifacts of **its** version: spec prose,
JSON Schema, canon registry, the 27+ canonical examples, the 6 instantiable
templates and the manifest that hashes them (nika-spec README §The examples
pack). `nika try` / `nika new` / `nika spec` read this crate — offline, version-locked, verifiable.

## 2. Public API

```rust
pub fn pack_version() -> &'static str;              // the spec VERSION
pub fn manifest() -> &'static str;                  // examples/manifest.yaml
pub fn canon() -> &'static str;                     // canon.yaml
pub fn schema_json() -> &'static str;               // workflow.schema.json
pub fn quickstart() -> &'static str;
pub fn example(slug: &str) -> Option<&'static str>; // foundation + showcase/
pub fn example_slugs() -> Vec<String>;
pub fn template(name: &str) -> Option<&'static str>;
pub fn template_names() -> Vec<String>;
pub fn doc(path: &str) -> Option<&'static str>;     // spec/*.md · stdlib/*.md
pub fn doc_paths() -> Vec<String>;
pub fn lean(yaml: &str) -> &str;                    // the hashed/rendered range
```

## 3. The snapshot + sync

`pack/` is vendored by `scripts/sync-pack.sh <spec-checkout-at-SPEC_PIN>`;
the mapping in that script is the complete copy contract.
Bumping the pack = run the script, commit the diff. The pack version MUST
equal the spec tag the engine targets — `version_is_nonempty_and_matches_manifest`
plus the per-file sha tests turn a stale or tampered snapshot into a red
`cargo test`, never a silent ship.

## 4. Tests (integrity = the contract)

- `every_manifest_entry_resolves_and_hashes_clean` — re-hashes the LEAN text
  of all 33 hashed files (27 workflows + 6 templates) against the manifest's
  `sha256_16` (the same bytes docs and website render · the Rust `lean()`
  ports the projector's, including the one-trailing-newline invariant).
- `manifest_and_embedded_sets_are_bijective` — entry count must equal
  embedded workflows + templates · drift in either direction fails.
- Known gap (accepted) · non-manifest pack files (`spec/*.md` · `stdlib/*.md`
  · `canon.yaml` · `QUICKSTART.md` · the schema) are content-asserted but not
  hash-pinned — the manifest only hashes workflows/templates by design; a
  future spec-side manifest extension would close it at the source.
- `surface_counts_hold` — 6 templates · ≥27 examples · ≥12 doc pages ·
  schema/canon/quickstart non-empty.
- `lean_strips_the_banner_and_nothing_else`.

## 5. Gate exemptions (per ADR-003 · documented justification)

- **Gate 6 PROPERTY · exempt** — pure compile-time data accessors · no
  parser/encoding/security surface · `lean()` is covered by directed tests
  including the newline-normalization invariant · property fuzzing would
  exercise `include_dir`'s lookup, not our logic.
- **Gate 7 BENCHMARKS · exempt** — no hot path (CLI cold-read surface).
- **Gate 9 CANARY · exempt** — data-only crate · no executable workflow
  path of its own · the embedded examples ARE conformance-gated spec-side
  on every spec push.
- **Gate 10 PARITY · exempt** — new surface · no legacy equivalent in
  brouillon to golden-test against.

## 6. Dependencies

`include_dir` (workspace-pinned 0.7 · build-time embedding) · `sha2`
(workspace-pinned 0.10 · dev-only integrity hash). `[lints] workspace =
true` — the full deny-set applies. Nothing else.
