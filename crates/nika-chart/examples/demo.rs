//! Vision-e2e demo: three run-surface charts from realistic trace-shaped data.
//! Writes SVGs + an index.html for headless-browser screenshot review.
//!
//! Demo harness — panicking on I/O failure IS the correct behavior here;
//! the determinism laws bind `src/`, not this driver.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
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
use nika_chart::{ChartArtifact, compile};

fn s(v: &str) -> Value {
    Value::Str(v.to_owned())
}
fn n(v: f64) -> Value {
    Value::Num(v)
}

fn per_step_duration() -> (ChartSpec, Vec<Row>) {
    let spec = ChartSpec {
        chart: ChartType::Bar,
        title: "Per-step duration · run 2026-07-09T09-44".to_owned(),
        x: Channel::new("step", Semantic::Category),
        y: Channel::new("ms", Semantic::DurationMs),
        y_lo: None,
        y_hi: None,
        y2: None,
        color: None,
        width: 520,
        height: 300,
    };
    let rows = vec![
        row(&[("step", s("fetch")), ("ms", n(141.0))]),
        row(&[("step", s("jq")), ("ms", n(6.0))]),
        row(&[("step", s("infer")), ("ms", n(2412.0))]),
        row(&[("step", s("extract")), ("ms", n(38.0))]),
        row(&[("step", s("write")), ("ms", n(12.0))]),
    ];
    (spec, rows)
}

fn cost_over_runs() -> (ChartSpec, Vec<Row>) {
    let spec = ChartSpec {
        chart: ChartType::Line,
        title: "Cost per run · last 12".to_owned(),
        x: Channel::new("run", Semantic::Count),
        y: Channel::new("usd", Semantic::Usd),
        y_lo: None,
        y_hi: None,
        y2: None,
        color: None,
        width: 520,
        height: 300,
    };
    let costs = [
        0.0031, 0.0029, 0.0044, 0.0038, 0.0052, 0.0047, 0.0061, 0.0058, 0.0072, 0.0069, 0.0081,
        0.0077,
    ];
    let rows = costs
        .iter()
        .enumerate()
        .map(|(i, c)| row(&[("run", n((i + 1) as f64)), ("usd", n(*c))]))
        .collect();
    (spec, rows)
}

fn forecast_band() -> (ChartSpec, Vec<Row>) {
    let spec = ChartSpec {
        chart: ChartType::AreaBand,
        title: "Duration forecast vs actual · task infer".to_owned(),
        x: Channel::new("run", Semantic::Count),
        y: Channel::new("p50", Semantic::DurationMs),
        y_lo: Some(Channel::new("p50", Semantic::DurationMs)),
        y_hi: Some(Channel::new("p90", Semantic::DurationMs)),
        y2: Some(Channel::new("actual", Semantic::DurationMs)),
        color: None,
        width: 520,
        height: 320,
    };
    let p50 = [
        2100.0, 2140.0, 2180.0, 2230.0, 2290.0, 2340.0, 2390.0, 2450.0, 2520.0, 2600.0,
    ];
    let p90 = [
        2840.0, 2890.0, 2950.0, 3010.0, 3090.0, 3160.0, 3230.0, 3310.0, 3400.0, 3510.0,
    ];
    let act = [
        2230.0, 2050.0, 2410.0, 2380.0, 2200.0, 2760.0, 2510.0, 2340.0, 2890.0, 2470.0,
    ];
    let rows = (0..10)
        .map(|i| {
            row(&[
                ("run", n((i + 1) as f64)),
                ("p50", n(p50[i])),
                ("p90", n(p90[i])),
                ("actual", n(act[i])),
            ])
        })
        .collect();
    (spec, rows)
}

fn render(name: &str, spec: &ChartSpec, rows: &[Row]) -> ChartArtifact {
    let a = compile(spec, rows).expect("compile");
    let b = compile(spec, rows).expect("compile twice");
    assert_eq!(a.svg, b.svg, "DETERMINISM VIOLATION on {name}");
    std::fs::write(format!("{name}.svg"), &a.svg).expect("write svg");
    println!(
        "{name}.svg · {} bytes · sha256 {} · data {} · double-render byte-eq OK",
        a.svg.len(),
        &a.sha256[..16],
        &a.data_sha256[..16]
    );
    a
}

fn cost_vs_duration() -> (ChartSpec, Vec<Row>) {
    let spec = ChartSpec {
        chart: ChartType::Scatter,
        title: "Cost vs duration \u{b7} per task \u{b7} last 60".to_owned(),
        x: Channel::new("ms", Semantic::DurationMs),
        y: Channel::new("usd", Semantic::Usd),
        y_lo: None,
        y_hi: None,
        y2: None,
        color: None,
        width: 520,
        height: 300,
    };
    // Deterministic pseudo-cloud: cost roughly tracks duration + outliers.
    let mut rows = Vec::new();
    for i in 0..60u32 {
        let ms = 150.0 + f64::from((i * 731) % 2600);
        let usd = 0.000004 * ms * (1.0 + f64::from((i * 137) % 40) / 100.0)
            + if i % 17 == 0 { 0.004 } else { 0.0 };
        rows.push(row(&[("ms", n(ms)), ("usd", n(usd))]));
    }
    (spec, rows)
}

fn flakiness_matrix() -> (ChartSpec, Vec<Row>) {
    let spec = ChartSpec {
        chart: ChartType::Heatmap,
        title: "Duration delta vs p50 \u{b7} step \u{d7} run".to_owned(),
        x: Channel::new("run", Semantic::Category),
        y: Channel::new("step", Semantic::Category),
        y_lo: None,
        y_hi: None,
        y2: None,
        color: Some(Channel::new("delta_ms", Semantic::Delta)),
        width: 520,
        height: 300,
    };
    let steps = ["fetch", "jq", "infer", "extract", "write"];
    let mut rows = Vec::new();
    for (si, step) in steps.iter().enumerate() {
        for run in 1..=8u32 {
            let seed = ((si as u64 * 8 + u64::from(run)) * 2_654_435_761u64) % 1000;
            let delta = (seed as f64) - 500.0
                + if *step == "infer" && run % 3 == 0 {
                    700.0
                } else {
                    0.0
                };
            rows.push(row(&[
                ("run", s(&format!("r{run}"))),
                ("step", s(step)),
                ("delta_ms", n(delta)),
            ]));
        }
    }
    (spec, rows)
}
fn cost_per_provider() -> (ChartSpec, Vec<Row>) {
    let spec = ChartSpec {
        chart: ChartType::Line,
        title: "Cost per run \u{b7} by provider".to_owned(),
        x: Channel::new("run", Semantic::Count),
        y: Channel::new("usd", Semantic::Usd),
        y_lo: None,
        y_hi: None,
        y2: None,
        color: Some(Channel::new("provider", Semantic::Category)),
        width: 520,
        height: 320,
    };
    let mut rows = Vec::new();
    for i in 1..=12u32 {
        let x = f64::from(i);
        rows.push(row(&[
            ("run", n(x)),
            ("provider", s("anthropic")),
            ("usd", n(0.0041 + x * 0.0003)),
        ]));
        rows.push(row(&[
            ("run", n(x)),
            ("provider", s("openai")),
            (
                "usd",
                n(0.0035 + x * 0.00022 + if i % 4 == 0 { 0.0012 } else { 0.0 }),
            ),
        ]));
        rows.push(row(&[
            ("run", n(x)),
            ("provider", s("ollama")),
            ("usd", n(0.0002)),
        ]));
    }
    (spec, rows)
}

fn main() {
    let (bs, br) = per_step_duration();
    let a1 = render("chart-bar", &bs, &br);
    let (ls, lr) = cost_over_runs();
    let a2 = render("chart-line", &ls, &lr);
    let (fs_, fr) = forecast_band();
    let a3 = render("chart-band", &fs_, &fr);
    let (ss, sr) = cost_vs_duration();
    let a4 = render("chart-scatter", &ss, &sr);
    let (hs, hr) = flakiness_matrix();
    let a5 = render("chart-heatmap", &hs, &hr);
    let (ms, mr) = cost_per_provider();
    let a6 = render("chart-multiline", &ms, &mr);
    assert!(a1.vega_lite.is_some() && a5.vega_lite.is_some());

    // 4th surface: deterministic PNG (raster projection of the same recipe).
    let png = nika_chart::render_png::bar(&bs, &br, &a1.data_sha256[..8]).expect("png render");
    let png2 = nika_chart::render_png::bar(&bs, &br, &a1.data_sha256[..8]).expect("png render 2");
    assert_eq!(png, png2, "PNG DETERMINISM VIOLATION");
    std::fs::write("chart-bar.png", &png).expect("write png");
    println!(
        "chart-bar.png · {} bytes · sha256 {} · double-encode byte-eq OK",
        png.len(),
        &nika_chart::det_hash::sha256_hex(&png)[..16]
    );
    std::fs::write("chart-bar.kitty.txt", nika_chart::term_img::kitty(&png))
        .expect("write kitty escape");
    std::fs::write("chart-band.vl.json", a3.vega_lite.as_deref().unwrap_or("")).expect("write vl");
    println!("chart-band.vl.json written (Vega-Lite compile target)");

    // TTY surface — same data, third renderer.
    let labels: Vec<String> = ["fetch", "jq", "infer", "extract", "write"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    println!("\nTTY bars (per-step duration):");
    print!(
        "{}",
        nika_chart::tty::bars(
            &labels,
            &[141.0, 6.0, 2412.0, 38.0, 12.0],
            Semantic::DurationMs,
            28
        )
    );
    let costs = [
        0.0031, 0.0029, 0.0044, 0.0038, 0.0052, 0.0047, 0.0061, 0.0058, 0.0072, 0.0069, 0.0081,
        0.0077,
    ];
    println!(
        "cost sparkline \u{b7} last 12 runs: {}",
        nika_chart::tty::sparkline(&costs)
    );

    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><style>body{{margin:0;background:#f4f4f4;\
         display:flex;flex-wrap:wrap;gap:16px;padding:16px}}div{{box-shadow:0 1px 4px \
         rgba(0,0,0,.15)}}</style></head><body><div>{}</div><div>{}</div><div>{}</div><div>{}</div><div>{}</div><div>{}</div></body></html>",
        a1.svg, a2.svg, a3.svg, a4.svg, a5.svg, a6.svg
    );
    std::fs::write("index.html", html).expect("write html");
    println!("index.html written · open for vision pass");
}
