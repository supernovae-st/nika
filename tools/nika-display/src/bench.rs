// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Bench display — `nika bench` output formatting.
//!
//! Pure-function formatters that return `Vec<String>` for testability.
//! Thin `print_*` wrappers handle actual output. Follows the same pattern
//! as `summary.rs` and `header.rs`.

use std::time::Duration;

use colored::Colorize;

use super::cli_format;
use super::colors::{self, stripped_len};
use super::icons;

// ═══════════════════════════════════════════════════════════════════════════
// DATA TYPES
// ═══════════════════════════════════════════════════════════════════════════

/// Percentile latency values for a single provider.
#[derive(Debug, Clone, Default)]
pub struct Percentiles {
    pub p50: u64,
    pub p90: u64,
    pub p99: u64,
}

/// Per-task timing entry within a benchmark run.
#[derive(Debug, Clone)]
pub struct BenchTaskTiming {
    pub task_id: String,
    pub verb: String,
    /// Start offset from workflow start, in milliseconds.
    pub start_ms: u64,
    /// Task duration in milliseconds.
    pub duration_ms: u64,
    /// True if this task was the bottleneck (longest on the critical path).
    pub is_bottleneck: bool,
}

/// Quality score for one evaluation criterion.
#[derive(Debug, Clone)]
pub struct QualityScore {
    pub criterion: String,
    /// Score from 0.0 to 1.0.
    pub score: f64,
}

/// Aggregated benchmark results for a single provider.
#[derive(Debug, Clone)]
pub struct BenchProviderResult {
    /// Display name (e.g. "anthropic", "openai", "native").
    pub provider: String,
    /// Model used (e.g. "claude-sonnet-4", "gpt-4o").
    pub model: String,

    // ── Speed ──
    /// Time-to-first-token percentiles across iterations.
    pub ttft: Percentiles,
    /// Output throughput in tokens per second.
    pub tokens_per_sec: f64,
    /// Total wall-clock time in seconds (averaged across iterations).
    pub total_secs: f64,

    // ── Cost ──
    /// Cost per run in USD.
    pub cost_per_run: f64,
    /// Total input tokens (averaged across iterations).
    pub input_tokens: u64,
    /// Total output tokens (averaged across iterations).
    pub output_tokens: u64,
    /// Cache read tokens (averaged across iterations).
    pub cache_tokens: u64,

    // ── Profile ──
    /// Per-task Gantt timeline (from the median run).
    pub task_timeline: Vec<BenchTaskTiming>,

    // ── Quality ──
    /// Quality evaluation scores (optional — requires judge model).
    pub quality_scores: Vec<QualityScore>,
    /// Overall quality score (0.0 to 1.0).
    pub quality_overall: Option<f64>,
}

// ═══════════════════════════════════════════════════════════════════════════
// HEADER
// ═══════════════════════════════════════════════════════════════════════════

/// Format the bench header box.
///
/// ```text
/// ╭──────────────────────────────────────────────────────────────────╮
/// │                                                                  │
/// │  N I K A   B E N C H                                     v0.50  │
/// │                                                                  │
/// │  research-pipeline.nika.yaml · 5 tasks · 3 iterations           │
/// │                                                                  │
/// ╰──────────────────────────────────────────────────────────────────╯
/// ```
pub fn format_bench_header(
    workflow: &str,
    task_count: usize,
    iterations: usize,
    providers: &[String],
    version: &str,
    term_width: u16,
) -> Vec<String> {
    let w = (term_width as usize).clamp(40, 72);
    let inner = w - 2;
    let border = "\u{2500}".repeat(inner); // ─

    let mut lines = Vec::new();
    lines.push(String::new());
    lines.push(format!("\u{256D}{}\u{256E}", border.dimmed())); // ╭╮
    lines.push(format!("\u{2502}{}\u{2502}", " ".repeat(inner))); // │ │

    // Title line: N I K A   B E N C H ... vX.XX
    let title = "N I K A   B E N C H";
    let ver = format!("v{}", version);
    let pad = inner.saturating_sub(title.len() + ver.len() + 4);
    lines.push(format!(
        "\u{2502}  {}{}{}  \u{2502}",
        title.bold().white(),
        " ".repeat(pad),
        ver.dimmed()
    ));
    lines.push(format!("\u{2502}{}\u{2502}", " ".repeat(inner))); // │ │

    // Info line: workflow · N tasks · M iterations
    let task_word = if task_count == 1 { "task" } else { "tasks" };
    let iter_word = if iterations == 1 {
        "iteration"
    } else {
        "iterations"
    };
    let info = format!(
        "{} \u{00B7} {} {} \u{00B7} {} {}",
        workflow, task_count, task_word, iterations, iter_word
    );
    let info_pad = inner.saturating_sub(info.len() + 2);
    lines.push(format!(
        "\u{2502}  {}{}  \u{2502}",
        info,
        " ".repeat(info_pad.saturating_sub(2))
    ));

    // Provider badges
    if !providers.is_empty() {
        let badges: Vec<String> = providers
            .iter()
            .map(|p| format!("{} {}", icons::provider(), p))
            .collect();
        let badge_line = badges.join("  ");
        let badge_pad = inner.saturating_sub(stripped_len(&badge_line) + 2);
        lines.push(format!(
            "\u{2502}  {}{}  \u{2502}",
            badge_line,
            " ".repeat(badge_pad.saturating_sub(2))
        ));
    }

    lines.push(format!("\u{2502}{}\u{2502}", " ".repeat(inner))); // │ │
    lines.push(format!("\u{2570}{}\u{256F}", border.dimmed())); // ╰╯

    lines
}

/// Print the bench header box.
pub fn print_bench_header(
    workflow: &str,
    task_count: usize,
    iterations: usize,
    providers: &[String],
) {
    let version = env!("CARGO_PKG_VERSION");
    let term_width = cli_format::terminal_width() as u16;
    for line in format_bench_header(
        workflow, task_count, iterations, providers, version, term_width,
    ) {
        println!("{}", line);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SPEED SECTION
// ═══════════════════════════════════════════════════════════════════════════

/// Format the speed comparison section.
///
/// ```text
///   ── Speed ─────────────────────────────────────────────────────────
///
///   Provider        TTFT [p50  p90  p99]       Tok/s      Total
///   ⋈ anthropic      230ms  340ms  510ms        85/s      12.4s
///   ⋈ openai          45ms   62ms   89ms       142/s       7.1s  ✧ fastest
///   ⋈ native           12ms   15ms   18ms        38/s      26.8s
///
///   openai ran 1.7× faster than anthropic
/// ```
pub fn format_speed_section(results: &[BenchProviderResult], term_width: u16) -> Vec<String> {
    let w = (term_width as usize).clamp(40, 80);
    let mut lines = Vec::new();

    lines.push(String::new());
    lines.push(format!(
        "  {} {} {}",
        "\u{2500}\u{2500}".dimmed(),
        "Speed".bold(),
        "\u{2500}".repeat(w.saturating_sub(12)).dimmed()
    ));
    lines.push(String::new());

    // Column header
    lines.push(format!(
        "  {:<16}{:<28}{:>6}  {:>8}",
        "Provider".dimmed(),
        "TTFT [p50  p90  p99]".dimmed(),
        "Tok/s".dimmed(),
        "Total".dimmed(),
    ));

    // Find fastest provider (lowest total_secs)
    let fastest_idx = results
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            a.total_secs
                .partial_cmp(&b.total_secs)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i);

    for (i, r) in results.iter().enumerate() {
        let p50 = colors::ttft(r.ttft.p50);
        let p90 = colors::ttft(r.ttft.p90);
        let p99 = colors::ttft(r.ttft.p99);

        let tps = format!("{}/s", r.tokens_per_sec.round() as u64);
        let total = colors::duration(r.total_secs as f32);

        let badge = if fastest_idx == Some(i) && results.len() > 1 {
            format!("  {}", "\u{2727} fastest".green())
        } else {
            String::new()
        };

        // Build the TTFT cell — pad each value to fixed width for alignment
        let ttft_cell = format!(
            "{} {} {}",
            pad_colored_right(&p50, 8),
            pad_colored_right(&p90, 8),
            pad_colored_right(&p99, 8)
        );

        let provider_cell = format!("{} {}", icons::provider(), r.provider);

        lines.push(format!(
            "  {:<16}{}  {:>6}  {:>8}{}",
            provider_cell,
            ttft_cell,
            tps.dimmed(),
            total,
            badge,
        ));
    }

    // Relative speed comparison
    if results.len() >= 2 {
        if let Some(fi) = fastest_idx {
            let fastest = &results[fi];
            // Find slowest among the rest
            let slowest =
                results
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != fi)
                    .max_by(|(_, a), (_, b)| {
                        a.total_secs
                            .partial_cmp(&b.total_secs)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
            if let Some((_, slow)) = slowest {
                if fastest.total_secs > 0.0 {
                    let ratio = slow.total_secs / fastest.total_secs;
                    if ratio > 1.05 {
                        lines.push(String::new());
                        lines.push(format!(
                            "  {} ran {}\u{00D7} faster than {}",
                            fastest.provider.green().bold(),
                            format!("{:.1}", ratio).white().bold(),
                            slow.provider.dimmed(),
                        ));
                    }
                }
            }
        }
    }

    lines
}

/// Print the speed comparison section.
pub fn print_speed_section(results: &[BenchProviderResult]) {
    let term_width = cli_format::terminal_width() as u16;
    for line in format_speed_section(results, term_width) {
        println!("{}", line);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// COST SECTION
// ═══════════════════════════════════════════════════════════════════════════

/// Format the cost comparison section.
///
/// ```text
///   ── Cost ──────────────────────────────────────────────────────────
///
///   Provider        $/run       In      Out    Cache    Savings
///   ⋈ anthropic     $0.032    12.4k    1.8k    4.2k       −
///   ⋈ openai        $0.018     8.1k    1.2k     0        44% cheaper
///   ⋈ native        $0.000     8.1k    1.0k     0       100% cheaper
/// ```
pub fn format_cost_section(results: &[BenchProviderResult], term_width: u16) -> Vec<String> {
    let w = (term_width as usize).clamp(40, 80);
    let mut lines = Vec::new();

    lines.push(String::new());
    lines.push(format!(
        "  {} {} {}",
        "\u{2500}\u{2500}".dimmed(),
        "Cost".bold(),
        "\u{2500}".repeat(w.saturating_sub(11)).dimmed()
    ));
    lines.push(String::new());

    // Column header
    lines.push(format!(
        "  {:<16}{:<12}{:>6}  {:>6}  {:>6}   {}",
        "Provider".dimmed(),
        "$/run".dimmed(),
        "In".dimmed(),
        "Out".dimmed(),
        "Cache".dimmed(),
        "Savings".dimmed(),
    ));

    // Find cheapest provider
    let cheapest_cost = results
        .iter()
        .map(|r| r.cost_per_run)
        .fold(f64::INFINITY, f64::min);

    // Find most expensive for savings %
    let max_cost = results
        .iter()
        .map(|r| r.cost_per_run)
        .fold(0.0_f64, f64::max);

    for r in results {
        let cost_str = colors::cost(r.cost_per_run);
        let in_str = colors::tokens(r.input_tokens);
        let out_str = colors::tokens(r.output_tokens);
        let cache_str = if r.cache_tokens > 0 {
            colors::tokens(r.cache_tokens)
        } else {
            "0".to_string()
        };

        let savings = if max_cost > 0.0 && (r.cost_per_run - cheapest_cost).abs() < f64::EPSILON {
            if results.len() > 1 && max_cost > cheapest_cost {
                let pct = ((max_cost - r.cost_per_run) / max_cost * 100.0).round() as u32;
                if pct > 0 {
                    format!("{}% cheaper", pct).green().to_string()
                } else {
                    "\u{2500}".dimmed().to_string() // ─
                }
            } else {
                "\u{2500}".dimmed().to_string()
            }
        } else if max_cost > 0.0 && r.cost_per_run < max_cost {
            let pct = ((max_cost - r.cost_per_run) / max_cost * 100.0).round() as u32;
            format!("{}% cheaper", pct).green().to_string()
        } else {
            "\u{2500}".dimmed().to_string()
        };

        let provider_cell = format!("{} {}", icons::provider(), r.provider);

        lines.push(format!(
            "  {:<16}{:<12}{:>6}  {:>6}  {:>6}   {}",
            provider_cell, cost_str, in_str, out_str, cache_str, savings,
        ));
    }

    lines
}

/// Print the cost comparison section.
pub fn print_cost_section(results: &[BenchProviderResult]) {
    let term_width = cli_format::terminal_width() as u16;
    for line in format_cost_section(results, term_width) {
        println!("{}", line);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROFILE SECTION (per-task Gantt)
// ═══════════════════════════════════════════════════════════════════════════

/// Format the per-task profile section with Gantt bars.
///
/// ```text
///   ── Profile ───────────────────────────────────────────────────────
///
///   ⋈ anthropic
///   ✧ research    ████████████░░░░░░░░░░░░░░░░░░     4.2s
///   ☄ scrape      ░░░░░░░░░░░░████░░░░░░░░░░░░░░     1.1s
///   ✧ summarize   ░░░░░░░░░░░░░░░░████████████████    7.1s  ⚠ bottleneck
///
///   ⋈ openai
///   ✧ research    ██████░░░░░░░░░░░░░░░░░░░░░░░░     2.1s
///   ☄ scrape      ░░░░░░████░░░░░░░░░░░░░░░░░░░░     1.0s
///   ✧ summarize   ░░░░░░░░░░████████████░░░░░░░░     4.0s  ⚠ bottleneck
/// ```
pub fn format_profile_section(results: &[BenchProviderResult], term_width: u16) -> Vec<String> {
    let w = (term_width as usize).clamp(40, 80);
    let bar_width = 30;
    let mut lines = Vec::new();

    lines.push(String::new());
    lines.push(format!(
        "  {} {} {}",
        "\u{2500}\u{2500}".dimmed(),
        "Profile".bold(),
        "\u{2500}".repeat(w.saturating_sub(14)).dimmed()
    ));

    for r in results {
        if r.task_timeline.is_empty() {
            continue;
        }

        lines.push(String::new());
        lines.push(format!("  {} {}", icons::provider(), r.provider.bold()));

        // Find total duration for this provider
        let total_ms = r
            .task_timeline
            .iter()
            .map(|t| t.start_ms + t.duration_ms)
            .max()
            .unwrap_or(1);

        for task in &r.task_timeline {
            let start_pct = task.start_ms as f64 / total_ms as f64;
            let dur_pct = task.duration_ms as f64 / total_ms as f64;
            let start_col = (start_pct * bar_width as f64).round() as usize;
            let dur_col = (dur_pct * bar_width as f64).round().max(1.0) as usize;
            let end_col = (start_col + dur_col).min(bar_width);

            let mut bar = String::new();
            for i in 0..bar_width {
                if i >= start_col && i < end_col {
                    bar.push('\u{2588}'); // █
                } else {
                    bar.push('\u{2591}'); // ░
                }
            }

            let colored_bar = match task.verb.as_str() {
                "infer" => bar.magenta().to_string(),
                "exec" => bar.yellow().to_string(),
                "fetch" => bar.cyan().to_string(),
                "invoke" => bar.green().to_string(),
                "agent" => bar.red().to_string(),
                _ => bar,
            };

            let dur_secs = task.duration_ms as f32 / 1000.0;
            let badge = if task.is_bottleneck {
                format!("  {}", "\u{26A0} bottleneck".yellow())
            } else {
                String::new()
            };

            lines.push(format!(
                "  {} {:<12} {} {:>7}{}",
                icons::verb(&task.verb),
                task.task_id.dimmed(),
                colored_bar,
                colors::duration(dur_secs),
                badge,
            ));
        }
    }

    lines
}

/// Print the profile section.
pub fn print_profile_section(results: &[BenchProviderResult]) {
    let term_width = cli_format::terminal_width() as u16;
    for line in format_profile_section(results, term_width) {
        println!("{}", line);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// QUALITY SECTION
// ═══════════════════════════════════════════════════════════════════════════

/// Format the quality comparison section with inline bar charts.
///
/// ```text
///   ── Quality ───────────────────────────────────────────────────────
///
///   Provider        Overall   Accuracy   Relevance   Completeness
///   ⋈ anthropic      92%     ▓▓▓▓▓▓▓▓░░  ▓▓▓▓▓▓▓▓▓░  ▓▓▓▓▓▓▓░░░
///   ⋈ openai         88%     ▓▓▓▓▓▓▓░░░  ▓▓▓▓▓▓▓▓░░  ▓▓▓▓▓▓▓▓░░
///   ⋈ native          61%     ▓▓▓▓▓░░░░░  ▓▓▓▓▓▓░░░░  ▓▓▓▓▓░░░░░
/// ```
pub fn format_quality_section(results: &[BenchProviderResult], term_width: u16) -> Vec<String> {
    let w = (term_width as usize).clamp(40, 80);
    let mut lines = Vec::new();

    // Only show if at least one provider has quality scores
    let has_quality = results.iter().any(|r| r.quality_overall.is_some());
    if !has_quality {
        return lines;
    }

    lines.push(String::new());
    lines.push(format!(
        "  {} {} {}",
        "\u{2500}\u{2500}".dimmed(),
        "Quality".bold(),
        "\u{2500}".repeat(w.saturating_sub(14)).dimmed()
    ));
    lines.push(String::new());

    // Collect all unique criteria across all providers
    let mut criteria: Vec<String> = Vec::new();
    for r in results {
        for qs in &r.quality_scores {
            if !criteria.contains(&qs.criterion) {
                criteria.push(qs.criterion.clone());
            }
        }
    }

    // Header line: Provider + Overall + each criterion
    let mut header = format!("  {:<16}{:<10}", "Provider".dimmed(), "Overall".dimmed());
    for c in &criteria {
        header.push_str(&format!("  {:<12}", c.dimmed()));
    }
    lines.push(header);

    for r in results {
        let overall_pct = r.quality_overall.unwrap_or(0.0);
        let overall_str = format!("{}%", (overall_pct * 100.0).round() as u32);
        let overall_colored = if overall_pct >= 0.9 {
            overall_str.green()
        } else if overall_pct >= 0.7 {
            overall_str.yellow()
        } else {
            overall_str.red()
        };

        let provider_cell = format!("{} {}", icons::provider(), r.provider);
        let mut row = format!("  {:<16}{:<10}", provider_cell, overall_colored,);

        for criterion in &criteria {
            let score = r
                .quality_scores
                .iter()
                .find(|qs| qs.criterion == *criterion)
                .map(|qs| qs.score)
                .unwrap_or(0.0);

            row.push_str(&format!("  {}", quality_bar(score, 10)));
        }

        lines.push(row);
    }

    lines
}

/// Print the quality section.
pub fn print_quality_section(results: &[BenchProviderResult]) {
    let term_width = cli_format::terminal_width() as u16;
    for line in format_quality_section(results, term_width) {
        println!("{}", line);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SUMMARY (verdict box)
// ═══════════════════════════════════════════════════════════════════════════

/// Format the bench summary verdict box.
///
/// ```text
/// ╭──────────────────────────────────────────────────────────────────╮
/// │                                                                  │
/// │  ✓  V E R D I C T                                        12.4s  │
/// │                                                                  │
/// │  Speed     ⋈ openai         7.1s    1.7× faster                 │
/// │  Cost      ⋈ native        $0.000   100% cheaper                │
/// │  Quality   ⋈ anthropic      92%     best overall                │
/// │                                                                  │
/// │  3 providers · 5 tasks · 3 iterations · 15 runs total           │
/// │                                                                  │
/// ╰──────────────────────────────────────────────────────────────────╯
/// ```
pub fn format_bench_summary(
    results: &[BenchProviderResult],
    bench_duration: Duration,
    task_count: usize,
    iterations: usize,
    term_width: u16,
) -> Vec<String> {
    let w = (term_width as usize).clamp(40, 72);
    let inner = w - 2;
    let border = "\u{2500}".repeat(inner);

    let mut lines = Vec::new();

    lines.push(String::new());
    lines.push(format!("\u{256D}{}\u{256E}", border.dimmed()));
    lines.push(format!("\u{2502}{}\u{2502}", " ".repeat(inner)));

    // Verdict title
    let dur_secs = bench_duration.as_secs_f32();
    let verdict_line = format!(
        "  {}  V E R D I C T                                       {}",
        icons::success(),
        colors::duration(dur_secs)
    );
    lines.push(format!(
        "\u{2502}{}\u{2502}",
        pad_right(&verdict_line, inner)
    ));
    lines.push(format!("\u{2502}{}\u{2502}", " ".repeat(inner)));

    if !results.is_empty() {
        // Speed winner (lowest total_secs)
        if let Some(speed_winner) = results.iter().min_by(|a, b| {
            a.total_secs
                .partial_cmp(&b.total_secs)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            let slowest = results.iter().max_by(|a, b| {
                a.total_secs
                    .partial_cmp(&b.total_secs)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let ratio_str = if let Some(slow) = slowest {
                if speed_winner.total_secs > 0.0 {
                    let ratio = slow.total_secs / speed_winner.total_secs;
                    if ratio > 1.05 && results.len() > 1 {
                        format!("{:.1}\u{00D7} faster", ratio).green().to_string()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            let speed_line = format!(
                "  {:<10}{} {:<14}{:>8}   {}",
                "Speed".dimmed(),
                icons::provider(),
                speed_winner.provider,
                colors::duration(speed_winner.total_secs as f32),
                ratio_str,
            );
            lines.push(format!("\u{2502}{}\u{2502}", pad_right(&speed_line, inner)));
        }

        // Cost winner (lowest cost_per_run)
        if let Some(cost_winner) = results.iter().min_by(|a, b| {
            a.cost_per_run
                .partial_cmp(&b.cost_per_run)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            let most_expensive = results.iter().max_by(|a, b| {
                a.cost_per_run
                    .partial_cmp(&b.cost_per_run)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let savings_str = if let Some(expensive) = most_expensive {
                if expensive.cost_per_run > 0.0 && results.len() > 1 {
                    let pct = ((expensive.cost_per_run - cost_winner.cost_per_run)
                        / expensive.cost_per_run
                        * 100.0)
                        .round() as u32;
                    if pct > 0 {
                        format!("{}% cheaper", pct).green().to_string()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            let cost_line = format!(
                "  {:<10}{} {:<14}{:>8}   {}",
                "Cost".dimmed(),
                icons::provider(),
                cost_winner.provider,
                colors::cost(cost_winner.cost_per_run),
                savings_str,
            );
            lines.push(format!("\u{2502}{}\u{2502}", pad_right(&cost_line, inner)));
        }

        // Quality winner (highest quality_overall)
        let quality_winner = results
            .iter()
            .filter(|r| r.quality_overall.is_some())
            .max_by(|a, b| {
                a.quality_overall
                    .partial_cmp(&b.quality_overall)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        if let Some(qw) = quality_winner {
            let pct = (qw.quality_overall.unwrap_or(0.0) * 100.0).round() as u32;
            let quality_line = format!(
                "  {:<10}{} {:<14}{:>7}%   {}",
                "Quality".dimmed(),
                icons::provider(),
                qw.provider,
                pct,
                "best overall".green(),
            );
            lines.push(format!(
                "\u{2502}{}\u{2502}",
                pad_right(&quality_line, inner)
            ));
        }
    }

    lines.push(format!("\u{2502}{}\u{2502}", " ".repeat(inner)));

    // Stats footer
    let total_runs = results.len() * iterations;
    let provider_count = results.len();
    let stats_line = format!(
        "  {} provider{} \u{00B7} {} task{} \u{00B7} {} iteration{} \u{00B7} {} runs total",
        provider_count,
        if provider_count == 1 { "" } else { "s" },
        task_count,
        if task_count == 1 { "" } else { "s" },
        iterations,
        if iterations == 1 { "" } else { "s" },
        total_runs,
    );
    lines.push(format!(
        "\u{2502}{}\u{2502}",
        pad_right(&format!("  {}", stats_line.dimmed()), inner)
    ));

    lines.push(format!("\u{2502}{}\u{2502}", " ".repeat(inner)));
    lines.push(format!("\u{2570}{}\u{256F}", border.dimmed()));

    lines
}

/// Print the bench summary verdict box.
pub fn print_bench_summary(
    results: &[BenchProviderResult],
    bench_duration: Duration,
    task_count: usize,
    iterations: usize,
) {
    let term_width = cli_format::terminal_width() as u16;
    for line in format_bench_summary(results, bench_duration, task_count, iterations, term_width) {
        println!("{}", line);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// HELPERS (private)
// ═══════════════════════════════════════════════════════════════════════════

/// Pad a string to `width` visible columns, accounting for ANSI escapes.
fn pad_right(s: &str, width: usize) -> String {
    let visible = stripped_len(s);
    if visible >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - visible))
    }
}

/// Pad a `ColoredString` to `width` visible columns.
fn pad_colored_right(cs: &colored::ColoredString, width: usize) -> String {
    let rendered = cs.to_string();
    let visible = stripped_len(&rendered);
    let pad = width.saturating_sub(visible);
    format!("{}{}", rendered, " ".repeat(pad))
}

/// Render a quality bar: filled ▓ + empty ░.
///
/// Score is 0.0 to 1.0. Bar width is `width` characters.
/// Color: green >= 0.8, yellow >= 0.6, red < 0.6.
fn quality_bar(score: f64, width: usize) -> String {
    let score = score.clamp(0.0, 1.0);
    let filled = (score * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    let bar = format!("{}{}", "\u{2593}".repeat(filled), "\u{2591}".repeat(empty));
    if score >= 0.8 {
        bar.green().to_string()
    } else if score >= 0.6 {
        bar.yellow().to_string()
    } else {
        bar.red().to_string()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_results() -> Vec<BenchProviderResult> {
        vec![
            BenchProviderResult {
                provider: "anthropic".to_string(),
                model: "claude-sonnet-4".to_string(),
                ttft: Percentiles {
                    p50: 230,
                    p90: 340,
                    p99: 510,
                },
                tokens_per_sec: 85.0,
                total_secs: 12.4,
                cost_per_run: 0.032,
                input_tokens: 12400,
                output_tokens: 1800,
                cache_tokens: 4200,
                task_timeline: vec![
                    BenchTaskTiming {
                        task_id: "research".to_string(),
                        verb: "infer".to_string(),
                        start_ms: 0,
                        duration_ms: 4200,
                        is_bottleneck: false,
                    },
                    BenchTaskTiming {
                        task_id: "scrape".to_string(),
                        verb: "fetch".to_string(),
                        start_ms: 4200,
                        duration_ms: 1100,
                        is_bottleneck: false,
                    },
                    BenchTaskTiming {
                        task_id: "summarize".to_string(),
                        verb: "infer".to_string(),
                        start_ms: 5300,
                        duration_ms: 7100,
                        is_bottleneck: true,
                    },
                ],
                quality_scores: vec![
                    QualityScore {
                        criterion: "Accuracy".to_string(),
                        score: 0.95,
                    },
                    QualityScore {
                        criterion: "Relevance".to_string(),
                        score: 0.90,
                    },
                    QualityScore {
                        criterion: "Completeness".to_string(),
                        score: 0.88,
                    },
                ],
                quality_overall: Some(0.92),
            },
            BenchProviderResult {
                provider: "openai".to_string(),
                model: "gpt-4o".to_string(),
                ttft: Percentiles {
                    p50: 45,
                    p90: 62,
                    p99: 89,
                },
                tokens_per_sec: 142.0,
                total_secs: 7.1,
                cost_per_run: 0.018,
                input_tokens: 8100,
                output_tokens: 1200,
                cache_tokens: 0,
                task_timeline: vec![
                    BenchTaskTiming {
                        task_id: "research".to_string(),
                        verb: "infer".to_string(),
                        start_ms: 0,
                        duration_ms: 2100,
                        is_bottleneck: false,
                    },
                    BenchTaskTiming {
                        task_id: "scrape".to_string(),
                        verb: "fetch".to_string(),
                        start_ms: 2100,
                        duration_ms: 1000,
                        is_bottleneck: false,
                    },
                    BenchTaskTiming {
                        task_id: "summarize".to_string(),
                        verb: "infer".to_string(),
                        start_ms: 3100,
                        duration_ms: 4000,
                        is_bottleneck: true,
                    },
                ],
                quality_scores: vec![
                    QualityScore {
                        criterion: "Accuracy".to_string(),
                        score: 0.85,
                    },
                    QualityScore {
                        criterion: "Relevance".to_string(),
                        score: 0.90,
                    },
                    QualityScore {
                        criterion: "Completeness".to_string(),
                        score: 0.88,
                    },
                ],
                quality_overall: Some(0.88),
            },
            BenchProviderResult {
                provider: "native".to_string(),
                model: "llama-3.2-3b".to_string(),
                ttft: Percentiles {
                    p50: 12,
                    p90: 15,
                    p99: 18,
                },
                tokens_per_sec: 38.0,
                total_secs: 26.8,
                cost_per_run: 0.0,
                input_tokens: 8100,
                output_tokens: 1000,
                cache_tokens: 0,
                task_timeline: vec![
                    BenchTaskTiming {
                        task_id: "research".to_string(),
                        verb: "infer".to_string(),
                        start_ms: 0,
                        duration_ms: 8400,
                        is_bottleneck: false,
                    },
                    BenchTaskTiming {
                        task_id: "scrape".to_string(),
                        verb: "fetch".to_string(),
                        start_ms: 8400,
                        duration_ms: 1100,
                        is_bottleneck: false,
                    },
                    BenchTaskTiming {
                        task_id: "summarize".to_string(),
                        verb: "infer".to_string(),
                        start_ms: 9500,
                        duration_ms: 17300,
                        is_bottleneck: true,
                    },
                ],
                quality_scores: vec![
                    QualityScore {
                        criterion: "Accuracy".to_string(),
                        score: 0.55,
                    },
                    QualityScore {
                        criterion: "Relevance".to_string(),
                        score: 0.65,
                    },
                    QualityScore {
                        criterion: "Completeness".to_string(),
                        score: 0.58,
                    },
                ],
                quality_overall: Some(0.61),
            },
        ]
    }

    #[test]
    fn bench_header_contains_title() {
        let lines = format_bench_header(
            "research-pipeline.nika.yaml",
            5,
            3,
            &["anthropic".into(), "openai".into()],
            "0.50.0",
            72,
        );
        let joined = lines.join("\n");
        assert!(joined.contains("N I K A   B E N C H"), "must contain title");
        assert!(joined.contains("v0.50.0"), "must contain version");
        assert!(
            joined.contains("research-pipeline"),
            "must contain workflow name"
        );
        assert!(joined.contains("5 tasks"), "must contain task count");
        assert!(
            joined.contains("3 iterations"),
            "must contain iteration count"
        );
    }

    #[test]
    fn bench_header_has_rounded_corners() {
        let lines = format_bench_header("test.nika.yaml", 1, 1, &[], "0.50.0", 72);
        let joined = lines.join("\n");
        assert!(joined.contains('\u{256D}'), "must have ╭");
        assert!(joined.contains('\u{256E}'), "must have ╮");
        assert!(joined.contains('\u{2570}'), "must have ╰");
        assert!(joined.contains('\u{256F}'), "must have ╯");
    }

    #[test]
    fn bench_header_singular_forms() {
        let lines = format_bench_header("t.nika.yaml", 1, 1, &[], "0.50.0", 72);
        let joined = lines.join("\n");
        assert!(joined.contains("1 task"), "singular task");
        assert!(joined.contains("1 iteration"), "singular iteration");
    }

    #[test]
    fn speed_section_marks_fastest() {
        let results = sample_results();
        let lines = format_speed_section(&results, 80);
        let joined = lines.join("\n");
        assert!(joined.contains("fastest"), "must mark fastest provider");
        // openai (7.1s) is fastest
        assert!(
            joined.contains("faster than"),
            "must show relative comparison"
        );
    }

    #[test]
    fn speed_section_single_provider_no_comparison() {
        let results = vec![sample_results().remove(0)];
        let lines = format_speed_section(&results, 80);
        let joined = lines.join("\n");
        assert!(
            !joined.contains("faster than"),
            "single provider should not compare"
        );
    }

    #[test]
    fn cost_section_shows_savings() {
        let results = sample_results();
        let lines = format_cost_section(&results, 80);
        let joined = lines.join("\n");
        assert!(joined.contains("cheaper"), "must show savings percentage");
    }

    #[test]
    fn profile_section_shows_bottleneck() {
        let results = sample_results();
        let lines = format_profile_section(&results, 80);
        let joined = lines.join("\n");
        assert!(joined.contains("bottleneck"), "must mark bottleneck tasks");
    }

    #[test]
    fn profile_section_has_gantt_bars() {
        let results = sample_results();
        let lines = format_profile_section(&results, 80);
        let joined = lines.join("\n");
        assert!(
            joined.contains('\u{2588}'),
            "must contain filled blocks (█)"
        );
        assert!(joined.contains('\u{2591}'), "must contain empty blocks (░)");
    }

    #[test]
    fn quality_section_shows_bars() {
        let results = sample_results();
        let lines = format_quality_section(&results, 80);
        let joined = lines.join("\n");
        assert!(!lines.is_empty(), "must have quality section");
        assert!(
            joined.contains('\u{2593}'),
            "must contain quality bar blocks (▓)"
        );
    }

    #[test]
    fn quality_section_hidden_when_no_scores() {
        let results = vec![BenchProviderResult {
            provider: "test".to_string(),
            model: "test-model".to_string(),
            ttft: Percentiles::default(),
            tokens_per_sec: 0.0,
            total_secs: 1.0,
            cost_per_run: 0.0,
            input_tokens: 0,
            output_tokens: 0,
            cache_tokens: 0,
            task_timeline: vec![],
            quality_scores: vec![],
            quality_overall: None,
        }];
        let lines = format_quality_section(&results, 80);
        assert!(lines.is_empty(), "no quality section when no scores");
    }

    #[test]
    fn summary_has_rounded_box() {
        let results = sample_results();
        let lines = format_bench_summary(&results, Duration::from_secs(42), 5, 3, 72);
        let joined = lines.join("\n");
        assert!(joined.contains('\u{256D}'), "must have ╭");
        assert!(joined.contains('\u{256F}'), "must have ╯");
        assert!(joined.contains("V E R D I C T"), "must contain verdict");
    }

    #[test]
    fn summary_shows_winners() {
        let results = sample_results();
        let lines = format_bench_summary(&results, Duration::from_secs(42), 5, 3, 72);
        let joined = lines.join("\n");
        assert!(joined.contains("Speed"), "must show speed winner");
        assert!(joined.contains("Cost"), "must show cost winner");
        assert!(joined.contains("Quality"), "must show quality winner");
    }

    #[test]
    fn summary_shows_run_stats() {
        let results = sample_results();
        let lines = format_bench_summary(&results, Duration::from_secs(42), 5, 3, 72);
        let joined = lines.join("\n");
        assert!(joined.contains("3 providers"), "must show provider count");
        assert!(joined.contains("5 tasks"), "must show task count");
        assert!(
            joined.contains("9 runs total"),
            "3 providers * 3 iterations = 9 runs"
        );
    }

    #[test]
    fn pad_right_works() {
        assert_eq!(pad_right("hi", 5), "hi   ");
        assert_eq!(pad_right("hello", 3), "hello");
        assert_eq!(pad_right("", 3), "   ");
    }

    #[test]
    fn quality_bar_extremes() {
        let full = quality_bar(1.0, 10);
        let empty = quality_bar(0.0, 10);
        assert_eq!(stripped_len(&full), 10);
        assert_eq!(stripped_len(&empty), 10);
    }

    #[test]
    fn quality_bar_clamped() {
        // Values outside 0-1 should be clamped, not panic
        let over = quality_bar(1.5, 10);
        let under = quality_bar(-0.5, 10);
        assert_eq!(stripped_len(&over), 10);
        assert_eq!(stripped_len(&under), 10);
    }

    #[test]
    fn quality_bar_zero_width() {
        let bar = quality_bar(0.5, 0);
        assert_eq!(stripped_len(&bar), 0);
    }

    #[test]
    fn empty_results_summary_still_valid() {
        let lines = format_bench_summary(&[], Duration::from_secs(1), 0, 0, 72);
        let joined = lines.join("\n");
        assert!(joined.contains("V E R D I C T"));
        assert!(joined.contains("0 runs total"));
    }
}
