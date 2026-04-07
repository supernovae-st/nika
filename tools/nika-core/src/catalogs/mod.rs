//! Static catalogs -- zero-dependency provider, model, and MCP alias definitions.
//!
//! These are pure data modules with no runtime dependencies. They define the
//! canonical catalogs that both `nika` and `nika-core` consumers can use.
//!
//! - [`providers`] -- 19 known providers (LLM, MCP, Local)
//! - [`models`] -- 15+ curated models for native inference (mistral.rs)
//! - [`mcp_aliases`] -- 100 MCP server short-name aliases
//! - [`builtins`] -- 63 known nika:* builtin tool names

pub mod builtins;
pub mod capabilities;
pub mod cost;
pub mod lsp_types;
pub mod mcp_aliases;
pub mod models;
pub mod providers;
pub mod resolver;

// Re-export main types for convenient access
pub use builtins::{is_known_builtin, KNOWN_BUILTIN_TOOLS};
pub use capabilities::{model_capabilities, ModelCapabilities, TokenLimitParam};
pub use cost::{
    estimate_cost, find_pricing, model_cost_label, CostEstimate, ModelPricing, KNOWN_PRICING,
};
pub use lsp_types::{DaemonCapabilities, KeySource, ProviderStatusInfo, WorkflowRunInfo};
pub use mcp_aliases::{
    aliases_by_category, is_alias, list_aliases, pricing_label, resolve_alias, resolve_name,
    McpAlias, McpPricing, CATEGORIES, MCP_ALIASES,
};
pub use models::{
    auto_select_quantization, find_model, models_by_type, KnownModel, ModelArchitecture,
    ModelResolveError, ModelType, Quantization, KNOWN_MODELS,
};
pub use providers::{
    find_provider, provider_to_env_var, providers_by_category, validate_key_format, Provider,
    ProviderCategory, KNOWN_PROVIDERS,
};
pub use resolver::{
    cheap_model_for_provider, default_model_for_provider, ModelCompatibility, ModelResolver,
    ModelSource, ResolvedModel, PROVIDER_CHEAP_MODELS, PROVIDER_DEFAULTS,
};
