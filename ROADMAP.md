# Nika Roadmap

**Philosophy: Forever v0.x.** Nika increments quality in v0.x releases
indefinitely. There is no v1.0 target. Each release is diamond-grade for its
declared scope. Unshipped features are on this roadmap — **nothing ships as a
surprise, nothing ships half-baked**.

> "Diamond is a CRAFT STANDARD, not a scope list. SQLite 1.0 didn't have WAL,
> FTS, JSON1, or window functions. Those came across 3.x releases over 20 years.
> Each release was diamond-grade. The product was complete AT EACH RELEASE for
> what it claimed to do."

## Why Diamond

The legacy v0.79 codebase grew to 322k LOC across 31 crates. At that scale the
single biggest correctness risk stopped being algorithmic and started being
**context**: no AI assistant could reason about the whole system; no human
could hold it in working memory. Refactors happened by pattern-match, and
pattern-match hallucinated.

Diamond is a rewrite under one constraint: **every crate fits in an AI context
window**. 15k LOC cap per crate. 1,500 LOC cap per file. 100 lines cap per
function. 40-42 crates instead of 31, because smaller, sharper crates compose
better than fewer, bloated ones.

The math: legacy 138k-LOC monolith = ~750k tokens = 75% of a 1M-token context
just to read ONE crate. Diamond 14k-LOC crate = ~70k tokens = 7% of context.
Same task. 10x the reasoning headroom. Zero hallucination refactor.

This is why we rewrite instead of refactor. Refactoring a 322k codebase with
1,276 `.unwrap()` calls and 47 files above 1,500 LOC is not a craft activity —
it's damage control. We want to craft. So we start from zero, keep legacy as a
read-only reference (`git show main:...`), and admit each crate through 12 gates
before it joins the workspace.

See `docs/architecture/ai-velocity.md` for the full argument.

## Current state — v0.80.x (April 2026)

Diamond foundation, 5 crates admitted to workspace, orphan branch from scratch.

- **nika-error** — error infrastructure (44 tests, 100% mutation, `42909b1c7`)
- **nika-catalog** — static catalogs with phf+unicase lookup (87 tests, `db0bf8e3f`)
- **nika-catalog-verify** — online registry verifier binary (`a977e35b1`)
  - 102 MCP aliases (post Phase A cleanup of 29 broken entries)
  - 21 LLM providers (7 cloud + 7 OpenAI-compat + 5 enterprise + native + mock)
  - 63 builtins, 65 transforms, 61 pricing entries
- **nika-kernel + nika-kernel-mock** — kernel traits + mock impls (99 + 88 tests, `ef8804371`)

Total: 318 tests, 0 clippy warnings, 0 unwrap in `src/`.

## v0.90 — "Diamond Foundation" (ships when ready)

> **No deadline.** Quality > speed. This roadmap is a vision, not a production
> schedule. v0.90 ships when all 40-42 crates pass 12 gates and the 7 shadow
> zones are green. Could be 6 months. Could be 18 months. Whatever it takes.

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

**Providers**: 25 shipped (+4 Tier 1 additions from provider audit). Split across 3 crates:
- `nika-provider-rig` — 7 cloud + 7 OpenAI-compat
- `nika-provider-native` — GGUF via mistral.rs (feature-gated)
- `nika-provider-mock` — deterministic tests

**Tier 1 additions** (high-priority providers):
- `moonshot` — Kimi K2 (1M+ context, top-tier open-weight)
- `qwen` — Alibaba DashScope (Qwen3-Max competitive with GPT-5/Claude 4.5)
- `deepinfra` — OpenAI-compat, popular open-weight hosting
- `novita` — OpenAI-compat, growing rapidly

**Default models updated to April 2026 flagships**: Claude Sonnet 4.5, GPT-5,
Grok 4, Gemini 3.0 Pro, Command A, Jamba 1.6, Voyage 3.5, cross-region Bedrock
profiles. Pricing entries for `gpt-5-{mini,nano}`, Perplexity Sonar (all 4
variants: sonar/sonar-pro/sonar-reasoning/sonar-deep-research), Cohere,
AI21 Jamba 1.6, Voyage embeddings (voyage-3.5/voyage-3.5-lite/voyage-code-3),
Grok 4 variants (grok-4/grok-4-fast).

**Model metadata expanded** (all models):
- `context_window_tokens: u32` — critical for `| token_count` sanity + cost
  estimation + fallback decisions
- `max_output_tokens: u32` — per-model output caps
- Per-provider capabilities (vision, thinking, structured output)

**Catalog P1 improvements** (fix silent bugs + UX):
- Scope `find_pricing` by provider (fix silent wrong-cost on custom endpoints)
- Enforce pricing-pattern ordering at compile time (no merge-conflict wrong pricing)
- `suggest()` fuzzy matcher with `strsim` — "did you mean firecrawl?" on typos
- Shared `Credential` type referenced by both Provider and McpServer
  (de-dupes perplexity/brave/stripe credential data)
- `Category` as enum, not `&'static str` (type safety)

**Security (7 shadow zones GREEN, non-negotiable)**:
- Gate 1: `nika serve` input trust (P0)
- Gate 2: cross-provider structured output parity (P0)
- Gate 3: binding/template hardening
- Gate 4: L1 taint runtime
- Gate 5: `for_each` per-element spotlight
- Gate 6: `NikaError` Display parity
- Gate 7: provider parity matrix

### Runtime internals — itemized

**5 verbs** (each a crate under `nika-verb-*`):
- `exec` — subprocess + `kill_on_drop(true)` + `tokio::try_join!` for concurrent pipe reading
- `fetch` — HTTP + SSRF guard + 9 extract modes (markdown/article/text/selector/metadata/links/jsonpath/feed/llm_txt) + `response: {full, binary}`
- `invoke` — builtins (`nika:*`) + MCP tools (`server::tool`) with shared dispatch
- `infer` — LLM call with structured output 5-layer defense (tool injection → rig extractor → validation → retry → repair)
- `agent` — multi-turn loop with `completion` modes (explicit/natural/pattern) + 4 guardrail types (length/schema/regex/llm-judge) + `token_budget` + `max_cost_usd`

**63 builtin tools** split by domain (lives in `nika-builtin` L2, with native API adapters split into bundle crates `nika-builtin-{github,cloud,workspace}` for heavy deps isolation):
- Core (7): `sleep`, `log`, `emit`, `assert`, `prompt`, `run`, `complete`
- File (5): `read`, `write`, `edit`, `glob`, `grep`
- Introspection (6): `dag_info`, `task_status`, `threads`, `orchestrate`, `cost`, `records`
- Data (19): `json_merge`, `json_diff`, `set_diff`, `zip`, `map`, `filter`, `group_by`, `chunk`, `token_count`, `enrich`, `jq`, `tree_data`, `inject`, `json_verify`, `yaml_validate`, `locale_lookup`, `aggregate`, `json_flatten`, `json_unflatten`
- Media always-on (5): `import`, `decode`, `dimensions`, `thumbhash`, `dominant_color`
- Media core (3): `thumbnail`, `convert`, `strip`
- Media opt-in (18): `metadata`, `optimize`, `svg_render`, `chart`, `phash`, `compare`, `pdf_extract`, `provenance`, `verify`, `qr_validate`, `quality`, `html_to_md`, `css_select`, `extract_metadata`, `extract_links`, `readability`, `pipeline`

**65 pipe transforms** (in `nika-binding`): string (9), array (9), aggregation (7), numeric (5), type (5), logic (1), introspection (1), parametric (8), query (9), string-test (3), URL (4), encoding (4), JQ (1 — `jq(expr)` full stdlib), system (1 — `shell` escape).

**Schema AST (`nika-schema` L0 ~13k LOC)**:
- Raw AST → analyzed AST with `Spanned<T>` source tracking for `ariadne` diagnostics
- Parser: YAML → workflow (schema version pinned `nika/workflow@0.12`)
- Analyzer: DAG construction + cycle detection + unreachable task detection + run-depth enforcement (workflow recursion guard)
- Validator: type coercion on 65 templatable fields (string → f64/u32/bool/provider enum/etc.)
- Taint propagation: `Trust::Untrusted` bits flow through `with:` bindings, enforced at verb boundaries
- Guardrail schema compilation (agent verb)
- Include partials (workflow composition via `include:` with prefix namespacing)
- `when:` conditionals (template expression evaluation)
- `depends_on` explicit ordering
- `for_each` concurrency + `fail_fast` semantics

**Binding / templating (`nika-binding` L0 ~13k LOC)**:
- Single-pass template resolver (no re-parse storms)
- 65 pipe transforms + `jq(expr)` subset
- Null-safety: 19 transforms fail on null; `default(x)` recovery
- `{{with.alias}}` (always required prefix — no bare `{{x}}`)
- `$binding_ref` in `for_each` (templates rejected per BUG-003)
- `$env.VAR` for environment access
- `| shell` transform REQUIRED on all bindings in `shell: true` exec (SEC-2)
- Skill injection into all `infer:` system prompts (not just `agent:`)

**Shield engine (`nika-shield` L1 ~5k LOC)**:
- Spotlight wrapping on untrusted data (NIKA-384)
- Canary token emission + outbound scanner (NIKA-382, NIKA-388 also in thinking trace)
- ML injection detector (NIKA-383, BERT-based — lands v0.95+, heuristic v0.90)
- Capability-denied on dangerous tool + untrusted data combo (NIKA-380)
- Trust violation strict mode (NIKA-381)
- Run depth / cycle detection for workflow recursion (NIKA-386/387)
- Untrusted vision images blocked (NIKA-389)

**Event log (`nika-event` L1)**:
- `EventKind` enum with exhaustive emission sites (invariant #24: exactly 1 emit per verb path)
- `EventLog` + `TraceWriter` for NDJSON output
- `nika-macros` inlined (`builtin_tool!`, `event_task_id!`, `nika_error_code!`)
- Subscription API for `nika serve` SSE + TUI live mode

**Display / renderer (`nika-display` L2 ~14k LOC)**:
- `Renderer` trait with modes: `tty` (spinners, icons, colors), `json` (machine), `ndjson` (streaming)
- Live DAG render + per-task status
- Benchmark display (TTFT, tok/s, per-provider comparison)
- CLI format (tables, progress bars)
- Feeds the TUI (when rebuilt, post-v0.90)

**`nika serve` (HTTP API, `nika-serve` L4)**:
- Routes: `POST /jobs`, `GET /jobs/:id`, `GET /jobs/:id/stream` (SSE), `DELETE /jobs/:id`
- Auth: bearer token + per-job scoping (env `NIKA_JOB_ID`/`NIKA_JOB_DIR`)
- Rate limiting per token
- Webhook delivery for job completion
- OpenAPI spec generation
- Worker pool with cancellation propagation

**`nika-cli` subcommands (L4 ~20k LOC)** — 40+ commands, keep/drop decision per:
- Run: `run`, `check`, `test`, `test --golden`, `eval`, `dry-run`, `explain`
- Lint: `lint` (rule IDs `L001/L010/L020/L030/L031/L050/L060/L070/L080/L090` — SEC-prefixed subset)
- Scaffolding: `new`, `init`, `init --from <git-url>`, `showcase`, `demo`
- Provider: `provider list`, `model list`, `switch`, `bench` (TTFT, tok/s, quality)
- MCP: `mcp list`, `mcp install`, `mcp run`
- Keys: `keys` subsystem (10 subcommands) — see `project_nika_keys_design.md`
- Daemon/Serve: `serve`, `daemon start|stop|status`
- pck: `pck add`, `pck update`, `pck publish`, `pck search`
- Diag: `doctor`, `trace`, `records`, `discover`, `inputs`, `tools`, `verbs`, `schema`
- TUI: `ui` (deferred — rebuilt Act 3 or never)
- Housekeeping: `clean`, `rules`, `onboarding`, `machine install`

**Event subsystem, artifact modes, output modes, templatable fields**:
- Artifact modes: `overwrite | append | unique | fail`, with optional `manifest: true` → `artifacts.json` index
- Output modes: `text | json | yaml | markdown | binary`
- 65 typed fields accept `{{...}}` templates (type-coerced after resolution, NIKA-041 on error)
- Endpoints config in `nika.toml` `[endpoints.<name>]` for self-hosted LLMs + `model: <endpoint>/<name>` slash syntax
- `nika:decode` builtin: base64 → CAS blob (for APIs returning inline images)
- `nika:import` builtin: file → CAS blob

**Per-provider features**:
- Anthropic: `extended_thinking: true` + `thinking_budget: N` tokens (exposed as `InferRequest` field)
- OpenAI: `reasoning_effort: low|medium|high` (o1, o3, o4 families)
- Gemini: JSON mode (`response_format: json`)
- All providers: streaming (MUST, even mock)

**MCP pool (`nika-mcp` L2 ~9k LOC)**:
- `rmcp_adapter` for official SDK compat
- Pool with retry policy per server
- Remote (streamable-http, sse) + local (stdio) transport
- 102 curated aliases + `server::tool` namespace
- Parameter validation (NIKA-107) + connection error (NIKA-100) + start error (NIKA-101)

**Multimodal / vision**:
- `content:` array alternative to `prompt:` (image + text parts)
- `source: "<CAS hash>"` — ONLY CAS hashes accepted (never file paths)
- Per-provider support: anthropic, openai, mistral, groq, gemini, xai
- Text-only: `native` (GGUF), `deepseek` (VisionNotSupported error)

### Cargo features + distribution

**Feature flags strategy** — minimal binary by default, opt-in for heavy deps:
- `nika-provider-native` — `[feature] ggufs` (gates mistral.rs, ~50MB linked)
- `nika-media-pdf` — `[feature] pdf` (gates pdfium + pdf-extract)
- `nika-media-document` — `[feature] html-md` (gates dom_smoothie, readability)
- `nika-media-provenance` — `[feature] c2pa` (gates c2pa full stack)
- `unstable-*` prefix for API surfaces not yet locked (per forward-compat-invariants)

**Distribution channels**:
- **Homebrew** — `brew tap supernovae-st/nika && brew install nika` (formula at `supernovae-st/homebrew-nika`)
- **`curl | sh`** — `curl -LsSf https://nika.sh/install.sh | sh` (platform detection, fallback to GH release tarball)
- **crates.io** — `cargo install nika` once v0.90 tagged
- **GitHub Releases** — pre-built binaries via cargo-dist (macOS arm64/x86_64, Linux x86_64)

### Site + docs + design

**nika.sh** (Astro 5 static, Scaleway par-fr + Cloudflare CDN):
- `/` landing + hero demo + manifesto link
- `/method` — the craft manifesto (public)
- `/changelog` — per-admission entries, RSS feed
- `/blog` — monthly deep dives
- `/errors/[NIKA-XXX]` — per-code lookup pages (prerendered)
- `/architecture` — 3D diamond view (R3F, P1 post-launch)
- `/install.sh` + `/schema/workflow.json` (static assets for CLI consumers)

**docs.nika.sh** (Mintlify monorepo mode, deploy from `nika:docs/mintlify/`):
- Getting started, concepts, guides, reference, examples
- 23 initial MDX files

**`nika-design-skill`** (Claude Code Skill, v0.1.0 AGPL-3.0):
- Consumed by nika.sh via git submodule
- Design tokens (colors, typography, spacing, motion)
- Voice guide (per `NIKA_NARRATIVE_LOCKED`)
- Anti-slop firewall (forbidden patterns)
- Canonical repo #6

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

## v0.95 — "Memory Complete" (ships after v0.90, no date)

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

## v0.100 — "Ecosystem" (ships after v0.95, no date)

- **WASM community plugins** (wasmtime + extism sandbox) — third-party native
  builtins via `nika pck install builtin:...` with signed WASM blobs
- **Full observability**: OpenTelemetry, `tracing` spans, trace replay, budget
  enforcement, metrics (tokio-metrics integration)
- **LSP full**: autocomplete, hover docs, go-to-definition, inline diagnostics
- **Scheduled triggers**: cron-like workflow scheduling
- **Keys subsystem**: macOS Keychain + Linux keyring + OAuth device flow
- **Public launch**: `nika.sh`, community registry promotion, blog launch

## v0.110+ — Ongoing

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

## What we deliberately dropped (from legacy or never built)

| Item | Decision | Why |
|---|---|---|
| `nika-sdk` (remote client SDK) | DELETED | 0 consumers, speculative abstraction |
| `nika-napi` (Node.js bindings) | DELETED | W1 removed, rebuild as `@nika/client` TypeScript if needed |
| `nika-py` (Python bindings) | DELETED | W1 removed, no clear user pull |
| `nika-tui` (terminal UI) | DELETED → rebuilt Act 3 or never | W1 removed; deferred until after v0.90 |
| `ProviderCategory` enum | DELETED | 11 ex-MCP providers migrated to `McpAlias` catalog |
| Agent-v2 (multi-turn, 4 guardrails) | DEFERRED v0.95 | Architecture gate-review: need Cortex memory first |
| Cortex (memory satellites × 9-10) | DEFERRED v0.95 | 20-24 crate deficit, need runtime-complete first |
| WASM host (wasmtime + extism) | DEFERRED v0.100 | Security unsolved, 10x perf loss, 200k LOC dep |
| Keys subsystem (Keychain/OAuth) | DEFERRED v0.100 | Not on critical path for v0.90 self-host use |
| Hosted cloud runner | DEFERRED v0.110+ | Self-host is primary; SaaS optional optional later |
| Commercial relicense (AGPL → commercial) | DEFERRED v0.110+ | Only when enterprise pull warrants legal setup |

## What this roadmap deliberately excludes (never)

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
