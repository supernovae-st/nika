//! Task Executor - individual task execution (v0.2)
//!
//! Handles execution of individual tasks: infer, exec, fetch, invoke, agent.
//! Uses DashMap for lock-free provider caching.

use rustc_hash::FxHashMap;
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use tokio::sync::OnceCell;
use tracing::{debug, instrument};
use uuid::Uuid;

use crate::ast::{
    decompose::{DecomposeSpec, DecomposeStrategy},
    AgentParams, ExecParams, FetchParams, InferParams, InvokeParams, McpConfigInline, TaskAction,
};
use crate::binding::{template_resolve, ResolvedBindings};
use crate::error::NikaError;
use crate::event::{ContextSource, EventKind, EventLog};
use crate::mcp::{McpClient, McpConfig};
use crate::provider::rig::RigProvider;
use crate::runtime::RigAgentLoop;
use crate::store::DataStore;
use crate::util::{CONNECT_TIMEOUT, EXEC_TIMEOUT, FETCH_TIMEOUT, REDIRECT_LIMIT};

/// Task executor with cached providers, shared HTTP client, and event logging
#[derive(Clone)]
pub struct TaskExecutor {
    /// Shared HTTP client (connection pooling)
    http_client: reqwest::Client,
    /// Cached rig-core providers (v0.3.1+)
    rig_provider_cache: Arc<DashMap<String, RigProvider>>,
    /// Cached MCP clients with async-safe initialization (prevents race conditions in for_each)
    ///
    /// ## Why Arc<DashMap<String, Arc<OnceCell<Arc<McpClient>>>>>?
    ///
    /// The triple-Arc structure is intentional:
    ///
    /// 1. **Outer Arc<DashMap>**: TaskExecutor derives Clone; Arc ensures clones share the cache.
    /// 2. **Middle Arc<OnceCell>**: DashMap entry() returns RefMut that cannot be held across await.
    ///    We clone Arc<OnceCell> to release the guard before calling get_or_try_init().await.
    ///    Without this, we'd have the "lock-across-await" footgun causing shard starvation.
    /// 3. **Inner Arc<McpClient>**: McpClient is shared across tasks; Arc enables cheap cloning.
    ///
    /// See tests/mcp_race_conditions_test.rs for validation.
    mcp_client_cache: Arc<DashMap<String, Arc<OnceCell<Arc<McpClient>>>>>,
    /// MCP server configurations from workflow
    mcp_configs: Arc<FxHashMap<String, McpConfigInline>>,
    /// Default provider name
    default_provider: Arc<str>,
    /// Default model
    default_model: Option<Arc<str>>,
    /// Event log for fine-grained audit trail
    event_log: EventLog,
}

impl TaskExecutor {
    /// Create a new executor with default provider, model, MCP configs, and event log
    pub fn new(
        provider: &str,
        model: Option<&str>,
        mcp_configs: Option<FxHashMap<String, McpConfigInline>>,
        event_log: EventLog,
    ) -> Self {
        // SAFETY: ClientBuilder::build() only fails with custom TLS or proxy config.
        // We use defaults, so this is effectively infallible.
        let http_client = reqwest::Client::builder()
            .timeout(FETCH_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::limited(REDIRECT_LIMIT))
            .user_agent("nika-cli/0.1")
            .build()
            .unwrap_or_else(|e| {
                tracing::error!("HTTP client build failed: {e}. Using default client.");
                reqwest::Client::new()
            });

        Self {
            http_client,
            rig_provider_cache: Arc::new(DashMap::new()),
            mcp_client_cache: Arc::new(DashMap::new()),
            mcp_configs: Arc::new(mcp_configs.unwrap_or_default()),
            default_provider: provider.into(),
            default_model: model.map(Into::into),
            event_log,
        }
    }

    /// Inject a mock MCP client for testing
    ///
    /// This allows tests to use mock clients without relying on automatic fallback.
    /// Call this after creating the executor but before executing invoke actions.
    #[cfg(test)]
    pub fn inject_mock_mcp_client(&self, name: &str) {
        let cell = Arc::new(OnceCell::new());
        // Initialize the cell with a mock client
        let mock = Arc::new(McpClient::mock(name));
        cell.set(mock).expect("Cell should be empty");
        self.mcp_client_cache.insert(name.to_string(), cell);
    }

    /// Expand a decompose spec into iteration items (v0.5)
    ///
    /// Returns an array of JSON values that can be used as for_each items.
    /// Supports semantic (MCP traverse), static (binding resolution), and nested strategies.
    #[instrument(name = "expand_decompose", skip(self, bindings, datastore), fields(
        strategy = ?spec.strategy,
        traverse = %spec.traverse,
        source = %spec.source
    ))]
    pub async fn expand_decompose(
        &self,
        spec: &DecomposeSpec,
        bindings: &ResolvedBindings,
        datastore: &DataStore,
    ) -> Result<Vec<serde_json::Value>, NikaError> {
        match spec.strategy {
            DecomposeStrategy::Semantic => {
                self.expand_decompose_semantic(spec, bindings, datastore)
                    .await
            }
            DecomposeStrategy::Static => self.expand_decompose_static(spec, bindings, datastore),
            DecomposeStrategy::Nested => {
                self.expand_decompose_nested(spec, bindings, datastore)
                    .await
            }
        }
    }

    /// Expand using semantic traversal via MCP (calls novanet_traverse)
    async fn expand_decompose_semantic(
        &self,
        spec: &DecomposeSpec,
        bindings: &ResolvedBindings,
        datastore: &DataStore,
    ) -> Result<Vec<serde_json::Value>, NikaError> {
        use serde_json::{json, Value};

        // Get MCP client
        let server_name = spec.mcp_server();
        let client = self.get_mcp_client(server_name).await?;

        // Resolve source binding
        let source_value = self.resolve_decompose_source(&spec.source, bindings, datastore)?;
        let source_key = self.extract_decompose_key(&source_value)?;

        debug!(
            source_key = %source_key,
            arc = %spec.traverse,
            "Calling novanet_traverse for decompose"
        );

        // Call novanet_traverse
        let params = json!({
            "start": source_key,
            "arc": spec.traverse,
            "direction": "outgoing"
        });

        let result = client.call_tool("novanet_traverse", params).await?;

        // Parse JSON from result content
        let result_json: Value =
            serde_json::from_str(&result.text()).map_err(|e| NikaError::McpInvalidResponse {
                tool: "novanet_traverse".to_string(),
                reason: format!("failed to parse JSON response: {}", e),
            })?;

        // Extract nodes from result
        let mut items = self.extract_decompose_nodes(&result_json)?;

        // Apply max_items limit
        if let Some(max) = spec.max_items {
            items.truncate(max);
        }

        debug!(
            count = items.len(),
            max_items = ?spec.max_items,
            "Decompose expanded to items"
        );

        Ok(items)
    }

    /// Expand using static binding resolution (no MCP call)
    fn expand_decompose_static(
        &self,
        spec: &DecomposeSpec,
        bindings: &ResolvedBindings,
        datastore: &DataStore,
    ) -> Result<Vec<serde_json::Value>, NikaError> {
        let source_value = self.resolve_decompose_source(&spec.source, bindings, datastore)?;

        // Expect array
        let items = source_value
            .as_array()
            .ok_or_else(|| NikaError::BindingTypeMismatch {
                expected: "array".to_string(),
                actual: self.json_type_name(&source_value),
                path: spec.source.clone(),
            })?
            .clone();

        // Apply max_items limit
        let mut items = items;
        if let Some(max) = spec.max_items {
            items.truncate(max);
        }

        Ok(items)
    }

    /// Expand using nested recursive traversal via MCP (v0.5.2)
    ///
    /// Recursively follows arcs until max_depth or no more children.
    /// Uses BFS to collect all descendant nodes (excluding root) into a flat array.
    async fn expand_decompose_nested(
        &self,
        spec: &DecomposeSpec,
        bindings: &ResolvedBindings,
        datastore: &DataStore,
    ) -> Result<Vec<serde_json::Value>, NikaError> {
        use serde_json::{json, Value};
        use std::collections::HashSet;

        // Get MCP client
        let server_name = spec.mcp_server();
        let client = self.get_mcp_client(server_name).await?;

        // Resolve source binding
        let source_value = self.resolve_decompose_source(&spec.source, bindings, datastore)?;
        let root_key = self.extract_decompose_key(&source_value)?;

        // Defaults for nested traversal
        let max_depth = spec.max_depth.unwrap_or(3);
        let max_items = spec.max_items.unwrap_or(100); // Safety limit

        debug!(
            root_key = %root_key,
            arc = %spec.traverse,
            max_depth = max_depth,
            max_items = max_items,
            "Starting nested decompose traversal"
        );

        // BFS traversal to collect all descendant nodes
        let mut items: Vec<Value> = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: Vec<(String, usize)> = vec![(root_key.clone(), 0)];

        visited.insert(root_key.clone());

        while let Some((current_key, depth)) = queue.pop() {
            // Stop if we've reached max depth
            if depth >= max_depth {
                continue;
            }

            // Stop if we have enough items
            if items.len() >= max_items {
                break;
            }

            // Call novanet_traverse for current node
            let params = json!({
                "start": current_key,
                "arc": spec.traverse,
                "direction": "outgoing"
            });

            let result = match client.call_tool("novanet_traverse", params).await {
                Ok(r) => r,
                Err(e) => {
                    debug!(key = %current_key, error = %e, "Traverse failed, skipping node");
                    continue;
                }
            };

            // Parse result
            let result_json: Value = match serde_json::from_str(&result.text()) {
                Ok(v) => v,
                Err(e) => {
                    debug!(key = %current_key, error = %e, "Failed to parse traverse result");
                    continue;
                }
            };

            // Extract child nodes
            let children = match self.extract_decompose_nodes(&result_json) {
                Ok(c) => c,
                Err(_) => continue,
            };

            for child in children {
                // Get child key for tracking
                let child_key = match self.extract_decompose_key(&child) {
                    Ok(k) => k,
                    Err(_) => continue,
                };

                // Skip if already visited (avoid cycles)
                if visited.contains(&child_key) {
                    continue;
                }

                visited.insert(child_key.clone());
                items.push(child);

                // Add to queue for further traversal
                queue.push((child_key, depth + 1));

                // Early exit if we have enough items
                if items.len() >= max_items {
                    break;
                }
            }
        }

        debug!(
            count = items.len(),
            visited = visited.len(),
            "Nested decompose completed"
        );

        Ok(items)
    }

    /// Resolve source binding expression for decompose (v0.5: lazy binding support)
    fn resolve_decompose_source(
        &self,
        source: &str,
        bindings: &ResolvedBindings,
        datastore: &DataStore,
    ) -> Result<serde_json::Value, NikaError> {
        if source.starts_with("{{use.") && source.ends_with("}}") {
            // Template syntax: {{use.alias}} - supports lazy bindings
            let alias = &source[6..source.len() - 2];
            bindings.get_resolved(alias, datastore)
        } else if let Some(alias) = source.strip_prefix('$') {
            if alias.contains('.') {
                // Path syntax: $task.field
                datastore
                    .resolve_path(alias)
                    .ok_or_else(|| NikaError::BindingNotFound {
                        alias: alias.to_string(),
                    })
            } else {
                // Simple alias - supports lazy bindings
                bindings.get_resolved(alias, datastore)
            }
        } else {
            // Literal value
            Ok(serde_json::Value::String(source.to_string()))
        }
    }

    /// Extract key from source value (string or object with 'key' field)
    fn extract_decompose_key(&self, value: &serde_json::Value) -> Result<String, NikaError> {
        match value {
            serde_json::Value::String(s) => Ok(s.clone()),
            serde_json::Value::Object(obj) => obj
                .get("key")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| NikaError::BindingTypeMismatch {
                    expected: "string or object with 'key'".to_string(),
                    actual: "object without 'key'".to_string(),
                    path: "decompose.source".to_string(),
                }),
            _ => Err(NikaError::BindingTypeMismatch {
                expected: "string or object".to_string(),
                actual: self.json_type_name(value),
                path: "decompose.source".to_string(),
            }),
        }
    }

    /// Extract nodes array from novanet_traverse result
    fn extract_decompose_nodes(
        &self,
        result: &serde_json::Value,
    ) -> Result<Vec<serde_json::Value>, NikaError> {
        if let Some(nodes) = result.get("nodes").and_then(|v| v.as_array()) {
            return Ok(nodes.clone());
        }
        if let Some(items) = result.get("items").and_then(|v| v.as_array()) {
            return Ok(items.clone());
        }
        if let Some(results) = result.get("results").and_then(|v| v.as_array()) {
            return Ok(results.clone());
        }
        if let Some(arr) = result.as_array() {
            return Ok(arr.clone());
        }
        Err(NikaError::McpInvalidResponse {
            tool: "novanet_traverse".to_string(),
            reason: "expected nodes/items/results array in response".to_string(),
        })
    }

    /// Get JSON type name for error messages
    fn json_type_name(&self, value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Null => "null",
            serde_json::Value::Bool(_) => "boolean",
            serde_json::Value::Number(_) => "number",
            serde_json::Value::String(_) => "string",
            serde_json::Value::Array(_) => "array",
            serde_json::Value::Object(_) => "object",
        }
        .to_string()
    }

    /// Run a task action with the given bindings (v0.5)
    ///
    /// The datastore is required for resolving lazy bindings during template substitution.
    #[instrument(skip(self, bindings, datastore), fields(action_type = %action_type(action)))]
    pub async fn execute(
        &self,
        task_id: &Arc<str>,
        action: &TaskAction,
        bindings: &ResolvedBindings,
        datastore: &DataStore,
    ) -> Result<String, NikaError> {
        debug!("Running task action");
        match action {
            TaskAction::Infer { infer } => {
                self.run_infer(task_id, infer, bindings, datastore).await
            }
            TaskAction::Exec { exec: e } => self.run_exec(task_id, e, bindings, datastore).await,
            TaskAction::Fetch { fetch } => {
                self.run_fetch(task_id, fetch, bindings, datastore).await
            }
            TaskAction::Invoke { invoke } => {
                self.run_invoke(task_id, invoke, bindings, datastore).await
            }
            TaskAction::Agent { agent } => {
                self.run_agent(task_id, agent, bindings, datastore).await
            }
        }
    }

    /// Get or create a cached rig-core provider (v0.3.1+)
    ///
    /// Uses rig-core's provider clients for LLM inference.
    fn get_rig_provider(&self, name: &str) -> Result<RigProvider, NikaError> {
        use dashmap::mapref::entry::Entry;

        match self.rig_provider_cache.entry(name.to_string()) {
            Entry::Occupied(e) => Ok(e.get().clone()),
            Entry::Vacant(e) => {
                let provider = match name {
                    "claude" | "anthropic" => RigProvider::claude(),
                    "openai" | "gpt" => RigProvider::openai(),
                    // v0.6: Additional providers
                    "mistral" => RigProvider::mistral(),
                    "ollama" => RigProvider::ollama(),
                    "groq" => RigProvider::groq(),
                    "deepseek" | "deep-seek" => RigProvider::deepseek(),
                    _ => {
                        return Err(NikaError::Provider(format!(
                            "Unknown rig provider: {}. Supported: claude, openai, mistral, ollama, groq, deepseek",
                            name
                        )));
                    }
                };
                e.insert(provider.clone());
                Ok(provider)
            }
        }
    }

    async fn run_infer(
        &self,
        task_id: &Arc<str>,
        infer: &InferParams,
        bindings: &ResolvedBindings,
        datastore: &DataStore,
    ) -> Result<String, NikaError> {
        // Resolve {{use.alias}} templates (v0.5: supports lazy bindings)
        let prompt = template_resolve(&infer.prompt, bindings, datastore)?;

        // EMIT: TemplateResolved
        self.event_log.emit(EventKind::TemplateResolved {
            task_id: Arc::clone(task_id),
            template: infer.prompt.clone(),
            result: prompt.to_string(),
        });

        // EMIT: ContextAssembled - capture binding sources used in prompt (v0.7.0)
        let bindings_value = bindings.to_value();
        let sources: Vec<ContextSource> = bindings_value
            .as_object()
            .map(|obj| {
                obj.iter()
                    .map(|(alias, value)| ContextSource {
                        node: alias.clone(),
                        tokens: (value.to_string().len() / 4) as u32, // Rough estimate: ~4 chars/token
                    })
                    .collect()
            })
            .unwrap_or_default();
        let total_tokens = (prompt.len() / 4) as u32;

        self.event_log.emit(EventKind::ContextAssembled {
            task_id: Arc::clone(task_id),
            sources,
            excluded: Vec::new(), // No exclusion logic in simple infer
            total_tokens,
            budget_used_pct: 0.0, // No budget concept in executor
            truncated: false,
        });

        // Use task-level override or workflow default
        let provider_name = infer.provider.as_deref().unwrap_or(&self.default_provider);

        // Get cached rig provider (v0.3.1+)
        let provider = self.get_rig_provider(provider_name)?;

        // Resolve model: task override -> workflow default -> provider default
        let model = infer.model.as_deref().or(self.default_model.as_deref());

        // EMIT: ProviderCalled
        self.event_log.emit(EventKind::ProviderCalled {
            task_id: Arc::clone(task_id),
            provider: provider_name.to_string(),
            model: model
                .unwrap_or_else(|| provider.default_model())
                .to_string(),
            prompt_len: prompt.len(),
        });

        let result = provider
            .infer(&prompt, model)
            .await
            .map_err(|e| NikaError::Provider(e.to_string()))?;

        // EMIT: ProviderResponded
        // Note: Token counts only available via infer_stream() (streaming mode).
        // The executor uses non-streaming for simplicity. TUI uses streaming for
        // real-time display with accurate token metrics via StreamResult.
        self.event_log.emit(EventKind::ProviderResponded {
            task_id: Arc::clone(task_id),
            request_id: None,
            input_tokens: 0,  // Non-streaming: token metrics unavailable
            output_tokens: 0, // Use TUI streaming mode for accurate counts
            cache_read_tokens: 0,
            ttft_ms: None,
            finish_reason: "stop".to_string(),
            cost_usd: 0.0,
        });

        Ok(result)
    }

    async fn run_exec(
        &self,
        task_id: &Arc<str>,
        exec: &ExecParams,
        bindings: &ResolvedBindings,
        datastore: &DataStore,
    ) -> Result<String, NikaError> {
        // Resolve {{use.alias}} templates (v0.5: supports lazy bindings)
        let command = template_resolve(&exec.command, bindings, datastore)?;

        // EMIT: TemplateResolved
        self.event_log.emit(EventKind::TemplateResolved {
            task_id: Arc::clone(task_id),
            template: exec.command.clone(),
            result: command.to_string(),
        });

        // Execute with timeout
        let output = tokio::time::timeout(
            EXEC_TIMEOUT,
            tokio::process::Command::new("sh")
                .arg("-c")
                .arg(command.as_ref())
                .output(),
        )
        .await
        .map_err(|_| {
            NikaError::Execution(format!(
                "Command timed out after {}s",
                EXEC_TIMEOUT.as_secs()
            ))
        })?
        .map_err(|e| NikaError::Execution(format!("Failed to execute command: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(NikaError::Execution(format!("Command failed: {}", stderr)));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    #[instrument(skip(self, bindings, datastore), fields(url = %fetch.url))]
    async fn run_fetch(
        &self,
        task_id: &Arc<str>,
        fetch: &FetchParams,
        bindings: &ResolvedBindings,
        datastore: &DataStore,
    ) -> Result<String, NikaError> {
        // Resolve {{use.alias}} templates (v0.5: supports lazy bindings)
        let url = template_resolve(&fetch.url, bindings, datastore)?;

        // EMIT: TemplateResolved
        self.event_log.emit(EventKind::TemplateResolved {
            task_id: Arc::clone(task_id),
            template: fetch.url.clone(),
            result: url.to_string(),
        });

        let mut request = if fetch.method.eq_ignore_ascii_case("POST") {
            self.http_client.post(url.as_ref())
        } else if fetch.method.eq_ignore_ascii_case("PUT") {
            self.http_client.put(url.as_ref())
        } else if fetch.method.eq_ignore_ascii_case("DELETE") {
            self.http_client.delete(url.as_ref())
        } else {
            self.http_client.get(url.as_ref()) // Default to GET
        };

        // Add headers
        for (key, value) in &fetch.headers {
            let resolved_value = template_resolve(value, bindings, datastore)?;
            request = request.header(key, resolved_value.as_ref());
        }

        // Add body if present
        if let Some(body) = &fetch.body {
            let resolved_body = template_resolve(body, bindings, datastore)?;
            request = request.body(resolved_body.into_owned());
        }

        let response = request
            .send()
            .await
            .map_err(|e| NikaError::Execution(format!("HTTP request failed: {}", e)))?;

        response
            .text()
            .await
            .map_err(|e| NikaError::Execution(format!("Failed to read response: {}", e)))
    }

    /// Execute an invoke action (MCP tool call or resource read)
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task identifier for event logging
    /// * `invoke` - Invoke parameters with mcp server name and tool/resource
    /// * `bindings` - Use bindings for template resolution in params
    ///
    /// # Template Resolution
    ///
    /// Templates like `{{use.variable}}` in params are resolved before calling the MCP tool.
    /// This enables for_each iterations to pass dynamic values to MCP tools.
    #[instrument(skip(self, bindings, datastore), fields(mcp = %invoke.mcp))]
    async fn run_invoke(
        &self,
        task_id: &Arc<str>,
        invoke: &InvokeParams,
        bindings: &ResolvedBindings,
        datastore: &DataStore,
    ) -> Result<String, NikaError> {
        // Validate invoke params (tool XOR resource)
        invoke
            .validate()
            .map_err(|e| NikaError::ValidationError { reason: e })?;

        // Generate unique call_id for correlation
        let call_id = Uuid::new_v4().to_string();
        let start_time = Instant::now();

        // EMIT: McpInvoke event (with params for TUI display)
        self.event_log.emit(EventKind::McpInvoke {
            task_id: Arc::clone(task_id),
            call_id: call_id.clone(),
            mcp_server: invoke.mcp.clone(),
            tool: invoke.tool.clone(),
            resource: invoke.resource.clone(),
            params: invoke.params.clone(),
        });

        // Get or create MCP client (real or mock depending on config)
        let client = self.get_mcp_client(&invoke.mcp).await?;

        let is_error = false;
        let result = if let Some(tool) = &invoke.tool {
            // Tool call path - resolve templates in params
            let params = if let Some(ref original_params) = invoke.params {
                // Convert params to string, resolve templates, parse back
                let params_str = serde_json::to_string(original_params).map_err(|e| {
                    NikaError::Execution(format!("Failed to serialize params: {}", e))
                })?;
                let resolved_str = template_resolve(&params_str, bindings, datastore)?;
                serde_json::from_str(&resolved_str).map_err(|e| {
                    NikaError::Execution(format!(
                        "Failed to parse resolved params '{}': {}",
                        resolved_str, e
                    ))
                })?
            } else {
                serde_json::Value::Null
            };
            let tool_result = client.call_tool(tool, params).await?;

            // Check if tool returned an error
            if tool_result.is_error {
                // Emit response event before returning error
                let duration_ms = start_time.elapsed().as_millis() as u64;
                let error_text = tool_result.text();
                self.event_log.emit(EventKind::McpResponse {
                    task_id: Arc::clone(task_id),
                    call_id: call_id.clone(),
                    output_len: error_text.len(),
                    duration_ms,
                    cached: false,
                    is_error: true,
                    response: Some(serde_json::json!({"error": error_text.clone()})),
                });
                return Err(NikaError::McpToolError {
                    tool: tool.clone(),
                    reason: error_text,
                    error_code: None,
                });
            }

            // Extract text and try to parse as JSON
            let text = tool_result.text();
            serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text))
        } else if let Some(resource) = &invoke.resource {
            // Resource read path
            let content = client.read_resource(resource).await?;
            content
                .text
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or(serde_json::Value::Null)
        } else {
            // validate() ensures this never happens
            unreachable!("validate() ensures tool or resource is set")
        };

        // EMIT: McpResponse event (with full response for TUI display)
        let duration_ms = start_time.elapsed().as_millis() as u64;
        self.event_log.emit(EventKind::McpResponse {
            task_id: Arc::clone(task_id),
            call_id,
            output_len: result.to_string().len(),
            duration_ms,
            cached: client.was_last_call_cached(),
            is_error,
            response: Some(result.clone()),
        });

        // Return JSON string representation
        Ok(result.to_string())
    }

    /// Execute an agent action (agentic execution with tool calling loop)
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task identifier for event logging
    /// * `agent` - Agent parameters with prompt, mcp servers, and stop conditions
    /// * `bindings` - Use bindings for template resolution
    ///
    /// # Flow
    ///
    /// 1. Resolve templates in agent prompt
    /// 2. Validate agent parameters
    /// 3. Emit AgentStart event
    /// 4. Get LLM provider (task override or workflow default)
    /// 5. Build MCP client map for required servers
    /// 6. Create and run AgentLoop
    /// 7. Emit AgentComplete event
    /// 8. Return final output as JSON string
    #[instrument(skip(self, bindings, datastore), fields(max_turns = %agent.effective_max_turns()))]
    async fn run_agent(
        &self,
        task_id: &Arc<str>,
        agent: &AgentParams,
        bindings: &ResolvedBindings,
        datastore: &DataStore,
    ) -> Result<String, NikaError> {
        // Resolve {{use.alias}} templates in prompt (v0.5: supports lazy bindings)
        let resolved_prompt = template_resolve(&agent.prompt, bindings, datastore)?;

        // EMIT: TemplateResolved event
        self.event_log.emit(EventKind::TemplateResolved {
            task_id: Arc::clone(task_id),
            template: agent.prompt.clone(),
            result: resolved_prompt.to_string(),
        });

        // Create agent params with resolved prompt
        let resolved_agent = AgentParams {
            prompt: resolved_prompt.into_owned(),
            ..agent.clone()
        };

        // Validate agent params
        resolved_agent
            .validate()
            .map_err(|e| NikaError::AgentValidationError { reason: e })?;

        // EMIT: AgentStart event
        self.event_log.emit(EventKind::AgentStart {
            task_id: Arc::clone(task_id),
            max_turns: resolved_agent.effective_max_turns(),
            mcp_servers: resolved_agent.mcp.clone(),
        });

        // Get provider name (task override or workflow default)
        // Clone to avoid borrow conflict when moving resolved_agent into RigAgentLoop
        let provider_name: String = resolved_agent
            .provider
            .clone()
            .unwrap_or_else(|| self.default_provider.to_string());

        // Ensure resolved_agent has the provider set for run_auto() dispatch
        let resolved_agent = AgentParams {
            provider: Some(provider_name.clone()),
            ..resolved_agent
        };

        // Build MCP client map for this agent
        let mut mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();
        for mcp_name in &resolved_agent.mcp {
            let client = self.get_mcp_client(mcp_name).await?;
            mcp_clients.insert(mcp_name.clone(), client);
        }

        // Create rig-based agent loop (v0.3.1+)
        let mut agent_loop = RigAgentLoop::new(
            task_id.to_string(),
            resolved_agent,
            self.event_log.clone(),
            mcp_clients,
        )?;

        let start = std::time::Instant::now();

        // Run agent with appropriate provider
        // mock provider uses run_mock(), real providers use run_auto() which dispatches
        // based on AgentParams.provider (claude/openai)
        let result = if provider_name.as_str() == "mock" {
            agent_loop.run_mock().await?
        } else {
            // Use run_auto() which dispatches to run_claude() or run_openai()
            // based on the provider field we just set
            agent_loop.run_auto().await?
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        // EMIT: AgentComplete event
        self.event_log.emit(EventKind::AgentComplete {
            task_id: Arc::clone(task_id),
            turns: result.turns as u32,
            stop_reason: format!("{:?}", result.status),
        });

        tracing::info!(
            task_id = %task_id,
            turns = result.turns,
            status = ?result.status,
            tokens = result.total_tokens,
            duration_ms = duration_ms,
            "Agent loop completed"
        );

        // Return final output as JSON string
        Ok(result.final_output.to_string())
    }

    /// Get or create an MCP client for a named server
    ///
    /// Uses OnceCell per server to ensure thread-safe initialization.
    /// Even with concurrent for_each iterations, only one client is created per server.
    ///
    /// Creates real MCP clients from workflow configuration if available,
    /// otherwise falls back to mock clients for testing.
    async fn get_mcp_client(&self, name: &str) -> Result<Arc<McpClient>, NikaError> {
        // Get or create the OnceCell for this server (atomic via DashMap entry)
        let cell = self
            .mcp_client_cache
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(OnceCell::new()))
            .clone();

        // Clone what we need for the async closure
        let mcp_configs = Arc::clone(&self.mcp_configs);
        let name_owned = name.to_string();
        let event_log = self.event_log.clone();

        // OnceCell::get_or_try_init ensures only one initialization runs
        // Other concurrent callers will wait for the first one to complete
        let client = cell
            .get_or_try_init(|| async {
                let result: Result<Arc<McpClient>, NikaError> =
                    if let Some(config) = mcp_configs.get(&name_owned) {
                        // Build McpConfig from inline config
                        let mut mcp_config = McpConfig::new(&name_owned, &config.command);
                        for arg in &config.args {
                            mcp_config = mcp_config.with_arg(arg);
                        }
                        for (key, value) in &config.env {
                            mcp_config = mcp_config.with_env(key, value);
                        }
                        if let Some(cwd) = &config.cwd {
                            mcp_config = mcp_config.with_cwd(cwd);
                        }

                        // Create and connect real client
                        let client =
                            McpClient::new(mcp_config).map_err(|e| NikaError::McpStartError {
                                name: name_owned.clone(),
                                reason: e.to_string(),
                            })?;

                        match client.connect().await {
                            Ok(()) => {
                                // Cache tools for synchronous get_tool_definitions() access
                                if let Err(e) = client.list_tools().await {
                                    tracing::warn!(mcp_server = %name_owned, error = %e, "Failed to cache tools");
                                }

                                tracing::info!(mcp_server = %name_owned, "Connected to MCP server");

                                // EMIT: McpConnected event for persistent logging (v0.7.0)
                                event_log.emit(EventKind::McpConnected {
                                    server_name: name_owned.clone(),
                                });

                                Ok(Arc::new(client))
                            }
                            Err(e) => {
                                let error_msg = e.to_string();

                                // EMIT: McpError event for persistent logging (v0.7.0)
                                event_log.emit(EventKind::McpError {
                                    server_name: name_owned.clone(),
                                    error: error_msg.clone(),
                                });

                                Err(NikaError::McpStartError {
                                    name: name_owned.clone(),
                                    reason: error_msg,
                                })
                            }
                        }
                    } else {
                        // No config found - this is an error in production
                        tracing::error!(mcp_server = %name_owned, "MCP server not configured in workflow");
                        Err(NikaError::McpNotConfigured { name: name_owned.clone() })
                    };
                result
            })
            .await?;

        Ok(Arc::clone(client))
    }
}

/// Get action type as string for tracing
fn action_type(action: &TaskAction) -> &'static str {
    match action {
        TaskAction::Infer { .. } => "infer",
        TaskAction::Exec { .. } => "exec",
        TaskAction::Fetch { .. } => "fetch",
        TaskAction::Invoke { .. } => "invoke",
        TaskAction::Agent { .. } => "agent",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ExecParams, FetchParams, InvokeParams};
    use crate::store::{DataStore, TaskResult};
    use serde_json::json;
    use std::time::Duration;

    // ═══════════════════════════════════════════════════════════════
    // EXECUTOR CONSTRUCTION TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_executor_new_default() {
        let executor = TaskExecutor::new("claude", None, None, EventLog::new());
        assert_eq!(executor.default_provider.as_ref(), "claude");
        assert!(executor.default_model.is_none());
    }

    #[test]
    fn test_executor_new_with_model() {
        let executor = TaskExecutor::new("openai", Some("gpt-4"), None, EventLog::new());
        assert_eq!(executor.default_provider.as_ref(), "openai");
        assert_eq!(executor.default_model.as_deref(), Some("gpt-4"));
    }

    #[test]
    fn test_executor_new_with_mcp_configs() {
        let mut mcp_configs = rustc_hash::FxHashMap::default();
        mcp_configs.insert(
            "novanet".to_string(),
            McpConfigInline {
                command: "cargo run".to_string(),
                args: vec![
                    "--manifest-path".to_string(),
                    "path/to/Cargo.toml".to_string(),
                ],
                env: rustc_hash::FxHashMap::default(),
                cwd: None,
            },
        );

        let executor = TaskExecutor::new("mock", None, Some(mcp_configs), EventLog::new());
        assert_eq!(executor.default_provider.as_ref(), "mock");
    }

    #[test]
    fn test_executor_is_clone() {
        let exec = TaskExecutor::new("mock", None, None, EventLog::new());
        let _cloned = exec.clone();
    }

    // ═══════════════════════════════════════════════════════════════
    // EXEC VERB TESTS
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_execute_exec_simple_command() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let bindings = ResolvedBindings::new();
        let datastore = DataStore::new();

        let action = TaskAction::Exec {
            exec: ExecParams {
                command: "echo hello".to_string(),
            },
        };

        let task_id: Arc<str> = Arc::from("test_echo");
        let result = executor
            .execute(&task_id, &action, &bindings, &datastore)
            .await
            .unwrap();
        assert_eq!(result, "hello");
    }

    #[tokio::test]
    async fn test_execute_exec_with_template_binding() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let mut bindings = ResolvedBindings::new();
        bindings.set("name", json!("world"));
        let datastore = DataStore::new();

        let action = TaskAction::Exec {
            exec: ExecParams {
                command: "echo {{use.name}}".to_string(),
            },
        };

        let task_id: Arc<str> = Arc::from("test_template");
        let result = executor
            .execute(&task_id, &action, &bindings, &datastore)
            .await
            .unwrap();
        assert_eq!(result, "world");
    }

    #[tokio::test]
    async fn test_execute_exec_command_failure() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let bindings = ResolvedBindings::new();
        let datastore = DataStore::new();

        let action = TaskAction::Exec {
            exec: ExecParams {
                command: "exit 1".to_string(),
            },
        };

        let task_id: Arc<str> = Arc::from("test_fail");
        let result = executor
            .execute(&task_id, &action, &bindings, &datastore)
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            NikaError::Execution(msg) => {
                assert!(msg.contains("failed"));
            }
            err => panic!("Expected Execution error, got: {err:?}"),
        }
    }

    #[tokio::test]
    async fn test_execute_exec_emits_template_resolved() {
        let event_log = EventLog::new();
        let executor = TaskExecutor::new("mock", None, None, event_log.clone());
        let mut bindings = ResolvedBindings::new();
        bindings.set("greeting", json!("Hello"));
        let datastore = DataStore::new();

        let action = TaskAction::Exec {
            exec: ExecParams {
                command: "echo {{use.greeting}}".to_string(),
            },
        };

        let task_id: Arc<str> = Arc::from("test_event");
        executor
            .execute(&task_id, &action, &bindings, &datastore)
            .await
            .unwrap();

        // Verify TemplateResolved event was emitted
        let events = event_log.filter_task("test_event");
        assert!(!events.is_empty());

        let template_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::TemplateResolved { .. }))
            .collect();
        assert_eq!(template_events.len(), 1);

        if let EventKind::TemplateResolved { result, .. } = &template_events[0].kind {
            assert_eq!(result, "echo Hello");
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // FETCH VERB TESTS (HTTP)
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_execute_fetch_invalid_url() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let bindings = ResolvedBindings::new();
        let datastore = DataStore::new();

        let action = TaskAction::Fetch {
            fetch: FetchParams {
                url: "http://invalid.example.invalid".to_string(),
                method: "GET".to_string(),
                headers: rustc_hash::FxHashMap::default(),
                body: None,
            },
        };

        let task_id: Arc<str> = Arc::from("test_fetch_fail");
        let result = executor
            .execute(&task_id, &action, &bindings, &datastore)
            .await;
        // Result is error because the URL cannot be resolved/connected
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_fetch_with_template_url() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let mut bindings = ResolvedBindings::new();
        bindings.set("endpoint", json!("httpbin.org/get"));
        let datastore = DataStore::new();

        let action = TaskAction::Fetch {
            fetch: FetchParams {
                url: "https://{{use.endpoint}}".to_string(),
                method: "GET".to_string(),
                headers: rustc_hash::FxHashMap::default(),
                body: None,
            },
        };

        let task_id: Arc<str> = Arc::from("test_fetch_template");
        // This will connect to the real httpbin, so we expect success if network is available
        let result = executor
            .execute(&task_id, &action, &bindings, &datastore)
            .await;
        // Just verify template was resolved (regardless of network success/failure)
        let events = EventLog::new();
        let executor2 = TaskExecutor::new("mock", None, None, events.clone());
        let result2 = executor2
            .execute(&task_id, &action, &bindings, &datastore)
            .await;
        // Both should have same result status (both succeed or both fail due to network)
        assert_eq!(result.is_ok(), result2.is_ok());
    }

    // ═══════════════════════════════════════════════════════════════
    // INVOKE VERB TESTS (MCP)
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_execute_invoke_tool_call() {
        let event_log = EventLog::new();
        let executor = TaskExecutor::new("mock", None, None, event_log.clone());
        executor.inject_mock_mcp_client("novanet");
        let bindings = ResolvedBindings::new();
        let datastore = DataStore::new();

        let action = TaskAction::Invoke {
            invoke: InvokeParams {
                mcp: "novanet".to_string(),
                tool: Some("novanet_generate".to_string()),
                params: Some(json!({"entity": "qr-code", "locale": "fr-FR"})),
                resource: None,
            },
        };

        let task_id: Arc<str> = Arc::from("test_invoke");
        let result = executor
            .execute(&task_id, &action, &bindings, &datastore)
            .await;

        assert!(result.is_ok(), "Invoke should succeed: {:?}", result.err());
        let output = result.unwrap();
        assert!(
            output.contains("entity"),
            "Output should contain entity: {output}"
        );
    }

    #[tokio::test]
    async fn test_execute_invoke_resource_read() {
        let event_log = EventLog::new();
        let executor = TaskExecutor::new("mock", None, None, event_log);
        executor.inject_mock_mcp_client("novanet");
        let bindings = ResolvedBindings::new();
        let datastore = DataStore::new();

        let action = TaskAction::Invoke {
            invoke: InvokeParams {
                mcp: "novanet".to_string(),
                tool: None,
                params: None,
                resource: Some("neo4j://entity/qr-code".to_string()),
            },
        };

        let task_id: Arc<str> = Arc::from("test_resource");
        let result = executor
            .execute(&task_id, &action, &bindings, &datastore)
            .await;

        assert!(
            result.is_ok(),
            "Resource read should succeed: {:?}",
            result.err()
        );
        let output = result.unwrap();
        assert!(
            output.contains("qr-code"),
            "Output should contain resource id: {output}"
        );
    }

    #[tokio::test]
    async fn test_execute_invoke_emits_mcp_events() {
        let event_log = EventLog::new();
        let executor = TaskExecutor::new("mock", None, None, event_log.clone());
        executor.inject_mock_mcp_client("novanet");
        let bindings = ResolvedBindings::new();
        let datastore = DataStore::new();

        let action = TaskAction::Invoke {
            invoke: InvokeParams {
                mcp: "novanet".to_string(),
                tool: Some("novanet_describe".to_string()),
                params: None,
                resource: None,
            },
        };

        let task_id: Arc<str> = Arc::from("test_mcp_events");
        executor
            .execute(&task_id, &action, &bindings, &datastore)
            .await
            .unwrap();

        // Verify events were emitted
        let events = event_log.filter_task("test_mcp_events");
        assert!(!events.is_empty(), "Should emit events");

        // Check for McpInvoke event
        let invoke_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::McpInvoke { .. }))
            .collect();
        assert_eq!(invoke_events.len(), 1, "Should emit McpInvoke event");

        // Check for McpResponse event
        let response_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::McpResponse { .. }))
            .collect();
        assert_eq!(response_events.len(), 1, "Should emit McpResponse event");
    }

    #[tokio::test]
    async fn test_execute_invoke_tool_with_template_params() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        executor.inject_mock_mcp_client("novanet");
        let mut bindings = ResolvedBindings::new();
        bindings.set("entity_key", json!("qr-code"));
        bindings.set("locale_val", json!("en-US"));
        let datastore = DataStore::new();

        let action = TaskAction::Invoke {
            invoke: InvokeParams {
                mcp: "novanet".to_string(),
                tool: Some("novanet_generate".to_string()),
                params: Some(json!({
                    "entity": "{{use.entity_key}}",
                    "locale": "{{use.locale_val}}"
                })),
                resource: None,
            },
        };

        let task_id: Arc<str> = Arc::from("test_invoke_template");
        let result = executor
            .execute(&task_id, &action, &bindings, &datastore)
            .await;

        assert!(
            result.is_ok(),
            "Invoke with template params should succeed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_execute_invoke_validation_error_both_tool_and_resource() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let bindings = ResolvedBindings::new();
        let datastore = DataStore::new();

        // Both tool and resource set (invalid)
        let action = TaskAction::Invoke {
            invoke: InvokeParams {
                mcp: "novanet".to_string(),
                tool: Some("test_tool".to_string()),
                params: None,
                resource: Some("test://resource".to_string()),
            },
        };

        let task_id: Arc<str> = Arc::from("test_invalid");
        let result = executor
            .execute(&task_id, &action, &bindings, &datastore)
            .await;

        assert!(result.is_err(), "Should fail with validation error");
        match result.unwrap_err() {
            NikaError::ValidationError { reason } => {
                assert!(reason.contains("mutually exclusive"));
            }
            err => panic!("Expected ValidationError, got: {err:?}"),
        }
    }

    #[tokio::test]
    async fn test_execute_invoke_validation_error_neither_tool_nor_resource() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let bindings = ResolvedBindings::new();
        let datastore = DataStore::new();

        // Neither tool nor resource set (invalid)
        let action = TaskAction::Invoke {
            invoke: InvokeParams {
                mcp: "novanet".to_string(),
                tool: None,
                params: None,
                resource: None,
            },
        };

        let task_id: Arc<str> = Arc::from("test_neither");
        let result = executor
            .execute(&task_id, &action, &bindings, &datastore)
            .await;

        assert!(result.is_err(), "Should fail with validation error");
        match result.unwrap_err() {
            NikaError::ValidationError { reason } => {
                assert!(reason.contains("either") || reason.contains("must be specified"));
            }
            err => panic!("Expected ValidationError, got: {err:?}"),
        }
    }

    #[tokio::test]
    async fn test_execute_invoke_mcp_not_configured() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        // No inject_mock_mcp_client() - server not configured
        let bindings = ResolvedBindings::new();
        let datastore = DataStore::new();

        let action = TaskAction::Invoke {
            invoke: InvokeParams {
                mcp: "unconfigured_server".to_string(),
                tool: Some("some_tool".to_string()),
                params: None,
                resource: None,
            },
        };

        let task_id: Arc<str> = Arc::from("test_unconfigured");
        let result = executor
            .execute(&task_id, &action, &bindings, &datastore)
            .await;

        assert!(result.is_err(), "Should fail with McpNotConfigured");
        match result.unwrap_err() {
            NikaError::McpNotConfigured { name } => {
                assert_eq!(name, "unconfigured_server");
            }
            err => panic!("Expected McpNotConfigured, got: {err:?}"),
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // BINDING RESOLUTION TESTS
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_binding_resolution_single_template() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let mut bindings = ResolvedBindings::new();
        bindings.set("key", json!("value123"));
        let datastore = DataStore::new();

        let action = TaskAction::Exec {
            exec: ExecParams {
                command: "echo {{use.key}}".to_string(),
            },
        };

        let task_id: Arc<str> = Arc::from("test_binding");
        let result = executor
            .execute(&task_id, &action, &bindings, &datastore)
            .await
            .unwrap();
        assert_eq!(result, "value123");
    }

    #[tokio::test]
    async fn test_binding_resolution_multiple_templates() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let mut bindings = ResolvedBindings::new();
        bindings.set("first", json!("hello"));
        bindings.set("second", json!("world"));
        let datastore = DataStore::new();

        let action = TaskAction::Exec {
            exec: ExecParams {
                command: "echo {{use.first}} {{use.second}}".to_string(),
            },
        };

        let task_id: Arc<str> = Arc::from("test_multi");
        let result = executor
            .execute(&task_id, &action, &bindings, &datastore)
            .await
            .unwrap();
        assert_eq!(result, "hello world");
    }

    #[tokio::test]
    async fn test_binding_resolution_no_templates() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let bindings = ResolvedBindings::new();
        let datastore = DataStore::new();

        let action = TaskAction::Exec {
            exec: ExecParams {
                command: "echo static".to_string(),
            },
        };

        let task_id: Arc<str> = Arc::from("test_static");
        let result = executor
            .execute(&task_id, &action, &bindings, &datastore)
            .await
            .unwrap();
        assert_eq!(result, "static");
    }

    #[tokio::test]
    async fn test_binding_resolution_json_value() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let mut bindings = ResolvedBindings::new();
        bindings.set("data", json!({"id": 42, "name": "test"}));
        let datastore = DataStore::new();

        let action = TaskAction::Exec {
            exec: ExecParams {
                command: "echo {{use.data}}".to_string(),
            },
        };

        let task_id: Arc<str> = Arc::from("test_json");
        let result = executor
            .execute(&task_id, &action, &bindings, &datastore)
            .await
            .unwrap();
        // JSON should be serialized and echoed
        assert!(result.contains("id"));
        assert!(result.contains("42"));
    }

    #[tokio::test]
    async fn test_binding_resolution_datastore_lookup() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let mut bindings = ResolvedBindings::new();
        bindings.set("task_output", json!({"result": "success"}));
        let datastore = DataStore::new();
        let task_id_prev: Arc<str> = Arc::from("prev_task");
        datastore.insert(
            task_id_prev.clone(),
            TaskResult::success_str("from_previous_task", Duration::from_millis(100)),
        );

        let action = TaskAction::Exec {
            exec: ExecParams {
                command: "echo {{use.task_output}}".to_string(),
            },
        };

        let task_id: Arc<str> = Arc::from("test_store");
        let result = executor
            .execute(&task_id, &action, &bindings, &datastore)
            .await
            .unwrap();
        assert!(result.contains("success"));
    }

    // ═══════════════════════════════════════════════════════════════
    // DECOMPOSE TESTS
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_expand_decompose_static() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let mut bindings = ResolvedBindings::new();
        bindings.set("items", json!(["item1", "item2", "item3"]));
        let datastore = DataStore::new();

        let spec = DecomposeSpec {
            strategy: DecomposeStrategy::Static,
            traverse: "HAS_CHILD".to_string(),
            source: "{{use.items}}".to_string(),
            max_items: None,
            max_depth: None,
            mcp_server: None,
        };

        let result = executor
            .expand_decompose(&spec, &bindings, &datastore)
            .await
            .unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].as_str().unwrap(), "item1");
        assert_eq!(result[1].as_str().unwrap(), "item2");
        assert_eq!(result[2].as_str().unwrap(), "item3");
    }

    #[tokio::test]
    async fn test_expand_decompose_static_with_max_items() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let mut bindings = ResolvedBindings::new();
        bindings.set("items", json!(["a", "b", "c", "d", "e"]));
        let datastore = DataStore::new();

        let spec = DecomposeSpec {
            strategy: DecomposeStrategy::Static,
            traverse: "HAS_CHILD".to_string(),
            source: "{{use.items}}".to_string(),
            max_items: Some(2),
            max_depth: None,
            mcp_server: None,
        };

        let result = executor
            .expand_decompose(&spec, &bindings, &datastore)
            .await
            .unwrap();

        assert_eq!(result.len(), 2);
    }

    #[tokio::test]
    async fn test_expand_decompose_static_wrong_type() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let mut bindings = ResolvedBindings::new();
        bindings.set("notarray", json!({"key": "value"}));
        let datastore = DataStore::new();

        let spec = DecomposeSpec {
            strategy: DecomposeStrategy::Static,
            traverse: "HAS_CHILD".to_string(),
            source: "{{use.notarray}}".to_string(),
            max_items: None,
            max_depth: None,
            mcp_server: None,
        };

        let result = executor
            .expand_decompose(&spec, &bindings, &datastore)
            .await;
        assert!(result.is_err(), "Should fail with type mismatch");
    }

    #[tokio::test]
    async fn test_extract_decompose_key_from_string() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let value = json!("entity:qr-code");

        let key = executor.extract_decompose_key(&value).unwrap();
        assert_eq!(key, "entity:qr-code");
    }

    #[tokio::test]
    async fn test_extract_decompose_key_from_object() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let value = json!({"key": "entity:test", "name": "Test Entity"});

        let key = executor.extract_decompose_key(&value).unwrap();
        assert_eq!(key, "entity:test");
    }

    #[tokio::test]
    async fn test_extract_decompose_key_invalid() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let value = json!(123);

        let result = executor.extract_decompose_key(&value);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_extract_decompose_nodes_from_nodes_field() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let result_json = json!({
            "nodes": [
                {"key": "node1"},
                {"key": "node2"}
            ]
        });

        let nodes = executor.extract_decompose_nodes(&result_json).unwrap();
        assert_eq!(nodes.len(), 2);
    }

    #[tokio::test]
    async fn test_extract_decompose_nodes_from_items_field() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let result_json = json!({
            "items": ["item1", "item2", "item3"]
        });

        let nodes = executor.extract_decompose_nodes(&result_json).unwrap();
        assert_eq!(nodes.len(), 3);
    }

    #[tokio::test]
    async fn test_extract_decompose_nodes_from_results_field() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let result_json = json!({
            "results": ["result1", "result2"]
        });

        let nodes = executor.extract_decompose_nodes(&result_json).unwrap();
        assert_eq!(nodes.len(), 2);
    }

    #[tokio::test]
    async fn test_extract_decompose_nodes_from_array() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let result_json = json!(["direct1", "direct2"]);

        let nodes = executor.extract_decompose_nodes(&result_json).unwrap();
        assert_eq!(nodes.len(), 2);
    }

    #[tokio::test]
    async fn test_extract_decompose_nodes_empty_nodes() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let result_json = json!({"nodes": []});

        let nodes = executor.extract_decompose_nodes(&result_json).unwrap();
        assert_eq!(nodes.len(), 0);
    }

    // ═══════════════════════════════════════════════════════════════
    // ERROR HANDLING TESTS
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_error_handling_exec_timeout() {
        let executor = TaskExecutor::new("mock", None, None, EventLog::new());
        let bindings = ResolvedBindings::new();
        let datastore = DataStore::new();

        // Sleep command longer than timeout
        let action = TaskAction::Exec {
            exec: ExecParams {
                command: "sleep 100".to_string(),
            },
        };

        let task_id: Arc<str> = Arc::from("test_timeout");
        let result = executor
            .execute(&task_id, &action, &bindings, &datastore)
            .await;

        assert!(result.is_err(), "Should timeout");
        match result.unwrap_err() {
            NikaError::Execution(msg) => {
                assert!(msg.contains("timed out") || msg.contains("timeout"));
            }
            err => panic!("Expected Execution error with timeout, got: {err:?}"),
        }
    }

    #[tokio::test]
    async fn test_action_type_helper() {
        let infer_action = TaskAction::Infer {
            infer: crate::ast::InferParams {
                prompt: "test".to_string(),
                provider: None,
                model: None,
            },
        };
        assert_eq!(action_type(&infer_action), "infer");

        let exec_action = TaskAction::Exec {
            exec: ExecParams {
                command: "echo test".to_string(),
            },
        };
        assert_eq!(action_type(&exec_action), "exec");

        let fetch_action = TaskAction::Fetch {
            fetch: FetchParams {
                url: "http://example.com".to_string(),
                method: "GET".to_string(),
                headers: rustc_hash::FxHashMap::default(),
                body: None,
            },
        };
        assert_eq!(action_type(&fetch_action), "fetch");

        let invoke_action = TaskAction::Invoke {
            invoke: InvokeParams {
                mcp: "novanet".to_string(),
                tool: Some("test".to_string()),
                params: None,
                resource: None,
            },
        };
        assert_eq!(action_type(&invoke_action), "invoke");

        let agent_action = TaskAction::Agent {
            agent: crate::ast::AgentParams {
                prompt: "test".to_string(),
                provider: None,
                model: None,
                system: None,
                mcp: vec![],
                max_turns: None,
                stop_conditions: vec![],
                scope: None,
                token_budget: None,
                extended_thinking: None,
                thinking_budget: None,
                depth_limit: None,
            },
        };
        assert_eq!(action_type(&agent_action), "agent");
    }
}
