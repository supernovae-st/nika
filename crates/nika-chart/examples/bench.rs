//! Perf harness (§10bis speed row) — release-mode wall-clock per chart type.
//! Clock lives HERE (demo driver), never in the lib.
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

use std::time::Instant;

use nika_chart::data::{Row, Value, row};
use nika_chart::spec::{Channel, ChartSpec, ChartType, Semantic};

fn n(v: f64) -> Value {
    Value::Num(v)
}

fn line_rows(count: usize) -> Vec<Row> {
    (0..count)
        .map(|i| {
            row(&[
                ("x", n(i as f64 + 1.0)),
                ("v", n(((i * 37) % 500) as f64 / 10.0 + 1.0)),
            ])
        })
        .collect()
}

fn bench(name: &str, spec: &ChartSpec, rows: &[Row], iters: u32) {
    // Warmup.
    let a = nika_chart::compile(spec, rows).expect("warmup");
    let t0 = Instant::now();
    for _ in 0..iters {
        let b = nika_chart::compile(spec, rows).expect("bench");
        assert_eq!(b.sha256.len(), 64);
    }
    let per = t0.elapsed().as_micros() / u128::from(iters);
    println!(
        "{name:<28} {per:>6} µs/chart · {} bytes · {} rows",
        a.svg.len(),
        rows.len()
    );
}

fn main() {
    let mk = |chart, y_sem| ChartSpec {
        chart,
        title: "bench".to_owned(),
        x: Channel::new("x", Semantic::Count),
        y: Channel::new("v", y_sem),
        y_lo: None,
        y_hi: None,
        y2: None,
        color: None,
        width: 520,
        height: 300,
    };
    bench(
        "line·30",
        &mk(ChartType::Line, Semantic::Usd),
        &line_rows(30),
        2000,
    );
    bench(
        "line·1000",
        &mk(ChartType::Line, Semantic::Usd),
        &line_rows(1000),
        200,
    );
    bench(
        "scatter·1000",
        &mk(ChartType::Scatter, Semantic::Usd),
        &line_rows(1000),
        200,
    );
    bench(
        "scatter·10000",
        &mk(ChartType::Scatter, Semantic::Usd),
        &line_rows(10_000),
        20,
    );

    // PNG surface (raster + hand-rolled deflate · the full pipeline).
    let spec = mk(ChartType::Line, Semantic::Usd);
    let rows = line_rows(1000);
    let t0 = Instant::now();
    let mut last = 0usize;
    for _ in 0..50 {
        last = nika_chart::render_png::line(&spec, &rows, "deadbeef")
            .expect("png")
            .len();
    }
    println!(
        "{:<28} {:>6} µs/chart · {} bytes · 1000 rows",
        "png-line·1000",
        t0.elapsed().as_micros() / 50,
        last
    );
}
