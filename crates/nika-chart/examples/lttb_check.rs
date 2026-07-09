//! One-off vision check: 1000-point noisy series with spikes through LTTB.
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
use nika_chart::data::{Row, Value, row};
use nika_chart::spec::{Channel, ChartSpec, ChartType, Semantic};

fn main() {
    let rows: Vec<Row> = (0..1000)
        .map(|i| {
            let base = f64::from((i * 37) % 200) / 20.0 + f64::from((i * 7) % 13);
            let spike = match i {
                250 => 60.0,
                700 => 45.0,
                _ => 0.0,
            };
            row(&[
                ("x", Value::Num(f64::from(i))),
                ("v", Value::Num(base + spike + 5.0)),
            ])
        })
        .collect();
    let spec = ChartSpec {
        chart: ChartType::Line,
        title: "LTTB check - 1000 pts, 2 spikes".to_owned(),
        x: Channel::new("x", Semantic::Count),
        y: Channel::new("v", Semantic::Count),
        y_lo: None,
        y_hi: None,
        y2: None,
        color: None,
        width: 700,
        height: 300,
    };
    let a = nika_chart::compile(&spec, &rows).expect("compile");
    std::fs::write("lttb-check.svg", &a.svg).expect("write");
    println!("{} bytes", a.svg.len());
    let html = format!(
        "<!doctype html><body style=\"margin:16px;background:#fff\">{}</body>",
        a.svg
    );
    std::fs::write("lttb-check.html", html).expect("write html");
}
