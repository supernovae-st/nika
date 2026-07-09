//! Vega-Lite emitter (G6) — the compile-target for RICH surfaces (vscode
//! interactivity later · flint-mcp interop). Hand-built JSON · zero-dep ·
//! `BTreeMap` field order + fixed-9 numerics ⇒ deterministic bytes.
//!
//! Schema URL is ONE seam (`VL_SCHEMA`) — bump on VL major verification.

use crate::data::{Row, Value};
use crate::fmt;
use crate::spec::{ChartSpec, ChartType, Semantic};

const VL_SCHEMA: &str = "https://vega.github.io/schema/vega-lite/v6.json";

fn esc(s: &str) -> String {
    fmt::json_escape(s)
}

/// VL `type` for a semantic (Timestamp maps quantitative in v0 · epoch ms).
fn vl_type(sem: Semantic) -> &'static str {
    match sem {
        Semantic::Category => "nominal",
        _ => "quantitative",
    }
}

fn values_json(rows: &[Row]) -> String {
    let mut out = String::with_capacity(rows.len() * 48);
    out.push('[');
    for (i, r) in rows.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('{');
        for (j, (k, v)) in r.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            out.push('"');
            out.push_str(&esc(k));
            out.push_str("\":");
            match v {
                Value::Num(n) => out.push_str(&fmt::fixed_trim(*n, 9)),
                Value::Str(s) => {
                    out.push('"');
                    out.push_str(&esc(s));
                    out.push('"');
                }
            }
        }
        out.push('}');
    }
    out.push(']');
    out
}

fn field(name: &str, sem: Semantic) -> String {
    format!(
        "{{\"field\":\"{}\",\"type\":\"{}\"}}",
        esc(name),
        vl_type(sem)
    )
}

fn head(spec: &ChartSpec, rows: &[Row], data_sha256: &str) -> String {
    format!(
        "\"$schema\":\"{VL_SCHEMA}\",\"usermeta\":{{\"nika_chart\":\"0.1.0\",\"data_sha256\":\"{}\"}},\"title\":\"{}\",\"width\":{},\"height\":{},\"data\":{{\"values\":{}}}",
        esc(data_sha256),
        esc(&spec.title),
        spec.width,
        spec.height,
        values_json(rows)
    )
}

/// Emit the Vega-Lite compile target for the spec. `None` only for shapes
/// the emitter does not cover yet (kept total on the v1 five).
#[must_use]
pub fn emit(spec: &ChartSpec, rows: &[Row], data_sha256: &str) -> Option<String> {
    let h = head(spec, rows, data_sha256);
    let x = field(&spec.x.field, spec.x.semantic);
    let y = field(&spec.y.field, spec.y.semantic);
    // Series split rides VL's color channel (SQ-K: dropping it made the VL
    // artifact LIE about multi-series charts) — heatmap keeps its own arm.
    let color = match (&spec.color, spec.chart) {
        (Some(c), ChartType::Bar | ChartType::Line | ChartType::Scatter) => format!(
            ",\"color\":{{\"field\":\"{}\",\"type\":\"nominal\"}}",
            esc(&c.field)
        ),
        _ => String::new(),
    };
    match spec.chart {
        ChartType::Bar => Some(format!(
            "{{{h},\"mark\":\"bar\",\"encoding\":{{\"x\":{x},\"y\":{y}{color}}}}}"
        )),
        ChartType::Line => Some(format!(
            "{{{h},\"mark\":{{\"type\":\"line\",\"point\":true}},\"encoding\":{{\"x\":{x},\"y\":{y}{color}}}}}"
        )),
        ChartType::Scatter => Some(format!(
            "{{{h},\"mark\":\"point\",\"encoding\":{{\"x\":{x},\"y\":{y}{color}}}}}"
        )),
        ChartType::Heatmap => {
            let c = spec.color.as_ref()?;
            let scheme = if c.semantic == Semantic::Delta {
                "\"scale\":{\"scheme\":\"redblue\",\"domainMid\":0}"
            } else {
                "\"scale\":{\"scheme\":\"viridis\"}"
            };
            Some(format!(
                "{{{h},\"mark\":\"rect\",\"encoding\":{{\"x\":{x},\"y\":{y},\"color\":{{\"field\":\"{}\",\"type\":\"quantitative\",{scheme}}}}}}}",
                esc(&c.field)
            ))
        }
        ChartType::AreaBand => {
            let lo = spec.y_lo.as_ref()?;
            let hi = spec.y_hi.as_ref()?;
            let band = format!(
                "{{\"mark\":{{\"type\":\"area\",\"opacity\":0.18}},\"encoding\":{{\"x\":{x},\"y\":{},\"y2\":{{\"field\":\"{}\"}}}}}}",
                field(&lo.field, lo.semantic),
                esc(&hi.field)
            );
            let mid = format!(
                "{{\"mark\":{{\"type\":\"line\",\"strokeDash\":[4,3]}},\"encoding\":{{\"x\":{x},\"y\":{y}}}}}"
            );
            let mut layers = vec![band, mid];
            if let Some(a) = &spec.y2 {
                layers.push(format!(
                    "{{\"mark\":\"line\",\"encoding\":{{\"x\":{x},\"y\":{},\"color\":{{\"value\":\"#D55E00\"}}}}}}",
                    field(&a.field, a.semantic)
                ));
            }
            Some(format!("{{{h},\"layer\":[{}]}}", layers.join(",")))
        }
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
mod tests {
    use super::*;
    use crate::data::{Value, row};
    use crate::spec::Channel;

    #[test]
    fn bar_emits_valid_shape() {
        let spec = ChartSpec {
            chart: ChartType::Bar,
            title: "t".to_owned(),
            x: Channel::new("step", Semantic::Category),
            y: Channel::new("ms", Semantic::DurationMs),
            y_lo: None,
            y_hi: None,
            y2: None,
            color: None,
            width: 400,
            height: 300,
        };
        let rows = vec![row(&[
            ("step", Value::Str("a".into())),
            ("ms", Value::Num(1.5)),
        ])];
        let vl = emit(&spec, &rows, "abc123").expect("vl");
        assert!(vl.contains("\"$schema\":\"https://vega.github.io/schema/vega-lite/v6.json\""));
        assert!(vl.contains("\"mark\":\"bar\""));
        assert!(vl.contains("\"type\":\"nominal\""));
        assert!(vl.contains("\"ms\":1.5"));
        // Deterministic re-emit.
        assert_eq!(vl, emit(&spec, &rows, "abc123").expect("vl2"));
        assert!(vl.contains("\"usermeta\""));
    }
}
