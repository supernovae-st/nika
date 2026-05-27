---
id: ADR-086
title: "builtin Rams sweep · nika:csv_to_json → nika:convert universal multi-format · less but better"
status: accepted
date: 2026-05-27
phase: "Phase 2 M2"
deciders: ["@ThibautMelen"]
tags: ["nika-catalog", "nika-schema", "stdlib", "builtins", "rams", "less-but-better", "consolidation"]
affects_crates: ["nika-catalog", "nika-schema"]
affects_layers: ["L0"]
supersedes: []
superseded_by: []
related: ["ADR-082", "ADR-084", "ADR-085"]
requires: ["ADR-084"]
enables: []
amends: []
fci: ["FCI-001"]
inv: ["INV-019", "INV-025"]
shadow_zones: []
---

# ADR-086 · builtin Rams sweep · nika:csv_to_json → nika:convert

## Context

Operator-led audit 2026-05-27 of the post-D-2026-05-22-N6 26-builtin
set surfaced 4 high-confidence Rams « less but better » consolidation
candidates · the operator named `nika:csv_to_json` specifically as
the canonical anti-pattern (single-direction · single-format · vs
the `fetch+extract` super-powerful-with-mode-args canonical exemplar).

Operator verbatim · « faut vraiment réfléchir de manière plus
atomique · `nika:csv_to_json` faudrait plutôt un builtin super
puissant qui gère à convertir n'importe quoi · le `fetch` il est
avancé avec extract · les autres ça devrait être pareil · faut pas
hésiter à améliorer · réunir · Rams · et chercher les meilleurs
crates correspondant ».

The D-N6 stdlib-collapse (42 → 27) + 2026-05-27 `json_merge` cut
(27 → 26) established the precedent · the canonical pattern is « one
super-powerful builtin · multi-mode parameters » (jq subsumes ~13
data builtins · fetch subsumes 8 extract modes). This ADR continues
the sweep at the format-conversion layer.

## Decision

`nika:csv_to_json` removed · `nika:convert` adopted as universal
multi-format converter.

### `nika:convert` surface

```yaml
invoke:
  tool: "nika:convert"
  args:
    input: "${{ tasks.A.output }}"   # data to convert
    from: csv                         # REQUIRED · enum · json | yaml | toml | csv
    to: json                          # REQUIRED · enum · json | yaml | toml | csv
    has_header: true                  # OPTIONAL · CSV only · default true
```

### Formats v0.1 baseline

4 formats · `json` · `yaml` · `toml` · `csv` · 12 directions in scope
(4×3 minus identity). Forward-compat · `xml` · `tsv` · `messagepack`
etc. ADD on consumer signal per `#[non_exhaustive]` ratchet (additive
MINOR per forever-v0.x).

### Engine catalog mutation

`nika-catalog::ALL_BUILTINS` (line ~21 in `data/builtins.rs`) ·

- DELETE entry · `Builtin { name: "csv_to_json", category: Data }`
- INSERT entry (alphabetical · between `assert` and `cost`) ·
  `Builtin { name: "convert", category: Data }`

Sort invariant preserved · `convert` < `cost` alphabetically (`c-o-n`
< `c-o-s`). The 6-existing tests (`builtin_count` · `sorted_order` ·
`find_known_builtins` · `is_known_builtin_works` · `case_sensitive` ·
`all_builtins_have_non_empty_names` · `category_counts`) stay GREEN ·
total 26 unchanged · 8 Data category unchanged.

### Engine trust.rs mutation (per ADR-084 §C precedent)

`TRUST_PROPAGATING_BUILTINS` list ·

- DELETE `"nika:csv_to_json"`
- INSERT `"nika:convert"` (alphabetical · first slot)

Trust class unchanged (`csv_to_json` was PROPAGATING · `convert` stays
PROPAGATING · pure-data transform · output trust = input trust).

### Engine codegen.rs mutation (ADR-085 no-drift gate)

`enum_excludes_legacy_post_d_n6` regression test extends the legacy
exclusion list with `nika:csv_to_json` · 5 → 6 legacy names guarded
(the cut-set regression).

### Spec mutation (companion · nika/spec submodule)

Spec ratchet shipped same-arc per the operator-led Rams sweep ·

- `nika/spec/stdlib/builtins-v0.1.md` · §1 Data row + §3 Data section
  rename csv_to_json → convert with full new semantics (formats list ·
  pattern rationale · reference implementation crates · alphabetical
  position preserved in §3).
- `nika/spec/schemas/workflow.schema.json` · invoke.tool oneOf nika
  branch enum · alphabetical re-sort (convert moves before cost) ·
  count stays 26 · LSP-perfect autocomplete preserved.

## Reference implementation (L2 nika-builtin admission · future)

Primary-source verified candidate crates (crates.io API · 2026-05-27) ·

| Crate | Version | Updated | Notes |
|---|---|---|---|
| `serde-transcode` | 1.1.1 | 2021-07-10 | Canonical orchestrator · sfackler · 15M+ downloads · API-stable (COMPLETE not abandoned) |
| `serde_json` | 1.0 | (already nika dep) | JSON · zero-add |
| `serde_yaml_bw` | 2.5.6 | 2026-05-02 | YAML · modern + maintained · 133k downloads · 2026-current |
| `toml` | 1.1.2 | 2026-04-01 | TOML · spec 1.1.0 compliant · active |
| `csv` | 1.4.0 | 2025-10-17 | CSV · quoting-aware · active |

Cohérent internal NUKE-LEGACY Class 2 (git is the archive · zero
deprecation alias) · zero-add prefer when an existing dep covers
the case · `serde_json` already in workspace. New deps gated on the
L2 nika-builtin admission ceremony per Diamond Rule 2 12-gate.

## Consequences

### Positive

- **Symmetry signal** · per « less but better » Rams principle · the
  fetch+extract pattern · jq subsumption · convert · all the same
  « one super-powerful builtin · multi-mode args » canonical shape
  applied at 3 distinct layers (HTTP · data transform · format
  conversion). The pattern propagates the more it's applied · easier
  cognitive surface for cookbook authors + LSP consumers.
- **Bidirectional coverage** · `nika:convert from:csv to:json` AND
  `nika:convert from:json to:csv` (the latter previously covered only
  via `jq @csv`) · single canonical builtin · no « which direction
  goes through which tool » mental load.
- **Forward extensibility** · `xml` · `tsv` · `messagepack` add via
  enum extension (forward-compat MINOR ratchet per forever-v0.x · zero
  new builtin slot consumed).
- **Trust class preserved** · csv_to_json was PROPAGATING · convert
  stays PROPAGATING · zero security regression.
- **No-drift gate enforced** · ADR-085 codegen.rs test
  `enum_matches_catalog_one_to_one` AUTO-detects the spec ↔ engine
  parity · the rename propagates automatically once ALL_BUILTINS
  mutates · `nika_builtin_tool_enum_schema()` returns the new 26-set
  derived at call time.

### Negative

- **Public API breaking** · existing workflows using `tool: nika:csv_to_json`
  will fail to parse against the spec-26 closed enum + against
  `is_known_builtin("csv_to_json")` in engine. Acceptable on forever-
  v0.x per ADR-002 (NUKE-LEGACY · git is the archive · zero deprecation
  alias per internal no-legacy doctrine).
  - Mitigation · the existing nika spec is pre-v1.0 GA · cookbook
    authors haven't started migration · breakage cost ≈ 0 today.
  - Mitigation · the trust.rs test `legacy_builtins_unknown_post_d_n6`
    explicitly catches any code path that still references
    `"nika:csv_to_json"` · fails closed (uncategorized → Untrusted
    output).

### Neutral

- **Tests · +1 net new legacy guard** in codegen.rs · 8 conformance
  tests stay GREEN (6 from baseline + 2 spec-26 gates from ADR-084) ·
  legacy exclusion list 5 → 6.
- **L0 layer constraint preserved** · the rename is pure data + pure
  test mutation · zero I/O · zero async · zero L0 violation.
- **Backward graph projection (TTL)** · internal monorepo nika-
  language graph carries 4 csv_to_json refs (instance · Builtin
  family member-list · `raises` arc · `invokedBy` arc) · these
  update SAME-ARC in the monorepo cascade (separate from this
  engine ADR).

## Alternatives considered

### A · Keep `nika:csv_to_json` · add `nika:yaml_to_json` · `nika:toml_to_json` · etc.

REJECTED · N² builtin slot explosion · 4 formats × 3 target = 12
builtins for what should be 1 · violates « less but better » Rams
principle + the D-N6 anti-anti-pattern. Per existing spec comment
« CSV parsing is not jq's job · the reverse direction is jq @csv » ·
the asymmetric treatment was exactly the friction the operator
audit named.

### B · Keep `csv_to_json` only · skip the Rams sweep

REJECTED · operator-led directive (« FAIS · do it ») + the symmetry
signal (fetch+extract · jq · now convert · all the same « one
super-powerful builtin · multi-mode args » pattern) · skipping leaves
the canonical anti-pattern in the spec.

### C · Adopt `dasel` library or similar all-in-one Go-origin tool

REJECTED · `dasel` is a Go binary · embedding requires unsavory
subprocess shenanigans · violates L0 « zero I/O · zero async » +
sovereignty Rule 1 + ADR-001 pure-Rust mandate. `serde-transcode` is
the canonical serde-ecosystem Rust solution · primary-source verified.

## Gate verification

Per ADR-084 precedent (data refactor not crate admission · 12-gate
reduces to applicable gates) ·

- ✅ Gate 2 TDD · trust.rs `legacy_builtins_unknown_post_d_n6` extended
  · codegen.rs `enum_excludes_legacy_post_d_n6` extended · existing
  builtins.rs `category_counts` (Data still 8) preserved · all GREEN.
- ✅ Gate 3 IMPL · `cargo check --workspace` GREEN.
- ✅ Gate 4 CLIPPY · `cargo clippy --workspace --all-targets -- -D
  warnings` 0 warnings.
- ✅ Gate 8 DOCS · `cargo doc --no-deps -p nika-catalog -p nika-schema`
  0 warnings.
- ✅ Gate 12 ATOMIC · single engine commit + single monorepo bump.
- `cargo fmt --check` GREEN · `cargo deny check` ok.
- Gates 1 / 5 / 6 / 7 / 9 / 10 / 11 · N/A for data refactor.

## Future Rams ratchets (trigger-gated · ADR-087+ reserved)

3 remaining consolidation candidates from the operator audit ·
trigger-gated per LOCK-031 spirit · don't pre-build infra ·

1. **`nika:wait`** · collapse `sleep` + `wait_until` into one temporal
   primitive with `duration:` OR `until:` mode. Net 26 → 25. Trigger ·
   D-lock + operator OUI/NON Bucket-C user-explicit-lock.
2. **`nika:inspect`** · collapse 4 introspection builtins (cost ·
   records · dag_info · threads) into one with `view:` enum. Net 26 →
   23. Trigger · D-lock + workflow.schema.json oneOf-vs-untyped design
   decision (return-shape heterogeneity).
3. **`nika:json_diff` jq-subsume feasibility** · jq CAN express
   diff (recipe in audit doc §5) but loses the RFC-6902-canonical-Patch
   output format. Trigger · cookbook adoption of the jq-diff recipe +
   operator preference signal.

When all 3 land · 26 → ~21 collapse · same `nika: v1` envelope ·
forever-v0.x discipline.

## Companion artefacts

- `nika-catalog/src/data/builtins.rs` · ALL_BUILTINS rename (delete
  csv_to_json · insert convert in alphabetical position).
- `nika-schema/src/trust.rs` · TRUST_PROPAGATING_BUILTINS rename +
  `legacy_builtins_unknown_post_d_n6` test extended (+csv_to_json).
- `nika-schema/src/codegen.rs` · `enum_excludes_legacy_post_d_n6`
  test extended (+csv_to_json · 5 → 6 legacy guarded).
- `nika/spec/stdlib/builtins-v0.1.md` (separate submodule commit
  `2a14540`) · csv_to_json section → convert section with universal
  multi-format semantics.
- `nika/spec/schemas/workflow.schema.json` (same submodule commit) ·
  invoke.tool oneOf nika branch enum alphabetical re-sort.
- Internal · Rams audit doc (private monorepo) · the 26-builtin
  builtin-by-builtin table + 4-collapse proposal + crate research +
  shipping plan.

## References

- ADR-085 · spec workflow.schema.json oneOf bridge (prerequisite)
- ADR-084 · nika-catalog reconciliation to spec 26 (prerequisite)
- ADR-082 · envelope `nika: v1` single-version-marker
- D-2026-05-22-N6 · stdlib-collapse 42 → 27 (precedent · « one
  obvious way » + jq subsumption)
- 2026-05-27 · `nika:json_merge` cut (precedent · razor-edge Rams
  decision + jaq source-verified)
- Internal · `no-legacy-no-back-compat.md` Class 2 (git is the
  archive)
- Internal · LOCK-031 trigger-gated discipline (no infra behind
  locked gate · future ratchet 1/2/3 deferred)
- Internal · `cross-source-validation.md` §2.6 Signal 7
  (verify-the-auditor · crate claims primary-source-verified via
  crates.io API)

🦋
