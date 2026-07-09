//! Vision: the 3 report charts on the PNG surface (kitty-ready).
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

fn n(v: f64) -> Value {
    Value::Num(v)
}

fn main() {
    // line
    let rows: Vec<Row> = (1..=12)
        .map(|i| {
            row(&[
                ("run", n(f64::from(i))),
                ("usd", n(0.003 + f64::from(i) * 0.0004)),
            ])
        })
        .collect();
    let spec = ChartSpec {
        chart: ChartType::Line,
        title: "Cost per run - last 12".to_owned(),
        x: Channel::new("run", Semantic::Count),
        y: Channel::new("usd", Semantic::Usd),
        y_lo: None,
        y_hi: None,
        y2: None,
        color: None,
        width: 520,
        height: 300,
    };
    let a = nika_chart::compile(&spec, &rows).expect("c");
    let png = nika_chart::render_png::line(&spec, &rows, &a.data_sha256[..8]).expect("png");
    std::fs::write("png-line.png", &png).expect("w");
    println!("png-line.png · {} bytes", png.len());
    // band
    let rows: Vec<Row> = (1..=10)
        .map(|i| {
            let f = f64::from(i);
            row(&[
                ("run", n(f)),
                ("p50", n(2100.0 + f * 50.0)),
                ("p90", n(2840.0 + f * 67.0)),
                ("act", n(2200.0 + f64::from((i * 731) % 600) - 200.0)),
            ])
        })
        .collect();
    let mut spec = ChartSpec {
        chart: ChartType::AreaBand,
        title: "Forecast vs actual".to_owned(),
        x: Channel::new("run", Semantic::Count),
        y: Channel::new("p50", Semantic::DurationMs),
        y_lo: None,
        y_hi: None,
        y2: None,
        color: None,
        width: 520,
        height: 300,
    };
    spec.y_lo = Some(Channel::new("p50", Semantic::DurationMs));
    spec.y_hi = Some(Channel::new("p90", Semantic::DurationMs));
    spec.y2 = Some(Channel::new("act", Semantic::DurationMs));
    let a = nika_chart::compile(&spec, &rows).expect("c");
    let png = nika_chart::render_png::area_band(&spec, &rows, &a.data_sha256[..8]).expect("png");
    let png2 = nika_chart::render_png::area_band(&spec, &rows, &a.data_sha256[..8]).expect("png2");
    assert_eq!(png, png2, "band png determinism");
    std::fs::write("png-band.png", &png).expect("w");
    println!(
        "png-band.png · {} bytes · double-encode byte-eq OK",
        png.len()
    );
    // scatter
    let rows: Vec<Row> = (0..60)
        .map(|i| {
            let ms = 150.0 + f64::from((i * 731) % 2600);
            row(&[
                ("ms", n(ms)),
                (
                    "usd",
                    n(0.000004 * ms * (1.0 + f64::from((i * 137) % 40) / 100.0)),
                ),
            ])
        })
        .collect();
    let spec = ChartSpec {
        chart: ChartType::Scatter,
        title: "Cost vs duration".to_owned(),
        x: Channel::new("ms", Semantic::DurationMs),
        y: Channel::new("usd", Semantic::Usd),
        y_lo: None,
        y_hi: None,
        y2: None,
        color: None,
        width: 520,
        height: 300,
    };
    let a = nika_chart::compile(&spec, &rows).expect("c");
    let png = nika_chart::render_png::scatter(&spec, &rows, &a.data_sha256[..8]).expect("png");
    std::fs::write("png-scatter.png", &png).expect("w");
    println!("png-scatter.png · {} bytes", png.len());
    // heatmap
    let s2 = |v: &str| Value::Str(v.to_owned());
    let steps = ["fetch", "jq", "infer", "extract", "write"];
    let mut hrows: Vec<Row> = Vec::new();
    for (si, step) in steps.iter().enumerate() {
        for run in 1..=8u32 {
            let seed = ((si as u64 * 8 + u64::from(run)) * 2_654_435_761u64) % 1000;
            hrows.push(row(&[
                ("run", s2(&format!("r{run}"))),
                ("step", s2(step)),
                ("d", n((seed as f64) - 500.0)),
            ]));
        }
    }
    let mut spec = ChartSpec {
        chart: ChartType::Heatmap,
        title: "Delta vs p50".to_owned(),
        x: Channel::new("run", Semantic::Category),
        y: Channel::new("step", Semantic::Category),
        y_lo: None,
        y_hi: None,
        y2: None,
        color: None,
        width: 520,
        height: 300,
    };
    spec.color = Some(Channel::new("d", Semantic::Delta));
    let a = nika_chart::compile(&spec, &hrows).expect("c");
    let png = nika_chart::render_png::heatmap(&spec, &hrows, &a.data_sha256[..8]).expect("png");
    let png2 = nika_chart::render_png::heatmap(&spec, &hrows, &a.data_sha256[..8]).expect("png");
    assert_eq!(png, png2, "heatmap png determinism");
    std::fs::write("png-heatmap.png", &png).expect("w");
    println!(
        "png-heatmap.png · {} bytes · double-encode byte-eq OK",
        png.len()
    );
}
