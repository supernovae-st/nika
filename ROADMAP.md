# Nika Roadmap

**Philosophy: Forever v0.x.** Nika increments quality in v0.x releases
indefinitely. There is no v1.0 target. Each release is diamond-grade for its
declared scope. Unshipped features are on this roadmap — **nothing ships as a
surprise, nothing ships half-baked**.

> "Diamond is a CRAFT STANDARD, not a scope list. SQLite 1.0 didn't have WAL,
> FTS, JSON1, or window functions. Those came across 3.x releases over 20 years.
> Each release was diamond-grade. The product was complete AT EACH RELEASE for
> what it claimed to do."

## Current state — v0.80.x (April 2026)

Diamond foundation, 4 crates admitted to workspace, orphan branch from scratch.

- **nika-error** — error infrastructure (44 tests, 100% mutation, `42909b1c7`)
- **nika-catalog** — static catalogs with phf+unicase lookup (87 tests, `db0bf8e3f`)
  - 102 MCP aliases (post Phase A cleanup of 29 broken entries)
  - 21 LLM providers (7 cloud + 7 OpenAI-compat + 5 enterprise + native + mock)
  - 63 builtins, 65 transforms, 61 pricing entries
- **nika-kernel + nika-kernel-mock** — kernel traits + mock impls (99 + 88 tests, `ef8804371`)

Total: 318 tests, 0 clippy warnings, 0 unwrap in `src/`.

## v0.90 — "Diamond Foundation" (target: May 2026)

**Theme**: complete, runtime-grade workflow engine + package manager MVP +
native API adapters + forward-compatible architecture for everything planned
afterward.

### Runtime-complete

**Core engine**: schema, binding, event, runtime, daemon, cli, 5 verbs
(infer/exec/fetch/invoke/agent).

**Catalog v3**: TOML source-of-truth + `build.rs` + `phf_codegen` compile-time
generation. `xtask verify-catalog` with parallel npm/pypi/docker registry
checks. GitHub Actions daily cron for drift detection. Multi-distribution
support: `Distribution::{Npm,Pypi,Oci,Remote,Binary,Mcpb}` aligned with
official MCP Registry schema (`packages[]` + `remotes[]`).

**Providers**: 21 shipped. Split across 3 crates:
- `nika-provider-rig` — 7 cloud + 7 OpenAI-compat
- `nika-provider-native` — GGUF via mistral.rs (feature-gated)
- `nika-provider-mock` — deterministic tests

**Default models updated to April 2026 flagships**: Claude Sonnet 4.5, GPT-5,
Grok 4, Gemini 3.0 Pro, Command A, Jamba 1.6, Voyage 3.5, cross-region Bedrock
profiles. Pricing entries for `gpt-5-{mini,nano}`, Perplexity Sonar (all
variants), Cohere, AI21 Jamba, Voyage embeddings, Grok 4 variants.

**Security (7 shadow zones GREEN, non-negotiable)**:
- Gate 1: `nika serve` input trust (P0)
- Gate 2: cross-provider structured output parity (P0)
- Gate 3: binding/template hardening
- Gate 4: L1 taint runtime
- Gate 5: `for_each` per-element spotlight
- Gate 6: `NikaError` Display parity
- Gate 7: provider parity matrix

### Native API adapters (7 builtins)

Thin workflow-grade wrappers over existing Rust SDKs — NOT CLI reimplementations.
Each adapter ≤500 LOC, covers 10-15 most-requested operations:

- `nika:github` (octocrab, ~800 LOC)
- `nika:aws` (aws-sdk-{s3,ec2,iam}, ~600 LOC)
- `nika:stripe` (async-stripe, ~400 LOC)
- `nika:cloudflare` (cloudflare-rs, ~500 LOC)
- `nika:slack` (slack-morphism, ~400 LOC)
- `nika:notion` (thin reqwest, ~400 LOC)
- `nika:vercel` (thin reqwest, ~500 LOC)

Packaged in 3 bundle crates: `nika-builtin-github`, `nika-builtin-cloud`,
`nika-builtin-workspace`.

### nika pck MVP — the Cargo of AI workflows

**Positioning**: NOT "the npm of AI". Opinionated, signed, versioned,
reproducible, eval-backed. Small sharp type system.

**Architecture**: git-repo registry (homebrew-tap pattern) + direct-from-GitHub
fallback (`nika pck install github.com/user/repo`). Zero central HTTP
infrastructure. PR-based publishing for quality gate.

**9 first-class content types** (classified by runtime maturity):

🥇 **Gold (runtime-complete)** — day-one public registry:
1. **workflow** — `.nika.yaml` DAG, the atomic unit
2. **skill** — markdown prompt (absorbs IDE rulesets via `targets:` field for
   Claude Code / Cursor / Windsurf / Zed)
3. **agent** — agent preset (verb + tools + completion + guardrails + budget)
4. **provider** — LLM provider + auth (absorbs Model presets via `[[models]]` inline)
5. **mcp** — alias → npm/pypi/oci server (Distribution enum)

🥈 **Beta (spec stable, runtime partial)** — `--experimental` flag:
6. **eval** — dataset + assertions (foundational trust layer)
7. **recipe** — meta-bundle (workflow + skills + providers + mcp + eval).
   The killer primitive. `create-react-app` for AI.
8. **shield** — security policy (trust/taint/spotlight/canary — Nika-unique moat)

🧪 **Experimental** — feature-flagged:
9. **lints** — dylint + workflow lint packs

**Crates** (~10k LOC):
- `nika-pck-manifest` (L0, ~1.5k LOC) — TOML types + validation
- `nika-pck-registry` (L1, ~2.5k LOC) — git clone via `gix`, index resolve
- `nika-pck-store` (L1, ~1.8k LOC) — `~/.nika/pkg/` layout, `pck.lock`
- `nika-pck` (L2, ~3k LOC) — install/publish/search/info/remove orchestration
- `nika-git` (L1, ~1.5k LOC) — gix wrapper, `GitClient` trait impl

**Commands**: `nika pck install <spec>`, `publish`, `search <query>`,
`info <name>`, `remove <name>`, `list`, `update`, `audit`.

**Extensibility**: `nika.*` core namespace reserved. Community uses `x-*` or
reverse-DNS (`com.acme.dashboard/v1`). Promotion path: `x-foo` → core requires
100+ installs + RFC + 2 Nika team reviewers.

### Sharing primitive: `nika init --from`

Git-native template scaffold. Zero registry dependency.

```bash
nika init --from github.com/user/awesome-workflow
```

Covers 80% of the "share my setup" use case. Pre-pck. Zero infrastructure.

### Forward-compatible kernel hooks (Cortex + agent-v2 ready without v0.90 refactor)

**Kernel traits defined, mock impls only** — real implementations arrive in
v0.95+ without touching v0.90 code:

- `MemoryStore` trait — Oxigraph impl in v0.95
- `EmbeddingProvider` trait — provider impls in v0.95
- `ToolExecutor` trait — **real impl v0.90** (native builtins)
- `WasmPluginHost` trait — impl in v0.100+
- `ObservabilitySink` trait — impl in v0.100+

`InferRequest` reserves `memory: Option<MemoryDirective>` (v0.95),
`budget: Option<BudgetDirective>` (v0.100), `replay_seed: Option<u64>` (v0.100).
All structs `#[non_exhaustive]` — future additions are NOT breaking changes.

## v0.95 — "Memory Complete" (target: Oct-Dec 2026)

**Theme**: Cortex + agent-v2 ship complete, on foundations proven in v0.90.

### Cortex (9-10 new crates, ~30-40k LOC)

- `nika-memory-core` — memory types, episodic/semantic/procedural taxonomy
- `nika-memory-oxigraph` — RDF triple store + SPARQL endpoint
- `nika-memory-fsrs` — FSRS-6 spaced repetition for agent learning
- `nika-memory-owl2` — OWL2 ontology reasoning
- `nika-memory-embed` — embedding provider abstraction
- `nika-memory-retrieval` — semantic + hybrid retrieval
- `nika-memory-reasoning` — captured reasoning traces (sanitized)

**Shareable via pck** (reference external, do NOT redistribute — W3C/Anki/HF
own those ecosystems):
- `recipe` manifests can reference external ontologies (Schema.org, BFO)
- Anki deck import/export compatibility
- HuggingFace embedding collection references

### Agent-v2 (~8-10k LOC)

- Parallel tool calls
- ReWOO planning
- Reflection loops
- Resume with state snapshots
- Context compression
- 4-type guardrails (length + schema + regex + LLM judge)

### pck full

- sigstore keyless signing (mandatory for community publish)
- PubGrub semver resolver
- Automated PR flow for `nika pck publish`
- 3 beta types (eval/recipe/shield) graduate to gold
- Community registry seeded with 20-30 `supernovae/*` recipes

### Additional native adapters (8 more)

`nika:linear`, `nika:huggingface`, `nika:ollama`, `nika:supabase`,
`nika:discord`, `nika:resend`, `nika:elevenlabs`, `nika:replicate`.

### Media subsystem (5-crate split)

- `nika-media-cas` — content-addressed storage
- `nika-media-image` — thumbnail/convert/strip/optimize/phash
- `nika-media-pdf` — PDF text extraction
- `nika-media-document` — html_to_md, readability
- `nika-media-provenance` — C2PA

## v0.100 — "Ecosystem" (target: Mid-2027)

- **WASM community plugins** (wasmtime + extism sandbox) — third-party native
  builtins via `nika pck install builtin:...` with signed WASM blobs
- **Full observability**: OpenTelemetry, `tracing` spans, trace replay, budget
  enforcement, metrics (tokio-metrics integration)
- **LSP full**: autocomplete, hover docs, go-to-definition, inline diagnostics
- **Scheduled triggers**: cron-like workflow scheduling
- **Keys subsystem**: macOS Keychain + Linux keyring + OAuth device flow
- **Public launch**: `nika.sh`, community registry promotion, blog launch

## v0.110+ — Ongoing (2027+)

Incremental quality additions. Order determined by community feedback + Nika
team priorities. Possible directions include (no commitment):

- **Hosted cloud runner** (`nika.sh` as optional SaaS) — self-host remains primary
- **Commercial relicense** (AGPL → commercial for enterprise customers)
- **Mobile SDK** (iOS/Android bindings to local Nika runtime)
- **RAG as composable primitive** (distinct from agent memory)
- **Enterprise features**: SSO, audit log retention, compliance dashboards
- **Performance**: hot-path optimization, zero-copy, SIMD for media
- **Security audits**: external review, fuzzing campaigns, supply chain
- **Additional native adapters**: ranked by usage telemetry

## What this roadmap deliberately excludes

- **Visual no-code editor** (anti-positioning — `.nika.yaml` is the user-facing format)
- **Chat UI / consumer assistant** (GPT Store territory)
- **Own model training infrastructure** (HuggingFace territory)

Nothing is forbidden forever. Items are re-evaluated each phase.

## Governance

Nika is open source under AGPL-3.0-or-later with commercial relicense
available. **Benevolent dictator model**: Thibaut Melen (SuperNovae Studio) is
the final arbiter on architecture, scope, and release. Community contributes
via pull request.

Trusted reviewers may be delegated as the community grows. No DAO, no voting.

## Release cadence

- **Major increment** (v0.90 → v0.95 → v0.100): 6-12 months. Aligns with a
  theme (foundation, memory, ecosystem, etc.).
- **Minor increment** (v0.90.1, v0.90.2): 2-4 weeks. Bug fixes, incremental
  additions within the declared theme scope.
- **Point release** (v0.90.0-alpha.N): 1-2 weeks. Pre-theme-complete iterations.

## Architectural forward-compatibility guarantees

See `docs/architecture/forward-compat-invariants.md` for the full list of
patterns, decisions, and rules that ensure v0.90 architecture accommodates
v0.95, v0.100, and beyond **without breaking changes**.

🦋
