# nika-catalog

Compile-time capability vocabulary — 39 LLM providers, 105 MCP servers,
13 embeddings, 30 builtins, 57 capability rules (as of 0.123.0, with `nika:image_compose` since).

<!-- The five counts above are not typed from memory. Three of them are
     asserted by this crate's own tests (`src/lib.rs` · `all_providers`,
     `all_mcp_servers`, `all_builtins`) — the truth sat 15 lines from the
     lie until 2026-08-12, when the header still read 32 providers and 63
     builtins against asserts of 38 and 28, and again until 0.123.0, when it
     read 38 and 28 against asserts of 39 and 29. The remaining two derive
     from the TOML data (`data/embeddings.toml` · `data/model-capabilities.toml`,
     one `[[…]]` per entry) and are guarded by NOTHING — re-derive them
     before trusting them. -->


Layer **L0** crate. Ships TOML-driven catalog data baked into the binary
via `phf` perfect-hash tables (build-time codegen by `nika-catalog-codegen`).
Zero runtime parsing, zero `HashMap` allocation, zero I/O on lookup — every
catalog query is a static const lookup. The catalog is the **single source
of truth** for what models exist, what pricing applies, what MCP servers
are known, and what capability rules constrain workflows.

## Usage

A workspace-internal crate (`publish = false`): depend on it by path.

```toml
[dependencies]
nika-catalog = { path = "../nika-catalog" }
```

```rust
use nika_catalog::{all_builtins, all_providers, find_builtin, find_pricing_scoped, find_provider};

// Static lookups over build-time tables — zero allocations, zero I/O.
let deepseek = find_provider("deepseek");                        // id or alias, case-insensitive
let price = find_pricing_scoped("deepseek", "deepseek-flash");   // Option<&'static ModelPricing>
let remove = find_builtin("remove_file");                        // name without the `nika:` prefix
let (providers, builtins) = (all_providers().len(), all_builtins().len()); // 39 · 29 in 0.123.0
```

## Catalog surface (0.123.0)

- **39 LLM providers** · Anthropic · OpenAI · Google · DeepSeek · Mistral · xAI · Scaleway · Groq · Ollama · LM Studio · + 29 more (31 speak the OpenAI chat dialect)
- **105 MCP servers** · vocabulary per [MCP spec](https://modelcontextprotocol.io)
- **13 embedding models** · BGE-M3 (multilingual default · Phase 0 lock per ADR-005) + alternatives
- **30 builtins** · the `nika:*` tools in six categories (core · file · data · network · introspection · media)
- **57 capability rules** · what each model accepts (modalities, token-limit parameter, JSON mode, supported parameters), first match wins

## Features

- `default = []` — pure compile-time tables, zero optional features.
- All TOML data lives at `data/*.toml`, regenerated via `nika-catalog-codegen` at build time.

## MSRV

Rust 1.91+ (matches `[workspace.package].rust-version`).

## License

AGPL-3.0-or-later. Co-author trailer `Nika 🦋 <nika@supernovae.studio>`.

## Related

- [ADR-008 — TOML-driven catalog](../../docs/adr/adr-008-toml-driven-catalog.md)
- [ADR-029 — Embedding-spec reservation](../../docs/adr/adr-029-embedding-spec-reservation.md)
- [COMMUNITY_EXTENSIONS.md](COMMUNITY_EXTENSIONS.md) — proposal process for new catalog entries
- [BLUEPRINT_2036.md](../../docs/architecture/BLUEPRINT_2036.md) — the earlier long-range proposal, kept as history
