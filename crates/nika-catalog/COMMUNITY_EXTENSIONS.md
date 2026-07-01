# Extending nika-catalog

`nika-catalog` is designed to be extended by community crates that add
vendor-specific, region-specific, or domain-specific catalog entries
without requiring changes to the core crate.

This document describes the **extension-author pattern** — how to publish a
catalog crate (e.g. `nika-catalog-cn` for Chinese providers) that plugs into
the runtime via the (forthcoming v0.95) `CatalogOverlay` trait.

---

## Pattern

Your extension crate depends on `nika-catalog` with **minimal features**:

```toml
[dependencies]
nika-catalog = { version = "0.92", default-features = false, features = ["extension-author"] }
```

This pulls in only the type definitions (`Provider`, `McpServer`, `Embedding`,
`Tag`, etc.) and the `CatalogError` / `Suggestion` types — **no bundled
catalog data** from the core crate.

Your crate then defines its own `&'static` slices matching the core types:

```rust
use nika_catalog::{Provider, ProviderModel, Tag};

pub static CN_PROVIDERS: &[Provider] = &[
    Provider {
        id: "moonshot",
        name: "Moonshot AI (Kimi)",
        aliases: &["kimi"],
        env_var: "MOONSHOT_API_KEY",
        key_prefixes: &["sk-"],
        default_model: "kimi-k2",
        cheap_model: "kimi-k1",
        requires_key: true,
        description: "Kimi K2/K1.5 — 2M context, Chinese-first frontier.",
        models: &[
            ProviderModel {
                id: "k2",
                model: "kimi-k2",
                context_window_tokens: 2_000_000,
                max_output_tokens: 8_192,
            },
        ],
        tags: &[Tag::Chinese, Tag::Frontier, Tag::LongContext],
        extra_tags: &[],
    },
    // …
];
```

Users opt into the extension via a `CatalogOverlay` registered at
`nika-runtime` startup (v0.95 — see `nika-kernel::CatalogOverlay`). Until
then, extensions can be exposed as utility statics that consumers iterate
explicitly.

---

## Cargo features reference

| Feature              | Pulls in                                          |
| -------------------- | ------------------------------------------------- |
| `default`            | `full + serde`                                    |
| `full`               | All six content features below                    |
| `minimal`            | Types + `Tag` + `error` only (no catalog data)    |
| `extension-author`   | Alias for `minimal` — clearer intent              |
| `mcp`                | MCP-server catalog (105 entries)                  |
| `providers`          | LLM-provider catalog (21 entries)                 |
| `embeddings`         | Embedding catalog (13 entries) — implies `providers` |
| `pricing`            | Cost tables — implies `providers`                 |
| `capabilities`       | Per-model capability resolver — implies `providers` |
| `builtins-transforms`| Builtin tools (63) + pipe transforms (65)         |
| `serde`              | `Serialize/Deserialize` on public types           |

---

## Naming conventions for community crates

| Crate name                  | Scope                                                     |
| --------------------------- | --------------------------------------------------------- |
| `nika-catalog-cn`           | Chinese providers / MCPs (Moonshot, Qwen, Z.ai, …)        |
| `nika-catalog-eu`           | European providers (Mistral, Nebius, Infomaniak, Scaleway)|
| `nika-catalog-enterprise`   | SaaS B2B (Salesforce, Workday, SAP, ServiceNow)           |
| `nika-catalog-research`     | Academic / specialty (Reka, Aleph Alpha, AI21)            |
| `nika-catalog-medical`      | HIPAA-compliant providers and embeddings                  |
| `nika-catalog-<org>`        | Private internal catalogs (e.g. `nika-catalog-acme`)      |

---

## What NOT to put in an extension

* Anything that already exists in the core catalog (overlay merge will reject
  duplicate IDs)
* Typos or untested entries — run `nika-catalog-verify` on your slices first
* Rate-limit or tool-schema data — these are v0.95+ concerns and not yet
  schema-stable
* `Tag` variants you wish existed — open an issue on the core crate; in the
  meantime, use `extra_tags: &["my-experimental-tag"]` (zero validation
  passthrough escape hatch)

---

## Reserved field names (v0.95+)

The following field names are reserved on catalog entry types — do **not**
use them in your extension's struct literals (they will be added in a
forward-compatible way to the core types):

* `emits_events: &'static [EventKind]` — runtime hint for planner
* `deprecated_in: Option<&'static str>` + `replacement_id` — lifecycle
  metadata (Session 4 of Phase D)
* `headquarters: Region` + `datacenters: &'static [Region]` — provider
  trust / EU-AI-Act compliance fields (Session 3 of Phase D)
