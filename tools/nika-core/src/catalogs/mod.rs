//! Static catalogs -- zero-dependency provider, model, and MCP alias definitions.
//!
//! These are pure data modules with no runtime dependencies. They define the
//! canonical catalogs that both `nika` and `nika-core` consumers can use.
//!
//! - [`providers`] -- 19 known providers (LLM, MCP, Local)
//! - [`models`] -- 15+ curated models for native inference (mistral.rs)
//! - [`mcp_aliases`] -- 100 MCP server short-name aliases

pub mod mcp_aliases;
pub mod models;
pub mod providers;

// Re-export main types for convenient access
pub use mcp_aliases::{
    aliases_by_category, is_alias, list_aliases, resolve_alias, resolve_name, MCP_ALIASES,
};
pub use models::{
    auto_select_quantization, find_model, models_by_type, KnownModel, ModelArchitecture,
    ModelResolveError, ModelType, Quantization, KNOWN_MODELS,
};
pub use providers::{
    find_provider, provider_to_env_var, providers_by_category, validate_key_format, Provider,
    ProviderCategory, KNOWN_PROVIDERS,
};
