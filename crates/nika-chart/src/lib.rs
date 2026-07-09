//! # nika-chart
//!
//! Deterministic chart compiler+renderer: data + semantic spec → byte-stable
//! SVG whose sha256 can join a workflow trace chain (« your workflow draws
//! its own receipts » · master plan CHT §2). ZERO dependencies (operator lock
//! 2026-07-09) · pure L1 library · no I/O, no clock, no rand.
//!
//! Determinism contract: identical `(spec, rows)` ⇒ identical bytes, across
//! platforms and re-runs. Enforced by construction (fixed-decimal integer-math
//! coordinates · POW10 table, no transcendentals · BTreeMap/Vec only · fixed
//! attribute order) and by tests (double-render byte-eq · golden hashes).

pub mod attest;
pub mod data;
pub mod decimate;
pub mod det_hash;
pub mod error;
pub mod fmt;
pub mod font8;
pub mod layout;
pub mod lint;
pub mod metrics;
pub mod palette;
pub mod png;
pub mod raster;
pub mod render;
pub mod render_png;
pub mod report;
pub mod scale;
pub mod spec;
pub mod svg;
pub mod term_img;
pub mod ticks;
pub mod tty;
pub mod vega;

use data::Row;
use error::ChartError;
use spec::{ChartSpec, ChartType};

/// Reverse of `svg::escape` for the `<metadata>` block (attest read-back).
/// `&amp;` decodes LAST — the classic double-decode trap.
pub(crate) fn svg_unescape_metadata(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Soft cap on rows (CHT §11bis SQ4 · decimation lands v1.5).
pub const MAX_ROWS: usize = 10_000;

/// The compile output — SVG bytes + provenance.
#[derive(Debug, Clone)]
pub struct ChartArtifact {
    pub svg: String,
    pub width: u32,
    pub height: u32,
    /// sha256 of the SVG bytes — the trace-chain commitment.
    pub sha256: String,
    /// sha256 of the canonical data serialization (embedded in the manifest).
    pub data_sha256: String,
    /// The Vega-Lite compile target (G6 · rich surfaces · flint interop).
    pub vega_lite: Option<String>,
    pub warnings: Vec<String>,
}

/// Canonical data bytes: rows in order, fields in `BTreeMap` order,
/// numbers in fixed-9 trim — deterministic by construction.
pub(crate) fn canonical_data(rows: &[Row]) -> String {
    let mut out = String::with_capacity(rows.len() * 32);
    for r in rows {
        for (k, v) in r {
            out.push_str(k);
            out.push('=');
            match v {
                data::Value::Num(n) => out.push_str(&fmt::fixed_trim(*n, 9)),
                data::Value::Str(s) => out.push_str(s),
            }
            out.push(';');
        }
        out.push('\n');
    }
    out
}

fn semantic_name(s: crate::spec::Semantic) -> &'static str {
    use crate::spec::Semantic::{
        Category, Count, Delta, DurationMs, Percent, Timestamp, Tokens, Usd,
    };
    match s {
        Usd => "usd",
        DurationMs => "duration_ms",
        Tokens => "tokens",
        Count => "count",
        Delta => "delta",
        Percent => "percent",
        Timestamp => "timestamp",
        Category => "category",
    }
}

/// `"key":{"field":"...","semantic":"..."}` fragment (manifest channels).
fn channel_json(key: &str, ch: &crate::spec::Channel) -> String {
    format!(
        "\"{key}\":{{\"field\":\"{}\",\"semantic\":\"{}\"}}",
        fmt::json_escape(&ch.field),
        semantic_name(ch.semantic)
    )
}

fn chart_type_name(t: ChartType) -> &'static str {
    match t {
        ChartType::Bar => "bar",
        ChartType::Line => "line",
        ChartType::AreaBand => "area_band",
        ChartType::Scatter => "scatter",
        ChartType::Heatmap => "heatmap",
    }
}

/// The self-describing manifest (G9 · default-ON) — the COMPLETE recipe:
/// every channel + semantics + dimensions. `nika verify` reconstructs the
/// spec from this alone (SQ-O: the earlier shape dropped semantics/bands/
/// dims — an under-delivering recipe is a half-lie against law 5).
fn manifest_json(spec: &ChartSpec, data_sha256: &str, rows: usize) -> String {
    use std::fmt::Write as _;
    let mut m = String::with_capacity(320);
    let _ = write!(
        m,
        "{{\"nika_chart\":\"0.1.0\",\"type\":\"{}\",\"title\":\"{}\",\"width\":{},\"height\":{},",
        chart_type_name(spec.chart),
        fmt::json_escape(&spec.title),
        spec.width,
        spec.height
    );
    m.push_str(&channel_json("x", &spec.x));
    m.push(',');
    m.push_str(&channel_json("y", &spec.y));
    for (key, ch) in [
        ("y_lo", &spec.y_lo),
        ("y_hi", &spec.y_hi),
        ("y2", &spec.y2),
        ("color", &spec.color),
    ] {
        if let Some(ch) = ch {
            m.push(',');
            m.push_str(&channel_json(key, ch));
        }
    }
    let _ = write!(m, ",\"rows\":{rows},\"data_sha256\":\"{data_sha256}\"}}");
    m
}

/// Compile `(spec, rows)` → deterministic SVG artifact.
/// # Errors
/// Typed [`crate::error::ChartError`] (`NIKA-BUILTIN-CHART-*`) on invalid spec or data.
pub fn compile(spec: &ChartSpec, rows: &[Row]) -> Result<ChartArtifact, ChartError> {
    spec.validate()?;
    if rows.is_empty() {
        return Err(ChartError::EmptyData);
    }
    if rows.len() > MAX_ROWS {
        return Err(ChartError::TooManyPoints {
            got: rows.len(),
            cap: MAX_ROWS,
        });
    }
    let data_sha256 = det_hash::sha256_hex(canonical_data(rows).as_bytes());
    let manifest = manifest_json(spec, &data_sha256, rows.len());
    let data8 = data_sha256.get(..8).unwrap_or("--------");
    let svg = match spec.chart {
        ChartType::Bar => render::bar(spec, rows, &manifest, data8)?,
        ChartType::Line => render::line(spec, rows, &manifest, data8)?,
        ChartType::AreaBand => render::area_band(spec, rows, &manifest, data8)?,
        ChartType::Scatter => render::scatter(spec, rows, &manifest, data8)?,
        ChartType::Heatmap => render::heatmap(spec, rows, &manifest, data8)?,
    };
    let sha256 = det_hash::sha256_hex(svg.as_bytes());
    let vega_lite = vega::emit(spec, rows, &data_sha256);
    let warnings = lint::advisories(spec, rows);
    Ok(ChartArtifact {
        svg,
        width: spec.width,
        height: spec.height,
        sha256,
        data_sha256,
        vega_lite,
        warnings,
    })
}

/// The attestation verb (philosophy law 1): recompile the recipe and
/// byte-compare against a claimed artifact hash. `Ok(true)` = the artifact
/// is exactly what `(spec, rows)` produce · `Ok(false)` = tampered or stale.
/// # Errors
/// Typed [`crate::error::ChartError`] (`NIKA-BUILTIN-CHART-*`) on invalid spec or data.
pub fn verify(spec: &ChartSpec, rows: &[Row], claimed_sha256: &str) -> Result<bool, ChartError> {
    Ok(compile(spec, rows)?.sha256 == claimed_sha256)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::unreadable_literal
)]
mod tests {
    use super::*;
    use data::{Value, row};
    use spec::{Channel, Semantic};

    fn bar_spec() -> ChartSpec {
        ChartSpec {
            chart: ChartType::Bar,
            title: "Per-step duration".to_owned(),
            x: Channel::new("step", Semantic::Category),
            y: Channel::new("ms", Semantic::DurationMs),
            y_lo: None,
            y_hi: None,
            y2: None,
            color: None,
            width: 480,
            height: 300,
        }
    }

    fn bar_rows() -> Vec<Row> {
        vec![
            row(&[
                ("step", Value::Str("fetch".into())),
                ("ms", Value::Num(141.0)),
            ]),
            row(&[("step", Value::Str("jq".into())), ("ms", Value::Num(6.0))]),
            row(&[
                ("step", Value::Str("infer".into())),
                ("ms", Value::Num(2412.0)),
            ]),
        ]
    }

    #[test]
    fn double_render_is_byte_identical() {
        let a = compile(&bar_spec(), &bar_rows()).expect("render a");
        let b = compile(&bar_spec(), &bar_rows()).expect("render b");
        assert_eq!(a.svg, b.svg);
        assert_eq!(a.sha256, b.sha256);
    }

    #[test]
    fn artifact_embeds_manifest_with_data_hash() {
        let a = compile(&bar_spec(), &bar_rows()).expect("render");
        assert!(a.svg.contains("<metadata>"));
        assert!(a.svg.contains(&a.data_sha256));
        assert!(a.svg.contains("nika_chart"));
    }

    #[test]
    fn empty_data_typed_error() {
        let e = compile(&bar_spec(), &[]);
        assert!(matches!(e, Err(ChartError::EmptyData)));
        assert_eq!(
            format!("{}", ChartError::EmptyData),
            "NIKA-BUILTIN-CHART-001 · data has zero rows"
        );
    }

    #[test]
    fn data_change_changes_both_hashes() {
        let a = compile(&bar_spec(), &bar_rows()).expect("a");
        let mut rows2 = bar_rows();
        rows2.push(row(&[
            ("step", Value::Str("write".into())),
            ("ms", Value::Num(12.0)),
        ]));
        let b = compile(&bar_spec(), &rows2).expect("b");
        assert_ne!(a.sha256, b.sha256);
        assert_ne!(a.data_sha256, b.data_sha256);
    }

    #[test]
    fn verify_round_trip_and_tamper() {
        let a = compile(&bar_spec(), &bar_rows()).expect("render");
        assert_eq!(verify(&bar_spec(), &bar_rows(), &a.sha256), Ok(true));
        let mut tampered = a.sha256.clone();
        tampered.replace_range(..1, if a.sha256.starts_with('0') { "1" } else { "0" });
        assert_eq!(verify(&bar_spec(), &bar_rows(), &tampered), Ok(false));
        let mut rows2 = bar_rows();
        rows2.push(row(&[
            ("step", Value::Str("x".into())),
            ("ms", Value::Num(1.0)),
        ]));
        assert_eq!(verify(&bar_spec(), &rows2, &a.sha256), Ok(false));
    }

    #[test]
    fn no_clock_no_rand_markers() {
        let a = compile(&bar_spec(), &bar_rows()).expect("render");
        assert!(!a.svg.contains("timestamp"));
        assert!(!a.svg.contains("Date"));
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::unreadable_literal
)]
mod manifest_tests {
    use super::*;
    use data::{Value, row};
    use spec::{Channel, Semantic};

    #[test]
    fn manifest_carries_the_complete_recipe() {
        let rows: Vec<Row> = (1..=4)
            .map(|i| {
                let f = f64::from(i);
                row(&[
                    ("x", Value::Num(f)),
                    ("lo", Value::Num(f * 10.0)),
                    ("hi", Value::Num(f * 14.0)),
                    ("act", Value::Num(f * 11.0)),
                ])
            })
            .collect();
        let sp = ChartSpec {
            chart: ChartType::AreaBand,
            title: "m".to_owned(),
            x: Channel::new("x", Semantic::Count),
            y: Channel::new("lo", Semantic::DurationMs),
            y_lo: Some(Channel::new("lo", Semantic::DurationMs)),
            y_hi: Some(Channel::new("hi", Semantic::DurationMs)),
            y2: Some(Channel::new("act", Semantic::DurationMs)),
            color: None,
            width: 480,
            height: 300,
        };
        sp.validate().expect("valid");
        let a = compile(&sp, &rows).expect("compile");
        // The SVG-embedded manifest (escaped) must carry EVERY binding.
        for needle in [
            "&quot;width&quot;:480",
            "&quot;height&quot;:300",
            "&quot;y_lo&quot;:",
            "&quot;y_hi&quot;:",
            "&quot;y2&quot;:",
            "&quot;semantic&quot;:&quot;duration_ms&quot;",
            "&quot;semantic&quot;:&quot;count&quot;",
        ] {
            assert!(a.svg.contains(needle), "manifest missing {needle}");
        }
    }

    #[test]
    fn optional_channels_absent_when_unused() {
        let rows = vec![row(&[
            ("k", Value::Str("a".into())),
            ("v", Value::Num(1.0)),
        ])];
        let sp = ChartSpec {
            chart: ChartType::Bar,
            title: "m".to_owned(),
            x: Channel::new("k", Semantic::Category),
            y: Channel::new("v", Semantic::Count),
            y_lo: None,
            y_hi: None,
            y2: None,
            color: None,
            width: 300,
            height: 200,
        };
        let a = compile(&sp, &rows).expect("compile");
        assert!(!a.svg.contains("&quot;y_lo&quot;"));
        assert!(!a.svg.contains("&quot;color&quot;"));
    }
}
