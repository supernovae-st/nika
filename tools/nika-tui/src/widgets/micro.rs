//! # Micro-Widgets for TaskBox Rendering
//!
//! Small inline visual elements that fit inside a TaskBox (1-2 lines, 20-60 chars wide).
//! Each is a pure function: data in, styled `Line`/`Vec<Span>` out.
//!
//! ## Design Language
//!
//! - **Sparklines**: `▁▂▃▄▅▆▇█` (8-level block elements)
//! - **Progress**:  `━` filled, `╌` empty, partial blocks `▏▎▍▌▋▊▉█`
//! - **Badges**:    `╭╮╰╯` rounded corners, single-char height
//! - **Borders**:   `│` left accent, `─` separators
//! - **Indicators**: `●` on, `○` off, `◐◓◑◒` spinning, `▲▼` delta
//!
//! ## Color Palette (Tailwind via `tokens::compat`)
//!
//! | Semantic       | Color         | Use                         |
//! |----------------|---------------|-----------------------------|
//! | `GREEN_500`    | `#22c55e`     | Success, healthy, <50%      |
//! | `AMBER_500`    | `#f59e0b`     | Warning, 50-80%             |
//! | `RED_500`      | `#ef4444`     | Error, >80%, outlier        |
//! | `CYAN_500`     | `#06b6d4`     | Fetch, HTTP, network        |
//! | `VIOLET_500`   | `#8b5cf6`     | Infer, LLM, tokens          |
//! | `EMERALD_500`  | `#10b981`     | Invoke, MCP, connected      |
//! | `ROSE_500`     | `#f43f5e`     | Agent, active turn          |
//! | `SLATE_500`    | `#64748b`     | Muted, disabled, labels     |
//! | `SLATE_200`    | `#e2e8f0`     | Primary text                |
//! | `INDIGO_500`   | `#6366f1`     | Accent, highlight           |

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::tokens::compat;

// ═══════════════════════════════════════════════════════════════════════════════
// SHARED CONSTANTS
// ═══════════════════════════════════════════════════════════════════════════════

/// Sparkline block characters ordered by fill level (8 levels)
const SPARK: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Braille spinner frames (8 frames, ~125ms per frame at 60fps tick)
const SPINNER: &[&str] = &["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];

/// Horizontal bar characters for sub-character precision (8 levels)
const HBAR: &[&str] = &["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];

// ═══════════════════════════════════════════════════════════════════════════════
// 1. THROUGHPUT METER (tokens/sec)
// ═══════════════════════════════════════════════════════════════════════════════
//
// ╭ ASCII MOCKUP ──────────────────────────────────────────────────────────╮
// │                                                                       │
// │  ⚡ 142.7 tok/s ▁▂▃▅▆█▇▅▃▂▁▂▃▅▆▇██▆▅ ⬆ 186                        │
// │                                                                       │
// ╰───────────────────────────────────────────────────────────────────────╯
//
// COLOR ANNOTATION:
//   "⚡"              → VIOLET_500  (verb accent)
//   "142.7 tok/s"     → SLATE_200 + BOLD  (primary metric, bright white)
//   sparkline chars   → gradient: CYAN_400 (low) → VIOLET_500 (mid) → ROSE_500 (peak)
//   "⬆ 186"           → SLATE_400  (peak label, dimmer)
//
// WIDTH: 3 (icon+space) + ~12 (rate) + 20 (sparkline) + ~7 (peak) = ~42 chars
//
// ANIMATION:
//   - Sparkline shifts left each sample (new data appended right)
//   - Rate number updates every ~500ms (smoothed)
//   - Peak value pulses BOLD for 3 frames when a new peak is hit

/// Render an F1-style throughput gauge.
///
/// ```text
/// ⚡ 142.7 tok/s ▁▂▃▅▆█▇▅▃▂▁▂▃▅▆▇██▆▅ ⬆ 186
/// ```
pub fn throughput_meter(current_rate: f32, history: &[f32], peak: f32) -> Line<'static> {
    let rate_color = if current_rate > peak * 0.8 {
        compat::ROSE_500
    } else if current_rate > peak * 0.5 {
        compat::VIOLET_500
    } else {
        compat::CYAN_400
    };

    // Build sparkline from history
    let _sparkline: String = if peak > 0.0 {
        history
            .iter()
            .map(|&v| {
                let idx = ((v / peak) * (SPARK.len() - 1) as f32).round() as usize;
                SPARK[idx.min(SPARK.len() - 1)]
            })
            .collect()
    } else {
        "▁".repeat(history.len())
    };

    // Color each sparkline char based on relative height
    let mut spark_spans: Vec<Span<'static>> = Vec::with_capacity(history.len());
    for &v in history {
        let ratio = if peak > 0.0 { v / peak } else { 0.0 };
        let color = if ratio > 0.85 {
            compat::ROSE_500
        } else if ratio > 0.5 {
            compat::VIOLET_500
        } else {
            compat::CYAN_400
        };
        let idx = ((ratio) * (SPARK.len() - 1) as f32).round() as usize;
        let ch = SPARK[idx.min(SPARK.len() - 1)];
        spark_spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
    }

    let mut spans = vec![
        Span::styled("⚡ ", Style::default().fg(compat::VIOLET_500)),
        Span::styled(
            format!("{:.1} tok/s ", current_rate),
            Style::default().fg(rate_color).add_modifier(Modifier::BOLD),
        ),
    ];
    spans.extend(spark_spans);
    spans.push(Span::styled(
        format!(" ⬆ {:.0}", peak),
        Style::default().fg(compat::SLATE_400),
    ));

    Line::from(spans)
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. TIMING WATERFALL (HTTP phases)
// ═══════════════════════════════════════════════════════════════════════════════
//
// ╭ ASCII MOCKUP ──────────────────────────────────────────────────────────╮
// │                                                                       │
// │  DNS━━TLS━━━━━━━TTFB━━━━━━━━━━━━━━━━━━━━TX━━━━━ 847ms               │
// │  12   45        120                      670                          │
// │                                                                       │
// ╰───────────────────────────────────────────────────────────────────────╯
//
// COLOR ANNOTATION:
//   "DNS━━"         → BLUE_400    (resolution phase)
//   "TLS━━━"        → AMBER_400   (handshake phase)
//   "TTFB━━━━━━"    → VIOLET_500  (server processing — usually the longest)
//   "TX━━━"         → GREEN_400   (data transfer)
//   "847ms"         → SLATE_200 + BOLD  (total)
//
// WIDTH: exactly `bar_width` (default 40) + ~8 (total label) = ~48 chars
//
// ANIMATION:
//   - Segments grow left-to-right when first rendered (reveal effect)
//   - Completed phases are static; active phase pulses

/// Render a horizontal HTTP timing waterfall chart.
///
/// ```text
/// DNS━━TLS━━━━━TTFB━━━━━━━━━━━━━TX━━━ 847ms
/// ```
pub fn timing_waterfall(
    dns_ms: u64,
    connect_ms: u64,
    tls_ms: u64,
    ttfb_ms: u64,
    transfer_ms: u64,
    bar_width: usize,
) -> Line<'static> {
    let total = dns_ms + connect_ms + tls_ms + ttfb_ms + transfer_ms;
    if total == 0 {
        return Line::from(vec![
            Span::styled(
                "─".repeat(bar_width),
                Style::default().fg(compat::SLATE_600),
            ),
            Span::styled(" 0ms", Style::default().fg(compat::SLATE_400)),
        ]);
    }

    let phases: &[(u64, &str, Color)] = &[
        (dns_ms, "DNS", compat::BLUE_400),
        (connect_ms, "TCP", compat::TEAL_400),
        (tls_ms, "TLS", compat::AMBER_400),
        (ttfb_ms, "TTFB", compat::VIOLET_500),
        (transfer_ms, "TX", compat::GREEN_400),
    ];

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;

    for &(ms, label, color) in phases {
        if ms == 0 {
            continue;
        }
        let width = ((ms as f64 / total as f64) * bar_width as f64).round() as usize;
        let width = width.max(label.len() + 1).min(bar_width - used);
        if width == 0 {
            continue;
        }

        let bar_chars = width.saturating_sub(label.len());
        let segment = format!("{}{}", label, "━".repeat(bar_chars));
        spans.push(Span::styled(segment, Style::default().fg(color)));
        used += width;
    }

    // Fill remaining space
    if used < bar_width {
        spans.push(Span::styled(
            "━".repeat(bar_width - used),
            Style::default().fg(compat::SLATE_600),
        ));
    }

    // Total label
    let total_str = if total >= 1000 {
        format!(" {:.1}s", total as f64 / 1000.0)
    } else {
        format!(" {}ms", total)
    };
    spans.push(Span::styled(
        total_str,
        Style::default()
            .fg(compat::SLATE_200)
            .add_modifier(Modifier::BOLD),
    ));

    Line::from(spans)
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. TOKEN BUDGET BAR
// ═══════════════════════════════════════════════════════════════════════════════
//
// ╭ ASCII MOCKUP ──────────────────────────────────────────────────────────╮
// │                                                                       │
// │  📊 ████████████▓▓▓▓░░░░░░░░░░░░░░░ 42K/100K (32% cached)           │
// │     ↑ input    ↑ output   ↑ cache                                    │
// │                                                                       │
// ╰───────────────────────────────────────────────────────────────────────╯
//
// COLOR ANNOTATION:
//   overall bar     → GREEN_400 if <50%, AMBER_400 if 50-80%, RED_400 if >80%
//   "████" input    → same color, full opacity
//   "▓▓▓▓" output   → same color, medium opacity (darker shade)
//   "░░░░" cache     → INDIGO_400  (cache reads are "free", distinct color)
//   remaining "─"   → SLATE_700  (empty track)
//   "42K/100K"      → SLATE_200  (metric)
//   "(32% cached)"  → SLATE_400  (secondary)
//
// WIDTH: 2 (icon) + bar_width (default 30) + ~20 (label) = ~52 chars
//
// ANIMATION:
//   - Bar fills left-to-right as tokens are consumed
//   - Color transitions smoothly at 50%/80% thresholds

/// Render a segmented token budget progress bar.
///
/// ```text
/// 📊 ████████▓▓▓▓░░░░──────────── 42K/100K (32% cached)
/// ```
pub fn token_budget_bar(used: u64, total: u64, cache_read: u64, bar_width: usize) -> Line<'static> {
    let ratio = if total > 0 {
        used as f64 / total as f64
    } else {
        0.0
    };

    let (input_color, _output_color) = if ratio > 0.8 {
        (compat::RED_400, compat::RED_600)
    } else if ratio > 0.5 {
        (compat::AMBER_400, compat::AMBER_600)
    } else {
        (compat::GREEN_400, Color::Rgb(22, 163, 74)) // green-600
    };

    // Segment widths: input (used - cache), cache, remaining
    let effective_used = used.saturating_sub(cache_read);
    let total_f = total.max(1) as f64;
    let input_w = ((effective_used as f64 / total_f) * bar_width as f64).round() as usize;
    let cache_w = ((cache_read as f64 / total_f) * bar_width as f64).round() as usize;
    let filled = (input_w + cache_w).min(bar_width);
    let empty = bar_width.saturating_sub(filled);

    let mut spans = vec![Span::styled("📊 ", Style::default().fg(compat::VIOLET_500))];

    // Input segment (solid blocks)
    if input_w > 0 {
        spans.push(Span::styled(
            "█".repeat(input_w),
            Style::default().fg(input_color),
        ));
    }

    // Cache segment (medium shade blocks)
    if cache_w > 0 {
        spans.push(Span::styled(
            "░".repeat(cache_w),
            Style::default().fg(compat::INDIGO_400),
        ));
    }

    // Empty track
    if empty > 0 {
        spans.push(Span::styled(
            "─".repeat(empty),
            Style::default().fg(compat::SLATE_700),
        ));
    }

    // Label
    let used_str = format_count(used);
    let total_str = format_count(total);
    let cache_pct = if used > 0 {
        (cache_read as f64 / used as f64 * 100.0).round() as u64
    } else {
        0
    };

    spans.push(Span::styled(
        format!(" {}/{}", used_str, total_str),
        Style::default().fg(compat::SLATE_200),
    ));

    if cache_read > 0 {
        spans.push(Span::styled(
            format!(" ({}% cached)", cache_pct),
            Style::default().fg(compat::SLATE_400),
        ));
    }

    Line::from(spans)
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. COST ACCUMULATOR
// ═══════════════════════════════════════════════════════════════════════════════
//
// ╭ ASCII MOCKUP ──────────────────────────────────────────────────────────╮
// │                                                                       │
// │  💰 $0.0847 ▲ $0.12/min                                              │
// │  💰 $1.2340 ▼ $0.03/min                                              │
// │  💰 $0.0000 ─ $0.00/min                                              │
// │                                                                       │
// ╰───────────────────────────────────────────────────────────────────────╯
//
// COLOR ANNOTATION:
//   "💰"            → AMBER_400
//   "$0.0847"       → SLATE_200 + BOLD  (when < $1.00)
//                   → AMBER_400 + BOLD  (when $1-$5)
//                   → RED_400 + BOLD    (when > $5)
//   "▲"             → RED_400   (rate increasing = spending more)
//   "▼"             → GREEN_400 (rate decreasing = slowing down)
//   "─"             → SLATE_500 (rate stable)
//   "$0.12/min"     → SLATE_400
//
// WIDTH: 2 (icon) + ~8 (cost) + 3 (arrow) + ~10 (rate) = ~23 chars
//
// ANIMATION:
//   - Cost number ticks up smoothly (not jumpy)
//   - Delta arrow blinks when direction changes

/// Render a cost accumulator with rate indicator.
///
/// ```text
/// 💰 $0.0847 ▲ $0.12/min
/// ```
pub fn cost_accumulator(cost_usd: f64, rate_per_min: f64, prev_rate: Option<f64>) -> Line<'static> {
    let cost_color = if cost_usd > 5.0 {
        compat::RED_400
    } else if cost_usd > 1.0 {
        compat::AMBER_400
    } else {
        compat::SLATE_200
    };

    let (delta_icon, delta_color) = match prev_rate {
        Some(prev) if rate_per_min > prev * 1.1 => ("▲", compat::RED_400),
        Some(prev) if rate_per_min < prev * 0.9 => ("▼", compat::GREEN_400),
        _ => ("─", compat::SLATE_500),
    };

    Line::from(vec![
        Span::styled("💰 ", Style::default().fg(compat::AMBER_400)),
        Span::styled(
            format!("${:.4}", cost_usd),
            Style::default().fg(cost_color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {} ", delta_icon),
            Style::default().fg(delta_color),
        ),
        Span::styled(
            format!("${:.2}/min", rate_per_min),
            Style::default().fg(compat::SLATE_400),
        ),
    ])
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. LATENCY SPARKLINE (per-server)
// ═══════════════════════════════════════════════════════════════════════════════
//
// ╭ ASCII MOCKUP ──────────────────────────────────────────────────────────╮
// │                                                                       │
// │  ▁▂▃▂▁▂▃▅▃▂▁▂▃▂▁▂▃▅▃█ 234ms  p50:120 p99:890                      │
// │                       ↑                                               │
// │                    current (RED because > 2x avg)                     │
// │                                                                       │
// ╰───────────────────────────────────────────────────────────────────────╯
//
// COLOR ANNOTATION:
//   sparkline bars   → CYAN_400 (normal)
//   current (last)   → GREEN_400 if within 2x avg, RED_400 if outlier
//   "234ms"          → same as current dot color
//   "p50:120"        → SLATE_400
//   "p99:890"        → AMBER_400 (warning tier)
//
// WIDTH: 20 (sparkline) + ~7 (current) + ~16 (percentiles) = ~43 chars
//
// ANIMATION:
//   - New data point slides in from the right, older data shifts left
//   - Outlier dot (current > 2x avg) blinks for 3 frames

/// Render a latency sparkline with percentile labels and outlier detection.
///
/// ```text
/// ▁▂▃▂▁▂▃▅▃▂▁▂▃▂▁▂▃▅▃█ 234ms  p50:120 p99:890
/// ```
pub fn latency_sparkline(history: &[u64], current: u64, p50: u64, p99: u64) -> Line<'static> {
    let max_val = history.iter().max().copied().unwrap_or(1).max(1);
    let avg = if history.is_empty() {
        0
    } else {
        history.iter().sum::<u64>() / history.len() as u64
    };
    let is_outlier = avg > 0 && current > avg * 2;

    let current_color = if is_outlier {
        compat::RED_400
    } else {
        compat::GREEN_400
    };

    // Build sparkline spans (all but last in CYAN, last in current_color)
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(history.len() + 4);

    for (i, &v) in history.iter().enumerate() {
        let idx = ((v as f64 / max_val as f64) * (SPARK.len() - 1) as f64).round() as usize;
        let ch = SPARK[idx.min(SPARK.len() - 1)];
        let color = if i == history.len() - 1 {
            current_color
        } else {
            compat::CYAN_400
        };
        spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
    }

    // Current value label
    spans.push(Span::styled(
        format!(" {}ms", current),
        Style::default()
            .fg(current_color)
            .add_modifier(Modifier::BOLD),
    ));

    // Percentile labels
    spans.push(Span::styled(
        "  p50:",
        Style::default().fg(compat::SLATE_500),
    ));
    spans.push(Span::styled(
        format_ms_short(p50),
        Style::default().fg(compat::SLATE_400),
    ));
    spans.push(Span::styled(
        " p99:",
        Style::default().fg(compat::SLATE_500),
    ));
    let p99_color = if p99 > 2000 {
        compat::RED_400
    } else if p99 > 500 {
        compat::AMBER_400
    } else {
        compat::SLATE_400
    };
    spans.push(Span::styled(
        format_ms_short(p99),
        Style::default().fg(p99_color),
    ));

    Line::from(spans)
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. RETRY PROGRESS
// ═══════════════════════════════════════════════════════════════════════════════
//
// ╭ ASCII MOCKUP ──────────────────────────────────────────────────────────╮
// │                                                                       │
// │  🔄 ●━━●━━━━●━━━━━━━━⣾  3/5  [1s → 2s → 4s]                        │
// │     ↑fail  ↑fail      ↑current(spinning)                             │
// │                                                                       │
// │  🔄 ●━━●━━━━✗  2/3 FAILED  timeout                                  │
// │                                                                       │
// ╰───────────────────────────────────────────────────────────────────────╯
//
// COLOR ANNOTATION:
//   "🔄"                → AMBER_400
//   "●" (failed)        → RED_400
//   "━━" (delay bar)    → SLATE_600 (proportional to delay)
//   "⣾" (current)       → CYAN_400 + BOLD (spinning)
//   "3/5"               → SLATE_200
//   "[1s → 2s → 4s]"   → SLATE_400 (backoff visualization)
//   "✗ FAILED"          → RED_400 + BOLD (terminal failure)
//
// WIDTH: 2 (icon) + ~20 (timeline) + ~6 (counter) + ~18 (delays) = ~46 chars
//
// ANIMATION:
//   - Current attempt shows braille spinner
//   - Delay bars grow proportionally to actual backoff times
//   - On failure, last dot turns to ✗ with flash

/// Render a retry progress timeline with backoff visualization.
///
/// ```text
/// 🔄 ●━━●━━━━●━━━━━━━━⣾  3/5  [1s → 2s → 4s]
/// ```
pub fn retry_progress(
    attempt: u32,
    max: u32,
    delays: &[u64],
    errors: &[String],
    frame: usize,
    is_terminal_failure: bool,
) -> Line<'static> {
    let mut spans = vec![Span::styled("🔄 ", Style::default().fg(compat::AMBER_400))];

    let max_delay = delays.iter().max().copied().unwrap_or(1).max(1);

    // Render past attempts as dots with proportional delay bars
    for &delay in delays.iter() {
        // Failed dot
        spans.push(Span::styled("●", Style::default().fg(compat::RED_400)));

        // Delay bar (proportional width, 1-8 chars)
        let bar_w = ((delay as f64 / max_delay as f64) * 6.0).round() as usize + 1;
        spans.push(Span::styled(
            "━".repeat(bar_w),
            Style::default().fg(compat::SLATE_600),
        ));
    }

    // Current attempt
    if is_terminal_failure {
        spans.push(Span::styled(
            "✗",
            Style::default()
                .fg(compat::RED_400)
                .add_modifier(Modifier::BOLD),
        ));
    } else if attempt <= max {
        let spinner_char = SPINNER[frame % SPINNER.len()];
        spans.push(Span::styled(
            spinner_char.to_string(),
            Style::default()
                .fg(compat::CYAN_400)
                .add_modifier(Modifier::BOLD),
        ));
    }

    // Counter
    let counter_color = if is_terminal_failure {
        compat::RED_400
    } else {
        compat::SLATE_200
    };
    spans.push(Span::styled(
        format!("  {}/{}", attempt, max),
        Style::default().fg(counter_color),
    ));

    // Terminal failure label
    if is_terminal_failure {
        spans.push(Span::styled(
            " FAILED",
            Style::default()
                .fg(compat::RED_400)
                .add_modifier(Modifier::BOLD),
        ));
        if let Some(last_err) = errors.last() {
            let err_preview: String = last_err.chars().take(20).collect();
            spans.push(Span::styled(
                format!("  {}", err_preview),
                Style::default().fg(compat::SLATE_400),
            ));
        }
    } else if !delays.is_empty() {
        // Backoff visualization
        let delay_strs: Vec<String> = delays.iter().map(|d| format_ms_short(*d)).collect();
        spans.push(Span::styled(
            format!("  [{}]", delay_strs.join(" → ")),
            Style::default().fg(compat::SLATE_400),
        ));
    }

    Line::from(spans)
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. STRUCTURED OUTPUT LAYERS
// ═══════════════════════════════════════════════════════════════════════════════
//
// ╭ ASCII MOCKUP ──────────────────────────────────────────────────────────╮
// │                                                                       │
// │  ┃ Parse ┃━━━┃ Valid ┃━━━┃ Transform ┃━━━┃ Output ┃                  │
// │  ┃  ✓ 1  ┃   ┃  ✓ 1  ┃   ┃   ✗ 3    ┃   ┃  ── ── ┃                │
// │  ╰green──╯   ╰green──╯   ╰──red─────╯   ╰─gray──╯                  │
// │                                                                       │
// ╰───────────────────────────────────────────────────────────────────────╯
//
// COLOR ANNOTATION:
//   Green box border  → GREEN_400  (succeeded)
//   Red box border    → RED_400    (all attempts failed)
//   Gray box border   → SLATE_500  (skipped / not reached)
//   "✓" success icon  → GREEN_400
//   "✗" failure icon  → RED_400
//   "──" skipped      → SLATE_500
//   "━━━" connectors  → SLATE_600  (pipeline flow)
//   attempt count     → SLATE_300
//
// WIDTH: 4 layers * ~12 chars + 3 connectors * 3 = ~57 chars
//
// ANIMATION:
//   - Layers illuminate left-to-right during processing
//   - Active layer has pulsing border

/// Layer data for structured output pipeline.
pub struct OutputLayer {
    pub name: String,
    pub attempts: u32,
    pub success: bool,
}

/// Render a 4-layer structured output pipeline visualization.
///
/// ```text
/// ┃ Parse ┃━━┃ Valid ┃━━┃ Transform ┃━━┃ Output ┃
/// ┃  ✓ 1  ┃  ┃  ✓ 1  ┃  ┃   ✗ 3    ┃  ┃  ── ── ┃
/// ```
pub fn structured_output_layers(layers: &[OutputLayer]) -> Vec<Line<'static>> {
    let mut name_spans: Vec<Span<'static>> = Vec::new();
    let mut status_spans: Vec<Span<'static>> = Vec::new();

    for (i, layer) in layers.iter().enumerate() {
        let (border_color, status_icon, status_color) = if layer.success {
            (compat::GREEN_400, "✓", compat::GREEN_400)
        } else if layer.attempts > 0 {
            (compat::RED_400, "✗", compat::RED_400)
        } else {
            (compat::SLATE_500, "─", compat::SLATE_500)
        };

        let border_style = Style::default().fg(border_color);
        let pad_name = format!(" {} ", layer.name);
        let pad_width = pad_name.len();

        // Name row: ┃ Parse ┃
        name_spans.push(Span::styled("┃", border_style));
        name_spans.push(Span::styled(pad_name, border_style));
        name_spans.push(Span::styled("┃", border_style));

        // Status row: ┃  ✓ 1  ┃
        let status_text = if layer.attempts > 0 {
            format!("{} {}", status_icon, layer.attempts)
        } else {
            "── ──".to_string()
        };
        // Center the status text in the same width
        let padding = pad_width.saturating_sub(status_text.chars().count());
        let left_pad = padding / 2;
        let right_pad = padding - left_pad;
        let padded_status = format!(
            "{}{}{}",
            " ".repeat(left_pad),
            status_text,
            " ".repeat(right_pad)
        );

        status_spans.push(Span::styled("┃", border_style));
        status_spans.push(Span::styled(
            padded_status,
            Style::default().fg(status_color),
        ));
        status_spans.push(Span::styled("┃", border_style));

        // Connector between layers
        if i < layers.len() - 1 {
            name_spans.push(Span::styled("━━", Style::default().fg(compat::SLATE_600)));
            status_spans.push(Span::styled("  ", Style::default()));
        }
    }

    vec![Line::from(name_spans), Line::from(status_spans)]
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. GUARDRAIL BADGES
// ═══════════════════════════════════════════════════════════════════════════════
//
// ╭ ASCII MOCKUP ──────────────────────────────────────────────────────────╮
// │                                                                       │
// │  ╭──────╮ ╭────────╮ ╭──────────╮ ╭───────╮                         │
// │  │✓ JSON│ │✓ Schema│ │✗ MaxLen  │ │✓ Regex│                         │
// │  ╰──────╯ ╰────────╯ ╰──────────╯ ╰───────╯                         │
// │                                                                       │
// │  COMPACT FORM (preferred for inline):                                 │
// │  ✓JSON ✓Schema ✗MaxLen ✓Regex                                        │
// │                                                                       │
// ╰───────────────────────────────────────────────────────────────────────╯
//
// COLOR ANNOTATION:
//   "✓" + name (passed) → GREEN_400
//   "✗" + name (failed) → RED_400 + BOLD
//   separator "│"       → SLATE_600
//
// WIDTH: variable, ~6-10 chars per badge
//
// ANIMATION: failed badges pulse for 3 frames on first appearance

/// Guardrail badge data.
pub struct GuardrailBadge {
    pub badge_type: String,
    pub name: String,
    pub passed: bool,
}

/// Render inline guardrail badges.
///
/// ```text
/// ✓JSON ✓Schema ✗MaxLen ✓Regex
/// ```
pub fn guardrail_badges(guardrails: &[GuardrailBadge]) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    for (i, g) in guardrails.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" ", Style::default()));
        }

        let (icon, style) = if g.passed {
            ("✓", Style::default().fg(compat::GREEN_400))
        } else {
            (
                "✗",
                Style::default()
                    .fg(compat::RED_400)
                    .add_modifier(Modifier::BOLD),
            )
        };

        spans.push(Span::styled(format!("{}{}", icon, g.name), style));
    }

    Line::from(spans)
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. VISION CONTENT INDICATOR
// ═══════════════════════════════════════════════════════════════════════════════
//
// ╭ ASCII MOCKUP ──────────────────────────────────────────────────────────╮
// │                                                                       │
// │  👁 3 images  2.4 MB  resolved 120ms                                  │
// │  👁 1 image   48 KB   resolved 23ms                                   │
// │                                                                       │
// ╰───────────────────────────────────────────────────────────────────────╯
//
// COLOR ANNOTATION:
//   "👁"              → VIOLET_400
//   "3 images"        → SLATE_200
//   "2.4 MB"          → AMBER_400 if > 1MB, SLATE_300 otherwise
//   "resolved 120ms"  → GREEN_400 if < 200ms, AMBER_400 if < 1s, RED_400 if > 1s
//
// WIDTH: ~2 (icon) + ~10 (count) + ~8 (size) + ~14 (resolve) = ~34 chars
//
// ANIMATION: none (static badge, appears after resolution)

/// Render a compact vision content indicator.
///
/// ```text
/// 👁 3 images  2.4 MB  resolved 120ms
/// ```
pub fn vision_indicator(images: u32, total_bytes: u64, resolve_ms: u64) -> Line<'static> {
    let count_text = if images == 1 {
        "1 image".to_string()
    } else {
        format!("{} images", images)
    };

    let size_text = format_bytes(total_bytes);
    let size_color = if total_bytes > 1_048_576 {
        compat::AMBER_400
    } else {
        compat::SLATE_300
    };

    let resolve_color = if resolve_ms > 1000 {
        compat::RED_400
    } else if resolve_ms > 200 {
        compat::AMBER_400
    } else {
        compat::GREEN_400
    };

    Line::from(vec![
        Span::styled("👁 ", Style::default().fg(compat::VIOLET_400)),
        Span::styled(count_text, Style::default().fg(compat::SLATE_200)),
        Span::styled("  ", Style::default()),
        Span::styled(size_text, Style::default().fg(size_color)),
        Span::styled("  resolved ", Style::default().fg(compat::SLATE_500)),
        Span::styled(
            format_ms_short(resolve_ms),
            Style::default().fg(resolve_color),
        ),
    ])
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. AGENT TURN PROGRESS
// ═══════════════════════════════════════════════════════════════════════════════
//
// ╭ ASCII MOCKUP ──────────────────────────────────────────────────────────╮
// │                                                                       │
// │  ◉ Turn 3/10 ━━━━━━━━━━╌╌╌╌╌╌╌╌╌╌╌╌ 30%  conf: ████░░ 0.72        │
// │                                                                       │
// │  ◉ Turn 8/10 ━━━━━━━━━━━━━━━━━━━╌╌╌╌ 80%  conf: █████░ 0.91        │
// │                                                                       │
// ╰───────────────────────────────────────────────────────────────────────╯
//
// COLOR ANNOTATION:
//   "◉"                   → ROSE_500 (agent verb color)
//   "Turn 3/10"           → SLATE_200 + BOLD
//   "━━━━" (filled)       → ROSE_500 (verb color, intensity varies)
//   "╌╌╌╌" (remaining)    → SLATE_600
//   "30%"                 → SLATE_400
//   "conf:" label         → SLATE_500
//   confidence bar        → GREEN_400 if >0.7, AMBER_400 if >0.4, RED_400 otherwise
//   "0.72"                → same as confidence bar color
//
// WIDTH: 2 (icon) + ~10 (turn) + 20 (bar) + ~5 (pct) + ~16 (conf) = ~53 chars
//
// ANIMATION:
//   - ◉ pulses (alternates with ◎) while running
//   - Progress bar grows on each turn advance

/// Render a circular-style agent turn progress indicator.
///
/// ```text
/// ◉ Turn 3/10 ━━━━━━━━╌╌╌╌╌╌╌╌╌╌╌╌ 30%  conf: ████░░ 0.72
/// ```
pub fn agent_turn_progress(
    current: u32,
    max: u32,
    confidence: f32,
    bar_width: usize,
    frame: u64,
) -> Line<'static> {
    let ratio = if max > 0 {
        (current as f32 / max as f32).min(1.0)
    } else {
        0.0
    };
    let percent = (ratio * 100.0).round() as u32;

    // Pulsing icon
    let icon = if frame % 30 < 15 { "◉" } else { "◎" };

    let filled = (ratio * bar_width as f32).round() as usize;
    let empty = bar_width.saturating_sub(filled);

    // Confidence color
    let conf_color = if confidence > 0.7 {
        compat::GREEN_400
    } else if confidence > 0.4 {
        compat::AMBER_400
    } else {
        compat::RED_400
    };

    // Confidence mini-bar (6 chars wide)
    let conf_filled = (confidence * 6.0).round() as usize;
    let conf_empty = 6usize.saturating_sub(conf_filled);

    Line::from(vec![
        Span::styled(format!("{} ", icon), Style::default().fg(compat::ROSE_500)),
        Span::styled(
            format!("Turn {}/{} ", current, max),
            Style::default()
                .fg(compat::SLATE_200)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("━".repeat(filled), Style::default().fg(compat::ROSE_500)),
        Span::styled("╌".repeat(empty), Style::default().fg(compat::SLATE_600)),
        Span::styled(
            format!(" {}%", percent),
            Style::default().fg(compat::SLATE_400),
        ),
        Span::styled("  conf: ", Style::default().fg(compat::SLATE_500)),
        Span::styled("█".repeat(conf_filled), Style::default().fg(conf_color)),
        Span::styled(
            "░".repeat(conf_empty),
            Style::default().fg(compat::SLATE_600),
        ),
        Span::styled(
            format!(" {:.2}", confidence),
            Style::default().fg(conf_color).add_modifier(Modifier::BOLD),
        ),
    ])
}

// ═══════════════════════════════════════════════════════════════════════════════
// 11. EXTRACT MODE BADGE
// ═══════════════════════════════════════════════════════════════════════════════
//
// ╭ ASCII MOCKUP ──────────────────────────────────────────────────────────╮
// │                                                                       │
// │  ⛏ markdown ─ 2.4K chars extracted                                   │
// │  ⛏ metadata ─ 12 fields (og:title, og:image, ...)                    │
// │  ⛏ article  ─ 847 words, 3 images                                    │
// │  ⛏ links    ─ 42 links (28 internal, 14 external)                    │
// │  ⛏ jsonpath ─ $.data[*].name → 5 results                            │
// │  ⛏ feed     ─ 25 entries (RSS 2.0)                                   │
// │                                                                       │
// ╰───────────────────────────────────────────────────────────────────────╯
//
// COLOR ANNOTATION:
//   "⛏"                   → CYAN_400 (fetch verb accent)
//   mode name              → CYAN_500 + BOLD
//   "─"                    → SLATE_600
//   result summary         → SLATE_300
//
// WIDTH: 2 (icon) + ~10 (mode) + 3 (sep) + ~30 (summary) = ~45 chars
//
// ANIMATION: none (static after extraction completes)

/// Render a compact extract mode badge with result preview.
///
/// ```text
/// ⛏ markdown ─ 2.4K chars extracted
/// ```
pub fn extract_mode_badge(mode: &str, result_summary: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled("⛏ ", Style::default().fg(compat::CYAN_400)),
        Span::styled(
            mode.to_string(),
            Style::default()
                .fg(compat::CYAN_500)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ─ ", Style::default().fg(compat::SLATE_600)),
        Span::styled(
            result_summary.to_string(),
            Style::default().fg(compat::SLATE_300),
        ),
    ])
}

// ═══════════════════════════════════════════════════════════════════════════════
// 12. MCP SERVER STATUS
// ═══════════════════════════════════════════════════════════════════════════════
//
// ╭ ASCII MOCKUP ──────────────────────────────────────────────────────────╮
// │                                                                       │
// │  ● novanet     12ms  47 calls                                         │
// │  ◐ filesystem  89ms   3 calls   (degraded)                            │
// │  ○ slack       ──     0 calls   (offline)                             │
// │                                                                       │
// ╰───────────────────────────────────────────────────────────────────────╯
//
// COLOR ANNOTATION:
//   "●" (connected)    → EMERALD_400  (bright green dot)
//   "◐" (degraded)     → AMBER_400   (half-filled, spinning)
//   "○" (offline)      → RED_400     (empty dot)
//   server name        → SLATE_200   (bright, primary)
//   latency "12ms"     → GREEN_400 if <100ms, AMBER_400 if <500ms, RED_400 if >500ms
//   "47 calls"         → SLATE_400
//   "(degraded)"       → AMBER_400
//   "(offline)"        → RED_400
//
// WIDTH: 2 (dot) + ~14 (name) + ~6 (latency) + ~10 (calls) + ~12 (status) = ~44 chars
//
// ANIMATION:
//   - Degraded dot cycles through ◐◓◑◒ (orbital spinner)
//   - Connected dot pulses brightness subtly
//   - Offline dot is static

/// MCP server connection status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpConnectionStatus {
    Connected,
    Offline,
    Degraded,
}

/// Render an MCP server status indicator.
///
/// ```text
/// ● novanet     12ms  47 calls
/// ◐ filesystem  89ms   3 calls  (degraded)
/// ○ slack       ──     0 calls  (offline)
/// ```
pub fn mcp_server_status(
    name: &str,
    status: McpConnectionStatus,
    latency_ms: u64,
    call_count: u32,
    frame: usize,
) -> Line<'static> {
    let orbital_spinner = ["◐", "◓", "◑", "◒"];

    let (dot, dot_color) = match status {
        McpConnectionStatus::Connected => ("●", compat::EMERALD_400),
        McpConnectionStatus::Degraded => {
            let ch = orbital_spinner[frame % orbital_spinner.len()];
            (ch, compat::AMBER_400)
        }
        McpConnectionStatus::Offline => ("○", compat::RED_400),
    };

    let latency_str = if status == McpConnectionStatus::Offline {
        "──".to_string()
    } else {
        format_ms_short(latency_ms)
    };

    let latency_color = if status == McpConnectionStatus::Offline {
        compat::SLATE_500
    } else if latency_ms > 500 {
        compat::RED_400
    } else if latency_ms > 100 {
        compat::AMBER_400
    } else {
        compat::GREEN_400
    };

    // Pad name to 14 chars for alignment
    let padded_name = format!("{:<14}", name);
    let calls_text = if call_count == 1 {
        "1 call".to_string()
    } else {
        format!("{} calls", call_count)
    };

    let mut spans = vec![
        Span::styled(format!("{} ", dot), Style::default().fg(dot_color)),
        Span::styled(padded_name, Style::default().fg(compat::SLATE_200)),
        Span::styled(
            format!("{:<6}", latency_str),
            Style::default().fg(latency_color),
        ),
        Span::styled(
            format!("  {}", calls_text),
            Style::default().fg(compat::SLATE_400),
        ),
    ];

    // Status suffix for non-connected states
    match status {
        McpConnectionStatus::Degraded => {
            spans.push(Span::styled(
                "  (degraded)",
                Style::default().fg(compat::AMBER_400),
            ));
        }
        McpConnectionStatus::Offline => {
            spans.push(Span::styled(
                "  (offline)",
                Style::default().fg(compat::RED_400),
            ));
        }
        _ => {}
    }

    Line::from(spans)
}

// ═══════════════════════════════════════════════════════════════════════════════
// UTILITY FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════════════

/// Format a count with K/M suffixes.
fn format_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.0}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Format milliseconds in short form.
fn format_ms_short(ms: u64) -> String {
    if ms >= 10_000 {
        format!("{:.0}s", ms as f64 / 1000.0)
    } else if ms >= 1_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}ms", ms)
    }
}

/// Format bytes with appropriate unit.
fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1_024 {
        format!("{:.1} KB", bytes as f64 / 1_024.0)
    } else {
        format!("{} B", bytes)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // --- Utility tests ---

    #[test]
    fn test_format_count() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1_000), "1K");
        assert_eq!(format_count(42_000), "42K");
        assert_eq!(format_count(1_500_000), "1.5M");
    }

    #[test]
    fn test_format_ms_short() {
        assert_eq!(format_ms_short(0), "0ms");
        assert_eq!(format_ms_short(50), "50ms");
        assert_eq!(format_ms_short(999), "999ms");
        assert_eq!(format_ms_short(1_000), "1.0s");
        assert_eq!(format_ms_short(1_500), "1.5s");
        assert_eq!(format_ms_short(10_000), "10s");
        assert_eq!(format_ms_short(65_000), "65s");
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1_024), "1.0 KB");
        assert_eq!(format_bytes(1_536), "1.5 KB");
        assert_eq!(format_bytes(1_048_576), "1.0 MB");
        assert_eq!(format_bytes(1_073_741_824), "1.0 GB");
    }

    // --- 1. Throughput Meter ---

    #[test]
    fn test_throughput_meter_basic() {
        let history = [10.0, 20.0, 30.0, 40.0, 50.0];
        let line = throughput_meter(45.0, &history, 50.0);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("45.0 tok/s"));
        assert!(text.contains("⬆ 50"));
    }

    #[test]
    fn test_throughput_meter_zero_peak() {
        let history = [0.0; 5];
        let line = throughput_meter(0.0, &history, 0.0);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("0.0 tok/s"));
    }

    #[test]
    fn test_throughput_meter_empty_history() {
        let history: [f32; 0] = [];
        let line = throughput_meter(42.0, &history, 42.0);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("42.0 tok/s"));
    }

    #[test]
    fn test_throughput_meter_high_rate_uses_rose_color() {
        let history = [90.0, 95.0, 100.0];
        let line = throughput_meter(95.0, &history, 100.0);
        // 95.0 > 100.0 * 0.8 = 80.0 → should use ROSE_500
        let rate_span = &line.spans[1]; // rate span
        assert_eq!(rate_span.style.fg, Some(compat::ROSE_500));
    }

    // --- 2. Timing Waterfall ---

    #[test]
    fn test_timing_waterfall_basic() {
        let line = timing_waterfall(12, 15, 45, 120, 50, 40);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("DNS"));
        assert!(text.contains("TTFB"));
        assert!(text.contains("TX"));
        assert!(text.contains("242ms"));
    }

    #[test]
    fn test_timing_waterfall_zero_total() {
        let line = timing_waterfall(0, 0, 0, 0, 0, 40);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("0ms"));
    }

    #[test]
    fn test_timing_waterfall_single_phase() {
        let line = timing_waterfall(0, 0, 0, 500, 0, 40);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("TTFB"));
        assert!(text.contains("500ms"));
    }

    #[test]
    fn test_timing_waterfall_large_total_shows_seconds() {
        let line = timing_waterfall(100, 200, 300, 2000, 400, 40);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("3.0s"));
    }

    // --- 3. Token Budget Bar ---

    #[test]
    fn test_token_budget_bar_basic() {
        let line = token_budget_bar(42_000, 100_000, 13_000, 30);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("42K/100K"));
        assert!(text.contains("cached"));
    }

    #[test]
    fn test_token_budget_bar_no_cache() {
        let line = token_budget_bar(5_000, 100_000, 0, 30);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("5K/100K"));
        assert!(!text.contains("cached"));
    }

    #[test]
    fn test_token_budget_bar_green_under_50_percent() {
        let line = token_budget_bar(30_000, 100_000, 0, 30);
        // ratio = 0.3 → should use GREEN_400
        let block_span = &line.spans[1]; // first block segment
        assert_eq!(block_span.style.fg, Some(compat::GREEN_400));
    }

    #[test]
    fn test_token_budget_bar_amber_over_50_percent() {
        let line = token_budget_bar(60_000, 100_000, 0, 30);
        // ratio = 0.6 → should use AMBER_400
        let block_span = &line.spans[1];
        assert_eq!(block_span.style.fg, Some(compat::AMBER_400));
    }

    #[test]
    fn test_token_budget_bar_red_over_80_percent() {
        let line = token_budget_bar(85_000, 100_000, 0, 30);
        // ratio = 0.85 → should use RED_400
        let block_span = &line.spans[1];
        assert_eq!(block_span.style.fg, Some(compat::RED_400));
    }

    #[test]
    fn test_token_budget_bar_zero_total() {
        let line = token_budget_bar(0, 0, 0, 30);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("0/0"));
    }

    // --- 4. Cost Accumulator ---

    #[test]
    fn test_cost_accumulator_basic() {
        let line = cost_accumulator(0.0847, 0.12, None);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("$0.0847"));
        assert!(text.contains("$0.12/min"));
        assert!(text.contains("─")); // stable (no prev_rate)
    }

    #[test]
    fn test_cost_accumulator_increasing_rate() {
        let line = cost_accumulator(0.50, 0.20, Some(0.10));
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("▲")); // rate doubled
    }

    #[test]
    fn test_cost_accumulator_decreasing_rate() {
        let line = cost_accumulator(0.50, 0.05, Some(0.20));
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("▼")); // rate dropped
    }

    #[test]
    fn test_cost_accumulator_high_cost_color() {
        let line = cost_accumulator(6.0, 0.10, None);
        // cost > $5 → RED_400
        let cost_span = &line.spans[1];
        assert_eq!(cost_span.style.fg, Some(compat::RED_400));
    }

    #[test]
    fn test_cost_accumulator_medium_cost_color() {
        let line = cost_accumulator(2.5, 0.10, None);
        // cost > $1 → AMBER_400
        let cost_span = &line.spans[1];
        assert_eq!(cost_span.style.fg, Some(compat::AMBER_400));
    }

    // --- 5. Latency Sparkline ---

    #[test]
    fn test_latency_sparkline_basic() {
        let history = [100, 120, 110, 130, 105, 115, 234];
        let line = latency_sparkline(&history, 234, 120, 890);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("234ms"));
        assert!(text.contains("p50:"));
        assert!(text.contains("p99:"));
    }

    #[test]
    fn test_latency_sparkline_outlier_detection() {
        // Average is ~100, current is 500 → outlier (> 2x avg)
        let history = [90, 100, 110, 95, 105, 500];
        let line = latency_sparkline(&history, 500, 100, 500);
        // The last sparkline char and the current label should be RED
        let current_label = &line.spans[history.len()]; // the "500ms" span
        assert_eq!(current_label.style.fg, Some(compat::RED_400));
    }

    #[test]
    fn test_latency_sparkline_normal_current() {
        let history = [100, 120, 110, 130, 105];
        let line = latency_sparkline(&history, 105, 110, 130);
        // 105 is < 2x avg (~113) → GREEN
        let current_label = &line.spans[history.len()];
        assert_eq!(current_label.style.fg, Some(compat::GREEN_400));
    }

    #[test]
    fn test_latency_sparkline_high_p99_color() {
        let history = [100, 200, 300];
        let line = latency_sparkline(&history, 300, 200, 3000);
        // p99 > 2000 → RED_400
        let last_span = line.spans.last().unwrap();
        assert_eq!(last_span.style.fg, Some(compat::RED_400));
    }

    // --- 6. Retry Progress ---

    #[test]
    fn test_retry_progress_in_progress() {
        let delays = vec![1000, 2000];
        let errors = vec!["timeout".to_string(), "timeout".to_string()];
        let line = retry_progress(3, 5, &delays, &errors, 0, false);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("3/5"));
        assert!(text.contains("●")); // past failures
        assert!(text.contains("[1.0s → 2.0s]")); // backoff viz
    }

    #[test]
    fn test_retry_progress_terminal_failure() {
        let delays = vec![1000, 2000, 4000];
        let errors = vec![
            "timeout".to_string(),
            "timeout".to_string(),
            "connection refused".to_string(),
        ];
        let line = retry_progress(3, 3, &delays, &errors, 0, true);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("✗"));
        assert!(text.contains("FAILED"));
        assert!(text.contains("connection refused"));
    }

    #[test]
    fn test_retry_progress_first_attempt() {
        let line = retry_progress(1, 5, &[], &[], 0, false);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("1/5"));
        // No past failures, no backoff viz
        assert!(!text.contains("●"));
        assert!(!text.contains("["));
    }

    #[test]
    fn test_retry_progress_spinner_rotates() {
        let delays = vec![1000];
        let errors = vec!["err".to_string()];
        let line0 = retry_progress(2, 5, &delays, &errors, 0, false);
        let line4 = retry_progress(2, 5, &delays, &errors, 4, false);
        // Different frames should show different spinner chars
        let text0: String = line0.spans.iter().map(|s| s.content.as_ref()).collect();
        let text4: String = line4.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_ne!(text0, text4);
    }

    // --- 7. Structured Output Layers ---

    #[test]
    fn test_structured_output_layers_basic() {
        let layers = vec![
            OutputLayer {
                name: "Parse".into(),
                attempts: 1,
                success: true,
            },
            OutputLayer {
                name: "Valid".into(),
                attempts: 1,
                success: true,
            },
            OutputLayer {
                name: "Transform".into(),
                attempts: 3,
                success: false,
            },
            OutputLayer {
                name: "Output".into(),
                attempts: 0,
                success: false,
            },
        ];
        let lines = structured_output_layers(&layers);
        assert_eq!(lines.len(), 2); // name row + status row

        let name_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(name_text.contains("Parse"));
        assert!(name_text.contains("Valid"));
        assert!(name_text.contains("Transform"));
        assert!(name_text.contains("Output"));

        let status_text: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(status_text.contains("✓"));
        assert!(status_text.contains("✗"));
        assert!(status_text.contains("──"));
    }

    #[test]
    fn test_structured_output_layers_all_success() {
        let layers = vec![
            OutputLayer {
                name: "A".into(),
                attempts: 1,
                success: true,
            },
            OutputLayer {
                name: "B".into(),
                attempts: 1,
                success: true,
            },
        ];
        let lines = structured_output_layers(&layers);
        let status_text: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
        // All should be green check
        assert!(!status_text.contains("✗"));
        assert!(!status_text.contains("──"));
    }

    // --- 8. Guardrail Badges ---

    #[test]
    fn test_guardrail_badges_basic() {
        let badges = vec![
            GuardrailBadge {
                badge_type: "json".into(),
                name: "JSON".into(),
                passed: true,
            },
            GuardrailBadge {
                badge_type: "schema".into(),
                name: "Schema".into(),
                passed: true,
            },
            GuardrailBadge {
                badge_type: "maxlen".into(),
                name: "MaxLen".into(),
                passed: false,
            },
        ];
        let line = guardrail_badges(&badges);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("✓JSON"));
        assert!(text.contains("✓Schema"));
        assert!(text.contains("✗MaxLen"));
    }

    #[test]
    fn test_guardrail_badges_all_pass() {
        let badges = vec![
            GuardrailBadge {
                badge_type: "a".into(),
                name: "A".into(),
                passed: true,
            },
            GuardrailBadge {
                badge_type: "b".into(),
                name: "B".into(),
                passed: true,
            },
        ];
        let line = guardrail_badges(&badges);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(!text.contains("✗"));
    }

    #[test]
    fn test_guardrail_badges_empty() {
        let line = guardrail_badges(&[]);
        assert!(line.spans.is_empty());
    }

    #[test]
    fn test_guardrail_badges_failed_is_bold() {
        let badges = vec![GuardrailBadge {
            badge_type: "x".into(),
            name: "Fail".into(),
            passed: false,
        }];
        let line = guardrail_badges(&badges);
        let style = line.spans[0].style;
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    // --- 9. Vision Indicator ---

    #[test]
    fn test_vision_indicator_single() {
        let line = vision_indicator(1, 48_000, 23);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("1 image"));
        assert!(text.contains("46.9 KB"));
        assert!(text.contains("23ms"));
    }

    #[test]
    fn test_vision_indicator_multiple() {
        let line = vision_indicator(3, 2_500_000, 120);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("3 images"));
        assert!(text.contains("MB"));
    }

    #[test]
    fn test_vision_indicator_large_size_amber() {
        let line = vision_indicator(1, 2_000_000, 50);
        // > 1MB → AMBER_400
        let size_span = &line.spans[3]; // size text span
        assert_eq!(size_span.style.fg, Some(compat::AMBER_400));
    }

    #[test]
    fn test_vision_indicator_slow_resolve_red() {
        let line = vision_indicator(1, 1000, 2000);
        // > 1000ms → RED_400
        let resolve_span = line.spans.last().unwrap();
        assert_eq!(resolve_span.style.fg, Some(compat::RED_400));
    }

    // --- 10. Agent Turn Progress ---

    #[test]
    fn test_agent_turn_progress_basic() {
        let line = agent_turn_progress(3, 10, 0.72, 20, 0);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("Turn 3/10"));
        assert!(text.contains("30%"));
        assert!(text.contains("0.72"));
    }

    #[test]
    fn test_agent_turn_progress_full() {
        let line = agent_turn_progress(10, 10, 0.95, 20, 0);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("100%"));
    }

    #[test]
    fn test_agent_turn_progress_pulse_icon() {
        let line0 = agent_turn_progress(1, 5, 0.5, 10, 0);
        let line20 = agent_turn_progress(1, 5, 0.5, 10, 20);
        // Frame 0 → "◉", frame 20 → "◎" (20 % 30 = 20, 20 >= 15)
        let icon0: String = line0.spans[0].content.to_string();
        let icon20: String = line20.spans[0].content.to_string();
        assert!(icon0.contains("◉"));
        assert!(icon20.contains("◎"));
    }

    #[test]
    fn test_agent_turn_progress_low_confidence_red() {
        let line = agent_turn_progress(1, 5, 0.2, 10, 0);
        // confidence 0.2 < 0.4 → RED_400
        let conf_value_span = line.spans.last().unwrap();
        assert_eq!(conf_value_span.style.fg, Some(compat::RED_400));
    }

    // --- 11. Extract Mode Badge ---

    #[test]
    fn test_extract_mode_badge_basic() {
        let line = extract_mode_badge("markdown", "2.4K chars extracted");
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("⛏"));
        assert!(text.contains("markdown"));
        assert!(text.contains("2.4K chars extracted"));
    }

    #[test]
    fn test_extract_mode_badge_metadata() {
        let line = extract_mode_badge("metadata", "12 fields (og:title, og:image, ...)");
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("metadata"));
        assert!(text.contains("12 fields"));
    }

    #[test]
    fn test_extract_mode_badge_mode_is_bold() {
        let line = extract_mode_badge("jsonpath", "5 results");
        let mode_span = &line.spans[1]; // mode name span
        assert!(mode_span.style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(mode_span.style.fg, Some(compat::CYAN_500));
    }

    // --- 12. MCP Server Status ---

    #[test]
    fn test_mcp_server_status_connected() {
        let line = mcp_server_status("novanet", McpConnectionStatus::Connected, 12, 47, 0);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("●"));
        assert!(text.contains("novanet"));
        assert!(text.contains("12ms"));
        assert!(text.contains("47 calls"));
        assert!(!text.contains("offline"));
        assert!(!text.contains("degraded"));
    }

    #[test]
    fn test_mcp_server_status_offline() {
        let line = mcp_server_status("slack", McpConnectionStatus::Offline, 0, 0, 0);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("○"));
        assert!(text.contains("slack"));
        assert!(text.contains("──"));
        assert!(text.contains("(offline)"));
    }

    #[test]
    fn test_mcp_server_status_degraded_spins() {
        let line0 = mcp_server_status("fs", McpConnectionStatus::Degraded, 89, 3, 0);
        let line1 = mcp_server_status("fs", McpConnectionStatus::Degraded, 89, 3, 1);
        let text0: String = line0.spans.iter().map(|s| s.content.as_ref()).collect();
        let text1: String = line1.spans.iter().map(|s| s.content.as_ref()).collect();
        // Different frames → different orbital spinner
        assert_ne!(text0, text1);
        assert!(text0.contains("(degraded)"));
    }

    #[test]
    fn test_mcp_server_status_high_latency_red() {
        let line = mcp_server_status("slow", McpConnectionStatus::Connected, 600, 1, 0);
        // latency > 500ms → RED_400
        let latency_span = &line.spans[2]; // latency value span
        assert_eq!(latency_span.style.fg, Some(compat::RED_400));
    }

    #[test]
    fn test_mcp_server_status_single_call_grammar() {
        let line = mcp_server_status("test", McpConnectionStatus::Connected, 10, 1, 0);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("1 call"));
        assert!(!text.contains("1 calls"));
    }
}
