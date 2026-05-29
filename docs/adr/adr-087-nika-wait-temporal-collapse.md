---
id: ADR-087
title: "nika:sleep + nika:wait_until → nika:wait · temporal-control Rams collapse"
status: accepted
date: 2026-05-27
phase: "Phase 2 M2"
deciders: ["@ThibautMelen"]
tags: ["nika-catalog", "nika-schema", "stdlib", "builtins", "rams", "less-but-better", "temporal-control"]
affects_crates: ["nika-catalog", "nika-schema"]
affects_layers: ["L0"]
supersedes: []
superseded_by: []
related: ["ADR-082", "ADR-084", "ADR-085", "ADR-086", "ADR-088", "ADR-089"]
requires: ["ADR-084", "ADR-086"]
enables: []
amends: []
fci: ["FCI-001"]
inv: ["INV-019", "INV-025"]
shadow_zones: []
---

# ADR-087 · nika:sleep + nika:wait_until → nika:wait

## Context

Operator-led Rams audit 2026-05-27 of the canonical builtin set
identified `nika:sleep` and `nika:wait_until` as two temporal-control
primitives with overlapping semantics ·

- `nika:sleep` · pause for a **relative** duration (Go-duration string ·
  `"5s"` · `"30m"` · ...). Trust PURE.
- `nika:wait_until` · pause until an **absolute** ISO 8601 timestamp.
  Trust PURE.

Operator verbatim · « `nika:wait_until` ça pourra pas être le sleep
aussi ? ». The answer is structural · BOTH are « temporal pause »
operations · same trust class · same semantic family · the split was
historical (separate names · separate args) not architectural (the
operation is the same · just relative vs absolute mode).

Per « less but better » Rams principle + the canonical « one super-
powerful builtin · multi-mode args » pattern established by ADR-086
(convert) + fetch+extract + jq subsumption · ADR-087 unifies the two
temporal builtins into a single `nika:wait`.

## Decision

`nika:sleep` and `nika:wait_until` removed · `nika:wait` adopted as
unified temporal-control builtin.

### `nika:wait` surface

```yaml
invoke:
  tool: "nika:wait"
  args:
    duration: "5s"                   # RELATIVE · Go-duration string ("500ms"/"30s"/"5m"/"1h30m")
    # OR (mutually exclusive · exactly-one-of) ·
    until: "2026-05-23T09:00:00Z"    # ABSOLUTE · ISO 8601 timestamp · MAY be CEL expression
    timeout: "1h"                    # OPTIONAL · cap for absolute wait (until: only)
```

### Mode semantics

- `duration:` mode · relative pause (sleep equivalent). Engine parses
  Go-duration string ("500ms" · "30s" · "5m" · "1h30m") · validates non-
  negative · returns no value (side-effect timer).
- `until:` mode · absolute pause (wait_until equivalent). Engine parses
  ISO 8601 timestamp (with timezone · or assumes UTC if naive) · MAY be a
  CEL `${{ }}` expression · `timeout:` caps the wait duration.

### Exactly-one-of validation

Parse-time check · the builtin args MUST set exactly one of `duration:`
or `until:`. Both set OR neither set = `NIKA-BUILTIN-WAIT-003` error.

### Error codes

- `NIKA-BUILTIN-WAIT-001` · absolute timeout exceeded (only when `until:`)
- `NIKA-BUILTIN-WAIT-002` · timestamp in past (only when `until:`)
- `NIKA-BUILTIN-WAIT-003` · NEW · neither `duration:` nor `until:` set OR
  both set (exactly-one-of violation)

### Engine catalog mutation

`nika-catalog::ALL_BUILTINS` ·

- DELETE 2 entries · `Builtin { name: "sleep", category: Core }` ·
  `Builtin { name: "wait_until", category: Core }`
- INSERT 1 entry (alphabetical · between `validate` and `write`) ·
  `Builtin { name: "wait", category: Core }`

Sort invariant preserved · `wait` < `write` (`a` < `r`) · `wait` >
`validate` (`v` < `w`). Total · 26 → **25**. Core category · 7 → **6**.

### Engine trust.rs mutation

`TRUST_PURE_BUILTINS` list ·

- DELETE `"nika:sleep"` + `"nika:wait_until"`
- INSERT `"nika:wait"`

Trust class unchanged (both were PURE · wait stays PURE · pure
control-flow timer · no I/O · no data flow). Total PURE 14 → **13**.

### Engine codegen.rs mutation (ADR-085 no-drift gate)

- `enum_excludes_legacy_post_d_n6` · ADD `"nika:sleep"` + `"nika:wait_until"`
  to legacy exclusion list (6 → 8 legacy names guarded).
- `enum_includes_canonical_anchors` · `"nika:sleep"` → `"nika:wait"` (the
  canonical-anchor sentinel for Core category).
- `enum_has_26_entries` → `enum_has_25_entries`.

### Engine catalog test mutations

- `builtin_count` · 26 → 25
- `category_counts` · Core 7 → 6 · total 25
- `find_known_builtins` · sleep → wait sentinel
- `is_known_builtin_works` · sleep is now legacy (not categorized) · wait
  is the canonical · ADD sleep + wait_until to negative-assertion list

### Spec mutation (companion · nika/spec submodule `8372ca2`)

- `nika/spec/stdlib/builtins-v0.1.md` · §0 + §1 Core count 26 → 25 · 7 →
  6 · §3 Core section DELETE sleep + wait_until subsections · ADD wait
  subsection with full duration:/until:/timeout: API · 3 error codes
  documented.
- `nika/spec/schemas/workflow.schema.json` · invoke.tool oneOf nika
  branch enum · DELETE 2 entries · ADD 1 · 26 → 25 · description string
  updated (Core 7 → 6).
- `nika/spec/spec/06-stdlib-contract.md` · enumeration line + total
  updated · sleep + wait_until merge cited.

## Consequences

### Positive

- **Symmetry signal extends** · same « one super-powerful builtin · multi-
  mode args » canonical pattern now realized at 4 distinct layers ·
  HTTP (fetch+extract) · data transform (jq subsumption) · format
  conversion (convert) · temporal control (wait). Cookbook authors +
  LSP consumers get consistent cognitive surface.
- **Unified parse-time validation** · the exactly-one-of check is a
  single canonical surface · the prior split required the engine to
  understand TWO trust paths + TWO arg shapes for what was always one
  family.
- **Trust class preserved** · both were PURE · wait stays PURE · zero
  security regression.
- **No-drift gate auto-validates** · ADR-085 codegen.rs test
  `enum_matches_catalog_one_to_one` AUTO-detects the spec ↔ engine
  parity · the rename propagates automatically through
  `nika_builtin_tool_enum_schema()` since it reads ALL_BUILTINS at call
  time.

### Negative

- **Public API breaking** · existing workflows using `tool: nika:sleep`
  OR `tool: nika:wait_until` fail against the spec-25 closed enum +
  `is_known_builtin` returns false for both. Acceptable on forever-v0.x
  per ADR-002 (NUKE-LEGACY · git is the archive · zero deprecation alias).
  - Mitigation · spec is pre-v1.0 GA · cookbook adoption is early ·
    breakage cost ≈ 0 today.
  - Mitigation · the trust.rs `legacy_builtins_unknown_post_d_n6` test
    extends with both legacy names · fail-closed regression guard.
- **New error code** · NIKA-BUILTIN-WAIT-003 (exactly-one-of) is new ·
  documented in the spec error namespace · engine impl gates on the L2
  nika-builtin admission (future).

### Neutral

- **Tests · +2 net new legacy guards** in trust.rs +  +2 in codegen.rs ·
  existing 6 conformance tests + 2 spec-25 gates stay GREEN (renamed ·
  total counts asserted).
- **L0 layer constraint preserved** · pure data + pure test mutation ·
  zero L0 violation.
- **TTL graph projection** · monorepo `nika-language-v1.ttl` updates ·
  4 sleep refs + 4 wait_until refs → 4 wait refs (instance · family
  member-list · raises arc · invokedBy arc) · count 26 → 25 in family
  list.

## Alternatives considered

### A · Keep both builtins · accept the duplication

REJECTED · the operator-led directive is explicit (« faut pas hésiter à
améliorer · réunir · Rams ») · the temporal duplication is the
canonical « less but better » anti-pattern · keeping it leaves the
symmetry signal incomplete (3-of-4 canonical layers Rams'd · skipping
this layer is structural inconsistency).

### B · Two flat builtins with shared trust list

REJECTED · doesn't address the operator's directive · just shuffles
the trust.rs list without consolidating the builtin surface · cookbook
authors still need to choose sleep vs wait_until · cognitive load
unchanged.

### C · `nika:wait` with mode discriminator field (mode: "relative"|"absolute")

REJECTED · adds redundant ceremony · the discriminator is already
implicit in WHICH arg is present (`duration:` ↔ relative · `until:` ↔
absolute) · the exactly-one-of check is the cleaner self-documenting
surface. Operator-facing prose just says « set duration OR until ».

## Gate verification

Per ADR-084/086 precedent (data refactor not crate admission · 12-gate
reduces to applicable gates) ·

- ✅ Gate 2 TDD · trust.rs + codegen.rs tests updated · 226 nika-catalog
  + 205 nika-schema lib tests GREEN.
- ✅ Gate 3 IMPL · `cargo check --workspace` GREEN.
- ✅ Gate 4 CLIPPY · `cargo clippy --workspace --all-targets -- -D
  warnings` 0 warnings.
- ✅ Gate 8 DOCS · `cargo doc --no-deps -p nika-catalog -p nika-schema`
  0 warnings.
- ✅ Gate 12 ATOMIC · single engine commit + single monorepo bump.
- `cargo fmt --check` GREEN (post auto-fmt).
- `cargo deny check` advisories+bans+licenses+sources ok (no new deps).
- Gates 1 / 5 / 6 / 7 / 9 / 10 / 11 · N/A for data refactor.

## Future Rams ratchets (trigger-gated)

Remaining from the operator-led 26-set audit ·

1. **ADR-088 · `nika:inspect`** · collapse 4 introspection builtins
   (cost · records · dag_info · threads) into one with `view:` enum.
   Net 25 → 22. Trigger · workflow.schema.json oneOf vs untyped design
   decision (return-shape heterogeneity).
2. **ADR-089 · `nika:json_diff` jq-subsume feasibility** · jq CAN
   express diff (recipe documented internally) but loses RFC-6902
   Patch output format. Trigger · cookbook adoption of jq-diff recipe
   + operator preference signal.

When all land · 25 → ~21 collapse. Same `nika: v1` envelope · forever-
v0.x discipline.

## Companion artefacts

- `nika-catalog/src/data/builtins.rs` · ALL_BUILTINS · DELETE 2 ·
  INSERT 1 · 26 → 25 · test counts adjusted.
- `nika-catalog/src/types/builtin.rs` · BuiltinCategory Core doc-comment
  refreshed (`log · emit · assert · prompt · done · wait` · 7 → 6).
- `nika-catalog/src/lib.rs` · `all_builtins_non_empty` test 26 → 25.
- `nika-schema/src/trust.rs` · TRUST_PURE_BUILTINS rename + legacy
  exclusion test extended +sleep +wait_until + `category_totals_match_
  spec_26` → `_25`.
- `nika-schema/src/codegen.rs` · `enum_has_26_entries` → `_25_entries` +
  `enum_includes_canonical_anchors` sleep → wait + `enum_excludes_legacy_
  post_d_n6` extended.
- `nika/spec/stdlib/builtins-v0.1.md` (submodule `8372ca2`) · sleep +
  wait_until sections deleted · wait section added · counts updated.
- `nika/spec/schemas/workflow.schema.json` (same submodule commit) ·
  invoke.tool oneOf nika branch enum 26 → 25.
- `nika/spec/spec/06-stdlib-contract.md` (same submodule commit) ·
  enumeration + total updated.

## References

- ADR-086 · csv_to_json → convert (prerequisite · same Rams sweep)
- ADR-085 · spec workflow.schema.json oneOf bridge (no-drift gate)
- ADR-084 · nika-catalog reconciliation to spec 26 (foundation)
- ADR-082 · envelope `nika: v1` single-version-marker
- Internal · LOCK-031 trigger-gated discipline (no infra behind locked
  gate · ADR-088 + ADR-089 deferred)
- Internal · cross-source-validation §2.6 Signal 7 (verify-the-auditor)
- Internal · no-legacy-no-back-compat Class 2 (git is the archive)

🦋
