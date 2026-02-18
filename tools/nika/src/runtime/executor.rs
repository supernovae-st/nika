//! Task Executor - individual task execution (v0.2)
//!
//! Handles execution of individual tasks: infer, exec, fetch, invoke.
//! Uses DashMap for lock-free provider caching.

use std::sync::Arc;

use dashmap::DashMap;
use tracing::{debug, instrument};

use crate::ast::{ExecParams, FetchParams, InferParams, InvokeParams, TaskAction};
use crate::binding::{template_resolve, UseBindings};
use crate::error::NikaError;
use crate::event::{EventKind, EventLog};
use crate::mcp::McpClient;
use crate::provider::{create_provider, Provider};
use crate::util::{CONNECT_TIMEOUT, EXEC_TIMEOUT, FETCH_TIMEOUT, REDIRECT_LIMIT};

/// Task executor with cached providers, shared HTTP client, and event logging
#[derive(Clone)]
pub struct TaskExecutor {
    /// Shared HTTP client (connection pooling)
    http_client: reqwest::Client,
    /// Cached providers (lock-free)
    provider_cache: Arc<DashMap<String, Arc<dyn Provider>>>,
    /// Default provider name
    default_provider: Arc<str>,
    /// Default model
    default_model: Option<Arc<str>>,
    /// Event log for fine-grained audit trail
    event_log: EventLog,
}

impl TaskExecutor {
    /// Create a new executor with default provider, model, and event log
    pub fn new(provider: &str, model: Option<&str>, event_log: EventLog) -> Self {
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
            default_provider: provider.into(),
            default_model: model.map(Into::into),
            event_log,
        }
    }

    /// Execute a task action with the given bindings
    #[instrument(skip(self, bindings), fields(action_type = %action_type(action)))]
    pub async fn execute(
        &self,
        task_id: &Arc<str>,
        action: &TaskAction,
        bindings: &UseBindings,
    ) -> Result<String, NikaError> {
        debug!("Executing task action");
        match action {
            TaskAction::Infer { infer } => self.execute_infer(task_id, infer, bindings).await,
            TaskAction::Exec { exec } => self.execute_exec(task_id, exec, bindings).await,
            TaskAction::Fetch { fetch } => self.execute_fetch(task_id, fetch, bindings).await,
            TaskAction::Invoke { invoke } => {
                self.execute_invoke(task_id, invoke).await
            }
        }
    }

    /// Get or create a cached provider (atomic via DashMap entry API)
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

    async fn execute_infer(
        &self,
        task_id: &Arc<str>,
        infer: &InferParams,
        bindings: &UseBindings,
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

        // Get cached provider (or create and cache)
        let provider = self.get_provider(provider_name)?;

        // Resolve model: task override -> workflow default -> provider default
        let model = infer
            .model
            .as_deref()
            .or(self.default_model.as_deref())
            .unwrap_or_else(|| provider.default_model());

        // EMIT: ProviderCalled
        self.event_log.emit(EventKind::ProviderCalled {
            task_id: Arc::clone(task_id),
            provider: provider_name.to_string(),
            model: model.to_string(),
            prompt_len: prompt.len(),
        });

        let result = provider
            .infer(&prompt, model)
            .await
            .map_err(|e| NikaError::Provider(e.to_string()))?;

        // EMIT: ProviderResponded
        self.event_log.emit(EventKind::ProviderResponded {
            task_id: Arc::clone(task_id),
            output_len: result.len(),
            tokens_used: None, // TODO(v0.2): Add token counting to Provider trait
        });

        Ok(result)
    }

    async fn execute_exec(
        &self,
        task_id: &Arc<str>,
        exec: &ExecParams,
        bindings: &UseBindings,
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
        bindings: &UseBindings,
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
    ///
    /// # Note
    ///
    /// Currently uses mock MCP client since real MCP manager is not yet available.
    /// The mock client provides canned responses for testing and development.
    #[instrument(skip(self), fields(mcp = %invoke.mcp))]
    async fn execute_invoke(
        &self,
        task_id: &Arc<str>,
        invoke: &InvokeParams,
    ) -> Result<String, NikaError> {
        // Validate invoke params (tool XOR resource)
        invoke
            .validate()
            .map_err(|e| NikaError::ValidationError { reason: e })?;

        // EMIT: McpInvoke event
        self.event_log.emit(EventKind::McpInvoke {
            task_id: Arc::clone(task_id),
            mcp_server: invoke.mcp.clone(),
            tool: invoke.tool.clone(),
            resource: invoke.resource.clone(),
        });

        // For now, use mock client since we don't have MCP manager yet
        // TODO(v0.2): Replace with MCP manager that maintains real connections
        let client = McpClient::mock(&invoke.mcp);

        let result = if let Some(tool) = &invoke.tool {
            // Tool call path
            let params = invoke.params.clone().unwrap_or(serde_json::Value::Null);
            let tool_result = client.call_tool(tool, params).await?;

            // Check if tool returned an error
            if tool_result.is_error {
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
        self.event_log.emit(EventKind::McpResponse {
            task_id: Arc::clone(task_id),
            output_len: result.to_string().len(),
        });

        // Return JSON string representation
        Ok(result.to_string())
    }
}

/// Get action type as string for tracing
fn action_type(action: &TaskAction) -> &'static str {
    match action {
        TaskAction::Infer { .. } => "infer",
        TaskAction::Exec { .. } => "exec",
        TaskAction::Fetch { .. } => "fetch",
        TaskAction::Invoke { .. } => "invoke",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ExecParams, InvokeParams};
    use serde_json::json;

    #[test]
    fn executor_is_clone() {
        let exec = TaskExecutor::new("mock", None, EventLog::new());
        let _cloned = exec.clone();
    }

    #[tokio::test]
    async fn execute_exec_echo() {
        let exec = TaskExecutor::new("mock", None, EventLog::new());
        let bindings = UseBindings::new();
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
        let exec = TaskExecutor::new("mock", None, EventLog::new());
        let mut bindings = UseBindings::new();
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
        let exec = TaskExecutor::new("mock", None, event_log.clone());
        let mut bindings = UseBindings::new();
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
        let exec = TaskExecutor::new("mock", None, event_log.clone());
        let bindings = UseBindings::new();

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

        assert!(result.is_ok(), "Invoke tool call should succeed: {:?}", result.err());
        let output = result.unwrap();
        assert!(output.contains("entity"), "Output should contain entity: {output}");
    }

    #[tokio::test]
    async fn execute_invoke_resource_read() {
        let event_log = EventLog::new();
        let exec = TaskExecutor::new("mock", None, event_log.clone());
        let bindings = UseBindings::new();

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

        assert!(result.is_ok(), "Invoke resource read should succeed: {:?}", result.err());
        let output = result.unwrap();
        assert!(output.contains("qr-code"), "Output should contain entity id: {output}");
    }

    #[tokio::test]
    async fn execute_invoke_emits_mcp_events() {
        let event_log = EventLog::new();
        let exec = TaskExecutor::new("mock", None, event_log.clone());
        let bindings = UseBindings::new();

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
        let exec = TaskExecutor::new("mock", None, event_log);
        let bindings = UseBindings::new();

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
}
