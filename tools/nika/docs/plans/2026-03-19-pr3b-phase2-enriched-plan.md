# PR3b Phase 2 — Enriched Plan

> Start in a NEW Claude Code session. Context: v0.34.0 on main, 13 media tools, 6002 tests.

## Priority 1: nika media import (UNBLOCKS EVERYTHING)

Add `nika media import <file>` CLI command:
- Reads local file, stores in CAS, prints hash
- 50 LOC in src/cli/media.rs
- Unblocks: audio, video, PDF, any file → CAS → media tools

```rust
// src/cli/media.rs — add Import variant
Import {
    #[arg(help = "File to import")]
    file: PathBuf,
},
// Handler: read file, call store.store(&data), print hash
```

Also add `nika:import` builtin tool (for workflows):
```yaml
- id: imp
  invoke: { tool: nika:import, params: { path: "/path/to/file.mp3" } }
# Returns: {"hash": "blake3:...", "mime_type": "audio/mpeg", "size_bytes": 1234}
```

## Priority 2: Vision support (content_blocks in infer:)

AST change: add `content:` field to InferParams:
```yaml
- id: describe
  infer:
    prompt: "Describe this image"
    content:
      - type: image
        source: "{{with.img.media[0].hash}}"  # CAS hash
```

Executor change: build multimodal content blocks from CAS data.
Provider support: Claude (native), OpenAI (base64 in messages), Gemini (inline_data).

## Priority 3: PR3b remaining tools

### C2: nika:chart (charts-rs 0.3)
- JSON config → PNG/SVG chart
- Feature: media-chart

### C6: nika:provenance (c2pa 0.78)
- C2PA content credentials
- Feature: media-provenance
- Note: requires rust_native_crypto feature (no OpenSSL dep)

### C9: CAS compression (zstd 0.13)
- Compress non-media blobs in CAS
- Transparent: store compressed, decompress on read

## Priority 4: Crates to investigate

### Audio
- symphonia 0.5 — pure Rust audio decoder (MP3, FLAC, WAV, OGG)
- rodio — audio playback (not needed for processing)

### Video
- mp4 0.14 — MP4 container parsing (duration, tracks, codec)
- ffmpeg-sidecar — FFmpeg CLI wrapper (heavy but complete)
- video-rs — Rust video processing (wraps ffmpeg)

### ML/Vision
- ort 2.0 — ONNX Runtime for Rust (inference engine)
- candle 0.8 — Hugging Face ML framework for Rust
- ocrs 0.12 + rten 0.24 — pure Rust OCR (no external deps)

### Mistral native
- Already integrated via mistralrs 0.7 (native-inference feature)
- Supports GGUF models locally
- Can be used with: provider: native

### Image processing
- kornia-rs — pure Rust CV (3-5x faster than image-rs for some ops)
- quantette — color quantization without GPL
- jxl-rs 0.4 — JPEG XL decoder

### Document
- pdf_oxide — pure Rust PDF parser (alternative to pdf-extract)
- krilla — PDF generation from Rust
- calamine — Excel/ODS spreadsheet reading

## Priority 5: Telemetry improvements

- Add MediaToolStarted/MediaToolCompleted events
- Add budget_remaining to telemetry events
- Add compute_pool queue depth metric
- Track per-tool latency percentiles

## Priority 6: Performance

- Use fast_image_resize SIMD (currently unused dep!)
- Initialize rayon global pool at startup (cap oxipng threads)
- Pre-warm fontdb on startup (not on first SVG)
- Cache decoded images in WorkingMemory for pipeline reuse

## Implementation order

```
Session N+1:
  1. nika media import (CLI + builtin tool) — 1h
  2. Test with real audio/video/PDF files — 30min
  3. nika:chart (charts-rs) — 2h
  4. nika:provenance (c2pa) — 2h
  5. CAS zstd compression — 1h
  6. Tests + audit — 1h

Session N+2:
  7. Vision content_blocks in infer: — 3h
  8. nika:ocr (ocrs + rten) — 2h
  9. Performance: fast_image_resize SIMD — 1h
  10. Rig agent loop media tool registration — 1h
```

## Prompt for next session

```
Read docs/plans/2026-03-19-pr3b-phase2-enriched-plan.md

We're on main, v0.34.0. 13 media tools, 6002 tests, 3 providers verified.

Start with Priority 1: nika media import.
Add Import subcommand to src/cli/media.rs.
Add nika:import builtin tool to src/runtime/builtin/media/.
Test with real audio (download MP3), video (download MP4), PDF files.
Then continue with Priority 3 (chart, provenance, compression).

TDD. Quality gates. Commit per feature.
```
