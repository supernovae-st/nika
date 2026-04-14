# Changelog

All notable changes to Nika are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Nika Diamond is a ground-up rewrite on an orphan branch (`nika-diamond`).
Legacy main sits at v0.79.3. Diamond starts at v0.80.0, targeting v0.90.0.
No public releases yet -- internal tracking only.

## [Unreleased]

### Fixed

- **nika-catalog Phase A cleanup** (db0bf8e3f, 2026-04-14) -- removed 29 broken
  MCP aliases found via 5-agent deep audit:
  - 7 deprecated Anthropic reference servers (puppeteer, brave-search, google-maps,
    github, gitlab, postgres, neon -- Anthropic marked them "Package no longer
    supported" on npm). Plus `brave` alias pointing to the deprecated brave-search.
  - 18 aliases pointing to non-existent npm packages (fetch, sqlite, stability,
    elevenlabs, deepgram, intercom, weaviate, qdrant, chroma, milvus, posthog,
    grafana, coinbase, fly, asana, raygun, buildkite, bing -- Python-only, Go
    binaries, remote-only, or fabricated names).
  - 3 zero-weekly-download abandoned community forks (semrush, trello, todoist).
  - New test `removed_broken_aliases_not_present` blocks regression.
  - Total: 131 -> 102 aliases. Categories recounted: anthropic(9->3),
    databases(10->7), search(9->7), developer(19->17), productivity(15->12),
    image(4->3), audio(2->0), communication(6->5), vectordb(6->2),
    analytics(8->6), ecommerce(7->6), devops(7->6), marketing(4->3).

### Added

- **nika-catalog v2 enrichment** (6a943e885, 2026-04-13) -- 31 npm package
  name corrections (all verified on npm registry), 20 new aliases, 6 gap-fill
  additions (chrome-devtools 406k dl/wk, pdf 277k/wk official Anthropic,
  dynatrace, apify, heroku, browserstack). 5 enterprise providers added:
  bedrock, azure, vertex, perplexity (LLM), voyage. Aliases added:
  aws-bedrock, azure-openai, gcp, pplx. #[non_exhaustive] on McpAlias.
  New test `all_provider_aliases_in_map` enforces Provider.aliases coverage.
  Pre-cleanup total: 131 MCP aliases, 21 providers. 86 tests.
- **nika-catalog** -- L0 static catalogs crate admitted to workspace (55a451695).
  Hybrid lookup: phf + unicase for providers/MCP (O(1)), sorted arrays for
  builtins/transforms (O(log n)). 16 providers, 113 MCP aliases, 63 builtins,
  65 transforms, 61 pricing entries. 2,235 LOC, 85 tests, 94.7% mutation killed.
- **nika-kernel + nika-kernel-mock** -- L0.5 kernel traits admitted (ef8804371).
  99 kernel tests, 88 kernel-mock tests.
- **nika-error** -- L0 error infrastructure crate admitted to workspace (42909b1c7).
  Strategy C+: trait `NikaErrorCode` + `NikaError(Box<dyn>)` + `CoreError`.
  `NikaCode` dual wire format (Display `"NIKA-001"`, serde roundtrip).
  1,013 LOC, 44 tests, 100% mutation killed.

### Planned (Phase C — v0.90 catalog architecture)

See `CATALOG_V3_PLAN.md` in private memory. Key moves:

- Migrate catalog data from hardcoded Rust arrays to `data/*.toml` files,
  compiled via `build.rs` + `phf_codegen` to zero-runtime-overhead phf maps.
- Adopt official MCP Registry schema (packages[] + remotes[]) for multi-
  distribution support (npm, pypi, oci, remote streamable-http/sse).
- New `xtask verify-catalog` with parallel npm/pypi/docker registry checks;
  GitHub Actions daily cron on drift detection.
- Add Tier 1 providers: moonshot (Kimi K2), qwen (Alibaba), deepinfra, novita.
- Update 9 default model IDs to April 2026 flagships (Claude 4.5 Sonnet,
  GPT-5, Grok 4, Gemini 3.0 Pro, Command A, Jamba 1.6, Voyage 3.5, cross-
  region Bedrock profile).
- Fill pricing gaps (gpt-5-mini/nano, Perplexity Sonar variants, Cohere,
  AI21 Jamba 1.6, Voyage embeddings, grok-4).
- Add `context_window_tokens` + `max_output_tokens` per model.
- Scope `find_pricing` by provider (fix silent wrong-cost bug on custom endpoints).
- Add `Credential` shared type de-duping Provider + McpAlias secret metadata.

### Planned (Phase D — native API adapters, post-v0.90)

Based on 5-agent brainstorm 2026-04-14:

- Top-10 native Rust adapters (~500 LOC each, ~5k LOC total, 3-5 new crates):
  `nika:github` (octocrab), `nika:aws` (aws-sdk-rust), `nika:slack`
  (slack-morphism), `nika:notion`, `nika:linear`, `nika:huggingface`
  (hf-hub), `nika:ollama`, `nika:stripe` (async-stripe), `nika:cloudflare`
  (cloudflare-rs).
- Framing locked: thin adapters wrapping existing Rust SDKs, NOT CLI
  reimplementations. Each builtin must erase >=80% of `fetch:` boilerplate
  or it's not worth shipping.
- Skip list: terraform, pulumi, modal, heroku (decline), spotify (niche),
  railway (GraphQL churn), jira (enterprise auth complexity), full Discord
  (stateful WS).

### Planned (Phase E — sharing primitive, v0.90 candidate)

- `nika init --from github.com/user/repo` -- git-native template scaffold,
  zero registry infrastructure. Covers 80% of the sharing use case.
- `nika-catalog` extended to include curated workflows + skills + provider
  configs, phf-compiled. PR-based contribution (Obsidian / Homebrew pattern).
- `nika pck` full registry DEFERRED to v1.5+. Solo team cannot run a
  cloud registry. Git-repo-as-registry (homebrew-tap model) when/if it ships.

## [0.80.0-alpha.0] - 2026-04-13

Workspace scaffold. Orphan branch `nika-diamond` created from scratch --
no code inherited from main.

### Added

- Orphan branch initialized with workspace Cargo.toml (edition 2024, Rust 1.91).
- Workspace-level lint policy: `clippy::unwrap_used = "deny"`,
  `clippy::expect_used = "warn"`, `clippy::panic = "deny"`.
- `.gitignore` excluding all 32 legacy crate directories and tool output.
- `DIAMOND.md` architectural vision with complete 34-crate catalog.
- `README.md` rewritten for Phase 1 state.
- Claude Code environment, DX rules, session discipline, and hooks.
- Linear setup guide for diamond project tracking.

[Unreleased]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.0...HEAD
[0.80.0-alpha.0]: https://github.com/supernovae-st/nika/commits/v0.80.0-alpha.0
