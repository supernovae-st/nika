---
id: ADR-031
title: "RecallQuery.tenant reservation (multi-tenant keyspace seed)"
status: proposed
date: "2026-04-17"
phase: "Phase D — Wave 4A R3"
deciders: ["@ThibautMelen"]
tags: ["l0", "cortex", "multi-tenant", "tenant-id", "reservation", "forward-compat"]
affects_crates: ["nika-types", "nika-kernel"]
affects_layers: ["L0", "L0.5"]
supersedes: []
superseded_by: []
related: ["ADR-028", "ADR-033", "ADR-034"]
requires: ["ADR-028", "ADR-033"]
enables: []
amends: []
fci: ["FCI-035"]
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.81.0-alpha.4, Wave 4A R3 seed (prose pending Phase C)"
follow_ups:
  - "Flesh out full rationale + alternatives during Phase C ADR prose sweep"
  - "Define tenant-scoping semantics (required vs. default vs. wildcard) in the prose version"
---

# ADR-031: RecallQuery.tenant reservation (multi-tenant keyspace seed)

> **STUB — prose pending Phase C.** Decision is load-bearing in code; this
> file exists so vector 19 (adr-orphan-proposed) can surface it at day 31
> and force full prose authoring.

## Context

Wave 4A R3 (commit `41e8a1467`, 2026-04-17) adds `tenant: Option<TenantId>`
as a reserved field on `RecallQuery` (re-exported from `nika-kernel` via
`nika-types`). The field lets v0.95 Cortex scope every recall by tenant
without breaking consumers that ship at v0.8x with a single-tenant
assumption.

## Decision (preliminary)

`RecallQuery.tenant` is `Option<TenantId>` where `None` = "use
`TenantId::default_tenant()` (single-tenant mode)". When v0.95 multi-tenant
support ships, the field becomes populated by the verb runtime from the
execution context. Memory store impls MUST scope recall by this field to
prevent cross-tenant read. Shield consumes the field to gate compliance
(Category::Tenant rules). Full rationale (required vs. optional, wildcard
semantics, tenant-ID format, migration path for single-tenant deployments)
to be authored during Phase C ADR sweep before v0.90.

## See also

- ADR-028 — Forward-compat reservation policy (this ADR is an instance).
- ADR-033 — `TenantId` newtype + `default_tenant()` constant.
- ADR-034 — `MemoryStore::recall` signature (tenant flows through).
- FCI-035 addendum — Wave 4A/4B reservation catalog.

🦋
