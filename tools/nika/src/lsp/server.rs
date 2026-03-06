//! Nika Language Server Implementation
//!
//! Core LSP server that integrates with the Two-Phase IR for parsing and validation.

#[cfg(feature = "lsp")]
use std::sync::Arc;

#[cfg(feature = "lsp")]
use tokio::sync::RwLock;

#[cfg(feature = "lsp")]
use tower_lsp::jsonrpc::Result;
#[cfg(feature = "lsp")]
use tower_lsp::lsp_types::*;
#[cfg(feature = "lsp")]
use tower_lsp::{Client, LanguageServer};

#[cfg(feature = "lsp")]
use crate::ast::analyzer::AnalyzeError;
#[cfg(feature = "lsp")]
use crate::ast::raw;

#[cfg(feature = "lsp")]
use super::ast_index::AstIndex;
#[cfg(feature = "lsp")]
use super::capabilities::server_capabilities;
#[cfg(feature = "lsp")]
use super::conversion::span_to_range;
#[cfg(feature = "lsp")]
use super::document_store::DocumentStore;
#[cfg(feature = "lsp")]
use super::handlers;

/// The Nika Language Server
///
/// Provides LSP support for `.nika.yaml` workflow files using the Two-Phase IR:
/// 1. Parse YAML to RawWorkflow (with spans for precise error locations)
/// 2. Analyze to AnalyzedWorkflow (semantic validation)
/// 3. Convert errors to LSP Diagnostics
#[cfg(feature = "lsp")]
pub struct NikaLanguageServer {
    /// LSP client for sending notifications (diagnostics, logs)
    client: Client,
    /// In-memory store of open documents
    documents: Arc<RwLock<DocumentStore>>,
    /// AST index for position-aware lookups (Phase 2)
    ast_index: AstIndex,
}

#[cfg(feature = "lsp")]
impl NikaLanguageServer {
    /// Create a new language server instance
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(DocumentStore::new())),
            ast_index: AstIndex::new(),
        }
    }

    /// Parse and analyze a document, publishing diagnostics
    ///
    /// Uses AstIndex for caching and analysis. The AST is cached for
    /// subsequent hover, completion, and definition requests.
    async fn analyze_document(&self, uri: &Url, text: &str) {
        // Use AstIndex to parse and cache the document
        // This handles both Phase 1 (parse) and Phase 2 (analyze)
        let errors = self.ast_index.parse_document(uri, text, 0);

        // Convert errors to LSP diagnostics
        let mut diagnostics = self.errors_to_diagnostics(&errors, text);

        // Check for parse errors (Phase 1 failures)
        if let Some(parse_error) = self.ast_index.get_parse_error(uri) {
            diagnostics.push(self.parse_error_to_diagnostic(&parse_error, text));
        }

        // Publish diagnostics
        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
    }

    /// Convert analysis errors to LSP diagnostics
    fn errors_to_diagnostics(&self, errors: &[AnalyzeError], source: &str) -> Vec<Diagnostic> {
        errors
            .iter()
            .map(|e| Diagnostic {
                range: span_to_range(&e.span, source),
                severity: Some(DiagnosticSeverity::ERROR), // All analysis errors are errors
                code: Some(NumberOrString::String(e.kind.code().to_string())),
                code_description: None,
                source: Some("nika".to_string()),
                message: e.message.clone(),
                related_information: None,
                tags: None,
                data: None,
            })
            .collect()
    }

    /// Convert a parse error to an LSP diagnostic
    fn parse_error_to_diagnostic(&self, error: &raw::ParseError, source: &str) -> Diagnostic {
        Diagnostic {
            range: span_to_range(&error.span, source),
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String("NIKA-001".to_string())),
            code_description: None,
            source: Some("nika".to_string()),
            message: error.message.clone(),
            related_information: None,
            tags: None,
            data: None,
        }
    }
}

#[cfg(feature = "lsp")]
#[tower_lsp::async_trait]
impl LanguageServer for NikaLanguageServer {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: server_capabilities(),
            server_info: Some(ServerInfo {
                name: "nika-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Nika language server initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;

        // Store document
        {
            let mut docs = self.documents.write().await;
            docs.insert(uri.clone(), text.clone());
        }

        // Analyze and publish diagnostics
        self.analyze_document(&uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;

        // Apply incremental changes
        let text = {
            let mut docs = self.documents.write().await;

            // Check if document was opened first
            if !docs.contains(&uri) {
                // Log warning but still process changes - could be a race condition
                self.client
                    .log_message(
                        MessageType::WARNING,
                        format!("Received did_change for unopened document: {}", uri),
                    )
                    .await;
                return;
            }

            for change in params.content_changes {
                docs.apply_change(&uri, change);
            }
            // Clone is necessary here as we need the text outside the lock
            docs.get(&uri).cloned().unwrap_or_default()
        };

        // Re-analyze
        self.analyze_document(&uri, &text).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        // Re-analyze on save (if text is included)
        if let Some(text) = params.text {
            self.analyze_document(&params.text_document.uri, &text)
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;

        // Remove from store
        {
            let mut docs = self.documents.write().await;
            docs.remove(&uri);
        }

        // Clear diagnostics
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let docs = self.documents.read().await;
        let text = docs.get(uri).cloned().unwrap_or_default();

        let completions = handlers::completion::compute_completions(&text, position);
        Ok(Some(CompletionResponse::Array(completions)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let text = docs.get(uri).cloned().unwrap_or_default();

        // Use AST-aware hover for semantic context
        Ok(handlers::hover::compute_hover_with_ast(
            &self.ast_index,
            uri,
            &text,
            position,
        ))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let text = docs.get(uri).cloned().unwrap_or_default();

        Ok(handlers::definition::find_definition(
            &text,
            position,
            uri.clone(),
        ))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = &params.text_document.uri;
        let range = params.range;
        let diagnostics = &params.context.diagnostics;

        let docs = self.documents.read().await;
        let text = docs.get(uri).cloned().unwrap_or_default();

        let actions =
            handlers::code_action::compute_code_actions(&text, range, diagnostics, uri.clone());
        Ok(Some(actions))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;

        let docs = self.documents.read().await;
        let text = docs.get(uri).cloned().unwrap_or_default();

        let symbols = handlers::symbols::compute_document_symbols(&text);
        Ok(Some(DocumentSymbolResponse::Flat(symbols)))
    }
}

// Stub when LSP feature is disabled
#[cfg(not(feature = "lsp"))]
pub struct NikaLanguageServer;

#[cfg(not(feature = "lsp"))]
impl NikaLanguageServer {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_server_stub_compiles() {
        // Just verify the module compiles without lsp feature
    }
}
