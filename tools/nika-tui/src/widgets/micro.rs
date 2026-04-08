// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! # Micro-Widgets for TaskBox Rendering
//!
//! Small inline visual elements that fit inside a TaskBox (1-2 lines, 20-60 chars wide).
//! Each is a pure function: data in, styled `Line`/`Vec<Span>` out.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::tokens::compat;

// ═══════════════════════════════════════════════════════════════════════════════
// SHARED CONSTANTS
// ═══════════════════════════════════════════════════════════════════════════════

/// Sparkline block characters ordered by fill level (8 levels)
const SPARK: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

// ═══════════════════════════════════════════════════════════════════════════════
// 1. THROUGHPUT METER (tokens/sec)
// ═══════════════════════════════════════════════════════════════════════════════

/// Render an F1-style throughput gauge with sparkline history.
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
// 2. COST ACCUMULATOR
// ═══════════════════════════════════════════════════════════════════════════════

/// Render a cost display with rate trend indicator.
///
/// ```text
/// 💰 $0.0234 ▲ $0.12/min
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_throughput_meter_zero_peak() {
        let line = throughput_meter(0.0, &[], 0.0);
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_throughput_meter_with_history() {
        let history = vec![10.0, 50.0, 80.0, 30.0];
        let line = throughput_meter(50.0, &history, 100.0);
        // Icon + rate + sparkline chars + peak label
        assert!(line.spans.len() >= 6);
    }

    #[test]
    fn test_cost_accumulator_low_cost() {
        let line = cost_accumulator(0.05, 0.01, None);
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_cost_accumulator_trend_up() {
        let line = cost_accumulator(2.0, 0.5, Some(0.3));
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("▲"));
    }

    #[test]
    fn test_cost_accumulator_trend_down() {
        let line = cost_accumulator(0.5, 0.1, Some(0.5));
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("▼"));
    }
}
