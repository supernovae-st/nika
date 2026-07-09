//! Cross-architecture byte-parity probe: prints sha256 of 3 golden renders.
//! Run natively (aarch64) AND under wasm32-wasip1 — lines must be identical.
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

fn s(v: &str) -> Value {
    Value::Str(v.to_owned())
}
fn n(v: f64) -> Value {
    Value::Num(v)
}

fn main() {
    // bar
    let bar_rows: Vec<Row> = [("fetch", 141.0), ("jq", 6.0), ("infer", 2412.0)]
        .iter()
        .map(|(k, v)| row(&[("step", s(k)), ("ms", n(*v))]))
        .collect();
    let bar = ChartSpec {
        chart: ChartType::Bar,
        title: "parity bar \u{b7} $0.0031".to_owned(),
        x: Channel::new("step", Semantic::Category),
        y: Channel::new("ms", Semantic::DurationMs),
        y_lo: None,
        y_hi: None,
        y2: None,
        color: None,
        width: 480,
        height: 300,
    };
    // line (LTTB path + odd magnitudes)
    let line_rows: Vec<Row> = (0..800)
        .map(|i| {
            row(&[
                ("x", n(f64::from(i))),
                ("v", n(f64::from((i * 37) % 977) / 3.7 + 0.001)),
            ])
        })
        .collect();
    let line = ChartSpec {
        chart: ChartType::Line,
        title: "parity line".to_owned(),
        x: Channel::new("x", Semantic::Count),
        y: Channel::new("v", Semantic::Usd),
        y_lo: None,
        y_hi: None,
        y2: None,
        color: None,
        width: 640,
        height: 300,
    };
    // band
    let band_rows: Vec<Row> = (1..=10)
        .map(|i| {
            let f = f64::from(i);
            row(&[
                ("x", n(f)),
                ("lo", n(2000.0 + f * 51.3)),
                ("hi", n(2800.0 + f * 66.7)),
                ("act", n(2200.0 + f64::from((i * 731) % 500))),
            ])
        })
        .collect();
    let mut band = ChartSpec {
        chart: ChartType::AreaBand,
        title: "parity band".to_owned(),
        x: Channel::new("x", Semantic::Count),
        y: Channel::new("lo", Semantic::DurationMs),
        y_lo: None,
        y_hi: None,
        y2: None,
        color: None,
        width: 560,
        height: 320,
    };
    band.y_lo = Some(Channel::new("lo", Semantic::DurationMs));
    band.y_hi = Some(Channel::new("hi", Semantic::DurationMs));
    band.y2 = Some(Channel::new("act", Semantic::DurationMs));

    for (name, spec, rows) in [
        ("bar", &bar, &bar_rows),
        ("line", &line, &line_rows),
        ("band", &band, &band_rows),
    ] {
        let a = nika_chart::compile(spec, rows).expect("compile");
        println!("{name} svg {}", a.sha256);
        // PNG parity too (the raster+deflate pipeline crosses the boundary)
        if name == "bar" {
            let png = nika_chart::render_png::bar(spec, rows, &a.data_sha256[..8]).expect("png");
            println!("{name} png {}", nika_chart::det_hash::sha256_hex(&png));
        }
    }
}
