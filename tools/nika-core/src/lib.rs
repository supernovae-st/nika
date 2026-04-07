//! Lightweight AST and analysis core for Nika workflows.
//!
//! This crate contains the protocol-agnostic parts of Nika:
//! - Source spans and file registry
//! - Raw AST (YAML → RawWorkflow)
//! - Analyzed AST (RawWorkflow → AnalyzedWorkflow)
//! - Binding types and transforms
//! - Static catalogs (providers, models, MCP aliases)
//!
//! It does NOT contain:
//! - Runtime execution (tokio, reqwest, rig-core)
//! - Media pipeline (image, zstd, c2pa)
//! - MCP client (rmcp)
//! - TUI (ratatui)
//! - LSP server (tower-lsp)

pub use serde_saphyr as serde_yaml;

pub mod ast;
pub mod binding;
pub mod catalogs;
pub mod error;
pub mod error_codes;
pub mod mcp;
pub mod provider_name;
pub mod schema;
pub mod source;
pub mod trust;

pub use provider_name::ProviderName;
