---
id: ADR-085
title: "spec/workflow.schema.json invoke.tool oneOf · interim hand-derived bridge to engine schemars codegen"
status: accepted
date: 2026-05-27
phase: "Phase 2 M2"
deciders: ["@ThibautMelen"]
tags: ["nika-schema", "nika-catalog", "spec", "json-schema", "lsp", "yaml-language-server", "codegen", "schemars-future"]
affects_crates: ["nika-schema", "nika-catalog"]
affects_layers: ["L0"]
supersedes: []
superseded_by: []
related: ["ADR-082", "ADR-084", "ADR-086", "ADR-087", "ADR-088", "ADR-089"]
requires: ["ADR-084"]
enables: []
amends: []
fci: ["FCI-001"]
inv: ["INV-019", "INV-025"]
shadow_zones: []
---

# ADR-085 · spec workflow.schema.json oneOf · bridge to schemars codegen

## Context

`nika/spec/schemas/workflow.schema.json` ships the structural contract
for `nika: v1` workflow documents · consumed by JSON-Schema-aware tools
(IDE plugins · yaml-language-server · CI validators · 3rd-party
authoring tooling).

Pre-this-ADR · the `invoke.tool` field was a permissive regex pattern ·

```json
"tool": {
  "type": "string",
  "pattern": "^(nika|mcp):[a-z][a-z0-9_]*(/[a-z][a-z0-9_]*)*$"
}
```

This permits ANY `nika:<name>` string. yaml-language-server (the canonical
LSP backbone for VS Code · IntelliJ · Neovim) **cannot autocomplete from
regex patterns** · only from closed `enum` arrays per JSON-Schema 2020-12
spec. The cookbook author writes `tool: nika:` and gets ZERO completion
suggestions · friction the new-author surface absorbs disproportionately.

The G1 LSP-readiness research (per `nika-lsp-readiness-design-v0.1` ·
D-2026-05-27-N2) established the central theorem · « one obvious way »
(D-2026-05-22-N6 stdlib-collapse 42→26) IS « LSP-perfect » (closed
finite enumerable set) · they're the same property at different layers.
The catalog reconciliation (ADR-084) closed the engine-side · this ADR
closes the spec-side.

## Decision

`nika/spec/schemas/workflow.schema.json` `invoke.tool` field splits from
the single regex pattern into a `oneOf` ·

- **nika branch** · CLOSED enum of 26 canonical builtins (sorted
  alphabetically from `nika/spec/stdlib/builtins-v0.1.md` ·
  `nika:assert` · `nika:cost` · ... · `nika:write`). Enables
  yaml-language-server LSP-perfect autocomplete for any cookbook author.
- **mcp branch** · KEEPS the open regex pattern (`^mcp:[a-z][a-z0-9_]*
  (/[a-z][a-z0-9_]*)*$`) · MCP tool surface is external + plugin-dynamic ·
  closed-enum is structurally impossible (each MCP server publishes its
  own dynamic catalog at runtime).

Engine companion · `nika-schema::codegen::nika_builtin_tool_enum_schema()`
emits the same enum **derived from** `nika_catalog::ALL_BUILTINS` at
call time · the no-drift gate (spec hand-derived → engine catalog
canonical) is enforced by `nika-schema/src/codegen.rs::tests::
enum_matches_catalog_one_to_one`.

## Why interim hand-derived (NOT engine-codegen output today)

Per LOCK-031 trigger-gate discipline (no infra behind a locked gate)
+ scope-shrink (canon NOW · code DEFER per consumer signal) ·

- **LSP value is operator-immediate** · the hand-derived oneOf gives
  yaml-language-server consumers 100% of the LSP-completion value
  TODAY · zero engine-ship dependency · zero schemars adoption ·
  zero `nika-schema-codegen` xtask required.
- **Schemars adoption requires nika-schema parser maturation** · the
  full G1 ratchet (`WorkflowDocument` Rust type with `#[derive(JsonSchema)]`
  + `#[schemars(schema_with = ...)]` on the `tool` field) requires
  the WIP nika-schema parser scaffolding to reach a complete
  `WorkflowDocument` shape · today nika-schema ships parser + raw + types
  modules but the canonical document-level type is still maturing.
- **The bridge is the right shape** · per the schema file header comment
  (« INTERIM hand-derived schema · prose spec is the single source of
  truth · regenerate + diff against engine `nika-schema` at GA ») · the
  decision was always to hand-derive then codegen · this ADR locks the
  transition mechanic.

## Future ratchet (trigger-gated)

The full schemars-adoption G1 codegen (the « next ADR-086 » slot) ships
when ALL of these trigger gates clear ·

1. **nika-schema parser maturity** · `WorkflowDocument` is the canonical
   document-level Rust type · all sub-types (`Task` · `Verb` · `Invoke` ·
   `Agent` · etc.) admitted into the public surface.
2. **Cookbook `$schema` modeline adoption** · ≥1 example in
   `nika/spec/examples/` carries `# yaml-language-server: $schema=...`
   pointing to `workflow.schema.json` · proving the LSP-consumer path
   is live.
3. **LSP UX feedback signal** · ≥1 explicit operator-side incident OR
   feature request citing « completion missed builtin X » OR
   « workflow schema autocomplete is the killer feature ».
4. **schemars 1.0 in workspace.dependencies** · today absent · added
   only when the full `WorkflowDocument` derive is shippable.

When all 4 clear · the future ADR-086 (or ADR-085 extension) authors ·

- `crates/nika-schema-codegen/` xtask binary (or `cargo run --bin
  nika-schema-codegen`) that emits `workflow.schema.json` from
  `schema_for!(WorkflowDocument)`.
- `nika-schema::codegen::nika_builtin_tool_enum_schema` becomes the
  `schema_with` adapter consumed by schemars during the derive.
- The hand-derived `nika/spec/schemas/workflow.schema.json` becomes a
  CHECK-IN CODEGEN OUTPUT (regenerable + diff-able · per Diamond Rule 7
  hallucination forbidden + Self-Evolving Olympus PILLAR 1 projection-
  by-default).
- Hygiene vector `check-spec-schema-codegen-parity` (P1/fail) verifies
  spec/schemas/workflow.schema.json matches the engine codegen output
  at commit time · structural no-drift gate.

## Consequences

### Positive

- **LSP value lands now** · yaml-language-server autocompletes the 26
  builtins · zero engine dependency · operator wins immediately.
- **No-drift property structural at engine layer** ·
  `nika_builtin_tool_enum_schema()` reads `ALL_BUILTINS` at call time ·
  catalog mutations propagate automatically · the 8 conformance tests
  in `codegen.rs` enforce parity (26 entries · alphabetically sorted ·
  prefixed `nika:` · matches catalog · includes canonical anchors ·
  excludes legacy post-D-N6).
- **Bridge is publicly documented** · the schema file header + this ADR
  + `nika-schema::codegen` module doc all cross-reference the future
  ratchet · no surprise transition when ADR-086 ships.
- **Symbol seam established** · `pub fn nika_builtin_tool_enum_schema`
  is the seam future schemars adoption plugs into (`schema_with`
  attribute target) · the migration is mechanical when triggered.

### Negative

- **Spec ↔ engine sync is manual today** · if `ALL_BUILTINS` mutates
  (e.g. spec adds 27th builtin via a future D-lock) · the spec schema's
  hand-derived enum must be regenerated manually · the engine no-drift
  test catches the catalog side but NOT the schema-file side until the
  future hygiene vector ships.
  - Mitigation · spec changes to builtins are extremely rare (governed
    by D-locks per `nika/spec/stdlib/builtins-v0.1.md` § Promotion
    criteria · the canon is « one obvious way » per D-N6).
  - Mitigation · `nika-schema::codegen::nika_builtin_tool_enum_schema`
    PRINTS the canonical JSON-Schema fragment a maintainer can paste
    into the spec schema (zero invention · just copy).
- **Schemars dep deferred** · the full G1 codegen ratchet ships only
  when nika-schema parser matures + LSP UX signal · this is intentional
  scope-shrink, not weakness.

### Neutral

- **L0 layer constraint preserved** · `codegen.rs` is pure-Rust ·
  zero I/O · zero async · `serde_json` already in nika-schema deps ·
  no new crate · no L0 violation.
- **Tests · +8 net new** in `nika-schema/src/codegen.rs::tests` ·
  enum_has_26_entries · enum_alphabetically_sorted · enum_all_prefixed_with_nika ·
  enum_matches_catalog_one_to_one · schema_is_string_type · schema_has_description ·
  enum_includes_canonical_anchors · enum_excludes_legacy_post_d_n6.

## Alternatives considered

### A · Generate workflow.schema.json from schemars NOW

REJECTED · requires `WorkflowDocument` Rust type maturation (nika-schema
WIP) · scope creep + premature surface lock · LSP value lands without
this · scope-shrink discipline applies.

### B · Keep regex pattern · accept zero LSP completion

REJECTED · the LSP-perfection theorem (D-2026-05-27-N2) established that
« one obvious way » + closed-enum-schema = SAME PROPERTY · this ADR
materializes the property at the spec surface.

### C · Generate the schema fragment in a build.rs at engine build time

REJECTED · engine build artifacts don't propagate to the spec submodule ·
the spec is a separate distribution · build.rs would be invisible to
external schema consumers. The future codegen xtask (ADR-086) writes
the file at the spec submodule path · explicit + checked-in.

## Gate verification

This is a refactor of admitted-crate data + types (not a new crate
admission) · the ADR-003 12-gate ceremony reduces to applicable gates ·

- ✅ Gate 2 TDD · 8 new conformance tests in `codegen.rs` · the no-drift
  gate (`enum_matches_catalog_one_to_one`) is the canonical seam.
- ✅ Gate 3 IMPL · `cargo check --workspace` GREEN.
- ✅ Gate 4 CLIPPY · `cargo clippy --workspace --all-targets -- -D
  warnings` 0 warnings.
- ✅ Gate 8 DOCS · `cargo doc --no-deps -p nika-schema` 0 warnings.
- ✅ Gate 12 ATOMIC · single engine commit.
- `cargo fmt --check` GREEN.
- `cargo deny check` advisories+bans+licenses+sources ok.
- Gates 1 / 5 / 6 / 7 / 9 / 10 / 11 · N/A for data refactor.

## Companion artefacts

- `nika-schema/src/codegen.rs` · `pub fn nika_builtin_tool_enum_schema`
  + 8 conformance tests · the engine-side no-drift gate.
- `nika-schema/src/lib.rs` · module registration + re-export.
- `nika/spec/schemas/workflow.schema.json` (separate submodule commit
  · invoke.tool oneOf · 26-builtin closed enum).
- Internal G1 design + closure artefacts in private monorepo (cited by
  D-id only · audit trail preserved at the source-of-truth surface).

## References

- ADR-084 · nika-catalog reconciliation to spec 26 (prerequisite)
- ADR-082 · envelope `nika: v1` single-version-marker
- `nika/spec/stdlib/builtins-v0.1.md` · the canonical 26 source-of-truth
- Internal · D-2026-05-27-N2 LSP-perfection theorem (private blueprint)
- Internal · LOCK-031 trigger-gate discipline (private doctrine)
- Internal · scope-shrink discipline (canon NOW · code DEFER)
- Internal · cross-source-validation §2.6 Signal 7 (verify-the-auditor)

🦋
