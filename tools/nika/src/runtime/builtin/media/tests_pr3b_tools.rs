//! Additional tests for PR3b tools: chart, provenance, import edge cases.

#[cfg(test)]
mod tests {
  use std::sync::Arc;
  use crate::media::CasStore;
  use crate::runtime::builtin::media::context::MediaToolContext;
  use crate::runtime::builtin::media::import::ImportOp;
  use crate::runtime::builtin::media::{MediaOp, MediaOpResult, MediaToolAdapter};
  use crate::runtime::builtin::BuiltinTool;

  async fn setup() -> (tempfile::TempDir, Arc<MediaToolContext>) {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())));
    (dir, ctx)
  }

  fn fixture_png(w: u32, h: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
    use image::{ImageBuffer, Rgb};
    let img = ImageBuffer::from_pixel(w, h, Rgb([r, g, b]));
    let mut buf = Vec::new();
    let enc = image::codecs::png::PngEncoder::new(&mut buf);
    image::ImageEncoder::write_image(enc, img.as_raw(), w, h, image::ExtendedColorType::Rgb8).unwrap();
    buf
  }

  fn fixture_jpeg(w: u32, h: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
    use image::{ImageBuffer, Rgb};
    let img = ImageBuffer::from_pixel(w, h, Rgb([r, g, b]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();
    buf.into_inner()
  }

  // ═══════════════════════════════════════════════════════════════
  // IMPORT: MediaToolAdapter integration (tests the full call path)
  // ═══════════════════════════════════════════════════════════════

  #[tokio::test]
  async fn import_via_adapter_returns_json() {
    let (_dir, ctx) = setup().await;

    let png = fixture_png(20, 20, 128, 0, 255);
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), &png).unwrap();

    let adapter = MediaToolAdapter::new(Arc::new(ImportOp), Arc::clone(&ctx));

    let json_str = adapter.call(
      serde_json::json!({"path": tmp.path().to_string_lossy()}).to_string()
    ).await.unwrap();

    let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert!(v["hash"].as_str().unwrap().starts_with("blake3:"));
    assert_eq!(v["mime_type"], "image/png");
    assert!(v["size_bytes"].as_u64().unwrap() > 0);
  }

  #[tokio::test]
  async fn import_via_adapter_invalid_json() {
    let (_dir, ctx) = setup().await;
    let adapter = MediaToolAdapter::new(Arc::new(ImportOp), Arc::clone(&ctx));

    let result = adapter.call("not valid json".to_string()).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("NIKA-294"));
  }

  #[tokio::test]
  async fn import_adapter_name_and_schema() {
    let (_dir, ctx) = setup().await;
    let adapter = MediaToolAdapter::new(Arc::new(ImportOp), Arc::clone(&ctx));

    assert_eq!(adapter.name(), "import");
    assert!(adapter.description().contains("Import"));

    let schema = adapter.parameters_schema();
    let props = schema["properties"].as_object().unwrap();
    assert!(props.contains_key("path"));
    assert!(schema["required"].as_array().unwrap().iter().any(|v| v == "path"));
  }

  // ═══════════════════════════════════════════════════════════════
  // IMPORT: various edge cases
  // ═══════════════════════════════════════════════════════════════

  #[tokio::test]
  async fn import_single_byte_file() {
    let (_dir, ctx) = setup().await;
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), &[0xFF]).unwrap();

    let result = ImportOp.execute(
      serde_json::json!({"path": tmp.path().to_string_lossy()}),
      &ctx,
    ).await.unwrap();

    if let MediaOpResult::Metadata(v) = result {
      assert_eq!(v["size_bytes"], 1);
      // Single byte has no magic → octet-stream
      assert_eq!(v["mime_type"], "application/octet-stream");
    }
  }

  #[tokio::test]
  async fn import_four_bytes_file() {
    let (_dir, ctx) = setup().await;
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), &[0x89, 0x50, 0x4E, 0x47]).unwrap(); // PNG magic but truncated

    let result = ImportOp.execute(
      serde_json::json!({"path": tmp.path().to_string_lossy()}),
      &ctx,
    ).await.unwrap();

    if let MediaOpResult::Metadata(v) = result {
      assert_eq!(v["size_bytes"], 4);
    }
  }

  #[tokio::test]
  async fn import_binary_data_no_magic() {
    let (_dir, ctx) = setup().await;
    let data: Vec<u8> = (0..100).map(|i| (i * 7 + 13) as u8).collect();
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), &data).unwrap();

    let result = ImportOp.execute(
      serde_json::json!({"path": tmp.path().to_string_lossy()}),
      &ctx,
    ).await.unwrap();

    if let MediaOpResult::Metadata(v) = result {
      assert_eq!(v["mime_type"], "application/octet-stream");
    }
  }

  #[tokio::test]
  async fn import_wasm_detected() {
    let (_dir, ctx) = setup().await;
    // WASM magic: \0asm
    let mut wasm = vec![0x00, 0x61, 0x73, 0x6D];
    wasm.extend_from_slice(&[1, 0, 0, 0]); // version 1
    wasm.extend_from_slice(&[0u8; 50]);

    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), &wasm).unwrap();

    let result = ImportOp.execute(
      serde_json::json!({"path": tmp.path().to_string_lossy()}),
      &ctx,
    ).await.unwrap();

    if let MediaOpResult::Metadata(v) = result {
      assert_eq!(v["mime_type"], "application/wasm");
    }
  }

  #[tokio::test]
  async fn import_tiff_detected() {
    let (_dir, ctx) = setup().await;
    // TIFF little-endian magic: II + 42
    let mut tiff = vec![0x49, 0x49, 0x2A, 0x00];
    tiff.extend_from_slice(&[0u8; 50]);

    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), &tiff).unwrap();

    let result = ImportOp.execute(
      serde_json::json!({"path": tmp.path().to_string_lossy()}),
      &ctx,
    ).await.unwrap();

    if let MediaOpResult::Metadata(v) = result {
      assert_eq!(v["mime_type"], "image/tiff");
    }
  }

  #[tokio::test]
  async fn import_bmp_detected() {
    let (_dir, ctx) = setup().await;
    // BMP magic: BM
    let mut bmp = vec![0x42, 0x4D];
    bmp.extend_from_slice(&100u32.to_le_bytes()); // file size
    bmp.extend_from_slice(&[0u8; 50]);

    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), &bmp).unwrap();

    let result = ImportOp.execute(
      serde_json::json!({"path": tmp.path().to_string_lossy()}),
      &ctx,
    ).await.unwrap();

    if let MediaOpResult::Metadata(v) = result {
      assert_eq!(v["mime_type"], "image/bmp");
    }
  }

  #[tokio::test]
  async fn import_svg_as_octet_stream() {
    let (_dir, ctx) = setup().await;
    // SVG is text-based — no magic bytes
    let svg = b"<svg xmlns='http://www.w3.org/2000/svg'><rect/></svg>";
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), svg).unwrap();

    let result = ImportOp.execute(
      serde_json::json!({"path": tmp.path().to_string_lossy()}),
      &ctx,
    ).await.unwrap();

    if let MediaOpResult::Metadata(v) = result {
      // SVG has no magic bytes — infer won't detect it
      // It may detect as XML or fall back to octet-stream
      let mime = v["mime_type"].as_str().unwrap();
      assert!(
        mime == "application/octet-stream" || mime == "text/xml" || mime == "image/svg+xml",
        "SVG should be detected as xml or octet-stream, got: {mime}"
      );
    }
  }

  #[tokio::test]
  async fn import_multiple_different_files() {
    let (_dir, ctx) = setup().await;

    let formats = vec![
      ("png", fixture_png(10, 10, 255, 0, 0)),
      ("jpeg", fixture_jpeg(10, 10, 0, 255, 0)),
    ];

    let mut hashes = Vec::new();

    for (name, data) in &formats {
      let tmp = tempfile::NamedTempFile::new().unwrap();
      std::fs::write(tmp.path(), data).unwrap();

      let result = ImportOp.execute(
        serde_json::json!({"path": tmp.path().to_string_lossy()}),
        &ctx,
      ).await.unwrap();

      if let MediaOpResult::Metadata(v) = result {
        let hash = v["hash"].as_str().unwrap().to_string();
        hashes.push((name.to_string(), hash));
      }
    }

    // Different content should produce different hashes
    assert_ne!(hashes[0].1, hashes[1].1, "PNG and JPEG should have different hashes");

    // All should be readable from CAS
    for (_, hash) in &hashes {
      let data = ctx.read_media(hash).await;
      assert!(data.is_ok(), "should be able to read back {hash}");
    }
  }

  // ═══════════════════════════════════════════════════════════════
  // CHART: edge cases (feature-gated)
  // ═══════════════════════════════════════════════════════════════

  #[cfg(feature = "media-chart")]
  mod chart_tests {
    use super::*;
    use crate::runtime::builtin::media::chart::ChartOp;

    #[tokio::test]
    async fn chart_single_value_series() {
      let (_dir, ctx) = setup().await;
      let result = ChartOp.execute(serde_json::json!({
        "type": "bar",
        "series": [{"name": "Single", "data": [42.0]}],
        "labels": ["Only"]
      }), &ctx).await.unwrap();

      if let MediaOpResult::Binary { data, .. } = result {
        assert_eq!(&data[..4], &[0x89, 0x50, 0x4E, 0x47]);
      }
    }

    #[tokio::test]
    async fn chart_many_series() {
      let (_dir, ctx) = setup().await;
      let series: Vec<_> = (0..10).map(|i| {
        serde_json::json!({"name": format!("Series {i}"), "data": [i as f64, (i*2) as f64]})
      }).collect();

      let result = ChartOp.execute(serde_json::json!({
        "type": "bar",
        "series": series,
        "labels": ["A", "B"]
      }), &ctx).await.unwrap();

      if let MediaOpResult::Binary { data, .. } = result {
        assert!(!data.is_empty());
      }
    }

    #[tokio::test]
    async fn chart_negative_values() {
      let (_dir, ctx) = setup().await;
      let result = ChartOp.execute(serde_json::json!({
        "type": "line",
        "series": [{"name": "Temp", "data": [-10.0, -5.0, 0.0, 5.0, 10.0]}],
        "labels": ["Jan", "Feb", "Mar", "Apr", "May"]
      }), &ctx).await.unwrap();

      if let MediaOpResult::Binary { data, .. } = result {
        assert_eq!(&data[..4], &[0x89, 0x50, 0x4E, 0x47]);
      }
    }

    #[tokio::test]
    async fn chart_float_values() {
      let (_dir, ctx) = setup().await;
      let result = ChartOp.execute(serde_json::json!({
        "type": "pie",
        "series": [
          {"name": "A", "data": [33.33]},
          {"name": "B", "data": [66.67]}
        ]
      }), &ctx).await.unwrap();

      if let MediaOpResult::Binary { data, .. } = result {
        assert!(!data.is_empty());
      }
    }

    #[tokio::test]
    async fn chart_zero_values() {
      let (_dir, ctx) = setup().await;
      let result = ChartOp.execute(serde_json::json!({
        "type": "bar",
        "series": [{"name": "Zero", "data": [0.0, 0.0, 0.0]}],
        "labels": ["A", "B", "C"]
      }), &ctx).await.unwrap();

      if let MediaOpResult::Binary { data, .. } = result {
        assert!(!data.is_empty());
      }
    }

    #[tokio::test]
    async fn chart_large_values() {
      let (_dir, ctx) = setup().await;
      let result = ChartOp.execute(serde_json::json!({
        "type": "bar",
        "series": [{"name": "Big", "data": [1000000.0, 5000000.0, 10000000.0]}],
        "labels": ["Q1", "Q2", "Q3"]
      }), &ctx).await.unwrap();

      if let MediaOpResult::Binary { data, .. } = result {
        assert!(!data.is_empty());
      }
    }

    #[tokio::test]
    async fn chart_via_adapter() {
      let (_dir, ctx) = setup().await;
      let adapter = MediaToolAdapter::new(Arc::new(ChartOp), Arc::clone(&ctx));

      let json_str = adapter.call(serde_json::json!({
        "type": "pie",
        "series": [
          {"name": "Yes", "data": [70.0]},
          {"name": "No", "data": [30.0]}
        ]
      }).to_string()).await.unwrap();

      let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
      assert!(v["hash"].as_str().unwrap().starts_with("blake3:"));
      assert_eq!(v["mime_type"], "image/png");
    }

    #[tokio::test]
    async fn chart_with_title() {
      let (_dir, ctx) = setup().await;
      let result = ChartOp.execute(serde_json::json!({
        "type": "line",
        "title": "Revenue Growth 2026",
        "series": [{"name": "Revenue", "data": [100.0, 150.0, 200.0, 250.0]}],
        "labels": ["Q1", "Q2", "Q3", "Q4"]
      }), &ctx).await.unwrap();

      if let MediaOpResult::Binary { metadata, .. } = result {
        assert_eq!(metadata["chart_type"], "line");
      }
    }

    #[tokio::test]
    async fn chart_pie_many_slices() {
      let (_dir, ctx) = setup().await;
      let series: Vec<_> = (0..20).map(|i| {
        serde_json::json!({"name": format!("Slice {i}"), "data": [(i + 1) as f64]})
      }).collect();

      let result = ChartOp.execute(serde_json::json!({
        "type": "pie",
        "series": series
      }), &ctx).await.unwrap();

      if let MediaOpResult::Binary { data, .. } = result {
        assert!(!data.is_empty());
      }
    }

    #[tokio::test]
    async fn chart_empty_labels() {
      let (_dir, ctx) = setup().await;
      // Bar chart with no labels — should still work
      let result = ChartOp.execute(serde_json::json!({
        "type": "bar",
        "series": [{"name": "X", "data": [1.0, 2.0]}],
        "labels": []
      }), &ctx).await.unwrap();

      if let MediaOpResult::Binary { data, .. } = result {
        assert!(!data.is_empty());
      }
    }
  }

  // ═══════════════════════════════════════════════════════════════
  // PROVENANCE: edge cases (feature-gated)
  // ═══════════════════════════════════════════════════════════════

  #[cfg(feature = "media-provenance")]
  mod provenance_tests {
    use super::*;
    use crate::runtime::builtin::media::provenance::ProvenanceOp;

    #[tokio::test]
    async fn provenance_ai_modified() {
      let (_dir, ctx) = setup().await;
      let jpeg = fixture_jpeg(50, 50, 0, 128, 255);
      let sr = ctx.cas.store(&jpeg).await.unwrap();

      let result = ProvenanceOp.execute(serde_json::json!({
        "hash": sr.hash,
        "assertion": "ai.modified"
      }), &ctx).await.unwrap();

      if let MediaOpResult::Binary { metadata, .. } = result {
        assert_eq!(metadata["assertion"], "ai.modified");
      }
    }

    #[tokio::test]
    async fn provenance_with_custom_title() {
      let (_dir, ctx) = setup().await;
      let png = fixture_png(30, 30, 255, 128, 0);
      let sr = ctx.cas.store(&png).await.unwrap();

      let result = ProvenanceOp.execute(serde_json::json!({
        "hash": sr.hash,
        "assertion": "human.created",
        "title": "My Custom Art Piece"
      }), &ctx).await.unwrap();

      if let MediaOpResult::Binary { metadata, .. } = result {
        assert_eq!(metadata["title"], "My Custom Art Piece");
      }
    }

    #[tokio::test]
    async fn provenance_via_adapter() {
      let (_dir, ctx) = setup().await;
      let jpeg = fixture_jpeg(40, 40, 128, 128, 128);
      let sr = ctx.cas.store(&jpeg).await.unwrap();

      let adapter = MediaToolAdapter::new(Arc::new(ProvenanceOp), Arc::clone(&ctx));
      let json_str = adapter.call(serde_json::json!({
        "hash": sr.hash,
        "assertion": "ai.generated"
      }).to_string()).await.unwrap();

      let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
      assert!(v["hash"].as_str().unwrap().starts_with("blake3:"));
      assert_eq!(v["metadata"]["signed"], true);
    }

    #[tokio::test]
    async fn provenance_signed_larger_than_original() {
      let (_dir, ctx) = setup().await;
      let png = fixture_png(100, 100, 0, 0, 255);
      let original_size = png.len();
      let sr = ctx.cas.store(&png).await.unwrap();

      let result = ProvenanceOp.execute(serde_json::json!({
        "hash": sr.hash,
        "assertion": "ai.generated"
      }), &ctx).await.unwrap();

      if let MediaOpResult::Binary { data, .. } = result {
        assert!(data.len() > original_size,
          "signed image ({}) should be larger than original ({original_size})", data.len());
      }
    }

    #[tokio::test]
    async fn provenance_name_and_schema() {
      let op = ProvenanceOp;
      assert_eq!(op.name(), "provenance");
      assert!(op.description().contains("C2PA"));

      let schema = op.parameters_schema();
      let required = schema["required"].as_array().unwrap();
      assert!(required.iter().any(|v| v == "hash"));
      assert!(required.iter().any(|v| v == "assertion"));
    }
  }

  // ═══════════════════════════════════════════════════════════════
  // IMPORT→PROVENANCE pipeline (feature-gated)
  // ═══════════════════════════════════════════════════════════════

  #[cfg(feature = "media-provenance")]
  #[tokio::test]
  async fn pipeline_import_then_provenance() {
    use crate::runtime::builtin::media::provenance::ProvenanceOp;

    let (_dir, ctx) = setup().await;

    // Import a JPEG
    let jpeg = fixture_jpeg(80, 80, 255, 0, 128);
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), &jpeg).unwrap();

    let import_result = ImportOp.execute(
      serde_json::json!({"path": tmp.path().to_string_lossy()}),
      &ctx,
    ).await.unwrap();

    let hash = if let MediaOpResult::Metadata(v) = import_result {
      v["hash"].as_str().unwrap().to_string()
    } else {
      panic!("expected Metadata");
    };

    // Sign with provenance
    let prov_result = ProvenanceOp.execute(serde_json::json!({
      "hash": hash,
      "assertion": "ai.generated",
      "title": "Workflow Output"
    }), &ctx).await.unwrap();

    if let MediaOpResult::Binary { data, mime_type, metadata, .. } = prov_result {
      assert_eq!(mime_type, "image/jpeg");
      assert!(data.len() > jpeg.len());
      assert_eq!(metadata["signed"], true);
      assert_eq!(metadata["assertion"], "ai.generated");
    }
  }

  // ═══════════════════════════════════════════════════════════════
  // IMPORT→CHART pipeline (feature-gated)
  // ═══════════════════════════════════════════════════════════════

  #[cfg(feature = "media-chart")]
  #[tokio::test]
  async fn chart_output_can_be_read_from_cas_via_adapter() {
    use crate::runtime::builtin::media::chart::ChartOp;

    let (_dir, ctx) = setup().await;

    let adapter = MediaToolAdapter::new(Arc::new(ChartOp), Arc::clone(&ctx));

    let json_str = adapter.call(serde_json::json!({
      "type": "bar",
      "series": [{"name": "Test", "data": [10.0, 20.0, 30.0]}],
      "labels": ["A", "B", "C"]
    }).to_string()).await.unwrap();

    let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    let hash = v["hash"].as_str().unwrap();

    // Read the chart PNG from CAS
    let png_data = ctx.read_media(hash).await.unwrap();
    assert_eq!(&png_data[..4], &[0x89, 0x50, 0x4E, 0x47], "stored data should be PNG");
    assert!(png_data.len() > 100);
  }

  // ═══════════════════════════════════════════════════════════════
  // create_media_tool_adapters: registration count with features
  // ═══════════════════════════════════════════════════════════════

  #[test]
  fn import_is_in_tool_list() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())));
    let tools = crate::runtime::builtin::media::create_media_tool_adapters(ctx);

    let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    assert!(names.contains(&"import"), "import should be in tool list: {:?}", names);

    // import should be first (before dimensions)
    assert_eq!(names[0], "import", "import should be first tool");
  }

  #[test]
  fn all_tool_names_unique() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())));
    let tools = crate::runtime::builtin::media::create_media_tool_adapters(ctx);

    let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(names.len(), sorted.len(), "tool names must be unique: {:?}", names);
  }

  #[test]
  fn all_tools_have_description() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())));
    let tools = crate::runtime::builtin::media::create_media_tool_adapters(ctx);

    for tool in &tools {
      assert!(!tool.description().is_empty(),
        "tool '{}' should have a description", tool.name());
    }
  }

  #[test]
  fn all_tools_have_valid_schema() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())));
    let tools = crate::runtime::builtin::media::create_media_tool_adapters(ctx);

    for tool in &tools {
      let schema = tool.parameters_schema();
      assert!(schema.is_object(),
        "tool '{}' schema should be an object, got: {:?}", tool.name(), schema);
      assert_eq!(schema["type"], "object",
        "tool '{}' schema type should be 'object'", tool.name());
    }
  }
}
