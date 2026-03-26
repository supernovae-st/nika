//! Summary display helpers — workflow completion and doctor diagnostics.
//!
//! Contains both the legacy `print_done_summary` and the rich box-style
//! `print_run_summary` / `print_run_quiet_summary` used by both
//! `CliRenderer` and `LiveRenderer`.

use colored::Colorize;

use super::colors::{self, stripped_len};
use super::icons;
use super::renderer::{format_bytes, RunStats};
use super::DetailLevel;

// Unicode box characters for lightweight framing.
const TOP_LEFT: &str = "\u{250c}"; // ┌
const TOP_RIGHT: &str = "\u{2510}"; // ┐
const BOTTOM_LEFT: &str = "\u{2514}"; // └
const BOTTOM_RIGHT: &str = "\u{2518}"; // ┘
const HORIZONTAL: &str = "\u{2500}"; // ─
const VERTICAL: &str = "\u{2502}"; // │

/// Print the workflow completion summary.
///
/// ```text
/// ──────────────────────────────────────────────────
/// ✓ Done! (1.7s | 42 tokens | $0.0003)
/// ```
pub fn print_done_summary(
    elapsed_str: &str,
    total_tokens: u64,
    total_cost: f64,
    trace_path: Option<&str>,
    task_count: usize,
    parallel_count: usize,
) {
    println!();
    println!("{}", HORIZONTAL.repeat(50).dimmed());

    // Main done line
    println!(
        "{} {}",
        "\u{2713}".green().bold(), // ✓
        "Done!".green().bold(),
    );

    // One-liner stats
    let mut parts: Vec<String> = Vec::new();
    parts.push(format!(
        "{} task{}",
        task_count,
        if task_count == 1 { "" } else { "s" }
    ));
    if parallel_count > 0 {
        parts.push(format!("{} parallel", parallel_count));
    }
    if total_tokens > 0 {
        let tok_str = if total_tokens >= 1000 {
            format!("{:.1}k tokens", total_tokens as f64 / 1000.0)
        } else {
            format!("{} tokens", total_tokens)
        };
        parts.push(tok_str);
        parts.push(format!(
            "${}",
            crate::provider::cost::format_cost(total_cost).trim_start_matches('$')
        ));
    }
    parts.push(elapsed_str.to_string());

    println!("  {}", parts.join(" | ").dimmed());

    // Trace link
    if let Some(path) = trace_path {
        println!("  {} {}", "trace:".dimmed(), path.dimmed());
    }

    println!();
}

/// Print the doctor header with a nice box.
///
/// ```text
/// ┌─ Nika Doctor ──────────────────────────────────┐
/// │ v0.12.0 | Checking system health...            │
/// └────────────────────────────────────────────────┘
/// ```
pub fn print_doctor_header(version: &str) {
    let title = "Nika Doctor";
    let inner = format!(" v{} | Checking system health... ", version);

    let title_segment = format!("{} {} ", HORIZONTAL, title);
    let min_width = inner.len() + 2;
    let fill_len = if title_segment.len() < min_width {
        min_width - title_segment.len()
    } else {
        1
    };
    let top_fill = HORIZONTAL.repeat(fill_len);
    let top_line = format!(
        "{}{}{}{}",
        TOP_LEFT.dimmed(),
        title_segment.bold(),
        top_fill.dimmed(),
        TOP_RIGHT.dimmed()
    );

    let total_width = title_segment.len() + fill_len;
    let pad = if inner.len() < total_width {
        " ".repeat(total_width - inner.len())
    } else {
        String::new()
    };
    let content_line = format!(
        "{}{}{}{}",
        VERTICAL.dimmed(),
        inner.dimmed(),
        pad,
        VERTICAL.dimmed()
    );

    let bottom_fill = HORIZONTAL.repeat(total_width);
    let bottom_line = format!(
        "{}{}{}",
        BOTTOM_LEFT.dimmed(),
        bottom_fill.dimmed(),
        BOTTOM_RIGHT.dimmed()
    );

    println!();
    println!("{}", top_line);
    println!("{}", content_line);
    println!("{}", bottom_line);
    println!();
}

// ═══════════════════════════════════════════════════════════════
// Run summary — free functions shared by CliRenderer + LiveRenderer
// ═══════════════════════════════════════════════════════════════

/// Print the compact single-line summary (quiet / min mode).
pub fn print_run_quiet_summary(stats: &RunStats, total_duration_ms: u64) {
    let dur_secs = total_duration_ms as f32 / 1000.0;
    let total = stats.tasks_passed + stats.tasks_failed + stats.tasks_skipped;
    let status = if stats.tasks_failed > 0 {
        icons::failed()
    } else {
        icons::success()
    };
    let cost_str = if stats.total_cost > 0.0 {
        format!(" · {}", colors::cost(stats.total_cost))
    } else {
        String::new()
    };
    println!(
        "{} {} · {}/{}{}",
        status,
        colors::duration(dur_secs),
        stats.tasks_passed,
        total,
        cost_str
    );
}

/// Print the full summary box with tokens, cost, timeline, and provider breakdown.
pub fn print_run_summary(
    stats: &RunStats,
    detail: DetailLevel,
    total_duration_ms: u64,
    trace_path: Option<&str>,
) {
    if detail.is_json() {
        return;
    }

    if detail == DetailLevel::Min {
        print_run_quiet_summary(stats, total_duration_ms);
        return;
    }

    let term_width = terminal_size::terminal_size()
        .map(|(w, _)| w.0)
        .unwrap_or(80);
    let dur_secs = total_duration_ms as f32 / 1000.0;

    println!();

    // ── Summary box ──
    let w = (term_width as usize).clamp(30, 72);
    let border = "─".repeat(w);
    println!("╭{}╮", border.dimmed());
    println!("│{}│", " ".repeat(w));

    // Done/Failed line
    if stats.tasks_failed > 0 {
        let root_cause = stats.root_failure.as_deref().unwrap_or("unknown");
        let failed_line = format!(
            "  {}  F A I L E D                                            {}",
            icons::failed(),
            colors::duration(dur_secs)
        );
        println!("│{}│", pad_right(&failed_line, w));
        println!(
            "│{}│",
            pad_right(&format!("  root cause: {}", root_cause.red()), w)
        );
    } else {
        let done = format!(
            "  {}  D O N E                                              {}",
            icons::success(),
            colors::duration(dur_secs)
        );
        println!("│{}│", pad_right(&done, w));
    }
    println!("│{}│", " ".repeat(w));

    // Tasks
    let passed = stats.tasks_passed.to_string().green();
    let total = (stats.tasks_passed + stats.tasks_failed + stats.tasks_skipped).to_string();
    println!(
        "│{}│",
        pad_right(&format!("  Tasks    {}/{} passed", passed, total), w)
    );
    println!("│{}│", " ".repeat(w));

    // ── Tokens ──
    if stats.total_input_tokens > 0 && detail.show_full_summary() {
        println!(
            "│{}│",
            pad_right(
                &format!("  {} Tokens {}", "──".dimmed(), "─".repeat(w - 16).dimmed()),
                w
            )
        );
        let max_tok = stats
            .total_input_tokens
            .max(stats.total_output_tokens)
            .max(stats.total_cache_tokens);
        println!(
            "│{}│",
            pad_right(
                &format!(
                    "    in {} {}",
                    token_bar(stats.total_input_tokens, max_tok, 30, "blue"),
                    colors::tokens(stats.total_input_tokens)
                ),
                w
            )
        );
        println!(
            "│{}│",
            pad_right(
                &format!(
                    "   out {} {}",
                    token_bar(stats.total_output_tokens, max_tok, 30, "magenta"),
                    colors::tokens(stats.total_output_tokens)
                ),
                w
            )
        );
        if stats.total_cache_tokens > 0 {
            println!(
                "│{}│",
                pad_right(
                    &format!(
                        "    $↻ {} {} saved",
                        token_bar(stats.total_cache_tokens, max_tok, 30, "green"),
                        colors::tokens(stats.total_cache_tokens)
                    ),
                    w
                )
            );
        }
        println!("│{}│", " ".repeat(w));
    }

    // ── Cost ──
    if stats.total_cost > 0.0 && detail.show_full_summary() {
        println!(
            "│{}│",
            pad_right(
                &format!("  {} Cost {}", "──".dimmed(), "─".repeat(w - 14).dimmed()),
                w
            )
        );
        // Per-task cost breakdown using blocks
        let mut task_costs: Vec<(String, f64)> = Vec::new();
        for call in &stats.provider_calls {
            if let Some(existing) = task_costs.iter_mut().find(|(t, _)| *t == call.task_id) {
                existing.1 += call.cost;
            } else {
                task_costs.push((call.task_id.clone(), call.cost));
            }
        }
        let mut cost_parts = Vec::new();
        for (task, c) in &task_costs {
            let blocks = ((c / stats.total_cost) * 20.0).round() as usize;
            cost_parts.push(format!("{} {}", task.dimmed(), "▪".repeat(blocks.max(1))));
        }
        println!(
            "│{}│",
            pad_right(
                &format!(
                    "  {} {}",
                    colors::cost(stats.total_cost),
                    cost_parts.join("  ")
                ),
                w
            )
        );
        println!("│{}│", " ".repeat(w));
    }

    // ── Performance ──
    if !stats.ttft_values.is_empty() && detail.show_full_summary() {
        println!(
            "│{}│",
            pad_right(
                &format!(
                    "  {} Performance {}",
                    "──".dimmed(),
                    "─".repeat(w - 20).dimmed()
                ),
                w
            )
        );
        let count = stats.ttft_values.len() as u64;
        let avg_ttft = if count > 0 {
            stats.ttft_values.iter().sum::<u64>() / count
        } else {
            0
        };
        let min_ttft = stats.ttft_values.iter().min().copied().unwrap_or(0);
        let max_ttft = stats.ttft_values.iter().max().copied().unwrap_or(0);
        let throughput = if dur_secs > 0.0 {
            (stats.total_output_tokens as f32 / dur_secs).round() as u64
        } else {
            0
        };

        println!(
            "│{}│",
            pad_right(
                &format!(
                    "  TTFT     avg {} · min {} · max {}",
                    colors::ttft(avg_ttft),
                    colors::ttft(min_ttft),
                    colors::ttft(max_ttft)
                ),
                w
            )
        );
        println!(
            "│{}│",
            pad_right(&format!("  Throughput  {} tok/s", throughput), w)
        );
        println!("│{}│", " ".repeat(w));
    }

    // ── Infrastructure ──
    if detail.show_full_summary() {
        println!(
            "│{}│",
            pad_right(
                &format!(
                    "  {} Infrastructure {}",
                    "──".dimmed(),
                    "─".repeat(w - 24).dimmed()
                ),
                w
            )
        );
        if stats.mcp_calls > 0 {
            let errors_str = if stats.mcp_errors > 0 {
                stats.mcp_errors.to_string().red().to_string()
            } else {
                stats.mcp_errors.to_string().green().to_string()
            };
            println!(
                "│{}│",
                pad_right(
                    &format!(
                        "  MCP      {} calls · {} retries · {} errors",
                        stats.mcp_calls,
                        stats.mcp_retries.to_string().yellow(),
                        errors_str
                    ),
                    w
                )
            );
        }
        if stats.media_stored > 0 {
            println!(
                "│{}│",
                pad_right(
                    &format!(
                        "  Media    {} stored · {} · {} dedup · ✓ integrity",
                        stats.media_stored,
                        format_bytes(stats.media_bytes),
                        stats.media_dedup
                    ),
                    w
                )
            );
        }
        if stats.artifacts_count > 0 {
            println!(
                "│{}│",
                pad_right(
                    &format!(
                        "  Output   {} artifacts · {} total",
                        stats.artifacts_count,
                        format_bytes(stats.artifacts_bytes)
                    ),
                    w
                )
            );
        }
        if stats.guardrails_passed + stats.guardrails_failed > 0 {
            println!(
                "│{}│",
                pad_right(
                    &format!(
                        "  Guards   {} passed · {} failed · {} escalations",
                        stats.guardrails_passed.to_string().green(),
                        stats.guardrails_failed.to_string().yellow(),
                        stats.guardrails_escalations
                    ),
                    w
                )
            );
        }
        println!("│{}│", " ".repeat(w));
    }

    // ── Timeline (Gantt) ──
    if !stats.task_timeline.is_empty() && detail.show_full_summary() {
        println!(
            "│{}│",
            pad_right(
                &format!(
                    "  {} Timeline {}",
                    "──".dimmed(),
                    "─".repeat(w - 18).dimmed()
                ),
                w
            )
        );

        let total_ms = total_duration_ms;
        let bar_width = 38;

        for (task_id, verb, start_ms, dur_ms) in &stats.task_timeline {
            if total_ms == 0 {
                break;
            }
            let start_pct = *start_ms as f64 / total_ms as f64;
            let dur_pct = *dur_ms as f64 / total_ms as f64;
            let start_col = (start_pct * bar_width as f64).round() as usize;
            let dur_col = (dur_pct * bar_width as f64).round().max(1.0) as usize;
            let end_col = (start_col + dur_col).min(bar_width);

            let mut bar = String::new();
            for i in 0..bar_width {
                if i >= start_col && i < end_col {
                    bar.push('\u{2588}');
                } else {
                    bar.push('\u{2591}');
                }
            }
            // Color the bar based on verb
            let colored_bar = match verb.as_str() {
                "infer" => bar.magenta().to_string(),
                "exec" => bar.yellow().to_string(),
                "fetch" => bar.cyan().to_string(),
                "invoke" => bar.green().to_string(),
                "agent" => bar.red().to_string(),
                _ => bar,
            };
            let dur_secs = *dur_ms as f32 / 1000.0;
            let padded_id = format!("{:<12}", task_id);
            println!(
                "│{}│",
                pad_right(
                    &format!(
                        "  {} {} {} {:>5}",
                        icons::verb(verb),
                        padded_id.dimmed(),
                        colored_bar,
                        colors::duration(dur_secs)
                    ),
                    w
                )
            );
        }

        // Time axis
        let axis = format!(
            "  {:12} 0s{:>12}{:>12} {:.1}s",
            "",
            "",
            "",
            total_ms as f64 / 1000.0
        );
        println!("│{}│", pad_right(&axis.dimmed().to_string(), w));
        println!("│{}│", " ".repeat(w));
    }

    // ── Provider Breakdown Table ──
    if !stats.provider_calls.is_empty() && detail.show_full_summary() {
        println!(
            "│{}│",
            pad_right(
                &format!(
                    "  {} Provider Breakdown {}",
                    "──".dimmed(),
                    "─".repeat(w - 27).dimmed()
                ),
                w
            )
        );
        println!(
            "│{}│",
            pad_right(
                &format!(
                    "  {}   {}      {}    {}   {}    {}",
                    "#".dimmed(),
                    "Task".dimmed(),
                    "In".dimmed(),
                    "Out".dimmed(),
                    "Cache".dimmed(),
                    "Cost".dimmed()
                ),
                w
            )
        );
        for (i, call) in stats.provider_calls.iter().enumerate() {
            let padded_task = format!("{:<12}", call.task_id);
            println!(
                "│{}│",
                pad_right(
                    &format!(
                        "  {}   {} {:>5}  {:>5}  {:>5}   {}",
                        i + 1,
                        padded_task,
                        colors::tokens(call.input_tokens),
                        colors::tokens(call.output_tokens),
                        colors::tokens(call.cache_tokens),
                        colors::cost(call.cost)
                    ),
                    w
                )
            );
        }
        // Totals row
        println!(
            "│{}│",
            pad_right(&format!("  {}", "─".repeat(w - 4).dimmed()), w)
        );
        println!(
            "│{}│",
            pad_right(
                &format!(
                    "  \u{03a3}   {:12} {:>5}  {:>5}  {:>5}   {}",
                    "",
                    colors::tokens(stats.total_input_tokens),
                    colors::tokens(stats.total_output_tokens),
                    colors::tokens(stats.total_cache_tokens),
                    colors::cost(stats.total_cost)
                ),
                w
            )
        );
        println!("│{}│", " ".repeat(w));
    }

    // Trace path
    if let Some(path) = trace_path {
        println!("│{}│", pad_right(&format!("  trace {}", path.dimmed()), w));
    }

    println!("│{}│", " ".repeat(w));
    println!("╰{}╯", border.dimmed());
}

/// Pad a string to width, accounting for ANSI escape codes.
fn pad_right(s: &str, width: usize) -> String {
    let visible = stripped_len(s);
    if visible >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - visible))
    }
}

/// Generate a token bar: filled + empty.
fn token_bar(value: u64, max: u64, width: usize, color: &str) -> String {
    let ratio = if max == 0 {
        0.0
    } else {
        value as f64 / max as f64
    };
    let filled = (ratio * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    let bar = format!(
        "{}{}",
        "\u{2588}".repeat(filled),
        "\u{2591}".repeat(empty)
    );
    match color {
        "blue" => bar.blue().to_string(),
        "magenta" => bar.magenta().to_string(),
        "green" => bar.green().to_string(),
        _ => bar,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_right_no_padding_needed() {
        let result = pad_right("hello", 5);
        assert_eq!(result, "hello");
    }

    #[test]
    fn pad_right_adds_spaces() {
        let result = pad_right("hi", 5);
        assert_eq!(result, "hi   ");
    }

    #[test]
    fn pad_right_wider_than_target() {
        // No truncation — returns the original string as-is
        let result = pad_right("hello world", 5);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn pad_right_exact_width() {
        let result = pad_right("abcde", 5);
        assert_eq!(result, "abcde");
    }

    #[test]
    fn pad_right_empty_string() {
        let result = pad_right("", 5);
        assert_eq!(result, "     ");
    }

    #[test]
    fn pad_right_zero_width() {
        let result = pad_right("hi", 0);
        assert_eq!(result, "hi");
    }

    #[test]
    fn token_bar_zero_max() {
        // Should not panic; ratio becomes 0.0 so all blocks are empty
        let bar = token_bar(100, 0, 10, "blue");
        assert!(!bar.is_empty());
    }

    #[test]
    fn token_bar_full_ratio() {
        let bar = token_bar(100, 100, 10, "blue");
        let vis = stripped_len(&bar);
        assert_eq!(vis, 10, "full ratio bar should be 10 visible columns");
    }

    #[test]
    fn token_bar_zero_value() {
        let bar = token_bar(0, 100, 10, "blue");
        let vis = stripped_len(&bar);
        assert_eq!(vis, 10, "zero-value bar should still be 10 visible columns");
    }

    #[test]
    fn token_bar_half_ratio() {
        let bar = token_bar(50, 100, 10, "magenta");
        let vis = stripped_len(&bar);
        assert_eq!(vis, 10, "half ratio bar should be 10 visible columns");
    }

    #[test]
    fn token_bar_unknown_color_returns_uncolored() {
        let bar = token_bar(50, 100, 10, "unknown");
        // Should not panic; returns the bar without ANSI codes
        assert!(!bar.is_empty());
        // No ANSI escape sequences
        assert!(!bar.contains('\x1b'), "unknown color should produce no ANSI codes");
    }

    #[test]
    fn token_bar_zero_width() {
        let bar = token_bar(50, 100, 0, "green");
        // Width 0 means empty bar (just the color wrapper)
        let vis = stripped_len(&bar);
        assert_eq!(vis, 0);
    }
}

/// Print the doctor summary with colored counts, separator, and optional next steps.
pub fn print_doctor_summary(pass_count: usize, warn_count: usize, fail_count: usize) {
    println!();
    println!("{}", HORIZONTAL.repeat(50).dimmed());

    let status_icon = if fail_count > 0 {
        "\u{2717}".red().bold() // ✗
    } else if warn_count > 0 {
        "\u{26a0}".yellow().bold() // ⚠
    } else {
        "\u{2713}".green().bold() // ✓
    };

    let status_word = if fail_count > 0 {
        "Issues found".red().bold()
    } else if warn_count > 0 {
        "Mostly healthy".yellow().bold()
    } else {
        "All good!".green().bold()
    };

    println!(
        "{} {} \u{2014} {} passed, {} warnings, {} failed", // — (em dash)
        status_icon,
        status_word,
        pass_count.to_string().green(),
        warn_count.to_string().yellow(),
        fail_count.to_string().red()
    );

    if warn_count > 0 || fail_count > 0 {
        println!();
        if !std::path::Path::new(".nika").exists() {
            println!(
                "  {} Run {} to initialize project",
                "\u{2192}".cyan(),
                "nika init".bold()
            );
        }
        println!(
            "  {} Run {} to auto-fix issues",
            "\u{2192}".cyan(),
            "nika doctor --fix".bold()
        );
    }

    println!();
}
