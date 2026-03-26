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

/// Auto-detect provider from environment variables (priority order).
pub fn detect_provider() -> Option<String> {
    for (var, provider) in [
        ("ANTHROPIC_API_KEY", "anthropic"),
        ("OPENAI_API_KEY", "openai"),
        ("MISTRAL_API_KEY", "mistral"),
        ("GROQ_API_KEY", "groq"),
        ("DEEPSEEK_API_KEY", "deepseek"),
        ("GEMINI_API_KEY", "gemini"),
        ("XAI_API_KEY", "xai"),
    ] {
        if std::env::var(var).is_ok_and(|v| !v.trim().is_empty()) {
            return Some(provider.to_string());
        }
    }
    None
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
fn resolve_provider_model(
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

/// Create a one-shot TaskExecutor.
async fn one_shot_executor(
    provider: &str,
    model: Option<&str>,
) -> Result<(TaskExecutor, EventLog), NikaError> {
    let event_log = EventLog::new();
    let executor = TaskExecutor::new(provider, model, None, event_log.clone())?;
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

fn print_header(label: &str, is_tty: bool) {
    if is_tty {
        eprintln!("\n  {} {}", "┌─".dimmed(), label.cyan());
    }
}

fn print_footer(elapsed: std::time::Duration, extra: &str, is_tty: bool) {
    if is_tty {
        eprintln!(
            "  {} {}",
            "└─".dimmed(),
            format!("{}ms{}", elapsed.as_millis(), extra).dimmed()
        );
        eprintln!();
    }
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

    // Show header
    let display_model = model_name.as_deref().unwrap_or("(default)");
    if !quiet {
        print_header(&format!("{} via {}", display_model, provider_name), is_tty);
    }

    // Execute
    let (executor, event_log) = one_shot_executor(&provider_name, model_name.as_deref()).await?;
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();
    let start = Instant::now();
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

    // Print output
    println!("{output}");

    // Cost footer (TTY only)
    if !quiet {
        let events = event_log.events();
        let mut tokens = 0u64;
        let mut cost = 0.0f64;
        for event in events.iter().rev() {
            if let nika_engine::event::EventKind::ProviderResponded {
                input_tokens,
                output_tokens,
                cost_usd,
                ..
            } = &event.kind
            {
                tokens = input_tokens + output_tokens;
                cost = *cost_usd;
                break;
            }
        }
        let extra = if tokens > 0 {
            format!(" · {} tokens · ${:.4}", tokens, cost)
        } else {
            String::new()
        };
        print_footer(elapsed, &extra, is_tty);
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

    let fetch = FetchParams {
        url: url.clone(),
        method: method.unwrap_or_else(|| "GET".to_string()),
        headers: header_map,
        body,
        json: json_value,
        timeout,
        extract: extract.clone(),
        selector,
        response,
        retry: None,
        follow_redirects: None,
    };

    let action = TaskAction::Fetch { fetch };
    let task_id: Arc<str> = Arc::from("cli");

    let extract_label = extract.as_deref().unwrap_or("raw");
    if !quiet {
        print_header(&format!("{} · {}", url, extract_label), is_tty);
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

    println!("{output}");

    if !quiet {
        let extra = format!(" · {} bytes", output.len());
        print_footer(elapsed, &extra, is_tty);
    }

    Ok(())
}

/// Handle `nika invoke tool` — call builtin nika:* or MCP tool.
pub async fn handle_invoke(
    tool: String,
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
        print_header(&tool, is_tty);
    }

    let (executor, _) = one_shot_executor("mock", None).await?;
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();
    let start = Instant::now();
    let output = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await?;
    let elapsed = start.elapsed();

    println!("{output}");

    if !quiet {
        print_footer(elapsed, "", is_tty);
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
        provider: Some(provider_name.clone()),
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

    let display_model = model_name.as_deref().unwrap_or("(default)");
    if !quiet {
        print_header(
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
    let output = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await?;
    let elapsed = start.elapsed();

    println!("{output}");

    if !quiet {
        let events = event_log.events();
        let mut total_tokens = 0u64;
        let mut total_cost = 0.0f64;
        for event in &events {
            if let nika_engine::event::EventKind::ProviderResponded {
                input_tokens,
                output_tokens,
                cost_usd,
                ..
            } = &event.kind
            {
                total_tokens += input_tokens + output_tokens;
                total_cost += cost_usd;
            }
        }
        let extra = if total_tokens > 0 {
            format!(" · {} tokens · ${:.4}", total_tokens, total_cost)
        } else {
            String::new()
        };
        print_footer(elapsed, &extra, is_tty);
    }

    Ok(())
}
