# Changelog

All notable changes to Nika are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Nika follows [forever-v0.x](ROADMAP.md) — incremental quality, no v1.0 target.

Nika Diamond is a ground-up rewrite on an orphan branch (`nika-diamond`).
Legacy main sits at v0.79.3. Diamond starts at v0.80.0.

---

## [Unreleased]

### 📚 Wave 4E — Mintlify rebuild + docs repo split (2026-04-17)

End-user documentation split out to a dedicated public repository and
rebuilt from the current workspace state.

- **`supernovae-st/nika-docs`** — new public repo, serves
  [`docs.nika.sh`](https://docs.nika.sh) via Mintlify. Replaces the
  in-engine `docs/mintlify/` directory, which is removed from this
  repo. Engine-internal docs (`docs/adr/`, `docs/architecture/`,
  `docs/crate-specs/`) stay here.
- **Mintlify content refreshed** — 2-tab navigation (Guide / Reference),
  honest v0.80 pre-release framing, live snapshot of 32 providers, 49
  capability rules, 35 ADRs (11 thematic groups), L0 architecture
  decisions, admission 12-gate walkthrough.
- **Dead pages purged** — 8 Mintlify pages that no longer mapped to the
  Diamond workspace state removed pre-split.
- Cross-links from this repo's README + ROADMAP point to
  `docs.nika.sh` for end-user content.

### ⚡ Swarm-3 Batches I.b + II ε.2/ε.3 + Wave 3A + Wave 4A + 4B seeds + Wave 4C (2026-04-17)

**Hygiene — Batch I.b vectors 30-33 (+4 new):**

- **Vector 30 `check-cancel-safety.sh`** — every `async fn` in
  `crates/nika-kernel/src/**` now carries a `// CANCEL SAFETY:` or
  `/// CANCEL SAFETY:` marker. 43 kernel methods annotated
  (cancel-safe contract: drop semantics, atomic vs non-atomic writes,
  `kill_on_drop` requirement, billing/telemetry exposure).
- **Vector 31 `check-owned-strings.sh`** — preventive ratchet: bans
  non-static `&str` in nika-catalog `pub` fields / `pub fn` return
  types. Catalog stays 100% `&'static str` per ADR-008 codegen pragma.
- **Vector 32 `check-unsafe-count.sh`** — `unsafe` token counter
  vs `scripts/hygiene/baselines/unsafe-count.txt` (currently 0).
  Substitutes cargo-geiger which is hostile to virtual manifests.
- **Vector 33 `check-layer-deps.sh`** — per-layer banned third-party
  deps (`[workspace.metadata.diamond] layer-bans`). L0 rejects 17
  deps (tokio family, rayon, async-std, smol, futures family,
  reqwest, hyper, axum, actix-web); L0.5 rejects 11.
- **Killed vector 7** (linear-issue-states stub) **and vector 18**
  (adr-dangling duplicate of vector 16).

**Wave 3A — engine post-commit hook for Olympus snapshots:**

- `scripts/hooks/post-commit-olympus-xtask.sh` wired via lefthook.
  Background `pnpm tsx olympus/scripts/xtask.ts` regenerates
  workspace.json + snapshots + hygiene-status.json on every engine
  commit; Olympus live-refreshes `/timeline`, `/graph/diff`,
  `/graph/fitness`, `/hygiene`.

**Wave 4A — v0.95 Cortex + v0.100 WASM reservations (R1-R5):**

- **R1 `EmbeddingSpec`** (`nika-types::embedding`) — Dtype,
  DistanceMetric, EmbeddingSpec; `#[non_exhaustive]` + snake_case wire.
- **R2 `MemoryFrameRef.trust: TrustLevel`** — sticky ingest taint;
  `#[serde(default)]` → UNTRUSTED fail-safe.
- **R3 `RecallQuery.tenant: TenantId`** — mandatory multi-tenant
  keyspace scope. `TenantId::default_tenant()` → `"default"`.
- **R4 `WasmPluginError::OutOfFuel` + `Trap { kind: TrapKind }` +
  `PluginCallContext`** — fuel metering, W3C-style trap taxonomy,
  per-call context with trust + cancel + budget.
- **R5 `MemoryLifecycle` trait** with default-impl consolidate/prune
  returning empty reports. Standalone; Cortex opts in at v0.95.

**Wave 4B seeds (telemetry foundations):**

- **#1 `SpanGuard.parent_span_id` + `links: Vec<SpanRef>`** — W3C
  Trace Context parent linkage unblocks Olympus `/trace`. Default
  `TracerProvider::start_child_span` backfills parent.
- **#3 `Timestamp(i64 unix_ns)` + `WallDuration(i64 nanos)`** in
  `nika-types::timestamp`. RFC 3339 Display via inlined Hinnant
  civil-from-days algorithm. Serde-transparent wire. Field retrofit
  (`_ms: u64` → `timestamp`) deferred.

**Batch II — test depth:**

- **ε.2 Loom** — `#[cfg(loom)]` interleaving tests for `CancelCtx`
  (INV-029). Conditional `[target.'cfg(loom)'.dependencies]`.
  Run explicitly via `RUSTFLAGS="--cfg loom" cargo test`.
- **ε.3 proptest audit** — 14 new properties: TrustLevel lattice
  invariants (meet/join bounds, idempotence, commutativity,
  associativity, absorption); ID serde roundtrip (TenantId,
  ProviderId, ModelId, TaskId, TraceId full 2^128 surface, SpanId
  full 2^64 surface).
- **ε.1 mutation baseline** — `cargo mutants -p nika-error` run:
  60 mutants, 31 caught, 13 missed (mostly miette::Diagnostic
  accessor returns — no observable behaviour), 16 unviable.
  Viable kill rate 70.5%. Pushing to ≥90% requires dedicated
  miette diagnostic-method assertion tests; deferred to a focused
  follow-up session.

**Batch V.2** — `docs/architecture/axes.md`: 12-axis × crate ISP
matrix with shipped/reserved/not-yet markers. Source of truth for
Olympus `/graph/architecture` edge rendering + Gate 12 audits.

**Observability locks (parallel work already landed):**

- Q12 — `ObservabilitySink` dropped (5→4 effect channels);
  `AuditSink` added as compliance-grade 5th channel.
- Q13 — `GenAiAttrs` OTel semconv bridge on Infer{Request,Response}.

**CI ratchets:**

- `cargo-public-api` snapshot workflow (Gate 12 mechanical).
- `cargo-semver-checks` workflow.
- Public-api baseline files regenerated on every reservation commit
  (`--all-features --omit auto-trait-impls` to match CI invocation).

**Forward-compat seams:**

- nika-types `no_std`/`alloc` seam at module level (F1 complete;
  shipped 2026-04-17 morning).
- F2 (full per-module cfg-gating) deferred — requires uuid dep
  re-architecture (currently in `serde` feature but used in
  non-serde struct fields in RunId/EventId/CorrelationId/MemoryId).
  Re-open trigger: uuid becomes unconditional OR UUID-backed IDs
  move to a dedicated feature separate from serde.

**Numbers at close:**

| field              | value                                      |
|--------------------|--------------------------------------------|
| HEAD               | (updated at commit time)                   |
| lib tests          | 905 (+58 this session)                     |
| integration tests  | 10                                         |
| loom tests         | 2 (cfg-gated)                              |
| clippy             | 0 warnings                                 |
| hygiene vectors    | 31 deployed (27 green / 4 yellow)          |
| crates admitted    | 6 + 1 WIP (unchanged)                      |
| ADRs               | 25+ (seeds ADR-029-032 + 035 authored)     |

### ⚡ Phase D Session 4B — Data enrichment (2026-04-16)

Pure data expansion on the structural foundation laid by Session 4A.
Zero trait/struct changes — only enum variants, TOML data, and tests.

- **6 new `ParamFlag` variants** — `BatchApi`, `ContextCaching`,
  `PredictedOutputs`, `ComputerUse`, `Citations`, `IncludeReasoning`.
  Aligned with `OpenRouter` 25-value `supported_parameters` vocabulary.
  Enum: 7→13 variants.
- **3 new `Modality` variants** — `Embedding` (vector output), `Speech`
  (TTS/ASR), `ImageGen` (text-to-image). Covers non-LLM provider
  capabilities. Enum: 5→8 variants.
- **4 new `TokenizerFamily` variants** — `LlamaV4` (~200k vocab, distinct
  from LlamaV3), `Granite` (IBM `StarCoder` BPE), `Glm` (Zhipu
  `SentencePiece`), `Grok` (xAI custom). Enum: 8→12 variants.
- **7 new providers** — nvidia-nim (FIX: inventory discrepancy),
  deepinfra, replicate, hyperbolic, writer, databricks, cloudflare.
  All `openai-chat` dialect. Count: 25→32.
- **7 new capability rules** — one `Matcher::Any` fallback per new
  provider (text-only, `json_schema` where applicable). Count: 42→49.
- `mock-full` rule updated with all 13 `ParamFlag` variants.
- Cross-catalog overlap allowlist: replicate + cloudflare (dual-role).

### ⚡ Phase D Session 4A — Catalog structural enrichment (2026-04-16)

Context-window + output-limit + JSON mode enrichment. First structural
expansion of capabilities beyond the Session 2a/2b foundation.

- **3 new CapPatch fields** — `context_window_tokens: Option<u32>`,
  `max_output_tokens: Option<u32>`, `json_mode: Option<JsonMode>`.
  Per-model context windows and output limits are now expressible in the
  TOML-driven capability resolver.
- **`JsonMode` enum** — `Schema` (tool_use enforcement) / `Object`
  (unstructured json_object mode). Per-provider granularity.
- **`ContainsAny` matcher** — word-boundary-anchored substring matching
  with left/right boundary chars (`-`, `_`, `/`, `.`, `@`). Prevents
  "sonnet-4" from matching "sonnet-4-60" (the `6` after "sonnet-4" is
  not a boundary character).
- **`#[non_exhaustive]` on 20 mock structs** — all `nika-kernel-mock`
  types now enforce invariant #19 (attribute + `pub fn new()`).
- **`HttpStreamResponse::new()`** — invariant #19 compliance for the
  only `#[non_exhaustive]` struct that was missing a constructor.
- **12-field merge_with regression guard** — all CapPatch fields covered
  by a single test with confirmed RED on removal.
- **estimate_cost edge cases** — zero tokens → $0.00, nonexistent model → None.
- **MemoryId deserialize error paths** — missing `mem-` prefix and invalid
  UUID now have dedicated tests.
- Token count: 625 → **630 lib tests** (+5).

### 🛡️ Phase C Wave 3 — Stabilization + review-swarm defense (2026-04-16)

Hardening pass after the foundational-types expansion. Mutation testing,
proptest campaigns, and a 3-agent review swarm closed all P0/P1 findings.

- **Seal `SecretResolver`** — `cargo-expand` verified private supertrait;
  community can't implement, allowing future method additions (P1-1).
- **`CancelCtx` Acquire/Release** — correctness fix for v0.95 DAG cancel
  semantics (P1-6). Drop guard prevents leaked tokens.
- **Reserve NIKA-700..819** + `Category::Memory` / `WasmPlugin` / `Sandbox`
  / `Observability` — error-code real estate for v0.95+ subsystems.
- **Cost stdlib arithmetic** — `Add`/`Sub`/`AddAssign`/`SubAssign` with
  panic-in-debug, wrap-in-release semantics. `checked_add` / `checked_sub`
  for fallible callers.
- **Remove `TrustLevel::Default`** — safe-by-default inversion (P1-2).
  All trust must be explicitly stated.
- **`InferResponse.cost: Option<Cost>`** — structured cost replaces the
  deprecated `cost_usd` float. Provider-side cost tracking now type-safe.
- **Structured `DenialKind`** — replaces `CapabilityDenied { reason: String }`
  with enum variants (`FsReadNotGranted`, `FsWriteNotGranted`, `NetEgressBlocked`,
  `ExecBlocked`, `EnvReadBlocked`, `Custom`).
- **20 proptest lattice/identity laws** — cost commutativity, associativity,
  identity; trust lattice meet/join; baggage merge idempotence (integration tests).
- **MemoryId UUIDv7** — `MemoryId(u128)` → `MemoryId { uuid: Uuid }`.
  Time-sortable, standard format, `Display`/`FromStr` roundtrip.
- **`#[deprecated]` cost_usd** on `InferResponse`, `AgentOutcome`,
  `AgentCheckpoint` + `Cost::to_usd_f64()` bridge for deprecation window.
- **Pin zeroize=1.8** — workspace-wide version lock for `SecretString`.
- **cargo-mutants 88.5% kill rate** on nika-error L0 (cost/trust/baggage).
- Token count: 572 → **585 lib / 621 total** (+13 lib, +49 total).

### ⚡ Phase C Wave 2 — L0 foundational types + L0.5 traits (2026-04-16)

23 pure-data types landed in L0 crates, 6 kernel traits in L0.5, plus
forward-compat seams for v0.95 Cortex and v0.100 WASM.

- **23 L0 value types** across nika-error and nika-kernel — cost, budget,
  trust, retry, schema versioning, baggage, resource URI, content hash,
  memory frame, deny kind, cancel context, plugin DTOs, sandbox policy,
  observability event.
- **6 L0.5 kernel traits** — `IdGenerator`, `SecretResolver`, `MetricsExporter`,
  `TracerProvider`, `EventSink`, `BillingSink`. Sealed: `SecretResolver`,
  `EventSink`, `BillingSink`. Open: `IdGenerator`, `MetricsExporter`,
  `TracerProvider`. All have mock implementations in nika-kernel-mock.
- **Sealing pattern** — `Provider`, `EventSink`, `BillingSink`,
  `SecretResolver` now sealed via `mod sealed { pub trait Sealed {} }`.
  Open traits (`MemoryStore`, `EmbeddingProvider`, `ToolExecutor`) remain
  community-implementable.
- **Forward-compat seams** — `cancel.rs`, `plugin.rs`, `sandbox.rs`,
  `observability.rs` in nika-kernel. `MemoryFrame` gains reserved
  `Option<_>` fields (`cipher`, `provenance`, `retention`, `redactions`).
- **ADRs 016-020** — cancellation, streaming, runtime, retry, WASM
  (Batch F part 1). **ADRs 033-034** — L0/L0.5 expansion plans.
- Token count: 416 → **572** (+156 tests).

### ⚡ Phase D Session 2a — TOML-driven model capabilities (2026-04-14)

Zero-allocation capability resolver migrated from hardcoded Rust to a
TOML-driven rule table. Zero-alloc, proptest-verified, forward-compatible.

- **`data/model-capabilities.toml`** — 9 ordered rules covering OpenAI o-series,
  GPT-5, Claude family, Anthropic catch-all, DeepSeek reasoner, DeepSeek any,
  and xAI Grok-4. Schema `nika/model-capabilities@1.0`. First-match-wins
  semantics with build-time FK checks (providers must exist in
  `llm-providers.toml`, api_dialect must be in the closed dialect set).
- **`src/types/capabilities.rs`** — `CapPatch` (5 `Option<T>` fields,
  `const fn merge_with`, `fn materialize`), `Matcher` (Any/Exact/ExactAny/PrefixAny,
  zero-alloc `eq_ignore_ascii_case`), `Rule` (providers + api_dialect scope + matcher + caps).
- **`build/capabilities.rs`** — extracted from `build.rs` (380 LOC) to stay under
  the 1500-LOC-per-file budget. Validates TOML schema, FK checks, closed-set
  enum validation, all-None rule prevention, emits static Rust arrays at compile time.
- **`api_dialect`** — `Option<&'static str>` added to all 21 providers in
  `llm-providers.toml`. Closed set: anthropic / openai-chat / openai-responses /
  gemini / cohere / ai21 / bedrock / voyage / mock. Reserved for Session 2b+
  dialect-scoped rule authoring.
- **`supports_thinking` → `reasoning` rename** — aligns with 2026 industry
  convention (LiteLLM `supports_reasoning`, models.dev `reasoning`, OpenRouter
  `reasoning`). No compat shim (forever-v0.x nuke-and-rebuild).
- **`TokenLimitParam::MaxOutputTokens`** — variant added (OpenAI Responses API
  future-proofing). No rule maps to it yet; the `#[non_exhaustive]` enum can
  grow without a schema bump.
- **Proptest parity harness** — 10,000 random (provider, model) pairs compared
  against frozen legacy body in `mod parity_tests`. Regex widened to cover slash
  syntax, uppercase, underscore (HF-style), long names.
- **Insta snapshot** — 31 golden (provider, model) pairs reviewable under
  `src/data/snapshots/`.
- **Invariant #19 FULL** — 15 `new()` constructors across the crate (every
  `#[non_exhaustive]` public struct). Includes: `ProviderModel`, `Provider`,
  `ProviderModel`, `McpServer`, `Embedding`, `TransformDef`, `Builtin`,
  `EnvVarSpec`, `McpPackage`, `McpRemote`, `ModelCapabilities`, `ModelPricing`,
  `CostEstimate`, `ParseTagError`, `ParseCategoryError`, `Suggestion`.
- **Gate 8 GREEN** — `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` clean.
  8+ broken intra-doc links fixed across the crate.
- **5-agent review** — rust-architect + rust-pro + rust-perf + spn-nika +
  feature-dev:code-reviewer. All P0/P1 findings addressed in same session
  across 2 hardening commits.

### 🏷️ Phase D Session 1 — Tag vocabulary + Cargo features (2026-04-14)

Typed tag system for catalog entries, Cargo feature gating, and Shield
safety invariant enforcement.

- **42-variant `Tag` enum** (`#[non_exhaustive]`) — model I/O modalities,
  reasoning/generation behaviour, economics, deployment/sovereignty,
  specialisation, domain, and MCP-server permissioning. Kebab-case wire
  format (`Tag::as_str()` + `FromStr`). Locked as enum (not `&str`) so
  pck authors get compile errors on typos.
- **`tags` + `extra_tags` fields** on `Provider`, `McpServer`, `Embedding` —
  `&'static [Tag]` (validated at build time) + `&'static [&'static str]`
  (passthrough escape hatch for community-specific vocabulary).
- **All 139 catalog entries tagged** (21 providers + 13 embeddings + 105 MCP
  servers). build.rs enforces: known tags only, sorted, deduplicated, and
  MCP entries MUST carry exactly one of `read-only` / `destructive` (Shield
  security-filter invariant, compile-time enforced).
- **Cargo features for subset compilation** — `full` (default), `minimal`,
  `mcp`, `providers`, `embeddings`, `pricing`, `capabilities`,
  `builtins-transforms`, `extension-author`. Community crates depend on
  `features = ["extension-author"]` for types-only (no bundled data).
- **7 runtime tag invariant tests** — XOR, Budget/Frontier mutex,
  Embedding/Reranker presence, sort/dedup codegen integrity, spot-checks
  (anthropic tags, stripe MCP tags).
- **COMMUNITY_EXTENSIONS.md** — pck-author pattern documentation for
  `nika-catalog-cn`, `nika-catalog-eu`, etc.
- **3-agent review** (spn-nika + feature-dev + rust-pro) — all P0/P1
  findings addressed: `f64::INFINITY` validation gap, `#[allow(dead_code)]`
  scoping, `tag_variant` drift guard, `Tag::Sandbox` doc clarification,
  `extra_tags` Gate 1 SAFETY note, version pin fix.

### ⚙️ Hygiene + automation (2026-04-14 PM)

Autonomous ecosystem hygiene stack added to prevent drift over the 11-12 month build:

- **15-vector hygiene dashboard** (`scripts/hygiene/check-all.sh`) — MEMORY HEAD,
  crate count, LOC, CHANGELOG, ROADMAP, crate specs, Linear, GitHub milestones,
  org profile, CITATION, unwraps, file LOC cap, Claude coauthor leak, private
  path leak, cargo audit. Green/yellow/red table, exit codes 0/1/2.
- **Claude Code hooks** — PreToolUse blocks 5 dangerous ops (force push,
  `git add -A`, `cargo test --test`, checkout main, `--no-verify`); PostToolUse
  inspects HEAD commit for Claude coauthor + auto-runs hygiene on admissions;
  SessionStart injects grep-verified HEAD + crate count + hygiene state.
- **Skills** — `/gate-check` and `/crate-admit` for 12-gate discipline;
  `review-swarm.md` subagent for parallel 3-agent review (Gate 11).
- **CI workflows** — `hygiene-nightly.yml` (cron 3h UTC, idempotent drift issue),
  `forward-compat.yml` (cargo-public-api + cargo-semver-checks on PR),
  `changelog-cliff.yml` (auto-PR prepend CHANGELOG on tag push).
- **git-cliff config** (`cliff.toml`) — groups match content pipeline.

## [0.80.0-alpha.4] - 2026-04-14

### 🆕 Crate admitted: nika-catalog-verify

The immune system.

Where `nika-catalog` answers "what do we know?" in O(1) from compile-time data,
`nika-catalog-verify` answers "is what we know still true?" It probes real
package registries (npm, PyPI, Docker) and remote MCP endpoints in parallel,
producing a JSON drift report. Binary, not library — runs nightly from CI or
on-demand via `cargo run -p nika-catalog-verify`.

This is the second catalog crate and the first L4 binary admitted. It exists
because static catalogs decay: a package gets deprecated, an API endpoint goes
away, a provider renames a model. Without verify, the catalog silently rots.

Exempted from Gate 5 (mutation ≥90%) because binary I/O code produces
tautological mutations. Gate 10 (legacy parity) is N/A — this is new tooling.

| Metric | Value |
|--------|-------|
| LOC | ~600 |
| Tests | partial (logic only, I/O excluded) |
| Clippy warnings | 0 |
| Unwraps in src/ | 0 |

Commit `a977e35b1`. 🦋

---

## [Previously Unreleased] — moved to 0.80.0-alpha.4

### 🔨 Refactors

- **nika-catalog Phase C migration** — migrating catalog data from hardcoded
  Rust arrays to `data/*.toml` source files, compiled at build time via
  `build.rs` + `phf_codegen`. Same zero-runtime-overhead phf maps, but the
  source of truth is now human-readable TOML. This unblocks community
  contributions to the catalog (PR a TOML file, not a Rust array).

### 🐛 Fixes

- **nika-catalog Phase A cleanup** (db0bf8e3f) — a 5-agent deep audit
  discovered 29 of our 131 MCP aliases were broken. Some pointed to
  Anthropic reference servers that were quietly deprecated ("Package no
  longer supported" on npm). Others referenced npm packages that never
  existed — Python-only tools, Go binaries, or names we'd fabricated from
  incomplete documentation. Three were community forks with zero weekly
  downloads.

  We removed all 29 and added a regression test (`removed_broken_aliases_not_present`)
  so they can't sneak back. The catalog went from 131 to 102 aliases.
  Every remaining alias now resolves to a real, installable package.

---

## [0.80.0-alpha.3] - 2026-04-13

### 🆕 Crates admitted: nika-kernel + nika-kernel-mock

The nervous system.

`nika-kernel` defines the **trait contracts for every side effect** in Nika.
It sits at L0.5 — above the pure types (error, catalog) and below the
implementations (fs, http, process, provider). Zero implementations live here.
This crate is the constitution: it says what each organ *must* do, not how.

The design follows Interface Segregation Principle to the max: ~20 fine-grained
atomic traits (`FsRead`, `FsWrite`, `HttpGet`, `ShellRun`...) grouped into ~6
super-traits of convenience (`Fs`, `HttpClient`, `ShellExecutor`, `Provider`...).
Consumers depend on exactly the surface they need — a context loader imports
`FsRead` alone, not the entire filesystem umbrella.

All async traits use `trait_variant` (Rust 1.91 native AFIT) instead of
`async_trait`. Zero boxing on the static dispatch path. The kernel carries no
tokio dependency — pure trait definitions that any async runtime can implement.

We also planted the **Cortex + agent-v2 hooks** now: `MemoryStore`,
`EmbeddingProvider`, `ToolExecutor`, `ContextCompressor`, and agent checkpoint
types. These won't be implemented until v0.95, but defining them in Phase 1
means we won't need breaking changes to `#[non_exhaustive]` structs later.
Forward compatibility bought cheaply.

`nika-kernel-mock` is the companion: deterministic mocks for every kernel trait
(`MockClock`, `InMemoryFs`, `MockHttp`, `MockShell`, `MockProvider`...).
Test hermeticity from day one — no test in Nika will ever touch a real
filesystem, a real network, or a real LLM provider.

| Metric | nika-kernel | nika-kernel-mock |
|--------|-------------|------------------|
| LOC | 3,369 | 1,731 |
| Tests | 99 | 88 |
| Mutation killed | 100% | 95.7% |
| Clippy warnings | 0 | 0 |
| Unwraps in src/ | 0 | 0 |

### Key decisions

- **Clock is SYNC, everything else ASYNC** — YAGNI on network time. Hot paths
  stay simple.
- **`BTreeMap` over `HashMap`** — deterministic iteration order, no hasher
  dependency. Tests are reproducible.
- **Cancel as `fn` param, not in struct** — keeps `ShellCommand` free of
  tokio-util. The kernel stays runtime-agnostic.
- **Provider = Infer + Stream + Meta** — all providers MUST stream (even mock).
  Embed and Vision are opt-in traits.
- **Errors per subsystem** — `ProviderError`, `ShellError`, `ToolExecError`,
  `MemoryError`. No god-enum.

All 12 gates passed. Commit `ef8804371`. 🦋

---

## [0.80.0-alpha.2] - 2026-04-13

### 🆕 Crate admitted: nika-catalog

The memory.

`nika-catalog` is Nika's static knowledge of the world: every LLM provider it
can talk to, every MCP server it knows how to install, every builtin tool it
ships, every pipe transform it supports, and the pricing of every model it's
seen.

The catalog is compiled into the binary at build time. No runtime I/O, no
config files, no network calls. You ask "do you know `anthropic`?" and the
answer comes back in O(1) via a [perfect hash function](https://en.wikipedia.org/wiki/Perfect_hash_function).

Why this matters: when a user writes `provider: claude` in their YAML, the
engine resolves the alias → canonical provider → model → capabilities → pricing
in a chain of zero-allocation lookups. No guessing, no fuzzy matching, no
"did you mean?" The catalog is the ground truth.

The lookup strategy is hybrid by design:
- **phf + unicase** for case-insensitive lookups (providers, MCP aliases) —
  because users write `Claude`, `claude`, `CLAUDE` and they all mean Anthropic.
- **Sorted arrays + binary_search** for case-sensitive lookups (builtins,
  transforms) — because `nika:read` and `nika:Read` are different things
  (actually `nika:Read` doesn't exist, and the catalog should say so clearly).

At admission: 16 providers, 105 MCP aliases, 63 builtins, 65 transforms,
61 model pricing entries. All from a single `cargo build`.

| Metric | Value |
|--------|-------|
| LOC | 2,235 |
| Tests | 85 |
| Mutation killed | 94.7% |
| Clippy warnings | 0 |
| Unwraps in src/ | 0 |

All 12 gates passed. Commit `55a451695`. 🦋

---

## [0.80.0-alpha.1] - 2026-04-13

### 🆕 Crate admitted: nika-error

The DNA.

Every error in Nika carries a code. `NIKA-001` means schema validation failed.
`NIKA-053` means a blocked command was attempted. `NIKA-382` means a canary
token leaked (prompt injection detected). There are hundreds of these codes,
and every single one must roundtrip through Display, parse back from a string,
serialize to JSON, and match the exact same format across every provider, every
verb, every transport layer.

`nika-error` is the crate that makes this possible. It defines:

- **`NikaErrorCode`** — a trait that every per-crate error enum must implement.
  This is the contract: if you want to be a Nika error, you carry a code, a
  severity, a category, and you format yourself as `"NIKA-XXX: message"`.
- **`NikaError`** — a `Box<dyn NikaErrorCode>` wrapper. The unified error type
  that flows through `?` propagation across the entire codebase.
- **`NikaCode`** — the code itself. Dual format: Display gives you `"NIKA-140"`,
  serde gives you `{"num":140,"category":"ast","severity":"error","slug":"ast-analysis-failure"}`.
- **`CoreError`** — cross-cutting errors that don't belong to any specific crate
  (Validation, NotFound, Unsupported, Internal).

This is the L0 anchor. Zero `nika-*` dependencies. Reachable from every crate
in the workspace. The first cell of the organism.

It also resolves **shadow zone 6** from the pre-launch audit: every admitted
`NIKA-XXX` now ships with a Display parity golden test against the legacy
format. No silent drift.

| Metric | Value |
|--------|-------|
| LOC | 1,013 |
| Tests | 44 |
| Mutation killed | 100% |
| Clippy warnings | 0 |
| Unwraps in src/ | 0 |

All 12 gates passed. Commit `42909b1c7`. 🦋

---

## [0.80.0-alpha.0] - 2026-04-13

### The beginning

Orphan branch `nika-diamond` created from scratch. No code inherited from main.
Clean slate, edition 2024, Rust 1.91.

From the start, the workspace enforces:
- `clippy::unwrap_used = "deny"` — zero unwraps, everywhere, always.
- `clippy::panic = "deny"` — if it can panic, it doesn't compile.
- `clippy::expect_used = "warn"` — we'll get there.

32 legacy crate directories excluded via `.gitignore` — they exist on disk
(the orphan branch inherits the working tree) but cargo ignores them. We read
legacy code via `git show main:path/to/file.rs` when we need guidance, but
nothing is copied verbatim. Every line is rewritten.

The organism's skeleton is in place. Now it grows. 🦋

---

[Unreleased]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.4...HEAD
[0.80.0-alpha.4]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.3...v0.80.0-alpha.4
[0.80.0-alpha.3]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.2...v0.80.0-alpha.3
[0.80.0-alpha.2]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.1...v0.80.0-alpha.2
[0.80.0-alpha.1]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.0...v0.80.0-alpha.1
[0.80.0-alpha.0]: https://github.com/supernovae-st/nika/commits/v0.80.0-alpha.0
