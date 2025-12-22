//! # Workflow Runner (v4.6)
//!
//! Main workflow execution orchestrator using SharedAgentRunner and IsolatedAgentRunner.
//!
//! This module contains the high-level Runner that:
//! - Computes task execution order (topological sort)
//! - Manages context across tasks
//! - Delegates to specialized runners for agent/subagent tasks
//! - Handles shell, http, mcp, function, llm tasks directly

use crate::limits::{CircuitBreaker, ResourceLimits};
use crate::provider::{create_provider, Provider, TokenUsage};
use crate::runner::context::{ContextWriter, GlobalContext};
use crate::runner::core::AgentConfig;
use crate::runner::isolated::IsolatedAgentRunner;
use crate::runner::shared::SharedAgentRunner;
use crate::workflow::{Task, TaskKeyword, Workflow};
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};
use wait_timeout::ChildExt;

// ============================================================================
// CONSTANTS
// ============================================================================

/// Default timeout for shell commands (30 seconds)
const DEFAULT_SHELL_TIMEOUT: Duration = Duration::from_secs(30);

/// Default backoff for retries (1 second)
const DEFAULT_RETRY_BACKOFF: Duration = Duration::from_secs(1);

// ============================================================================
// DURATION PARSING
// ============================================================================

/// Parse a duration string like "30s", "5m", "1h" into a Duration
fn parse_duration(duration_str: &str) -> Option<Duration> {
    let s = duration_str.trim();
    if s.is_empty() {
        return None;
    }

    if let Some(ms) = s.strip_suffix("ms") {
        return ms.parse::<u64>().ok().map(Duration::from_millis);
    }
    if let Some(secs) = s.strip_suffix('s') {
        return secs.parse::<u64>().ok().map(Duration::from_secs);
    }
    if let Some(mins) = s.strip_suffix('m') {
        return mins
            .parse::<u64>()
            .ok()
            .map(|m| Duration::from_secs(m * 60));
    }
    if let Some(hours) = s.strip_suffix('h') {
        return hours
            .parse::<u64>()
            .ok()
            .map(|h| Duration::from_secs(h * 3600));
    }

    s.parse::<u64>().ok().map(Duration::from_secs)
}

fn parse_timeout(timeout_str: &str) -> Option<Duration> {
    parse_duration(timeout_str)
}

// ============================================================================
// URL VALIDATION
// ============================================================================

/// Validate HTTP URL for security (SSRF prevention)
///
/// Blocks:
/// - Non-HTTP(S) schemes (file://, ftp://, gopher://, etc.)
/// - Localhost and loopback addresses
/// - Private IP ranges (10.x, 172.16-31.x, 192.168.x, 127.x)
/// - IPv6 private ranges (ULA fc00::/7, link-local fe80::/10)
/// - Link-local addresses (169.254.x)
/// - Cloud metadata endpoints (169.254.169.254, *.internal)
/// - URL encoding tricks (hex IPs, integer IPs)
fn validate_http_url(url: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;

    // Check scheme - only http/https allowed
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(format!(
                "Invalid URL scheme '{}': only http/https allowed",
                scheme
            ))
        }
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| "URL has no host".to_string())?;

    // Block URL encoding tricks in hostname
    if host.contains('%') || host.contains("0x") || host.contains("0X") {
        return Err("SSRF blocked: URL encoding in hostname not allowed".to_string());
    }

    // Block localhost variants
    if host == "localhost"
        || host == "127.0.0.1"
        || host == "::1"
        || host == "[::1]"
        || host.ends_with(".localhost")
        || host.ends_with(".localdomain")
    {
        return Err("SSRF blocked: localhost not allowed".to_string());
    }

    // Check for IP addresses (both direct and parsed by url crate)
    if let Some(url_host) = parsed.host() {
        match url_host {
            url::Host::Ipv4(ip) => {
                if is_private_ip(&std::net::IpAddr::V4(ip)) {
                    return Err(format!("SSRF blocked: private IP {} not allowed", ip));
                }
            }
            url::Host::Ipv6(ip) => {
                if is_private_ip(&std::net::IpAddr::V6(ip)) {
                    return Err(format!("SSRF blocked: private IPv6 {} not allowed", ip));
                }
            }
            url::Host::Domain(domain) => {
                // Try parsing as IP in case it's an unusual format (integer IP, etc.)
                if let Ok(ip) = domain.parse::<std::net::IpAddr>() {
                    if is_private_ip(&ip) {
                        return Err(format!("SSRF blocked: private IP {} not allowed", ip));
                    }
                }
            }
        }
    }

    // Block cloud metadata endpoints
    if host == "169.254.169.254"
        || host.ends_with(".internal")
        || host.ends_with(".metadata")
        || host == "metadata.google.internal"
        || host == "metadata.goog"
    {
        return Err("SSRF blocked: cloud metadata endpoint not allowed".to_string());
    }

    // Block Kubernetes service discovery
    if host.ends_with(".svc.cluster.local") || host.ends_with(".pod.cluster.local") {
        return Err("SSRF blocked: Kubernetes internal endpoint not allowed".to_string());
    }

    Ok(())
}

fn is_private_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ipv4) => {
            let octets = ipv4.octets();
            // 10.0.0.0/8
            octets[0] == 10
                // 172.16.0.0/12
                || (octets[0] == 172 && (16..=31).contains(&octets[1]))
                // 192.168.0.0/16
                || (octets[0] == 192 && octets[1] == 168)
                // 127.0.0.0/8 (loopback)
                || octets[0] == 127
                // 169.254.0.0/16 (link-local)
                || (octets[0] == 169 && octets[1] == 254)
                // 0.0.0.0
                || octets == [0, 0, 0, 0]
        }
        std::net::IpAddr::V6(ipv6) => {
            let octets = ipv6.octets();
            // ::1 (loopback)
            ipv6.is_loopback()
                // :: (unspecified)
                || ipv6.is_unspecified()
                // fc00::/7 (Unique Local Addresses - ULA)
                || (octets[0] & 0xfe) == 0xfc
                // fe80::/10 (link-local)
                || (octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80)
                // fec0::/10 (site-local, deprecated but check anyway)
                || (octets[0] == 0xfe && (octets[1] & 0xc0) == 0xc0)
                // ::ffff:0:0/96 (IPv4-mapped, check the embedded IPv4)
                || is_ipv4_mapped_private(ipv6)
        }
    }
}

/// Check if IPv4-mapped IPv6 address contains private IPv4
fn is_ipv4_mapped_private(ipv6: &std::net::Ipv6Addr) -> bool {
    // Check if it's an IPv4-mapped address (::ffff:x.x.x.x)
    let octets = ipv6.octets();
    if octets[..10] == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0] && octets[10] == 0xff && octets[11] == 0xff {
        // Extract the IPv4 part and check if private
        let ipv4 = std::net::Ipv4Addr::new(octets[12], octets[13], octets[14], octets[15]);
        return is_private_ip(&std::net::IpAddr::V4(ipv4));
    }
    false
}

// ============================================================================
// ERROR TYPES
// ============================================================================

/// Error context for failed tasks
#[derive(Debug, Clone, Default)]
pub struct ErrorContext {
    pub keyword: Option<String>,
    pub category: Option<ErrorCategory>,
    pub details: Option<String>,
}

/// Error categories
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorCategory {
    Timeout,
    Network,
    Provider,
    Template,
    Config,
    Execution,
}

// ============================================================================
// TASK RESULT
// ============================================================================

/// Execution result for a task
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: String,
    pub success: bool,
    pub output: String,
    pub tokens_used: Option<u32>,
    pub error_context: Option<ErrorContext>,
}

impl TaskResult {
    pub fn success(id: impl Into<String>, output: impl Into<String>, tokens: Option<u32>) -> Self {
        Self {
            task_id: id.into(),
            success: true,
            output: output.into(),
            tokens_used: tokens,
            error_context: None,
        }
    }

    pub fn ok(id: impl Into<String>, output: impl Into<String>, tokens: u32) -> Self {
        Self::success(id, output, Some(tokens))
    }

    pub fn failure(id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            task_id: id.into(),
            success: false,
            output: error.into(),
            tokens_used: None,
            error_context: None,
        }
    }

    pub fn err(id: impl Into<String>, msg: impl Into<String>, cat: ErrorCategory) -> Self {
        Self::failure_with_context(id, msg, "", cat)
    }

    pub fn failure_with_context(
        id: impl Into<String>,
        error: impl Into<String>,
        keyword: impl Into<String>,
        category: ErrorCategory,
    ) -> Self {
        Self {
            task_id: id.into(),
            success: false,
            output: error.into(),
            tokens_used: None,
            error_context: Some(ErrorContext {
                keyword: Some(keyword.into()),
                category: Some(category),
                details: None,
            }),
        }
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        if let Some(ref mut ctx) = self.error_context {
            ctx.details = Some(details.into());
        } else {
            self.error_context = Some(ErrorContext {
                keyword: None,
                category: None,
                details: Some(details.into()),
            });
        }
        self
    }

    pub fn is_timeout(&self) -> bool {
        self.error_context
            .as_ref()
            .and_then(|c| c.category.as_ref())
            .is_some_and(|cat| *cat == ErrorCategory::Timeout)
    }

    pub fn as_json(&self) -> Option<serde_json::Value> {
        serde_json::from_str(&self.output).ok()
    }
}

// ============================================================================
// RUN RESULT
// ============================================================================

/// Workflow execution summary
#[derive(Debug)]
pub struct RunResult {
    pub workflow_name: String,
    pub tasks_completed: usize,
    pub tasks_failed: usize,
    pub results: Vec<TaskResult>,
    pub total_tokens: u32,
    pub context: GlobalContext,
}

// ============================================================================
// RUNNER
// ============================================================================

/// Workflow runner - orchestrates task execution
pub struct Runner {
    provider: Arc<dyn Provider>,
    limits: ResourceLimits,
    circuit_breaker: Option<Arc<CircuitBreaker>>,
    verbose: bool,
    /// Runner for agent: tasks (shared context)
    shared_runner: SharedAgentRunner,
    /// Runner for subagent: tasks (isolated context)
    isolated_runner: IsolatedAgentRunner,
}

impl Runner {
    /// Create a new runner with the specified provider
    pub fn new(provider_name: &str) -> Result<Self> {
        let provider: Arc<dyn Provider> = Arc::from(create_provider(provider_name)?);
        let config = AgentConfig::new("claude-sonnet-4-5");

        Ok(Self {
            provider: provider.clone(),
            limits: ResourceLimits::default(),
            circuit_breaker: None,
            verbose: false,
            shared_runner: SharedAgentRunner::new(provider.clone(), config.clone()),
            isolated_runner: IsolatedAgentRunner::new(provider, config),
        })
    }

    pub fn verbose(mut self, v: bool) -> Self {
        self.verbose = v;
        self
    }

    pub fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn with_circuit_breaker(mut self, breaker: Arc<CircuitBreaker>) -> Self {
        self.circuit_breaker = Some(breaker);
        self
    }

    /// Execute a workflow with default empty context
    pub async fn run(&self, workflow: &Workflow) -> Result<RunResult> {
        self.run_with_context(workflow, GlobalContext::new()).await
    }

    /// Execute a workflow with provided inputs
    pub async fn run_with_inputs(
        &self,
        workflow: &Workflow,
        inputs: HashMap<String, String>,
    ) -> Result<RunResult> {
        self.run_with_context(workflow, GlobalContext::with_inputs(inputs))
            .await
    }

    /// Execute a workflow with a pre-configured context
    pub async fn run_with_context(
        &self,
        workflow: &Workflow,
        mut ctx: GlobalContext,
    ) -> Result<RunResult> {
        let workflow_start = Instant::now();
        let mut results = Vec::new();
        let mut total_tokens = 0u32;

        let task_map: HashMap<&str, &Task> =
            workflow.tasks.iter().map(|t| (t.id.as_str(), t)).collect();

        let order = self
            .topological_sort(workflow)
            .context("Failed to determine task execution order")?;

        if self.verbose {
            println!("Execution order: {:?}", order);
        }

        for task_id in &order {
            if workflow_start.elapsed() > self.limits.max_workflow_duration {
                return Err(anyhow!(
                    "Workflow timeout exceeded ({:?})",
                    self.limits.max_workflow_duration
                ));
            }

            let task = task_map
                .get(task_id.as_str())
                .ok_or_else(|| anyhow!("Task not found: {}", task_id))?;

            if self.verbose {
                let keyword = task.keyword();
                println!("\n-> Executing: {} ({})", task_id, keyword);
            }

            let result = self
                .execute_task_with_retry(task, workflow, &mut ctx)
                .await
                .with_context(|| {
                    format!(
                        "Failed to execute task '{}' ({:?})",
                        task_id,
                        task.keyword()
                    )
                })?;

            if let Some(tokens) = result.tokens_used {
                total_tokens += tokens;
            }

            if result.output.len() > self.limits.max_output_size {
                return Err(anyhow!(
                    "Task '{}' output exceeds size limit ({} > {})",
                    task_id,
                    result.output.len(),
                    self.limits.max_output_size
                ));
            }

            // Store output in context (for subagent, this is the auto-bridge)
            ctx.set_output(&result.task_id, result.output.clone());

            if let Some(json) = result.as_json() {
                ctx.set_structured_output(&result.task_id, json);
            }

            if self.verbose {
                println!(
                    "  {} {}",
                    if result.success { "[OK]" } else { "[FAIL]" },
                    if result.output.len() > 100 {
                        format!("{}...", &result.output[..100])
                    } else {
                        result.output.clone()
                    }
                );
            }

            results.push(result);
        }

        let tasks_completed = results.iter().filter(|r| r.success).count();
        let tasks_failed = results.len() - tasks_completed;

        Ok(RunResult {
            workflow_name: workflow
                .agent
                .system_prompt
                .as_deref()
                .and_then(|s| s.lines().next())
                .unwrap_or("workflow")
                .to_string(),
            tasks_completed,
            tasks_failed,
            results,
            total_tokens,
            context: ctx,
        })
    }

    async fn execute_task_with_retry(
        &self,
        task: &Task,
        workflow: &Workflow,
        ctx: &mut GlobalContext,
    ) -> Result<TaskResult> {
        let max_attempts = task
            .config
            .as_ref()
            .and_then(|c| c.retry.as_ref())
            .map(|r| r.max)
            .unwrap_or(1);

        let backoff = task
            .config
            .as_ref()
            .and_then(|c| c.retry.as_ref())
            .and_then(|r| r.backoff.as_ref())
            .and_then(|b| parse_duration(b))
            .unwrap_or(DEFAULT_RETRY_BACKOFF);

        let mut last_result = None;

        for attempt in 1..=max_attempts {
            let result = self.execute_task(task, workflow, ctx).await?;

            if result.success {
                return Ok(result);
            }

            last_result = Some(result);

            if attempt < max_attempts {
                if self.verbose {
                    println!(
                        "  [RETRY] {}/{} for task '{}' after {:?}",
                        attempt, max_attempts, task.id, backoff
                    );
                }
                tokio::time::sleep(backoff).await;
            }
        }

        Ok(last_result
            .unwrap_or_else(|| TaskResult::failure(&task.id, "Task failed with no result")))
    }

    async fn execute_task(
        &self,
        task: &Task,
        workflow: &Workflow,
        ctx: &mut GlobalContext,
    ) -> Result<TaskResult> {
        match task.keyword() {
            TaskKeyword::Agent => self.execute_agent(task, workflow, ctx).await,
            TaskKeyword::Subagent => self.execute_subagent(task, workflow, ctx).await,
            TaskKeyword::Shell => self.execute_shell(task, ctx).await,
            TaskKeyword::Http => self.execute_http(task, ctx).await,
            TaskKeyword::Mcp => self.execute_mcp(task, ctx),
            TaskKeyword::Function => self.execute_function(task, ctx),
            TaskKeyword::Llm => self.execute_llm(task, workflow).await,
        }
    }

    /// Execute agent: task using SharedAgentRunner
    async fn execute_agent(
        &self,
        task: &Task,
        workflow: &Workflow,
        ctx: &mut GlobalContext,
    ) -> Result<TaskResult> {
        use crate::task::TaskAction;

        let agent_def = match &task.action {
            TaskAction::Agent { agent } => agent,
            _ => return Ok(TaskResult::failure(&task.id, "Expected agent task")),
        };

        let prompt = resolve_templates(&agent_def.prompt, ctx)
            .with_context(|| format!("Failed to resolve templates for agent task '{}'", task.id))?;

        // Build config for this task
        let config = AgentConfig::new(agent_def.model.as_deref().unwrap_or(&workflow.agent.model))
            .with_system_prompt(
                agent_def
                    .system_prompt
                    .as_deref()
                    .or(workflow.agent.system_prompt.as_deref())
                    .unwrap_or(""),
            )
            .with_tools(agent_def.allowed_tools.clone().unwrap_or_default());

        // Use SharedAgentRunner
        match self
            .shared_runner
            .execute_with_config(&task.id, &prompt, ctx, &config)
            .await
        {
            Ok(result) => Ok(TaskResult {
                task_id: result.task_id,
                success: result.success,
                output: result.output,
                tokens_used: Some(result.usage.total_tokens),
                error_context: None,
            }),
            Err(e) => Ok(TaskResult::failure_with_context(
                &task.id,
                e.to_string(),
                "agent",
                ErrorCategory::Provider,
            )),
        }
    }

    /// Execute subagent: task using IsolatedAgentRunner
    async fn execute_subagent(
        &self,
        task: &Task,
        workflow: &Workflow,
        ctx: &mut GlobalContext,
    ) -> Result<TaskResult> {
        use crate::task::TaskAction;

        let subagent_def = match &task.action {
            TaskAction::Subagent { subagent } => subagent,
            _ => return Ok(TaskResult::failure(&task.id, "Expected subagent task")),
        };

        let prompt = resolve_templates(&subagent_def.prompt, ctx).with_context(|| {
            format!(
                "Failed to resolve templates for subagent task '{}'",
                task.id
            )
        })?;

        let config = AgentConfig::new(
            subagent_def
                .model
                .as_deref()
                .unwrap_or(&workflow.agent.model),
        )
        .with_system_prompt(
            subagent_def
                .system_prompt
                .as_deref()
                .or(workflow.agent.system_prompt.as_deref())
                .unwrap_or(""),
        )
        .with_tools(subagent_def.allowed_tools.clone().unwrap_or_default());

        // Use IsolatedAgentRunner (takes &GlobalContext, not &mut)
        match self
            .isolated_runner
            .execute_with_config(&task.id, &prompt, ctx, &config)
            .await
        {
            Ok(result) => {
                // v4.7.1: Auto-write subagent output to GlobalContext
                // The workflow runner handles the bridging automatically
                Ok(TaskResult {
                    task_id: result.task_id,
                    success: result.success,
                    output: result.output,
                    tokens_used: Some(result.usage.total_tokens),
                    error_context: None,
                })
            }
            Err(e) => Ok(TaskResult::failure_with_context(
                &task.id,
                e.to_string(),
                "subagent",
                ErrorCategory::Provider,
            )),
        }
    }

    async fn execute_shell(&self, task: &Task, ctx: &GlobalContext) -> Result<TaskResult> {
        use crate::task::TaskAction;

        let shell_def = match &task.action {
            TaskAction::Shell { shell } => shell,
            _ => return Ok(TaskResult::failure(&task.id, "Expected shell task")),
        };

        let cmd_str = resolve_templates(&shell_def.command, ctx)
            .with_context(|| format!("Failed to resolve templates for shell task '{}'", task.id))?;

        let timeout = task
            .config
            .as_ref()
            .and_then(|c| c.timeout.as_ref())
            .and_then(|t| parse_timeout(t))
            .unwrap_or(DEFAULT_SHELL_TIMEOUT);

        let task_id = task.id.clone();
        let cmd_str_clone = cmd_str.clone();

        let result =
            tokio::task::spawn_blocking(move || -> Result<(bool, String, usize, Option<i32>)> {
                let mut child = Command::new("sh")
                    .arg("-c")
                    .arg(&cmd_str_clone)
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .with_context(|| format!("Failed to spawn shell command: {}", cmd_str_clone))?;

                match child.wait_timeout(timeout)? {
                    Some(status) => {
                        let stdout = child
                            .stdout
                            .take()
                            .map(|mut s| {
                                let mut buf = String::new();
                                std::io::Read::read_to_string(&mut s, &mut buf).ok();
                                buf
                            })
                            .unwrap_or_default();

                        let stderr = child
                            .stderr
                            .take()
                            .map(|mut s| {
                                let mut buf = String::new();
                                std::io::Read::read_to_string(&mut s, &mut buf).ok();
                                buf
                            })
                            .unwrap_or_default();

                        if status.success() {
                            let output = if stdout.trim().is_empty() && !stderr.trim().is_empty() {
                                stderr.trim().to_string()
                            } else {
                                stdout.trim().to_string()
                            };
                            Ok((true, output, cmd_str_clone.len(), None))
                        } else {
                            let error_msg = if stderr.trim().is_empty() {
                                format!("Command exited with code: {}", status.code().unwrap_or(-1))
                            } else {
                                stderr.trim().to_string()
                            };
                            Ok((false, error_msg, cmd_str_clone.len(), status.code()))
                        }
                    }
                    None => {
                        let _ = child.kill();
                        let _ = child.wait();
                        Ok((
                            false,
                            format!("Shell command timed out after {:?}", timeout),
                            cmd_str_clone.len(),
                            None,
                        ))
                    }
                }
            })
            .await
            .context("Shell task panicked")??;

        let (success, output, cmd_len, exit_code) = result;
        if success {
            let tokens = TokenUsage::estimate(cmd_len, output.len());
            Ok(TaskResult::success(
                &task_id,
                output,
                Some(tokens.total_tokens),
            ))
        } else if output.contains("timed out") {
            Ok(TaskResult::failure_with_context(
                &task_id,
                output,
                "shell",
                ErrorCategory::Timeout,
            ))
        } else {
            Ok(TaskResult::failure_with_context(
                &task_id,
                &output,
                "shell",
                ErrorCategory::Execution,
            )
            .with_details(format!("command: {}, exit_code: {:?}", cmd_str, exit_code)))
        }
    }

    async fn execute_http(&self, task: &Task, ctx: &GlobalContext) -> Result<TaskResult> {
        use crate::task::TaskAction;

        let http_def = match &task.action {
            TaskAction::Http { http } => http,
            _ => return Ok(TaskResult::failure(&task.id, "Expected http task")),
        };

        let resolved_url = resolve_templates(&http_def.url, ctx)
            .with_context(|| format!("Failed to resolve URL for http task '{}'", task.id))?;

        if let Err(e) = validate_http_url(&resolved_url) {
            return Ok(TaskResult::failure_with_context(
                &task.id,
                e,
                "http",
                ErrorCategory::Config,
            ));
        }

        let method = http_def.method.as_deref().unwrap_or("GET").to_uppercase();

        // Build HTTP client with security settings
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            // SECURITY: Disable automatic redirect following to prevent SSRF bypass
            // An attacker could redirect from a public URL to an internal service
            .redirect(reqwest::redirect::Policy::none())
            // SECURITY: Disable automatic decompression to prevent zip bombs
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .build()
            .context("Failed to create HTTP client")?;

        let mut request = match method.as_str() {
            "GET" => client.get(&resolved_url),
            "POST" => client.post(&resolved_url),
            "PUT" => client.put(&resolved_url),
            "DELETE" => client.delete(&resolved_url),
            "PATCH" => client.patch(&resolved_url),
            "HEAD" => client.head(&resolved_url),
            _ => {
                return Ok(TaskResult::failure_with_context(
                    &task.id,
                    format!("Unsupported HTTP method: {}", method),
                    "http",
                    ErrorCategory::Config,
                ));
            }
        };

        if let Some(headers) = &http_def.headers {
            for (key, value) in headers {
                let resolved_value = resolve_templates(value, ctx)?;
                request = request.header(key.as_str(), resolved_value);
            }
        }

        if let Some(body) = &http_def.body {
            request = request.json(body);
        }

        let response = request.send().await;

        match response {
            Ok(resp) => {
                let status = resp.status();
                let status_code = status.as_u16();

                let body = resp
                    .text()
                    .await
                    .unwrap_or_else(|e| format!("Failed to read response body: {}", e));

                let truncated_body = if body.len() > 10_000_000 {
                    format!(
                        "{}... [truncated, {} bytes total]",
                        &body[..10000],
                        body.len()
                    )
                } else {
                    body
                };

                if status.is_success() {
                    Ok(TaskResult::success(&task.id, truncated_body, Some(0)))
                } else {
                    Ok(TaskResult::failure_with_context(
                        &task.id,
                        format!(
                            "HTTP {} {}: {}",
                            status_code,
                            status.canonical_reason().unwrap_or(""),
                            truncated_body
                        ),
                        "http",
                        ErrorCategory::Network,
                    ))
                }
            }
            Err(e) => {
                let category = if e.is_timeout() {
                    ErrorCategory::Timeout
                } else if e.is_connect() {
                    ErrorCategory::Network
                } else {
                    ErrorCategory::Execution
                };

                Ok(TaskResult::failure_with_context(
                    &task.id,
                    format!("HTTP request failed: {}", e),
                    "http",
                    category,
                ))
            }
        }
    }

    fn execute_mcp(&self, task: &Task, ctx: &GlobalContext) -> Result<TaskResult> {
        use crate::task::TaskAction;

        let mcp_def = match &task.action {
            TaskAction::Mcp { mcp } => mcp,
            _ => return Ok(TaskResult::failure(&task.id, "Expected mcp task")),
        };

        let args_str = resolve_args(mcp_def.args.as_ref(), ctx)
            .with_context(|| format!("Failed to resolve args for mcp task '{}'", task.id))?;

        Ok(TaskResult::success(
            &task.id,
            format!(
                "[mcp] Would call {} with args: {}",
                mcp_def.reference, args_str
            ),
            Some(0),
        ))
    }

    fn execute_function(&self, task: &Task, ctx: &GlobalContext) -> Result<TaskResult> {
        use crate::task::TaskAction;

        let func_def = match &task.action {
            TaskAction::Function { function } => function,
            _ => return Ok(TaskResult::failure(&task.id, "Expected function task")),
        };

        let args_str = resolve_args(func_def.args.as_ref(), ctx)
            .with_context(|| format!("Failed to resolve args for function task '{}'", task.id))?;

        Ok(TaskResult::success(
            &task.id,
            format!(
                "[function] Would call {} with args: {}",
                func_def.reference, args_str
            ),
            Some(0),
        ))
    }

    async fn execute_llm(&self, task: &Task, _workflow: &Workflow) -> Result<TaskResult> {
        use crate::provider::PromptRequest;
        use crate::task::TaskAction;

        let llm_def = match &task.action {
            TaskAction::Llm { llm } => llm,
            _ => return Ok(TaskResult::failure(&task.id, "Expected llm task")),
        };

        let prompt = &llm_def.prompt;
        let model = llm_def.model.as_deref().unwrap_or("claude-haiku");

        let request = PromptRequest::new(prompt, model).isolated();
        let response = self.provider.execute(request).await?;

        if response.success {
            Ok(TaskResult::success(
                &task.id,
                response.content,
                Some(response.usage.total_tokens),
            ))
        } else {
            Ok(TaskResult::failure_with_context(
                &task.id,
                &response.content,
                "llm",
                ErrorCategory::Provider,
            ))
        }
    }

    fn topological_sort(&self, workflow: &Workflow) -> Result<Vec<String>> {
        let task_count = workflow.tasks.len();
        let mut in_degree: HashMap<&str, usize> = HashMap::with_capacity(task_count);
        let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::with_capacity(task_count);

        for task in &workflow.tasks {
            in_degree.insert(&task.id, 0);
            adjacency.insert(&task.id, Vec::with_capacity(2));
        }

        for flow in &workflow.flows {
            if let Some(adj) = adjacency.get_mut(flow.source.as_str()) {
                adj.push(&flow.target);
            }
            if let Some(deg) = in_degree.get_mut(flow.target.as_str()) {
                *deg += 1;
            }
        }

        let mut queue: Vec<&str> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut result = Vec::with_capacity(task_count);

        while let Some(node) = queue.pop() {
            result.push(node.to_string());

            if let Some(neighbors) = adjacency.get(node) {
                for &neighbor in neighbors {
                    if let Some(deg) = in_degree.get_mut(neighbor) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push(neighbor);
                        }
                    }
                }
            }
        }

        if result.len() != workflow.tasks.len() {
            return Err(anyhow!("Workflow has cycles"));
        }

        Ok(result)
    }
}

// ============================================================================
// TEMPLATE RESOLUTION
// ============================================================================

/// Resolve templates in a string using the execution context
pub fn resolve_templates(template: &str, ctx: &GlobalContext) -> Result<String> {
    crate::template::resolve_templates(template, ctx)
}

/// Resolve task args (YAML -> String with template resolution)
fn resolve_args(args: Option<&serde_json::Value>, ctx: &GlobalContext) -> Result<String> {
    match args {
        Some(args) => {
            let raw_args = serde_json::to_string(args).unwrap_or_default();
            resolve_templates(&raw_args, ctx)
        }
        None => Ok(String::new()),
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::context::ContextReader;
    use crate::workflow::Workflow;

    fn make_workflow_v6() -> Workflow {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test workflow"

tasks:
  - id: step1
    agent:
      prompt: "Analyze this"

  - id: step2
    function:
      reference: "transform::uppercase"

flows:
  - source: step1
    target: step2
"#;
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn test_topological_sort_v6() {
        let workflow = make_workflow_v6();
        let runner = Runner::new("mock").unwrap();
        let order = runner.topological_sort(&workflow).unwrap();
        assert_eq!(order, vec!["step1", "step2"]);
    }

    #[tokio::test]
    async fn test_run_workflow_v6() {
        let workflow = make_workflow_v6();
        let runner = Runner::new("mock").unwrap();
        let result = runner.run(&workflow).await.unwrap();

        assert_eq!(result.tasks_completed, 2, "Should complete 2 tasks");
        assert_eq!(result.tasks_failed, 0, "No tasks should fail");
        assert_eq!(result.results.len(), 2, "Should have 2 results");

        assert!(result.context.get_output("step1").is_some());
        assert!(result.context.get_output("step2").is_some());
    }

    #[tokio::test]
    async fn test_all_7_keywords_execution() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: t1
    agent:
      prompt: "agent task"
  - id: t2
    subagent:
      prompt: "subagent task"
  - id: t3
    shell:
      command: "echo test"
  - id: t4
    http:
      url: "https://example.com"
  - id: t5
    mcp:
      reference: "fs::read"
  - id: t6
    function:
      reference: "tools::fn"
  - id: t7
    llm:
      prompt: "classify"

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock").unwrap();
        let result = runner.run(&workflow).await.unwrap();

        assert_eq!(result.tasks_completed, 7, "All 7 tasks should complete");

        for i in 1..=7 {
            assert!(
                result.context.get_output(&format!("t{}", i)).is_some(),
                "t{} should have output",
                i
            );
        }
    }

    // ========================================================================
    // SECURITY TESTS - SSRF Protection
    // ========================================================================

    #[test]
    fn test_ssrf_blocks_localhost() {
        assert!(validate_http_url("http://localhost/api").is_err());
        assert!(validate_http_url("http://localhost:8080/api").is_err());
        assert!(validate_http_url("http://127.0.0.1/api").is_err());
        assert!(validate_http_url("http://127.0.0.1:3000/api").is_err());
        assert!(validate_http_url("http://[::1]/api").is_err());
        assert!(validate_http_url("https://test.localhost/api").is_err());
        assert!(validate_http_url("http://app.localdomain/api").is_err());
    }

    #[test]
    fn test_ssrf_blocks_private_ipv4() {
        // 10.0.0.0/8
        assert!(validate_http_url("http://10.0.0.1/api").is_err());
        assert!(validate_http_url("http://10.255.255.255/api").is_err());
        // 172.16.0.0/12
        assert!(validate_http_url("http://172.16.0.1/api").is_err());
        assert!(validate_http_url("http://172.31.255.255/api").is_err());
        // 192.168.0.0/16
        assert!(validate_http_url("http://192.168.0.1/api").is_err());
        assert!(validate_http_url("http://192.168.255.255/api").is_err());
        // 169.254.0.0/16 (link-local)
        assert!(validate_http_url("http://169.254.1.1/api").is_err());
    }

    #[test]
    fn test_ssrf_blocks_private_ipv6() {
        // ULA fc00::/7
        assert!(validate_http_url("http://[fc00::1]/api").is_err());
        assert!(validate_http_url("http://[fd00::1]/api").is_err());
        // Link-local fe80::/10
        assert!(validate_http_url("http://[fe80::1]/api").is_err());
        // Site-local fec0::/10 (deprecated but blocked)
        assert!(validate_http_url("http://[fec0::1]/api").is_err());
    }

    #[test]
    fn test_ssrf_blocks_cloud_metadata() {
        // AWS/GCP/Azure metadata
        assert!(validate_http_url("http://169.254.169.254/latest/meta-data").is_err());
        assert!(validate_http_url("http://metadata.internal/api").is_err());
        assert!(validate_http_url("http://metadata.google.internal/").is_err());
        assert!(validate_http_url("http://metadata.goog/").is_err());
    }

    #[test]
    fn test_ssrf_blocks_kubernetes() {
        assert!(validate_http_url("http://app.default.svc.cluster.local/api").is_err());
        assert!(validate_http_url("http://10-0-0-1.default.pod.cluster.local/api").is_err());
    }

    #[test]
    fn test_ssrf_blocks_url_encoding() {
        // Hex encoding in hostname
        assert!(validate_http_url("http://0x7f.0x0.0x0.0x1/api").is_err());
        // URL-encoded hostname
        assert!(validate_http_url("http://127%2e0%2e0%2e1/api").is_err());
    }

    #[test]
    fn test_ssrf_blocks_non_http_schemes() {
        assert!(validate_http_url("file:///etc/passwd").is_err());
        assert!(validate_http_url("ftp://example.com/file").is_err());
        assert!(validate_http_url("gopher://example.com/").is_err());
        assert!(validate_http_url("dict://example.com/d:word").is_err());
        assert!(validate_http_url("ldap://example.com/").is_err());
    }

    #[test]
    fn test_ssrf_allows_public_urls() {
        assert!(validate_http_url("https://api.example.com/v1").is_ok());
        assert!(validate_http_url("http://httpbin.org/get").is_ok());
        assert!(validate_http_url("https://8.8.8.8/dns").is_ok());
        assert!(validate_http_url("https://google.com/").is_ok());
        assert!(validate_http_url("http://[2607:f8b0:4000::1]/").is_ok()); // Public IPv6
    }

    #[test]
    fn test_is_private_ip_v4() {
        use std::net::{IpAddr, Ipv4Addr};

        // Private ranges
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))));

        // Public
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
    }

    #[test]
    fn test_is_private_ip_v6() {
        use std::net::{IpAddr, Ipv6Addr};

        // Private/special
        assert!(is_private_ip(&IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(is_private_ip(&IpAddr::V6(Ipv6Addr::UNSPECIFIED)));

        // ULA (fc00::/7)
        let ula = Ipv6Addr::new(0xfc00, 0, 0, 0, 0, 0, 0, 1);
        assert!(is_private_ip(&IpAddr::V6(ula)));

        // Link-local (fe80::/10)
        let link_local = Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1);
        assert!(is_private_ip(&IpAddr::V6(link_local)));

        // Public
        let public = Ipv6Addr::new(0x2607, 0xf8b0, 0x4000, 0, 0, 0, 0, 1);
        assert!(!is_private_ip(&IpAddr::V6(public)));
    }
}
