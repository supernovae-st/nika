//! Golden-BYTE pins (CHT §3bis) — the attestation property as a test.
//! If a toolchain bump or refactor changes ONE byte, this fails loudly.
//! Pinned 2026-07-09 post vision-pass-2 (macOS · pinned via CI matrix at W2).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use nika_chart::data::{Row, Value, row};
use nika_chart::spec::{Channel, ChartSpec, ChartType, Semantic};

fn s(v: &str) -> Value {
    Value::Str(v.to_owned())
}
fn n(v: f64) -> Value {
    Value::Num(v)
}

fn bar_fixture() -> (ChartSpec, Vec<Row>) {
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

#[test]
fn golden_bar_sha256_is_pinned() {
    let (spec, rows) = bar_fixture();
    let a = nika_chart::compile(&spec, &rows).expect("compile");
    assert_eq!(
        a.sha256, "2cc3a7f6d775db460b9dff3271e2ade8d23a7f3067406adf25e67c8f5206df59",
        "byte drift detected — the attestation property broke"
    );
}

#[test]
fn tamper_detection_round_trip() {
    // The « chart lies » detector: re-render from the same recipe must
    // byte-equal; a mutated artifact must NOT match the fresh hash.
    let (spec, rows) = bar_fixture();
    let a = nika_chart::compile(&spec, &rows).expect("compile a");
    let b = nika_chart::compile(&spec, &rows).expect("compile b");
    assert_eq!(a.sha256, b.sha256);

    let tampered = a.svg.replace("2.4s", "1.1s");
    assert_ne!(
        nika_chart::det_hash::sha256_hex(tampered.as_bytes()),
        a.sha256,
        "a tampered chart must not hash-match"
    );
}
