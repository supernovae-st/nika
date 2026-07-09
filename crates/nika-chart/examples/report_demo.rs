//! Vision check for the report surface.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    clippy::unreadable_literal,
    clippy::float_cmp
)]
use nika_chart::report::{ForecastPoint, RunReport, RunStat, StepStat};

fn main() {
    let r = RunReport {
        workflow: "site-audit".to_owned(),
        run_id: "2026-07-09T09-44-30Z-3772".to_owned(),
        chain: "a74c131a40c69f9efea79590c4e099a8".to_owned(),
        steps: vec![
            StepStat {
                id: "fetch".into(),
                verb: "invoke".into(),
                duration_ms: 141.0,
                cost_usd: None,
                ok: true,
            },
            StepStat {
                id: "jq".into(),
                verb: "invoke".into(),
                duration_ms: 6.0,
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
                id: "extract".into(),
                verb: "infer".into(),
                duration_ms: 380.0,
                cost_usd: Some(0.0009),
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
        history: (1..=12)
            .map(|i| RunStat {
                seq: f64::from(i),
                cost_usd: 0.0028 + f64::from(i) * 0.00045 + if i % 5 == 0 { 0.001 } else { 0.0 },
                duration_ms: 2500.0,
            })
            .collect(),
        forecast: (1..=10)
            .map(|i| ForecastPoint {
                seq: f64::from(i),
                p50: 2100.0 + f64::from(i) * 50.0,
                p90: 2840.0 + f64::from(i) * 67.0,
                actual: 2230.0 + f64::from((i * 731) % 600) - 200.0,
            })
            .collect(),
    };
    let (html, sha) = nika_chart::report::report_with_hash(&r).expect("report");
    std::fs::write("report.html", &html).expect("write");
    println!("report.html · {} bytes · sha256 {}", html.len(), &sha[..16]);
}
