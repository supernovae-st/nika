//! Advisory lints (master plan §3ter · the crate-computable subset).
//! ADVISORY, not receipts: they ride `ChartArtifact.warnings`, never the
//! manifest — the engine surfaces them as `nika check`/run hints.
//!
//! Most §3ter rules are ENFORCED BY CONSTRUCTION here (bars always anchor
//! zero · no rainbow ramps exist · delta ⇒ diverging is automatic). What
//! remains lintable is what the author can overload: category count ·
//! point density · aspect ratio.

use crate::data::{self, Row};
use crate::spec::{ChartSpec, ChartType};

/// Okabe-Ito distinct-hue budget (palette cycles past 7).
const CATEGORY_BUDGET: usize = 7;

fn uniq_count(vals: &[String]) -> usize {
    let mut seen = std::collections::BTreeSet::new();
    for v in vals {
        seen.insert(v.as_str());
    }
    seen.len()
}

/// Median |slope| in PLOT space ≈ 1.0 is Cleveland's 45° target — far off
/// means the aspect fights slope perception (Cleveland-McGill 1988 ·
/// Talbot-Gerth-Hanrahan 2011). Advisory only, never a resize.
fn banking_ratio(xs: &[f64], ys: &[f64], spec: &ChartSpec) -> Option<f64> {
    let (xlo, xhi) = data::extent(xs)?;
    let (ylo, yhi) = data::extent(ys)?;
    if xhi - xlo <= 0.0 || yhi - ylo <= 0.0 || xs.len() < 3 {
        return None;
    }
    let sx = f64::from(spec.width) / (xhi - xlo);
    let sy = f64::from(spec.height) / (yhi - ylo);
    let mut slopes: Vec<f64> = xs
        .windows(2)
        .zip(ys.windows(2))
        .filter_map(|(xw, yw)| {
            let dx = (xw.get(1)? - xw.first()?) * sx;
            let dy = (yw.get(1)? - yw.first()?) * sy;
            (dx != 0.0).then(|| (dy / dx).abs())
        })
        .filter(|s| s.is_finite() && *s > 0.0)
        .collect();
    if slopes.is_empty() {
        return None;
    }
    slopes.sort_by(f64::total_cmp);
    slopes.get(slopes.len() / 2).copied()
}

/// Compute the advisory list (deterministic order · stable strings).
#[must_use]
pub fn advisories(spec: &ChartSpec, rows: &[Row]) -> Vec<String> {
    let mut out = Vec::new();

    // chart-categorical-color-budget (§3ter · Okabe-Ito ceiling).
    if let Some(ch) = &spec.color
        && spec.chart != ChartType::Heatmap
        && let Ok(cats) = data::string_column(rows, &ch.field)
    {
        let n = uniq_count(&cats);
        if n > CATEGORY_BUDGET {
            out.push(format!(
                "chart-categorical-color-budget · {n} series exceed the {CATEGORY_BUDGET}-hue \
                         CVD-safe palette (colors will cycle) — consider faceting or aggregating"
            ));
        }
    }

    // decimation notice (LTTB fires above 2×plot-width points · line only).
    if spec.chart == ChartType::Line {
        let budget = (spec.width as usize).saturating_sub(60).max(64) * 2;
        if rows.len() > budget {
            out.push(format!(
                "chart-decimated · {} points exceed the pixel budget — LTTB keeps extremes \
                 (Steinarsson 2013) · the DATA hash still covers every row",
                rows.len()
            ));
        }
    }

    // chart-aspect-banking (§3ter · warn-only · Cleveland 45°).
    if spec.chart == ChartType::Line
        && spec.color.is_none()
        && let (Ok(xs), Ok(ys)) = (
            data::numeric_column(rows, &spec.x.field),
            data::numeric_column(rows, &spec.y.field),
        )
        && let Some(m) = banking_ratio(&xs, &ys, spec)
        && !(0.15..=6.0).contains(&m)
    {
        out.push(format!(
            "chart-aspect-banking · median |slope| ≈ {m:.1} in plot space (45° target ≈ 1) \
                         — a {} canvas would read better (Cleveland 1988)",
            if m > 6.0 { "wider" } else { "taller" }
        ));
    }

    out
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
    use crate::data::{Value, row};
    use crate::spec::{Channel, Semantic};

    fn line_spec(color: bool) -> ChartSpec {
        ChartSpec {
            chart: ChartType::Line,
            title: "t".to_owned(),
            x: Channel::new("x", Semantic::Count),
            y: Channel::new("v", Semantic::Count),
            y_lo: None,
            y_hi: None,
            y2: None,
            color: color.then(|| Channel::new("p", Semantic::Category)),
            width: 400,
            height: 300,
        }
    }

    #[test]
    fn category_budget_fires_past_seven() {
        let rows: Vec<Row> = (0..9)
            .map(|i| {
                row(&[
                    ("x", Value::Num(f64::from(i))),
                    ("v", Value::Num(1.0)),
                    ("p", Value::Str(format!("s{i}"))),
                ])
            })
            .collect();
        let w = advisories(&line_spec(true), &rows);
        assert!(
            w.iter()
                .any(|s| s.starts_with("chart-categorical-color-budget"))
        );
    }

    #[test]
    fn quiet_on_healthy_charts() {
        let rows: Vec<Row> = (0..20)
            .map(|i| {
                row(&[
                    ("x", Value::Num(f64::from(i))),
                    ("v", Value::Num(f64::from(i * 2))),
                ])
            })
            .collect();
        assert!(advisories(&line_spec(false), &rows).is_empty());
    }

    #[test]
    fn decimation_notice_fires() {
        let rows: Vec<Row> = (0..2000)
            .map(|i| {
                row(&[
                    ("x", Value::Num(f64::from(i))),
                    ("v", Value::Num(f64::from(i % 7))),
                ])
            })
            .collect();
        let w = advisories(&line_spec(false), &rows);
        assert!(w.iter().any(|s| s.starts_with("chart-decimated")));
    }

    #[test]
    fn banking_fires_on_pathological_aspect() {
        // Slopes ~30× in plot space: y swings the full range every x step.
        let rows: Vec<Row> = (0..30)
            .map(|i| {
                row(&[
                    ("x", Value::Num(f64::from(i))),
                    ("v", Value::Num(if i % 2 == 0 { 0.0 } else { 100.0 })),
                ])
            })
            .collect();
        let w = advisories(&line_spec(false), &rows);
        assert!(
            w.iter().any(|s| s.starts_with("chart-aspect-banking")),
            "{w:?}"
        );
    }

    #[test]
    fn deterministic_order() {
        let rows: Vec<Row> = (0..2000)
            .map(|i| {
                row(&[
                    ("x", Value::Num(f64::from(i))),
                    ("v", Value::Num(if i % 2 == 0 { 0.0 } else { 100.0 })),
                ])
            })
            .collect();
        assert_eq!(
            advisories(&line_spec(false), &rows),
            advisories(&line_spec(false), &rows)
        );
    }
}
