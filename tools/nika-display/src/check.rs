//! CheckRenderer -- pre-flight validation checklist for `nika check`.
//!
//! Displays validation phases as a checklist with pass/fail status,
//! timing, and inline error details. Uses the Cosmic icon palette.

use colored::Colorize;

use crate::colors::stripped_len;
use crate::icons;

/// Result of a single validation phase.
pub struct PhaseResult {
    pub name: &'static str,
    pub passed: bool,
    pub detail: String,
    pub duration_ms: u64,
    /// If failed, optional error context lines
    pub errors: Vec<String>,
    /// If failed, optional hint box lines
    pub hints: Vec<String>,
}

/// Result of MCP server validation (--strict mode).
pub struct McpCheckResult {
    pub server_name: String,
    pub tool_count: usize,
    pub connect_ms: u64,
    pub validations: Vec<McpCallValidation>,
}

/// Validation result for a single MCP call.
pub struct McpCallValidation {
    pub task_id: String,
    pub tool_name: String,
    pub valid: bool,
    pub errors: Vec<McpParamError>,
}

/// A single parameter validation error in an MCP call.
pub struct McpParamError {
    pub path: String,
    pub message: String,
}

/// Terminal width capped at 72 for consistent layout.
fn term_width() -> usize {
    terminal_size::terminal_size()
        .map(|(tw, _)| tw.0 as usize)
        .unwrap_or(80)
        .min(72)
}

/// Print the check header with rounded corners.
///
/// ```text
/// +-----------------------------------------------------------+
/// |                                                           |
/// |  N I K A  C H E C K                             v0.40.2   |
/// |                                                           |
/// |  workflow.nika.yaml                                       |
/// |                                                           |
/// +-----------------------------------------------------------+
/// ```
pub fn print_check_header(file: &str, strict: bool, version: &str) {
    let w = term_width();
    let inner = w - 2;
    let border = "\u{2500}".repeat(inner);

    println!("\u{256D}{}\u{256E}", border.dimmed());
    println!("\u{2502}{}\u{2502}", " ".repeat(inner));

    let title = if strict {
        "N I K A  C H E C K  \u{2500} \u{2500}  S T R I C T"
    } else {
        "N I K A  C H E C K"
    };
    let ver = format!("v{}", version);
    let pad = inner.saturating_sub(title.len() + ver.len() + 4);
    println!(
        "\u{2502}  {}{}{}  \u{2502}",
        title.bold().white(),
        " ".repeat(pad),
        ver.dimmed()
    );
    println!("\u{2502}{}\u{2502}", " ".repeat(inner));

    // File name
    let file_pad = inner.saturating_sub(file.len() + 2);
    println!("\u{2502}  {}{}\u{2502}", file.bold(), " ".repeat(file_pad));

    println!("\u{2502}{}\u{2502}", " ".repeat(inner));
    println!("\u{2570}{}\u{256F}", border.dimmed());
    println!();
}

/// Print a single validation phase line.
///
/// ```text
///   +  schema          YAML valid against @0.12                      1ms
///   x  dag             CYCLE DETECTED                                0ms
/// ```
pub fn print_phase(result: &PhaseResult) {
    let icon = if result.passed {
        icons::success()
    } else {
        icons::failed()
    };

    let dur = format!("{}ms", result.duration_ms);

    // Build the content parts with plain strings first, then color
    let name_padded = format!("{:<16}", result.name);
    let detail_padded = format!("{:<50}", result.detail);

    println!(
        "  {}  {} {} {}",
        icon,
        name_padded,
        detail_padded,
        dur.dimmed()
    );

    // Error details (indented under the phase)
    for err in &result.errors {
        println!("     {}", "\u{2502}".dimmed());
        println!("     {} {}", "\u{2502}".dimmed(), err.red());
    }

    // Hint box (dashed border)
    if !result.hints.is_empty() {
        println!("     {}", "\u{2502}".dimmed());
        let max_w = result.hints.iter().map(|h| h.len()).max().unwrap_or(40);
        let dashes = "\u{254C}".repeat(max_w + 2);
        println!(
            "     {} \u{256D}{}\u{256E}",
            "\u{2502}".dimmed(),
            dashes.dimmed()
        );
        for hint in &result.hints {
            let pad = max_w.saturating_sub(hint.len());
            println!(
                "     {} \u{2502} {}{} \u{2502}",
                "\u{2502}".dimmed(),
                hint,
                " ".repeat(pad)
            );
        }
        println!(
            "     {} \u{2570}{}\u{256F}",
            "\u{2502}".dimmed(),
            dashes.dimmed()
        );
    }
}

/// Print a skipped phase (dependency failed).
///
/// ```text
///   (/)  bindings        skipped (DAG invalid)
/// ```
pub fn print_phase_skipped(name: &str, reason: &str) {
    let name_padded = format!("{:<16}", name);
    println!(
        "  {}  {} {}",
        icons::skipped(),
        name_padded,
        format!("skipped ({})", reason).dimmed()
    );
}

/// Print the MCP validation section for --strict mode.
///
/// ```text
///   -- MCP Validation -------------------------------------------------------
///
///   (+) novanet
///   | connected . 47 tools available                            320ms
///   |
///   | v analyze     -> novanet_search        params valid
///   | x publish     -> novanet_write         2 errors
///   |   | [params.resource]  must be one of: ...
///   |
///   | 2/3 calls valid
/// ```
pub fn print_mcp_validation(results: &[McpCheckResult]) {
    let w = term_width();

    println!();
    let label = "\u{2500}\u{2500} MCP Validation ";
    let fill = "\u{2500}".repeat(w.saturating_sub(label.len() + 2));
    println!("  {}{}", label.dimmed(), fill.dimmed());
    println!();

    for result in results {
        println!("  {} {}", icons::mcp(), result.server_name.green().bold());

        // Connection info line
        let conn_info = format!("connected \u{00B7} {} tools available", result.tool_count);
        let dur_str = format!("{}ms", result.connect_ms);
        let conn_pad = w.saturating_sub(
            // "  | " prefix = 4 chars, plus conn_info + dur_str
            4 + conn_info.len() + dur_str.len() + 2,
        );
        println!(
            "  {} {} \u{00B7} {} tools available{}{}",
            "\u{2502}".dimmed(),
            "connected".green(),
            result.tool_count,
            " ".repeat(conn_pad),
            dur_str.dimmed()
        );
        println!("  {}", "\u{2502}".dimmed());

        let mut valid_count = 0u32;
        let total = result.validations.len() as u32;

        for v in &result.validations {
            if v.valid {
                valid_count += 1;
                println!(
                    "  {} {} {:<14}\u{2192} {:<24} {}",
                    "\u{2502}".dimmed(),
                    icons::success(),
                    v.task_id,
                    v.tool_name,
                    "params valid".dimmed()
                );
            } else {
                println!(
                    "  {} {} {:<14}\u{2192} {:<24} {}",
                    "\u{2502}".dimmed(),
                    icons::failed(),
                    v.task_id.red(),
                    v.tool_name,
                    format!("{} errors", v.errors.len()).red()
                );
                for err in &v.errors {
                    println!(
                        "  {}   {} {}  {}",
                        "\u{2502}".dimmed(),
                        "\u{2502}".dimmed(),
                        format!("[{}]", err.path).yellow(),
                        err.message.dimmed()
                    );
                }
            }
        }

        println!("  {}", "\u{2502}".dimmed());
        let summary = format!("{}/{} calls valid", valid_count, total);
        let summary_colored = if valid_count == total {
            summary.green()
        } else {
            summary.yellow()
        };
        println!("  {} {}", "\u{2502}".dimmed(), summary_colored);
        println!();
    }
}

/// Print the check summary footer.
///
/// ```text
/// +-----------------------------------------------------------+
/// |                                                           |
/// |  v  V A L I D                                      6ms   |
/// |                                                           |
/// |  6 tasks . 5 edges . 3 layers . 2 schemas . 0 warnings   |
/// |                                                           |
/// +-----------------------------------------------------------+
/// ```
#[allow(clippy::too_many_arguments)]
pub fn print_check_summary(
    valid: bool,
    total_ms: u64,
    task_count: usize,
    edge_count: usize,
    layer_count: usize,
    schema_count: u32,
    strict_info: Option<(u32, u32, u32)>, // (valid_calls, total_calls, param_errors)
    error_codes: &[(&str, &str)],         // (code, message) for NIKA-XXX errors
) {
    let w = term_width();
    let inner = w - 2;
    let border = "\u{2500}".repeat(inner);

    println!("\u{256D}{}\u{256E}", border.dimmed());
    println!("\u{2502}{}\u{2502}", " ".repeat(inner));

    // Status line: build plain text first, measure, then color
    let (icon, label) = if valid {
        (icons::success(), "V A L I D".green().bold())
    } else {
        (icons::failed(), "I N V A L I D".red().bold())
    };
    let dur = format!("{}ms", total_ms);
    let status_line = format!("  {}  {}", icon, label);
    let pad = inner.saturating_sub(stripped_len(&status_line) + dur.len() + 2);
    println!(
        "\u{2502}{}{}{}  \u{2502}",
        status_line,
        " ".repeat(pad),
        dur.dimmed()
    );
    println!("\u{2502}{}\u{2502}", " ".repeat(inner));

    // Stats line
    let mut stats_parts = vec![
        format!(
            "{} {}",
            task_count,
            if task_count == 1 { "task" } else { "tasks" }
        ),
        format!(
            "{} {}",
            edge_count,
            if edge_count == 1 { "edge" } else { "edges" }
        ),
        format!(
            "{} {}",
            layer_count,
            if layer_count == 1 { "layer" } else { "layers" }
        ),
    ];
    if schema_count > 0 {
        stats_parts.push(format!("{} schemas", schema_count));
    }
    let stats = stats_parts.join(" \u{00B7} ");
    let stats_pad = inner.saturating_sub(stats.len() + 2);
    println!("\u{2502}  {}{}\u{2502}", stats, " ".repeat(stats_pad));

    // Strict info
    if let Some((valid_calls, total_calls, param_errors)) = strict_info {
        let strict_line = format!(
            "strict: {}/{} MCP calls valid \u{00B7} {} param errors",
            valid_calls, total_calls, param_errors
        );
        let strict_pad = inner.saturating_sub(strict_line.len() + 2);
        println!(
            "\u{2502}  {}{}\u{2502}",
            strict_line,
            " ".repeat(strict_pad)
        );
    }

    // Error codes
    for (code, msg) in error_codes {
        let err_line = format!("{}: {}", code, msg);
        let err_pad = inner.saturating_sub(err_line.len() + 2);
        println!(
            "\u{2502}  {}{}\u{2502}",
            err_line.red(),
            " ".repeat(err_pad)
        );
    }

    println!("\u{2502}{}\u{2502}", " ".repeat(inner));
    println!("\u{2570}{}\u{256F}", border.dimmed());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_result_pass() {
        let result = PhaseResult {
            name: "schema",
            passed: true,
            detail: "YAML valid against @0.12".to_string(),
            duration_ms: 1,
            errors: vec![],
            hints: vec![],
        };
        // Should not panic
        print_phase(&result);
    }

    #[test]
    fn test_phase_result_fail_with_hints() {
        let result = PhaseResult {
            name: "dag",
            passed: false,
            detail: "CYCLE DETECTED".to_string(),
            duration_ms: 0,
            errors: vec!["step_a \u{2192} step_b \u{2192} step_c \u{2192} step_a".to_string()],
            hints: vec![
                "Remove one dependency to break the cycle.".to_string(),
                "Common fix: use with: binding instead of depends_on.".to_string(),
            ],
        };
        print_phase(&result);
    }

    #[test]
    fn test_stripped_len_plain() {
        // Plain text: no ANSI escapes
        assert_eq!(stripped_len("hello"), 5);
        assert_eq!(stripped_len(""), 0);
        assert_eq!(stripped_len("V A L I D"), 9);
    }

    #[test]
    fn test_stripped_len_colored_crate() {
        // Test with colored crate output (matches real usage)
        use colored::Colorize;
        let green = "hello".green().to_string();
        assert_eq!(stripped_len(&green), 5);
        let bold_green = "\u{2713}".green().bold().to_string();
        assert_eq!(stripped_len(&bold_green), 1);
    }
}
