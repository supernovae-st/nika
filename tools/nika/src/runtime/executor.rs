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
    AgentParams, ExecParams, FetchParams, InferParams, InvokeParams, McpConfigInline, TaskAction,
};
use crate::binding::{ResolvedBindings, template_resolve};
use crate::error::NikaError;
use crate::event::{EventKind, EventLog};
use crate::mcp::{McpClient, McpConfig};
#[allow(deprecated)]
use crate::provider::{Provider, create_provider};
use crate::provider::rig::RigProvider;
use crate::runtime::AgentLoop;
use crate::util::{CONNECT_TIMEOUT, EXEC_TIMEOUT, FETCH_TIMEOUT, REDIRECT_LIMIT};

/// Task executor with cached providers, shared HTTP client, and event logging
#[derive(Clone)]
pub struct TaskExecutor {
    /// Shared HTTP client (connection pooling)
    http_client: reqwest::Client,
    /// Cached providers (lock-free) - DEPRECATED: Use rig_provider_cache instead
    #[allow(deprecated)]
    provider_cache: Arc<DashMap<String, Arc<dyn Provider>>>,
    /// Cached rig-core providers (v0.3.1+)
    rig_provider_cache: Arc<DashMap<String, RigProvider>>,
    /// Cached MCP clients with async-safe initialization (prevents race conditions in for_each)
    /// Uses OnceCell per server to ensure only one client is created even with concurrent access
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
        let http_client = reqwest::Client::builder()
            .timeout(FETCH_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::limited(REDIRECT_LIMIT))
            .user_agent("nika-cli/0.1")
            .build()
            .expect("Failed to build HTTP client");

        Self {
            http_client,
            provider_cache: Arc::new(DashMap::new()),
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

    /// Execute a task action with the given bindings
    #[instrument(skip(self, bindings), fields(action_type = %action_type(action)))]
    pub async fn execute(
        &self,
        task_id: &Arc<str>,
        action: &TaskAction,
        bindings: &ResolvedBindings,
    ) -> Result<String, NikaError> {
        debug!("Executing task action");
        match action {
            TaskAction::Infer { infer } => self.execute_infer(task_id, infer, bindings).await,
            TaskAction::Exec { exec } => self.execute_exec(task_id, exec, bindings).await,
            TaskAction::Fetch { fetch } => self.execute_fetch(task_id, fetch, bindings).await,
            TaskAction::Invoke { invoke } => self.execute_invoke(task_id, invoke, bindings).await,
            TaskAction::Agent { agent } => self.execute_agent(task_id, agent, bindings).await,
        }
    }

    /// Get or create a cached provider (atomic via DashMap entry API)
    ///
    /// DEPRECATED: Use `get_rig_provider` instead for new code.
    #[allow(deprecated)]
    fn get_provider(&self, name: &str) -> Result<Arc<dyn Provider>, NikaError> {
        // Use entry API for atomic get-or-insert (avoids race condition)
        use dashmap::mapref::entry::Entry;

        match self.provider_cache.entry(name.to_string()) {
            Entry::Occupied(e) => Ok(Arc::clone(e.get())),
            Entry::Vacant(e) => {
                let provider: Arc<dyn Provider> = Arc::from(
                    create_provider(name).map_err(|e| NikaError::Provider(e.to_string()))?,
                );
                e.insert(Arc::clone(&provider));
                Ok(provider)
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
                    _ => {
                        return Err(NikaError::Provider(format!(
                            "Unknown rig provider: {}. Supported: claude, openai",
                            name
                        )));
                    }
                };
                e.insert(provider.clone());
                Ok(provider)
            }
        }
    }

    async fn execute_infer(
        &self,
        task_id: &Arc<str>,
        infer: &InferParams,
        bindings: &ResolvedBindings,
    ) -> Result<String, NikaError> {
        // Resolve {{use.alias}} templates
        let prompt = template_resolve(&infer.prompt, bindings)?;

        // EMIT: TemplateResolved
        self.event_log.emit(EventKind::TemplateResolved {
            task_id: Arc::clone(task_id),
            template: infer.prompt.clone(),
            result: prompt.to_string(),
        });

        // Use task-level override or workflow default
        let provider_name = infer.provider.as_deref().unwrap_or(&self.default_provider);

        // Get cached rig provider (v0.3.1+)
        let provider = self.get_rig_provider(provider_name)?;

        // Resolve model: task override -> workflow default -> provider default
        let model = infer
            .model
            .as_deref()
            .or(self.default_model.as_deref());

        // EMIT: ProviderCalled
        self.event_log.emit(EventKind::ProviderCalled {
            task_id: Arc::clone(task_id),
            provider: provider_name.to_string(),
            model: model.unwrap_or_else(|| provider.default_model()).to_string(),
            prompt_len: prompt.len(),
        });

        let result = provider
            .infer(&prompt, model)
            .await
            .map_err(|e| NikaError::Provider(e.to_string()))?;

        // EMIT: ProviderResponded
        // TODO(v0.2): Get actual token counts from provider response
        self.event_log.emit(EventKind::ProviderResponded {
            task_id: Arc::clone(task_id),
            request_id: None, // TODO: Get from provider response
            input_tokens: 0,  // TODO: Get from provider response
            output_tokens: 0, // TODO: Get from provider response
            cache_read_tokens: 0,
            ttft_ms: None,
            finish_reason: "stop".to_string(),
            cost_usd: 0.0,
        });

        Ok(result)
    }

    async fn execute_exec(
        &self,
        task_id: &Arc<str>,
        exec: &ExecParams,
        bindings: &ResolvedBindings,
    ) -> Result<String, NikaError> {
        // Resolve {{use.alias}} templates
        let command = template_resolve(&exec.command, bindings)?;

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

    #[instrument(skip(self, bindings), fields(url = %fetch.url))]
    async fn execute_fetch(
        &self,
        task_id: &Arc<str>,
        fetch: &FetchParams,
        bindings: &ResolvedBindings,
    ) -> Result<String, NikaError> {
        // Resolve {{use.alias}} templates
        let url = template_resolve(&fetch.url, bindings)?;

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
            let resolved_value = template_resolve(value, bindings)?;
            request = request.header(key, resolved_value.as_ref());
        }

        // Add body if present
        if let Some(body) = &fetch.body {
            let resolved_body = template_resolve(body, bindings)?;
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
    #[instrument(skip(self, bindings), fields(mcp = %invoke.mcp))]
    async fn execute_invoke(
        &self,
        task_id: &Arc<str>,
        invoke: &InvokeParams,
        bindings: &ResolvedBindings,
    ) -> Result<String, NikaError> {
        // Validate invoke params (tool XOR resource)
        invoke
            .validate()
            .map_err(|e| NikaError::ValidationError { reason: e })?;

        // Generate unique call_id for correlation
        let call_id = Uuid::new_v4().to_string();
        let start_time = Instant::now();

        // EMIT: McpInvoke event
        self.event_log.emit(EventKind::McpInvoke {
            task_id: Arc::clone(task_id),
            call_id: call_id.clone(),
            mcp_server: invoke.mcp.clone(),
            tool: invoke.tool.clone(),
            resource: invoke.resource.clone(),
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
                let resolved_str = template_resolve(&params_str, bindings)?;
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
                self.event_log.emit(EventKind::McpResponse {
                    task_id: Arc::clone(task_id),
                    call_id: call_id.clone(),
                    output_len: tool_result.text().len(),
                    duration_ms,
                    cached: false,
                    is_error: true,
                });
                return Err(NikaError::McpToolError {
                    tool: tool.clone(),
                    reason: tool_result.text(),
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

        // EMIT: McpResponse event
        let duration_ms = start_time.elapsed().as_millis() as u64;
        self.event_log.emit(EventKind::McpResponse {
            task_id: Arc::clone(task_id),
            call_id,
            output_len: result.to_string().len(),
            duration_ms,
            cached: false, // TODO: Implement MCP response caching
            is_error,
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
    #[instrument(skip(self, bindings), fields(max_turns = %agent.effective_max_turns()))]
    async fn execute_agent(
        &self,
        task_id: &Arc<str>,
        agent: &AgentParams,
        bindings: &ResolvedBindings,
    ) -> Result<String, NikaError> {
        // Resolve {{use.alias}} templates in prompt
        let resolved_prompt = template_resolve(&agent.prompt, bindings)?;

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

        // Get provider (task override or workflow default)
        let provider_name = resolved_agent
            .provider
            .as_deref()
            .unwrap_or(&self.default_provider);
        let provider = self.get_provider(provider_name)?;

        // Build MCP client map for this agent
        let mut mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();
        for mcp_name in &resolved_agent.mcp {
            let client = self.get_mcp_client(mcp_name).await?;
            mcp_clients.insert(mcp_name.clone(), client);
        }

        // Create and run agent loop with resolved prompt
        let agent_loop = AgentLoop::new(
            task_id.to_string(),
            resolved_agent,
            self.event_log.clone(),
            mcp_clients,
        )?;

        let start = std::time::Instant::now();
        let result = agent_loop.run(provider).await?;
        let duration_ms = start.elapsed().as_millis() as u64;

        // EMIT: AgentComplete event
        self.event_log.emit(EventKind::AgentComplete {
            task_id: Arc::clone(task_id),
            turns: result.turns,
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

                        client.connect().await.map_err(|e| NikaError::McpStartError {
                            name: name_owned.clone(),
                            reason: e.to_string(),
                        })?;

                        tracing::info!(mcp_server = %name_owned, "Connected to MCP server");
                        Ok(Arc::new(client))
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
    use crate::ast::{ExecParams, InvokeParams};
    use serde_json::json;

    #[test]
    fn executor_is_clone() {
        let exec = TaskExecutor::new("mock", None, None, EventLog::new());
        let _cloned = exec.clone();
    }

    #[tokio::test]
    async fn execute_exec_echo() {
        let exec = TaskExecutor::new("mock", None, None, EventLog::new());
        let bindings = ResolvedBindings::new();
        let action = TaskAction::Exec {
            exec: ExecParams {
                command: "echo hello".to_string(),
            },
        };

        let task_id: Arc<str> = Arc::from("test_task");
        let result = exec.execute(&task_id, &action, &bindings).await.unwrap();
        assert_eq!(result, "hello");
    }

    #[tokio::test]
    async fn execute_exec_with_template() {
        let exec = TaskExecutor::new("mock", None, None, EventLog::new());
        let mut bindings = ResolvedBindings::new();
        bindings.set("name", json!("world"));

        let action = TaskAction::Exec {
            exec: ExecParams {
                command: "echo {{use.name}}".to_string(),
            },
        };

        let task_id: Arc<str> = Arc::from("test_task");
        let result = exec.execute(&task_id, &action, &bindings).await.unwrap();
        assert_eq!(result, "world");
    }

    #[tokio::test]
    async fn execute_emits_template_resolved() {
        let event_log = EventLog::new();
        let exec = TaskExecutor::new("mock", None, None, event_log.clone());
        let mut bindings = ResolvedBindings::new();
        bindings.set("name", json!("Alice"));

        let action = TaskAction::Exec {
            exec: ExecParams {
                command: "echo Hello {{use.name}}".to_string(),
            },
        };

        let task_id: Arc<str> = Arc::from("greet");
        exec.execute(&task_id, &action, &bindings).await.unwrap();

        // Check TemplateResolved event was emitted
        let events = event_log.filter_task("greet");
        assert!(!events.is_empty());

        let template_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::TemplateResolved { .. }))
            .collect();
        assert_eq!(template_events.len(), 1);

        if let EventKind::TemplateResolved { result, .. } = &template_events[0].kind {
            assert_eq!(result, "echo Hello Alice");
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // INVOKE VERB TESTS (v0.2)
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn execute_invoke_tool_call() {
        let event_log = EventLog::new();
        let exec = TaskExecutor::new("mock", None, None, event_log.clone());
        exec.inject_mock_mcp_client("novanet"); // Explicit mock injection
        let bindings = ResolvedBindings::new();

        let action = TaskAction::Invoke {
            invoke: InvokeParams {
                mcp: "novanet".to_string(),
                tool: Some("novanet_generate".to_string()),
                params: Some(json!({"entity": "qr-code", "locale": "fr-FR"})),
                resource: None,
            },
        };

        let task_id: Arc<str> = Arc::from("invoke_test");
        let result = exec.execute(&task_id, &action, &bindings).await;

        assert!(
            result.is_ok(),
            "Invoke tool call should succeed: {:?}",
            result.err()
        );
        let output = result.unwrap();
        assert!(
            output.contains("entity"),
            "Output should contain entity: {output}"
        );
    }

    #[tokio::test]
    async fn execute_invoke_resource_read() {
        let event_log = EventLog::new();
        let exec = TaskExecutor::new("mock", None, None, event_log.clone());
        exec.inject_mock_mcp_client("novanet"); // Explicit mock injection
        let bindings = ResolvedBindings::new();

        let action = TaskAction::Invoke {
            invoke: InvokeParams {
                mcp: "novanet".to_string(),
                tool: None,
                params: None,
                resource: Some("neo4j://entity/qr-code".to_string()),
            },
        };

        let task_id: Arc<str> = Arc::from("resource_test");
        let result = exec.execute(&task_id, &action, &bindings).await;

        assert!(
            result.is_ok(),
            "Invoke resource read should succeed: {:?}",
            result.err()
        );
        let output = result.unwrap();
        assert!(
            output.contains("qr-code"),
            "Output should contain entity id: {output}"
        );
    }

    #[tokio::test]
    async fn execute_invoke_emits_mcp_events() {
        let event_log = EventLog::new();
        let exec = TaskExecutor::new("mock", None, None, event_log.clone());
        exec.inject_mock_mcp_client("novanet"); // Explicit mock injection
        let bindings = ResolvedBindings::new();

        let action = TaskAction::Invoke {
            invoke: InvokeParams {
                mcp: "novanet".to_string(),
                tool: Some("novanet_describe".to_string()),
                params: None,
                resource: None,
            },
        };

        let task_id: Arc<str> = Arc::from("mcp_events_test");
        exec.execute(&task_id, &action, &bindings).await.unwrap();

        // Check MCP events were emitted
        let events = event_log.filter_task("mcp_events_test");
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
    async fn execute_invoke_validation_error() {
        let event_log = EventLog::new();
        let exec = TaskExecutor::new("mock", None, None, event_log);
        let bindings = ResolvedBindings::new();

        // Both tool and resource set (invalid)
        let action = TaskAction::Invoke {
            invoke: InvokeParams {
                mcp: "novanet".to_string(),
                tool: Some("test".to_string()),
                params: None,
                resource: Some("test://resource".to_string()),
            },
        };

        let task_id: Arc<str> = Arc::from("invalid_test");
        let result = exec.execute(&task_id, &action, &bindings).await;

        assert!(result.is_err(), "Should fail with validation error");
        match result.unwrap_err() {
            NikaError::ValidationError { reason } => {
                assert!(reason.contains("mutually exclusive"));
            }
            err => panic!("Expected ValidationError, got: {err:?}"),
        }
    }

    #[tokio::test]
    async fn execute_invoke_mcp_not_configured_error() {
        let event_log = EventLog::new();
        let exec = TaskExecutor::new("mock", None, None, event_log);
        // NOTE: No inject_mock_mcp_client() - should fail with McpNotConfigured
        let bindings = ResolvedBindings::new();

        let action = TaskAction::Invoke {
            invoke: InvokeParams {
                mcp: "unknown_server".to_string(),
                tool: Some("some_tool".to_string()),
                params: None,
                resource: None,
            },
        };

        let task_id: Arc<str> = Arc::from("unconfigured_test");
        let result = exec.execute(&task_id, &action, &bindings).await;

        assert!(result.is_err(), "Should fail with McpNotConfigured");
        match result.unwrap_err() {
            NikaError::McpNotConfigured { name } => {
                assert_eq!(name, "unknown_server");
            }
            err => panic!("Expected McpNotConfigured, got: {err:?}"),
        }
    }
}
