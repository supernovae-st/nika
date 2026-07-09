//! SQ-F: every demo chart round-trips through `verify()` · kitty structural.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    clippy::unreadable_literal,
    clippy::float_cmp,
    clippy::items_after_statements
)]
use nika_chart::verify;
fn main() {
    // Reuse the demo fixtures by regenerating them (same code paths).
    let checks: Vec<(&str, bool)> = {
        let mut v = Vec::new();
        // bar
        use nika_chart::data::{Value, row};
        use nika_chart::spec::{Channel, ChartSpec, ChartType, Semantic};
        let s = |x: &str| Value::Str(x.to_owned());
        let n = Value::Num;
        let rows = vec![
            row(&[("step", s("fetch")), ("ms", n(141.0))]),
            row(&[("step", s("jq")), ("ms", n(6.0))]),
        ];
        let spec = ChartSpec {
            chart: ChartType::Bar,
            title: "v".to_owned(),
            x: Channel::new("step", Semantic::Category),
            y: Channel::new("ms", Semantic::DurationMs),
            y_lo: None,
            y_hi: None,
            y2: None,
            color: None,
            width: 300,
            height: 200,
        };
        let a = nika_chart::compile(&spec, &rows).expect("c");
        v.push(("honest", verify(&spec, &rows, &a.sha256).expect("v")));
        v.push((
            "tampered",
            !verify(&spec, &rows, &a.sha256.replace('a', "b")).expect("v"),
        ));
        v
    };
    for (name, ok) in &checks {
        assert!(*ok, "{name}");
        println!("verify {name} · OK");
    }
    // kitty escape structural on the real chart PNG
    let png = std::fs::read("chart-bar.png").expect("png exists (demo ran)");
    let esc = nika_chart::term_img::kitty(&png);
    let chunks: Vec<&str> = esc.split("\x1b\\").filter(|s| !s.is_empty()).collect();
    assert!(
        chunks[0].starts_with("\x1b_Gf=100,a=T,m=1;"),
        "first chunk keys"
    );
    assert!(
        chunks.last().expect("last").starts_with("\x1b_Gm=0;"),
        "final m=0"
    );
    for c in &chunks[1..chunks.len() - 1] {
        assert!(c.starts_with("\x1b_Gm=1;"), "continuation m=1");
    }
    let payload: usize = chunks
        .iter()
        .map(|c| c.split(';').nth(1).unwrap_or("").len())
        .sum();
    assert_eq!(payload, png.len().div_ceil(3) * 4, "base64 length");
    println!(
        "kitty escape · {} chunks · all m-flags correct · payload length exact",
        chunks.len()
    );
}
