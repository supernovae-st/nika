# Nika · the intelligence layer · vision toward 2040

> **Status** · vision / north-star (non-binding direction · not a gate). The
> *what-ships-when* lives in `ROADMAP.md`; the *crate-ladder* in
> `BLUEPRINT_2036.md`; the *memory organism* in
> `nika/02-engineering/architecture/blueprint/NIKA_DIAMOND_CONNECTOME-v1.md`
> (private companion). This doc connects those into one picture and adds the
> **agent-facing intelligence layer** they don't yet cover: the LSP, the
> workflow generator, agent-comprehension (skill + MCP + llms.txt), and the
> long-horizon **Nika-OS** concept.
>
> **Grounding** · the SOTA claims below are sourced from a 2026 research sweep
> (LSP DSL tooling · constrained-decoding generation · MCP/AGENTS.md/skills
> agent-comprehension · hybrid agent memory). Sources cited inline. This is
> direction, not a phantom-feature list — every "today" claim cites the code
> that exists; every "future" claim cites the ADR/crate that would carry it.

---

## §0 · The one-sentence vision

Nika is the **sovereign workflow engine for AI** — 4 verbs (`infer · exec ·
invoke · agent`), AGPL, local-first, multi-provider — and over the 2026→2040
horizon it grows an **intelligence layer** around that core: workflows you can
*author with an LSP*, *generate from intent*, that any agent *understands
perfectly*, backed by a *cognitive memory organism* (the Connectome), running
on a *sovereign agent runtime* (Nika-OS). The engine never depends on Olympus
(cross-flow D-2026-05-08-N1); Nika-OS is the engine's *own* full-Nika runtime,
not a fork of the atelier.

---

## §1 · The five layers (where we are · where we go)

```
                          ┌─────────────────────────────────────────────┐
   author                 │  L-INTEL · the intelligence layer (this doc) │
   generate               │  LSP · generator · agent-comprehension       │
   understand             └───────────────────────┬─────────────────────┘
                                                   │ sits on top of
   ┌───────────────────────────────────────────────────────────────────┐
   │  L4 interfaces · cli · serve · mcp · lsp · sdk         (the surface) │
   │  L3 orchestration · runtime · daemon                  (the loop)     │
   │  L2 domain · verbs · connectome · catalog             (the meaning)  │
   │  L0.5 kernel · sealed traits                          (the contract) │
   │  L0 primitives · types · error · schema              (the floor)     │
   └───────────────────────────────────────────────────────────────────┘
                          the Diamond engine (BLUEPRINT_2036 · the ladder)
```

The Diamond crate-ladder (L0→L5) is the **engine**. The intelligence layer is
the **experience** — it's how a human or an agent *meets* the engine. Today the
engine is 55 crates (projected — the count is never hand-typed · ADR-037
horizon 50-90 · cap 100); the intelligence layer is mostly design + seams.

---

## §2 · The LSP · authoring `.nika.yaml` at 2040 quality

**Today's seam.** `.nika.yaml` workflows carry the nine-key envelope forever
(`nika: <id>` opens every file · ADR-113). `spec/workflow.schema.json` already gives **free completion** in any
editor via `yaml-language-server` (the canonical LSP backbone for VS Code ·
IntelliJ · Neovim) — per ADR-085, the hand-derived `invoke.tool` `oneOf`
autocompletes the builtin catalog *today*, zero engine ship required. That is
the floor, and it's already real.

**The custom `nika-lsp` (L4).** Schema-completion is necessary but not
sufficient — it can't do `${{ ... }}` binding resolution, cross-step type-flow,
go-to-definition on a task output, or semantic diagnostics that understand the
4-verb model. The SOTA Rust LSP stack (2026 sweep) is convergent:

| Concern | Crate | Why |
|---|---|---|
| Lossless CST | `rowan` | round-trips the YAML + the `${{ }}` expression sub-grammar |
| Incremental semantics | `salsa` | re-analyze only the changed step on each keystroke |
| Protocol plumbing | `tower-lsp` (established) **or** `async-lsp` (lighter, composable) | JSON-RPC to any editor |
| Schema-driven completion | derive from `workflow.schema.json` + `nika-schema` | one source → completions + validation |

The design principle (from the same sweep, and from how Pkl/CUE/Nickel/KCL ship
tooling): **the strongest completion is schema/type-driven, not text-driven** —
surface the inferred type-flow and constraint failures, not just keywords. Nika
has an advantage here: the workflow AST + the builtin/provider/verb catalog are
*already typed* in `nika-schema` + `nika-catalog`, so the LSP projects a real
type system, not a guessed one.

**Differentiation (3 angles):**
1. **`${{ }}` binding intelligence** — go-to-def from `${{ tasks.X.output.field }}`
   to the producing step, with type-flow + "did that field exist" diagnostics.
   This is the DSL-specific feature generic YAML LSPs can't do.
2. **Verb-aware diagnostics** — the LSP knows `infer`/`exec`/`invoke`/`agent`
   semantics (e.g. `invoke` needs a real builtin from the catalog), so it flags
   a hallucinated `nika:nonexistent` before the engine ever runs.
3. **Evidence-first hover** — hover a step → show its canonical error codes
   (NIKA-XXXX), its provider/builtin contract, its `is_transient` retry
   semantics. The LSP surfaces the canon, not a docstring.

**Status** · `nika-lsp` is an L4 crate in the ladder (ROADMAP v0.91). ADR-085's
`schemars` codegen path (engine generates `workflow.schema.json` from the
typed AST instead of hand-deriving it) is the bridge — trigger-gated on
`nika-schema` parser maturity + an LSP UX signal.

---

## §3 · The workflow generator · intent → valid `.nika.yaml`

**The problem.** A user describes intent ("scrape these pages, summarize each,
email me the digest") and Nika produces a *valid, runnable* workflow — correct
verbs, valid `${{ }}` bindings, schema-conformant, no hallucinated builtins.

**The SOTA approach (2026 sweep).** The consensus is **schema-grounded
constrained decoding** as the default, with **generate-validate-repair** as the
fallback — *prevent* invalid fields at generation time rather than correcting
them after. The hierarchy:

- **JSON-schema-constrained generation** when the output is structured data
  with known keys/types/enums (the bulk of a workflow) — token-masking from
  `workflow.schema.json` makes hallucinated keys impossible by construction.
- **Grammar-guided (GBNF-style)** for the syntax-sensitive parts the schema
  can't express — the `${{ }}` expression sub-language (Nika's CEL-subset).
- **Validate + repair** as the fallback loop for the semi-freeform residue
  (prompt text, descriptions).

**Why Nika is well-positioned.** The grounding artifacts *already exist*: the
generator constrains on `workflow.schema.json` (the SSOT, already LSP-perfect)
+ `canon.yaml` (the verb/builtin/provider catalog). This is the *same*
anti-hallucination discipline the engine applies to itself (ADR-090 · "project
the SSOT"): the generator can't emit a builtin the catalog doesn't declare,
because the catalog *is* the decoding constraint.

**Architecture (3 decisions):**
1. **Two-stage** — (a) plan the DAG shape (nodes · edges · verb per node) under
   the schema, (b) fill each step's params under the per-verb sub-schema. Plan
   then fill, both constrained.
2. **The schema is the constraint, not a post-check** — the generator binds to
   `workflow.schema.json` at decode time; validation is the same `nika-schema`
   parser the engine uses, so "valid for the generator" ≡ "valid for the
   engine" by construction (no two-source drift).
3. **Evidence-first output** — every generated step carries provenance (which
   intent clause produced it), mirroring the engine's evidence-first error
   shape. A generated workflow is *auditable*, not a black box.

**Status** · future. Lives behind the `nika-schema` parser maturity + an
`infer`-side constrained-decoding capability. The seam is the schema + catalog
that already ship. A `nika-gen` (or an `agent`-verb capability) is the carrier.

---

## §4 · Agent-comprehension · every agent understands Nika perfectly

**The goal (your ask).** Any AI coding agent — Claude Code, Cursor, Codex,
Cline, any MCP client — understands Nika deeply enough to author correct
workflows, and that understanding is wired **at install time**.

**The convergent 2026 standard (sweep).** Four layers, each owning a concern:

| Layer | Owns | Nika today | Gap |
|---|---|---|---|
| **MCP server** (tools · resources · prompts) | runtime capability discovery + invocation | `nika-mcp` (L2) + `nika-mcp-server` (L4) planned | ship resources (the catalog, the schema) + prompts (workflow templates), not just tools |
| **AGENTS.md** | repo-local operating manual | ✅ exists, agnostic (Claude/Cursor/Aider named, routes to rules + 12 gates), de-drifted to projection-by-default | — |
| **llms.txt** | compact model-friendly docs index | ✗ **missing** | a curated on-ramp at the repo root pointing agents at the spec + canon + examples |
| **skills** (progressive disclosure) | task-specific authoring guidance | ✅ marketplace `spn-nika/*` (crate-dev, arch, builtin-migration) | a `nika-workflow-author` skill (intent → workflow, using the generator + LSP discipline) |

**The install-time wire.** The SOTA move (how modern frameworks ship AI-context
on install — sweep) is that *the framework ships its own agent-facing knowledge
base* and wires it on setup. For Nika: `nika init` (the L4 onboarding crate)
should, on a project, drop/refresh an `AGENTS.md` pointer + register the
`nika-mcp-server` for the user's agent client + surface `llms.txt`. One install,
every agent comprehends.

**3 SOTA-2040 moves:**
1. **MCP resources = the live catalog** — expose `canon.yaml` (verbs · builtins
   · providers) + `workflow.schema.json` as MCP *resources* so an agent reads
   the *current* surface, not a stale doc. The pre-claim discipline (the agent
   queries the catalog before asserting a builtin exists) becomes a resource
   read, not a guess.
2. **MCP prompts = workflow templates** — ship canonical workflow shapes as MCP
   prompts (the "scrape→summarize→notify" pattern), so an agent composes from
   verified templates.
3. **llms.txt + the spec as the knowledge base** — the engine *is* its own
   agent-facing knowledge base (the spec repo is Apache-public, structured,
   example-rich). `llms.txt` is the index; the agent on-ramps in one read.

---

## §5 · The Connectome · the cognitive memory organism

**What it is** (private companion · `NIKA_DIAMOND_CONNECTOME-v1.md`). The L2
memory layer: 1 orchestrator (`nika-connectome`) + 9 L1 satellites (HNSW ·
BM25 · RRF · FSRS · graph-algos · RDFS-reasoner · temporal · autodesc-minimal ·
autodesc-full), 12 cognitive mechanisms, a **zero-LLM deterministic
write-ingest path**, Oxigraph (RDF-star + W3C SPARQL 1.2) + bitemporal +
provenance. Single binary. AGPL. Local.

**Where it sits vs SOTA (2026 sweep · honest):**

| Axis | SOTA consensus 2026 | Connectome | Verdict |
|---|---|---|---|
| Hybrid recall (graph + vector + lexical) | the convergent winner — pure-vector is "one signal among several" | RDF (graph) + HNSW (vector) + BM25 (lexical) + RRF (fusion) | **on / ahead** — Nika ships the full hybrid as a single sovereign binary; the ecosystem (Mem0/Letta/Zep/Cognee/Graphiti) is *converging* toward this, mostly cloud/Python |
| Graph/RDF for structure + temporal reasoning | "better than pure vector for facts, change-over-time, multi-hop" | RDF-star + bitemporal native | **ahead** — bitemporal RDF-star locally is rare |
| FSRS / spaced-repetition forgetting | "promising but not yet dominant" | `nika-fsrs` satellite (Anki's optimal forgetting scheduler) | **ahead** — almost nobody does principled memory-decay |
| Deterministic / zero-LLM write path | "strong design direction" (provenance, auditability) | zero-LLM deterministic ingest + provenance-attached | **ahead** — the moat: memory survives without any LLM provider |
| Sovereign / local-first | the local-agent trend (Ollama, LM Studio) | single AGPL binary, multi-provider, portable RDF dumps | **on / ahead** |

**Honest gaps.** The 11 "frontier" dimensions in the Connectome blueprint
(causal · multi-modal · GraphRAG · CRDT sync · cryptographic provenance ·
datalog · energy-frugal · …) are *design*, not shipped. The Connectome itself
is Phase 1.5 (the engineering waypoint), not admitted. The vision is sound and
SOTA-aligned; the build is ahead of us.

**The differentiation (3):** (1) the *composition contract* — single binary,
single transaction, RDF-star + 12 mechanisms — is what nobody copies without
re-architecting; (2) zero-LLM write path = memory is provider-independent; (3)
FSRS decay = the memory *forgets well*, which the field hasn't solved.

---

## §6 · Nika-OS · the sovereign agent runtime (long horizon)

**The concept (your ask · new).** Beyond the engine + the intelligence layer,
the 2030+ horizon is **Nika-OS** — a full-Nika agent operating system: a
sovereign control plane where workflows, the Connectome, providers, and tools
compose into a running, persistent, self-observing agent runtime. Think "what
Olympus is to the atelier, Nika-OS is to the engine" — but built *from* Nika,
not a fork of Olympus.

**Cross-flow discipline (load-bearing).** Per `olympus-vs-nika-distinction.md`
D-2026-05-08-N1: **Olympus consumes Nika; Nika never depends on Olympus.**
Nika-OS is the *engine's own* runtime — it reuses the Diamond crates (runtime,
daemon, connectome, mcp) and adds the OS-grade concerns (supervision,
persistence, multi-agent shared-graph, scheduling). It is **not** olympus-os
re-skinned; it shares the *patterns* (single-writer journal, capability tokens,
projection-by-default, structural enforcement) the way two good systems share
good ideas — never a Cargo dependency from Nika → Olympus.

**Grounded shape (2026 sweep · "what an agent OS means in 2026"):**
- **Sovereign / local-first** — Ollama/LM-Studio-class local-first runtime;
  the user owns the loop, the memory, the providers.
- **Persistent + self-observing** — the daemon (`nika-daemon` L3) + the journal
  + the Connectome give a runtime that remembers across sessions and audits
  itself (deterministic, provenance-attached).
- **Multi-agent shared-graph** — the Connectome's frontier dimension
  (multi-agent shared-graph + CRDT sync) is the substrate for a *team* of
  agents sharing one sovereign memory.
- **Composable** — workflows + verbs + builtins + providers as the OS's
  "syscalls"; the 4-verb model is the stable kernel ABI.

**Status** · concept / north-star. The carrier crates exist as ladder entries
(`nika-runtime` L3, `nika-daemon` L3, `nika-supervisor`, `nika-connectome` L2,
`nika-mcp-server` L4). Nika-OS is what they *become* when composed at v0.95+.
No new ADR yet — this doc is the first cadrage; a `nika-os` ADR is the next
artifact when the carrier crates are admitted.

---

## §7 · The through-line · structural enforcement everywhere

Every layer above shares one discipline — the one this engine already lives by
and just hardened (ADR-090): **the doctrine is the SSOT; the gate/LSP/generator
projects it.** The schema drives the LSP completion *and* the generator
constraint *and* the validator *and* the MCP resource. The catalog drives the
agent's pre-claim *and* the generator's builtin set *and* the LSP's diagnostics.
The error registry drives the hover *and* the runtime *and* the audit. One
source, N projections, zero drift — from the gates that admit a crate, up to the
agent that authors a workflow. That is the 2040 bet: not more features, but a
*single coherent surface* that an agent and a human meet the same way.

---

## §8 · TODO · the ordered seams (direction, not commitment)

These are *seams already present in the ladder*, ordered by dependency. Each is
trigger-gated (LOCK-031 spirit · no infra behind a locked gate); none is a dated
promise.

1. **`workflow.schema.json` from schemars codegen** (ADR-085 bridge) — when
   `nika-schema` parser matures. Unlocks: a single typed source for LSP +
   generator + validator.
2. **`nika-lsp` MVP** (L4) — schema-completion → `${{ }}` binding resolution →
   verb-aware diagnostics. rowan + salsa + tower-lsp/async-lsp.
3. **`llms.txt` at the spec/engine root** — the agent on-ramp (the cheapest,
   highest-leverage agent-comprehension move; ship-able now).
4. **`nika-mcp-server` resources + prompts** (L4) — expose canon + schema as
   resources, workflow templates as prompts (not just tools).
5. **`nika init` agent-wire** — install-time AGENTS.md pointer + MCP
   registration + llms.txt surface.
6. **`nika-workflow-author` skill** — intent → workflow, using the generator +
   the LSP discipline.
7. **The workflow generator** — schema-grounded constrained decoding on
   `workflow.schema.json` + `canon.yaml`. Carrier: `agent` verb capability or
   `nika-gen`.
8. **The Connectome** (Phase 1.5) — the 10 satellites + orchestrator admitted.
9. **Nika-OS** (v0.95+) — compose runtime + daemon + connectome + mcp into the
   sovereign agent runtime. First `nika-os` ADR.

---

## §9 · Related (canonical sources)

- `ROADMAP.md` — what ships when (the phases, the tag scheme)
- `docs/architecture/BLUEPRINT_2036.md` — the crate ladder (ADR-037 horizon 50-90 · cap 100) + 7 future ADRs + SOTA-2030
- `docs/architecture/forward-compat-invariants.md` — the 8 patterns / 10 rules (Gate 12)
- `docs/adr/adr-085-spec-schema-oneof-bridge-to-schemars-codegen.md` — the LSP + schema-codegen seam
- `docs/adr/adr-090-structural-doctrine-enforcement.md` — the SSOT-projection discipline (this doc's through-line)
- `nika/02-engineering/architecture/blueprint/NIKA_DIAMOND_CONNECTOME-v1.md` — the memory organism (private companion)
- `AGENTS.md` — the agnostic agent entry (Claude/Cursor/Aider/Codex)
- `spec/canon.yaml` + `spec/workflow.schema.json` — the SSOT the whole layer projects

SOTA sources (2026 sweep, inline above): LSP DSL tooling (Pkl/CUE/Nickel/KCL ·
rowan/salsa/tower-lsp) · constrained-decoding generation (schema/grammar-guided
vs validate-repair) · agent-comprehension (MCP tools/resources/prompts ·
AGENTS.md · llms.txt · Anthropic skills · ai.engineer 2026 Q1 report) · hybrid
agent memory (Mem0/Letta/Zep/Cognee/Graphiti · graph+vector+lexical consensus ·
H-Mem · Supermemory).
