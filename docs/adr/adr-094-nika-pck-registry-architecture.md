---
id: ADR-094
title: "nika-pck · content-addressed sharing — identity decentralized, discovery a forkable index, trust in the artifact"
status: accepted
date: 2026-06-11
phase: "Phase 3+ · post-v0.81 (v0.9x sharing suite)"
deciders: ["@ThibautMelen"]
tags: ["pck", "registry", "packages", "cas", "signing", "conformance", "supply-chain"]
affects_crates: ["nika-pck", "nika-pck-manifest", "nika-pck-registry", "nika-git", "nika-blob", "nika-cli"]
affects_layers: ["L0", "L1", "L2", "L4"]
supersedes: []
superseded_by: []
related: ["ADR-092", "ADR-091", "ADR-003"]
requires: ["ADR-092"]
enables: []
amends: []
inv: []
nika_codes: ["NIKA-5xx band (reserved at scaffold · pck error family)"]
timeline: "manifest types may land early (L0 · zero deps); the suite ships v0.9x"
follow_ups:
  - "minisign key distribution UX (author keys in the index vs TOFU) — decide at registry scaffold"
  - "Connectome-machinery artifact class activates when the Connectome ships (shape reserved now)"
---

# ADR-094: nika-pck — content-addressed sharing with trust in the artifact

> **ACCEPTED 2026-07-08** (operator ratification · D-2026-07-08-N2): the D4
> reserved-core taxonomy is LOCKED and FCI-004's pck-types list is amended to
> match in the same PR (the prior 9-list was aspirational — `recipe`/`shield`/
> `lints` had zero code usage). This unblocks `nika-pck-manifest`, the 42nd
> crate (the 42/42 master plan §2: closed `#[non_exhaustive]` ArtifactKind of
> the 11 named classes + `CustomTool(String)` for `x-<vendor>:*`, total
> deserialization).

## Context

Nika already ships the seed of a sharing layer: `nika-pack` (a versioned
content pack — manifest + per-file sha256, embedded in the binary) and
`nika-blob` (a blake3 content-addressed store, admitted). The planned `pck`
cluster (5 crates, reserved in the crate plan) needs an architecture before
any scaffold. Meanwhile `nika check` (ADR-092) statically proves what a
workflow *does* — schema validity, DAG soundness, effects, permits, secrets
read, cost interval — **before it runs**. That proof is the design input
everything below follows from.

Two decades of package-registry failure modes inform the constraints
(install-script worms · unpublish cascades · namespace squatting · LLM-era
hallucinated-name squatting per arXiv:2406.10279 · registry capture):

1. **No code executes at install time.** Ever. Artifacts are data.
2. **Nothing installed can disappear or change.** Content-addressing +
   lockfiles, not mutable tags.
3. **A name must not be the security boundary.** Names get 2 of
   {human-meaningful, decentralized, secure} — so identity must not depend
   on a name resolver staying honest.
4. **The index must be losable.** If the discovery surface dies or
   misbehaves, installed software and pinned builds keep working and the
   index is re-hostable by anyone (it's a git repo).

## Decision

### D1 · Identity ⊥ discovery (the structural split)

- **Identity** is fully decentralized: an artifact IS its blake3 content
  hash; a *ref* is a git URL + path + version (Go-module style naming — the
  hosting platform is the namespace, no global name table to squat).
  Publishing = pushing to a git repo you own. Authors sign manifests
  (minisign · detached, offline-verifiable).
- **Discovery** is a thin, forkable, git-hosted **index**: rows of
  `ref → {hash, sig, cert-summary, tags}`. The default index is a
  convenience, not an authority — `nika` accepts any index URL, and the
  lockfile pins hashes so builds survive any index dying.

### D2 · One CAS, two object sizes

All artifact content lands in the existing blake3 CAS (`nika-blob`) — the
planned separate `nika-pck-store` crate is **folded into `nika-blob`**
(one store, one dedup domain, resumable fetch for free). Text artifacts
(workflows/packs/skills/agents/configs — KB) ride git; the one heavy class
(**model weights**, GB) is fetched from its upstream source (e.g. HF) by
hash-pinned pointer and landed in the same CAS. Weights are **GGUF /
safetensors only** — formats whose load path executes no code.

### D3 · Trust lives in the artifact (conformance certs)

Every shared artifact carries a **cert**: the `nika check` static proof
(conformance PASS · effects · permits · secrets-read enumeration · cost
interval · content sha set), signed by the author, **re-verifiable locally
by the installer** (`nika verify` re-runs the oracle — the cert is a claim
the installer's own engine re-derives, not a badge it must believe).
Install UX surfaces the cert before anything lands: what it touches, what
it may spend, what it reads.

### D4 · The artifact taxonomy is closed-but-extensible

v1 classes: workflow · pack · skill · agent-preset · template-variant ·
model-pointer · mcp-config · conformance-fixture · custom-tool declaration
(`x-<vendor>:*` — capability-scoped, the tool's *declaration* is data; any
binary it names is sandbox-scoped and never auto-fetched) · provider-profile
· policy-preset · bench-suite. A future class is **reserved, not built**:
cognitive-machinery bundles (ontology schema · retrieval-pipeline config ·
scheduling params — configuration for the memory subsystem; **never user
graph data**, which is local-only by doctrine).

### D5 · Anti-hallucination guard (LLM-era squatting)

`nika` **never auto-installs a name an LLM suggested**: `nika add` resolves
refs interactively (show author · sig state · cert summary · download
provenance) unless the exact hash is already lockfile-pinned or
`--yes --ref-hash` is passed (CI). Typo-distance warnings against the
index's existing refs apply at resolve time (the did-you-mean machinery the
CLI already has).

### D6 · Crate mapping (amended cluster)

| Crate | Layer | Role |
|---|---|---|
| `nika-pck-manifest` | L0 | manifest + cert + lockfile types (pure · serde) |
| `nika-pck-registry` | L1 | index fetch/parse · ref resolution (over kernel http seam) |
| `nika-git` | L1 | gix wrapper · ref → repo content |
| `nika-pck` | L2 | orchestrator · resolve→fetch→verify→land pipeline |
| ~~`nika-pck-store`~~ | — | **folded into `nika-blob`** (D2) |

CLI verbs (L4 · the `nika` binary): `add · remove · search · publish ·
verify · model pull|list|rm`.

## Consequences

- No central artifact server exists to operate, fund, or capture; the
  default index is a git repo anyone can fork, and losing it loses nothing
  installed.
- Supply-chain posture is structural, not moderated: data-only artifacts
  (no install hooks), hash-pinned content, offline signature + cert
  re-verification, code-free model formats.
- The cert turns `nika check` (ADR-092) into the sharing layer's trust
  primitive — one oracle serves authoring, CI, and distribution.
- Cost: signing/key UX and index curation conventions need their own pass
  (follow-ups); resumable big-blob fetch rides the CAS.

🦋 Nika — workflow engine for AI, AGPL, SuperNovae Studio.
