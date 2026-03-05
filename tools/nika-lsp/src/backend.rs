//! LSP Backend implementation for Nika.
//!
//! This is the main LSP server that handles all protocol messages and
//! orchestrates validation, completion, and hover providers.

use dashmap::DashMap;
use lsp_types::*;
use tokio::sync::mpsc;
use tower_lsp::jsonrpc::Result;
use tower_lsp::{Client, LanguageServer};

use crate::completion::{
    get_completion_context, provider_completions, schema_completions,
    structured_output_completions, task_id_completions, verb_completions, CompletionContext,
};
use crate::diagnostics::validate_document;
use crate::document::DocumentState;
use crate::hover::get_hover;

/// Request to validate a document.
pub struct ValidationRequest {
    pub uri: Url,
    pub content: String,
    pub version: i32,
}

/// The Nika LSP backend.
pub struct NikaBackend {
    /// LSP client for sending notifications
    client: Client,
    /// Open documents
    documents: DashMap<Url, DocumentState>,
    /// Validation request channel
    validation_tx: mpsc::Sender<ValidationRequest>,
}

impl NikaBackend {
    /// Create a new backend instance.
    pub fn new(client: Client) -> Self {
        let (tx, rx) = mpsc::channel(100);

        let backend = Self {
            client: client.clone(),
            documents: DashMap::new(),
            validation_tx: tx,
        };

        // Spawn validation worker
        tokio::spawn(validation_worker(rx, client));

        backend
    }

    /// Extract task IDs from a document.
    fn extract_task_ids(&self, uri: &Url) -> Vec<String> {
        let doc = match self.documents.get(uri) {
            Some(d) => d,
            None => return vec![],
        };

        let content = doc.content();
        let mut task_ids = Vec::new();

        // Simple regex-free extraction: look for "- id: xxx" patterns
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("- id:") || trimmed.starts_with("-id:") {
                if let Some(id_part) = trimmed.strip_prefix("- id:").or_else(|| trimmed.strip_prefix("-id:")) {
                    let id = id_part.trim().trim_matches('"').trim_matches('\'');
                    if !id.is_empty() {
                        task_ids.push(id.to_string());
                    }
                }
            }
        }

        task_ids
    }
}

/// Background worker that processes validation requests.
async fn validation_worker(mut rx: mpsc::Receiver<ValidationRequest>, client: Client) {
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
                    // Validate document
                    let diagnostics = validate_document(&req.content, &req.uri);

                    // Publish diagnostics
                    client
                        .publish_diagnostics(req.uri, diagnostics, Some(req.version))
                        .await;
                }
            }
        }
    }
}

#[tower_lsp::async_trait]
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
                // Diagnostics
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions {
                        identifier: Some("nika".to_string()),
                        inter_file_dependencies: true,
                        workspace_diagnostics: true,
                        ..Default::default()
                    },
                )),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "nika-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
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

        tracing::debug!("Document opened: {}", uri);

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
        tracing::debug!("Document saved: {}", params.text_document.uri);
        // Validation is already triggered by did_change
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        tracing::debug!("Document closed: {}", uri);

        // Remove document
        self.documents.remove(&uri);

        // Clear diagnostics
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    // ===== Completion =====

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let doc = match self.documents.get(uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        // Determine completion context
        let context = get_completion_context(&doc, position);

        let items = match context {
            CompletionContext::TaskVerb => verb_completions(),
            CompletionContext::Schema => schema_completions(),
            CompletionContext::StructuredSchema => structured_output_completions(),
            CompletionContext::Provider => provider_completions(),
            CompletionContext::UseReference { partial } => {
                let task_ids = self.extract_task_ids(uri);
                task_id_completions(&task_ids, &partial)
            }
            CompletionContext::McpServer => {
                // Extract MCP server names from the document content
                let content = doc.content();
                let servers = crate::completion::extract_mcp_servers(&content);
                crate::mcp_discovery::invoke_completions(&servers, None)
            }
            CompletionContext::McpTool { server } => {
                // Complete tools for the specified MCP server
                crate::mcp_discovery::mcp_tool_completions(&server)
            }
            CompletionContext::Unknown => {
                // Return all possible completions in unknown context
                let mut items = verb_completions();
                items.extend(schema_completions());
                items
            }
        };

        if items.is_empty() {
            return Ok(None);
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

        Ok(get_hover(&doc, position))
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

        // Search for task definition
        for (line_num, line_content) in lines.iter().enumerate() {
            let trimmed = line_content.trim();
            if trimmed.starts_with("- id:") {
                let id = trimmed
                    .strip_prefix("- id:")
                    .unwrap_or("")
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'');

                if id == word {
                    return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                        uri: uri.clone(),
                        range: Range {
                            start: Position {
                                line: line_num as u32,
                                character: 0,
                            },
                            end: Position {
                                line: line_num as u32,
                                character: line_content.len() as u32,
                            },
                        },
                    })));
                }
            }
        }

        Ok(None)
    }
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
}
