// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shared event formatting — free functions returning formatted strings.
//!
//! Both `CliRenderer` (println) and `LiveRenderer` (multi.println) call
//! these functions, then emit via their own mechanism.

use colored::Colorize;
use serde_json::Value;

use super::renderer::format_bytes;
use super::{colors, icons};

// ── Helpers ─────────────────────────────────────────────────────────────

/// Standard sub-event indent: 6 spaces + dimmed │.
const INDENT: &str = "      ";

fn sub(content: String) -> String {
    format!("{}     {} {}", INDENT, "│".dimmed(), content)
}

// ── Provider ────────────────────────────────────────────────────────────

pub(crate) fn fmt_provider_responded(
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    ttft_ms: Option<u64>,
) -> String {
    let ttft_str = ttft_ms
        .map(|t| format!(" · ttft:{}", colors::ttft(t)))
        .unwrap_or_default();
    sub(format!(
        "{} {} in:{} out:{} cache:{}{}",
        icons::provider(),
        "←".dimmed(),
        colors::tokens(input_tokens).as_str().dimmed(),
        colors::tokens(output_tokens).as_str().white(),
        colors::tokens(cache_read_tokens).as_str().dimmed(),
        ttft_str
    ))
}

pub(crate) fn fmt_provider_sparkline(
    output_tokens: u64,
    input_tokens: u64,
    cost_usd: f64,
) -> String {
    let max_tok = input_tokens.max(output_tokens);
    sub(format!(
        "   tok {} cost {}",
        colors::sparkline(output_tokens, max_tok),
        colors::cost(cost_usd)
    ))
}

// ── Context ─────────────────────────────────────────────────────────────

pub(crate) fn fmt_context_assembled(
    sources_len: usize,
    total_tokens: u64,
    budget_used_pct: f32,
) -> String {
    let warn = if budget_used_pct > 90.0 {
        " ⚠".red().to_string()
    } else {
        String::new()
    };
    sub(format!(
        "{} {} src · {} tok · {}{}",
        "ctx".dimmed(),
        sources_len,
        colors::tokens(total_tokens),
        colors::budget_bar(budget_used_pct, 25),
        warn
    ))
}

// ── MCP ─────────────────────────────────────────────────────────────────

pub(crate) fn fmt_mcp_connected(server_name: &str) -> String {
    sub(format!(
        "{} connected {}",
        icons::mcp(),
        server_name.green()
    ))
}

pub(crate) fn fmt_mcp_error(server_name: &str, error: &str) -> String {
    sub(format!(
        "{} {} {}",
        icons::mcp(),
        format!("{} ✗", server_name).red(),
        error.red()
    ))
}

pub(crate) fn fmt_mcp_invoke(
    mcp_server: &str,
    tool: Option<&str>,
    resource: Option<&str>,
    call_id: &str,
) -> String {
    let target = tool.or(resource).unwrap_or("?");
    sub(format!(
        "{} {} → {} {}",
        icons::mcp(),
        mcp_server.dimmed(),
        target.white(),
        format!("call:{}", call_id).dimmed()
    ))
}

pub(crate) fn fmt_mcp_response(
    call_id: &str,
    output_len: usize,
    duration_ms: u64,
    cached: bool,
    is_error: bool,
) -> String {
    let cache_tag = if cached {
        " cached".green().to_string()
    } else {
        String::new()
    };
    let err_tag = if is_error {
        " ✗".red().to_string()
    } else {
        String::new()
    };
    let suffix = format!("{}{}", cache_tag, err_tag);
    sub(format!(
        "{} {} {} · {}{}{}",
        icons::mcp(),
        format!("call:{}", call_id).dimmed(),
        "←".dimmed(),
        format_bytes(output_len as u64),
        format!(" · {}ms", duration_ms).dimmed(),
        suffix
    ))
}

pub(crate) fn fmt_mcp_retry(
    operation: &str,
    attempt: u32,
    max_attempts: u32,
    error: &str,
) -> String {
    sub(format!(
        "{} {} {}/{} · {}",
        icons::retry(),
        format!("retry {}", operation).yellow(),
        attempt.to_string().yellow(),
        max_attempts,
        error.dimmed()
    ))
}

// ── Agent ───────────────────────────────────────────────────────────────

pub(crate) fn fmt_agent_start(max_turns: u32, mcp_servers: &[String]) -> String {
    let servers = mcp_servers.join(", ");
    sub(format!(
        "{} {} max_turns:{} · mcp:[{}]",
        icons::agent_meta(),
        "agent".dimmed(),
        max_turns,
        servers.green()
    ))
}

pub(crate) fn fmt_agent_turn(turn_index: u32, kind: &nika_event::AgentTurnKind) -> String {
    sub(format!(
        "{} turn {}/…  {}",
        icons::agent_meta(),
        (turn_index + 1).to_string().white(),
        kind.as_str().dimmed()
    ))
}

pub(crate) fn fmt_agent_turn_tool_use() -> String {
    sub(format!("{} tool_use", "↳".dimmed()))
}

pub(crate) fn fmt_agent_complete(turns: u32, stop_reason: &nika_event::AgentStopReason) -> String {
    sub(format!(
        "{} {} {} turns · {}",
        icons::agent_meta(),
        "done".green(),
        turns,
        stop_reason.as_str().dimmed()
    ))
}

pub(crate) fn fmt_agent_spawned(child_task_id: &str, depth: u32) -> String {
    sub(format!(
        "{} spawned {} depth:{}",
        "⤋".magenta(),
        child_task_id.white(),
        depth
    ))
}

// ── For-each ────────────────────────────────────────────────────────────

pub(crate) fn fmt_for_each_completed(
    task_id: &str,
    total: u32,
    succeeded: u32,
    failed: u32,
) -> String {
    let status = if failed > 0 {
        format!("{}/{} ok · {} failed", succeeded, total, failed)
            .yellow()
            .to_string()
    } else {
        format!("{}/{} ok", succeeded, total).green().to_string()
    };
    sub(format!(
        "{} for_each {} · {}",
        icons::log(),
        task_id.dimmed(),
        status
    ))
}

// ── Exec ────────────────────────────────────────────────────────────────

pub(crate) fn fmt_exec_completed(exit_code: i32, duration_ms: u64) -> String {
    let code_str = if exit_code == 0 {
        "0".green().to_string()
    } else {
        exit_code.to_string().red().to_string()
    };
    sub(format!(
        "{} exit:{} · {}ms",
        icons::verb("exec"),
        code_str,
        duration_ms
    ))
}

// ── Routing ─────────────────────────────────────────────────────────────

pub(crate) fn fmt_fallback_triggered(
    from_provider: &str,
    to_provider: &str,
    reason: &str,
    attempt: u32,
) -> String {
    sub(format!(
        "{} \u{21AF} fallback #{} {} \u{2192} {} ({})",
        icons::provider(),
        attempt,
        from_provider.dimmed(),
        to_provider.cyan(),
        reason.dimmed(),
    ))
}

// ── Policy ──────────────────────────────────────────────────────────────

pub(crate) fn fmt_policy_blocked(ts: &str, policy_type: &str, reason: &str) -> String {
    format!(
        "{}  {} {} {} · {}",
        ts,
        "⊘".red().bold(),
        "BLOCKED".red().bold(),
        policy_type.yellow(),
        reason.red()
    )
}

// ── Guardrails ──────────────────────────────────────────────────────────

pub(crate) fn fmt_guardrail_passed(
    guardrail_type: &nika_event::GuardrailType,
    description: &str,
) -> String {
    sub(format!(
        "{} {} {}",
        icons::guardrail(),
        icons::success(),
        format!("{} · {}", guardrail_type, description).dimmed()
    ))
}

pub(crate) fn fmt_guardrail_failed(
    guardrail_type: &nika_event::GuardrailType,
    message: &str,
) -> String {
    sub(format!(
        "{} {} {}",
        icons::guardrail(),
        icons::failed(),
        format!("{} · {}", guardrail_type, message).red()
    ))
}

pub(crate) fn fmt_guardrail_escalation(
    guardrail_type: &nika_event::GuardrailType,
    severity: &nika_event::Severity,
    message: &str,
) -> String {
    sub(format!(
        "  {} {} · {}",
        icons::retry(),
        format!("{} escalation · {}", guardrail_type, severity).yellow(),
        message.dimmed()
    ))
}

// ── Log / Custom ────────────────────────────────────────────────────────

pub(crate) fn fmt_log(ts: &str, level: &str, message: &str) -> String {
    let level_colored = match level {
        "error" => level.red(),
        "warn" => level.yellow(),
        "info" => level.green(),
        "debug" => level.dimmed(),
        "trace" => level.dimmed(),
        _ => level.normal(),
    };
    format!("{}  {} {} · {}", ts, icons::log(), level_colored, message)
}

pub(crate) fn fmt_custom(ts: &str, name: &str, payload: &Value) -> String {
    let preview = serde_json::to_string(payload).unwrap_or_default();
    let short = if colors::stripped_len(&preview) > 60 {
        format!("{}…", &preview[..colors::floor_char_boundary(&preview, 60)])
    } else {
        preview
    };
    format!(
        "{}  {} {} · {}",
        ts,
        icons::log(),
        name.cyan(),
        short.dimmed()
    )
}

// ── Artifacts ───────────────────────────────────────────────────────────

pub(crate) fn fmt_artifact_written(path: &str, size: u64, format: &str) -> String {
    sub(format!(
        "{} {} {}",
        icons::artifact(),
        format!("→ {}", path).cyan(),
        format!("{} · {}", format_bytes(size), format).dimmed()
    ))
}

pub(crate) fn fmt_artifact_failed(path: &str, reason: &str) -> String {
    sub(format!(
        "{} {} {}",
        icons::artifact(),
        format!("✗ {}", path).red(),
        reason.dimmed()
    ))
}

// ── Media ───────────────────────────────────────────────────────────────

pub(crate) fn fmt_media_extracted(block_count: u32, content_types: &[String]) -> String {
    sub(format!(
        "{} {} blocks · types: [{}]",
        icons::media(),
        block_count,
        content_types.join(", ").magenta()
    ))
}

pub(crate) fn fmt_media_stored(size_bytes: u64, path: &str, hash: &str) -> String {
    let short_hash = if hash.len() > 16 {
        &hash[..super::colors::floor_char_boundary(hash, 16)]
    } else {
        hash
    };
    sub(format!(
        "{} {} · {} · {}…",
        icons::media(),
        format_bytes(size_bytes),
        path.dimmed(),
        short_hash.dimmed()
    ))
}

pub(crate) fn fmt_media_stored_detail(
    deduplicated: bool,
    verified: bool,
    pipeline_ms: u64,
) -> String {
    let dedup = if deduplicated {
        "yes".yellow()
    } else {
        "no".dimmed()
    };
    let verif = if verified { "yes".green() } else { "no".red() };
    sub(format!(
        "  dedup:{} · verified:{} · pipeline:{}ms",
        dedup, verif, pipeline_ms
    ))
}

pub(crate) fn fmt_media_store_failed(reason: &str) -> String {
    sub(format!("{} {} {}", icons::media(), "✗".red(), reason.red()))
}

// ── Structured output ───────────────────────────────────────────────────

pub(crate) fn fmt_structured_output_attempt(
    layer: u8,
    layer_name: &str,
    success: bool,
    error: Option<&str>,
) -> String {
    let status = if success {
        icons::success()
    } else {
        icons::failed()
    };
    // Truncate long error messages (e.g. validation lists) to keep CLI readable
    let err_msg = error
        .map(|e| {
            let truncated = if e.len() > 200 {
                format!("{}...", &e[..colors::floor_char_boundary(e, 197)])
            } else {
                e.to_string()
            };
            format!(" ({})", truncated.dimmed())
        })
        .unwrap_or_default();
    sub(format!(
        "{} L{}: {} {}{}",
        icons::structured(),
        layer,
        layer_name,
        status,
        err_msg
    ))
}

// ── Vision ──────────────────────────────────────────────────────────────

pub(crate) fn fmt_vision_content_resolved(
    image_count: u32,
    total_bytes: u64,
    resolve_ms: u64,
) -> String {
    sub(format!(
        "{} {} images · {} · resolved {}ms",
        icons::vision(),
        image_count,
        format_bytes(total_bytes),
        resolve_ms
    ))
}

// ── HTTP ────────────────────────────────────────────────────────────────

pub(crate) fn fmt_http_request(method: &str, url: &str) -> String {
    sub(format!(
        "{} → {} {}",
        icons::http(),
        method.cyan(),
        url.underline()
    ))
}

pub(crate) fn fmt_http_response(
    status_code: u16,
    content_type: Option<&str>,
    content_length: Option<u64>,
    elapsed_ms: u64,
) -> String {
    let status_colored = if status_code < 300 {
        status_code.to_string().green()
    } else if status_code < 400 {
        status_code.to_string().yellow()
    } else {
        status_code.to_string().red()
    };
    let ct = content_type.unwrap_or("?");
    let cl = content_length.map(format_bytes).unwrap_or_default();
    sub(format!(
        "{} ← {} · {} · {} · {}ms",
        icons::http(),
        status_colored,
        ct.dimmed(),
        cl,
        elapsed_ms
    ))
}

// ── Provider Called ─────────────────────────────────────────────────────

pub(crate) fn fmt_provider_called(provider: &str, model: &str, prompt_len: usize) -> String {
    sub(format!(
        "{} {}/{} {} {} chars",
        icons::provider(),
        provider.dimmed(),
        model.white(),
        "· prompt:".dimmed(),
        prompt_len
    ))
}

// ── Fetch retry ────────────────────────────────────────────────────────

pub(crate) fn fmt_fetch_retry(
    url: &str,
    attempt: u32,
    max_attempts: u32,
    status_code: Option<u16>,
    backoff_ms: u64,
) -> String {
    let status = status_code
        .map(|c| format!(" http:{}", c.to_string().yellow()))
        .unwrap_or_default();
    sub(format!(
        "{} {} {}/{} · {}{}  backoff:{}ms",
        icons::retry(),
        "fetch retry".yellow(),
        attempt.to_string().yellow(),
        max_attempts,
        url.underline(),
        status,
        backoff_ms
    ))
}

// ── Boot ───────────────────────────────────────────────────────────────

pub(crate) fn fmt_boot_phase(
    phase: &str,
    success: bool,
    duration_ms: u64,
    warnings: &[String],
) -> String {
    let status = if success {
        icons::success()
    } else {
        icons::failed()
    };
    let warn = if warnings.is_empty() {
        String::new()
    } else {
        format!(" · {} warnings", warnings.len())
            .yellow()
            .to_string()
    };
    sub(format!(
        "{} boot {} {} · {}ms{}",
        icons::log(),
        phase.cyan(),
        status,
        duration_ms,
        warn
    ))
}

// ── Native model ───────────────────────────────────────────────────────

pub(crate) fn fmt_native_model_loaded(
    model: &str,
    kind: &str,
    duration_ms: u64,
    is_vision: bool,
) -> String {
    let vision_tag = if is_vision {
        " +vision".magenta().to_string()
    } else {
        String::new()
    };
    sub(format!(
        "{} native {} · {} · {}ms{}",
        icons::provider(),
        model.white(),
        kind.dimmed(),
        duration_ms,
        vision_tag
    ))
}

// ── Binding ────────────────────────────────────────────────────────────

pub(crate) fn fmt_binding_default(alias: &str, path: &str) -> String {
    sub(format!(
        "{} {} ?? → default ({})",
        "bind".dimmed(),
        alias.cyan(),
        path.dimmed()
    ))
}

pub(crate) fn fmt_binding_transform(alias: &str, transform_chain: &str) -> String {
    sub(format!(
        "{} {} | {}",
        "bind".dimmed(),
        alias.cyan(),
        transform_chain.dimmed()
    ))
}

pub(crate) fn fmt_binding_env(var_name: &str, found: bool) -> String {
    let status = if found {
        icons::success()
    } else {
        icons::failed()
    };
    sub(format!(
        "{} $env.{} {}",
        "bind".dimmed(),
        var_name.cyan(),
        status
    ))
}

// ── Decompose ──────────────────────────────────────────────────────────

pub(crate) fn fmt_decompose_started(task_id: &str, strategy: &str) -> String {
    sub(format!(
        "{} decompose {} · strategy:{}",
        icons::log(),
        task_id.white(),
        strategy.cyan()
    ))
}

pub(crate) fn fmt_decompose_completed(
    task_id: &str,
    item_count: usize,
    duration_ms: u64,
) -> String {
    sub(format!(
        "{} decompose {} · {} items · {}ms",
        icons::success(),
        task_id.white(),
        item_count.to_string().green(),
        duration_ms
    ))
}

// ── For-each started ───────────────────────────────────────────────────

pub(crate) fn fmt_for_each_started(task_id: &str, item_count: usize, concurrency: usize) -> String {
    sub(format!(
        "{} for_each {} · {} items · concurrency:{}",
        icons::log(),
        task_id.dimmed(),
        item_count,
        concurrency
    ))
}

// ── Provider initialized ───────────────────────────────────────────────

pub(crate) fn fmt_provider_initialized(provider: &str, model: &str, cached: bool) -> String {
    let cache_tag = if cached {
        " cached".green().to_string()
    } else {
        String::new()
    };
    sub(format!(
        "{} {} · {}{}",
        icons::provider(),
        provider.cyan(),
        model.white(),
        cache_tag
    ))
}

// ── Builtin tool invoked ───────────────────────────────────────────────

pub(crate) fn fmt_builtin_tool_invoked(tool_name: &str, duration_ms: u64, success: bool) -> String {
    let status = if success {
        icons::success()
    } else {
        icons::failed()
    };
    sub(format!(
        "{} {} {} · {}ms",
        icons::log(),
        tool_name.cyan(),
        status,
        duration_ms
    ))
}

// ── Extract applied ────────────────────────────────────────────────────

pub(crate) fn fmt_extract_applied(mode: &str, input_len: usize, output_len: usize) -> String {
    let ratio = if input_len > 0 {
        format!(" ({}%)", output_len * 100 / input_len)
    } else {
        String::new()
    };
    sub(format!(
        "{} extract:{} · {} → {}{}",
        icons::http(),
        mode.cyan(),
        format_bytes(input_len as u64),
        format_bytes(output_len as u64),
        ratio.dimmed()
    ))
}

// ── Template ────────────────────────────────────────────────────────────

pub(crate) fn fmt_template_resolved(template: &str, result: &str) -> String {
    sub(format!(
        "{} {} → {}",
        "tmpl".dimmed(),
        template.dimmed(),
        result.dimmed()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_output_attempt_multibyte_error_no_panic() {
        // CJK characters are 3 bytes each — a 200+ char string of them would panic
        // with naive &e[..197] slicing on a char boundary
        let cjk_error = "日本語".repeat(100); // 300 CJK chars = 900 bytes
                                              // Must not panic
        let result = fmt_structured_output_attempt(1, "rig-extract", false, Some(&cjk_error));
        assert!(result.contains("..."));
    }

    #[test]
    fn structured_output_attempt_short_error_no_truncation() {
        let result = fmt_structured_output_attempt(2, "json-parse", false, Some("bad json"));
        assert!(result.contains("bad json"));
        assert!(!result.contains("..."));
    }

    #[test]
    fn structured_output_attempt_success_no_error() {
        let result = fmt_structured_output_attempt(0, "tool-inject", true, None);
        assert!(result.contains("tool-inject"));
    }
}
