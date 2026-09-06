---
id: ADR-124
title: "one typed oracle facade: the CLI verb, the MCP tool and the session read the same audit"
status: accepted
date: "2026-09-03"
phase: "pre-1.0 · one door"
deciders: ["@ThibautMelen"]
tags: ["architecture", "oracle", "mcp", "check", "parity", "one-door"]
affects_crates: ["nika-cli-host", "nika-cli", "nika-mcp"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-122", "ADR-123", "ADR-003"]
requires: ["ADR-123"]
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.118"
follow_ups: ["the session (wave 4) reads the facade directly, never the MCP transport", "the LSP's diagnostics lane folds the same audit"]
---

# ADR-124: one typed oracle facade

## Context

The static audit had two machine doors and two implementations. The CLI
verb (`nika check --json`) judged through `check_composed` with a
filesystem reader, folded skills, drift, the access plan, pricing, the
budget and the engine identity, and judged a templated `model:` by its
declared default. The MCP tool (`nika_check`) judged through the pure
`check`, carried its own `model_crosscheck` (a hand-written twin of the
CLI's rung that did not know templated seats), its own verdict prose and
its own explain ladder, and its `fix: true` walked the repair ladder
WITHOUT the CLI's prepass (a bare `exec:` scalar · a `needs:` list) while
its description claimed « the same ladder ». Census D of the one-door
arc counted the drift: the two lanes agreed on five keys and disagreed
on twelve, and the pack's oracle law (pack 12 §9) reads:

> Direct capability result ≈ CLI machine projection ≈ MCP tool semantic
> result, for the same input. Projection formatting can differ. Semantic
> verdict must not.

## Decision

1. **`nika_cli_host::oracle` is the ONE audit.** `audit_source` parses,
   judges (composed when a reader is given · child-blind when it is not,
   and the verdict SAYS which), resolves the frozen access plan
   (ADR-122), folds the MODELS rung (resolution · thinking · capacity ·
   the templated-default law), the skills, the four layered verdicts
   (ADR-123) and the risk grade into a typed `Audit`. `audit_json`
   projects the ONE verdict object every machine lane emits.
2. **The CLI verb and the MCP tool are projections of that `Audit`.**
   `check --json` is `audit_json` plus the CLI's own decorations (the
   ambient budget it can see from its cwd, the engine identity). The
   MCP `nika_check` result is `audit_json` plus `next_actions`; its
   green text is rendered from the same `Audit`. `model_crosscheck` is
   deleted. The MCP lane reads `composition_judged: false` (it has no
   filesystem) instead of claiming a composed workflow clean.
3. **One repair ladder.** The prepass (`apply_prepass`: the bare `exec:`
   scalar · the `needs:` list) moves beside the ladder in
   `nika_cli_host::fix_ladder`; the MCP `fix: true` runs it first, so
   « the same ladder » is true.
4. **One explain ladder.** `nika_cli_host::explain::explain(code)` is
   the four-rung ladder (hint kinds · the registry · the spec rows · the
   namespaces); the CLI renders it with its theme, the MCP tool returns
   it verbatim.
5. **The plugin's teaching surface is derived, never typed.** A test
   pins every example slug the engine-owned plugin teaches to
   `nika_pack::example_slugs()`; the six dead slugs it taught are gone.
6. **The oracle stays read-only** (pack 12 §6 · #1303's wall). It learns
   RUN READY (the four verdicts ride the MCP payload — presence on THIS
   machine, never a dial) but never gains an execution verb.

## Consequences

- An agent through the oracle and an operator at the CLI read the same
  verdict for the same bytes; a divergence is a failing test, not a
  gauntlet finding.
- Proof: `crates/nika-cli/tests/oracle_parity_e2e.rs` drives the three
  doors on the same fixtures (a clean file · a dirty file · a
  hallucinated model · a templated default · a capacity cap · a
  native-first exec under strict) and compares the semantic keys
  (`clean` · `findings[].code` · `models_resolve` · `model_findings` ·
  `models_catalog_warnings` · `verdicts` · `risk_grade` · `paid_ready`);
  the MCP door is driven over the real stdio transport. The host's
  facade tests · the MCP repair tests now assert the prepass · the
  plugin slug test.
- `REPORT_VERSION` stays 1: every key is additive (`composition_judged`
  · `next_actions` on the CLI lane · `verdicts` and `access_plan` on the
  MCP lane).

## Amendments

- Wave 3.b (the W3 gauntlet · 2026-09-03): the oracle's clean answer
  names the child workflows it did not read (`judged.children` · a prose
  line on the clean lane), `verbose: true` returns the verdict object on
  a clean answer, and `next_actions` name `nika_explain` — the door an
  agent without a shell can open.

## Follow-ups

- The session (wave 4) grounds itself on the facade directly — the
  capability registry path of pack 12 §3, never a JSON-RPC round trip.
- The LSP diagnostics lane folds the same `Audit`.
