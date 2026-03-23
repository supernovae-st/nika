//! Task Executor - individual task execution
//!
//! Handles execution of individual tasks: infer, exec, fetch, invoke, agent.
//! Uses DashMap for lock-free provider caching.
//!
//! ## Module Organization
//! - `mod.rs`: TaskExecutor struct, constructors, dispatch, shared helpers
//! - `verbs.rs`: Five verb implementations (run_infer, run_exec, run_fetch, run_invoke, run_agent)
//! - `decompose.rs`: Decompose expansion strategies (semantic, static, nested)

mod decompose;
mod extract;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_extract_e2e;
#[cfg(test)]
mod tests_extraction_e2e;
#[cfg(test)]
mod tests_wiremock;
mod verbs;

use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use std::sync::Arc;

use dashmap::DashMap;
use tokio_util::sync::CancellationToken;
use tracing::{debug, instrument};

use crate::ast::output::{OutputFormat, OutputPolicy, SchemaRef};
use crate::ast::{McpConfigInline, TaskAction};
use crate::binding::ResolvedBindings;
use crate::error::NikaError;
use crate::event::{EventKind, EventLog};
use crate::mcp::{McpClient, McpClientPool};
use crate::media::CasStore;
use crate::provider::rig::RigProvider;
use crate::runtime::boot::PolicyConfig;
use crate::runtime::builtin::media::context::MediaToolContext;
use crate::runtime::policy::PolicyEnforcer;
use crate::runtime::BuiltinToolRouter;
use crate::runtime::SkillInjector;
use crate::store::RunContext;
use crate::tools::{PermissionMode, ToolContext};
use crate::util::{CONNECT_TIMEOUT, FETCH_TIMEOUT, REDIRECT_LIMIT};

/// Task executor with cached providers, shared HTTP client, and event logging
#[derive(Clone)]
pub struct TaskExecutor {
    /// Shared HTTP client (connection pooling)
    http_client: reqwest::Client,
    /// Cached rig-core providers
    rig_provider_cache: Arc<DashMap<String, RigProvider>>,
    /// Centralized MCP client pool
    ///
    /// Replaces the previous `mcp_client_cache` + `mcp_configs` pair.
    /// Handles lazy initialization, per-server deduplication via DashMap + OnceCell,
    /// and graceful shutdown. Shared across TaskExecutor, TUI App, and ChatAgent.
    mcp_pool: McpClientPool,
    /// Default provider name
    default_provider: Arc<str>,
    /// Default model
    default_model: Option<Arc<str>>,
    /// Event log for fine-grained audit trail
    event_log: EventLog,
    /// Router for builtin nika:* tools
    builtin_router: Arc<BuiltinToolRouter>,
    /// Policy enforcer for security checks
    policy_enforcer: Arc<parking_lot::RwLock<PolicyEnforcer>>,
    /// Cancellation token for aborting in-flight operations
    ///
    /// When cancelled, MCP invoke operations race against this token
    /// so they can abort promptly instead of waiting for INVOKE_TASK_DEADLINE.
    cancel_token: CancellationToken,
    /// CAS store for reading media blobs (used by vision content resolution)
    cas: Arc<CasStore>,
    /// Tool context for setting permission mode after construction
    tool_ctx: Arc<ToolContext>,
    /// Shared SkillInjector for loading and caching skill files
    skill_injector: Arc<SkillInjector>,
    /// Workflow-level skills mapping (alias -> file path)
    skills_map: std::collections::HashMap<String, String>,
    /// Base directory for resolving relative skill paths
    workflow_base_dir: std::path::PathBuf,
}

impl TaskExecutor {
    /// Create a new executor with default provider, model, MCP configs, and event log
    pub fn new(
        provider: &str,
        model: Option<&str>,
        mcp_configs: Option<FxHashMap<String, McpConfigInline>>,
        event_log: EventLog,
    ) -> Result<Self, NikaError> {
        Self::with_policy(provider, model, mcp_configs, event_log, None, None)
    }

    /// Create a new executor with explicit policy configuration.
    ///
    /// Returns an error if the media compute pool cannot be created.
    pub fn with_policy(
        provider: &str,
        model: Option<&str>,
        mcp_configs: Option<FxHashMap<String, McpConfigInline>>,
        event_log: EventLog,
        policy_config: Option<PolicyConfig>,
        permission_mode: Option<PermissionMode>,
    ) -> Result<Self, NikaError> {
        // SAFETY: ClientBuilder::build() only fails with custom TLS or proxy config.
        // We use defaults, so this is effectively infallible.
        //
        // Custom redirect policy: check each hop against SSRF blocklist to prevent
        // SSRF bypass via HTTP redirect (e.g., external → 169.254.169.254).
        let ssrf_redirect_policy = reqwest::redirect::Policy::custom(|attempt| {
            use crate::runtime::policy::is_ssrf_blocked;

            if attempt.previous().len() >= REDIRECT_LIMIT {
                attempt.stop()
            } else {
                let blocked = attempt.url().host_str().and_then(|host| {
                    let h = host.to_lowercase();
                    let h_normalized = h.trim_start_matches('[').trim_end_matches(']');
                    if is_ssrf_blocked(h_normalized) {
                        Some(h)
                    } else {
                        None
                    }
                });
                if let Some(host) = blocked {
                    attempt.error(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        format!("SSRF protection: redirect to '{}' blocked", host),
                    ))
                } else {
                    attempt.follow()
                }
            }
        });
        let http_client = reqwest::Client::builder()
            .timeout(FETCH_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(ssrf_redirect_policy)
            .user_agent(format!("nika/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("HTTP client build with default TLS is infallible");

        let policy_enforcer = PolicyEnforcer::new(policy_config.unwrap_or_default());

        // Create ToolContext for file tools
        let working_dir = std::env::current_dir().unwrap_or_else(|_| {
            tracing::warn!("Failed to get current directory, using /tmp");
            std::path::PathBuf::from("/tmp")
        });
        let perm = permission_mode.unwrap_or(PermissionMode::Plan);
        tracing::debug!(?perm, "File tools using PermissionMode");
        let tool_ctx = Arc::new(ToolContext::new(working_dir.clone(), perm));

        // Create media tool context with CAS store at workspace default
        let media_ctx = Arc::new(MediaToolContext::new(CasStore::workspace_default(
            &working_dir,
        ))?);
        // Separate CAS handle for vision content resolution (same directory)
        let cas = Arc::new(CasStore::workspace_default(&working_dir));

        Ok(Self {
            http_client,
            rig_provider_cache: Arc::new(DashMap::new()),
            mcp_pool: McpClientPool::with_configs(
                event_log.clone(),
                mcp_configs.unwrap_or_default(),
            ),
            default_provider: provider.into(),
            default_model: model.map(Into::into),
            event_log,
            builtin_router: Arc::new(BuiltinToolRouter::with_all_tools(
                tool_ctx.clone(),
                media_ctx,
            )),
            policy_enforcer: Arc::new(RwLock::new(policy_enforcer)),
            cancel_token: CancellationToken::new(),
            cas,
            tool_ctx,
            skill_injector: Arc::new(SkillInjector::new()),
            skills_map: std::collections::HashMap::new(),
            workflow_base_dir: working_dir,
        })
    }

    /// Set the permission mode for file tools (nika:write, nika:edit, etc.)
    pub fn set_permission_mode(&self, mode: PermissionMode) {
        self.tool_ctx.set_permission_mode(mode);
    }

    /// Set a cancellation token for aborting in-flight operations.
    ///
    /// When the token is cancelled, MCP invoke operations will abort promptly
    /// instead of waiting for the full INVOKE_TASK_DEADLINE timeout.
    pub fn with_cancel_token(mut self, token: CancellationToken) -> Self {
        self.cancel_token = token;
        self
    }

    /// Check if the executor has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    /// Set the workflow-level skills mapping for agent skill injection.
    ///
    /// When set, agents with `skills:` configured will have skill content
    /// loaded and prepended to their system prompts via `SkillInjector`.
    pub fn with_skills(
        mut self,
        skills_map: std::collections::HashMap<String, String>,
        base_dir: std::path::PathBuf,
    ) -> Self {
        self.skills_map = skills_map;
        self.workflow_base_dir = base_dir;
        self
    }

    /// Inject a mock MCP client for testing
    ///
    /// This allows tests to use mock clients without relying on automatic fallback.
    /// Call this after creating the executor but before executing invoke actions.
    #[cfg(test)]
    pub fn inject_mock_mcp_client(&self, name: &str) {
        self.mcp_pool
            .inject_mock(name, Arc::new(McpClient::mock(name)));
    }

    /// Build JSON schema instruction for LLM prompts
    ///
    /// When output policy requires JSON format with a schema, this generates
    /// an instruction string to append to the prompt, telling the LLM to
    /// output valid JSON conforming to the schema.
    pub(super) fn build_json_schema_instruction(
        output_policy: Option<&OutputPolicy>,
    ) -> Option<String> {
        let policy = output_policy?;
        if policy.format != OutputFormat::Json {
            return None;
        }
        let schema_ref = policy.schema.as_ref()?;
        let schema_json = match schema_ref {
            SchemaRef::Inline(v) => v.clone(),
            SchemaRef::File(_) => {
                return Some(
                    "\n\n---\n\
                     CRITICAL OUTPUT REQUIREMENT:\n\
                     Your response MUST be valid JSON.\n\n\
                     Rules:\n\
                     - Output ONLY the JSON object, no additional text\n\
                     - Do NOT wrap in markdown code blocks (no ```json)\n\
                     - Ensure all JSON is properly formatted and valid"
                        .to_string(),
                );
            }
        };
        let schema_str = serde_json::to_string_pretty(&schema_json).unwrap_or_default();
        Some(format!(
            "\n\n---\n\
             CRITICAL OUTPUT REQUIREMENT:\n\
             Your response MUST be valid JSON that conforms to this schema:\n\n\
             ```json\n{}\n```\n\n\
             Rules:\n\
             - Output ONLY the JSON object, no additional text before or after\n\
             - Do NOT wrap your response in markdown code blocks (no ```json)\n\
             - All required fields must be present\n\
             - Field types must match the schema exactly",
            schema_str
        ))
    }

    /// Run a task action with the given bindings
    ///
    /// The datastore is required for resolving lazy bindings during template substitution.
    /// The output_policy is used to inject JSON schema instructions into prompts for infer/agent.
    #[instrument(skip(self, bindings, datastore, output_policy), fields(action_type = %action_type(action)))]
    pub async fn execute(
        &self,
        task_id: &Arc<str>,
        action: &TaskAction,
        bindings: &ResolvedBindings,
        datastore: &RunContext,
        output_policy: Option<&OutputPolicy>,
    ) -> Result<String, NikaError> {
        debug!("Running task action");
        match action {
            TaskAction::Infer { infer } => {
                self.run_infer(task_id, infer, bindings, datastore, output_policy)
                    .await
            }
            TaskAction::Exec { exec: e } => self.run_exec(task_id, e, bindings, datastore).await,
            TaskAction::Fetch { fetch } => {
                self.run_fetch(task_id, fetch, bindings, datastore).await
            }
            TaskAction::Invoke { invoke } => {
                self.run_invoke(task_id, invoke, bindings, datastore).await
            }
            TaskAction::Agent { agent } => {
                self.run_agent(task_id, agent, bindings, datastore, output_policy)
                    .await
            }
        }
    }

    /// Get or create a cached rig-core provider.
    ///
    /// Resolves provider names and aliases via [`RigProvider::from_name()`],
    /// which uses `core::find_provider()` as the single source of truth.
    pub(super) fn get_rig_provider(&self, name: &str) -> Result<RigProvider, NikaError> {
        use dashmap::mapref::entry::Entry;

        // Normalize provider name so aliases ("claude") and canonical ("anthropic")
        // share the same cache entry, avoiding double-instantiation.
        let canonical = crate::core::find_provider(name)
            .map(|p| p.id)
            .unwrap_or(name);

        match self.rig_provider_cache.entry(canonical.to_string()) {
            Entry::Occupied(e) => Ok(e.get().clone()),
            Entry::Vacant(e) => {
                let provider = RigProvider::from_name(name)?;
                e.insert(provider.clone());
                // EMIT: ProviderInitialized (cache miss — first use)
                self.event_log.emit(EventKind::ProviderInitialized {
                    provider: canonical.to_string(),
                    model: provider.default_model().to_string(),
                    cached: false,
                });
                Ok(provider)
            }
        }
    }

    /// Get the default provider name.
    pub fn default_provider(&self) -> &str {
        &self.default_provider
    }

    /// Get or create an MCP client for a named server
    ///
    /// Uses OnceCell per server to ensure thread-safe initialization.
    /// Even with concurrent for_each iterations, only one client is created per server.
    ///
    /// Delegates to [`McpClientPool::get_or_connect`] which handles lazy initialization,
    /// per-server deduplication via DashMap + OnceCell, and event logging.
    pub(super) async fn get_mcp_client(&self, name: &str) -> Result<Arc<McpClient>, NikaError> {
        self.mcp_pool.get_or_connect(name).await.map_err(Into::into)
    }
}

/// Get action type as string for tracing
pub(super) fn action_type(action: &TaskAction) -> &'static str {
    match action {
        TaskAction::Infer { .. } => "infer",
        TaskAction::Exec { .. } => "exec",
        TaskAction::Fetch { .. } => "fetch",
        TaskAction::Invoke { .. } => "invoke",
        TaskAction::Agent { .. } => "agent",
    }
}
