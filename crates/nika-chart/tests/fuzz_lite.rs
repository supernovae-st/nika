//! Fuzz-lite — zero-dep proptest stand-in. A CONST-seeded LCG (deterministic:
//! the same 200 cases forever, no clock/rand) sweeps weird-but-plausible
//! specs; every case must either compile cleanly (then double-render
//! byte-eq) or fail with a TYPED error. Panics = the only real failure.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use nika_chart::data::{Row, Value, row};
use nika_chart::spec::{Channel, ChartSpec, ChartType, Semantic};

/// Hostile-title pool — stresses the FULL escape chain
/// (`json_escape` → xml escape → xml unescape → json unescape).
const TITLES: [&str; 6] = [
    "plain fuzz case",
    "he said \"hi\" \\ done",
    "newline\ntab\ttitle",
    "unicode \u{b7} \u{2190} \u{1F98B} \u{a7}",
    "<xml>&amp;</xml> &quot;raw&quot;",
    "ctrl\u{7}bell",
];

/// Knuth MMIX LCG · fixed seed ⇒ reproducible corpus.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }

    fn pick(&mut self, n: u64) -> u64 {
        (self.next() >> 33) % n.max(1)
    }

    fn value(&mut self) -> f64 {
        // Magnitudes 1e-6..1e6 · ~20% negatives · occasional zero.
        let mag = [1e-6, 1e-3, 1.0, 1e3, 1e6][(self.pick(5)) as usize];
        let base = (self.pick(10_000) as f64) / 100.0;
        let sign = if self.pick(5) == 0 { -1.0 } else { 1.0 };
        if self.pick(23) == 0 {
            0.0
        } else {
            sign * base * mag
        }
    }
}

const SEMANTICS: [Semantic; 6] = [
    Semantic::Usd,
    Semantic::DurationMs,
    Semantic::Tokens,
    Semantic::Count,
    Semantic::Delta,
    Semantic::Percent,
];

fn build_case(r: &mut Lcg, case: u64) -> (ChartSpec, Vec<Row>) {
    let kind = match r.pick(5) {
        0 => ChartType::Bar,
        1 => ChartType::Line,
        2 => ChartType::AreaBand,
        3 => ChartType::Scatter,
        _ => ChartType::Heatmap,
    };
    let sem = SEMANTICS[r.pick(6) as usize];
    let n = 1 + r.pick(40) as usize;
    let mut rows = Vec::with_capacity(n);
    for i in 0..n {
        let mut pairs: Vec<(&str, Value)> = Vec::new();
        match kind {
            ChartType::Bar => {
                pairs.push(("cat", Value::Str(format!("c{}", r.pick(12)))));
                pairs.push(("v", Value::Num(r.value())));
            }
            ChartType::Line | ChartType::Scatter => {
                pairs.push(("x", Value::Num(i as f64 + 1.0)));
                pairs.push(("v", Value::Num(r.value())));
            }
            ChartType::AreaBand => {
                let lo = r.value().abs();
                pairs.push(("x", Value::Num(i as f64 + 1.0)));
                pairs.push(("lo", Value::Num(lo)));
                pairs.push(("hi", Value::Num(lo * 1.4 + 1.0)));
                pairs.push(("act", Value::Num(lo * 1.1)));
            }
            ChartType::Heatmap => {
                pairs.push(("cx", Value::Str(format!("x{}", r.pick(6)))));
                pairs.push(("cy", Value::Str(format!("y{}", r.pick(5)))));
                pairs.push(("v", Value::Num(r.value())));
            }
            _ => unreachable!("closed corpus"),
        }
        rows.push(row(&pairs));
    }
    let (xf, xsem) = match kind {
        ChartType::Bar => ("cat", Semantic::Category),
        ChartType::Heatmap => ("cx", Semantic::Category),
        _ => ("x", Semantic::Count),
    };
    let spec = ChartSpec {
        chart: kind,
        title: format!("{} {case}", TITLES[(case % 6) as usize]),
        x: Channel::new(xf, xsem),
        y: match kind {
            ChartType::AreaBand => Channel::new("lo", sem),
            ChartType::Heatmap => Channel::new("cy", Semantic::Category),
            _ => Channel::new("v", sem),
        },
        y_lo: (kind == ChartType::AreaBand).then(|| Channel::new("lo", sem)),
        y_hi: (kind == ChartType::AreaBand).then(|| Channel::new("hi", sem)),
        y2: (kind == ChartType::AreaBand).then(|| Channel::new("act", sem)),
        color: (kind == ChartType::Heatmap).then(|| Channel::new("v", sem)),
        width: 200 + (r.pick(500) as u32),
        height: 120 + (r.pick(400) as u32),
    };
    (spec, rows)
}

#[test]
fn two_hundred_cases_never_panic_and_stay_deterministic() {
    let mut rng = Lcg(0x5EED_2026_0709);
    let mut ok = 0;
    let mut typed_err = 0;
    for case in 0..200 {
        let (spec, rows) = build_case(&mut rng, case);
        match nika_chart::compile(&spec, &rows) {
            Ok(a) => {
                let b = nika_chart::compile(&spec, &rows).expect("second render");
                assert_eq!(a.svg, b.svg, "determinism broke on case {case}");
                assert!(a.svg.contains("<metadata>"));
                // The attestation property, corpus-wide: every artifact
                // must verify from ITSELF (manifest → spec → re-render).
                assert_eq!(
                    nika_chart::attest::verify_artifact(&a.svg, &rows),
                    nika_chart::attest::Verdict::Match,
                    "attest round-trip broke on case {case} (title: {:?})",
                    spec.title
                );
                ok += 1;
            }
            Err(e) => {
                // Typed by construction — exercise Display.
                assert!(format!("{e}").starts_with("NIKA-BUILTIN-CHART-"));
                typed_err += 1;
            }
        }
    }
    assert_eq!(ok + typed_err, 200);
    assert!(ok >= 150, "corpus should mostly render (got {ok})");
}
