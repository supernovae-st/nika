//! Run report · the W3 flagship consumer as a PURE function — trace-shaped
//! data in, one self-contained HTML out (inline SVGs · zero JS · printable ·
//! theme-adaptive via the same prefers-color-scheme seam as the charts).
//!
//! Honesty laws apply to the PAGE, not just the charts: absent cost renders
//! « unpriced », never $0 (cost-intelligence) · absent forecast → the section
//! simply does not exist · every embedded chart's sha256 is printed beside it
//! (the receipts strip) · the report itself is deterministic bytes.
#![allow(clippy::write_with_newline)] // deliberate: LF is part of the byte contract

use std::fmt::Write as _;

use crate::data::{Row, Value};
use crate::error::ChartError;
use crate::spec::{Channel, ChartSpec, ChartType, Semantic};
use crate::{compile, det_hash};

/// One executed step (fold of the trace's task settles).
#[derive(Debug, Clone)]
pub struct StepStat {
    pub id: String,
    pub verb: String,
    pub duration_ms: f64,
    /// `None` = unpriced (absent ≠ $0 · the fold.rs honesty).
    pub cost_usd: Option<f64>,
    pub ok: bool,
}

/// One prior run (the K-window history).
#[derive(Debug, Clone)]
pub struct RunStat {
    pub seq: f64,
    pub cost_usd: f64,
    pub duration_ms: f64,
}

/// Run-level forecast band vs actual (per-run points).
#[derive(Debug, Clone)]
pub struct ForecastPoint {
    pub seq: f64,
    pub p50: f64,
    pub p90: f64,
    pub actual: f64,
}

/// Everything the report needs — the CLI (W3) folds this from traces.
#[derive(Debug, Clone)]
pub struct RunReport {
    pub workflow: String,
    pub run_id: String,
    /// The trace chain head (hex) — the receipt this report extends.
    pub chain: String,
    pub steps: Vec<StepStat>,
    pub history: Vec<RunStat>,
    pub forecast: Vec<ForecastPoint>,
}

fn spec(chart: ChartType, title: &str, x: Channel, y: Channel, w: u32, h: u32) -> ChartSpec {
    ChartSpec {
        chart,
        title: title.to_owned(),
        x,
        y,
        y_lo: None,
        y_hi: None,
        y2: None,
        color: None,
        width: w,
        height: h,
    }
}

/// One chart card: inline SVG + receipt line + the advisories strip
/// (SQ-P: `compile()` computes lint advisories · a report that hides them
/// kills the signal — surfaced here, styled as advice, never as noise).
fn card(html: &mut String, svg: &str, sha: &str, warnings: &[String]) {
    let _ = write!(
        html,
        "<figure class=\"card\">{svg}<figcaption>artifact sha256 <code>{sha}</code></figcaption>"
    );
    for w in warnings {
        let _ = write!(
            html,
            "<p class=\"adv\">\u{26a0} {}</p>",
            crate::svg::escape(w)
        );
    }
    html.push_str("</figure>\n");
}

type ChartCard = (String, String, Vec<String>);

fn steps_chart(r: &RunReport) -> Result<ChartCard, ChartError> {
    let rows: Vec<Row> = r
        .steps
        .iter()
        .map(|s| {
            let mut row = Row::new();
            row.insert("step".to_owned(), Value::Str(s.id.clone()));
            row.insert("ms".to_owned(), Value::Num(s.duration_ms));
            row
        })
        .collect();
    let a = compile(
        &spec(
            ChartType::Bar,
            "Per-step duration",
            Channel::new("step", Semantic::Category),
            Channel::new("ms", Semantic::DurationMs),
            560,
            280,
        ),
        &rows,
    )?;
    Ok((a.svg, a.sha256, a.warnings))
}

fn history_chart(r: &RunReport) -> Result<ChartCard, ChartError> {
    let rows: Vec<Row> = r
        .history
        .iter()
        .map(|s| {
            let mut row = Row::new();
            row.insert("run".to_owned(), Value::Num(s.seq));
            row.insert("usd".to_owned(), Value::Num(s.cost_usd));
            row
        })
        .collect();
    let a = compile(
        &spec(
            ChartType::Line,
            "Cost per run",
            Channel::new("run", Semantic::Count),
            Channel::new("usd", Semantic::Usd),
            560,
            280,
        ),
        &rows,
    )?;
    Ok((a.svg, a.sha256, a.warnings))
}

fn forecast_chart(r: &RunReport) -> Result<ChartCard, ChartError> {
    let rows: Vec<Row> = r
        .forecast
        .iter()
        .map(|p| {
            let mut row = Row::new();
            row.insert("run".to_owned(), Value::Num(p.seq));
            row.insert("p50".to_owned(), Value::Num(p.p50));
            row.insert("p90".to_owned(), Value::Num(p.p90));
            row.insert("actual".to_owned(), Value::Num(p.actual));
            row
        })
        .collect();
    let mut sp = spec(
        ChartType::AreaBand,
        "Duration forecast vs actual",
        Channel::new("run", Semantic::Count),
        Channel::new("p50", Semantic::DurationMs),
        560,
        300,
    );
    sp.y_lo = Some(Channel::new("p50", Semantic::DurationMs));
    sp.y_hi = Some(Channel::new("p90", Semantic::DurationMs));
    sp.y2 = Some(Channel::new("actual", Semantic::DurationMs));
    let a = compile(&sp, &rows)?;
    Ok((a.svg, a.sha256, a.warnings))
}

/// USD cell honesty: absent is « unpriced », NEVER $0.
fn cost_cell(c: Option<f64>) -> String {
    match c {
        Some(v) => crate::fmt::label(v, Semantic::Usd),
        None => "unpriced".to_owned(),
    }
}

const STYLE: &str = "<style>:root{color-scheme:light dark}\
body{font:14px/1.45 -apple-system,'Helvetica Neue',Arial,sans-serif;margin:24px auto;max-width:640px;\
color:#1a1a1a;background:#fff}h1{font-size:19px;margin:0 0 2px}h2{font-size:15px;margin:22px 0 8px}\
.meta{color:#555;font-size:12px}code{font-family:ui-monospace,Menlo,monospace;font-size:11px}\
table{border-collapse:collapse;width:100%;margin:8px 0}th,td{text-align:left;padding:4px 8px;\
border-bottom:1px solid #e6e6e6;font-size:13px}th{color:#555;font-weight:600}\
td.num,th.num{text-align:right;font-variant-numeric:tabular-nums}\
.fail{color:#D55E00;font-weight:600}.adv{color:#8a6d1a;font-size:12px;margin:2px 0}.card{margin:10px 0}.card svg{max-width:100%;height:auto}\
figcaption{color:#555;font-size:11px;margin-top:2px}\
@media(prefers-color-scheme:dark){body{color:#e6e6e6;background:#0f1318}\
.meta,figcaption,th{color:#9aa4ad}th,td{border-color:#262c33}}</style>";

/// The report: deterministic self-contained HTML.
/// # Errors
/// Typed [`crate::error::ChartError`] (`NIKA-BUILTIN-CHART-*`) on invalid spec or data.
pub fn report_html(r: &RunReport) -> Result<String, ChartError> {
    let mut html = String::with_capacity(32 * 1024);
    let _ = write!(
        html,
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>nika report \u{b7} {wf}</title>{STYLE}</head><body>\
         <h1>nika report \u{b7} {wf}</h1>\
         <p class=\"meta\">run <code>{run}</code> \u{b7} chain <code>{chain}</code></p>\n",
        wf = crate::svg::escape(&r.workflow),
        run = crate::svg::escape(&r.run_id),
        chain = crate::svg::escape(&r.chain),
    );

    // 1 · per-step duration (always · a run has steps).
    html.push_str("<h2>Steps</h2>\n");
    let (svg, sha, warns) = steps_chart(r)?;
    card(&mut html, &svg, &sha, &warns);

    // Cost table · unpriced honesty · ✖ on failed steps.
    html.push_str(
        "<table><tr><th>step</th><th>verb</th><th class=\"num\">duration</th>\
         <th class=\"num\">cost</th></tr>\n",
    );
    let mut total_cost = 0.0_f64;
    let mut any_unpriced = false;
    for s in &r.steps {
        match s.cost_usd {
            Some(c) => total_cost += c,
            None => any_unpriced = true,
        }
        let _ = write!(
            html,
            "<tr><td>{}{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>\n",
            if s.ok {
                ""
            } else {
                "<span class=\"fail\">\u{2716}</span> "
            },
            crate::svg::escape(&s.id),
            crate::svg::escape(&s.verb),
            crate::fmt::label(s.duration_ms, Semantic::DurationMs),
            cost_cell(s.cost_usd),
        );
    }
    let total = if any_unpriced {
        format!("\u{2265} {}", crate::fmt::label(total_cost, Semantic::Usd))
    } else {
        crate::fmt::label(total_cost, Semantic::Usd)
    };
    let _ = write!(
        html,
        "<tr><th>total</th><th></th><th></th><th class=\"num\">{total}</th></tr></table>\n"
    );

    // 2 · run-over-run cost (when history exists).
    if r.history.len() >= 2 {
        html.push_str("<h2>Cost over runs</h2>\n");
        let (svg, sha, warns) = history_chart(r)?;
        card(&mut html, &svg, &sha, &warns);
    }

    // 3 · forecast vs actual (only when the forecast exists — never faked).
    if r.forecast.len() >= 2 {
        html.push_str("<h2>Forecast</h2>\n");
        let (svg, sha, warns) = forecast_chart(r)?;
        card(&mut html, &svg, &sha, &warns);
    }

    html.push_str("</body></html>\n");
    Ok(html)
}

/// Report + its own sha256 (the receipt of receipts).
/// # Errors
/// Typed [`crate::error::ChartError`] (`NIKA-BUILTIN-CHART-*`) on invalid spec or data.
pub fn report_with_hash(r: &RunReport) -> Result<(String, String), ChartError> {
    let html = report_html(r)?;
    let sha = det_hash::sha256_hex(html.as_bytes());
    Ok((html, sha))
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

    pub(super) fn sample(forecast: bool) -> RunReport {
        RunReport {
            workflow: "site-audit".to_owned(),
            run_id: "2026-07-09T09-44-30Z-3772".to_owned(),
            chain: "a74c131a40c69f9e".to_owned(),
            steps: vec![
                StepStat {
                    id: "fetch".into(),
                    verb: "invoke".into(),
                    duration_ms: 141.0,
                    cost_usd: None,
                    ok: true,
                },
                StepStat {
                    id: "infer".into(),
                    verb: "infer".into(),
                    duration_ms: 2412.0,
                    cost_usd: Some(0.0031),
                    ok: true,
                },
                StepStat {
                    id: "write".into(),
                    verb: "invoke".into(),
                    duration_ms: 12.0,
                    cost_usd: None,
                    ok: false,
                },
            ],
            history: (1..=8)
                .map(|i| RunStat {
                    seq: f64::from(i),
                    cost_usd: 0.003 + f64::from(i) * 0.0004,
                    duration_ms: 2500.0,
                })
                .collect(),
            forecast: if forecast {
                (1..=8)
                    .map(|i| ForecastPoint {
                        seq: f64::from(i),
                        p50: 2100.0 + f64::from(i) * 50.0,
                        p90: 2800.0 + f64::from(i) * 70.0,
                        actual: 2200.0 + f64::from(i * (i % 3)) * 90.0,
                    })
                    .collect()
            } else {
                Vec::new()
            },
        }
    }

    #[test]
    fn deterministic_bytes() {
        let (a, ha) = report_with_hash(&sample(true)).expect("a");
        let (b, hb) = report_with_hash(&sample(true)).expect("b");
        assert_eq!(a, b);
        assert_eq!(ha, hb);
    }

    #[test]
    fn unpriced_honesty_and_floor_total() {
        let (html, _) = report_with_hash(&sample(true)).expect("html");
        assert!(html.contains("unpriced"));
        assert!(
            html.contains("\u{2265} $0.0031"),
            "total must carry the ≥ floor marker"
        );
        assert!(!html.contains("$0.00<"), "never a fake-zero cell");
    }

    #[test]
    fn forecast_section_never_faked() {
        let (with, _) = report_with_hash(&sample(true)).expect("with");
        let (without, _) = report_with_hash(&sample(false)).expect("without");
        assert!(with.contains("<h2>Forecast</h2>"));
        assert!(!without.contains("<h2>Forecast</h2>"));
    }

    #[test]
    fn failed_step_is_marked() {
        let (html, _) = report_with_hash(&sample(true)).expect("html");
        assert!(html.contains("\u{2716}"));
    }

    #[test]
    fn advisories_surface_in_the_report() {
        let mut r = sample(false);
        r.history = (1..=3000)
            .map(|i| RunStat {
                seq: f64::from(i),
                cost_usd: 0.003 + f64::from(i % 13) * 0.0001,
                duration_ms: 2500.0,
            })
            .collect();
        let (html, _) = report_with_hash(&r).expect("html");
        assert!(html.contains("class=\"adv\""), "advisory strip missing");
        assert!(html.contains("chart-decimated"));
    }

    #[test]
    fn report_embeds_artifacts_verbatim() {
        // The receipts strip prints each artifact's sha256 — which is only
        // honest if the embedded SVG is BYTE-VERBATIM the standalone
        // artifact (no dedupe/minify of embedded charts, ever).
        let r = sample(true);
        let (html, _) = report_with_hash(&r).expect("html");
        let (svg, sha, _) = steps_chart(&r).expect("chart");
        assert!(html.contains(&svg), "embedded steps chart is not verbatim");
        assert!(html.contains(&sha), "its receipt hash is not printed");
    }

    #[test]
    fn charts_carry_receipts() {
        let (html, _) = report_with_hash(&sample(true)).expect("html");
        assert_eq!(html.matches("artifact sha256").count(), 3);
        assert!(!html.contains("<script"), "zero JS by construction");
    }
}

/// The SAME report, TTY projection (SQ-Q · W3's `nika report --tty`) —
/// plain text · zero ANSI (color is the CALLER's seam per house canon) ·
/// the honesty laws travel: unpriced never $0 · ≥ floor · ✖ fails ·
/// advisories surface · forecast section only when data exists.
/// # Errors
/// Typed [`crate::error::ChartError`] (`NIKA-BUILTIN-CHART-*`) on invalid spec or data.
pub fn report_tty(r: &RunReport) -> Result<String, ChartError> {
    use crate::spec::Semantic;
    let mut out = String::with_capacity(2048);
    let _ = writeln!(out, "nika report \u{b7} {}", r.workflow);
    let _ = writeln!(out, "run {} \u{b7} chain {}", r.run_id, r.chain);
    out.push('\n');

    // Steps: eighth-block bars + cost column with the honesty laws.
    let labels: Vec<String> = r
        .steps
        .iter()
        .map(|s| {
            if s.ok {
                s.id.clone()
            } else {
                format!("\u{2716} {}", s.id)
            }
        })
        .collect();
    let durations: Vec<f64> = r.steps.iter().map(|s| s.duration_ms).collect();
    out.push_str(&crate::tty::bars(
        &labels,
        &durations,
        Semantic::DurationMs,
        24,
    ));

    let mut total = 0.0_f64;
    let mut any_unpriced = false;
    for s in &r.steps {
        match s.cost_usd {
            Some(c) => total += c,
            None => any_unpriced = true,
        }
    }
    let floor = if any_unpriced { "\u{2265} " } else { "" };
    let _ = writeln!(
        out,
        "total {floor}{}",
        crate::fmt::label(total, Semantic::Usd)
    );

    // History sparkline (only when it exists).
    if r.history.len() >= 2 {
        let costs: Vec<f64> = r.history.iter().map(|h| h.cost_usd).collect();
        let _ = writeln!(
            out,
            "\ncost \u{b7} last {} runs: {}",
            costs.len(),
            crate::tty::sparkline(&costs)
        );
    }

    // Forecast: last-point verdict only (a band needs pixels · never fake).
    if let Some(p) = r.forecast.last() {
        let inside = p.actual >= p.p50 && p.actual <= p.p90;
        let _ = writeln!(
            out,
            "forecast \u{b7} actual {} vs p50 {} / p90 {} \u{b7} {}",
            crate::fmt::label(p.actual, Semantic::DurationMs),
            crate::fmt::label(p.p50, Semantic::DurationMs),
            crate::fmt::label(p.p90, Semantic::DurationMs),
            if inside {
                "within band"
            } else {
                "OUTSIDE band"
            }
        );
    }

    // Advisories (the SQ-P chain · TTY leg).
    let (_, _, warns) = steps_chart(r)?;
    for w in &warns {
        let _ = writeln!(out, "\u{26a0} {w}");
    }
    Ok(out)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::unreadable_literal
)]
mod tty_tests {
    use super::tests::sample;
    use super::*;

    #[test]
    fn tty_deterministic_and_honest() {
        let a = report_tty(&sample(true)).expect("a");
        let b = report_tty(&sample(true)).expect("b");
        assert_eq!(a, b);
        assert!(a.contains("\u{2265} $0.0031"), "floor marker: {a}");
        assert!(!a.contains("$0.00\n"), "never fake-zero");
        assert!(a.contains("\u{2716} write"), "failed step marked");
        assert!(a.contains("within band") || a.contains("OUTSIDE band"));
    }

    #[test]
    fn tty_forecast_absent_when_empty() {
        let t = report_tty(&sample(false)).expect("t");
        assert!(!t.contains("forecast \u{b7}"));
    }

    #[test]
    fn tty_carries_zero_ansi() {
        let t = report_tty(&sample(true)).expect("t");
        assert!(!t.contains('\u{1b}'), "color is the caller's seam");
    }
}
