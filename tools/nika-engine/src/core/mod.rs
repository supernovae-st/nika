// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Core types for Nika — zero-dependency provider, model, and MCP definitions.
//!
//! This module provides the canonical definitions for:
//! - **Providers**: LLM providers (Anthropic, OpenAI, etc.) and MCP providers (Neo4j, GitHub, etc.)
//! - **Models**: Curated local models for native inference (mistral.rs)
//! - **MCP Aliases**: Short names → npm package mappings for MCP servers
//! - **Backend**: Backend types for model management (progress, info, config)
//! - **Storage**: HuggingFace model storage with download and checksum verification
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  nika::core MODULE                                                          │
//! ├─────────────────────────────────────────────────────────────────────────────┤
//! │                                                                             │
//! │  providers.rs                                                               │
//! │  ├── KNOWN_PROVIDERS: &[Provider] (18 providers)                            │
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
//! │  ├── MCP_ALIASES: &[(&str, &str)] (100 aliases)                              │
//! │  └── Helper functions (resolve_alias, list_aliases)                         │
//! │                                                                             │
//! │  backend.rs                                                                 │
//! │  ├── PullProgress, ModelInfo, DownloadRequest, DownloadResult               │
//! │  ├── LoadConfig, ChatRole, ChatMessage, ChatOptions, ChatResponse           │
//! │  └── BackendError enum for error handling                                   │
//! │                                                                             │
//! │  storage.rs                                                                 │
//! │  ├── HuggingFaceStorage for model downloads                                 │
//! │  ├── default_model_dir(), detect_system_ram_gb()                            │
//! │  └── StorageError, extract_quantization()                                   │
//! │                                                                             │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Architecture
//!
//! This is the core module for Nika. All provider, model, and MCP
//! definitions are defined directly in this module with zero external dependencies.
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

// ── Catalog modules (from nika-core) ────────────────────────────────────────
// Pure data: types, statics, lookup functions. Zero runtime dependencies.
pub use nika_core::catalogs::mcp_aliases;
pub use nika_core::catalogs::providers;

// Models: catalog types from nika-core + runtime functions in local models.rs
pub mod models;

// ── Local modules (runtime dependencies) ────────────────────────────────────
pub mod backend;
pub mod mcp_config;
pub mod paths;
pub mod storage;

// Re-export main types for convenient access
pub use backend::{
    BackendError, ChatMessage, ChatOptions, ChatResponse, ChatRole, DownloadRequest,
    DownloadResult, LoadConfig, ModelInfo, NativeModelKind, PullProgress, VisionImage,
};
pub use mcp_aliases::{
    aliases_by_category, list_aliases, pricing_label, resolve_alias, resolve_name, McpAlias,
    McpPricing, CATEGORIES, MCP_ALIASES,
};
pub use mcp_config::{
    add_server_to_global, add_server_to_project, global_config_path, load_global_config,
    load_merged_config, load_project_config, project_config_path, remove_server_from_global,
    remove_server_from_project, save_global_config, save_project_config, server_from_npm_package,
    McpConfig, McpConfigError, McpConfigResolver, McpResolveSource, McpServer, McpSource,
};
pub use models::{
    auto_select_quantization, detect_available_ram_gb, find_model, models_by_type, resolve_model,
    KnownModel, ModelArchitecture, ModelResolveError, ModelType, Quantization, ResolvedModel,
    KNOWN_MODELS,
};
pub use providers::{
    find_provider, provider_to_env_var, providers_by_category, validate_key_format, Provider,
    ProviderCategory, KNOWN_PROVIDERS,
};
pub use storage::{
    default_model_dir, detect_system_ram_gb, extract_quantization, HuggingFaceStorage, StorageError,
};

// Path utilities
pub use paths::{
    // Directory paths
    backups_dir,
    cache_dir,
    daemon_dir,
    // File paths (prefixed to avoid conflicts with mcp_config)
    daemon_pid_path,
    daemon_socket_path,
    // Directory creation
    ensure_nika_home,
    ensure_project_nika_dir,
    global_mcp_config_path,
    models_dir,
    // Home directory
    nika_home,
    nika_home_opt,
    // Package paths
    package_dir,
    package_manifest_path,
    packages_dir,
    // Project paths
    project_lockfile_path,
    project_manifest_path,
    project_mcp_config_path,
    project_nika_dir,
    project_sessions_dir,
    registry_index_path,
    // Constants
    DAEMON_PID,
    DAEMON_SOCKET,
    GLOBAL_CONFIG,
    MCP_CONFIG,
    NIKA_DIR_NAME,
    NIKA_HOME_ENV,
    NIKA_LOCKFILE,
    NIKA_MANIFEST,
    NIKA_PROJECT_DIR,
    REGISTRY_INDEX,
};
// Note: global_config_path is exported from paths module but accessed via paths::global_config_path
// to avoid conflict with mcp_config::global_config_path
