# Legacy → Diamond Migration Checklist

> Authoritative map of every feature on legacy brouillon (`830aa6154`) and its
> diamond destination. Zero feature lost.
>
> Last updated: 2026-04-14. Produced by 4-agent legacy audit.

## Migrated (admitted crates, 5 at the time)

| Legacy source | Diamond crate | Status |
|---|---|---|
| `nika-core/src/error.rs` + `error_codes.rs` | `nika-error` | ✅ admitted `42909b1c7` |
| `nika-core/src/catalogs/*` (7 catalogs) | `nika-catalog` | ✅ admitted `55a451695` |
| `crates/catalog-verify` (xtask) | `nika-catalog-verify` | ✅ admitted `a977e35b1` |
| `nika-kernel/src/*` | `nika-kernel` | ✅ admitted `ef8804371` |
| kernel mocks | `nika-kernel-mock` | ✅ admitted `ef8804371` |

## Mapped (spec'd, awaiting admission)

### Phase 1 remaining

| Legacy source | Diamond crate | Phase |
|---|---|---|
| `nika-core/src/ast/*` (15k LOC) | `nika-schema` | 1 |
| `nika-core/src/binding/*` (13k LOC incl. 7243 LOC template mod) | `nika-binding` | 1 |

### Phase 2 (L1 effects — 11 crates)

| Legacy | Diamond | LOC |
|---|---|---|
| `nika-clock` | `nika-clock` | ~1k |
| `nika-fs` | `nika-fs` | ~2k |
| `nika-http` | `nika-http` | ~3k |
| `nika-blob` | `nika-blob` | ~1k |
| `nika-exec-runner` | **`nika-process`** (renamed) | ~2k |
| `nika-event` (no macros — Q1 decision) | `nika-event` (L0 types ~4-5k) + `nika-event-store` (L1 ~3k) + `nika-event-export` (L2 ~2k) | ~9k total |
| `nika-core/extract/*` + new extract logic | `nika-extract` | ~2k |
| `nika-security/*` | `nika-security` | ~3k |
| `nika-engine/runtime/{shield,canary,spotlight,output_scanner}` | **`nika-shield`** (new) | ~5k |
| `gix` wrapper | **`nika-git`** (new for pck) | ~1.5k |
| `nika-vault` | `nika-vault` | ~1.3k |

### Phase 3 (L2 domain — 20 crates)

4 verbs (per D-2026-05-22-N18 · `fetch` is now the `nika:fetch` builtin via
`invoke`, not a verb crate):

| Legacy | Diamond |
|---|---|
| `nika-verb-exec` | `nika-verb-exec` |
| `nika-verb-fetch` | `nika:fetch` builtin (under `invoke` · no longer a verb crate) |
| `nika-verb-invoke` | `nika-verb-invoke` |
| `nika-verb-infer` | `nika-verb-infer` |
| (new, split from engine) | `nika-verb-agent` |

Providers (legacy `nika-engine/provider/*` · landing SHIPPED 2026-06-11,
supersedes the 3-crate split plan):

| Legacy | Diamond |
|---|---|
| `nika-engine/provider/rig/*` | `nika-providers` (L1.5 · 14/14 wire-direct · rig NOT carried · D-2026-05-22-N17) |
| `nika-engine/provider/native/*` | `nika-infer-local` (candle sidecar · ADR-091) |
| `nika-engine/provider/mock/*` | in-crate mock inside `nika-providers` |

5 media crates (split legacy `nika-media` 14k LOC, some deferred v0.95):

| Legacy | Diamond | Phase |
|---|---|---|
| `nika-media/src/cas/*` | `nika-media-cas` | 3 |
| `nika-media/src/image/*` (thumbnail, convert, strip, phash, optimize) | `nika-media-image` | 3 |
| `nika-media/src/pdf/*` | `nika-media-pdf` | **v0.95** |
| `nika-media/src/{svg,chart,readability,html_to_md}` | `nika-media-document` | **v0.95** |
| `nika-media/src/{c2pa,qr}` | `nika-media-provenance` | **v0.95** |

3 builtin bundles (new for native API adapters):

| Diamond | Adapters |
|---|---|
| `nika-builtin-github` | octocrab |
| `nika-builtin-cloud` | aws, cloudflare, vercel, stripe |
| `nika-builtin-workspace` | slack, notion |

Services:

| Legacy | Diamond |
|---|---|
| `nika-builtin/*` (63 tools) | `nika-builtin` + 3 bundles above |
| `nika-mcp/*` (rmcp pool, 102 aliases) | `nika-mcp` |
| `nika-display/*` (13,360 LOC renderer) | `nika-display` |
| `nika-builtin/runtime/tool.rs` (Tool trait) | `nika-tool` |

### Phase 4 (L3/L4/L5 — 12 crates)

| Legacy | Diamond |
|---|---|
| `nika-engine/runtime/*` (policy, cache, executor) | `nika-runtime` (L3, policy+cache as modules per invariants) |
| `nika-daemon/*` + `nika-storage/*` | `nika-daemon` (L3, storage as module) |
| `nika-cli/*` (40+ subcommands) | `nika-cli` (L4) |
| `nika-lsp/*` + `nika-lsp-core/*` merged | `nika-lsp` (L4) |
| `nika-serve/*` | `nika-serve` (L4) |
| `nika-init/*` (+ `init --from`) | `nika-init` (L4) |
| dylint rules (new) | `nika-lints` (L4, may defer post-v0.90) |
| — | `nika-pck-{manifest,registry,store,pck}` (new, pck MVP) |
| binary composition | `nika` (L5, <500 LOC) |

## AT RISK — features requiring spec care (28)

These are real legacy capabilities without a dedicated spec line yet. Each
must be enumerated in its destination crate's spec doc BEFORE the admission PR,
OR explicitly dropped with reason.

### CLI ergonomics (owned by nika-cli)
- `nika doctor` — environment diagnostics
- `nika explain` — workflow reasoning trace
- `nika discover` — skill/provider discovery
- `nika trace` — replay EventLog for a job
- `nika onboarding` — first-run wizard
- `nika machine install` — install to `~/.nika` + shell integration
- `nika inputs` — prompt for workflow inputs interactively
- `nika token` — token counting on an expression
- `nika tools` — list builtins + MCP tools
- `nika verbs` — documentation for the 4 verbs
- `nika switch` — provider/model switching
- `nika clean` — clear cache + temp
- `nika demo` — run bundled showcase workflows
- `nika rules` — validate CLAUDE.md-style rules files
- `nika test --golden <snap>` — compare output to golden file
- `nika eval --dataset <d>` — assertion framework
- `nika bench` — provider benchmarking (TTFT, tok/s, quality)
- `nika schema` — dump JSON Schema for YAML validation
- 10 lint rule IDs: `L001`, `L010`, `L020`, `L030`, `L031`, `L050`, `L060`, `L070`, `L080`, `L090` (all `L-SEC-*` SEC-prefixed subset)

### Runtime features
- HITL (human-in-the-loop) bridge — `runtime/hitl.rs`, `hitl_bridge.rs`
- Record compression — `runtime/record_compress.rs`
- robots.txt enforcement — `runtime/robots.rs`
- Fetch cache — `runtime/fetch_cache.rs` (module in `nika-runtime` per invariants)
- Rate limiting — `runtime/rate_limit.rs` + `nika-serve/rate_limit.rs`
- Output scanner (outbound secret/canary scan) — `runtime/output_scanner.rs` (part of Shield)
- Chat workflow — `runtime/chat_workflow.rs`
- Artifact persistence modes — `overwrite | append | unique | fail` + `manifest.json`
- Output modes — `text | json | yaml | markdown | binary`
- 65 templatable fields — per-field type coercion rules

### Provider-specific features (owned by nika-providers / nika-verb-infer)
- Anthropic `extended_thinking` + `thinking_budget` tokens
- OpenAI `reasoning_effort` for o1/o3/o4
- Gemini JSON mode `response_format: json`
- Multimodal `content:` array with CAS-hash `source:` only
- Slash syntax `model: groq/llama-3.3-70b-versatile` + `[endpoints.<name>]` config

### Schema / Binding (owned by nika-schema / nika-binding)
- Include partials (`include:` with prefix namespacing)
- `depends_on` explicit ordering (no data dep)
- `when:` conditionals (template expression eval)
- `for_each` concurrency + `fail_fast` semantics
- Run depth / cycle detection for workflow recursion (NIKA-386/387)

### Security / Storage
- ML injection detector (BERT-based) — heuristic at v0.90, ML at v0.95
- Postgres storage backend — `nika-storage/postgres.rs` (module in `nika-daemon`)
- Webhook delivery — `nika-serve/webhook.rs` + `daemon/services/watch.rs`
- OpenAPI spec generator — `nika-serve/openapi.rs`

## Explicitly DROPPED (with rationale)

| Item | Drop reason |
|---|---|
| `nika-sdk` (remote client SDK) | 0 consumers, speculative |
| `nika-napi` (Node.js bindings) | W1 removed, use TypeScript `@nika/client` SDK instead |
| `nika-py` (Python bindings) | W1 removed, no user pull |
| `nika-tui` (terminal UI) | W1 removed, rebuild Act 3 or never |
| `ProviderCategory` enum | 11 ex-MCP entries migrated to `McpAlias` catalog |
| `unwrap-baseline.txt` | Implicitly dropped — new policy is zero unwrap in src/ |
| legacy `nika-macros` crate | REMOVED — no proc macros in Diamond (Q1 decision 2026-04-16, manual impl + macro_rules!) |
| legacy `nika-lsp-core` crate | Merged into `nika-lsp` per invariants L49 |
| legacy `nika-policy` crate | Module in `nika-runtime` per invariants L52 |
| legacy `nika-cache` crate | Module in `nika-runtime` per invariants L54 |
| legacy `nika-storage` crate | Module in `nika-daemon` per invariants L53 |

## Verification protocol

Before admitting each Phase 2+ crate:
1. Grep legacy `git show brouillon:tools/<legacy-name>/src/` for top-level modules
2. Cross-reference this checklist — every feature mapped or explicitly dropped
3. Update `docs/crate-specs/nika-X.md` with migration notes
4. Run `scripts/hygiene/check-all.sh` to verify ROADMAP ↔ checklist ↔ Linear coherence

🦋 Zero feature lost in the rewrite. Everything either admitted, spec'd, or
explicitly dropped.
