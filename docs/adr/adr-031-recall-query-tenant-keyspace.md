---
id: ADR-031
title: "RecallQuery.tenant reservation (multi-tenant keyspace seed)"
status: accepted
date: "2026-04-17"
phase: "Phase D — Wave 4A R3"
deciders: ["@ThibautMelen"]
tags: ["l0", "cortex", "multi-tenant", "tenant-id", "reservation", "forward-compat"]
affects_crates: ["nika-types", "nika-kernel"]
affects_layers: ["L0", "L0.5"]
supersedes: []
superseded_by: []
related: ["ADR-028", "ADR-030", "ADR-033", "ADR-034"]
requires: ["ADR-028", "ADR-033"]
enables: []
amends: []
fci: ["FCI-035"]
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.81.0-alpha.4, Wave 4A R3 seed · prose authored + accepted 2026-07-12 (merged-code rule)"
follow_ups:
  - "Populate `tenant` from the execution context when multi-tenant recall ships (the field and the scoped constructor are wired; the writer is not)"
---

# ADR-031: RecallQuery.tenant reservation (multi-tenant keyspace seed)

Status flipped `proposed → accepted` 2026-07-12 under the merged-code
rule (the #474 ADR ruling): the decision is load-bearing in shipped
code — `RecallQuery.tenant` exists, is mandatory, and every recall
scope flows through it. The prose documents what IS, including where
the shipped shape deliberately hardened beyond the original seed.

## Context

Wave 4A R3 (commit `41e8a1467`, 2026-04-17) reserved a tenant keyspace
on `RecallQuery` so multi-tenant recall can ship later without a
breaking change (ADR-028 reservation policy). The reservation exists so
that a v0.8x consumer compiled against a single-tenant world keeps
working the day a multi-tenant host arrives — and so that cross-tenant
reads are structurally impossible rather than policy-forbidden.

## Decision

- `RecallQuery` carries `pub tenant: TenantId`
  (`nika-kernel-ai/src/memory.rs` — re-exported through `nika-kernel`)
  — **mandatory, not `Option`**. The original seed sketched
  `Option<TenantId>` with `None` = default; the shipped shape hardened:
  an optional tenant invites every store impl to invent its own `None`
  policy, which is exactly how cross-tenant leaks are born. Instead:
  - `RecallQuery::new(…)` scopes to `TenantId::default_tenant()` —
    single-tenant deployments use it and never think about the field;
  - `RecallQuery::scoped(tenant, …)` is the multi-tenant constructor;
  - every recall MUST scope by the field (the struct doc states the
    contract; store impls return only same-tenant frames).
- The struct is `#[non_exhaustive]` with constructor discipline
  (FCI invariant #19), so later filters (the `observed_after/_before`
  temporal pair rode in on the same shape) land without a major.

## Alternatives considered

- **`Option<TenantId>`, `None` = default tenant** (the seed's sketch) —
  rejected at hardening: per-impl `None` policies are the leak vector;
  a mandatory field with a defaulting constructor gives the same
  single-tenant ergonomics with ONE policy instead of N.
- **Tenant as a store-construction parameter only** — rejected: it
  forces one store instance per tenant (hosts want pooled stores) and
  removes the per-query audit trail.
- **No reservation until multi-tenant ships** — rejected per ADR-028:
  adding a mandatory field to a serialized, cross-layer query type
  later is the breaking change the policy exists to prevent.

## Empirical validation (what makes this "merged code")

- `nika-kernel-ai/src/memory.rs` — the field with its mandatory-scope
  contract in the struct docs, `new()` defaulting to
  `TenantId::default_tenant()` (line 113), `scoped()` for multi-tenant
  hosts (line 119).
- `TenantId` lives beside the provider types (ADR-033's family) and
  serializes with the query.
- The mock kernel exercises the same shape — recall tests construct
  queries through the defaulting constructor.

## See also

- ADR-028 — Forward-compat reservation policy (this ADR is an instance).
- ADR-030 — `MemoryFrameRef.trust` (the sibling Wave 4A reservation —
  trust gates what a recall may spotlight; tenant gates what it may see).
- ADR-033 — `TenantId` newtype + `default_tenant()`.
- ADR-034 — `MemoryStore::recall` signature (tenant flows through).
- FCI-035 addendum — Wave 4A/4B reservation catalog.

🦋
