//! Emit VL for all chart types → validated by the REAL vega-lite compiler.
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

fn base(chart: ChartType, x: Channel, y: Channel) -> ChartSpec {
    ChartSpec {
        chart,
        title: "vl".to_owned(),
        x,
        y,
        y_lo: None,
        y_hi: None,
        y2: None,
        color: None,
        width: 400,
        height: 300,
    }
}

fn main() {
    let cat_rows: Vec<Row> = (0..4)
        .map(|i| {
            row(&[
                ("k", s(&format!("c{i}"))),
                ("v", n(f64::from(i) * 2.0 + 1.0)),
            ])
        })
        .collect();
    let num_rows: Vec<Row> = (1..=8)
        .map(|i| row(&[("x", n(f64::from(i))), ("v", n(f64::from(i * i)))]))
        .collect();
    let multi_rows: Vec<Row> = (1..=6)
        .flat_map(|i| {
            ["a", "b"].map(|p| {
                row(&[
                    ("x", n(f64::from(i))),
                    ("p", s(p)),
                    ("v", n(f64::from(i) + if p == "a" { 4.0 } else { 0.0 })),
                ])
            })
        })
        .collect();
    let hm_rows: Vec<Row> = (0..3)
        .flat_map(|a| {
            (0..3).map(move |b| {
                row(&[
                    ("cx", s(&format!("x{a}"))),
                    ("cy", s(&format!("y{b}"))),
                    ("v", n(f64::from(a * 3 + b) - 4.0)),
                ])
            })
        })
        .collect();
    let band_rows: Vec<Row> = (1..=6)
        .map(|i| {
            let f = f64::from(i);
            row(&[
                ("x", n(f)),
                ("lo", n(f * 10.0)),
                ("hi", n(f * 14.0 + 5.0)),
                ("act", n(f * 11.0)),
            ])
        })
        .collect();

    let mut specs: Vec<(&str, ChartSpec, &Vec<Row>)> = Vec::new();
    specs.push((
        "vl-bar",
        base(
            ChartType::Bar,
            Channel::new("k", Semantic::Category),
            Channel::new("v", Semantic::Usd),
        ),
        &cat_rows,
    ));
    specs.push((
        "vl-line",
        base(
            ChartType::Line,
            Channel::new("x", Semantic::Count),
            Channel::new("v", Semantic::DurationMs),
        ),
        &num_rows,
    ));
    let mut ml = base(
        ChartType::Line,
        Channel::new("x", Semantic::Count),
        Channel::new("v", Semantic::Usd),
    );
    ml.color = Some(Channel::new("p", Semantic::Category));
    specs.push(("vl-multiline", ml, &multi_rows));
    specs.push((
        "vl-scatter",
        base(
            ChartType::Scatter,
            Channel::new("x", Semantic::Count),
            Channel::new("v", Semantic::Tokens),
        ),
        &num_rows,
    ));
    let mut hm = base(
        ChartType::Heatmap,
        Channel::new("cx", Semantic::Category),
        Channel::new("cy", Semantic::Category),
    );
    hm.color = Some(Channel::new("v", Semantic::Delta));
    specs.push(("vl-heatmap", hm, &hm_rows));
    let mut bd = base(
        ChartType::AreaBand,
        Channel::new("x", Semantic::Count),
        Channel::new("lo", Semantic::DurationMs),
    );
    bd.y_lo = Some(Channel::new("lo", Semantic::DurationMs));
    bd.y_hi = Some(Channel::new("hi", Semantic::DurationMs));
    bd.y2 = Some(Channel::new("act", Semantic::DurationMs));
    specs.push(("vl-band", bd, &band_rows));

    for (name, spec, rows) in &specs {
        let a = nika_chart::compile(spec, rows).expect("compile");
        let vl = a.vega_lite.expect("vl emitted");
        std::fs::write(format!("{name}.vl.json"), &vl).expect("write");
        println!("{name}.vl.json · {} bytes", vl.len());
    }
}
