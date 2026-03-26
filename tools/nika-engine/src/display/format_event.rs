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

pub fn fmt_provider_responded(
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

pub fn fmt_provider_sparkline(output_tokens: u64, input_tokens: u64, cost_usd: f64) -> String {
    let max_tok = input_tokens.max(output_tokens);
    sub(format!(
        "   tok {} cost {}",
        colors::sparkline(output_tokens, max_tok),
        colors::cost(cost_usd)
    ))
}

// ── Context ─────────────────────────────────────────────────────────────

pub fn fmt_context_assembled(
    sources_len: usize,
    total_tokens: u64,
    budget_used_pct: f64,
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
        colors::budget_bar(budget_used_pct as f32, 25),
        warn
    ))
}

// ── MCP ─────────────────────────────────────────────────────────────────

pub fn fmt_mcp_connected(server_name: &str) -> String {
    sub(format!(
        "{} connected {}",
        icons::mcp(),
        server_name.green()
    ))
}

pub fn fmt_mcp_error(server_name: &str, error: &str) -> String {
    sub(format!(
        "{} {} {}",
        icons::mcp(),
        format!("{} ✗", server_name).red(),
        error.red()
    ))
}

pub fn fmt_mcp_invoke(mcp_server: &str, tool: Option<&str>, resource: Option<&str>, call_id: &str) -> String {
    let target = tool.or(resource).unwrap_or("?");
    sub(format!(
        "{} {} → {} {}",
        icons::mcp(),
        mcp_server.dimmed(),
        target.white(),
        format!("call:{}", call_id).dimmed()
    ))
}

pub fn fmt_mcp_response(
    call_id: &str,
    output_len: u64,
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
        format_bytes(output_len),
        format!(" · {}ms", duration_ms).dimmed(),
        suffix
    ))
}

pub fn fmt_mcp_retry(operation: &str, attempt: u32, max_attempts: u32, error: &str) -> String {
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

pub fn fmt_agent_start(max_turns: u32, mcp_servers: &[String]) -> String {
    let servers = mcp_servers.join(", ");
    sub(format!(
        "{} {} max_turns:{} · mcp:[{}]",
        icons::agent_meta(),
        "agent".dimmed(),
        max_turns,
        servers.green()
    ))
}

pub fn fmt_agent_turn(turn_index: u32, kind: &str) -> String {
    sub(format!(
        "{} turn {}/…  {}",
        icons::agent_meta(),
        (turn_index + 1).to_string().white(),
        kind.dimmed()
    ))
}

pub fn fmt_agent_turn_tool_use() -> String {
    sub(format!("{} tool_use", "↳".dimmed()))
}

pub fn fmt_agent_complete(turns: u32, stop_reason: &str) -> String {
    sub(format!(
        "{} {} {} turns · {}",
        icons::agent_meta(),
        "done".green(),
        turns,
        stop_reason.dimmed()
    ))
}

pub fn fmt_agent_spawned(child_task_id: &str, depth: u32) -> String {
    sub(format!(
        "{} spawned {} depth:{}",
        "⤋".magenta(),
        child_task_id.white(),
        depth
    ))
}

// ── For-each ────────────────────────────────────────────────────────────

pub fn fmt_for_each_completed(task_id: &str, total: u32, succeeded: u32, failed: u32) -> String {
    let status = if failed > 0 {
        format!("{}/{} ok · {} failed", succeeded, total, failed)
            .yellow()
            .to_string()
    } else {
        format!("{}/{} ok", succeeded, total)
            .green()
            .to_string()
    };
    sub(format!(
        "{} for_each {} · {}",
        icons::log(),
        task_id.dimmed(),
        status
    ))
}

// ── Exec ────────────────────────────────────────────────────────────────

pub fn fmt_exec_completed(exit_code: i32, duration_ms: u64) -> String {
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

// ── Policy ──────────────────────────────────────────────────────────────

pub fn fmt_policy_blocked(ts: &str, policy_type: &str, reason: &str) -> String {
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

pub fn fmt_guardrail_passed(guardrail_type: &str, description: &str) -> String {
    sub(format!(
        "{} {} {}",
        icons::guardrail(),
        icons::success(),
        format!("{} · {}", guardrail_type, description).dimmed()
    ))
}

pub fn fmt_guardrail_failed(guardrail_type: &str, message: &str) -> String {
    sub(format!(
        "{} {} {}",
        icons::guardrail(),
        icons::failed(),
        format!("{} · {}", guardrail_type, message).red()
    ))
}

pub fn fmt_guardrail_escalation(severity: &str, message: &str) -> String {
    sub(format!(
        "  {} {} · {}",
        icons::retry(),
        format!("escalation · {}", severity).yellow(),
        message.dimmed()
    ))
}

// ── Log / Custom ────────────────────────────────────────────────────────

pub fn fmt_log(ts: &str, level: &str, message: &str) -> String {
    let level_colored = match level {
        "error" => level.red(),
        "warn" => level.yellow(),
        "info" => level.green(),
        "debug" => level.dimmed(),
        "trace" => level.dimmed(),
        _ => level.normal(),
    };
    format!(
        "{}  {} {} · {}",
        ts,
        icons::log(),
        level_colored,
        message
    )
}

pub fn fmt_custom(ts: &str, name: &str, payload: &Value) -> String {
    let preview = serde_json::to_string(payload).unwrap_or_default();
    let short = if preview.len() > 60 {
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

pub fn fmt_artifact_written(path: &str, size: u64, format: &str) -> String {
    sub(format!(
        "{} {} {}",
        icons::artifact(),
        format!("→ {}", path).cyan(),
        format!("{} · {}", format_bytes(size), format).dimmed()
    ))
}

pub fn fmt_artifact_failed(path: &str, reason: &str) -> String {
    sub(format!(
        "{} {} {}",
        icons::artifact(),
        format!("✗ {}", path).red(),
        reason.dimmed()
    ))
}

// ── Media ───────────────────────────────────────────────────────────────

pub fn fmt_media_extracted(block_count: u32, content_types: &[String]) -> String {
    sub(format!(
        "{} {} blocks · types: [{}]",
        icons::media(),
        block_count,
        content_types.join(", ").magenta()
    ))
}

pub fn fmt_media_stored(size_bytes: u64, path: &str, hash: &str) -> String {
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

pub fn fmt_media_stored_detail(deduplicated: bool, verified: bool, pipeline_ms: u64) -> String {
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

pub fn fmt_media_store_failed(reason: &str) -> String {
    sub(format!("{} {} {}", icons::media(), "✗".red(), reason.red()))
}

// ── Structured output ───────────────────────────────────────────────────

pub fn fmt_structured_output_attempt(
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
    let err_msg = error
        .map(|e| format!(" {}", e.dimmed()))
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

pub fn fmt_vision_content_resolved(image_count: u32, total_bytes: u64, resolve_ms: u64) -> String {
    sub(format!(
        "{} {} images · {} · resolved {}ms",
        icons::vision(),
        image_count,
        format_bytes(total_bytes),
        resolve_ms
    ))
}

// ── HTTP ────────────────────────────────────────────────────────────────

pub fn fmt_http_request(method: &str, url: &str) -> String {
    sub(format!(
        "{} → {} {}",
        icons::http(),
        method.cyan(),
        url.underline()
    ))
}

pub fn fmt_http_response(
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

// ── Template ────────────────────────────────────────────────────────────

pub fn fmt_template_resolved(template: &str, result: &str) -> String {
    sub(format!(
        "{} {} → {}",
        "tmpl".dimmed(),
        template.dimmed(),
        result.dimmed()
    ))
}
