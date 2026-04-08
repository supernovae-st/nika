// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Nika Language Server Implementation
//!
//! Core LSP server that integrates with the Two-Phase IR for parsing and validation.

#[cfg(feature = "lsp")]
use std::sync::Arc;

#[cfg(feature = "lsp")]
use tokio::sync::RwLock;

#[cfg(feature = "lsp")]
use tower_lsp_server::jsonrpc::Result;
#[cfg(feature = "lsp")]
use tower_lsp_server::ls_types::*;
#[cfg(feature = "lsp")]
use tower_lsp_server::{Client, LanguageServer};

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
    /// Delegated handler from nika-lsp-core (fallback when AST unavailable)
    core_handler: nika_lsp_core::handler::DefaultHandler,
}

#[cfg(feature = "lsp")]
impl NikaLanguageServer {
    /// Create a new language server instance
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(DocumentStore::new())),
            ast_index: AstIndex::new(),
            core_handler: nika_lsp_core::handler::DefaultHandler::new(),
        }
    }

    /// Parse and analyze a document, publishing diagnostics
    ///
    /// Uses AstIndex for caching and analysis. The AST is cached for
    /// subsequent hover, completion, and definition requests.
    async fn analyze_document(&self, uri: &Uri, text: &str) {
        // Use AstIndex to parse and cache the document
        // This handles both Phase 1 (parse) and Phase 2 (analyze)
        let errors = self.ast_index.parse_document(uri, text, 0);

        // Convert errors to LSP diagnostics
        let mut diagnostics = self.errors_to_diagnostics(&errors, text);

        // Check for parse errors (Phase 1 failures)
        if let Some(parse_error) = self.ast_index.get_parse_error(uri) {
            diagnostics.push(self.parse_error_to_diagnostic(&parse_error, text));
        }

        // Check model compatibility (after Phase 2 analysis)
        diagnostics.extend(self.model_compatibility_diagnostics(uri, text));

        // Add tree-sitter recovery diagnostics for syntax errors
        let partial = nika_lsp_core::parse::parse_and_extract(text);
        if partial.has_errors {
            for err_range in &partial.error_ranges {
                let start = super::conversion::offset_to_position(err_range.start as usize, text);
                let end = super::conversion::offset_to_position(err_range.end as usize, text);
                diagnostics.push(Diagnostic {
                    range: Range { start, end },
                    severity: Some(DiagnosticSeverity::WARNING),
                    code: Some(NumberOrString::String("NIKA-SYNTAX".to_string())),
                    code_description: None,
                    source: Some("nika".to_string()),
                    message: "YAML syntax error — completions may be incomplete in this region"
                        .to_string(),
                    related_information: None,
                    tags: None,
                    data: None,
                });
            }
        }

        // Publish diagnostics
        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
    }

    /// Convert analysis errors to LSP diagnostics.
    ///
    /// Includes "did you mean?" suggestions from the analyzer when available
    /// (e.g., NIKA-140 unknown task references with fuzzy match).
    fn errors_to_diagnostics(&self, errors: &[AnalyzeError], source: &str) -> Vec<Diagnostic> {
        errors
            .iter()
            .map(|e| {
                let message = if let Some(ref suggestion) = e.suggestion {
                    format!("{} ({})", e.message, suggestion)
                } else {
                    e.message.clone()
                };
                Diagnostic {
                    range: span_to_range(&e.span, source),
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String(e.kind.code().to_string())),
                    code_description: None,
                    source: Some("nika".to_string()),
                    message,
                    related_information: None,
                    tags: None,
                    data: None,
                }
            })
            .collect()
    }

    /// Convert a parse error to an LSP diagnostic
    fn parse_error_to_diagnostic(&self, error: &raw::ParseError, source: &str) -> Diagnostic {
        Diagnostic {
            range: span_to_range(&error.span, source),
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String(error.kind.code().to_string())),
            code_description: None,
            source: Some("nika".to_string()),
            message: error.message.clone(),
            related_information: None,
            tags: None,
            data: None,
        }
    }

    /// Check each task's model against the ModelCatalog for compatibility issues.
    fn model_compatibility_diagnostics(&self, uri: &Uri, source: &str) -> Vec<Diagnostic> {
        check_model_compatibility(&self.ast_index, uri, source)
    }
}

/// Check each task's model against the ModelCatalog for compatibility issues.
///
/// Produces WARNING diagnostics for deprecated models (NIKA-033) and ERROR
/// diagnostics for capability mismatches (NIKA-032), e.g. extended_thinking
/// with a non-Claude model.
///
/// Extracted as a free function for testability (no `NikaLanguageServer` needed).
#[cfg(feature = "lsp")]
fn check_model_compatibility(ast_index: &AstIndex, uri: &Uri, source: &str) -> Vec<Diagnostic> {
    use crate::ast::analyzed::{AnalyzedTaskAction, OutputFormat};
    use crate::lsp::model_intel::{self, IssueSeverity, TaskModelConfig};

    let cached = match ast_index.get(uri) {
        Some(c) => c,
        None => return Vec::new(),
    };

    let analyzed = match cached.analyzed.as_ref() {
        Some(a) => a,
        None => return Vec::new(),
    };

    let mut diagnostics = Vec::new();

    for task in &analyzed.tasks {
        // Build TaskModelConfig from the analyzed task
        let model_id = task.model.clone();
        if model_id.is_none() {
            continue; // No model specified, nothing to check
        }

        let (extended_thinking, tool_choice_required) = match &task.action {
            AnalyzedTaskAction::Infer(infer) => (
                infer
                    .extended_thinking
                    .as_ref()
                    .and_then(|t| t.value())
                    .unwrap_or(false),
                false,
            ),
            AnalyzedTaskAction::Agent(agent) => {
                let et = agent
                    .extended_thinking
                    .as_ref()
                    .and_then(|t| t.value())
                    .unwrap_or(false);
                let tc = agent
                    .tool_choice
                    .as_deref()
                    .is_some_and(|v| v == "required");
                (et, tc)
            }
            _ => (false, false),
        };

        let json_output = task
            .output
            .as_ref()
            .is_some_and(|o| o.format == OutputFormat::Json);

        let config = TaskModelConfig {
            model_id,
            provider: task.provider.as_ref().map(|p| p.to_string()),
            extended_thinking,
            json_output,
            tool_choice_required,
        };

        let issues = model_intel::check_compatibility(&config);
        for issue in issues {
            let severity = match issue.severity {
                IssueSeverity::Error => DiagnosticSeverity::ERROR,
                IssueSeverity::Warning => DiagnosticSeverity::WARNING,
            };

            // Use the task span for positioning
            let range = span_to_range(&task.span, source);

            diagnostics.push(Diagnostic {
                range,
                severity: Some(severity),
                code: Some(NumberOrString::String(issue.code.to_string())),
                code_description: None,
                source: Some("nika".to_string()),
                message: issue.message,
                related_information: None,
                tags: if issue.code == "NIKA-033" {
                    Some(vec![DiagnosticTag::DEPRECATED])
                } else {
                    None
                },
                data: None,
            });
        }
    }

    diagnostics
}

#[cfg(feature = "lsp")]
impl LanguageServer for NikaLanguageServer {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: server_capabilities(),
            server_info: Some(ServerInfo {
                name: "nika-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            offset_encoding: None,
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

        // NOTE: No debounce needed here. `.nika.yaml` files are typically under
        // 500 lines -- full reparse + analysis via AstIndex is sub-millisecond,
        // so running on every keystroke gives instant diagnostics without
        // measurable overhead. If workflows ever grow to thousands of lines,
        // consider debouncing (record Instant per URI, skip analysis if <150ms
        // since last edit, trigger on did_save or next request instead).

        // Check document exists (read lock, no await under lock)
        let is_known = {
            let docs = self.documents.read().await;
            docs.contains(&uri)
        };
        if !is_known {
            self.client
                .log_message(
                    MessageType::WARNING,
                    format!("Received did_change for unopened document: {:?}", uri),
                )
                .await;
            return;
        }

        // Apply incremental changes (write lock, no await under lock)
        let text = {
            let mut docs = self.documents.write().await;
            for change in params.content_changes {
                docs.apply_change(&uri, change);
            }
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

        // Use nika-lsp-core for unified completions
        let offset = super::conversion::position_to_offset(position, &text) as u32;
        let context = nika_lsp_core::analysis::context::detect_context_with_recovery(&text, offset);
        let items = nika_lsp_core::handlers::completion::completions(&text, offset, &context, None);

        if !items.is_empty() {
            return Ok(Some(CompletionResponse::Array(items)));
        }

        // Fallback to existing AST-aware completion for contexts
        // nika-lsp-core doesn't cover yet (e.g. MCP discovery)
        let completions = handlers::completion::compute_completions_with_ast(
            &self.ast_index,
            uri,
            &text,
            position,
        );
        Ok(Some(CompletionResponse::Array(completions)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let text = docs.get(uri).cloned().unwrap_or_default();

        // Use AST-aware hover for semantic context
        if let Some(hover) =
            handlers::hover::compute_hover_with_ast(&self.ast_index, uri, &text, position)
        {
            return Ok(Some(hover));
        }

        // Fallback: use nika-lsp-core handler with CursorContext
        let offset = super::conversion::position_to_offset(position, &text) as u32;
        let context = nika_lsp_core::analysis::context::detect_context_with_recovery(&text, offset);
        if let Some(result) =
            nika_lsp_core::handler::LspHandler::hover(&self.core_handler, &text, offset, &context)
        {
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: result.contents,
                }),
                range: None,
            }));
        }

        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let text = docs.get(uri).cloned().unwrap_or_default();

        // Use AST-aware definition lookup for semantic precision
        if let Some(result) =
            handlers::definition::find_definition_with_ast(&self.ast_index, uri, &text, position)
        {
            return Ok(Some(result));
        }

        // Fallback: use nika-lsp-core definition handler
        let offset = super::conversion::position_to_offset(position, &text) as u32;
        let context = nika_lsp_core::analysis::context::detect_context_with_recovery(&text, offset);
        if let Some(def) = nika_lsp_core::handler::LspHandler::definition(
            &self.core_handler,
            &text,
            offset,
            &context,
        ) {
            let start = super::conversion::offset_to_position(def.offset as usize, &text);
            let end = super::conversion::offset_to_position(def.end_offset as usize, &text);
            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri: uri.clone(),
                range: Range { start, end },
            })));
        }

        Ok(None)
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = &params.text_document.uri;
        let range = params.range;
        let diagnostics = &params.context.diagnostics;

        let docs = self.documents.read().await;
        let text = docs.get(uri).cloned().unwrap_or_default();

        // Use AST-aware code actions for semantic fixes (fuzzy task matching)
        let mut actions = handlers::code_action::compute_code_actions_with_ast(
            &self.ast_index,
            uri,
            &text,
            range,
            diagnostics,
        );

        // Merge core code actions (schema upgrade, expand shorthands, add provider)
        let start_offset = super::conversion::position_to_offset(range.start, &text) as u32;
        let end_offset = super::conversion::position_to_offset(range.end, &text) as u32;
        let core_actions = nika_lsp_core::handler::LspHandler::code_actions(
            &self.core_handler,
            &text,
            start_offset,
            end_offset,
        );
        for entry in core_actions {
            if let Some(edit) = entry.edit {
                let edit_start = super::conversion::offset_to_position(edit.offset as usize, &text);
                let edit_end =
                    super::conversion::offset_to_position(edit.end_offset as usize, &text);
                let kind = match entry.kind {
                    nika_lsp_core::handlers::code_action::CodeActionKind::QuickFix => {
                        CodeActionKind::QUICKFIX
                    }
                    nika_lsp_core::handlers::code_action::CodeActionKind::Refactor => {
                        CodeActionKind::REFACTOR
                    }
                };
                let mut changes = std::collections::HashMap::new();
                changes.insert(
                    uri.clone(),
                    vec![TextEdit {
                        range: Range {
                            start: edit_start,
                            end: edit_end,
                        },
                        new_text: edit.new_text,
                    }],
                );
                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: entry.title,
                    kind: Some(kind),
                    is_preferred: Some(entry.is_preferred),
                    edit: Some(WorkspaceEdit {
                        changes: Some(changes),
                        ..Default::default()
                    }),
                    ..Default::default()
                }));
            }
        }

        if actions.is_empty() {
            return Ok(None);
        }
        Ok(Some(actions))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;

        let docs = self.documents.read().await;
        let text = docs.get(uri).cloned().unwrap_or_default();

        // Use AST-aware symbols for hierarchical structure (tasks contain verbs)
        let symbols =
            handlers::symbols::compute_document_symbols_with_ast(&self.ast_index, uri, &text);
        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        tracing::info!("Configuration changed: {:?}", params.settings);

        // Re-analyze all open documents and re-publish diagnostics
        let docs = self.documents.read().await;
        let uris: Vec<_> = docs.uris().cloned().collect();
        let texts: Vec<_> = uris
            .iter()
            .filter_map(|uri| docs.get(uri).map(|t| (uri.clone(), t.clone())))
            .collect();
        drop(docs);

        for (uri, text) in &texts {
            self.analyze_document(uri, text).await;
        }

        self.client
            .log_message(MessageType::INFO, "Configuration updated")
            .await;
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = &params.text_document.uri;

        let docs = self.documents.read().await;
        let text = docs.get(uri).cloned().unwrap_or_default();

        let raw_tokens = handlers::semantic_tokens::compute_semantic_tokens_with_ast(
            &self.ast_index,
            uri,
            &text,
        );
        let encoded = handlers::semantic_tokens::encode_tokens(raw_tokens);

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: encoded,
        })))
    }

    // ===== Inlay Hints =====

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = &params.text_document.uri;

        let docs = self.documents.read().await;
        let text = docs.get(uri).cloned().unwrap_or_default();

        let hints = handlers::inlay_hints::compute_inlay_hints(&text, params.range);
        if hints.is_empty() {
            Ok(None)
        } else {
            Ok(Some(hints))
        }
    }

    // ===== Folding Ranges (delegated to nika-lsp-core) =====

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let uri = &params.text_document.uri;

        let docs = self.documents.read().await;
        let text = docs.get(uri).cloned().unwrap_or_default();

        let entries = nika_lsp_core::handlers::folding_ranges::folding_ranges(&text);
        if entries.is_empty() {
            return Ok(None);
        }

        let ranges: Vec<FoldingRange> = entries
            .into_iter()
            .map(|e| FoldingRange {
                start_line: e.start_line,
                start_character: None,
                end_line: e.end_line,
                end_character: None,
                kind: Some(match e.kind {
                    nika_lsp_core::handlers::folding_ranges::FoldKind::Region => {
                        FoldingRangeKind::Region
                    }
                    nika_lsp_core::handlers::folding_ranges::FoldKind::Comment => {
                        FoldingRangeKind::Comment
                    }
                }),
                collapsed_text: None,
            })
            .collect();
        Ok(Some(ranges))
    }

    // ===== Code Lens =====

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = &params.text_document.uri;

        let docs = self.documents.read().await;
        let text = docs.get(uri).cloned().unwrap_or_default();

        let lenses = handlers::code_lens::compute_code_lenses(&text, uri);
        if lenses.is_empty() {
            Ok(None)
        } else {
            Ok(Some(lenses))
        }
    }

    // ===== References (delegated to nika-lsp-core) =====

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let docs = self.documents.read().await;
        let text = docs.get(uri).cloned().unwrap_or_default();

        // Convert position to byte offset for lsp-core
        let offset = position_to_offset(&text, position);
        let task_id = match nika_lsp_core::handlers::references::find_task_at_offset(&text, offset)
        {
            Some(id) => id,
            None => return Ok(None),
        };

        let entries = nika_lsp_core::handlers::references::find_task_references(&text, &task_id);
        if entries.is_empty() {
            return Ok(None);
        }

        let locations: Vec<Location> = entries
            .into_iter()
            .map(|e| Location {
                uri: uri.clone(),
                range: offset_range_to_lsp(&text, e.start_offset, e.end_offset),
            })
            .collect();
        Ok(Some(locations))
    }

    // ===== Document Links (delegated to nika-lsp-core) =====

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let uri = &params.text_document.uri;

        let docs = self.documents.read().await;
        let text = docs.get(uri).cloned().unwrap_or_default();

        let entries = nika_lsp_core::handlers::document_links::document_links(&text);
        if entries.is_empty() {
            return Ok(None);
        }

        let links: Vec<DocumentLink> = entries
            .into_iter()
            .map(|e| DocumentLink {
                range: offset_range_to_lsp(&text, e.start_offset, e.end_offset),
                target: e.target.parse().ok(),
                tooltip: Some(e.tooltip),
                data: None,
            })
            .collect();
        Ok(Some(links))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// OFFSET / POSITION CONVERSION HELPERS
// ═══════════════════════════════════════════════════════════════════════════

/// Convert a line/column Position to a byte offset in the text.
#[cfg(feature = "lsp")]
fn position_to_offset(text: &str, position: Position) -> u32 {
    let mut offset = 0u32;
    for (i, line) in text.lines().enumerate() {
        if i == position.line as usize {
            return offset + position.character.min(line.len() as u32);
        }
        offset += line.len() as u32 + 1; // +1 for newline
    }
    offset
}

/// Convert a byte offset range to an LSP Range (line/column).
#[cfg(feature = "lsp")]
fn offset_range_to_lsp(text: &str, start: u32, end: u32) -> Range {
    let start_pos = offset_to_position(text, start);
    let end_pos = offset_to_position(text, end);
    Range {
        start: start_pos,
        end: end_pos,
    }
}

#[cfg(feature = "lsp")]
fn offset_to_position(text: &str, offset: u32) -> Position {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in text.char_indices() {
        if i as u32 >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    Position::new(line, col)
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

    #[test]
    #[cfg(feature = "lsp")]
    fn test_deprecated_model_emits_warning_diagnostic() {
        use super::*;
        use tower_lsp_server::ls_types::Uri;

        let ast_index = AstIndex::new();
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let text = r#"schema: nika/workflow@0.12
workflow: test

tasks:
  - id: step1
    model: gpt-4-turbo
    infer: "Hello"
"#;

        ast_index.parse_document(&uri, text, 1);
        let diags = check_model_compatibility(&ast_index, &uri, text);

        assert!(
            !diags.is_empty(),
            "Should emit diagnostics for deprecated model"
        );
        assert!(
            diags
                .iter()
                .any(|d| d.severity == Some(DiagnosticSeverity::WARNING)
                    && d.code == Some(NumberOrString::String("NIKA-033".to_string()))),
            "Should have NIKA-033 warning for deprecated model: {:?}",
            diags
        );
        // Check deprecated tag
        let deprecated_diag = diags
            .iter()
            .find(|d| d.code == Some(NumberOrString::String("NIKA-033".to_string())))
            .unwrap();
        assert!(
            deprecated_diag
                .tags
                .as_ref()
                .is_some_and(|t| t.contains(&DiagnosticTag::DEPRECATED)),
            "NIKA-033 should have DEPRECATED tag"
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_extended_thinking_with_non_claude_emits_error() {
        use super::*;
        use tower_lsp_server::ls_types::Uri;

        let ast_index = AstIndex::new();
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let text = r#"schema: nika/workflow@0.12
workflow: test

tasks:
  - id: step1
    model: gpt-4o
    infer:
      prompt: "Hello"
      extended_thinking: true
"#;

        ast_index.parse_document(&uri, text, 1);
        let diags = check_model_compatibility(&ast_index, &uri, text);

        assert!(
            diags
                .iter()
                .any(|d| d.severity == Some(DiagnosticSeverity::ERROR)
                    && d.code == Some(NumberOrString::String("NIKA-032".to_string()))),
            "Should have NIKA-032 error for extended_thinking with non-Claude model: {:?}",
            diags
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_extended_thinking_with_claude_no_error() {
        use super::*;
        use tower_lsp_server::ls_types::Uri;

        let ast_index = AstIndex::new();
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let text = r#"schema: nika/workflow@0.12
workflow: test

tasks:
  - id: step1
    model: claude-sonnet-4-6
    infer:
      prompt: "Hello"
      extended_thinking: true
"#;

        ast_index.parse_document(&uri, text, 1);
        let diags = check_model_compatibility(&ast_index, &uri, text);

        assert!(
            !diags.iter().any(
                |d| d.code == Some(NumberOrString::String("NIKA-032".to_string()))
                    && d.severity == Some(DiagnosticSeverity::ERROR)
            ),
            "Should NOT have NIKA-032 error for Claude with extended_thinking"
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_active_model_no_deprecation_diagnostic() {
        use super::*;
        use tower_lsp_server::ls_types::Uri;

        let ast_index = AstIndex::new();
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let text = r#"schema: nika/workflow@0.12
workflow: test

tasks:
  - id: step1
    model: gpt-4o
    infer: "Hello"
"#;

        ast_index.parse_document(&uri, text, 1);
        let diags = check_model_compatibility(&ast_index, &uri, text);

        assert!(
            !diags
                .iter()
                .any(|d| d.code == Some(NumberOrString::String("NIKA-033".to_string()))),
            "Should NOT emit NIKA-033 for active model"
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_no_model_no_diagnostics() {
        use super::*;
        use tower_lsp_server::ls_types::Uri;

        let ast_index = AstIndex::new();
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let text = r#"schema: nika/workflow@0.12
workflow: test

tasks:
  - id: step1
    infer: "Hello"
"#;

        ast_index.parse_document(&uri, text, 1);
        let diags = check_model_compatibility(&ast_index, &uri, text);

        assert!(
            diags.is_empty(),
            "Should emit no model diagnostics when no model specified"
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_unknown_model_no_diagnostics() {
        use super::*;
        use tower_lsp_server::ls_types::Uri;

        let ast_index = AstIndex::new();
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let text = r#"schema: nika/workflow@0.12
workflow: test

tasks:
  - id: step1
    model: some-custom-model
    infer: "Hello"
"#;

        ast_index.parse_document(&uri, text, 1);
        let diags = check_model_compatibility(&ast_index, &uri, text);

        assert!(
            diags.is_empty(),
            "Should emit no diagnostics for unknown/custom model"
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_unparseable_document_no_crash() {
        use super::*;
        use tower_lsp_server::ls_types::Uri;

        let ast_index = AstIndex::new();
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let text = "this is not valid yaml: [[[";

        ast_index.parse_document(&uri, text, 1);
        let diags = check_model_compatibility(&ast_index, &uri, text);

        // Should not crash, just return empty
        let _ = diags;
    }
}
