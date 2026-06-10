# Crate spec — `nika-pack`

| | |
|---|---|
| Status | **proposal · feat/nika-pack branch** (admission via the normal 12-gate ceremony) |
| Layer | L0 — pure data · zero I/O · zero async · no error enum (total accessors) |
| Design | `include_dir!` snapshot of the spec pack + total accessor functions |
| LOC budget | ≤300 src (actual ~170) |
| Crate version | tracks workspace (`0.80.0`) · embedded `pack_version()` tracks the SPEC (`0.1.0-draft`) |
| License | `AGPL-3.0-or-later` (the embedded pack files are Apache-2.0 · spec repo) |
| Publish | `false` |
| NIKA codes | none — accessors return `Option`/defaults · integrity is enforced by tests |

---

## 1. Purpose

The binary carries the language artifacts of **its** version: spec prose,
JSON Schema, canon registry, the 27+ canonical examples, the 6 instantiable
templates and the manifest that hashes them (nika-spec README §The examples
pack). `nika examples` / `nika docs` / `nika schema` / `nika new` (CLI verbs
follow as additive nika-cli subcommands) read this crate — offline, version-locked, verifiable.

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

`pack/` is vendored by `scripts/sync-pack.sh <spec-checkout>` (51 files).
Bumping the pack = run the script, commit the diff. The pack version MUST
equal the spec tag the engine targets — `version_is_nonempty_and_matches_manifest`
plus the per-file sha tests turn a stale or tampered snapshot into a red
`cargo test`, never a silent ship.

## 4. Tests (integrity = the contract)

- `every_manifest_entry_resolves_and_hashes_clean` — re-hashes the LEAN text
  of all 33 workflows against the manifest's `sha256_16` (the same bytes the
  docs and the website render · the Rust `lean()` ports the projector's).
- `surface_counts_hold` — 6 templates · ≥27 examples · ≥12 doc pages ·
  schema/canon/quickstart non-empty.
- `lean_strips_the_banner_and_nothing_else`.

## 5. Dependencies

`include_dir 0.7` (runtime · widely used · build-time embedding only) ·
`sha2 0.10` (dev-only · the integrity hash). Nothing else.
