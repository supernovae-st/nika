//! LSP Request Handlers
//!
//! Individual handlers for each LSP request type.

pub mod code_action;
pub mod code_lens;
pub mod completion;
pub mod definition;
// document_links: migrated to nika-lsp-core
// folding_ranges: migrated to nika-lsp-core
pub mod hover;
pub mod inlay_hints;
// references: migrated to nika-lsp-core
pub mod semantic_tokens;
pub mod symbols;
