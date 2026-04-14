# Nika Diamond -- Release Progress

<!--
  SPDX-License-Identifier: AGPL-3.0-or-later
  Copyright (c) 2026 SuperNovae Studio
-->

Internal tracking document for the Nika Diamond rewrite.

Diamond is a full rewrite on the `nika-diamond` orphan branch. Every crate is
rewritten from scratch (not copy-pasted from legacy `main`), passes 12 admission
gates, and targets zero unwraps in `src/`.

**Version model**: **forever v0.x** (no v1.0 target, incremental quality). See
[ROADMAP.md](../../ROADMAP.md) for phase-by-phase feature inventory.

Target first tag: v0.90.0 (~11-12 months). Architecture: 40-42 crates v0.90,
expanding to ~75 at v0.100, hard cap 100 ever.

Legacy reference: `main` at `830aa6154` (read-only, accessed via `git show`).

---

## Phase 1 -- Split nika-core (5-7 weeks)

Decompose the legacy `nika-core` monolith (45k LOC) into 5 focused sub-crates:
nika-error, nika-catalog, nika-kernel + mock, nika-schema, nika-binding.

### Step 1: nika-error -- DONE

- Commit: `42909b1c7`
- Layer: L0 (pure, zero I/O, zero async)
- Design: Option C+ error strategy
  - Trait `NikaErrorCode` for structured error identity
  - `NikaError(Box<dyn NikaErrorCode>)` wrapper with `miette::Diagnostic` delegation
  - `CoreError` as the primary implementor
  - `AsAny` blanket for trait-object downcasting (explicit deref fix)
- Key types: `NikaErrorCode`, `NikaError`, `CoreError`, `NikaCode`, `Category`, `Severity`
- Code system: dual NikaCode -- wire format `NIKA-XXX` + structured constants
  - NIKA-001: validation-failed
  - NIKA-002: not-found
  - NIKA-003: unsupported
  - NIKA-999: internal
- Stats: 1,013 LOC | 44 tests | 100% mutation score | 0 unwrap in src/
- Review: 3-agent swarm, all P0/P1 fixed same session

### Step 2: nika-catalog -- DONE

- Commit: `55a451695`
- Layer: L0 (pure, zero I/O, zero async)
- Design: hybrid lookup strategy
  - `phf` + `unicase` compile-time maps for providers and MCP aliases
  - Sorted arrays with binary search for builtins, transforms, pricing
  - Case-insensitive zero-alloc via `UniCase::ascii()`
- 3 design decisions locked:
  - Case-sensitivity is per-catalog (providers insensitive, builtins exact)
  - Provider catalog is LLM-only (MCP unified into `McpAlias`, 113 entries)
  - Hybrid strategy over uniform phf (binary search better for sorted data)
- Catalogs:
  - 16 LLM providers
  - 113 MCP aliases
  - 63 builtins
  - 65 transforms
  - 61 pricing patterns
- `model_capabilities()` provider-aware with alias normalization
- Stats: 2,235 LOC | 85 tests | 94.7% mutation score (71/75 killed) | 0 unwrap in src/
- Review: 3-agent swarm, all P0/P1 fixed same session

### Step 3: nika-kernel + nika-kernel-mock -- DONE

- Commit: `ef8804371`
- Layer: L0.5 (trait definitions + mock implementations only)
- Design: ISP-layered (~20 atomic traits + ~6 super-traits). `trait_variant`
  for async support. Cortex and agent-v2 hooks (`MemoryStore`, `EmbeddingProvider`,
  `ToolExecutor`, `WasmPluginHost` reserved, `ObservabilitySink` reserved)
  baked in from day one per forward-compat invariants.
- Stats nika-kernel: ~3,700 LOC | 99 tests | 0 unwrap in src/
- Stats nika-kernel-mock: ~850 LOC | 88 tests | 0 unwrap in src/
- Review: 3-agent swarm, all P0/P1 fixed same session

### Catalog enrichment + Phase A cleanup -- DONE (2026-04-13/14)

Subsequent to Step 2 admission, nika-catalog was enriched v2 then cleaned:

- v2 enrichment `6a943e885`: +20 new aliases, 31 npm package name corrections
  (all verified on registry), +5 enterprise providers (bedrock/azure/vertex/
  perplexity/voyage). 113 → 131 aliases, 16 → 21 providers.
- Phase A cleanup `db0bf8e3f`: removed 29 broken aliases after 5-agent audit
  (7 deprecated Anthropic reference servers, 18 non-existent npm packages,
  3 zero-download abandoned forks). 131 → 102 aliases.
- New test `removed_broken_aliases_not_present` blocks regression.

### Step 4: nika-schema -- PLANNED (NEXT)

AST + semantic analyzer + DAG resolution. Single crate (no ast/analyze split
due to circular dependency risk). ~15k LOC budget (biggest L0).

### Step 4: nika-schema -- PLANNED

AST + semantic analyzer + DAG resolution. Single crate (no ast/analyze split
due to circular dependency risk). ~15k LOC budget.

### Step 5: nika-binding -- PLANNED

Template engine + 65 transform implementations. `TemplateContext = BindingStore`
(locked decision). 3-phase structured output pipeline.

---

## Phase 2 -- Copy/rewrite extract-ready crates (3-4 weeks)

Rewrite the 9 crates that were cleanly separable from legacy:
nika-event, nika-display, nika-mcp, nika-security, nika-vault,
nika-clock, nika-fs, nika-http, nika-process.

Not yet started.

---

## Phase 3 -- Verbs + provider split + media split (12-15 weeks)

- 5 verb crates: exec, fetch, invoke, infer, agent
- Provider split x3: nika-provider-rig, nika-provider-native, nika-provider-mock
- Media split x5: nika-media-cas, nika-media-image, nika-media-pdf,
  nika-media-document, nika-media-provenance

Not yet started.

---

## Phase 4 -- Runtime, daemon, interfaces, shadow zones (8-10 weeks)

Runtime orchestration, daemon with embedded DB, CLI/LSP/serve interfaces.
7 pre-launch shadow zones resolved.

Not yet started.

---

## Phase 5 -- Parity + shadow zone validation (4 weeks)

Golden tests against legacy binary (`~/bin/nika-legacy`). All 7 shadow zone
gates verified green.

Not yet started.

---

## Phase 6 -- Cutover (2 weeks)

Final integration, tag `v0.90.0`.

Not yet started.

---

## Cumulative Stats

| Metric              | Value                |
| ------------------- | -------------------- |
| Crates admitted     | 4 / 40-42 (v0.90)    |
| Total LOC in src/   | ~8,900               |
| Tests               | 318                  |
| Unwraps in src/     | 0                    |
| Panics in src/      | 0                    |
| Clippy warnings     | 0                    |
| Branch HEAD         | `4ad16c22c`          |
| Last updated        | 2026-04-14           |
