//! Terminal renderers — same recipe, third surface (SVG · Vega-Lite · TTY).
//! Pure functions → String · zero escape codes (color is the CALLER's seam ·
//! cli-presentation-canon) · deterministic.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)] // block-level quantization (f64→cell eighths)
use std::fmt::Write as _;

use crate::fmt;
use crate::spec::Semantic;

const LEVELS: [char; 8] = [
    '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}', '\u{2588}',
];
const PARTIALS: [char; 8] = [
    ' ', '\u{258F}', '\u{258E}', '\u{258D}', '\u{258C}', '\u{258B}', '\u{258A}', '\u{2589}',
];

/// Word-sized sparkline (Tufte) · one char per value · `·` for non-finite.
#[must_use]
pub fn sparkline(values: &[f64]) -> String {
    let finite: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    let Some((lo, hi)) = crate::data::extent(&finite) else {
        return "\u{b7}".repeat(values.len());
    };
    let span = hi - lo;
    values
        .iter()
        .map(|v| {
            if !v.is_finite() {
                '\u{b7}'
            } else if span <= 0.0 {
                '\u{2584}'
            } else {
                let t = (v - lo) / span;
                let idx = ((t * 7.0).round() as usize).min(7);
                *LEVELS.get(idx).unwrap_or(&'\u{2588}')
            }
        })
        .collect()
}

/// Horizontal bars with labels + semantic values · eighth-block precision.
/// `width` = bar column budget in cells.
#[must_use]
pub fn bars(labels: &[String], values: &[f64], sem: Semantic, width: usize) -> String {
    let finite: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    let hi = crate::data::extent(&finite).map_or(0.0, |(_, h)| h);
    let label_w = labels.chars_max();
    let mut out = String::new();
    for (label, v) in labels.iter().zip(values) {
        let pad = " ".repeat(label_w.saturating_sub(label.chars().count()));
        if !v.is_finite() || hi <= 0.0 {
            let _ = writeln!(out, "{pad}{label} \u{2502} \u{b7}");
            continue;
        }
        let eighths = ((v / hi) * (width as f64) * 8.0).round() as usize;
        let full = eighths / 8;
        let part = eighths % 8;
        let mut bar = "\u{2588}".repeat(full);
        if part > 0 {
            bar.push(*PARTIALS.get(part).unwrap_or(&' '));
        }
        let _ = writeln!(out, "{pad}{label} \u{2502}{bar} {}", fmt::label(*v, sem));
    }
    out
}

/// Widest label in chars (helper trait keeps `bars` under the fn cap).
trait CharsMax {
    fn chars_max(&self) -> usize;
}
impl CharsMax for &[String] {
    fn chars_max(&self) -> usize {
        self.iter().map(|l| l.chars().count()).max().unwrap_or(0)
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::unreadable_literal
)]
mod tests {
    use super::*;

    #[test]
    fn sparkline_spans_levels() {
        let s = sparkline(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
        assert_eq!(s.chars().count(), 8);
        assert!(s.starts_with('\u{2581}'));
        assert!(s.ends_with('\u{2588}'));
    }

    #[test]
    fn sparkline_marks_non_finite() {
        let s = sparkline(&[1.0, f64::NAN, 3.0]);
        assert_eq!(s.chars().nth(1), Some('\u{b7}'));
    }

    #[test]
    fn bars_align_and_label() {
        let labels = vec!["fetch".to_owned(), "jq".to_owned()];
        let out = bars(&labels, &[141.0, 6.0], Semantic::DurationMs, 20);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("141ms"));
        assert!(lines[1].starts_with("   jq"));
    }

    #[test]
    fn deterministic() {
        let v = [3.0, 1.0, 4.0, 1.0, 5.0];
        assert_eq!(sparkline(&v), sparkline(&v));
    }
}
