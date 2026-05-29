---
id: ADR-088
title: "nika:cost + records + dag_info + threads → nika:inspect · introspection Rams collapse"
status: accepted
date: 2026-05-27
phase: "Phase 2 M2"
deciders: ["@ThibautMelen"]
tags: ["nika-catalog", "nika-schema", "stdlib", "builtins", "rams", "less-but-better", "introspection"]
affects_crates: ["nika-catalog", "nika-schema"]
affects_layers: ["L0"]
supersedes: []
superseded_by: []
related: ["ADR-082", "ADR-084", "ADR-085", "ADR-086", "ADR-087", "ADR-089"]
requires: ["ADR-084", "ADR-086", "ADR-087"]
enables: []
amends: []
fci: ["FCI-001"]
inv: ["INV-019", "INV-025"]
shadow_zones: []
---

# ADR-088 · 4 introspection builtins → nika:inspect

## Context

Operator-led Rams audit 2026-05-27 of the canonical builtin set
identified the 4 introspection primitives as a Rams collapse candidate ·

- `nika:cost` → `{ total_usd, by_task, by_provider }` · workflow cost
- `nika:records` → `{ tasks: [{ id, status, duration_ms, ... }] }` · execution record
- `nika:dag_info` → `{ nodes, edges, waves }` · DAG topology
- `nika:threads` → `{ active, queued, completed }` · task-pool state

All 4 are PURE trust · pure-compute queries of own workflow state · same
semantic family. The split into 4 separate builtins was historical
(one-per-return-shape) not structural. ADR-088 unifies them into a
single view-discriminated `nika:inspect`.

This is the 4th and most ambitious Rams collapse in the operator-led
sweep · completes the « one super-powerful builtin · multi-mode args »
symmetry across 5 distinct domains.

## Decision

`nika:cost` · `nika:records` · `nika:dag_info` · `nika:threads` removed ·
`nika:inspect` adopted as unified introspection builtin with `view:`
enum discriminator.

### `nika:inspect` surface

```yaml
invoke:
  tool: "nika:inspect"
  args:
    view: cost                     # REQUIRED · enum · cost | records | dag_info | threads
```

### Per-view return shapes

- `view: cost` → `{ total_usd, by_task, by_provider }` (sleep equivalent of legacy `nika:cost`)
- `view: records` → `{ tasks: [{ id, status, duration_ms, ... }] }` (legacy `nika:records`)
- `view: dag_info` → `{ nodes, edges, waves }` (legacy `nika:dag_info`)
- `view: threads` → `{ active, queued, completed }` · advisory (legacy `nika:threads`)

### Validation

Parse-time check · `view:` MUST be one of the 4 canonical enum values.
Engine returns `NIKA-BUILTIN-INSPECT-001` if `view:` value not in the
canonical set.

### Engine catalog mutation

`nika-catalog::ALL_BUILTINS` ·

- DELETE 4 entries · `Builtin { name: "cost", category: Introspection }` ·
  `Builtin { name: "records", category: Introspection }` ·
  `Builtin { name: "dag_info", category: Introspection }` ·
  `Builtin { name: "threads", category: Introspection }`
- INSERT 1 entry (alphabetical · between `hash` and `jq` · `h-a-s` <
  `i-n-s` < `j-q` verified) · `Builtin { name: "inspect", category:
  Introspection }`

Sort invariant preserved. Total · 25 → **22**. Introspection category ·
4 → **1**.

### Engine trust.rs mutation

`TRUST_PURE_BUILTINS` list ·

- DELETE `"nika:cost"` + `"nika:records"` + `"nika:dag_info"` + `"nika:threads"`
- INSERT `"nika:inspect"`

Trust class unchanged (all 4 were PURE · inspect stays PURE · pure-
compute query of own workflow state · no I/O · no flow change · output
trust = TRUSTED always). Total PURE · 13 → **10**.

### Engine codegen.rs mutation (ADR-085 no-drift gate)

- `enum_excludes_legacy_post_d_n6` · ADD all 4 to legacy exclusion list
  (8 → **12** legacy names guarded).
- `enum_includes_canonical_anchors` · `"nika:cost"` → `"nika:inspect"`
  (the canonical-anchor sentinel for Introspection category).
- `enum_has_25_entries` → `enum_has_22_entries`.

### Engine catalog test mutations

- `builtin_count` · 25 → 22
- `category_counts` · Introspection 4 → 1 · total 22
- `find_known_builtins` · `"cost"` → `"inspect"` sentinel
- `is_known_builtin_works` · `"inspect"` is the canonical · 4 legacy
  added to negative-assertion list

### Spec mutation (companion · nika/spec submodule `8437909`)

- `nika/spec/stdlib/builtins-v0.1.md` · §0 + §1 Introspection count 25
  → 22 · 4 → 1 · §3 Introspection section DELETE 4 subsections · ADD
  inspect subsection with `view:` enum + per-view return shapes + 1
  NIKA-BUILTIN-INSPECT-001 error code.
- `nika/spec/schemas/workflow.schema.json` · invoke.tool oneOf nika
  branch enum · DELETE 4 entries · ADD 1 · 25 → 22 · description
  Introspection 4 → 1.
- `nika/spec/spec/06-stdlib-contract.md` · enumeration + total updated
  · 4-into-1 inspect collapse cited per ADR-088.

## Consequences

### Positive

- **Symmetry signal COMPLETE · 5 layers** · « one super-powerful builtin
  · multi-mode args » canonical pattern now realized at 5 distinct
  domains · HTTP (fetch+extract · 8 modes) · data transform (jq ·
  subsumes ~13) · format conversion (convert · 4 formats × 3 directions)
  · temporal control (wait · 2 modes) · introspection (inspect · 4 views).
  The Rams sweep is structurally complete · cookbook authors + LSP
  consumers get fully consistent cognitive surface across all 5 domains.
- **Trust class preserved** · all 4 were PURE · inspect stays PURE ·
  zero security regression.
- **No-drift gate auto-validates** · ADR-085 codegen.rs test
  `enum_matches_catalog_one_to_one` AUTO-detects spec ↔ engine parity
  via `nika_builtin_tool_enum_schema()` reading ALL_BUILTINS at call time.
- **Workflow.schema.json LSP-completion ENRICHED** · 4 → 1 builtin
  reduces autocomplete surface AND focuses the prompt on the `view:`
  discriminator (the actual operator decision point) · cleaner LSP UX
  than 4 sibling completions with similar names.

### Negative

- **Return-shape heterogeneity** · the 4 views have semantically
  distinct return shapes (cost = numeric metrics · records = list ·
  dag_info = graph · threads = pool counts) · workflow.schema.json
  models this as untyped output (operator-side handles per-view) ·
  could ratchet to `oneOf` per view in a future ADR if LSP UX feedback
  signals need typed return.
  - Mitigation · per-view return shapes documented inline in spec §3
    so authors know what to expect.
  - Mitigation · the canonical fetch+extract precedent has the same
    return-shape heterogeneity across its 8 modes · workflow author
    pattern is well-established.
- **Public API breaking** · existing workflows using `tool: nika:cost`
  (or records · dag_info · threads) fail against the spec-22 closed
  enum + `is_known_builtin` returns false for all 4. Acceptable on
  forever-v0.x per ADR-002 (NUKE-LEGACY · git is the archive · zero
  deprecation alias).
  - Mitigation · spec is pre-v1.0 GA · breakage cost ≈ 0 today.
  - Mitigation · trust.rs `legacy_builtins_unknown_post_d_n6` extends
    with all 4 · fail-closed regression guard.
- **NIKA-BUILTIN-INSPECT-001 new error code** · documented in spec
  error namespace · engine impl gates on L2 nika-builtin admission.

### Neutral

- **Tests · +4 net new legacy guards** in trust.rs + +4 in codegen.rs
  · existing conformance tests stay GREEN (renamed counts asserted).
- **L0 layer constraint preserved** · pure data + pure test mutation ·
  zero L0 violation.
- **TTL graph projection** · monorepo `nika-language-v1.ttl` updates ·
  4 sets of 4 refs (cost · records · dag_info · threads) → 4 refs
  (inspect · instance + family + raises + invokedBy) · count 25 → 22
  in family list. Per ADR-087 precedent · session B's nika-spec-graph-
  parity vector auto-syncs.

## Alternatives considered

### A · Keep 4 atomic introspection builtins

REJECTED · the symmetry argument · 4-of-5 Rams collapses landed (jq ·
convert · wait + this) · the 5th uncollapsed (introspection) breaks
the canonical pattern's structural completeness · cookbook authors
face inconsistent surface.

### B · `view:` enum but keep 4 wrappers as syntactic sugar

REJECTED · violates « less but better » · syntactic sugar accumulates
back to N builtin slots · the discriminator IS the canonical API · no
ceremony needed.

### C · Typed `oneOf` return shape per view

DEFERRED to future ADR if LSP UX signal arrives. Per `nika:fetch`
precedent (8 modes · untyped return per spec · still successful) ·
untyped is acceptable v0.1. Trigger gate · operator-side incident
OR LSP UX feedback citing "missed completion for inspect view X
return field Y".

## Gate verification

Per ADR-084/086/087 precedent ·

- ✅ Gate 2 TDD · trust.rs + codegen.rs tests updated · catalog tests
  updated · 226 nika-catalog + 205 nika-schema lib tests GREEN.
- ✅ Gate 3 IMPL · `cargo check --workspace` GREEN.
- ✅ Gate 4 CLIPPY · `cargo clippy --workspace --all-targets -- -D
  warnings` 0 warnings.
- ✅ Gate 8 DOCS · `cargo doc --no-deps -p nika-catalog -p nika-schema`
  0 warnings.
- ✅ Gate 12 ATOMIC · single engine commit + single monorepo bump.
- `cargo fmt --check` GREEN.
- `cargo deny check` ok (no new deps).
- Gates 1 / 5 / 6 / 7 / 9 / 10 / 11 · N/A for data refactor.

## Rams sweep · COMPLETE state post-ADR-088

```
RAMS COLLAPSE ARC (2026-05-27)         BEFORE   AFTER   NET     ADR
────────────────────────                ──────   ─────   ───     ───
csv_to_json → convert                   1        1       0       086
sleep + wait_until → wait               2        1       -1      087
cost+records+dag_info+threads → inspect 4        1       -3      088
                                                          ────
Cumulative builtin count                26       22      -4
```

5 layers now canonically consolidated · HTTP (fetch+extract) · data
transform (jq) · format conversion (convert) · temporal control (wait) ·
introspection (inspect). Symmetry signal STRUCTURALLY COMPLETE.

## Optional future Rams ratchets (LOCK-031 trigger-gated)

1. **ADR-089 · `nika:json_diff` jq-subsume feasibility** · jq CAN
   express diff but loses RFC-6902 Patch output format. Trigger ·
   cookbook adoption of jq-diff recipe + operator preference signal.
2. **Crate-level Rams audit** · 4 candidate domains identified
   internally (insta + expect-test consolidation · YAML triumvirate
   audit · jiff date pre-decision · hash family canonical). Trigger ·
   ratchet candidates accumulate ≥3 cross-product signals.

When ADR-089 + selected crate Rams land · 22 → ~21 + workspace dep
hygiene improvements. Same `nika: v1` envelope · forever-v0.x discipline.

## Companion artefacts

- `nika-catalog/src/data/builtins.rs` · ALL_BUILTINS · DELETE 4 ·
  INSERT 1 · 25 → 22 · test counts adjusted.
- `nika-catalog/src/types/builtin.rs` · BuiltinCategory Introspection
  doc-comment refreshed (1 builtin · view-discriminated).
- `nika-catalog/src/lib.rs` · `all_builtins_non_empty` test 25 → 22.
- `nika-schema/src/trust.rs` · TRUST_PURE_BUILTINS · DELETE 4 +
  INSERT 1 (13 → 10) · legacy exclusion test extended +4 · total
  assertion 25 → 22.
- `nika-schema/src/codegen.rs` · `enum_has_25_entries` → `_22_entries`
  · `enum_includes_canonical_anchors` cost → inspect ·
  `enum_excludes_legacy_post_d_n6` extended +4.
- `nika/spec/stdlib/builtins-v0.1.md` (submodule `8437909`) · 4
  introspection sections deleted · inspect section added · counts
  updated.
- `nika/spec/schemas/workflow.schema.json` (same submodule commit) ·
  invoke.tool oneOf nika branch enum 25 → 22.
- `nika/spec/spec/06-stdlib-contract.md` (same submodule commit) ·
  enumeration + total updated.

## References

- ADR-087 · sleep + wait_until → wait (prerequisite · same Rams sweep)
- ADR-086 · csv_to_json → convert (prerequisite · same Rams sweep)
- ADR-085 · spec workflow.schema.json oneOf bridge (no-drift gate)
- ADR-084 · nika-catalog reconciliation to spec 26 (foundation)
- ADR-082 · envelope `nika: v1` single-version-marker
- Internal · LOCK-031 trigger-gated discipline (no infra behind locked
  gate · ADR-089 + crate Rams audit deferred)
- Internal · cross-source-validation §2.6 Signal 7 (verify-the-auditor)
- Internal · no-legacy-no-back-compat Class 2 (git is the archive)

🦋
