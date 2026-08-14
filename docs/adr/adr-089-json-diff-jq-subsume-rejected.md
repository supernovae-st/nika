---
id: ADR-089
title: "nika:json_diff jq-subsume feasibility · REJECTED · keep RFC-6902 Patch as canonical output format"
status: rejected
date: 2026-05-27
phase: "Phase 2 M2"
deciders: ["@ThibautMelen"]
tags: ["nika-catalog", "stdlib", "builtins", "rams", "less-but-better", "json-diff", "rfc-6902", "jq"]
affects_crates: []
affects_layers: []
supersedes: []
superseded_by: []
related: ["ADR-084", "ADR-085", "ADR-086", "ADR-087", "ADR-088"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
---

# ADR-089 · `nika:json_diff` jq-subsume feasibility · REJECTED

## Context

Operator-led Rams audit 2026-05-27 of the canonical 22-builtin set
(post-ADR-086/087/088 collapses) identified `nika:json_diff` as one of
the residual data builtins · question to answer · « can jq subsume
json_diff like it subsumed map/filter/group_by/json_merge? »

This is the same Rams test that drove the 42 → 22 cascade · the
canonical razor « a nika:builtin is justified SSI NOT expressible in
jq+CEL+Schema · IS the logic behind the D-N6 collapse 42→26 + ADR-086
csv_to_json cut + ADR-088 4-introspection collapse ».

Per Internal · cross-source-validation §2.6 Signal 3 · single-source jq
research insufficient · this ADR documents the primary-source
verification (jq capability matrix) and the disposition.

## Decision

**REJECTED · keep `nika:json_diff` as canonical builtin emitting RFC-6902
JSON Patch.**

### Rationale · what jq CAN do

jq is Turing-equivalent for JSON transformation. A recursive walk over
two documents to find differences IS expressible · the rough shape ·

```jq
# Compare two documents at $base and $patch · recursive · produces
# {path, base, patch} tuples for each difference (one possible shape)
def diff($base; $patch; path):
  if ($base | type) == "object" and ($patch | type) == "object" then
    (($base | keys) + ($patch | keys) | unique)[] as $k |
    diff($base[$k]; $patch[$k]; path + [$k])
  elif $base != $patch then
    { path: path, before: $base, after: $patch }
  else empty end ;

diff(.[0]; .[1]; [])
```

Capability-wise · jq CAN walk + compare + emit difference tuples. The
test « is NOT expressible in jq+CEL+Schema » FAILS at the capability
layer for ARBITRARY difference reporting.

### Rationale · what jq CANNOT do (the actual cut)

The justification for `nika:json_diff` is NOT « jq can't diff » · it's
**« jq cannot emit RFC-6902 JSON Patch (the canonical interop format) »**.

RFC 6902 is the canonical IETF standard for JSON document diffs ·
shape ·

```json
[
  { "op": "replace", "path": "/foo/bar", "value": 42 },
  { "op": "add",     "path": "/baz",     "value": "new" },
  { "op": "remove",  "path": "/qux" }
]
```

The 6 operations (`add` · `remove` · `replace` · `move` · `copy` · `test`)
have semantic rules · particularly ·

- `replace` vs `add` discrimination · based on key existence in the
  base · jq doesn't track « did this key exist in the original document »
  in a way that survives the recursive walk
- `remove` requires the absence in patch (jq can express absence-of-key
  via `has` but threading it through nested recursion is verbose · ~30+
  lines of jq for a correctness-equivalent emitter)
- `move` + `copy` require structural pattern detection (was this
  add+remove pair semantically a move?) · jq has no notion of « detect
  this pair as one logical operation »
- `test` is a runtime assertion semantic · belongs to the consumer

A 30+ line jq diff emitter that gets RFC 6902 SEMANTICS right is ·

1. **Not « less but better »** · longer than the 22-builtin emit shape it
   would replace · violates Rams 10 spirit
2. **Cookbook anti-pattern** · Internal · the « one obvious way »
   validator canon (D-N9/N10) requires authors to NOT memorize 30-line
   jq programs · prefer the canonical builtin
3. **Interop loss** · the RFC 6902 output format is the contract
   downstream tools (JSON Patch apply libraries · OpenAPI diff tooling ·
   GraphQL change-set encoders) expect. Emitting a custom
   `{path, before, after}` shape via jq breaks interop · forces every
   consumer to write a translator

### The other 4 introspection razor sister tests

Same razor applied to ADR-086/087/088 · those COLLAPSED because the
collapsed forms WERE strictly better (Rams 10) ·

- ADR-086 `csv_to_json → convert` · was 1 narrow direction · convert
  exposes 4 formats × 3 directions = 12 capabilities · strictly more
  power · same surface
- ADR-087 `sleep + wait_until → wait` · the two former builtins were
  artificially split by mode (relative vs absolute) · unified mode arg
  IS the « one obvious way »
- ADR-088 `4 introspection → inspect view:` · the 4 were artificially
  split by query-shape · unified view-discriminator IS the « one
  obvious way »

The `json_diff` case is different · the alternative (« express via
jq ») would be **LESS obvious** (30+ lines vs 1 builtin call) AND
**LESS interoperable** (custom diff shape vs RFC 6902 standard). Rams
test FAILS in both directions.

## Disposition

**KEEP `nika:json_diff` as canonical builtin** ·

```yaml
diff:
  invoke:
    tool: "nika:json_diff"
    args:
      base: ${{ inputs.original }}
      patch: ${{ inputs.modified }}
  # output · RFC 6902 JSON Patch array
```

Output shape · canonical RFC 6902 `Patch` array per IETF spec ·
downstream-interop-clean.

Canonical 22-builtin count UNCHANGED post this ADR · the 5-layer Rams
symmetry (fetch+extract · jq · convert · wait · inspect) holds ·
`json_diff` stands as the « jq can't diff to a canonical format »
exception · same shape as `json_merge_patch` (RFC 7396 null-delete · jq
can't) · same shape as `validate` (JSON Schema · jq is not a
validator).

## Future revisit triggers (LOCK-031 strict)

Per Internal · round-1-ship-pattern §6 LOCK-031 spirit · documented triggers
that COULD reopen this disposition ·

| Trigger | What it would mean | Likelihood |
|---|---|---|
| Cookbook adopts a `jq '... diff ...'` recipe with ≥3 successful invocations | Authors prefer jq-form over builtin · razor shifts | LOW · 30+ line jq programs are off-doctrine |
| RFC 6902 deprecated by IETF | Output-format interop argument weakens | NEAR-ZERO · RFC is stable since 2013 · widely deployed |
| jaq ships an `--rfc6902` flag or canonical `diff` filter | jq becomes the canonical RFC 6902 emitter | LOW · would require upstream jaq change |
| Operator-explicit `« cut json_diff »` signal | The ultimate trigger · same shape as ADR-086 | OPEN |

Per Internal · cross-source-validation §2.6 Signal 3 · this ADR is the
canonical anti-promotion gate · any future « should we cut json_diff? »
discussion routes here.

## Cross-source verification (per §2.6 Signal 3 + §2.7.1 cascade-completeness)

Primary-source verified 2026-05-27 ·

1. **jaq source** · `jaq/jaq-json/src/lib.rs` · NO native `diff` or
   `rfc6902` filter. The capability is expressible (Turing-equivalent)
   BUT no canonical output-format emitter shipped.
2. **jq manual** · `jq-1.7.1` manual at https://jqlang.org/manual/ ·
   no `diff` builtin · no RFC 6902 support in stdlib.
3. **RFC 6902** · IETF spec stable since 2013 · 6 ops canonical ·
   widely supported by JSON Patch libraries (e.g. `serde_json_patch`
   in Rust ecosystem).
4. **Cookbook signal** · NIKA_COOKBOOK-v0.1.md · zero recipes that
   express diff-via-jq · the « one obvious way » authoring contract
   would reject 30+ line jq programs.

Bucket A (already canon) · `nika:json_diff` was a D-N5 KEEP · this ADR
formalizes the KEEP rationale.

## Cohérent

- ADR-082 `nika: v1` envelope · 22 builtins canonical · ADR-089 holds count
- ADR-084 catalog-spec reconciliation · `json_diff` stays in catalog
- ADR-085 schemars codegen · `json_diff` enum variant stays
- ADR-086/087/088 · 5-layer Rams symmetry · `json_diff` is the canonical
  exception that proves the « jq can't do everything » rule
- Per Internal · cross-source-validation §2.6 Signal 3 verification
- Per Internal · cognitive-design-canon P4 cognitive load · 1-line
  builtin call beats 30-line jq program

## Update log

```
2026-05-27  v1.0 — Initial ADR · disposition REJECTED (keep nika:json_diff)
              · Trigger · operator Rams audit 2026-05-27 evening · post
                ADR-086/087/088 5-layer symmetry close · « can jq subsume
                json_diff like jq subsumed json_merge? »
              · Verification · primary-source jaq + jq + RFC 6902 IETF
                · capability YES (Turing-equivalent walk) · output-format
                interop NO (no canonical RFC 6902 emitter)
              · Decision · keep `nika:json_diff` · 30+ line jq programs
                violate Rams 10 less-but-better · violate cookbook one-
                obvious-way contract · break RFC 6902 interop
              · Canonical 22-builtin count UNCHANGED · 5-layer Rams
                symmetry (fetch+extract · jq · convert · wait · inspect)
                holds · json_diff stands as the « jq can't diff to a
                canonical format » exception
              · Future revisit triggers documented per LOCK-031 spirit ·
                cookbook signal · RFC deprecation · jaq native diff filter ·
                operator explicit cut
              · Cohérent ADR-082 envelope + ADR-084/085/086/087/088 +
                Internal · cross-source-validation §2.6 + cognitive-design
                P4 + one-obvious-way validator canon D-N9/N10
              · 5 raisons coherence ✅✅✅✅✅
```
