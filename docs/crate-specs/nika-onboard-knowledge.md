# Crate spec — `nika-onboard-knowledge`

| | |
|---|---|
| Status | **MEMBER** (size-cap split of the admitted `nika-onboard` unit · the ADR-137 amendment · D-2026-07-09-N1 · 2026-10-11) |
| Layer | L4 — beside its unit; lateral L4 edge `nika-onboard → nika-onboard-knowledge`, never back · its own edges: `nika-compile` and `nika-compile-seats` (L4, lateral), `nika-event` (L0) |
| Design | the knowledge door: the strict admission of a Foundry knowledge release against a trusted identity, the releases this build embeds, the catalogue a compile is lent and the authoring pack, kept under their historical path `nika_onboard::knowledge` |
| IMPL | measured by `scripts/crate-metrics.sh nika-onboard-knowledge` at each freeze; the crate carries what `nika-onboard` held in `knowledge.rs` and `knowledge/` on 2026-10-11, `knowledge::pin` aside (the gate's own counter: 3,934 prod LOC at the split · 64 tests, moved with their modules) |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 (workspace-inherited) |
| Publish | `false` — member of the `nika-onboard` unit |
| Dependencies | **read from `Cargo.toml`, which is authoritative** · `nika-compile` · `nika-compile-seats` · `nika-event` · `include_dir`, `serde_json`, `rustix` (unix: the collection on held directory descriptors) |
| Features | `test-support` (the synthetic release `fixture`, for other doors' tests; `nika-onboard`'s own `test-support` enables it, so the unit keeps one feature) |
| NIKA codes | none minted here — a refused release is a typed `KnowledgeError` the doors speak with their own codes |

## 1. Purpose

`nika-onboard` reached 14,999 of its 15,000 production lines with the r3 Foundry release and
the families-only stemmer still to land. The knowledge door is one cluster: the strict admission,
the embedded releases, the catalogue and the authoring pack name nothing else in the onboarding
unit but the Compile types it reached through `crate::compile`. It moved here with git history,
with its release assets, build script and pinned vectors; `nika-onboard` re-exports every item at
`knowledge::`, so no consumer changes.

## 2. Surface

- `Snapshot` (open, from_files, pack, catalogue, entries, the identity record), `KnowledgeError`,
  `RefusalCode`, `TrustedIdentity`, `ADMISSION_PROFILE`, `RELEASE_FORMAT`, `PACK_BUILDER`,
  `Catalogue`, `ComponentCatalog`, `pack_sha256`, `pack_from_file`, `redact_host_paths`.
- `bundled` became a public module with `admit` and `identity`: `nika-onboard`'s pin and its
  authoring configuration read them, and `nika-onboard` re-exports it crate-visible only.
- `fixture` (feature `test-support`): the synthetic release the strict door admits.

## 3. Boundary

- One strict admission serves every door; the embedded bytes are admitted against their issued
  identity, never trusted for their own claims.
- Never depends on `nika-onboard` (the layer check refuses the edge back). `knowledge::pin`
  stays in `nika-onboard`, beside `compile_config` and `compile::round`, which it reads.

## 4. Tests

The door's suites moved with their modules (64 in all): the admission vectors (now under
this crate's `tests/knowledge-r1` and `tests/knowledge-r2`, the producer's common pinned vectors),
the embedded releases, the catalogue and its whole-catalogue reach, the pack and the stemmer's
held-out witness. The pin's suite stays in `nika-onboard`.

## 5. Admission evidence

A member of the admitted `nika-onboard` unit: code that moved under its gates, with its tests.
The public API snapshots are rendered on macOS and the Linux job confirms them.

## 6. Related

- `docs/crate-specs/nika-onboard.md` — the unit, and its pointer.
- `docs/adr/adr-137-nika-onboard-member-split-nika-compile.md` — the unit's split, amended with
  this member.
- `docs/architecture/crate-layer-registry.md` — the L4 row.
