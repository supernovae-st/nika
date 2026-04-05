//! Task Executor - individual task execution
//!
//! Handles execution of individual tasks: infer, exec, fetch, invoke, agent.
//! Uses DashMap for lock-free provider caching.
//!
//! ## Module Organization
//! - `mod.rs`: TaskExecutor struct, constructors, dispatch, shared helpers
//! - `verbs.rs`: Shared helper functions (estimate_tokens, coerce_json_types, etc.)
//! - `infer.rs`: `run_infer` + `run_infer_vision` + guardrails
//! - `exec.rs`: `run_exec` (shell command execution)
//! - `fetch.rs`: `run_fetch` (HTTP requests)
//! - `invoke.rs`: `run_invoke` (MCP tool calls / resource reads)
//! - `agent.rs`: `run_agent` (multi-turn agentic loops)
//! - `decompose.rs`: Decompose expansion strategies (semantic, static, nested)

mod agent;
mod decompose;
mod exec;
mod extract;
mod fetch;
mod infer;
mod invoke;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_extract_e2e;
#[cfg(test)]
mod tests_wiremock;
pub(crate) mod verbs;

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
    /// Arc-wrapped to avoid cloning the map for each spawned task.
    skills_map: Arc<std::collections::HashMap<String, String>>,
    /// Base directory for resolving relative skill paths
    workflow_base_dir: std::path::PathBuf,
    /// Project root directory (parent of nika.toml), used by working_dir_mode "project"
    project_root: Option<std::path::PathBuf>,
    /// Working directory mode from `[tools] working_dir` in nika.toml.
    ///
    /// - `"project"` → exec tasks default to project_root as cwd
    /// - `"workflow"` → exec tasks default to workflow_base_dir (parent of .nika.yaml)
    /// - `"none"` → no default cwd, inherit process cwd
    /// - `None` → same as "workflow" (backward compatible default)
    working_dir_mode: Option<String>,
    /// Custom endpoints for OpenAI-compatible servers (vLLM, TGI, Ollama)
    custom_endpoints: Arc<crate::provider::endpoints::CustomEndpointMap>,
    /// Resolved agent presets from the workflow's `agents:` block
    resolved_agents: Arc<crate::runtime::resolver::ResolvedAgents>,
    /// robots.txt compliance cache — shared across all fetch tasks in a workflow.
    robots_cache: Option<Arc<crate::runtime::robots::RobotsCache>>,
    /// Per-domain rate limiter for polite crawling.
    domain_rate_limiter: Option<Arc<crate::runtime::rate_limit::DomainRateLimiter>>,
    /// Shared cookie jar for session persistence (fetch tasks with session: true).
    cookie_jar: Arc<reqwest_cookie_store::CookieStoreRwLock>,
    /// HTTP response cache for ETag / If-Modified-Since conditional requests.
    fetch_cache: Arc<crate::runtime::fetch_cache::FetchCache>,
}

impl TaskExecutor {
    /// Create a new executor with default provider, model, MCP configs, and event log
    pub fn new(
        provider: &str,
        model: Option<&str>,
        mcp_configs: Option<FxHashMap<String, McpConfigInline>>,
        event_log: EventLog,
    ) -> Result<Self, NikaError> {
        Self::with_policy(provider, model, mcp_configs, event_log, None, None, None)
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
        custom_endpoints: Option<crate::provider::endpoints::CustomEndpointMap>,
    ) -> Result<Self, NikaError> {
        // SAFETY: ClientBuilder::build() only fails with custom TLS or proxy config.
        // We use defaults, so this is effectively infallible.
        //
        // Custom redirect policy: check each hop against SSRF blocklist to prevent
        // SSRF bypass via HTTP redirect (e.g., external → 169.254.169.254).
        // S3: Capture allowed_hosts for the shared redirect closure so that
        // hosts whitelisted in [policy] aren't blocked on redirect.
        let redirect_allowed: Vec<String> = policy_config
            .as_ref()
            .map(|p| p.allowed_hosts.clone())
            .unwrap_or_default();

        let ssrf_redirect_policy = reqwest::redirect::Policy::custom(move |attempt| {
            use crate::runtime::policy::is_ssrf_blocked;

            if attempt.previous().len() >= REDIRECT_LIMIT {
                attempt.stop()
            } else {
                let blocked = attempt.url().host_str().and_then(|host| {
                    let h = host.to_lowercase();
                    let h_normalized = h.trim_start_matches('[').trim_end_matches(']');
                    let explicitly_allowed = redirect_allowed
                        .iter()
                        .any(|a| h_normalized == a.to_lowercase());
                    if !explicitly_allowed && is_ssrf_blocked(h_normalized) {
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

        // Auto-allow custom endpoint hosts in SSRF policy.
        // Users who configure custom endpoints (e.g. http://10.0.1.42:8000/v1)
        // explicitly trust those hosts — add them to allowed_hosts so fetch:
        // from those hosts is not blocked by SSRF protection.
        let mut policy = policy_config.unwrap_or_default();
        if let Some(ref endpoints) = custom_endpoints {
            for (name, ep) in endpoints {
                if let Ok(url) = url::Url::parse(&ep.base_url) {
                    if let Some(host) = url.host_str() {
                        let host_str = host.to_string();
                        if !policy.allowed_hosts.contains(&host_str) {
                            debug!(
                                endpoint = %name,
                                host = %host_str,
                                "Auto-allowing custom endpoint host in SSRF policy"
                            );
                            policy.allowed_hosts.push(host_str);
                        }
                    }
                }
            }
        }
        let policy_enforcer = PolicyEnforcer::new(policy);

        // Create ToolContext for file tools
        let working_dir = std::env::current_dir().unwrap_or_else(|_| {
            tracing::warn!("Failed to get current directory, using /tmp");
            std::path::PathBuf::from("/tmp")
        });
        let perm = permission_mode.unwrap_or(PermissionMode::Plan);
        tracing::debug!(?perm, "File tools using PermissionMode");
        let tool_ctx = Arc::new(ToolContext::new(working_dir.clone(), perm));

        // Create media tool context with CAS store at workspace default
        // Path confinement: import only allows files within working_dir
        let media_ctx = Arc::new(
            MediaToolContext::new(CasStore::workspace_default(&working_dir))?
                .with_working_dir(working_dir.clone()),
        );
        // Separate CAS handle for vision content resolution (same directory)
        let cas = Arc::new(CasStore::workspace_default(&working_dir));

        // Crawl intelligence: robots.txt + per-domain rate limiting + cookies + cache
        let robots_cache = Some(Arc::new(crate::runtime::robots::RobotsCache::new(
            &format!("nika/{}", env!("CARGO_PKG_VERSION")),
        )));
        let domain_rate_limiter = Some(Arc::new(
            crate::runtime::rate_limit::DomainRateLimiter::new(10),
        ));
        let cookie_jar = Arc::new(reqwest_cookie_store::CookieStoreRwLock::new(
            cookie_store::CookieStore::default(),
        ));
        let fetch_cache = Arc::new(crate::runtime::fetch_cache::FetchCache::new());

        // Build router before struct init to avoid borrow-after-move on event_log
        let builtin_router = Arc::new(
            BuiltinToolRouter::with_all_tools(tool_ctx.clone(), media_ctx)
                .with_cost_tool(event_log.clone()),
        );

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
            builtin_router,
            policy_enforcer: Arc::new(RwLock::new(policy_enforcer)),
            cancel_token: CancellationToken::new(),
            cas,
            tool_ctx,
            skill_injector: Arc::new(SkillInjector::new()),
            skills_map: Arc::new(std::collections::HashMap::new()),
            workflow_base_dir: working_dir,
            project_root: None,
            working_dir_mode: None,
            custom_endpoints: Arc::new(custom_endpoints.unwrap_or_default()),
            resolved_agents: Arc::new(rustc_hash::FxHashMap::default()),
            robots_cache,
            domain_rate_limiter,
            cookie_jar,
            fetch_cache,
        })
    }

    /// Wire introspection tools that need RunContext (records, dag_info, task_status, threads, orchestrate).
    ///
    /// Must be called after construction because the datastore is created
    /// in the Runner, not the executor. Uses `Arc::get_mut` which succeeds
    /// because no other references exist at this point in initialization.
    pub fn wire_introspection_tools(&mut self, datastore: Arc<crate::store::RunContext>) {
        if let Some(router) = Arc::get_mut(&mut self.builtin_router) {
            router.register(super::builtin::RecordsTool::new(Arc::clone(&datastore)));
            router.register(super::builtin::DagInfoTool::new(self.event_log.clone()));
            router.register(super::builtin::TaskStatusTool::new(
                self.event_log.clone(),
                Arc::clone(&datastore),
            ));
            router.register(super::builtin::ThreadsTool::new(self.event_log.clone()));
            router.register(super::builtin::OrchestrateTool::new(
                self.event_log.clone(),
                datastore,
            ));
        } else {
            tracing::warn!("Could not wire introspection tools — router already shared");
        }
    }

    /// Set the permission mode for file tools (nika:write, nika:edit, etc.)
    pub fn set_permission_mode(&self, mode: PermissionMode) {
        self.tool_ctx.set_permission_mode(mode);
    }

    /// Set custom endpoints without rebuilding the executor.
    ///
    /// Preserves all existing state (policy, permission mode, MCP connections,
    /// HTTP client pool, CAS store). Only swaps the endpoint map.
    /// Also auto-allows endpoint hosts in the SSRF policy.
    pub fn set_custom_endpoints(
        &mut self,
        endpoints: crate::provider::endpoints::CustomEndpointMap,
    ) {
        // Auto-allow custom endpoint hosts in SSRF policy
        let mut enforcer = self.policy_enforcer.write();
        for (name, ep) in &endpoints {
            if let Ok(url) = url::Url::parse(&ep.base_url) {
                if let Some(host) = url.host_str() {
                    let host_str = host.to_string();
                    if enforcer.add_allowed_host(&host_str) {
                        debug!(
                            endpoint = %name,
                            host = %host_str,
                            "Auto-allowing custom endpoint host in SSRF policy"
                        );
                    }
                }
            }
        }
        drop(enforcer);
        self.custom_endpoints = Arc::new(endpoints);
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
        self.skills_map = Arc::new(skills_map);
        // Only set workflow_base_dir if not already set via with_base_path()
        // (with_base_path uses the workflow file directory which is more specific)
        let working_dir = std::env::current_dir().unwrap_or_default();
        if self.workflow_base_dir == working_dir {
            self.workflow_base_dir = base_dir;
        }
        self
    }

    /// Set the workflow base directory for exec `cwd:` security checks.
    ///
    /// Exec tasks with `cwd:` can only access paths under this directory.
    /// Also updates the BuiltinToolRouter's ToolContext so invoke: nika:read/glob
    /// use the workflow directory, not the process cwd (B02 fix).
    pub fn with_base_path(mut self, path: std::path::PathBuf) -> Self {
        self.workflow_base_dir = path.clone();
        // B02 fix: update shared ToolContext so file tools see the correct base path
        self.tool_ctx.set_working_dir(path);
        self
    }

    /// Set the project root directory (parent of nika.toml).
    ///
    /// Used by `working_dir_mode = "project"` to set exec task cwd
    /// and expand the ToolContext security boundary for nika:read/write/glob/grep.
    pub fn with_project_root(mut self, root: std::path::PathBuf) -> Self {
        self.project_root = Some(root.clone());
        // When working_dir_mode is already "project", update ToolContext security boundary
        if self.working_dir_mode.as_deref() == Some("project") {
            self.tool_ctx.set_working_dir(root);
        }
        self
    }

    /// Set the working directory mode from `[tools] working_dir` in nika.toml.
    ///
    /// - `"project"` → exec tasks + file tools default to project_root
    /// - `"workflow"` → exec tasks + file tools default to workflow_base_dir
    /// - `"none"` → no default cwd, inherit process cwd
    pub fn with_working_dir_mode(mut self, mode: String) -> Self {
        // When mode is "project" and project_root is set, update ToolContext security boundary
        // so nika:read/write/glob/grep resolve paths from project root, not workflow dir.
        if mode == "project" {
            if let Some(ref root) = self.project_root {
                self.tool_ctx.set_working_dir(root.clone());
            }
        }
        self.working_dir_mode = Some(mode);
        self
    }

    /// Resolve the effective default cwd for exec tasks based on working_dir_mode.
    ///
    /// Returns `None` when mode is "none" (inherit process cwd).
    pub(super) fn resolve_default_exec_cwd(&self) -> Option<&std::path::Path> {
        match self.working_dir_mode.as_deref() {
            Some("project") => self
                .project_root
                .as_deref()
                .or(Some(self.workflow_base_dir.as_path())),
            Some("none") => None,
            // "workflow" or None (default) → use workflow_base_dir
            _ => Some(self.workflow_base_dir.as_path()),
        }
    }

    /// Set the resolved agent presets from the workflow's `agents:` block.
    pub fn with_resolved_agents(
        mut self,
        agents: crate::runtime::resolver::ResolvedAgents,
    ) -> Self {
        self.resolved_agents = Arc::new(agents);
        self
    }

    /// Look up a resolved agent preset by name.
    pub fn get_preset(&self, name: &str) -> Option<&crate::runtime::resolver::ResolvedAgent> {
        self.resolved_agents.get(name)
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
    /// Build JSON schema instruction for LLM prompt injection.
    ///
    /// `cached_example` is used for file-based `from_example` — the caller pre-reads
    /// the file asynchronously and passes the parsed value here for synchronous injection.
    pub(super) fn build_json_schema_instruction(
        output_policy: Option<&OutputPolicy>,
        cached_example: Option<&serde_json::Value>,
    ) -> Option<String> {
        let policy = output_policy?;
        if policy.format != OutputFormat::Json {
            return None;
        }

        // from_example: inject example structure or generic instruction.
        match policy.from_example.as_ref() {
            Some(SchemaRef::Inline(ref example)) => {
                return Self::format_example_instruction(example);
            }
            Some(SchemaRef::File(_)) => {
                // File-based: use cached_example if pre-loaded, otherwise generic instruction.
                if let Some(example) = cached_example {
                    return Self::format_example_instruction(example);
                }
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
            None => {} // no from_example — fall through to schema-based injection below
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

    /// Format an example JSON value into a prompt injection instruction.
    fn format_example_instruction(example: &serde_json::Value) -> Option<String> {
        let example_str = match serde_json::to_string_pretty(example) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    "Failed to serialize from_example for prompt injection: {}",
                    e
                );
                return None;
            }
        };
        Some(format!(
            "\n\n---\n\
             CRITICAL OUTPUT REQUIREMENT:\n\
             Your response MUST be valid JSON matching this exact structure:\n\n\
             ```json\n{}\n```\n\n\
             Rules:\n\
             - Output ONLY the JSON object, no additional text\n\
             - Do NOT wrap in markdown code blocks (no ```json)\n\
             - All keys shown above must be present\n\
             - Value types must match (strings, numbers, arrays, objects)",
            example_str
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
        let is_llm_verb = matches!(action, TaskAction::Infer { .. } | TaskAction::Agent { .. });
        let result = match action {
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
        };

        // Scan LLM output for injection patterns (infer + agent verbs only)
        if is_llm_verb {
            if let Ok(ref output) = result {
                let findings = crate::runtime::output_scanner::scan_output(output);
                for finding in &findings {
                    self.event_log.emit(EventKind::SecurityScanFinding {
                        task_id: Arc::clone(task_id),
                        pattern: finding.pattern.clone(),
                        description: finding.description.clone(),
                    });
                }
            }
        }

        result
    }

    /// Execute a task action with routing (fallback chain support).
    ///
    /// If routing has a non-empty fallback chain, tries each provider in order.
    /// On provider error, emits `FallbackTriggered` and tries the next provider.
    /// If all providers fail, returns `FallbackChainExhausted` (NIKA-037).
    ///
    /// For tasks without routing, delegates directly to `execute()`.
    pub async fn execute_with_routing(
        &self,
        task_id: &Arc<str>,
        action: &TaskAction,
        bindings: &ResolvedBindings,
        datastore: &RunContext,
        output_policy: Option<&OutputPolicy>,
        routing: Option<&nika_core::ast::routing::RoutingConfig>,
    ) -> Result<String, NikaError> {
        let chain = routing.map(|r| &r.fallback).filter(|f| !f.is_empty());

        let Some(fallback_chain) = chain else {
            // No routing — standard execution
            return self
                .execute(task_id, action, bindings, datastore, output_policy)
                .await;
        };

        // Only infer and agent verbs support fallback routing
        let is_llm_verb = matches!(action, TaskAction::Infer { .. } | TaskAction::Agent { .. });
        if !is_llm_verb {
            return self
                .execute(task_id, action, bindings, datastore, output_policy)
                .await;
        }

        let mut errors: Vec<(String, String)> = Vec::new();

        for (idx, provider_name) in fallback_chain.iter().enumerate() {
            // Override the provider for this attempt
            let mut action_clone = action.clone();
            match &mut action_clone {
                TaskAction::Infer { infer } => {
                    infer.provider = Some(nika_core::ProviderName::parse(provider_name));
                    // Clear provider_chain so run_infer uses only the overridden single
                    // provider. Without this, run_infer ignores the override and uses the
                    // original chain (which always starts with the first provider).
                    infer.provider_chain = None;
                }
                TaskAction::Agent { agent } => {
                    agent.provider = Some(nika_core::ProviderName::parse(provider_name));
                    agent.provider_chain = None;
                }
                _ => {}
            }

            match self
                .execute(task_id, &action_clone, bindings, datastore, output_policy)
                .await
            {
                Ok(output) => return Ok(output),
                Err(e) => {
                    let reason = classify_fallback_reason(&e);
                    errors.push((provider_name.clone(), e.to_string()));

                    let is_last = idx == fallback_chain.len() - 1;
                    if !is_last {
                        let next_provider = &fallback_chain[idx + 1];
                        self.event_log
                            .emit(crate::event::EventKind::FallbackTriggered {
                                task_id: Arc::clone(task_id),
                                from_provider: provider_name.clone(),
                                to_provider: next_provider.clone(),
                                reason,
                                attempt: (idx + 1) as u32, // 1-indexed (consistent with TaskRetry)
                            });
                    }
                }
            }
        }

        // Build detailed error message with per-provider failures
        let last_error = errors
            .last()
            .map(|(_, e)| e.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let details: Vec<String> = errors.iter().map(|(p, e)| format!("{p}: {e}")).collect();
        tracing::warn!(
            task_id = %task_id,
            chain = %fallback_chain.join(" → "),
            "Fallback chain exhausted: {}",
            details.join("; ")
        );

        self.event_log.emit(EventKind::FallbackChainExhausted {
            task_id: Arc::clone(task_id),
            providers_tried: fallback_chain.clone(),
            last_error: last_error.clone(),
        });

        Err(NikaError::FallbackChainExhausted {
            providers: fallback_chain.join(", "),
            last_error,
        })
    }

    /// Get or create a cached rig-core provider.
    ///
    /// Resolves provider names and aliases via [`RigProvider::from_name()`],
    /// which uses `core::find_provider()` as the single source of truth.
    pub(super) fn get_rig_provider(&self, name: &str) -> Result<RigProvider, NikaError> {
        use dashmap::mapref::entry::Entry;

        // Check custom endpoints first — they don't alias through the catalog
        if self.custom_endpoints.contains_key(name) {
            match self.rig_provider_cache.entry(name.to_string()) {
                Entry::Occupied(e) => return Ok(e.get().clone()),
                Entry::Vacant(e) => {
                    let provider =
                        RigProvider::from_name_with_endpoints(name, &self.custom_endpoints)?;
                    e.insert(provider.clone());
                    self.event_log.emit(EventKind::ProviderInitialized {
                        provider: name.to_string(),
                        model: "ready".to_string(),
                        cached: false,
                    });
                    return Ok(provider);
                }
            }
        }

        // Catalog providers — normalize alias to canonical name for cache key
        let canonical = crate::core::find_provider(name)
            .map(|p| p.id)
            .unwrap_or(name);

        match self.rig_provider_cache.entry(canonical.to_string()) {
            Entry::Occupied(e) => Ok(e.get().clone()),
            Entry::Vacant(e) => {
                let provider = RigProvider::from_name(name)?;
                e.insert(provider.clone());
                self.event_log.emit(EventKind::ProviderInitialized {
                    provider: canonical.to_string(),
                    model: "ready".to_string(),
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

    /// Get hourly_rate for a custom endpoint (None for catalog providers).
    pub(super) fn endpoint_hourly_rate(&self, provider_name: &str) -> Option<f64> {
        self.custom_endpoints
            .get(provider_name)
            .and_then(|ep| ep.hourly_rate)
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

    /// Gracefully shut down all MCP server connections.
    ///
    /// Delegates to [`McpClientPool::shutdown_all`] which terminates server
    /// processes and marks the pool as shut down. Idempotent.
    pub async fn shutdown_mcp(&self) {
        self.mcp_pool.shutdown_all().await;
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

/// Classify error into a fallback reason string for the FallbackTriggered event.
fn classify_fallback_reason(err: &NikaError) -> String {
    match err {
        NikaError::Timeout { .. } | NikaError::McpTimeout { .. } => "timeout".to_string(),
        NikaError::StructuredOutputAllLayersFailed { .. } => "structured_failure".to_string(),
        NikaError::EndpointConnectionFailed { .. } => "connection_failed".to_string(),
        NikaError::EndpointNotFound { .. } => "endpoint_not_found".to_string(),
        NikaError::MissingApiKey { .. } => "missing_api_key".to_string(),
        NikaError::ProviderNotConfigured { .. } => "provider_not_configured".to_string(),
        NikaError::ProviderApiError { message, .. } => {
            let lower = message.to_lowercase();
            if lower.contains("429") || lower.contains("rate") || lower.contains("quota") {
                "rate_limited".to_string()
            } else if lower.contains("401")
                || lower.contains("unauthorized")
                || lower.contains("forbidden")
            {
                "auth_failed".to_string()
            } else {
                "provider_error".to_string()
            }
        }
        NikaError::AgentLimitExceeded { .. } => "limit_exceeded".to_string(),
        _ => "error".to_string(),
    }
}
