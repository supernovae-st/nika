# Phase 3: Package Manager Components

## Overview

**Goal**: Document the already-completed package manager migration.
**Status**: ✅ DONE (v0.27.0 development cycle)
**Lines**: ~0 (already migrated)

---

## Executive Summary

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  ✅ PHASE 3 COMPLETE                                                          ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  The package manager components were migrated during v0.27.0 development.     ║
║  This phase documents what was done for reference.                            ║
║                                                                               ║
║  No additional work required.                                                 ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## What Was Migrated

### 1. Provider Definitions (`src/core/providers.rs`)

**Source**: `spn-core/src/providers.rs`
**Destination**: `nika/tools/nika/src/core/providers.rs`

```rust
/// Known providers with validation rules.
pub static KNOWN_PROVIDERS: &[ProviderDef] = &[
    // LLM Providers (7)
    ProviderDef {
        name: "anthropic",
        env_var: "ANTHROPIC_API_KEY",
        prefix: "sk-ant-",
        display_name: "Anthropic (Claude)",
    },
    ProviderDef {
        name: "openai",
        env_var: "OPENAI_API_KEY",
        prefix: "sk-",
        display_name: "OpenAI",
    },
    ProviderDef {
        name: "mistral",
        env_var: "MISTRAL_API_KEY",
        prefix: "",
        display_name: "Mistral AI",
    },
    ProviderDef {
        name: "groq",
        env_var: "GROQ_API_KEY",
        prefix: "gsk_",
        display_name: "Groq",
    },
    ProviderDef {
        name: "deepseek",
        env_var: "DEEPSEEK_API_KEY",
        prefix: "sk-",
        display_name: "DeepSeek",
    },
    ProviderDef {
        name: "gemini",
        env_var: "GEMINI_API_KEY",
        prefix: "",
        display_name: "Google Gemini",
    },
    ProviderDef {
        name: "ollama",
        env_var: "OLLAMA_API_BASE_URL",
        prefix: "",
        display_name: "Ollama (Local)",
    },

    // MCP Secret Providers (6)
    ProviderDef {
        name: "neo4j",
        env_var: "NEO4J_PASSWORD",
        prefix: "",
        display_name: "Neo4j Database",
    },
    ProviderDef {
        name: "github",
        env_var: "GITHUB_TOKEN",
        prefix: "ghp_",
        display_name: "GitHub",
    },
    // ... + slack, perplexity, firecrawl, supadata
];
```

**Verification**: `cargo test core::providers`

---

### 2. Model Definitions (`src/core/models.rs`)

**Source**: `spn-core/src/models.rs`
**Destination**: `nika/tools/nika/src/core/models.rs`

```rust
/// Known models for native inference.
pub static KNOWN_MODELS: &[ModelDef] = &[
    ModelDef {
        id: "llama3.2:1b",
        name: "Llama 3.2 1B",
        family: "llama",
        parameters: "1B",
        quantization: "Q4_K_M",
        size_gb: 1.2,
        context_length: 8192,
    },
    ModelDef {
        id: "llama3.2:3b",
        name: "Llama 3.2 3B",
        family: "llama",
        parameters: "3B",
        quantization: "Q4_K_M",
        size_gb: 2.1,
        context_length: 8192,
    },
    ModelDef {
        id: "qwen3:8b",
        name: "Qwen 3 8B",
        family: "qwen",
        parameters: "8B",
        quantization: "Q4_K_M",
        size_gb: 4.8,
        context_length: 32768,
    },
    // ... 16+ models total
];
```

**Verification**: `cargo test core::models`

---

### 3. MCP Aliases (`src/core/mcp_aliases.rs`)

**Source**: `spn-core/src/mcp_aliases.rs`
**Destination**: `nika/tools/nika/src/core/mcp_aliases.rs`

```rust
/// MCP server aliases for auto-configuration.
/// 48 aliases covering common MCP servers.
pub static MCP_ALIASES: &[McpAlias] = &[
    McpAlias {
        name: "neo4j",
        full_name: "@neo4j/mcp-neo4j",
        command: "npx",
        args: &["-y", "@neo4j/mcp-neo4j"],
        env_vars: &["NEO4J_URI", "NEO4J_USERNAME", "NEO4J_PASSWORD"],
    },
    McpAlias {
        name: "github",
        full_name: "@anthropic/mcp-server-github",
        command: "npx",
        args: &["-y", "@anthropic/mcp-server-github"],
        env_vars: &["GITHUB_TOKEN"],
    },
    McpAlias {
        name: "perplexity",
        full_name: "@anthropic/mcp-server-perplexity",
        command: "npx",
        args: &["-y", "@anthropic/mcp-server-perplexity"],
        env_vars: &["PERPLEXITY_API_KEY"],
    },
    // ... 48 total aliases
];
```

**Verification**: `cargo test core::mcp_aliases`

---

### 4. MCP Config (`src/core/mcp_config.rs`)

**Source**: `spn/src/daemon/mcp.rs`
**Destination**: `nika/tools/nika/src/core/mcp_config.rs`

```rust
/// MCP configuration with global/project scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    pub servers: HashMap<String, McpServer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub command: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub enabled: bool,
}

impl McpConfig {
    /// Load global config (~/.spn/mcp.yaml)
    pub fn load_global() -> Result<Self, NikaError>;

    /// Load project config (./mcp.yaml)
    pub fn load_project() -> Result<Option<Self>, NikaError>;

    /// Merge configs (project overrides global)
    pub fn merge(&self, other: &Self) -> Self;
}
```

**Verification**: `cargo test core::mcp_config`

---

### 5. Secrets Management (`src/secrets/`)

**Source**: `spn/src/secrets/`
**Destination**: `nika/tools/nika/src/secrets/`

```rust
// src/secrets/mod.rs
pub mod resolve;

pub use resolve::{KeychainResolver, SecretValue};

// src/secrets/resolve.rs
/// Unified secret resolution with daemon IPC fallback.
pub struct KeychainResolver {
    /// spn daemon client (optional)
    daemon: Option<DaemonClient>,
}

impl KeychainResolver {
    /// Resolve a secret with fallback chain:
    /// 1. spn daemon (IPC)
    /// 2. OS Keychain
    /// 3. Environment variable
    pub async fn resolve(&self, provider: &str) -> Option<SecretValue>;
}
```

**Verification**: `cargo test secrets`

---

## CLI Commands Added

These commands were added to nika CLI during v0.27.0:

| Command | Subcommands | Description |
|---------|-------------|-------------|
| `nika provider` | `list`, `set`, `get`, `test`, `migrate` | API key management |
| `nika model` | `list`, `pull`, `info`, `search` | Local model management |
| `nika mcp` | `add`, `remove`, `list`, `test`, `tools` | MCP server management |
| `nika sync` | `--status`, `--enable`, `--disable` | Editor synchronization |
| `nika setup` | `nika`, `novanet`, `claude-code` | Interactive onboarding |
| `nika daemon` | `start`, `stop`, `status` | Background service |
| `nika jobs` | `submit`, `cancel`, `output`, `list` | Background jobs |
| `nika backup` | `create`, `restore`, `list`, `prune` | Data backup |

---

## Tests Added

```
tests/
├── core_providers_test.rs     # 12 tests
├── core_models_test.rs        # 8 tests
├── core_mcp_aliases_test.rs   # 6 tests
├── core_mcp_config_test.rs    # 10 tests
└── secrets_test.rs            # 8 tests
```

**Total**: 44 tests for package manager components

---

## Remaining External Dependencies

### spn-client

The `spn-client` crate remains as a **library** for IPC communication:

```toml
# nika/tools/nika/Cargo.toml
[dependencies]
spn-client = { path = "../../../supernovae-cli/crates/spn-client", optional = true }

[features]
spn-daemon = ["spn-client"]  # Enable daemon IPC
```

**Rationale**: Keeps the IPC protocol as a shared library that both old `spn` and new `nika` can use during the transition period.

### spn daemon process

The actual daemon process (`spn daemon start`) still lives in `supernovae-cli`. After Phase 2 (Daemon Features), this will be replaced by `nika daemon`.

---

## Validation Checklist

- [x] All provider definitions migrated
- [x] All model definitions migrated
- [x] All MCP aliases migrated
- [x] MCP config loading works
- [x] Secrets resolution works
- [x] CLI commands functional
- [x] Tests passing (44 tests)
- [x] Zero clippy warnings
- [x] Documentation updated

---

## Summary

Phase 3 was completed during v0.27.0 development. The package manager is fully integrated into nika. The remaining work is:

1. **Phase 2**: Migrate daemon features (MemoryStore, TraceStore, etc.)
2. **Phase 4**: Implement config scope system
3. **Phase 5**: Deprecate spn and finalize interop

No additional coding required for Phase 3.
