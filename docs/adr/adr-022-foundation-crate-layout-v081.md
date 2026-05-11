---
id: ADR-022
title: "Foundation crate layout v0.81: 14 crates, publish = false, 5 renames + extracts"
status: accepted
date: "2026-04-16"
phase: "Phase C — foundation lock v0.81"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "layout", "publish", "rename", "split"]
affects_crates: ["nika-types", "nika-core", "nika-error", "nika-envelope", "nika-schema", "nika-schema-ast", "nika-binding", "nika-binding-transform", "nika-pck-manifest", "nika-catalog-verify", "nika-macros"]
affects_layers: ["L0", "L0.5", "L4"]
supersedes: []
superseded_by: []
related: ["ADR-021", "ADR-023", "ADR-024", "ADR-025", "ADR-026", "ADR-027", "ADR-036"]
requires: ["ADR-021"]
enables: ["ADR-027"]
amends: ["ADR-006"]
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.81"
follow_ups: ["yank nika-core@0.74, nika-event@0.74, nika-engine@0.47 from crates.io"]
---

# ADR-022: Foundation crate layout v0.81

## Context

After multi-agent architectural review (rust-architect + web-researcher SOTA
workspaces + rust-pro + Explore file audit, 2026-04-16), the v0.80 foundation
layout (10 crate plan, 7 in workspace) is replaced by a 14-crate layout with
strict tier discipline, `publish = false` strategy, and 5 structural changes.

## Decision

### The 14-crate constellation

```
L0-tier-0 (LEAVES — zero nika-* deps):
  nika-core              [RENAME from nika-types]
  nika-macros            [NEW proc-macro stub]

L0-tier-1 (depend on tier-0):
  nika-error             [DROP nika-types re-export facade]
  nika-envelope          [NEW — apiVersion+Kind+Metadata+multi-doc+PckSpec]

L0-tier-2 (depend on tier-0+1):
  nika-event             [22 scoped sub-enums]
  nika-binding-transform [NEW — 65 pure transforms extracted]
  nika-catalog-codegen   [extract from catalog/build.rs]
  nika-schema-ast        [NEW — Raw + Analyzed AST types]

L0-tier-3:
  nika-catalog           [internal split data/models.rs + build/]
  nika-binding           [template engine + dispatch]
  nika-schema            [parser + analyzer + DAG + taint + jsonsubset]

L0.5:
  nika-kernel            [40 traits, NO split forever per ADR-006-amend]
  nika-kernel-mock

L4:
  nika-catalog-verify    [RELABEL from L0]
```

### `publish = false` for ALL foundation crates

Every foundation crate (13 internal + 1 L4) carries `publish = false`. Only
`nika` (binary) and future `nika-sdk` ever ship to crates.io.

### 5 structural changes

1. **Rename `nika-types` → `nika-core`** (matches helix/bevy/ra/polkadot
   convention; `nika-core@0.74.0` legacy on crates.io gets yanked)
2. **Extract `nika-envelope`** (single highest-ROI move — shared by 9 doc kinds)
3. **Drop `nika-error → nika_types::*` facade** (re-export pollution gone with
   0 users)
4. **Split `nika-schema` into 2** (`nika-schema-ast` + `nika-schema`) — circular-
   dep concern from original spec was wrong (analyzer reads raw, writes analyzed)
5. **Split `nika-binding` into 2** (`nika-binding-transform` + `nika-binding`) —
   65 pure transforms have known 2nd consumer (verb-exec basename)

### `nika-pck-manifest` MERGES into `nika-envelope`

A pck manifest IS an envelope document (`Envelope<PckSpec>`). Eliminates 1
crate, conceptually cleaner.

### `nika-catalog-verify` RELABEL L0 → L4

Already does HTTP. Wrong layer assignment. Free fix.

### `nika-macros` proc-macro stub now (empty)

Per Agent 2 SOTA finding: every surveyed Rust workspace (rust-analyzer, biome,
ruff, uv, bevy, oxc) has a companion `*_macros` crate. Adding it later requires
L0 shuffle; adding empty now is free insurance.

## Consequences

- ✅ 14 crates with crystal-clear single-responsibility
- ✅ Tier discipline enables `cargo public-api` snapshots without churn
- ✅ `publish = false` frees rename/refactor velocity forever
- ✅ Renames legitimate (no users)
- ❌ ~7 commits of refactor work in Phase D (nuke + redo)
- ❌ Existing nika-types imports across all admitted crates need update
  (mechanical — find/replace + cargo check)
- ⚠️ The legacy `crates/nika-core/` excluded directory (v0.79 main-leftover)
  must be deleted before the rename — handled in Phase D.1

## Alternatives considered

- **Keep nika-types** (no rename) — name is weak placeholder, every other
  improvement depends on knowing canonical name
- **Split nika-schema into 4** (types + parser + analyzer + jsonsubset) —
  over-fragmented, 2 is the right number per cohesion analysis
- **Polkadot-style 14 micro-crates for nika-core** (nika-id, nika-cost, ...) —
  Q2 anti-pattern, no consumer-side benefit
- **Publish all foundation crates** — 0 users, no value, locks API surface
  prematurely

## Reference

- `~/.claude/.../memory/project_foundation_v081_constellation.md`
- `~/.claude/.../memory/feedback_publish_false_foundation_strategy.md`
