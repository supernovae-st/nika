// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Verb analysis: analyze raw task actions into analyzed representations.

use super::*;

/// Analyze task action.
pub(super) fn analyze_action(
    raw: &RawTaskAction,
    ctx: &mut AnalyzerContext,
) -> AnalyzedTaskAction {
    match raw {
        RawTaskAction::Infer(s) => AnalyzedTaskAction::Infer(analyze_infer(&s.value)),
        RawTaskAction::Exec(s) => AnalyzedTaskAction::Exec(analyze_shell_cmd(&s.value)),
        RawTaskAction::Fetch(s) => AnalyzedTaskAction::Fetch(analyze_fetch(&s.value, ctx)),
        RawTaskAction::Invoke(s) => AnalyzedTaskAction::Invoke(analyze_invoke(&s.value)),
        RawTaskAction::Agent(s) => {
            AnalyzedTaskAction::Agent(Box::new(analyze_agent(&s.value, ctx)))
        }
    }
}

pub(super) fn analyze_infer(raw: &RawInferAction) -> AnalyzedInferAction {
    use crate::ast::content::analyze_content_part;

    AnalyzedInferAction {
        prompt: raw.prompt.value.clone(),
        system: raw.system.as_ref().map(|s| s.value.clone()),
        temperature: raw.temperature.as_ref().map(|s| s.value.clone()),
        max_tokens: raw.max_tokens.as_ref().map(|s| s.value.clone()),
        extended_thinking: raw.extended_thinking.as_ref().map(|s| s.value.clone()),
        thinking_budget: raw.thinking_budget.as_ref().map(|s| s.value.clone()),
        content: raw
            .content
            .as_ref()
            .map(|spanned| spanned.value.iter().map(analyze_content_part).collect()),
        response_format: raw.response_format.as_ref().map(|s| s.value.clone()),
        guardrails: raw.guardrails.clone(),
        span: raw.prompt.span,
    }
}

pub(super) fn analyze_shell_cmd(raw: &RawExecAction) -> AnalyzedExecAction {
    AnalyzedExecAction {
        command: raw.command.value.clone(),
        shell: raw
            .shell
            .as_ref()
            .map(|s| s.value.clone())
            .unwrap_or(Templatable::Value(false)),
        cwd: raw.cwd.as_ref().map(|s| s.value.clone()),
        env: raw
            .env
            .as_ref()
            .map(|s| {
                s.value
                    .iter()
                    .map(|(k, v)| (k.value.clone(), v.value.clone()))
                    .collect()
            })
            .unwrap_or_default(),
        timeout_ms: raw.timeout_ms.as_ref().map(|s| s.value.clone()),
        max_stdout: raw.max_stdout.as_ref().map(|s| s.value.clone()),
        span: raw.command.span,
    }
}

pub(super) fn analyze_fetch(
    raw: &RawFetchAction,
    ctx: &mut AnalyzerContext,
) -> AnalyzedFetchAction {
    let method = match raw.method.as_ref() {
        Some(s) if !s.value.is_empty() => match HttpMethod::parse(&s.value) {
            Some(m) => m,
            None => {
                ctx.add_warning(AnalyzeError::new(
                    AnalyzeErrorKind::InvalidValue,
                    s.span,
                    format!(
                        "unknown HTTP method '{}', defaulting to GET. \
                         Valid methods: GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS",
                        s.value
                    ),
                ));
                HttpMethod::Get
            }
        },
        _ => HttpMethod::Get,
    };

    AnalyzedFetchAction {
        url: raw.url.value.clone(),
        method,
        headers: raw
            .headers
            .as_ref()
            .map(|s| {
                s.value
                    .iter()
                    .map(|(k, v)| (k.value.clone(), v.value.clone()))
                    .collect()
            })
            .unwrap_or_default(),
        body: raw.body.as_ref().map(|s| s.value.clone()),
        json: raw.json.as_ref().map(|s| s.value.clone()),
        timeout_ms: raw.timeout_ms.as_ref().map(|s| s.value.clone()),
        follow_redirects: raw
            .follow_redirects
            .as_ref()
            .map(|s| s.value.clone())
            .unwrap_or(Templatable::Value(true)),
        response: raw.response.as_ref().and_then(
            |s| match crate::ast::extract::ResponseMode::parse(&s.value) {
                Some(mode) => Some(mode),
                None => {
                    ctx.add_warning(AnalyzeError::new(
                        AnalyzeErrorKind::InvalidValue,
                        s.span,
                        format!(
                            "unknown response mode '{}', expected one of: {}",
                            s.value,
                            crate::ast::extract::ResponseMode::ALL_NAMES.join(", ")
                        ),
                    ));
                    None
                }
            },
        ),
        extract: raw.extract.as_ref().and_then(|s| {
            match crate::ast::extract::ExtractMode::parse(&s.value) {
                Some(mode) => Some(mode),
                None => {
                    ctx.add_warning(AnalyzeError::new(
                        AnalyzeErrorKind::InvalidValue,
                        s.span,
                        format!(
                            "unknown extract mode '{}', expected one of: {}",
                            s.value,
                            crate::ast::extract::ExtractMode::ALL_NAMES.join(", ")
                        ),
                    ));
                    None
                }
            }
        }),
        selector: raw.selector.as_ref().map(|s| s.value.clone()),
        session: raw
            .session
            .as_ref()
            .map(|s| s.value.clone())
            .unwrap_or(Templatable::Value(false)),
        cache: raw
            .cache
            .as_ref()
            .map(|s| s.value.clone())
            .unwrap_or(Templatable::Value(false)),
        span: raw.url.span,
    }
}

pub(super) fn analyze_invoke(raw: &RawInvokeAction) -> AnalyzedInvokeAction {
    let parsed = raw.parse_tool_name();
    let (server, tool) = parsed.unwrap_or((None, ""));

    let span = raw
        .tool
        .as_ref()
        .map(|t| t.span)
        .or_else(|| raw.resource.as_ref().map(|r| r.span))
        .unwrap_or(Span::dummy());

    AnalyzedInvokeAction {
        server: server
            .map(|s| s.to_string())
            .or_else(|| raw.mcp.as_ref().map(|s| s.value.clone())),
        tool: tool.to_string(),
        resource: raw.resource.as_ref().map(|s| s.value.clone()),
        params: raw.params.as_ref().map(|s| s.value.clone()),
        timeout_ms: raw.timeout_ms.as_ref().map(|s| s.value.clone()),
        span,
    }
}

pub(super) fn analyze_agent(
    raw: &RawAgentAction,
    ctx: &mut AnalyzerContext,
) -> AnalyzedAgentAction {
    use crate::ast::guardrails::GuardrailConfig;

    // Warn if LLM guardrails are used (not yet implemented at runtime)
    if raw
        .guardrails
        .iter()
        .any(|g| matches!(g, GuardrailConfig::Llm(_)))
    {
        ctx.add_warning(AnalyzeError::new(
            AnalyzeErrorKind::UnsupportedFeature,
            raw.prompt.span,
            "LLM guardrails (type: llm) are parsed but not yet executed at runtime \
             — they will be silently skipped. Use type: regex or type: schema instead."
                .to_string(),
        ));
    }

    // Warn if extended_thinking + tools conflict
    let has_tools = raw
        .tools
        .as_ref()
        .map(|s| !s.value.is_empty())
        .unwrap_or(false);
    let has_thinking = raw
        .extended_thinking
        .as_ref()
        .and_then(|s| s.value.as_value().copied())
        .unwrap_or(false);
    if has_thinking && has_tools {
        ctx.add_warning(AnalyzeError::new(
            AnalyzeErrorKind::UnsupportedFeature,
            raw.prompt.span,
            "extended_thinking: true disables tool calling — tools will be ignored. \
             Extended thinking is single-turn, text-only mode."
                .to_string(),
        ));
    }

    AnalyzedAgentAction {
        prompt: raw.prompt.value.clone(),
        tools: raw
            .tools
            .as_ref()
            .map(|s| s.value.iter().map(|v| v.value.clone()).collect())
            .unwrap_or_default(),
        max_turns: raw.max_turns.as_ref().map(|s| s.value.clone()),
        max_tokens: raw.max_tokens.as_ref().map(|s| s.value.clone()),
        from: raw.from.as_ref().map(|s| s.value.clone()),
        skills: raw
            .skills
            .as_ref()
            .map(|s| s.value.iter().map(|v| v.value.clone()).collect())
            .unwrap_or_default(),
        mcp: raw
            .mcp
            .as_ref()
            .map(|s| s.value.iter().map(|v| v.value.clone()).collect())
            .unwrap_or_default(),
        system: raw.system.as_ref().map(|s| s.value.clone()),
        provider: raw
            .provider
            .as_ref()
            .map(|s| crate::ProviderName::parse(&s.value)),
        model: raw.model.as_ref().map(|s| s.value.clone()),
        temperature: raw.temperature.as_ref().map(|s| s.value.clone()),
        token_budget: raw.token_budget.as_ref().map(|s| s.value.clone()),
        extended_thinking: raw.extended_thinking.as_ref().map(|s| s.value.clone()),
        thinking_budget: raw.thinking_budget.as_ref().map(|s| s.value.clone()),
        depth_limit: raw.depth_limit.as_ref().map(|s| s.value.clone()),
        tool_choice: raw.tool_choice.as_ref().map(|s| s.value.clone()),
        stop_sequences: raw
            .stop_sequences
            .as_ref()
            .map(|s| s.value.iter().map(|v| v.value.clone()).collect())
            .unwrap_or_default(),
        scope: raw.scope.as_ref().map(|s| s.value.clone()),
        guardrails: raw.guardrails.clone(),
        completion: raw.completion.clone(),
        limits: raw.limits.clone(),
        span: raw.prompt.span,
    }
}

pub(super) fn analyze_output(
    raw: &crate::ast::raw::RawOutputConfig,
    ctx: &mut AnalyzerContext,
) -> AnalyzedOutput {
    let format = raw
        .format
        .as_ref()
        .and_then(|s| OutputFormat::parse(&s.value))
        .unwrap_or(OutputFormat::Text);

    // Warn if schema is present without format: json
    if (raw.schema.is_some() || raw.schema_ref.is_some()) && format != OutputFormat::Json {
        let span = raw
            .schema
            .as_ref()
            .map(|s| s.span)
            .or_else(|| raw.schema_ref.as_ref().map(|s| s.span))
            .unwrap_or(Span::dummy());
        ctx.add_warning(AnalyzeError::new(
            AnalyzeErrorKind::InvalidValue,
            span,
            "schema is present without format: json — structured output validation will not run"
                .to_string(),
        ));
    }

    AnalyzedOutput {
        format,
        schema: raw.schema.as_ref().map(|s| s.value.clone()),
        schema_ref: raw.schema_ref.as_ref().map(|s| s.value.clone()),
        max_retries: raw.max_retries.as_ref().map(|s| s.value.clone()),
        span: raw.format.as_ref().map(|s| s.span).unwrap_or(Span::dummy()),
    }
}

/// Analyze MCP server configuration.
pub(super) fn analyze_mcp_server(
    name: &str,
    raw: &crate::ast::raw::RawMcpServer,
    span: Span,
    ctx: &mut AnalyzerContext,
) -> AnalyzedMcpServer {
    let has_command = raw
        .command
        .as_ref()
        .map(|s| !s.value.trim().is_empty())
        .unwrap_or(false);
    let has_from = raw.from.is_some();

    // Validate from: vs command: rules
    let from_source = if has_from && has_command {
        // NIKA-110: both from: and command: present
        ctx.add_error(AnalyzeError::new(
            AnalyzeErrorKind::InvalidValue,
            span,
            format!(
                "MCP server '{}' has both 'from:' and 'command:' — use one or the other",
                name
            ),
        ));
        None
    } else if has_from {
        // Parse from: value
        let from_val = raw.from.as_ref().unwrap().value.as_str();
        match from_val {
            "config" => Some(McpFromSource::Config),
            "project" => Some(McpFromSource::Project),
            "global" => Some(McpFromSource::Global),
            other => {
                // NIKA-109: unknown from: source
                ctx.add_error(
                    AnalyzeError::new(
                        AnalyzeErrorKind::InvalidValue,
                        raw.from.as_ref().unwrap().span,
                        format!(
                            "Unknown MCP source '{}' in from: field of server '{}'",
                            other, name
                        ),
                    )
                    .with_suggestion("valid sources: config, project, global"),
                );
                None
            }
        }
    } else {
        None // inline server (no from:)
    };

    let transport = if raw.is_sse() {
        McpTransport::Sse
    } else {
        McpTransport::Stdio
    };

    // SSE servers are accepted by the analyzer but dropped during lowering
    if transport == McpTransport::Sse {
        ctx.add_warning(
            AnalyzeError::new(
                AnalyzeErrorKind::UnsupportedFeature,
                span,
                format!(
                    "SSE MCP server '{}' will be dropped during execution (no runtime support)",
                    name
                ),
            )
            .with_suggestion("use a stdio-based MCP server instead"),
        );
    }

    // NIKA-111: Stdio servers WITHOUT from: require a non-empty command field
    if transport == McpTransport::Stdio && !has_from && !has_command {
        let error_span = raw.command.as_ref().map(|s| s.span).unwrap_or(span);
        ctx.add_error(
            AnalyzeError::new(
                AnalyzeErrorKind::MissingField,
                error_span,
                format!("MCP server '{}' missing 'command:' or 'from:' field", name),
            )
            .with_suggestion("add command: for inline or from: config to resolve from .mcp.json"),
        );
    }

    AnalyzedMcpServer {
        name: name.to_string(),
        from: from_source,
        command: raw.command.as_ref().map(|s| s.value.clone()),
        args: raw
            .args
            .as_ref()
            .map(|s| s.value.iter().map(|v| v.value.clone()).collect())
            .unwrap_or_default(),
        env: raw
            .env
            .as_ref()
            .map(|s| {
                s.value
                    .iter()
                    .map(|(k, v)| (k.value.clone(), v.value.clone()))
                    .collect()
            })
            .unwrap_or_default(),
        cwd: raw.cwd.as_ref().map(|s| s.value.clone()),
        url: raw.url.as_ref().map(|s| s.value.clone()),
        transport,
        span,
    }
}

/// Analyze for_each iteration configuration.
pub(super) fn analyze_for_each(
    raw: &crate::ast::raw::RawForEach,
    span: Span,
) -> AnalyzedForEach {
    AnalyzedForEach {
        items: raw.items.value.clone(),
        as_var: raw
            .as_var
            .as_ref()
            .map(|s| s.value.clone())
            .unwrap_or_else(|| "item".to_string()),
        concurrency: raw.concurrency.as_ref().map(|s| s.value.clone()),
        fail_fast: raw
            .fail_fast
            .as_ref()
            .map(|s| s.value.clone())
            .unwrap_or(Templatable::Value(true)),
        span,
    }
}

/// Analyze retry configuration.
pub(super) fn analyze_retry(
    raw: &crate::ast::raw::RawRetryConfig,
    span: Span,
) -> AnalyzedRetry {
    AnalyzedRetry {
        max_attempts: raw
            .max_attempts
            .as_ref()
            .map(|s| s.value.clone())
            .unwrap_or(Templatable::Value(3)),
        delay_ms: raw
            .delay_ms
            .as_ref()
            .map(|s| s.value.clone())
            .unwrap_or(Templatable::Value(1000)),
        backoff: raw.backoff.as_ref().map(|s| s.value.clone()),
        span,
    }
}
