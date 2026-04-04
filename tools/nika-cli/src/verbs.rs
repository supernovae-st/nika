//! Direct verb execution from CLI — no YAML needed.
//!
//! Handles `nika infer`, `nika fetch`, `nika invoke`, `nika agent` commands.
//! Each verb creates a one-shot TaskExecutor and dispatches a single TaskAction.

use std::io::{IsTerminal, Read};
use std::sync::Arc;
use std::time::Instant;

use colored::Colorize;

use nika_engine::ast::output::{OutputFormat, OutputPolicy};
use nika_engine::ast::{
    AgentParams, FetchParams, InferParams, InvokeParams, ResponseFormat, TaskAction,
};
use nika_engine::binding::ResolvedBindings;
use nika_engine::error::NikaError;
use nika_engine::event::EventLog;
use nika_engine::runtime::TaskExecutor;
use nika_engine::store::RunContext;

// ═══════════════════════════════════════════════════════════════════════════
// PROVIDER AUTO-DETECTION
// ═══════════════════════════════════════════════════════════════════════════

/// Auto-detect provider from environment variables and vault (priority order).
///
/// Checks env vars first (fast), then NikaVault for keys stored via `nika provider set`.
pub fn detect_provider() -> Option<String> {
    use nika_engine::core::providers::{providers_by_category, ProviderCategory};

    // Check store + env vars first (fast path)
    for provider in providers_by_category(ProviderCategory::Llm) {
        if nika_engine::secrets::has_provider_key(provider) {
            return Some(provider.id.to_string());
        }
    }

    // Check NikaVault (keys stored via `nika provider set`)
    let nika_home = std::env::var("NIKA_HOME")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".nika"));
    let vault = nika_vault::NikaVault::new(&nika_home.join("secrets"));
    use secrecy::ExposeSecret;
    let vault_providers = [
        "anthropic",
        "openai",
        "xai",
        "gemini",
        "mistral",
        "groq",
        "deepseek",
    ];
    for provider in &vault_providers {
        if let Ok(Some(secret)) = vault.get(provider) {
            if !secret.expose_secret().is_empty() {
                // Set env var so downstream RigProvider can find it
                let env_var =
                    nika_engine::core::provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY");
                nika_engine::secrets::inject_secret_to_env(env_var, secret.expose_secret());
                return Some(provider.to_string());
            }
        }
    }

    None
}

/// Resolve the default model name for a provider (for display purposes).
///
/// Returns the well-known default model for each LLM provider so the CLI
/// header shows the actual model name instead of "(default)".
fn default_model_for_provider(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "claude-sonnet-4-6",
        "openai" => "gpt-4o",
        "mistral" => "mistral-large-latest",
        "groq" => "llama-3.3-70b-versatile",
        "deepseek" => "deepseek-chat",
        "gemini" => "gemini-2.0-flash",
        "xai" => "grok-3-fast",
        _ => "default",
    }
}

/// Parse composite model identifier: "anthropic/claude-sonnet" → (Some("anthropic"), "claude-sonnet")
pub fn parse_composite_model(model: &str) -> Result<(Option<&str>, &str), NikaError> {
    match model.split_once('/') {
        Some((provider, model_name)) => {
            if provider.is_empty() || model_name.is_empty() || model_name.contains('/') {
                Err(NikaError::ValidationError {
                    reason: format!(
                        "Invalid model format '{}'. Expected 'provider/model' or 'model'",
                        model
                    ),
                })
            } else {
                Ok((Some(provider), model_name))
            }
        }
        None => Ok((None, model)),
    }
}

/// Resolve provider + model from flags with auto-detection.
pub(crate) fn resolve_provider_model(
    provider_flag: Option<&str>,
    model_flag: Option<&str>,
) -> Result<(String, Option<String>), NikaError> {
    // If model has composite syntax (provider/model), extract both
    if let Some(model) = model_flag {
        let (composite_provider, model_name) = parse_composite_model(model)?;
        if let Some(cp) = composite_provider {
            return Ok((cp.to_string(), Some(model_name.to_string())));
        }
        // Model specified without provider — use provider flag or auto-detect
        let provider = provider_flag
            .map(|s| s.to_string())
            .or_else(detect_provider)
            .ok_or_else(|| NikaError::ValidationError {
                reason: "No provider configured. Set an API key env var or use -p <provider>\n\
                         Fix: export ANTHROPIC_API_KEY=sk-ant-..."
                    .to_string(),
            })?;
        return Ok((provider, Some(model_name.to_string())));
    }

    // No model specified — use provider flag or auto-detect
    let provider = provider_flag
        .map(|s| s.to_string())
        .or_else(detect_provider)
        .ok_or_else(|| NikaError::ValidationError {
            reason: "No provider configured. Set an API key env var or use -p <provider>\n\
                     Fix: export ANTHROPIC_API_KEY=sk-ant-..."
                .to_string(),
        })?;
    Ok((provider, None))
}

/// Create a one-shot TaskExecutor with custom endpoint + policy support.
async fn one_shot_executor(
    provider: &str,
    model: Option<&str>,
) -> Result<(TaskExecutor, EventLog), NikaError> {
    let event_log = EventLog::new();
    // Load config to resolve custom endpoints (e.g., h100, ollama)
    let custom_endpoints = nika_engine::config::NikaConfig::load()
        .ok()
        .and_then(|cfg| cfg.resolve_endpoints().ok())
        .filter(|m| !m.is_empty());
    // Load [policy] from nika.toml (allowed_hosts, blocked_hosts, etc.)
    let policy = nika_engine::runtime::boot::load_policy_config();
    let executor = TaskExecutor::with_policy(
        provider,
        model,
        None,
        event_log.clone(),
        Some(policy),
        None,
        custom_endpoints,
    )?;
    Ok((executor, event_log))
}

/// Read stdin content (spawn_blocking + 10MB limit to prevent OOM).
async fn read_stdin_content() -> Result<String, NikaError> {
    const MAX_STDIN_SIZE: u64 = 10 * 1024 * 1024; // 10 MB
    tokio::task::spawn_blocking(|| {
        let mut buf = String::new();
        std::io::stdin()
            .take(MAX_STDIN_SIZE)
            .read_to_string(&mut buf)
            .map_err(|e| NikaError::ParseError {
                details: format!("Failed to read stdin: {}", e),
            })?;
        Ok(buf)
    })
    .await
    .map_err(|e| NikaError::ParseError {
        details: format!("stdin reader panicked: {}", e),
    })?
}

// ═══════════════════════════════════════════════════════════════════════════
// VISUAL OUTPUT
// ═══════════════════════════════════════════════════════════════════════════

/// Print verb header with icon.
fn print_verb_header(verb_name: &str, label: &str, is_tty: bool) {
    if is_tty {
        let icon = nika_engine::display::icons::verb(verb_name);
        eprintln!("\n  {} {} {}", "┌─".dimmed(), icon, label.cyan());
    }
}

/// Print verb footer with timing, TTFT, tokens, and cost.
fn print_verb_footer(
    elapsed: std::time::Duration,
    ttft_ms: Option<u64>,
    tokens: u64,
    cost: f64,
    extra: &str,
    is_tty: bool,
) {
    if is_tty {
        let mut parts = vec![];

        // TTFT (if available)
        if let Some(ttft) = ttft_ms {
            parts.push(format!("TTFT {}ms", ttft));
        }

        // Total time
        parts.push(format!("{}ms", elapsed.as_millis()));

        // Tokens
        if tokens > 0 {
            parts.push(format!("{} tokens", tokens));
        }

        // Cost (green cheap, yellow moderate, red expensive)
        if cost > 0.0 {
            let cost_str = format!("${:.4}", cost);
            let colored_cost = if cost < 0.01 {
                cost_str.green()
            } else if cost < 0.10 {
                cost_str.yellow()
            } else {
                cost_str.red()
            };
            parts.push(format!("{}", colored_cost));
        }

        // Extra info
        if !extra.is_empty() {
            parts.push(extra.to_string());
        }

        eprintln!("  {} {}", "└─".dimmed(), parts.join(" · ").dimmed());
        eprintln!();
    }
}

/// Extract TTFT and token/cost info from EventLog.
fn extract_llm_metrics(event_log: &EventLog) -> (Option<u64>, u64, f64) {
    let events = event_log.events();
    let mut ttft_ms = None;
    let mut total_tokens = 0u64;
    let mut total_cost = 0.0f64;
    for event in &events {
        if let nika_engine::event::EventKind::ProviderResponded {
            input_tokens,
            output_tokens,
            cost_usd,
            ttft_ms: event_ttft,
            ..
        } = &event.kind
        {
            total_tokens += input_tokens + output_tokens;
            total_cost += cost_usd;
            if ttft_ms.is_none() {
                ttft_ms = *event_ttft;
            }
        }
    }
    (ttft_ms, total_tokens, total_cost)
}

/// Pretty-print JSON output on TTY (indented with colored keys).
fn print_output(output: &str, is_tty: bool) {
    if is_tty {
        // Try to parse as JSON for pretty-printing
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(output) {
            if let Ok(pretty) = serde_json::to_string_pretty(&value) {
                println!("{pretty}");
                return;
            }
        }
    }
    println!("{output}");
}

// ═══════════════════════════════════════════════════════════════════════════
// VERB HANDLERS
// ═══════════════════════════════════════════════════════════════════════════

/// Handle `nika infer "prompt"` — one-shot LLM call.
#[allow(clippy::too_many_arguments)]
pub async fn handle_infer(
    prompt: String,
    provider: Option<String>,
    model: Option<String>,
    system: Option<String>,
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    json_mode: bool,
    from_example: Option<String>,
    read_stdin: bool,
    quiet: bool,
) -> Result<(), NikaError> {
    let is_tty = std::io::stdout().is_terminal();

    // Read stdin if requested
    let full_prompt = if read_stdin || prompt == "-" {
        let stdin_content = read_stdin_content().await?;
        if prompt == "-" {
            stdin_content
        } else {
            format!("{}\n\n{}", stdin_content.trim(), prompt)
        }
    } else {
        prompt
    };

    // Resolve provider + model
    let (provider_name, model_name) =
        resolve_provider_model(provider.as_deref(), model.as_deref())?;

    // Build InferParams
    let infer = InferParams {
        prompt: full_prompt,
        system,
        temperature,
        max_tokens,
        response_format: if json_mode {
            Some(ResponseFormat::Json)
        } else {
            None
        },
        ..Default::default()
    };

    // Build output policy for structured output (from_example)
    let output_policy = if let Some(ref example) = from_example {
        let spec = if example.starts_with('{') || example.starts_with('[') {
            let value: serde_json::Value =
                serde_json::from_str(example).map_err(|e| NikaError::ParseError {
                    details: format!("Invalid JSON in --from-example: {}", e),
                })?;
            nika_engine::ast::StructuredOutputSpec::with_example_inline(value)
        } else {
            nika_engine::ast::StructuredOutputSpec::with_example_file(example)
        };
        Some(spec.to_output_policy())
    } else if json_mode {
        Some(OutputPolicy {
            format: OutputFormat::Json,
            schema: None,
            from_example: None,
            max_retries: None,
            source_structured_spec: None,
        })
    } else {
        None
    };

    let action = TaskAction::Infer { infer };
    let task_id: Arc<str> = Arc::from("cli");

    // Show header — resolve actual default model name instead of "(default)"
    let display_model = model_name
        .as_deref()
        .unwrap_or_else(|| default_model_for_provider(&provider_name));
    if !quiet {
        print_verb_header(
            "infer",
            &format!("{} via {}", display_model, provider_name),
            is_tty,
        );
    }

    // Execute with spinner on TTY
    let (executor, event_log) = one_shot_executor(&provider_name, model_name.as_deref()).await?;
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();
    let start = Instant::now();

    let spinner = if is_tty && !quiet {
        let sp = indicatif::ProgressBar::new_spinner();
        sp.set_style(
            indicatif::ProgressStyle::default_spinner()
                .tick_strings(nika_engine::display::spinner::TICK_STRINGS)
                .template("  {spinner} {msg}")
                .unwrap(),
        );
        sp.set_message("Inferring...");
        sp.enable_steady_tick(nika_engine::display::spinner::TICK_INTERVAL);
        Some(sp)
    } else {
        None
    };

    let output = executor
        .execute(
            &task_id,
            &action,
            &bindings,
            &datastore,
            output_policy.as_ref(),
        )
        .await?;
    let elapsed = start.elapsed();

    if let Some(sp) = spinner {
        sp.finish_and_clear();
    }

    // Print output (pretty-print JSON on TTY)
    print_output(&output, is_tty);

    // Footer with TTFT, tokens, cost
    if !quiet {
        let (ttft_ms, tokens, cost) = extract_llm_metrics(&event_log);
        print_verb_footer(elapsed, ttft_ms, tokens, cost, "", is_tty);
    }

    Ok(())
}

/// Handle `nika fetch URL` — HTTP request with extraction.
#[allow(clippy::too_many_arguments)]
pub async fn handle_fetch(
    url: String,
    extract: Option<String>,
    selector: Option<String>,
    method: Option<String>,
    headers: Vec<String>,
    body: Option<String>,
    json_body: Option<String>,
    response: Option<String>,
    timeout: Option<u64>,
    quiet: bool,
) -> Result<(), NikaError> {
    let is_tty = std::io::stdout().is_terminal();

    // Parse headers
    let mut header_map = rustc_hash::FxHashMap::default();
    for h in &headers {
        let (key, value) = h
            .split_once(':')
            .ok_or_else(|| NikaError::ValidationError {
                reason: format!("Invalid header '{}', expected KEY:VALUE", h),
            })?;
        header_map.insert(key.trim().to_string(), value.trim().to_string());
    }

    // Parse JSON body
    let json_value = json_body
        .map(|j| serde_json::from_str(&j))
        .transpose()
        .map_err(|e| NikaError::ParseError {
            details: format!("Invalid --json-body: {}", e),
        })?;

    let extract_mode = extract
        .as_deref()
        .map(|s| {
            nika_engine::ast::extract::ExtractMode::parse(s).ok_or_else(|| {
                NikaError::ValidationError {
                    reason: format!(
                        "unknown extract mode '{}', expected one of: {}",
                        s,
                        nika_engine::ast::extract::ExtractMode::ALL_NAMES.join(", ")
                    ),
                }
            })
        })
        .transpose()?;
    let response_mode = response
        .as_deref()
        .map(|s| {
            nika_engine::ast::extract::ResponseMode::parse(s).ok_or_else(|| {
                NikaError::ValidationError {
                    reason: format!(
                        "unknown response mode '{}', expected one of: {}",
                        s,
                        nika_engine::ast::extract::ResponseMode::ALL_NAMES.join(", ")
                    ),
                }
            })
        })
        .transpose()?;

    let fetch = FetchParams {
        url: url.clone(),
        method: method.unwrap_or_else(|| "GET".to_string()),
        headers: header_map,
        body,
        json: json_value,
        timeout,
        extract: extract_mode,
        selector,
        response: response_mode,
        retry: None,
        follow_redirects: None,
        session: None,
        cache: None,
    };

    let action = TaskAction::Fetch { fetch };
    let task_id: Arc<str> = Arc::from("cli");

    let extract_label = extract.as_deref().unwrap_or("raw");
    if !quiet {
        print_verb_header("fetch", &format!("{} · {}", url, extract_label), is_tty);
    }

    // Fetch doesn't need a real LLM provider — use "mock"
    let (executor, _) = one_shot_executor("mock", None).await?;
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();
    let start = Instant::now();
    let output = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await?;
    let elapsed = start.elapsed();

    // Pretty-print JSON on TTY (metadata, jsonpath, feed modes produce JSON)
    print_output(&output, is_tty);

    if !quiet {
        let extra = format!("{} bytes", output.len());
        print_verb_footer(elapsed, None, 0, 0.0, &extra, is_tty);
    }

    Ok(())
}

/// Handle `nika invoke tool` — call builtin nika:* or MCP tool.
pub async fn handle_invoke(
    tool: Option<String>,
    file: Option<String>,
    params: Option<String>,
    mcp: Option<String>,
    timeout: Option<u64>,
    list_tools: bool,
    quiet: bool,
) -> Result<(), NikaError> {
    // --list: show available tools and exit
    if list_tools {
        println!("{}", "Builtin Tools (nika:*)".bold());
        println!("{}", "─".repeat(50));
        println!("  Tier 1 (always on):");
        for t in [
            "import",
            "dimensions",
            "thumbhash",
            "dominant_color",
            "pipeline",
        ] {
            println!("    nika:{}", t);
        }
        println!("  Tier 2 (media-core):");
        for t in [
            "thumbnail",
            "convert",
            "strip",
            "metadata",
            "optimize",
            "svg_render",
        ] {
            println!("    nika:{}", t);
        }
        println!("  Tier 3 (opt-in):");
        for t in [
            "phash",
            "compare",
            "pdf_extract",
            "chart",
            "provenance",
            "verify",
            "qr_validate",
            "quality",
            "html_to_md",
            "css_select",
            "extract_metadata",
            "extract_links",
            "readability",
        ] {
            println!("    nika:{}", t);
        }
        println!();
        println!("  Use: nika invoke nika:<tool> [file] [--params JSON]");
        println!("  Full details: nika media tools");
        return Ok(());
    }

    let tool = tool.ok_or_else(|| NikaError::ValidationError {
        reason:
            "Tool name required. Use: nika invoke nika:dimensions file.jpg\nOr: nika invoke --list"
                .to_string(),
    })?;

    let is_tty = std::io::stdout().is_terminal();

    // Build params: merge positional file arg as "source"
    let mut tool_params = if let Some(ref p) = params {
        serde_json::from_str(p).map_err(|e| NikaError::ParseError {
            details: format!("Invalid --params JSON: {}", e),
        })?
    } else {
        serde_json::json!({})
    };

    if let Some(ref f) = file {
        if let Some(obj) = tool_params.as_object_mut() {
            if !obj.contains_key("source") {
                obj.insert("source".to_string(), serde_json::json!(f));
            }
        }
    }

    // Parse tool name: "server::tool" or "nika:tool"
    let (mcp_name, tool_name) = if tool.contains("::") {
        let (s, t) = tool.split_once("::").unwrap();
        (Some(s.to_string()), t.to_string())
    } else {
        (mcp, tool.clone())
    };

    let invoke = InvokeParams {
        tool: Some(tool_name),
        params: Some(tool_params),
        mcp: mcp_name,
        resource: None,
        timeout,
    };

    let action = TaskAction::Invoke { invoke };
    let task_id: Arc<str> = Arc::from("cli");

    if !quiet {
        print_verb_header("invoke", &tool, is_tty);
    }

    let (executor, _) = one_shot_executor("mock", None).await?;
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();
    let start = Instant::now();
    let output = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await?;
    let elapsed = start.elapsed();

    // Pretty-print JSON results on TTY
    print_output(&output, is_tty);

    if !quiet {
        print_verb_footer(elapsed, None, 0, 0.0, "", is_tty);
    }

    Ok(())
}

/// Handle `nika agent "prompt"` — multi-turn AI agent.
#[allow(clippy::too_many_arguments)]
pub async fn handle_agent(
    prompt: String,
    provider: Option<String>,
    model: Option<String>,
    system: Option<String>,
    tools: Vec<String>,
    mcp_servers: Vec<String>,
    turns: u32,
    max_tokens: Option<u32>,
    temperature: Option<f64>,
    read_stdin: bool,
    quiet: bool,
) -> Result<(), NikaError> {
    let is_tty = std::io::stdout().is_terminal();

    let full_prompt = if read_stdin || prompt == "-" {
        let stdin_content = read_stdin_content().await?;
        if prompt == "-" {
            stdin_content
        } else {
            format!("{}\n\n{}", stdin_content.trim(), prompt)
        }
    } else {
        prompt
    };

    let (provider_name, model_name) =
        resolve_provider_model(provider.as_deref(), model.as_deref())?;

    let agent = AgentParams {
        prompt: full_prompt,
        system,
        provider: Some(nika_core::ProviderName::parse(&provider_name)),
        model: model_name.clone(),
        tools,
        mcp: mcp_servers,
        max_turns: Some(turns),
        max_tokens,
        temperature: temperature.map(|t| t as f32),
        ..Default::default()
    };

    let action = TaskAction::Agent { agent };
    let task_id: Arc<str> = Arc::from("cli");

    // Resolve actual default model name instead of "(default)"
    let display_model = model_name
        .as_deref()
        .unwrap_or_else(|| default_model_for_provider(&provider_name));
    if !quiet {
        print_verb_header(
            "agent",
            &format!(
                "agent · {} via {} · {} turns",
                display_model, provider_name, turns
            ),
            is_tty,
        );
    }

    let (executor, event_log) = one_shot_executor(&provider_name, model_name.as_deref()).await?;
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();
    let start = Instant::now();

    let spinner = if is_tty && !quiet {
        let sp = indicatif::ProgressBar::new_spinner();
        sp.set_style(
            indicatif::ProgressStyle::default_spinner()
                .tick_strings(nika_engine::display::spinner::TICK_STRINGS)
                .template("  {spinner} {msg}")
                .unwrap(),
        );
        sp.set_message("Agent running...");
        sp.enable_steady_tick(nika_engine::display::spinner::TICK_INTERVAL);
        Some(sp)
    } else {
        None
    };

    let output = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await?;
    let elapsed = start.elapsed();

    if let Some(sp) = spinner {
        sp.finish_and_clear();
    }

    // Pretty-print JSON on TTY
    print_output(&output, is_tty);

    if !quiet {
        let (ttft_ms, total_tokens, total_cost) = extract_llm_metrics(&event_log);
        print_verb_footer(elapsed, ttft_ms, total_tokens, total_cost, "", is_tty);
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::sync::Mutex;

    /// All env-var-mutating tests must hold this lock so they don't race.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Provider env var names in priority order (must match `detect_provider`).
    const PROVIDER_VARS: [&str; 7] = [
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "MISTRAL_API_KEY",
        "GROQ_API_KEY",
        "DEEPSEEK_API_KEY",
        "GEMINI_API_KEY",
        "XAI_API_KEY",
    ];

    /// Clear all provider env vars + store, returning their previous values for restore.
    fn clear_provider_env() -> Vec<(String, Option<String>)> {
        PROVIDER_VARS
            .iter()
            .map(|var| {
                let prev = std::env::var(var).ok();
                // SAFETY: single-threaded access guaranteed by ENV_LOCK mutex
                unsafe { std::env::remove_var(var) };
                // Also clear the in-process SecretStore
                nika_engine::secrets::store::remove_secret(var);
                (var.to_string(), prev)
            })
            .collect()
    }

    /// Restore previously saved env vars.
    fn restore_provider_env(saved: Vec<(String, Option<String>)>) {
        for (var, val) in saved {
            match val {
                Some(v) => nika_engine::secrets::inject_secret_to_env(&var, &v),
                // SAFETY: remove_var in single-threaded test context (ENV_LOCK held)
                None => unsafe { std::env::remove_var(&var) },
            }
        }
    }

    // ───────────────────────────────────────────────────────────────────────
    // parse_composite_model
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn parse_composite_provider_and_model() {
        let (provider, model) = parse_composite_model("anthropic/claude-sonnet").unwrap();
        assert_eq!(provider, Some("anthropic"));
        assert_eq!(model, "claude-sonnet");
    }

    #[test]
    fn parse_composite_plain_model() {
        let (provider, model) = parse_composite_model("gpt-4o").unwrap();
        assert_eq!(provider, None);
        assert_eq!(model, "gpt-4o");
    }

    #[test]
    fn parse_composite_empty_string() {
        let (provider, model) = parse_composite_model("").unwrap();
        assert_eq!(provider, None);
        assert_eq!(model, "");
    }

    #[test]
    fn parse_composite_slash_only_is_error() {
        let err = parse_composite_model("/").unwrap_err();
        assert!(
            err.to_string().contains("Invalid model format"),
            "expected validation error, got: {}",
            err
        );
    }

    #[test]
    fn parse_composite_empty_model_is_error() {
        let err = parse_composite_model("anthropic/").unwrap_err();
        assert!(
            err.to_string().contains("Invalid model format"),
            "expected validation error, got: {}",
            err
        );
    }

    #[test]
    fn parse_composite_empty_provider_is_error() {
        let err = parse_composite_model("/claude").unwrap_err();
        assert!(
            err.to_string().contains("Invalid model format"),
            "expected validation error, got: {}",
            err
        );
    }

    #[test]
    fn parse_composite_multiple_slashes_is_error() {
        let err = parse_composite_model("a/b/c").unwrap_err();
        assert!(
            err.to_string().contains("Invalid model format"),
            "expected validation error, got: {}",
            err
        );
    }

    #[test]
    fn parse_composite_openai_with_model() {
        let (provider, model) = parse_composite_model("openai/gpt-4.1").unwrap();
        assert_eq!(provider, Some("openai"));
        assert_eq!(model, "gpt-4.1");
    }

    // ───────────────────────────────────────────────────────────────────────
    // detect_provider
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    #[serial]
    fn detect_provider_returns_none_when_no_keys() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = clear_provider_env();

        let result = detect_provider();
        assert_eq!(result, None);

        restore_provider_env(saved);
    }

    #[test]
    #[serial]
    fn detect_provider_returns_anthropic_when_set() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = clear_provider_env();

        // SAFETY: single-threaded access guaranteed by ENV_LOCK
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test") };
        let result = detect_provider();
        assert_eq!(result.as_deref(), Some("anthropic"));

        restore_provider_env(saved);
    }

    #[test]
    #[serial]
    fn detect_provider_priority_anthropic_over_openai() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = clear_provider_env();

        // SAFETY: single-threaded access guaranteed by ENV_LOCK
        unsafe {
            std::env::set_var("OPENAI_API_KEY", "sk-openai-test");
            std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test");
        }
        let result = detect_provider();
        assert_eq!(
            result.as_deref(),
            Some("anthropic"),
            "anthropic should win over openai in priority order"
        );

        restore_provider_env(saved);
    }

    #[test]
    #[serial]
    fn detect_provider_falls_through_to_openai() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = clear_provider_env();

        // SAFETY: single-threaded access guaranteed by ENV_LOCK
        unsafe { std::env::set_var("OPENAI_API_KEY", "sk-openai-test") };
        let result = detect_provider();
        assert_eq!(result.as_deref(), Some("openai"));

        restore_provider_env(saved);
    }

    #[test]
    #[serial]
    fn detect_provider_ignores_empty_and_whitespace_keys() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = clear_provider_env();

        // SAFETY: single-threaded access guaranteed by ENV_LOCK
        unsafe {
            std::env::set_var("ANTHROPIC_API_KEY", "");
            std::env::set_var("OPENAI_API_KEY", "   ");
            std::env::set_var("MISTRAL_API_KEY", "sk-mis-test");
        }
        let result = detect_provider();
        assert_eq!(
            result.as_deref(),
            Some("mistral"),
            "empty/whitespace keys should be skipped"
        );

        restore_provider_env(saved);
    }

    #[test]
    #[serial]
    fn detect_provider_xai_last_resort() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = clear_provider_env();

        // SAFETY: single-threaded access guaranteed by ENV_LOCK
        unsafe { std::env::set_var("XAI_API_KEY", "xai-test") };
        let result = detect_provider();
        assert_eq!(result.as_deref(), Some("xai"));

        restore_provider_env(saved);
    }

    // ───────────────────────────────────────────────────────────────────────
    // resolve_provider_model
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    #[serial]
    fn resolve_explicit_provider_flag_wins() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = clear_provider_env();

        // Even with ANTHROPIC_API_KEY set, explicit flag should win
        // SAFETY: single-threaded access guaranteed by ENV_LOCK
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test") };

        let (provider, model) = resolve_provider_model(Some("openai"), Some("gpt-4o")).unwrap();
        assert_eq!(provider, "openai");
        assert_eq!(model.as_deref(), Some("gpt-4o"));

        restore_provider_env(saved);
    }

    #[test]
    #[serial]
    fn resolve_composite_model_extracts_both() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = clear_provider_env();

        let (provider, model) =
            resolve_provider_model(None, Some("anthropic/claude-sonnet")).unwrap();
        assert_eq!(provider, "anthropic");
        assert_eq!(model.as_deref(), Some("claude-sonnet"));

        restore_provider_env(saved);
    }

    #[test]
    #[serial]
    fn resolve_composite_model_ignores_provider_flag() {
        // Composite syntax should take precedence over the -p flag
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = clear_provider_env();

        let (provider, model) =
            resolve_provider_model(Some("openai"), Some("anthropic/claude-sonnet")).unwrap();
        assert_eq!(
            provider, "anthropic",
            "composite model provider should override -p flag"
        );
        assert_eq!(model.as_deref(), Some("claude-sonnet"));

        restore_provider_env(saved);
    }

    #[test]
    #[serial]
    fn resolve_model_only_uses_env_auto_detect() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = clear_provider_env();

        // SAFETY: single-threaded access guaranteed by ENV_LOCK
        unsafe { std::env::set_var("GROQ_API_KEY", "gsk-test") };

        let (provider, model) = resolve_provider_model(None, Some("llama-4-maverick")).unwrap();
        assert_eq!(provider, "groq");
        assert_eq!(model.as_deref(), Some("llama-4-maverick"));

        restore_provider_env(saved);
    }

    #[test]
    #[serial]
    fn resolve_no_provider_no_env_is_error() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = clear_provider_env();

        let err = resolve_provider_model(None, Some("gpt-4o")).unwrap_err();
        assert!(
            err.to_string().contains("No provider configured"),
            "expected help message, got: {}",
            err
        );

        restore_provider_env(saved);
    }

    #[test]
    #[serial]
    fn resolve_no_provider_no_model_no_env_is_error() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = clear_provider_env();

        let err = resolve_provider_model(None, None).unwrap_err();
        assert!(
            err.to_string().contains("No provider configured"),
            "expected help message, got: {}",
            err
        );

        restore_provider_env(saved);
    }

    #[test]
    #[serial]
    fn resolve_provider_flag_only_no_model() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = clear_provider_env();

        let (provider, model) = resolve_provider_model(Some("anthropic"), None).unwrap();
        assert_eq!(provider, "anthropic");
        assert_eq!(model, None, "no model specified means None");

        restore_provider_env(saved);
    }

    #[test]
    #[serial]
    fn resolve_invalid_composite_model_propagates_error() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = clear_provider_env();

        let err = resolve_provider_model(None, Some("a/b/c")).unwrap_err();
        assert!(
            err.to_string().contains("Invalid model format"),
            "should propagate parse_composite_model error, got: {}",
            err
        );

        restore_provider_env(saved);
    }

    // ───────────────────────────────────────────────────────────────────────
    // Phase 10: verb display utilities
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn extract_llm_metrics_from_empty_log() {
        let log = EventLog::new();
        let (ttft, tokens, cost) = extract_llm_metrics(&log);
        assert_eq!(ttft, None);
        assert_eq!(tokens, 0);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn extract_llm_metrics_from_provider_responded() {
        let log = EventLog::new();
        log.emit(nika_engine::event::EventKind::ProviderResponded {
            task_id: "cli".into(),
            request_id: Some("req-1".into()),
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 0,
            ttft_ms: Some(187),
            finish_reason: nika_engine::event::types::FinishReason::Stop,
            cost_usd: 0.004,
        });
        let (ttft, tokens, cost) = extract_llm_metrics(&log);
        assert_eq!(ttft, Some(187));
        assert_eq!(tokens, 150);
        assert!((cost - 0.004).abs() < f64::EPSILON);
    }

    // ───────────────────────────────────────────────────────────────────────
    // Phase 10: verb display tests (44-50)
    // ───────────────────────────────────────────────────────────────────────

    // Test 44: verb header includes correct icon per verb type
    #[test]
    fn verb_header_includes_icon() {
        // Verify the icon function returns non-empty colored strings for all verbs
        use nika_engine::display::icons::verb;
        let infer_icon = verb("infer");
        let fetch_icon = verb("fetch");
        let invoke_icon = verb("invoke");
        let agent_icon = verb("agent");
        let exec_icon = verb("exec");

        // Each verb should produce a non-empty string
        assert!(!infer_icon.to_string().is_empty(), "infer icon");
        assert!(!fetch_icon.to_string().is_empty(), "fetch icon");
        assert!(!invoke_icon.to_string().is_empty(), "invoke icon");
        assert!(!agent_icon.to_string().is_empty(), "agent icon");
        assert!(!exec_icon.to_string().is_empty(), "exec icon");

        // Icons should be different for each verb
        let icons: std::collections::HashSet<String> = [
            infer_icon.to_string(),
            fetch_icon.to_string(),
            invoke_icon.to_string(),
            agent_icon.to_string(),
            exec_icon.to_string(),
        ]
        .into_iter()
        .collect();
        assert_eq!(icons.len(), 5, "all 5 verb icons should be distinct");
    }

    // Test 45: verb footer shows TTFT when available
    #[test]
    fn verb_footer_shows_ttft() {
        let log = EventLog::new();
        log.emit(nika_engine::event::EventKind::ProviderResponded {
            task_id: "cli".into(),
            request_id: None,
            input_tokens: 200,
            output_tokens: 100,
            cache_read_tokens: 0,
            ttft_ms: Some(250),
            finish_reason: nika_engine::event::types::FinishReason::Stop,
            cost_usd: 0.003,
        });
        let (ttft, tokens, cost) = extract_llm_metrics(&log);
        assert_eq!(ttft, Some(250), "TTFT should be extracted from EventLog");
        assert_eq!(tokens, 300, "tokens should sum input+output");
        assert!((cost - 0.003).abs() < f64::EPSILON, "cost should match");
    }

    // Test 46: JSON output is pretty-printed on TTY (no panic)
    #[test]
    fn json_output_pretty_printed_on_tty() {
        let json = r#"{"name":"Alice","skills":["rust","python"]}"#;
        // Should not panic — on TTY it pretty-prints, off TTY it passes through
        print_output(json, true);
    }

    // Test 47: JSON output is raw when piped (no formatting)
    #[test]
    fn json_output_raw_when_piped() {
        let json = r#"{"key":"value"}"#;
        // When not TTY, output should pass through unchanged
        // We can't capture stdout easily, but verify no panic
        print_output(json, false);
        // Also verify non-JSON passes through
        print_output("plain text output", false);
    }

    // Test 50: invoke result formatted as key-value on TTY (no panic)
    #[test]
    fn invoke_result_formatted_as_kv() {
        // JSON output from invoke should be pretty-printed on TTY
        let invoke_json = r#"{"width":1920,"height":1080,"format":"jpeg","size":245760}"#;
        print_output(invoke_json, true);
        print_output(invoke_json, false);
    }
}
