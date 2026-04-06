// LSP depends on unix-only daemon features; suppress unused warnings on Windows CI.
#![cfg_attr(not(unix), allow(unused_variables, unused_imports))]

//! Nika Language Server
//!
//! LSP server for Nika workflow files (.nika.yaml).
//!
//! # Features
//!
//! - Real-time diagnostics (red squiggly lines)
//! - Intelligent completion (verbs, params, task IDs, MCP tools)
//! - Hover documentation
//! - Go to definition
//!
//! # Usage
//!
//! ```bash
//! # Run via stdio (for VS Code extension)
//! nika-lsp
//!
//! # Run with debug logging
//! RUST_LOG=debug nika-lsp
//! ```

mod ast_integration;
mod backend;
mod completion;
#[cfg(unix)]
mod daemon_bridge;
mod diagnostics;
mod document;
mod mcp_discovery;
mod position;
mod template_validation;

use tower_lsp_server::{LspService, Server};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    // Initialize logging to stderr (LSP uses stdout for protocol)
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    tracing::info!("Starting Nika LSP server v{}", env!("CARGO_PKG_VERSION"));

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::build(backend::NikaBackend::new)
        .custom_method("nika/daemonStatus", backend::NikaBackend::daemon_status)
        .custom_method("nika/workflowGraph", backend::NikaBackend::workflow_graph)
        .finish();

    Server::new(stdin, stdout, socket).serve(service).await;
}
