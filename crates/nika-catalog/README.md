# nika-catalog

Compile-time capability vocabulary — 32 LLM providers, 105 MCP servers,
13 embeddings, 63 builtins, 49 capability rules.

Layer **L0** crate. Ships TOML-driven catalog data baked into the binary
via `phf` perfect-hash tables (build-time codegen by `nika-catalog-codegen`).
Zero runtime parsing, zero `HashMap` allocation, zero I/O on lookup — every
catalog query is a static const lookup. The catalog is the **single source
of truth** for what models exist, what pricing applies, what MCP servers
are known, and what capability rules constrain workflows.

## Usage

```toml
[dependencies]
nika-catalog = { version = "0.80", path = "../nika-catalog" }
```

```rust
use nika_catalog::{Catalog, ModelId, ProviderId};

// Static const lookup — zero allocations
let model = Catalog::model("claude-opus-4-7");
let provider = model.provider();   // ProviderId::Anthropic
let pricing = model.pricing();     // ModelPricing { input_usd: 15.00, ... }

// 7-axis pricing (ADR-008): input · output · cached_input · image · audio
// reasoning · batch — populated per-provider from live research.
```

## Catalog surface (2026-Q2 lock)

- **32 LLM providers** · Anthropic · OpenAI · Google · DeepSeek · Mistral · xAI · Groq · Ollama · LM Studio · + 23 more
- **105 MCP servers** · vocabulary per [MCP spec](https://modelcontextprotocol.io)
- **13 embedding models** · BGE-M3 (multilingual default · Phase 0 lock per ADR-005) + alternatives
- **63 builtins** · native API adapters (github · cloud · workspace · …)
- **49 capability rules** · workflow security constraints (`nika-shield` consumer)

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
- [BLUEPRINT_2036.md](../../docs/architecture/BLUEPRINT_2036.md) — 10-year horizon
