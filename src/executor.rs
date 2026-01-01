//! Task executor with provider caching (v0.1)
//!
//! Handles execution of individual tasks: infer, exec, fetch.
//! Uses DashMap for lock-free provider caching.

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tracing::{debug, instrument};

use crate::context::TaskContext;
use crate::error::NikaError;
use crate::provider::{create_provider, Provider};
use crate::task::{ExecDef, FetchDef, InferDef};
use crate::template;
use crate::workflow::TaskAction;

/// Default timeout for exec commands (60 seconds)
const EXEC_TIMEOUT: Duration = Duration::from_secs(60);
/// Default timeout for HTTP requests (30 seconds)
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Task executor with cached providers and shared HTTP client
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
}

impl TaskExecutor {
    /// Create a new executor with default provider and model
    pub fn new(provider: &str, model: Option<&str>) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(FETCH_TIMEOUT)
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent("nika-cli/0.1")
            .build()
            .expect("Failed to build HTTP client");

        Self {
            http_client,
            provider_cache: Arc::new(DashMap::new()),
            default_provider: provider.into(),
            default_model: model.map(Into::into),
        }
    }

    /// Execute a task action with the given context
    #[instrument(skip(self, context), fields(action_type = %action_type(action)))]
    pub async fn execute(
        &self,
        action: &TaskAction,
        context: &TaskContext,
    ) -> Result<String, NikaError> {
        debug!("Executing task action");
        match action {
            TaskAction::Infer { infer } => self.execute_infer(infer, context).await,
            TaskAction::Exec { exec } => self.execute_exec(exec, context).await,
            TaskAction::Fetch { fetch } => self.execute_fetch(fetch, context).await,
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
        infer: &InferDef,
        context: &TaskContext,
    ) -> Result<String, NikaError> {
        // Resolve {{use.alias}} templates
        let prompt = template::resolve(&infer.prompt, context)?;

        // Use task-level override or workflow default
        let provider_name = infer
            .provider
            .as_deref()
            .unwrap_or(&self.default_provider);

        // Get cached provider (or create and cache)
        let provider = self.get_provider(provider_name)?;

        // Resolve model: task override -> workflow default -> provider default
        let model = infer
            .model
            .as_deref()
            .or(self.default_model.as_deref())
            .unwrap_or_else(|| provider.default_model());

        provider
            .infer(&prompt, model)
            .await
            .map_err(|e| NikaError::Provider(e.to_string()))
    }

    async fn execute_exec(
        &self,
        exec: &ExecDef,
        context: &TaskContext,
    ) -> Result<String, NikaError> {
        // Resolve {{use.alias}} templates
        let command = template::resolve(&exec.command, context)?;

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

    #[instrument(skip(self, context), fields(url = %fetch.url))]
    async fn execute_fetch(
        &self,
        fetch: &FetchDef,
        context: &TaskContext,
    ) -> Result<String, NikaError> {
        // Resolve {{use.alias}} templates
        let url = template::resolve(&fetch.url, context)?;

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
            let resolved_value = template::resolve(value, context)?;
            request = request.header(key, resolved_value.as_ref());
        }

        // Add body if present
        if let Some(body) = &fetch.body {
            let resolved_body = template::resolve(body, context)?;
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
}

/// Get action type as string for tracing
fn action_type(action: &TaskAction) -> &'static str {
    match action {
        TaskAction::Infer { .. } => "infer",
        TaskAction::Exec { .. } => "exec",
        TaskAction::Fetch { .. } => "fetch",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn executor_is_clone() {
        let exec = TaskExecutor::new("mock", None);
        let _cloned = exec.clone();
    }

    #[tokio::test]
    async fn execute_exec_echo() {
        let exec = TaskExecutor::new("mock", None);
        let ctx = TaskContext::new();
        let action = TaskAction::Exec {
            exec: ExecDef {
                command: "echo hello".to_string(),
            },
        };

        let result = exec.execute(&action, &ctx).await.unwrap();
        assert_eq!(result, "hello");
    }

    #[tokio::test]
    async fn execute_exec_with_template() {
        let exec = TaskExecutor::new("mock", None);
        let mut ctx = TaskContext::new();
        ctx.set("name", json!("world"));

        let action = TaskAction::Exec {
            exec: ExecDef {
                command: "echo {{use.name}}".to_string(),
            },
        };

        let result = exec.execute(&action, &ctx).await.unwrap();
        assert_eq!(result, "world");
    }
}
