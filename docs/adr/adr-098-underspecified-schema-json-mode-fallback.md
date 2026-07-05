---
id: ADR-098
title: "Underspecified schemas fall back to native JSON mode + local validation"
status: proposed
date: 2026-07-05
phase: ""
deciders: ["@ThibautMelen"]
tags: [nika-verb-infer, nika-providers, structured-output, json-mode, field-report]
affects_crates: [nika-verb-infer, nika-providers]
affects_layers: [L1.5, L2]
supersedes: []
superseded_by: []
related: ["ADR-092"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: ["Gate 2 — cross-provider structured output parity"]
nika_codes: ["NIKA-430", "NIKA-INFER-002"]
timeline: ""
follow_ups: []
---

# ADR-098 — Underspecified schemas fall back to native JSON mode + local validation

- **Status**: Proposed — and implemented in the same change (2026-07-05
  · the conservative default ships behind zero YAML surface, so
  acceptance ratifies observed behavior rather than gating it)
- **Layers**: L2 (`nika-verb-infer` decision + walker) · L1.5
  (`nika-providers` wire-capability seam)
- **Relates**: ADR-092 (the check ladder — the static half of the same
  audit-before-run philosophy); shadow-zone Gate 2 (cross-provider
  structured output parity — this closes one of its rungs).

## Context

The first external-style dogfooding (atlas · 2026-07-04 field report F2)
hit a hard wall on the translate-payload class: « return the SAME
free-form JSON object, translated ». The author's honest schema for that
contract is underspecified by construction — `{ type: object }`, or an
object whose `head`/`sections` children are themselves shapeless.
OpenAI's strict mode (`response_format: json_schema` + `strict: true`)
rejects exactly this class with HTTP 400 ("object schema missing
`properties`" · "array schema missing `items`"), and fully specifying a
free-form payload recursively is impossible. The only shipped workaround
was schema-free promptware — which forfeits every guarantee.

The engine already owns a 5-layer local validation floor
(`nika-verb-infer/src/structured.rs` — lenient extraction · `jsonschema`
validation · bounded retry), so the provider's strict mode was never the
only enforcement point. And the kernel request shape already carries a
provider-native JSON mode (`ResponseFormat::Json` →
`{"type":"json_object"}` on the openai-compat wire ·
`responseMimeType: application/json` on gemini).

## Decision

When a task `schema:` is UNDERSPECIFIED — any node typed `object`
without `properties`, or typed `array` without `items`, anywhere in the
schema tree — do NOT forward it to the provider's strict structured
mode. Instead:

1. request the provider's **native JSON mode** where the wire has one
   (`ResponseFormat::Json` · openai-compat + gemini families),
2. steer the shape through the **prompt instruction** (the same
   instruction block the no-native fallback uses, schema render capped),
3. validate the reply **locally** against the user's schema — the
   existing floor (`extract_and_validate`, which also strips prose and
   code fences), with the existing bounded retry.

Fully-specified schemas keep today's path unchanged (forwarded verbatim
as `ResponseFormat::JsonSchema`). Wires with no native mode at all
(anthropic) keep the instruction-only fallback. The mock wire keeps
receiving the schema verbatim — its "strict mode" SYNTHESIZES a
conformant instance from ANY schema (F3), and the offline golden suite
(`nika test`) depends on that.

The decision is encoded as one closed enum at the verb layer
(`SchemaWire: None | Strict | JsonMode | Instruction`), computed once
per run from two wire-capability answers owned by `nika-providers`:
`WireFormat::supports_response_format()` (existing) and
`WireFormat::strict_rejects_underspecified()` (new · openai-compat +
gemini true · mock + anthropic false). Underspecification detection is
an iterative worklist walk (no recursion — an authored schema can never
overflow the stack) over `properties` · `patternProperties` · `items`
(both forms) · `prefixItems` · `additionalProperties` subschema ·
`$defs`/`definitions` · `anyOf`/`allOf`/`oneOf`.

Explicitly rejected: any new YAML surface. The author writes the schema
they mean; the engine picks the wire that can honor it.

## Consequences

### Positive
- The field repro is green end-to-end: `{ type: object }` and the
  partially-shaped head/sections schema no longer 400 on strict
  providers — pinned by adapter-path tests with the http seam mocked.
- Zero spec/YAML change — no `json: true` sugar to teach, document and
  keep forever; the schema stays the single authored contract.
- Local validation is the constant enforcement floor across ALL wires —
  provider strict mode remains an optimization, never the guarantee
  (shadow-zone Gate 2 direction).
- The conservative default preserves every existing behavior:
  fully-specified schemas, anthropic instruction fallback, and the mock
  synthesis path (F3/offline CI) are byte-identical.

### Negative
- JSON mode only guarantees *syntactic* JSON — an underspecified-schema
  task now leans on the prompt instruction + local retry for shape,
  so it can burn the retry budget where strict mode would have
  constrained decoding server-side. Accepted: the alternative was a
  hard 400 (no answer at all).
- One more public capability method on the providers surface
  (`strict_schema_rejects_underspecified`) to keep honest per wire
  family as providers evolve (openai could relax; local servers vary).

### Neutral
- The wire-family answer is a v0.1 approximation (whole openai-compat
  family treated as its strictest member). A per-profile override can
  land later without changing the verb-layer seam.
- `base_messages` now appends the schema instruction on the JsonMode
  path too — prompts for underspecified-schema tasks grow by the
  rendered schema (capped at 4096 chars · existing render cap).

## Evidence / Affected code

- `crates/nika-verb-infer/src/lib.rs` — `SchemaWire` enum ·
  `schema_wire()` decision · `build_request()` mapping (`Strict` →
  `JsonSchema` · `JsonMode` → `Json` · else `Text`).
- `crates/nika-verb-infer/src/structured.rs` — `is_underspecified()`
  iterative walker + detection tests.
- `crates/nika-providers/src/profile.rs` —
  `WireFormat::strict_rejects_underspecified()` (the wire-family source
  of truth, sibling of `supports_response_format()`).
- `crates/nika-providers/src/registry.rs` —
  `ResolvedProvider::strict_schema_rejects_underspecified()`.
- Field report: `2026-07-04` atlas dogfooding F2 (repro ladder:
  `{type: object}` → 400 · partially-specified → 400 · full recursive
  spec impossible for free-form payloads).

## Alternatives considered

### Alt A — `infer.json: true` (or `structured: passthrough`) YAML sugar
An explicit author opt-in to JSON mode. Rejected: it adds a second way
to say what the schema already says (`{type: object}` IS the free-form
declaration), grows the spec surface forever (`nika: v1` envelope is
frozen — additions are one-way doors), and forces every author to learn
which providers need the flag. The engine has all the information to
decide.

### Alt B — full recursive schema specification by the author
Status quo. Rejected: impossible by construction for free-form payloads
(the translate-payload class — the shape is the INPUT's shape, unknown
at authoring time). The field report proved authors fall back to
schema-free promptware, which is strictly worse than local validation.

### Alt C — normalize/tighten the schema before forwarding to strict mode
Synthesize `properties: {}` + `additionalProperties: true` rungs to
appease the strict validator. Rejected: OpenAI strict mode *requires*
`additionalProperties: false` — a "tightened" free-form object would
actively reject the payload keys the author wants passed through. The
existing openai `normalize_strict_schema` pass stays scoped to
fully-specified schemas.

## Related

- ADR-092 (check ladder · audit-before-run)
- `docs/adr/adr-095-exec-security-architecture.md` (the same
  conservative-default philosophy at another seam)
- Shadow zone Gate 2 — cross-provider structured output parity
  (`.claude/rules/diamond-discipline.md` Rule 5)

## Notes

Revisit triggers: (a) OpenAI relaxes strict-mode underspecification
handling — flip the family answer per profile; (b) gemini's
`additionalProperties` limitation decision lands (operator-gated ·
memory 2026-07-01) — the gemini adapter path may then prefer JsonMode
more broadly; (c) a per-profile capability override surfaces in
`ProvidersConfig` — move the answer from wire family to profile.
