---
id: ADR-131
title: "The resident's job door is public: admission by served name, digests as optional attestations, the engine as the one producer"
status: accepted
date: "2026-09-04"
phase: "pre-1.0 · one door"
deciders: ["@ThibautMelen"]
tags: ["architecture", "serve", "one-door", "snapshot", "admission"]
affects_crates: ["nika-execution", "nika-serve", "nika-error", "nika-cli-host", "nika-cli"]
affects_layers: ["L2", "L4"]
supersedes: []
superseded_by: []
related: ["ADR-122", "ADR-128", "ADR-132"]
requires: ["ADR-122"]
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: ""
follow_ups: ["the TypeScript SDK submits by name for served workflows and drops nothing else (it already lets the engine capture)", "docs.nika.sh · the serve page teaches both forms"]
---

# ADR-131: The resident's job door is public

## Context

`POST /v1/jobs` took one body: an execution snapshot with a `digest` the
caller had to produce, whose domain was documented nowhere. The wave-7
gauntlet's operator persona spent nineteen attempts and twelve digest
constructions on it and was refused every time as `snapshot_tampered` —
a word that accuses. `GET /v1/workflows` listed a registry the resident
would never run from; `nika explain snapshot_tampered` knew no such
code (#1441). The SDK never hashed anything: it asked the local engine
(`nika check <file> --json --sdk-snapshot`) — the producer existed and
nothing said so.

The one-door law: external consumers must not reverse-engineer the
snapshot's canonicalization or hashing; one Nika owner captures the
execution world; the public SDK submits a real job without private
filesystem, Rust or hidden-endpoint knowledge.

## Decision

1. **Two forms, one admission.** `POST /v1/jobs` (and `POST /v1/check`)
   accept `{"workflow": "<name>"}` for a workflow the served registry
   lists: the resident captures its world through `ExecutionService::
   admit`, exactly as a schedule fires — the one owner of the snapshot
   and of its digest domain. The snapshot form stays for a remote world;
   it is the body `nika check <file> --json --sdk-snapshot` prints.
2. **Digests are attestations, optional.** On the wire, `digest` and the
   unit digests may be absent: the engine computes them (`decode`
   verifies a present one, computes an absent one; the stored world is
   the engine's canonical encoding, digests present from admission on).
   A present digest that mismatches is refused as `snapshot_tampered`
   with the honest words: an attestation that failed, the producer named.
3. **The producer is named everywhere.** The OpenAPI (`JobByName` ·
   `ExecutionSnapshot` · the unit `kind` legend · the jobs and check
   summaries), the listen banner, `serve --workflows`'s help.
4. **The resident's codes are taught.** `nika explain <resident code>`
   answers from `nika_error::codes::resident_help` — the same table the
   MCP `nika_explain` tool reads (one voice).

## Consequences

- Positive: a human with the binary and `curl` submits a served workflow
  by name; a remote client posts bytes and never hashes; the registry
  the resident lists is the registry it runs; a refusal teaches.
- Negative: `POST /v1/jobs` bodies without a snapshot are no longer a
  `malformed_snapshot` by construction (the by-name probe runs first);
  the OpenAPI's `required` sets shrink (additive for every existing client).
- Follow-ups: the SDK's by-name path (F9); the docs page.

## Alternatives considered

- Documenting the digest domain for clients to reimplement: rejected —
  the law forbids it, and a second implementation drifts.
- A `nika snapshot <file>` verb: rejected — the producer exists as a
  hidden `check` adapter; a verb for it would grow the surface the
  consumer program is about to prune.
