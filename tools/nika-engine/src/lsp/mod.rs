// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Language Server Protocol support for Nika workflows
//!
//! This module implements LSP for `.nika.yaml` files, providing:
//! - Real-time diagnostics via Two-Phase IR (RawWorkflow → AnalyzedWorkflow)
//! - Schema-aware completions for verbs, fields, and bindings
//! - Hover documentation for task definitions
//! - Go-to-definition for task references and bindings
//! - Code actions for common fixes
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  VS Code / Editor                                               │
//! │       │                                                         │
//! │       │ LSP Protocol (JSON-RPC over stdio)                      │
//! │       ▼                                                         │
//! │  ┌─────────────────────────────────────────────────────────┐   │
//! │  │  NikaLanguageServer                                      │   │
//! │  │  ├── did_open/did_change → parse + validate + diagnostics│   │
//! │  │  ├── completion → schema-aware suggestions               │   │
//! │  │  ├── hover → documentation on hover                      │   │
//! │  │  └── goto_definition → jump to task/binding              │   │
//! │  └─────────────────────────────────────────────────────────┘   │
//! │       │                                                         │
//! │       ▼                                                         │
//! │  ┌─────────────────────────────────────────────────────────┐   │
//! │  │  Two-Phase IR (ast module)                               │   │
//! │  │  ├── raw::parse() → RawWorkflow with Spans               │   │
//! │  │  └── analyzer::analyze() → AnalyzedWorkflow + Errors     │   │
//! │  └─────────────────────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```bash
//! # Start LSP server (stdio transport)
//! nika lsp
//!
//! # VS Code settings.json
//! {
//!   "nika.lsp.path": "/path/to/nika"
//! }
//! ```

mod ast_index;
mod capabilities;
mod conversion;
mod document_store;
pub mod handlers;
pub mod model_intel;
mod server;

pub use ast_index::{AstIndex, AstNode, CachedAst};

pub use capabilities::server_capabilities;
pub use conversion::{offset_to_position, position_to_offset, span_to_range};
pub use document_store::DocumentStore;
pub use server::NikaLanguageServer;

/// Start the LSP server with stdio transport
///
/// This is the main entry point called by `nika lsp`.
#[cfg(feature = "lsp")]
pub async fn run_stdio() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tower_lsp_server::{LspService, Server};

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(NikaLanguageServer::new);

    Server::new(stdin, stdout, socket).serve(service).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Verify public API is accessible
        let _ = server_capabilities();
    }
}
