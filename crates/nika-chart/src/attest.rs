//! Attestation round-trip · SQ-R: the manifest claimed « autonomous proof »
//! but nothing could READ it back — `verify()` needed the original spec.
//! This closes the loop: extract the embedded manifest, reconstruct the
//! spec, re-render, byte-compare. The W7 `nika verify <chart.svg>` verb.
//!
//! The parser is a SCANNER over OUR OWN deterministic emission (never a
//! general JSON parser): every `"` inside a value is escaped `\"`, so a
//! raw `"key":` pattern can only match structure — checked against the
//! preceding-backslash case anyway.

use crate::data::Row;
use crate::spec::{Channel, ChartSpec, ChartType, Semantic};
use crate::{det_hash, svg_unescape_metadata};

/// Total verdict — every failure mode is a distinct, honest answer.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Verdict {
    /// Re-render byte-equals the artifact AND the data hash matches.
    Match,
    /// The rows are not the rows this artifact committed to.
    DataMismatch,
    /// Same data, but the artifact bytes differ from a fresh render.
    ByteMismatch,
    /// No `<metadata>` manifest in the SVG.
    NoManifest,
    /// A manifest exists but the spec could not be reconstructed.
    BadManifest,
}

/// Find `"key":` at structure level (previous char must not be `\`).
fn find_key(json: &str, key: &str) -> Option<usize> {
    let pat = format!("\"{key}\":");
    let mut from = 0;
    while let Some(rel) = json.get(from..)?.find(&pat) {
        let at = from + rel;
        if at == 0 || !json.get(..at)?.ends_with('\\') {
            return Some(at + pat.len());
        }
        from = at + 1;
    }
    None
}

/// Parse a string value at `pos` (expects leading `"`) · unescapes our own
/// `json_escape` set: \" \\ \n \r \t \uXXXX.
fn parse_string(json: &str, pos: usize) -> Option<String> {
    let bytes = json.as_bytes();
    if *bytes.get(pos)? != b'"' {
        return None;
    }
    let mut out = String::new();
    let mut chars = json.get(pos + 1..)?.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    let code = u32::from_str_radix(&hex, 16).ok()?;
                    out.push(char::from_u32(code)?);
                }
                _ => return None,
            },
            c => out.push(c),
        }
    }
    None
}

fn str_field(json: &str, key: &str) -> Option<String> {
    parse_string(json, find_key(json, key)?)
}

fn num_field(json: &str, key: &str) -> Option<u32> {
    let at = find_key(json, key)?;
    let rest = json.get(at..)?;
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    rest.get(..end)?.parse().ok()
}

fn semantic_from_name(name: &str) -> Option<Semantic> {
    Some(match name {
        "usd" => Semantic::Usd,
        "duration_ms" => Semantic::DurationMs,
        "tokens" => Semantic::Tokens,
        "count" => Semantic::Count,
        "delta" => Semantic::Delta,
        "percent" => Semantic::Percent,
        "timestamp" => Semantic::Timestamp,
        "category" => Semantic::Category,
        _ => return None,
    })
}

fn chart_type_from_name(name: &str) -> Option<ChartType> {
    Some(match name {
        "bar" => ChartType::Bar,
        "line" => ChartType::Line,
        "area_band" => ChartType::AreaBand,
        "scatter" => ChartType::Scatter,
        "heatmap" => ChartType::Heatmap,
        _ => return None,
    })
}

/// Parse a `{"field":"…","semantic":"…"}` channel object at `key`.
fn channel_field(json: &str, key: &str) -> Option<Channel> {
    let at = find_key(json, key)?;
    let obj = json.get(at..)?;
    if !obj.starts_with('{') {
        return None;
    }
    // Scope the scan to this object (safe: `}` cannot appear unescaped
    // inside our string values — json_escape leaves it alone but field
    // names/semantics never contain braces · title is NOT in a channel).
    let end = obj.find('}')?;
    let obj = obj.get(..end)?;
    let field = str_field(obj, "field")?;
    let semantic = semantic_from_name(&str_field(obj, "semantic")?)?;
    Some(Channel { field, semantic })
}

/// Reconstruct the `ChartSpec` from a manifest JSON string.
fn spec_from_manifest(m: &str) -> Option<ChartSpec> {
    Some(ChartSpec {
        chart: chart_type_from_name(&str_field(m, "type")?)?,
        title: str_field(m, "title")?,
        x: channel_field(m, "x")?,
        y: channel_field(m, "y")?,
        y_lo: channel_field(m, "y_lo"),
        y_hi: channel_field(m, "y_hi"),
        y2: channel_field(m, "y2"),
        color: channel_field(m, "color"),
        width: num_field(m, "width")?,
        height: num_field(m, "height")?,
    })
}

/// Extract the manifest JSON from an SVG artifact (XML-unescaped).
#[must_use]
pub fn extract_manifest(svg: &str) -> Option<String> {
    let start = svg.find("<metadata>")? + "<metadata>".len();
    let end = svg.find("</metadata>")?;
    Some(svg_unescape_metadata(svg.get(start..end)?))
}

/// The autonomous-proof verb: given ONLY the artifact and candidate rows,
/// answer whether this artifact is exactly what those rows produce.
#[must_use]
pub fn verify_artifact(svg: &str, rows: &[Row]) -> Verdict {
    let Some(manifest) = extract_manifest(svg) else {
        return Verdict::NoManifest;
    };
    let Some(spec) = spec_from_manifest(&manifest) else {
        return Verdict::BadManifest;
    };
    let Some(claimed_data) = str_field(&manifest, "data_sha256") else {
        return Verdict::BadManifest;
    };
    if det_hash::sha256_hex(crate::canonical_data(rows).as_bytes()) != claimed_data {
        return Verdict::DataMismatch;
    }
    match crate::compile(&spec, rows) {
        Ok(a) if a.svg == svg => Verdict::Match,
        Ok(_) => Verdict::ByteMismatch,
        Err(_) => Verdict::BadManifest,
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

    fn n(v: f64) -> Value {
        Value::Num(v)
    }
    fn s(v: &str) -> Value {
        Value::Str(v.to_owned())
    }

    #[allow(clippy::too_many_lines)] // fixture builder — 5 chart types inline
    fn all_five() -> Vec<(ChartSpec, Vec<Row>)> {
        let mut out = Vec::new();
        // bar · title with escapes to prove the unescape path
        out.push((
            ChartSpec {
                chart: ChartType::Bar,
                title: "he said \"hi\" \u{b7} <ok&done>".to_owned(),
                x: Channel::new("k", Semantic::Category),
                y: Channel::new("v", Semantic::Usd),
                y_lo: None,
                y_hi: None,
                y2: None,
                color: None,
                width: 300,
                height: 200,
            },
            vec![
                row(&[("k", s("a")), ("v", n(1.5))]),
                row(&[("k", s("b")), ("v", n(2.5))]),
            ],
        ));
        // line multi-series
        let ml = ChartSpec {
            chart: ChartType::Line,
            title: "ml".to_owned(),
            x: Channel::new("x", Semantic::Count),
            y: Channel::new("v", Semantic::DurationMs),
            y_lo: None,
            y_hi: None,
            y2: None,
            color: Some(Channel::new("p", Semantic::Category)),
            width: 400,
            height: 240,
        };
        ml.validate().expect("valid");
        out.push((
            ml,
            (1..=6)
                .flat_map(|i| {
                    ["a", "b"].map(|p| {
                        row(&[
                            ("x", n(f64::from(i))),
                            ("p", s(p)),
                            ("v", n(f64::from(i * 100))),
                        ])
                    })
                })
                .collect(),
        ));
        // band
        let bd = ChartSpec {
            chart: ChartType::AreaBand,
            title: "bd".to_owned(),
            x: Channel::new("x", Semantic::Count),
            y: Channel::new("lo", Semantic::DurationMs),
            y_lo: Some(Channel::new("lo", Semantic::DurationMs)),
            y_hi: Some(Channel::new("hi", Semantic::DurationMs)),
            y2: Some(Channel::new("act", Semantic::DurationMs)),
            color: None,
            width: 400,
            height: 240,
        };
        bd.validate().expect("valid");
        out.push((
            bd,
            (1..=5)
                .map(|i| {
                    let f = f64::from(i);
                    row(&[
                        ("x", n(f)),
                        ("lo", n(f * 100.0)),
                        ("hi", n(f * 140.0 + 10.0)),
                        ("act", n(f * 110.0)),
                    ])
                })
                .collect(),
        ));
        // scatter
        out.push((
            ChartSpec {
                chart: ChartType::Scatter,
                title: "sc".to_owned(),
                x: Channel::new("x", Semantic::DurationMs),
                y: Channel::new("v", Semantic::Usd),
                y_lo: None,
                y_hi: None,
                y2: None,
                color: None,
                width: 300,
                height: 200,
            },
            (1..=20)
                .map(|i| row(&[("x", n(f64::from(i * 50))), ("v", n(f64::from(i) * 0.001))]))
                .collect(),
        ));
        // heatmap
        out.push((
            ChartSpec {
                chart: ChartType::Heatmap,
                title: "hm".to_owned(),
                x: Channel::new("cx", Semantic::Category),
                y: Channel::new("cy", Semantic::Category),
                y_lo: None,
                y_hi: None,
                y2: None,
                color: Some(Channel::new("v", Semantic::Delta)),
                width: 300,
                height: 200,
            },
            (0..9)
                .map(|i| {
                    row(&[
                        ("cx", s(&format!("x{}", i % 3))),
                        ("cy", s(&format!("y{}", i / 3))),
                        ("v", n(f64::from(i) - 4.0)),
                    ])
                })
                .collect(),
        ));
        out
    }

    #[test]
    fn round_trip_all_five_types() {
        for (spec, rows) in all_five() {
            let a = crate::compile(&spec, &rows).expect("compile");
            assert_eq!(
                verify_artifact(&a.svg, &rows),
                Verdict::Match,
                "type {:?} failed the autonomous round-trip",
                spec.chart
            );
        }
    }

    #[test]
    fn tampered_bytes_detected() {
        let (spec, rows) = all_five().remove(0);
        let a = crate::compile(&spec, &rows).expect("compile");
        let tampered = a.svg.replacen("#0072B2", "#FF0000", 1);
        assert_eq!(verify_artifact(&tampered, &rows), Verdict::ByteMismatch);
    }

    #[test]
    fn wrong_rows_detected() {
        let (spec, mut rows) = all_five().remove(0);
        let a = crate::compile(&spec, &rows).expect("compile");
        rows.push(row(&[("k", s("z")), ("v", n(9.0))]));
        assert_eq!(verify_artifact(&a.svg, &rows), Verdict::DataMismatch);
    }

    #[test]
    fn foreign_svg_is_no_manifest() {
        assert_eq!(verify_artifact("<svg></svg>", &[]), Verdict::NoManifest);
    }
}
