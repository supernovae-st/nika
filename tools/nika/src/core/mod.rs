//! Core types for Nika — zero-dependency provider, model, and MCP definitions.
//!
//! This module provides the canonical definitions for:
//! - **Providers**: LLM providers (Anthropic, OpenAI, etc.) and MCP providers (Neo4j, GitHub, etc.)
//! - **Models**: Curated local models for native inference (mistral.rs)
//! - **MCP Aliases**: Short names → npm package mappings for MCP servers
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  nika::core MODULE                                                          │
//! ├─────────────────────────────────────────────────────────────────────────────┤
//! │                                                                             │
//! │  providers.rs                                                               │
//! │  ├── KNOWN_PROVIDERS: &[Provider] (20 providers)                            │
//! │  ├── Provider struct (id, name, env_var, key_prefix, category)              │
//! │  ├── ProviderCategory enum (Llm, Mcp, Local)                                │
//! │  └── Helper functions (find_provider, providers_by_category)                │
//! │                                                                             │
//! │  models.rs                                                                  │
//! │  ├── KNOWN_MODELS: &[KnownModel] (16+ curated models)                       │
//! │  ├── KnownModel struct (id, name, architecture, hf_repo, quantizations)     │
//! │  ├── ModelType enum (Text, Vision, Embedding, Audio, Diffusion)             │
//! │  ├── ModelArchitecture enum (~30 architectures for mistral.rs)              │
//! │  └── Helper functions (find_model, resolve_model, auto_select_quantization) │
//! │                                                                             │
//! │  mcp_aliases.rs                                                             │
//! │  ├── MCP_ALIASES: &[(&str, &str)] (48 aliases)                              │
//! │  └── Helper functions (resolve_alias, list_aliases)                         │
//! │                                                                             │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Migration from spn-core
//!
//! This module is part of the spn→nika feature fusion (ADR-TBD). Previously,
//! these types were in the `spn-core` crate and imported via `spn_client::KNOWN_PROVIDERS`.
//! Now they live directly in nika, removing the spn dependency for core types.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika::core::{KNOWN_PROVIDERS, find_provider, ProviderCategory};
//! use nika::core::{KNOWN_MODELS, find_model, ModelType};
//! use nika::core::{MCP_ALIASES, resolve_alias};
//!
//! // Find a provider
//! let anthropic = find_provider("anthropic").unwrap();
//! assert_eq!(anthropic.env_var, "ANTHROPIC_API_KEY");
//!
//! // Find a model
//! let qwen = find_model("qwen3:8b").unwrap();
//! assert_eq!(qwen.param_billions, 8.0);
//!
//! // Resolve MCP alias
//! let pkg = resolve_alias("neo4j").unwrap();
//! assert_eq!(pkg, "@neo4j/mcp-server-neo4j");
//! ```

pub mod mcp_aliases;
pub mod mcp_config;
pub mod models;
pub mod providers;

// Re-export main types for convenient access
pub use mcp_aliases::{aliases_by_category, list_aliases, resolve_alias, resolve_name, MCP_ALIASES};
pub use mcp_config::{
    add_server_to_global, add_server_to_project, global_config_path, load_global_config,
    load_merged_config, load_project_config, project_config_path, remove_server_from_global,
    remove_server_from_project, save_global_config, save_project_config, server_from_npm_package,
    McpConfig, McpConfigError, McpServer, McpSource,
};
pub use models::{
    auto_select_quantization, detect_available_ram_gb, find_model, models_by_type, resolve_model,
    KnownModel, ModelArchitecture, ModelType, Quantization, ResolvedModel, KNOWN_MODELS,
};
pub use providers::{
    find_provider, provider_to_env_var, providers_by_category, validate_key_format, Provider,
    ProviderCategory, KNOWN_PROVIDERS,
};
