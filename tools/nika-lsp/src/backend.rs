//! LSP Backend implementation for Nika.
//!
//! This is the main LSP server that handles all protocol messages and
//! orchestrates validation, completion, and hover providers.
//!
//! ## AST Integration
//!
//! This module uses Nika's two-phase AST for accurate parsing:
//! - Task ID extraction via `ast_integration::extract_task_ids_from_ast()`
//! - Go-to-definition via `ast_integration::find_task_by_id()`
//! - Context detection via `node_context::find_context_at_position()`

use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{mpsc, RwLock};
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer};

use crate::ast_integration;
use crate::completion::{get_completion_context, CompletionContext};
use async_trait::async_trait;
#[cfg(unix)]
use crate::daemon_bridge::DaemonBridge;
use nika_core::catalogs::{CostEstimate, DaemonCapabilities, ProviderStatusInfo, WorkflowRunInfo};
use nika_daemon::provider::DaemonProvider;
use crate::diagnostics::validate_document;
use crate::document::DocumentState;
use nika_lsp_core::handler::{DefaultHandler, LspHandler};
use nika_lsp_core::handlers::code_action::DiagnosticInfo;

/// Request to validate a document.
pub struct ValidationRequest {
    pub uri: Uri,
    pub content: String,
    pub version: i32,
}

/// Notification that a document parsed successfully (for AST cache).
struct ParseSuccessNotification {
    uri: Uri,
}

/// Adapter: delegates to a DaemonBridge behind an RwLock (supports reconnection).
#[cfg(unix)]
struct RwLockDaemonProvider(Arc<RwLock<Option<DaemonBridge>>>);

#[cfg(unix)]
#[async_trait]
impl DaemonProvider for RwLockDaemonProvider {
    fn is_connected(&self) -> bool {
        // Non-blocking check: try_read to avoid deadlock in sync context
        match self.0.try_read() {
            Ok(guard) => guard.as_ref().is_some_and(|b| b.is_connected()),
            Err(_) => false,
        }
    }

    async fn provider_status(&self) -> Vec<ProviderStatusInfo> {
        let guard = self.0.read().await;
        match guard.as_ref() {
            Some(bridge) if bridge.is_connected() => bridge.provider_status().await,
            _ => Vec::new(),
        }
    }

    async fn estimate_cost(
        &self,
        provider: &str,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> Option<CostEstimate> {
        let guard = self.0.read().await;
        match guard.as_ref() {
            Some(bridge) if bridge.is_connected() => {
                bridge.estimate_cost(provider, model, input_tokens, output_tokens).await
            }
            _ => None,
        }
    }

    async fn workflow_history(&self, workflow: &str) -> Vec<WorkflowRunInfo> {
        let guard = self.0.read().await;
        match guard.as_ref() {
            Some(bridge) if bridge.is_connected() => bridge.workflow_history(workflow).await,
            _ => Vec::new(),
        }
    }

    async fn capabilities(&self) -> Option<DaemonCapabilities> {
        let guard = self.0.read().await;
        match guard.as_ref() {
            Some(bridge) if bridge.is_connected() => bridge.capabilities().await,
            _ => None,
        }
    }
}

/// The Nika LSP backend.
pub struct NikaBackend {
    /// LSP client for sending notifications
    client: Client,
    /// Open documents
    documents: DashMap<Uri, DocumentState>,
    /// Validation request channel
    validation_tx: mpsc::Sender<ValidationRequest>,
    /// Delegated handler from nika-lsp-core
    handler: DefaultHandler,
    /// Daemon provider (IPC to standalone daemon, or disconnected stub)
    daemon: Arc<dyn DaemonProvider>,
    /// Channel for parse success notifications (AST cache)
    #[allow(dead_code)] // Held to keep the channel alive; receiver in AST cache updater
    parse_success_tx: mpsc::Sender<ParseSuccessNotification>,
}

impl NikaBackend {
    /// Create a new backend instance.
    pub fn new(client: Client) -> Self {
        let (tx, rx) = mpsc::channel(100);
        let (parse_tx, parse_rx) = mpsc::channel::<ParseSuccessNotification>(32);

        // Daemon provider: try to connect, fall back to disconnected stub.
        // On Unix: attempt IPC connection in background, reconnect on failure.
        // On non-Unix: always use embedded provider (no daemon socket support).
        #[cfg(unix)]
        let daemon: Arc<dyn DaemonProvider> = {
            let bridge_holder: Arc<RwLock<Option<DaemonBridge>>> = Arc::new(RwLock::new(None));
            let holder_clone = bridge_holder.clone();

            tokio::spawn(async move {
                if let Some(bridge) = DaemonBridge::try_connect().await {
                    *holder_clone.write().await = Some(bridge);
                }
            });
            DaemonBridge::spawn_reconnect_loop(bridge_holder.clone());

            Arc::new(RwLockDaemonProvider(bridge_holder))
        };
        #[cfg(not(unix))]
        let daemon: Arc<dyn DaemonProvider> =
            Arc::new(nika_daemon::EmbeddedDaemonProvider::new());

        let documents: DashMap<Uri, DocumentState> = DashMap::new();
        let documents_clone = documents.clone();
        let worker_parse_tx = parse_tx.clone();

        let backend = Self {
            client: client.clone(),
            documents,
            validation_tx: tx,
            handler: DefaultHandler::new(),
            daemon,
            parse_success_tx: parse_tx,
        };

        // Spawn validation worker (sends parse success for AST cache)
        tokio::spawn(validation_worker(rx, client, worker_parse_tx));

        // Spawn AST cache updater — marks documents valid on successful parse
        tokio::spawn(async move {
            let mut rx = parse_rx;
            while let Some(notif) = rx.recv().await {
                if let Some(mut doc) = documents_clone.get_mut(&notif.uri) {
                    doc.mark_valid();
                }
            }
        });

        backend
    }

    /// Create a backend with embedded daemon (no separate process needed).
    pub fn new_embedded(client: Client) -> Self {
        let (tx, rx) = mpsc::channel(100);
        let (parse_tx, parse_rx) = mpsc::channel::<ParseSuccessNotification>(32);

        let daemon: Arc<dyn DaemonProvider> =
            Arc::new(nika_daemon::EmbeddedDaemonProvider::new());

        let documents: DashMap<Uri, DocumentState> = DashMap::new();
        let documents_clone = documents.clone();
        let worker_parse_tx = parse_tx.clone();

        let backend = Self {
            client: client.clone(),
            documents,
            validation_tx: tx,
            handler: DefaultHandler::new(),
            daemon,
            parse_success_tx: parse_tx,
        };

        tokio::spawn(validation_worker(rx, client, worker_parse_tx));
        tokio::spawn(async move {
            let mut rx = parse_rx;
            while let Some(notif) = rx.recv().await {
                if let Some(mut doc) = documents_clone.get_mut(&notif.uri) {
                    doc.mark_valid();
                }
            }
        });

        backend
    }

    /// Custom request handler: nika/daemonStatus
    ///
    /// Called by the VS Code extension to display daemon status in the status bar.
    pub async fn daemon_status(&self) -> Result<serde_json::Value> {
        if !self.daemon.is_connected() {
            return Ok(serde_json::json!({ "connected": false }));
        }
        let caps = self.daemon.capabilities().await;
        Ok(serde_json::json!({
            "connected": true,
            "version": caps.as_ref().map(|c| c.version.as_str()),
            "uptime_secs": caps.as_ref().map(|c| c.uptime_secs),
            "cache_entries": caps.as_ref().map(|c| c.cache_entries),
            "cache_hit_rate": caps.as_ref().map(|c| c.cache_hit_rate),
            "active_jobs": caps.as_ref().map(|c| c.active_jobs),
            "watch_active": caps.as_ref().map(|c| c.watch_active),
        }))
    }

    /// Custom request handler: nika/workflowGraph
    ///
    /// Returns the DAG graph data for the webview panel.
    /// Parses the workflow AST and extracts nodes (tasks) and edges (dependencies).
    pub async fn workflow_graph(
        &self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let uri_str = params
            .get("uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                tower_lsp_server::jsonrpc::Error::invalid_params("missing uri")
            })?;

        let uri: Uri = uri_str.parse().map_err(|_| {
            tower_lsp_server::jsonrpc::Error::invalid_params("invalid uri")
        })?;

        let content = match self.documents.get(&uri) {
            Some(doc) => doc.content().to_string(),
            None => return Ok(serde_json::json!({ "workflowName": "", "nodes": [], "edges": [] })),
        };

        Ok(extract_workflow_graph(&content))
    }
}

/// Extract DAG graph data from workflow content (pure function for testing).
pub(crate) fn extract_workflow_graph(content: &str) -> serde_json::Value {
    use nika_core::ast::raw::RawTaskAction;

    let parsed = crate::ast_integration::parse_workflow(content);
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    if let Some(ref workflow) = parsed.workflow {
        for task in &workflow.tasks.value {
            let t = &task.value;
            let verb = match &t.action {
                Some(RawTaskAction::Infer(_)) => "infer",
                Some(RawTaskAction::Exec(_)) => "exec",
                Some(RawTaskAction::Fetch(_)) => "fetch",
                Some(RawTaskAction::Invoke(_)) => "invoke",
                Some(RawTaskAction::Agent(_)) => "agent",
                None => "unknown",
            };
            let (line, _col) = crate::ast_integration::span_to_line_col(content, &task.span);
            let dep_ids = t.depends_on_ids();

            nodes.push(serde_json::json!({
                "id": t.id.value,
                "label": t.id.value,
                "verb": verb,
                "status": "pending",
                "dependsOn": dep_ids,
                "line": line,
            }));

            for dep in &dep_ids {
                edges.push(serde_json::json!({
                    "id": format!("{}->{}", dep, t.id.value),
                    "source": dep,
                    "target": t.id.value,
                    "isDataEdge": false,
                }));
            }

            if let Some(ref with_refs) = t.with_refs {
                for (_alias, binding) in &with_refs.value {
                    let val = binding.value.as_str();
                    if let Some(ref_id) = val.strip_prefix('$') {
                        let base_id = ref_id.split('.').next().unwrap_or(ref_id);
                        if base_id != "env"
                            && base_id != "inputs"
                            && !dep_ids.contains(&base_id)
                        {
                            edges.push(serde_json::json!({
                                "id": format!("{}->{}:data", base_id, t.id.value),
                                "source": base_id,
                                "target": t.id.value,
                                "isDataEdge": true,
                            }));
                        }
                    }
                }
            }
        }
    }

    let workflow_name = parsed
        .workflow
        .as_ref()
        .and_then(|w| w.workflow.as_ref())
        .map(|s| s.value.as_str())
        .unwrap_or("workflow");

    serde_json::json!({
        "workflowName": workflow_name,
        "nodes": nodes,
        "edges": edges,
    })
}

/// Background worker that processes validation requests.
async fn validation_worker(
    mut rx: mpsc::Receiver<ValidationRequest>,
    client: Client,
    parse_success_tx: mpsc::Sender<ParseSuccessNotification>,
) {
    // Debounce: collect requests and process after a delay
    let mut pending: Option<ValidationRequest> = None;

    loop {
        tokio::select! {
            // Receive new request
            request = rx.recv() => {
                match request {
                    Some(req) => {
                        pending = Some(req);
                    }
                    None => break, // Channel closed
                }
            }
            // Process after debounce delay
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(150)), if pending.is_some() => {
                if let Some(req) = pending.take() {
                    let diagnostics = validate_document(&req.content, &req.uri, None);

                    // If no parse errors, mark document as valid for AST cache fallback
                    let has_parse_errors = diagnostics.iter().any(|d| {
                        d.severity == Some(tower_lsp_server::ls_types::DiagnosticSeverity::ERROR)
                    });
                    if !has_parse_errors {
                        let _ = parse_success_tx
                            .send(ParseSuccessNotification { uri: req.uri.clone() })
                            .await;
                    }

                    // Publish diagnostics
                    client
                        .publish_diagnostics(req.uri, diagnostics, Some(req.version))
                        .await;
                }
            }
        }
    }
}

impl LanguageServer for NikaBackend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // Document sync
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
                        save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                            include_text: Some(false),
                        })),
                        ..Default::default()
                    },
                )),
                // Completion
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ":".to_string(),
                        " ".to_string(),
                        "-".to_string(),
                        ".".to_string(),
                        "{".to_string(),
                        "@".to_string(),
                    ]),
                    resolve_provider: Some(true),
                    ..Default::default()
                }),
                // Hover
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                // Definition (future)
                definition_provider: Some(OneOf::Left(true)),
                // Semantic tokens
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types:
                                    nika_lsp_core::handlers::semantic_tokens::token_legend()
                                        .into_iter()
                                        .map(SemanticTokenType::new)
                                        .collect(),
                                token_modifiers: vec![
                                    SemanticTokenModifier::DECLARATION,
                                    SemanticTokenModifier::DEFINITION,
                                ],
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            range: None,
                            ..Default::default()
                        },
                    ),
                ),
                // Document symbols
                document_symbol_provider: Some(OneOf::Left(true)),
                // Code actions
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                // Code lens
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                // Inlay hints
                inlay_hint_provider: Some(OneOf::Left(true)),
                // References (Find All References)
                references_provider: Some(OneOf::Left(true)),
                // Rename (task ID refactoring)
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                // Document links (clickable paths)
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: Some(false),
                    work_done_progress_options: Default::default(),
                }),
                // Folding ranges
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                // Diagnostics: push model via publish_diagnostics on did_change
                // (pull-diagnostics not implemented, so no diagnostic_provider here)
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "nika-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            offset_encoding: None,
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Nika LSP server initialized!")
            .await;

        tracing::info!("Nika LSP initialized and ready");
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("Nika LSP shutting down");
        Ok(())
    }

    // ===== Document Sync =====

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let content = params.text_document.text;
        let version = params.text_document.version;

        tracing::debug!("Document opened: {:?}", uri);

        // Store document
        self.documents
            .insert(uri.clone(), DocumentState::new(content.clone(), version));

        // Request validation
        let _ = self
            .validation_tx
            .send(ValidationRequest {
                uri,
                content,
                version,
            })
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        if let Some(mut doc) = self.documents.get_mut(&uri) {
            // Apply incremental changes
            for change in params.content_changes {
                doc.apply_change(&change);
            }
            doc.version = version;

            // Request validation
            let _ = self
                .validation_tx
                .send(ValidationRequest {
                    uri: uri.clone(),
                    content: doc.content(),
                    version,
                })
                .await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        tracing::debug!("Document saved: {:?}", params.text_document.uri);
        // Validation is already triggered by did_change
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        tracing::debug!("Document closed: {:?}", uri);

        // Remove document
        self.documents.remove(&uri);

        // Clear diagnostics
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    // ===== Completion =====

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let text = {
            let doc = match self.documents.get(uri) {
                Some(d) => d,
                None => return Ok(None),
            };
            doc.content()
        };

        // Calculate byte offset from LSP Position
        let offset = crate::position::position_to_offset(&text, position.line, position.character)
            .map(|o| o.0)
            .unwrap_or(0);

        // Use nika-lsp-core for unified context detection + completions
        let context = nika_lsp_core::analysis::context::detect_context(&text, offset, None);
        // Get daemon provider status for enriched completions
        let daemon_providers = {
            let status = self.daemon.provider_status().await;
            if status.is_empty() { None } else { Some(status) }
        };
        let items = nika_lsp_core::handlers::completion::completions(
            &text,
            offset,
            &context,
            daemon_providers.as_deref(),
        );

        if items.is_empty() {
            // Fallback to legacy completion for contexts nika-lsp-core returns empty
            // (e.g. MCP discovery which requires runtime state)
            let doc = match self.documents.get(uri) {
                Some(d) => d,
                None => return Ok(None),
            };

            let legacy_context = get_completion_context(&doc, position);
            let legacy_items = match legacy_context {
                CompletionContext::McpServer => {
                    let content = doc.content();
                    let servers = crate::completion::extract_mcp_servers(&content);
                    crate::mcp_discovery::invoke_completions(&servers, None)
                }
                CompletionContext::McpTool { server } => {
                    crate::mcp_discovery::mcp_tool_completions(&server)
                }
                _ => vec![],
            };

            if legacy_items.is_empty() {
                return Ok(None);
            }
            return Ok(Some(CompletionResponse::Array(legacy_items)));
        }

        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn completion_resolve(&self, item: CompletionItem) -> Result<CompletionItem> {
        // Return item as-is (documentation already included)
        Ok(item)
    }

    // ===== Hover =====

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let doc = match self.documents.get(uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let content = doc.content();
        let offset =
            crate::position::position_to_offset(&content, position.line, position.character)
                .map(|o| o.0)
                .unwrap_or(0);
        let ctx = nika_lsp_core::analysis::context::detect_context(&content, offset, None);

        // Get workflow history from daemon for enriched hover
        let workflow_history = {
            let uri_str = uri.as_str();
            let filename = uri_str.rsplit('/').next().unwrap_or(uri_str);
            self.daemon.workflow_history(filename).await
        };
        let daemon_hover = if !workflow_history.is_empty() {
            Some(nika_lsp_core::handlers::hover::DaemonHoverData {
                workflow_history: &workflow_history,
            })
        } else {
            None
        };

        match nika_lsp_core::handlers::hover::hover(&content, offset, &ctx, daemon_hover.as_ref()) {
            Some(result) => Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: result.contents,
                }),
                range: None,
            })),
            None => Ok(None),
        }
    }

    // ===== Definition =====

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let doc = match self.documents.get(uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let content = doc.content();
        let lines: Vec<&str> = content.lines().collect();

        if position.line as usize >= lines.len() {
            return Ok(None);
        }

        let line = lines[position.line as usize];
        let col = position.character as usize;

        // Extract word at position
        let word = extract_word_at_col(line, col);

        if word.is_empty() {
            return Ok(None);
        }

        // Use AST-based task lookup for accurate definition finding
        if let Some(task_def) = ast_integration::find_task_by_id(&content, &word) {
            // Get the line content for the range
            let task_line = task_def.line as usize;
            let line_content = lines.get(task_line).unwrap_or(&"");

            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri: uri.clone(),
                range: Range {
                    start: Position {
                        line: task_def.line,
                        character: task_def.column,
                    },
                    end: Position {
                        line: task_def.line,
                        character: line_content.len() as u32,
                    },
                },
            })));
        }

        Ok(None)
    }

    // ===== Semantic Tokens (delegated to nika-lsp-core) =====

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = &params.text_document.uri;
        let text = match self.documents.get(uri) {
            Some(d) => d.content(),
            None => return Ok(None),
        };

        let raw_tokens = self.handler.semantic_tokens(&text);
        let data = nika_lsp_core::handlers::semantic_tokens::encode_tokens(&raw_tokens);
        let lsp_data: Vec<SemanticToken> = data
            .chunks_exact(5)
            .map(|c| SemanticToken {
                delta_line: c[0],
                delta_start: c[1],
                length: c[2],
                token_type: c[3],
                token_modifiers_bitset: c[4],
            })
            .collect();

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: lsp_data,
        })))
    }

    // ===== Document Symbols (delegated to nika-lsp-core) =====

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;
        let text = match self.documents.get(uri) {
            Some(d) => d.content(),
            None => return Ok(None),
        };

        let entries = self.handler.symbols(&text);
        let symbols = entries
            .into_iter()
            .map(|e| to_lsp_symbol(&text, e))
            .collect();
        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    // ===== Code Actions (delegated to nika-lsp-core) =====

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = &params.text_document.uri;
        let text = match self.documents.get(uri) {
            Some(d) => d.content(),
            None => return Ok(None),
        };

        let start = crate::position::position_to_offset(
            &text,
            params.range.start.line,
            params.range.start.character,
        )
        .map(|o| o.0)
        .unwrap_or(0);
        let end = crate::position::position_to_offset(
            &text,
            params.range.end.line,
            params.range.end.character,
        )
        .map(|o| o.0)
        .unwrap_or(0);

        // Convert LSP Diagnostics to protocol-agnostic DiagnosticInfo
        let diag_infos: Vec<DiagnosticInfo> = params
            .context
            .diagnostics
            .iter()
            .filter_map(|d| {
                let code = match &d.code {
                    Some(NumberOrString::String(s)) => s.clone(),
                    Some(NumberOrString::Number(n)) => format!("NIKA-{:03}", n),
                    None => return None,
                };
                let start_offset = crate::position::position_to_offset(
                    &text,
                    d.range.start.line,
                    d.range.start.character,
                )
                .map(|o| o.0)
                .unwrap_or(0);
                let end_offset = crate::position::position_to_offset(
                    &text,
                    d.range.end.line,
                    d.range.end.character,
                )
                .map(|o| o.0)
                .unwrap_or(0);
                Some(DiagnosticInfo {
                    code,
                    message: d.message.clone(),
                    start_offset,
                    end_offset,
                })
            })
            .collect();

        let entries = self
            .handler
            .code_actions_with_diagnostics(&text, start, end, &diag_infos);

        let actions: Vec<CodeActionOrCommand> = entries
            .into_iter()
            .filter_map(|e| {
                let edit = e.edit?;
                let range = offset_range(&text, edit.offset, edit.end_offset);
                let kind = match e.kind {
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
                    vec![tower_lsp_server::ls_types::TextEdit {
                        range,
                        new_text: edit.new_text,
                    }],
                );
                Some(CodeActionOrCommand::CodeAction(CodeAction {
                    title: e.title,
                    kind: Some(kind),
                    is_preferred: Some(e.is_preferred),
                    edit: Some(WorkspaceEdit {
                        changes: Some(changes),
                        ..Default::default()
                    }),
                    ..Default::default()
                }))
            })
            .collect();

        if actions.is_empty() {
            return Ok(None);
        }
        Ok(Some(actions))
    }

    // ===== Code Lens (delegated to nika-lsp-core) =====

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = &params.text_document.uri;
        let text = match self.documents.get(uri) {
            Some(d) => d.content(),
            None => return Ok(None),
        };

        let entries = self.handler.code_lenses(&text);
        if entries.is_empty() {
            return Ok(None);
        }

        let lenses: Vec<CodeLens> = entries
            .into_iter()
            .map(|e| {
                let range = Range {
                    start: Position {
                        line: e.line,
                        character: 0,
                    },
                    end: Position {
                        line: e.line,
                        character: 0,
                    },
                };
                CodeLens {
                    range,
                    command: Some(Command {
                        title: e.command.title(),
                        command: e.command.vscode_command().to_string(),
                        arguments: Some(vec![serde_json::json!(uri.as_str())]),
                    }),
                    data: None,
                }
            })
            .collect();

        Ok(Some(lenses))
    }

    // ===== Inlay Hints (delegated to nika-lsp-core) =====

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = &params.text_document.uri;
        let text = match self.documents.get(uri) {
            Some(d) => d.content(),
            None => return Ok(None),
        };

        let start = crate::position::position_to_offset(
            &text,
            params.range.start.line,
            params.range.start.character,
        )
        .map(|o| o.0)
        .unwrap_or(0);
        let end = crate::position::position_to_offset(
            &text,
            params.range.end.line,
            params.range.end.character,
        )
        .map(|o| o.0)
        .unwrap_or(text.len() as u32);

        let entries = self.handler.inlay_hints(&text, start, end);
        if entries.is_empty() {
            return Ok(None);
        }

        let hints: Vec<InlayHint> = entries
            .into_iter()
            .map(|e| {
                let kind = match e.kind {
                    nika_lsp_core::handlers::inlay_hints::HintKind::Type => InlayHintKind::TYPE,
                    nika_lsp_core::handlers::inlay_hints::HintKind::Parameter => {
                        InlayHintKind::PARAMETER
                    }
                };
                InlayHint {
                    position: Position {
                        line: e.line,
                        character: e.character,
                    },
                    label: InlayHintLabel::String(e.label),
                    kind: Some(kind),
                    text_edits: None,
                    tooltip: Some(InlayHintTooltip::String(e.tooltip)),
                    padding_left: None,
                    padding_right: None,
                    data: None,
                }
            })
            .collect();

        Ok(Some(hints))
    }

    // ===== References (Find All References) =====

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let text = match self.documents.get(uri) {
            Some(d) => d.content(),
            None => return Ok(None),
        };

        let offset = crate::position::position_to_offset(&text, position.line, position.character)
            .map(|o| o.0)
            .unwrap_or(0);

        let task_id = match self.handler.find_task_at_offset(&text, offset) {
            Some(id) => id,
            None => return Ok(None),
        };

        let refs = self.handler.references(&text, &task_id);
        let locations: Vec<Location> = refs
            .into_iter()
            .map(|r| {
                let range = offset_range(&text, r.start_offset, r.end_offset);
                Location {
                    uri: uri.clone(),
                    range,
                }
            })
            .collect();

        if locations.is_empty() {
            Ok(None)
        } else {
            Ok(Some(locations))
        }
    }

    // ===== Document Links =====

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let uri = &params.text_document.uri;
        let text = match self.documents.get(uri) {
            Some(d) => d.content(),
            None => return Ok(None),
        };

        let entries = self.handler.document_links(&text);
        let links: Vec<DocumentLink> = entries
            .into_iter()
            .filter_map(|e| {
                let range = offset_range(&text, e.start_offset, e.end_offset);
                let target: Uri = e.target.parse().ok()?;
                Some(DocumentLink {
                    range,
                    target: Some(target),
                    tooltip: Some(e.tooltip),
                    data: None,
                })
            })
            .collect();

        if links.is_empty() {
            Ok(None)
        } else {
            Ok(Some(links))
        }
    }

    // ===== Folding Ranges =====

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let uri = &params.text_document.uri;
        let text = match self.documents.get(uri) {
            Some(d) => d.content(),
            None => return Ok(None),
        };

        let entries = self.handler.folding_ranges(&text);
        let ranges: Vec<FoldingRange> = entries
            .into_iter()
            .map(|e| {
                let kind = match e.kind {
                    nika_lsp_core::handlers::folding_ranges::FoldKind::Region => {
                        Some(FoldingRangeKind::Region)
                    }
                    nika_lsp_core::handlers::folding_ranges::FoldKind::Comment => {
                        Some(FoldingRangeKind::Comment)
                    }
                };
                FoldingRange {
                    start_line: e.start_line,
                    start_character: None,
                    end_line: e.end_line,
                    end_character: None,
                    kind,
                    collapsed_text: None,
                }
            })
            .collect();

        if ranges.is_empty() {
            Ok(None)
        } else {
            Ok(Some(ranges))
        }
    }

    // ===== Rename =====

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = &params.text_document.uri;
        let text = match self.documents.get(uri) {
            Some(d) => d.content(),
            None => return Ok(None),
        };

        let position = params.position;
        let offset = crate::position::position_to_offset(&text, position.line, position.character)
            .map(|o| o.0)
            .unwrap_or(0);

        let range = nika_lsp_core::handlers::rename::prepare_rename(&text, offset);
        match range {
            Some(r) => {
                let start = crate::position::offset_to_position(
                    &text,
                    crate::position::ByteOffset(r.start),
                );
                let end =
                    crate::position::offset_to_position(&text, crate::position::ByteOffset(r.end));
                Ok(Some(PrepareRenameResponse::Range(Range { start, end })))
            }
            None => Ok(None),
        }
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = &params.text_document_position.text_document.uri;
        let text = match self.documents.get(uri) {
            Some(d) => d.content(),
            None => return Ok(None),
        };

        let position = params.text_document_position.position;
        let offset = crate::position::position_to_offset(&text, position.line, position.character)
            .map(|o| o.0)
            .unwrap_or(0);

        let edits = nika_lsp_core::handlers::rename::rename(&text, offset, &params.new_name);

        if edits.is_empty() {
            return Ok(None);
        }

        let text_edits: Vec<TextEdit> = edits
            .into_iter()
            .map(|e| {
                let start = crate::position::offset_to_position(
                    &text,
                    crate::position::ByteOffset(e.start),
                );
                let end =
                    crate::position::offset_to_position(&text, crate::position::ByteOffset(e.end));
                TextEdit {
                    range: Range { start, end },
                    new_text: e.new_text,
                }
            })
            .collect();

        let mut changes = std::collections::HashMap::new();
        changes.insert(uri.clone(), text_edits);

        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }))
    }
}

/// Convert nika-lsp-core SymbolEntry to LSP DocumentSymbol.
fn to_lsp_symbol(
    text: &str,
    entry: nika_lsp_core::handlers::symbols::SymbolEntry,
) -> DocumentSymbol {
    let range = offset_range(text, entry.offset, entry.end_offset);
    let kind = match entry.kind {
        nika_lsp_core::handlers::symbols::SymbolKind::File => SymbolKind::FILE,
        nika_lsp_core::handlers::symbols::SymbolKind::Module => SymbolKind::MODULE,
        nika_lsp_core::handlers::symbols::SymbolKind::Function => SymbolKind::FUNCTION,
        nika_lsp_core::handlers::symbols::SymbolKind::Property => SymbolKind::PROPERTY,
        nika_lsp_core::handlers::symbols::SymbolKind::Variable => SymbolKind::VARIABLE,
    };
    #[allow(deprecated)]
    DocumentSymbol {
        name: entry.name,
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range: range,
        children: if entry.children.is_empty() {
            None
        } else {
            Some(
                entry
                    .children
                    .into_iter()
                    .map(|c| to_lsp_symbol(text, c))
                    .collect(),
            )
        },
    }
}

/// Convert byte offsets to LSP Range (UTF-16 aware via position module).
fn offset_range(text: &str, start_offset: u32, end_offset: u32) -> Range {
    use crate::position::{offset_to_position, ByteOffset};
    let start = offset_to_position(text, ByteOffset(start_offset));
    let end = offset_to_position(text, ByteOffset(end_offset));
    Range { start, end }
}

/// Extract word at column position.
fn extract_word_at_col(line: &str, col: usize) -> String {
    let chars: Vec<char> = line.chars().collect();

    if col >= chars.len() {
        return String::new();
    }

    let mut start = col;
    while start > 0 && is_identifier_char(chars[start - 1]) {
        start -= 1;
    }

    let mut end = col;
    while end < chars.len() && is_identifier_char(chars[end]) {
        end += 1;
    }

    chars[start..end].iter().collect()
}

fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_word_at_col() {
        assert_eq!(extract_word_at_col("hello world", 0), "hello");
        assert_eq!(extract_word_at_col("hello world", 3), "hello");
        assert_eq!(extract_word_at_col("hello world", 6), "world");
        assert_eq!(extract_word_at_col("  step1", 4), "step1");
        assert_eq!(extract_word_at_col("use: my-task", 6), "my-task");
    }

    // ── workflow_graph tests ────────────────────────────────────────────

    #[test]
    fn workflow_graph_empty_content() {
        let graph = extract_workflow_graph("");
        assert_eq!(graph["nodes"].as_array().unwrap().len(), 0);
        assert_eq!(graph["edges"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn workflow_graph_single_infer_task() {
        let yaml = "schema: \"nika/workflow@0.12\"\nworkflow: test\ntasks:\n  - id: hello\n    infer: \"Say hello\"\n";
        let graph = extract_workflow_graph(yaml);
        let nodes = graph["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0]["id"], "hello");
        assert_eq!(nodes[0]["verb"], "infer");
        assert_eq!(nodes[0]["status"], "pending");
        assert!(graph["edges"].as_array().unwrap().is_empty());
        assert_eq!(graph["workflowName"], "test");
    }

    #[test]
    fn workflow_graph_multiple_tasks_with_verbs() {
        // Use infer short-form which is known to parse correctly
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: step1
    infer: "first task"
  - id: step2
    infer: "second task"
"#;
        let graph = extract_workflow_graph(yaml);
        let nodes = graph["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 2, "graph: {}", serde_json::to_string_pretty(&graph).unwrap());
        assert_eq!(nodes[0]["verb"], "infer");
        assert_eq!(nodes[1]["verb"], "infer");
    }

    #[test]
    fn workflow_graph_depends_on_creates_edges() {
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: a
    infer: "x"
  - id: b
    depends_on: [a]
    infer: "y"
"#;
        let graph = extract_workflow_graph(yaml);
        let edges = graph["edges"].as_array().unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0]["source"], "a");
        assert_eq!(edges[0]["target"], "b");
        assert_eq!(edges[0]["isDataEdge"], false);
    }

    #[test]
    fn workflow_graph_with_binding_creates_data_edge() {
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: src
    infer: "source data"
  - id: dst
    with:
      data: $src
    infer: "use data"
"#;
        let graph = extract_workflow_graph(yaml);
        let edges = graph["edges"].as_array().unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0]["source"], "src");
        assert_eq!(edges[0]["target"], "dst");
        assert_eq!(edges[0]["isDataEdge"], true);
    }

    #[test]
    fn workflow_graph_ignores_env_and_inputs() {
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: s
    with:
      k: $env.KEY
      t: $inputs.topic
    infer: "x"
"#;
        let graph = extract_workflow_graph(yaml);
        assert!(graph["edges"].as_array().unwrap().is_empty());
    }

    #[test]
    fn workflow_graph_malformed_yaml_returns_empty() {
        let graph = extract_workflow_graph("not: [valid: yaml: [[[");
        assert_eq!(graph["nodes"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn workflow_graph_no_duplicate_edge_with_depends_on_and_with() {
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: a
    infer: "x"
  - id: b
    depends_on: [a]
    with:
      d: $a
    infer: "y"
"#;
        let graph = extract_workflow_graph(yaml);
        let edges = graph["edges"].as_array().unwrap();
        assert_eq!(edges.len(), 1, "should not duplicate edge");
        assert_eq!(edges[0]["isDataEdge"], false);
    }

    #[test]
    fn workflow_graph_line_ordering() {
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: first
    infer: "a"
  - id: second
    infer: "b"
"#;
        let graph = extract_workflow_graph(yaml);
        let nodes = graph["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 2);
        let l0 = nodes[0]["line"].as_u64().unwrap();
        let l1 = nodes[1]["line"].as_u64().unwrap();
        assert!(l0 < l1, "second task on later line: {l0} < {l1}");
    }
}
