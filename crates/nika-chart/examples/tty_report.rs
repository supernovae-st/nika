//! Eyeball the TTY report (the terminal's vision pass).
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
                cost_usd: 0.0028 + f64::from(i) * 0.00045,
                duration_ms: 2500.0,
            })
            .collect(),
        forecast: vec![ForecastPoint {
            seq: 10.0,
            p50: 2600.0,
            p90: 3510.0,
            actual: 2470.0,
        }],
    };
    print!("{}", nika_chart::report::report_tty(&r).expect("tty"));
}
