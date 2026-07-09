//! `nika:chart` — deterministic chart artifact (stdlib §Media graduate #3
//! candidate · CHT master plan W2 §2).
//!
//! Pure compute (`nika_chart::compile` — zero-dep, byte-identical SVG) +
//! ONE permit-gated write. No http, no clock, no keys: the deterministic
//! sibling of `image_generate`/`tts_generate`. Re-runs are byte-identical
//! and idempotent (hash-equal artifacts are never rewritten — the
//! `image::save` precedent); the artifact sha256 rides `outputs:` into the
//! trace chain (« your workflow draws its own receipts »).
//!
//! Codes: `NIKA-BUILTIN-CHART-001..007` are the compile codes (owned by
//! `nika-chart::error` — its `Display` already speaks the 4-segment form) ·
//! `-008` is the builtin's own I/O tail (write/read failures).

use std::path::Path;

use nika_kernel::io::fs::{FsReadDyn, FsWriteDyn};

use crate::{Args, BuiltinFailure, BuiltinOutcome};

/// Arg-shape / spec problems (the crate's `InvalidSpec` class · 004).
const C_SPEC: &str = "NIKA-BUILTIN-CHART-004";
/// The builtin's own I/O tail — the ONLY code the crate does not own.
const C_IO: &str = "NIKA-BUILTIN-CHART-008";

/// Map a compile error to its full 4-segment code (the crate emits the
/// 3-digit tails · `BuiltinFailure` wants a `&'static str`).
fn code_of(e: &nika_chart::error::ChartError) -> &'static str {
    match e.code() {
        "001" => "NIKA-BUILTIN-CHART-001",
        "002" => "NIKA-BUILTIN-CHART-002",
        "003" => "NIKA-BUILTIN-CHART-003",
        "005" => "NIKA-BUILTIN-CHART-005",
        "006" => "NIKA-BUILTIN-CHART-006",
        "007" => "NIKA-BUILTIN-CHART-007",
        _ => C_SPEC,
    }
}

fn semantic_of(name: &str) -> Result<nika_chart::spec::Semantic, BuiltinFailure> {
    use nika_chart::spec::Semantic;
    Ok(match name {
        "usd" => Semantic::Usd,
        "duration_ms" => Semantic::DurationMs,
        "tokens" => Semantic::Tokens,
        "count" => Semantic::Count,
        "delta" => Semantic::Delta,
        "percent" => Semantic::Percent,
        "timestamp" => Semantic::Timestamp,
        "category" => Semantic::Category,
        other => {
            return Err(BuiltinFailure::new(
                C_SPEC,
                format!(
                    "unknown semantic `{other}` — the set is closed: usd | duration_ms | \
                     tokens | count | delta | percent | timestamp | category"
                ),
            ));
        }
    })
}

fn chart_type_of(name: &str) -> Result<nika_chart::spec::ChartType, BuiltinFailure> {
    use nika_chart::spec::ChartType;
    Ok(match name {
        "bar" => ChartType::Bar,
        "line" => ChartType::Line,
        "area_band" => ChartType::AreaBand,
        "scatter" => ChartType::Scatter,
        "heatmap" => ChartType::Heatmap,
        other => {
            return Err(BuiltinFailure::new(
                C_SPEC,
                format!(
                    "unknown chart type `{other}` — the set is closed: bar | line | \
                     area_band | scatter | heatmap"
                ),
            ));
        }
    })
}

/// Channel from `chart.<key>` + the `semantics:` map. `default` covers the
/// field when the author declared no semantic (x/categorical axes →
/// `category` · value axes → `count`).
fn channel(
    chart: &serde_json::Map<String, serde_json::Value>,
    semantics: Option<&serde_json::Map<String, serde_json::Value>>,
    key: &str,
    default: &str,
) -> Result<Option<nika_chart::spec::Channel>, BuiltinFailure> {
    let Some(value) = chart.get(key) else {
        return Ok(None);
    };
    let field = value.as_str().ok_or_else(|| {
        BuiltinFailure::new(
            C_SPEC,
            format!("`chart.{key}:` must be a field-name string"),
        )
    })?;
    let sem_name = semantics
        .and_then(|s| s.get(field))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(default);
    Ok(Some(nika_chart::spec::Channel {
        field: field.to_owned(),
        semantic: semantic_of(sem_name)?,
    }))
}

/// Convert one JSON row-object into the crate's `Row` (strings + finite
/// numbers only — bool/null/nested die typed, the 006 class).
fn row_of(
    value: &serde_json::Value,
    index: usize,
) -> Result<nika_chart::data::Row, BuiltinFailure> {
    let object = value.as_object().ok_or_else(|| {
        BuiltinFailure::new(C_SPEC, format!("data[{index}] is not an object row"))
    })?;
    let mut row = nika_chart::data::Row::new();
    for (key, cell) in object {
        let converted = match cell {
            serde_json::Value::String(s) => nika_chart::data::Value::Str(s.clone()),
            serde_json::Value::Number(n) => {
                let Some(f) = n.as_f64() else {
                    return Err(BuiltinFailure::new(
                        "NIKA-BUILTIN-CHART-003",
                        format!("data[{index}].{key} is not representable as f64"),
                    ));
                };
                nika_chart::data::Value::Num(f)
            }
            other => {
                return Err(BuiltinFailure::new(
                    "NIKA-BUILTIN-CHART-006",
                    format!(
                        "data[{index}].{key} is {} — rows carry strings and numbers only",
                        match other {
                            serde_json::Value::Bool(_) => "a bool",
                            serde_json::Value::Null => "null",
                            serde_json::Value::Array(_) => "an array",
                            _ => "a nested object",
                        }
                    ),
                ));
            }
        };
        row.insert(key.clone(), converted);
    }
    Ok(row)
}

/// Resolve `data:` — inline `[{…}]` rows, or `{path}` (a JSON file the
/// dispatch guard already cleared for READ).
async fn rows_of<F: FsReadDyn>(
    fs: &F,
    args: &Args,
) -> Result<Vec<nika_chart::data::Row>, BuiltinFailure> {
    let data = args
        .get("data")
        .ok_or_else(|| BuiltinFailure::new(C_SPEC, "`data:` is required"))?;
    let inline;
    let rows_json: &Vec<serde_json::Value> = match data {
        serde_json::Value::Array(rows) => rows,
        serde_json::Value::Object(o) => {
            let path = o
                .get("path")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    BuiltinFailure::new(
                        C_SPEC,
                        "`data:` must be an array of row objects or `{ path: <json file> }`",
                    )
                })?;
            let text = fs
                .read_to_string(Path::new(path))
                .await
                .map_err(|e| BuiltinFailure::new(C_IO, format!("data read failed: {e}")))?;
            let parsed: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
                BuiltinFailure::new(C_SPEC, format!("`{path}` is not valid JSON: {e}"))
            })?;
            let serde_json::Value::Array(rows) = parsed else {
                return Err(BuiltinFailure::new(
                    C_SPEC,
                    format!("`{path}` must contain a JSON array of row objects"),
                ));
            };
            inline = rows;
            &inline
        }
        _ => {
            return Err(BuiltinFailure::new(
                C_SPEC,
                "`data:` must be an array of row objects or `{ path: <json file> }`",
            ));
        }
    };
    rows_json
        .iter()
        .enumerate()
        .map(|(i, v)| row_of(v, i))
        .collect()
}

/// Build the `ChartSpec` from `chart:` + `semantics:`.
use nika_chart::spec::ChartType;

fn spec_of(args: &Args) -> Result<nika_chart::spec::ChartSpec, BuiltinFailure> {
    let chart = args
        .get("chart")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| BuiltinFailure::new(C_SPEC, "`chart:` object is required"))?;
    let semantics = args.get("semantics").and_then(serde_json::Value::as_object);
    let type_name = chart
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| BuiltinFailure::new(C_SPEC, "`chart.type:` is required"))?;
    let chart_type = chart_type_of(type_name)?;

    // Per-type defaults: categorical axes read `category`, value axes `count`.
    let x_default = match chart_type {
        ChartType::Bar | ChartType::Heatmap => "category",
        _ => "count",
    };
    let y_default = if chart_type == ChartType::Heatmap {
        "category"
    } else {
        "count"
    };
    let color_default = if chart_type == ChartType::Heatmap {
        "count"
    } else {
        "category"
    };

    let dim = |key: &str, default: u32| -> Result<u32, BuiltinFailure> {
        match chart.get(key) {
            None => Ok(default),
            Some(v) => v
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| {
                    BuiltinFailure::new(
                        C_SPEC,
                        format!("`chart.{key}:` must be a positive integer"),
                    )
                }),
        }
    };

    let spec = nika_chart::spec::ChartSpec {
        chart: chart_type,
        title: chart
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned(),
        x: channel(chart, semantics, "x", x_default)?
            .ok_or_else(|| BuiltinFailure::new(C_SPEC, "`chart.x:` is required"))?,
        y: channel(chart, semantics, "y", y_default)?
            .ok_or_else(|| BuiltinFailure::new(C_SPEC, "`chart.y:` is required"))?,
        y_lo: channel(chart, semantics, "y_lo", "count")?,
        y_hi: channel(chart, semantics, "y_hi", "count")?,
        y2: channel(chart, semantics, "y2", "count")?,
        color: channel(chart, semantics, "color", color_default)?,
        width: dim("width", 520)?,
        height: dim("height", 300)?,
    };
    Ok(spec)
}

/// Write `bytes` at `path` unless an identical artifact already sits there
/// (the idempotence law: byte-equal ⇒ zero I/O churn). Returns `wrote`.
async fn save<F: FsReadDyn + FsWriteDyn>(
    fs: &F,
    path: &str,
    bytes: &[u8],
) -> Result<bool, BuiltinFailure> {
    if fs.exists(Path::new(path)).await
        && let Ok(existing) = fs.read(Path::new(path)).await
        && existing == bytes
    {
        return Ok(false);
    }
    fs.write(Path::new(path), bytes)
        .await
        .map_err(|e| BuiltinFailure::new(C_IO, format!("write failed: {e}")))?;
    Ok(true)
}

/// The builtin: parse → compile (pure · deterministic) → save → receipts.
pub(crate) async fn render<F: FsReadDyn + FsWriteDyn>(fs: &F, args: &Args) -> BuiltinOutcome {
    let out = args
        .get("out")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            BuiltinFailure::new(C_SPEC, "`out:` (the .svg artifact path) is required")
        })?;
    if !Path::new(out)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
    {
        return Err(BuiltinFailure::new(
            C_SPEC,
            format!("`out:` must end in .svg (got `{out}`) — SVG is the attestation surface"),
        ));
    }
    let compile_to = match args.get("compile_to").and_then(serde_json::Value::as_str) {
        None => None,
        Some("vega_lite") => Some(()),
        Some(other) => {
            return Err(BuiltinFailure::new(
                C_SPEC,
                format!("unknown compile_to `{other}` — the set is closed: vega_lite"),
            ));
        }
    };

    let spec = spec_of(args)?;
    let rows = rows_of(fs, args).await?;
    let artifact = nika_chart::compile(&spec, &rows)
        .map_err(|e| BuiltinFailure::new(code_of(&e), e.to_string()))?;

    let wrote = save(fs, out, artifact.svg.as_bytes()).await?;

    let mut outputs = serde_json::json!({
        "path": out,
        "sha256": artifact.sha256,
        "data_sha256": artifact.data_sha256,
        "width": artifact.width,
        "height": artifact.height,
        "bytes": artifact.svg.len(),
        "wrote": wrote,
        "warnings": artifact.warnings,
    });

    if compile_to.is_some() {
        let vl = artifact.vega_lite.ok_or_else(|| {
            BuiltinFailure::new(C_SPEC, "this chart shape has no Vega-Lite compile target")
        })?;
        let vl_path = format!("{}.vl.json", out.trim_end_matches(".svg"));
        save(fs, &vl_path, vl.as_bytes()).await?;
        if let Some(map) = outputs.as_object_mut() {
            map.insert(
                "vega_lite".to_owned(),
                serde_json::json!({
                    "path": vl_path,
                    "sha256": nika_chart::det_hash::sha256_hex(vl.as_bytes()),
                }),
            );
        }
    }
    Ok(outputs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel_mock::MockFs;

    fn args(json: &serde_json::Value) -> Args {
        json.as_object().cloned().unwrap_or_default()
    }

    fn bar_args(out: &str) -> Args {
        args(&serde_json::json!({
            "data": [
                { "step": "fetch", "ms": 141.0 },
                { "step": "jq", "ms": 6.0 },
                { "step": "infer", "ms": 2412.0 },
            ],
            "semantics": { "step": "category", "ms": "duration_ms" },
            "chart": { "type": "bar", "x": "step", "y": "ms", "title": "Per-step duration" },
            "out": out,
        }))
    }

    #[tokio::test]
    async fn renders_deterministic_svg_with_receipts() {
        let fs = MockFs::default();
        let a = render(&fs, &bar_args("out/chart.svg"))
            .await
            .expect("render");
        assert_eq!(a["path"], "out/chart.svg");
        assert_eq!(a["sha256"].as_str().map(str::len), Some(64));
        assert_eq!(a["wrote"], true);
        // Idempotence: the second run rewrites NOTHING and hashes identical.
        let b = render(&fs, &bar_args("out/chart.svg"))
            .await
            .expect("rerun");
        assert_eq!(b["wrote"], false);
        assert_eq!(a["sha256"], b["sha256"]);
        assert_eq!(a["data_sha256"], b["data_sha256"]);
    }

    #[tokio::test]
    async fn vega_sibling_lands_with_compile_to() {
        let fs = MockFs::default();
        let mut call = bar_args("out/chart.svg");
        call.insert("compile_to".into(), serde_json::json!("vega_lite"));
        let a = render(&fs, &call).await.expect("render");
        assert_eq!(a["vega_lite"]["path"], "out/chart.vl.json");
        assert_eq!(a["vega_lite"]["sha256"].as_str().map(str::len), Some(64));
    }

    #[tokio::test]
    async fn closed_enums_die_typed() {
        let fs = MockFs::default();
        let mut call = bar_args("out/chart.svg");
        if let Some(chart) = call
            .get_mut("chart")
            .and_then(serde_json::Value::as_object_mut)
        {
            chart.insert("type".into(), serde_json::json!("sunburst"));
        }
        let e = render(&fs, &call).await.expect_err("must fail");
        assert_eq!(e.code, "NIKA-BUILTIN-CHART-004");
        assert!(e.message.contains("closed"));
    }

    #[tokio::test]
    async fn out_must_be_svg() {
        let fs = MockFs::default();
        let e = render(&fs, &bar_args("out/chart.png"))
            .await
            .expect_err("must fail");
        assert_eq!(e.code, "NIKA-BUILTIN-CHART-004");
        assert!(e.message.contains("attestation surface"));
    }

    #[tokio::test]
    async fn compile_errors_keep_their_codes() {
        let fs = MockFs::default();
        let mut call = bar_args("out/chart.svg");
        call.insert("data".into(), serde_json::json!([]));
        let e = render(&fs, &call).await.expect_err("must fail");
        assert_eq!(e.code, "NIKA-BUILTIN-CHART-001");
    }

    #[tokio::test]
    async fn bool_cell_dies_006() {
        let fs = MockFs::default();
        let mut call = bar_args("out/chart.svg");
        call.insert(
            "data".into(),
            serde_json::json!([{ "step": "a", "ms": true }]),
        );
        let e = render(&fs, &call).await.expect_err("must fail");
        assert_eq!(e.code, "NIKA-BUILTIN-CHART-006");
    }
}
