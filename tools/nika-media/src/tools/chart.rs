// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:chart — Generate charts from JSON data as PNG images.
//!
//! Uses `charts-rs` to render bar, line, and pie charts.
//! Input: JSON config with type, series data, labels, and optional styling.
//! Output: PNG binary stored in CAS.

use std::future::Future;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::MediaToolError;
use super::error::{invalid_args, tool_error};
use super::{MediaOp, MediaOpResult};

pub struct ChartOp;

impl MediaOp for ChartOp {
    fn name(&self) -> &'static str {
        "chart"
    }

    fn description(&self) -> &'static str {
        "Generate a chart (bar, line, pie) from JSON data as a PNG image"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
          "type": "object",
          "properties": {
            "type": {
              "type": "string",
              "enum": ["bar", "line", "pie"],
              "description": "Chart type"
            },
            "title": {
              "type": "string",
              "description": "Chart title (optional)"
            },
            "width": {
              "type": "integer",
              "description": "Chart width in pixels (default: 800)",
              "default": 800
            },
            "height": {
              "type": "integer",
              "description": "Chart height in pixels (default: 500)",
              "default": 500
            },
            "series": {
              "type": "array",
              "description": "Data series: [{name, data: [numbers]}]",
              "items": {
                "type": "object",
                "properties": {
                  "name": { "type": "string" },
                  "data": { "type": "array", "items": { "type": "number" } }
                },
                "required": ["name", "data"]
              }
            },
            "labels": {
              "type": "array",
              "description": "X-axis labels (required for bar/line, ignored for pie)",
              "items": { "type": "string" }
            }
          },
          "required": ["type", "series"],
          "additionalProperties": false
        })
    }

    fn execute<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a MediaToolContext,
    ) -> Pin<Box<dyn Future<Output = Result<MediaOpResult, MediaToolError>> + Send + 'a>> {
        Box::pin(async move {
            ctx.check_cancelled()?;

            let chart_type = args
                .get("type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| invalid_args("chart", "missing 'type' (bar, line, or pie)"))?;

            let series_arr = args
                .get("series")
                .and_then(|v| v.as_array())
                .ok_or_else(|| invalid_args("chart", "missing 'series' array"))?;

            if series_arr.is_empty() {
                return Err(invalid_args("chart", "'series' must not be empty"));
            }

            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let width = args
                .get("width")
                .and_then(|v| v.as_u64())
                .unwrap_or(800)
                .clamp(100, 4000) as f32;
            let height = args
                .get("height")
                .and_then(|v| v.as_u64())
                .unwrap_or(500)
                .clamp(100, 4000) as f32;

            // Parse series
            let series_list = parse_series(series_arr)?;

            // Parse labels (reject non-string values instead of silently filtering)
            let labels: Vec<String> = match args.get("labels").and_then(|v| v.as_array()) {
                Some(arr) => arr
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        v.as_str().map(|s| s.to_string()).ok_or_else(|| {
                            invalid_args("chart", format!("labels[{i}] must be a string"))
                        })
                    })
                    .collect::<Result<_, _>>()?,
                None => Vec::new(),
            };

            let chart_type_owned = chart_type.to_string();

            // Render chart on compute pool (CPU-bound SVG generation + PNG encoding)
            let png_data = ctx
                .compute
                .compute(move || -> Result<Vec<u8>, MediaToolError> {
                    let svg = render_chart_svg(
                        &chart_type_owned,
                        &title,
                        width,
                        height,
                        series_list,
                        &labels,
                    )?;
                    charts_rs::svg_to_png(&svg)
                        .map_err(|e| tool_error("chart", format!("PNG encoding failed: {e}")))
                })
                .await??;

            Ok(MediaOpResult::Binary {
                data: png_data,
                mime_type: "image/png".to_string(),
                extension: "png".to_string(),
                metadata: serde_json::json!({
                  "chart_type": chart_type,
                  "width": width as u32,
                  "height": height as u32,
                }),
            })
        })
    }
}

/// Parse JSON series array into charts-rs Series objects.
fn parse_series(arr: &[serde_json::Value]) -> Result<Vec<charts_rs::Series>, MediaToolError> {
    let mut result = Vec::with_capacity(arr.len());
    for (i, item) in arr.iter().enumerate() {
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| invalid_args("chart", format!("series[{i}] missing 'name'")))?;

        let data = item
            .get("data")
            .and_then(|v| v.as_array())
            .ok_or_else(|| invalid_args("chart", format!("series[{i}] missing 'data' array")))?;

        let values: Vec<f32> = data
            .iter()
            .enumerate()
            .map(|(j, v)| {
                let n = v.as_f64().ok_or_else(|| {
                    invalid_args("chart", format!("series[{i}].data[{j}] is not a number"))
                })?;
                let f = n as f32;
                if !f.is_finite() {
                    return Err(invalid_args(
                        "chart",
                        format!("series[{i}].data[{j}] is not a finite number"),
                    ));
                }
                Ok(f)
            })
            .collect::<Result<_, _>>()?;

        if values.is_empty() {
            return Err(invalid_args(
                "chart",
                format!("series[{i}].data must not be empty"),
            ));
        }

        result.push(charts_rs::Series::new(name.to_string(), values));
    }
    Ok(result)
}

/// Render chart to SVG string.
fn render_chart_svg(
    chart_type: &str,
    title: &str,
    width: f32,
    height: f32,
    series_list: Vec<charts_rs::Series>,
    labels: &[String],
) -> Result<String, MediaToolError> {
    match chart_type {
        "bar" => {
            let mut chart = charts_rs::BarChart::new(series_list, labels.to_vec());
            chart.width = width;
            chart.height = height;
            if !title.is_empty() {
                chart.title_text = title.to_string();
            }
            chart
                .svg()
                .map_err(|e| tool_error("chart", format!("bar chart SVG failed: {e}")))
        }
        "line" => {
            let mut chart = charts_rs::LineChart::new(series_list, labels.to_vec());
            chart.width = width;
            chart.height = height;
            if !title.is_empty() {
                chart.title_text = title.to_string();
            }
            chart
                .svg()
                .map_err(|e| tool_error("chart", format!("line chart SVG failed: {e}")))
        }
        "pie" => {
            let mut chart = charts_rs::PieChart::new(series_list);
            chart.width = width;
            chart.height = height;
            if !title.is_empty() {
                chart.title_text = title.to_string();
            }
            chart
                .svg()
                .map_err(|e| tool_error("chart", format!("pie chart SVG failed: {e}")))
        }
        other => Err(invalid_args(
            "chart",
            format!("unsupported chart type '{other}', use bar, line, or pie"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CasStore;
    use std::sync::Arc;

    async fn setup() -> (tempfile::TempDir, Arc<MediaToolContext>) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        (dir, ctx)
    }

    #[tokio::test]
    async fn chart_bar_basic() {
        let (_dir, ctx) = setup().await;
        let op = ChartOp;
        let result = op
            .execute(
                serde_json::json!({
                  "type": "bar",
                  "title": "Monthly Sales",
                  "series": [
                    {"name": "Product A", "data": [10.0, 20.0, 30.0, 25.0]},
                    {"name": "Product B", "data": [15.0, 25.0, 20.0, 35.0]}
                  ],
                  "labels": ["Jan", "Feb", "Mar", "Apr"]
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Binary {
            data,
            mime_type,
            metadata,
            ..
        } = result
        {
            assert_eq!(mime_type, "image/png");
            assert!(!data.is_empty(), "PNG data should not be empty");
            assert_eq!(
                &data[..4],
                &[0x89, 0x50, 0x4E, 0x47],
                "should start with PNG magic"
            );
            image::load_from_memory(&data).expect("bar chart PNG must be decodable");
            assert_eq!(metadata["chart_type"], "bar");
        } else {
            panic!("expected Binary result");
        }
    }

    #[tokio::test]
    async fn chart_line_basic() {
        let (_dir, ctx) = setup().await;
        let op = ChartOp;
        let result = op
            .execute(
                serde_json::json!({
                  "type": "line",
                  "series": [{"name": "Temp", "data": [22.0, 25.0, 28.0, 30.0, 27.0]}],
                  "labels": ["Mon", "Tue", "Wed", "Thu", "Fri"]
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Binary {
            data, mime_type, ..
        } = result
        {
            assert_eq!(mime_type, "image/png");
            assert_eq!(&data[..4], &[0x89, 0x50, 0x4E, 0x47]);
            image::load_from_memory(&data).expect("line chart PNG must be decodable");
        } else {
            panic!("expected Binary result");
        }
    }

    #[tokio::test]
    async fn chart_pie_basic() {
        let (_dir, ctx) = setup().await;
        let op = ChartOp;
        let result = op
            .execute(
                serde_json::json!({
                  "type": "pie",
                  "title": "Market Share",
                  "series": [
                    {"name": "Chrome", "data": [65.0]},
                    {"name": "Safari", "data": [18.0]},
                    {"name": "Firefox", "data": [10.0]},
                    {"name": "Other", "data": [7.0]}
                  ]
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Binary {
            data,
            mime_type,
            metadata,
            ..
        } = result
        {
            assert_eq!(mime_type, "image/png");
            assert_eq!(&data[..4], &[0x89, 0x50, 0x4E, 0x47]);
            image::load_from_memory(&data).expect("pie chart PNG must be decodable");
            assert_eq!(metadata["chart_type"], "pie");
        } else {
            panic!("expected Binary result");
        }
    }

    #[tokio::test]
    async fn chart_stored_in_cas() {
        let (_dir, ctx) = setup().await;
        let op = ChartOp;
        let result = op
            .execute(
                serde_json::json!({
                  "type": "bar",
                  "series": [{"name": "X", "data": [1.0, 2.0, 3.0]}],
                  "labels": ["A", "B", "C"]
                }),
                &ctx,
            )
            .await
            .unwrap();

        // Binary results are stored in CAS by the MediaToolAdapter (not by the op itself)
        // but we can verify the data is valid PNG
        if let MediaOpResult::Binary { data, .. } = result {
            assert!(data.len() > 100, "PNG should have meaningful size");
        }
    }

    #[tokio::test]
    async fn chart_custom_dimensions() {
        let (_dir, ctx) = setup().await;
        let op = ChartOp;
        let result = op
            .execute(
                serde_json::json!({
                  "type": "bar",
                  "width": 1200,
                  "height": 800,
                  "series": [{"name": "Test", "data": [5.0, 10.0]}],
                  "labels": ["X", "Y"]
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Binary { metadata, .. } = result {
            assert_eq!(metadata["width"], 1200);
            assert_eq!(metadata["height"], 800);
        }
    }

    #[tokio::test]
    async fn chart_missing_type() {
        let (_dir, ctx) = setup().await;
        let op = ChartOp;
        let result = op
            .execute(
                serde_json::json!({
                  "series": [{"name": "X", "data": [1.0]}]
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn chart_missing_series() {
        let (_dir, ctx) = setup().await;
        let op = ChartOp;
        let result = op
            .execute(
                serde_json::json!({
                  "type": "bar"
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn chart_empty_series() {
        let (_dir, ctx) = setup().await;
        let op = ChartOp;
        let result = op
            .execute(
                serde_json::json!({
                  "type": "bar",
                  "series": []
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[tokio::test]
    async fn chart_invalid_type() {
        let (_dir, ctx) = setup().await;
        let op = ChartOp;
        let result = op
            .execute(
                serde_json::json!({
                  "type": "scatter",
                  "series": [{"name": "X", "data": [1.0]}]
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unsupported"));
    }

    #[tokio::test]
    async fn chart_series_missing_data() {
        let (_dir, ctx) = setup().await;
        let op = ChartOp;
        let result = op
            .execute(
                serde_json::json!({
                  "type": "bar",
                  "series": [{"name": "X"}]
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("data"));
    }

    #[tokio::test]
    async fn chart_series_non_numeric_data() {
        let (_dir, ctx) = setup().await;
        let op = ChartOp;
        let result = op
            .execute(
                serde_json::json!({
                  "type": "bar",
                  "series": [{"name": "X", "data": ["not", "numbers"]}],
                  "labels": ["A", "B"]
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a number"));
    }

    #[tokio::test]
    async fn chart_cancelled_workflow() {
        let (_dir, ctx) = setup().await;
        ctx.cancel.cancel();
        let op = ChartOp;
        let result = op
            .execute(
                serde_json::json!({
                  "type": "bar",
                  "series": [{"name": "X", "data": [1.0]}]
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }

    #[tokio::test]
    async fn chart_rejects_non_finite_values() {
        let (_dir, ctx) = setup().await;
        let op = ChartOp;
        // f64::INFINITY serializes to null in JSON → triggers "not a number"
        let mut args = serde_json::json!({
          "type": "bar",
          "series": [{"name": "X", "data": [0.0]}],
          "labels": ["A"]
        });
        args["series"][0]["data"][0] = serde_json::Value::from(f64::INFINITY);
        let result = op.execute(args, &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a number"));

        // Very large f64 that becomes f32::INFINITY → triggers "not a finite number"
        let result2 = op
            .execute(
                serde_json::json!({
                  "type": "bar",
                  "series": [{"name": "X", "data": [3.5e38]}],
                  "labels": ["A"]
                }),
                &ctx,
            )
            .await;
        assert!(result2.is_err());
        assert!(result2.unwrap_err().to_string().contains("finite"));
    }

    #[tokio::test]
    async fn chart_rejects_non_string_labels() {
        let (_dir, ctx) = setup().await;
        let op = ChartOp;
        let result = op
            .execute(
                serde_json::json!({
                  "type": "bar",
                  "series": [{"name": "X", "data": [1.0, 2.0]}],
                  "labels": [1, 2]
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn chart_rejects_empty_series_data() {
        let (_dir, ctx) = setup().await;
        let op = ChartOp;
        let result = op
            .execute(
                serde_json::json!({
                  "type": "bar",
                  "series": [{"name": "X", "data": []}],
                  "labels": ["A"]
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[tokio::test]
    async fn chart_dimension_clamped() {
        let (_dir, ctx) = setup().await;
        let op = ChartOp;
        // Width/height clamped to 100..4000
        let result = op
            .execute(
                serde_json::json!({
                  "type": "bar",
                  "width": 50,
                  "height": 10000,
                  "series": [{"name": "X", "data": [1.0, 2.0]}],
                  "labels": ["A", "B"]
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Binary { metadata, .. } = result {
            assert_eq!(metadata["width"], 100, "width should be clamped to min 100");
            assert_eq!(
                metadata["height"], 4000,
                "height should be clamped to max 4000"
            );
        }
    }
}
