# PR4 — Vision + Innovations Master Plan v2.0

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add multimodal vision support to `infer:`, QR validation, CAS compression, and cutting-edge differentiators that position Nika ahead of every agentic framework.

**Architecture:** Four phases. Phase 1 threads `content:` through the AST pipeline and lights up vision across 6 providers. Phase 2 adds QR validate + CAS compression. Phase 3 adds gemmes rares (IQA, C2PA verify, JPEG XL, watermark). Phase 4 is the monster verification wave.

**Tech Stack:** rig-core 0.32 (native multimodal), rxing 0.8.5, zstd 0.13, jxl-oxide 0.12, butteraugli 0.7, blind_watermark 0.1, dssim 3.4, c2pa 0.78 Reader API

**Baseline:** 6054 tests (default), 6129 (all features), 282,884 LOC, 0 clippy warnings

---

## Research Synthesis (18 Opus agents, 2 sessions)

### Architecture Ratings
| Layer | Rating | Notes |
|-------|--------|-------|
| MediaOp Trait + Adapter | **ELEGANT** | Best tool abstraction in any agentic framework |
| CAS (blake3 + O_EXCL + shard) | **ELEGANT** | Ahead of LangChain/CrewAI/AutoGen (they have none) |
| Compute Pool (rayon + oneshot) | **GOOD** | Correct pattern, caps at 4 threads |
| Security (5 defense layers) | **ELEGANT** | No gaps found by 3 independent reviewers |
| Provider Architecture | **ADEQUATE** | 28+ identical match arms, text-only — needs refactor |

### Key Discoveries
- **rig-core 0.32 has FULL vision support** — `UserContent::image_base64()` works natively. No HTTP bypass needed.
- **Template resolution already works** — `{{with.photo.media[0].hash}}` resolves through existing binding engine.
- **EU AI Act Article 50 deadline: August 2, 2026** — C2PA is de facto standard (500+ member companies).
- **No competitor combines CAS + Vision + C2PA** — Nika's moat is real and unique.
- **Mistral/Pixtral also supports vision** — 4 provider formats total (Anthropic, OpenAI-compat, Gemini, Mistral).

### Competitive Position (updated with Google ADK)
| Feature | Nika | LangChain | CrewAI | AutoGen | Dify | Google ADK |
|---------|:----:|:---------:|:------:|:-------:|:----:|:----------:|
| CAS media storage | YES | no | no | no | no | no |
| C2PA provenance | YES | no | no | no | no | no |
| C2PA verification | **PR4** | no | no | no | no | no |
| Pipeline composition | YES | partial | no | no | partial | no |
| Vision in workflows | **PR4** | yes | no | yes | yes | yes |
| CAS→Vision bridge | **PR4** | no | no | no | no | no |
| Image quality scoring | **PR4** | no | no | no | no | no |
| EU AI Act compliance | **PR4** | no | no | no | no | partial |
| Invisible watermarking | **PR4** | no | no | no | no | no |

---

## Phase 1: Vision Support (P0 — Critical)

### Task 1.0: Define ContentPart types

**Files:**
- Create: `src/ast/content.rs`
- Modify: `src/ast/mod.rs` — add `pub mod content;`

```rust
/// Raw content part (with spans, from YAML parser)
#[derive(Debug, Clone)]
pub enum RawContentPart {
  Text { text: Spanned<String> },
  Image { source: Spanned<String>, detail: Option<Spanned<String>> },
  ImageUrl { url: Spanned<String>, detail: Option<Spanned<String>> },
}

/// Analyzed content part (spans stripped, validated)
#[derive(Debug, Clone)]
pub enum AnalyzedContentPart {
  Text { text: String },
  Image { source: String, detail: ImageDetail },
  ImageUrl { url: String, detail: ImageDetail },
}

/// Runtime content part (used in InferParams)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
  Text { text: String },
  Image { source: String, #[serde(default)] detail: ImageDetail },
  ImageUrl { url: String, #[serde(default)] detail: ImageDetail },
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageDetail { #[default] Auto, Low, High }
```

**Tests:** Unit tests for serde round-trip, Display impl, Default.

**Commit:** `feat(ast): add ContentPart types for multimodal vision support`

### Task 1.1: Thread content through AST pipeline

**Files (exact line numbers from architecture mapper):**
- `src/ast/raw/action.rs:60-79` — Add `content: Option<Spanned<Vec<RawContentPart>>>` to `RawInferAction`
- `src/ast/raw/parser.rs:437-477` — Parse `content:` array in `parse_infer_action`, make `prompt:` optional when `content:` present
- `src/ast/analyzed/task.rs:129-151` — Add `content: Option<Vec<AnalyzedContentPart>>` to `AnalyzedInferAction`
- `src/ast/analyzer/analyze.rs:670-680` — Map raw→analyzed content parts in `analyze_infer`
- `src/ast/lower.rs:162-178` — Lower content in `lower_infer`, add to `unlower_action` at line 586

**Decision: `prompt:` vs `content:` mutual exclusivity:**
- When `content:` present → `prompt:` becomes optional (sugar for single text part)
- If both present → `prompt:` prepended as first Text part
- If neither → validation error

**Tests:**
- Parse `content:` YAML with text + image parts
- Parse shorthand `infer: "prompt"` still works
- Parse full `infer: { prompt: "...", content: [...] }` — prompt prepended
- Error: neither prompt nor content
- Error: content with invalid type field

**Commit:** `feat(ast): thread content: field through raw → analyzed → lower pipeline`

### Task 1.2: Update runtime InferParams

**Files:**
- `src/ast/action.rs:57-207` — Add `content: Option<Vec<ContentPart>>` to `InferParams`
- Update `Deserialize` impl (InferParamsHelper)
- Update `validate()` — prompt optional when content present

**Tests:**
- Deserialize InferParams with content field
- Validate: content present, no prompt → OK
- Validate: prompt present, no content → OK (backward compat)
- Validate: neither → error

**Commit:** `feat(ast): add content to runtime InferParams with validation`

### Task 1.3: Add VisionContentResolved telemetry event

**Files:**
- `src/event/log.rs:~500` — Add `VisionContentResolved` variant
  - Fields: `task_id: String, image_count: u32, total_bytes: u64, resolve_ms: u64`
- Tests: serialization round-trip

**Commit:** `feat(event): add VisionContentResolved telemetry event`

### Task 1.4: Implement vision provider methods

**Files:**
- `src/provider/rig.rs:339+` — Add `infer_vision()` and `infer_vision_stream()` methods
- New helper: `build_vision_message(parts: &[ContentPart], image_data: &[(String, Vec<u8>)]) -> Result<Message>`

**Provider-specific notes:**
- **Anthropic:** `media_type` REQUIRED for base64. Uses `Content::Image { source: ImageSource::Base64 }`
- **OpenAI/xAI/Groq:** `media_type` + `detail` REQUIRED. Builds `data:image/png;base64,...` data URI
- **Mistral/Pixtral:** OpenAI-compatible format
- **Gemini:** `inlineData` with `mimeType` and base64 `data`
- **DeepSeek:** No vision → return `NikaError::VisionNotSupported`
- **Native:** No vision → return error

**Key: Always base64-encode. `DocumentSourceKind::Raw` is rejected by both Anthropic and OpenAI.**

**Tests:**
- Build vision message with text + image → verify UserContent types
- Error: unsupported MIME type
- Error: vision on DeepSeek → clear error message

**Commit:** `feat(provider): add infer_vision + infer_vision_stream for multimodal`

### Task 1.5: Wire vision into executor

**Files:**
- `src/runtime/executor/verbs.rs:44-464` — Add vision code path in `run_infer`

**Logic:**
```
if infer_params.content.is_some() {
  1. Resolve templates in content[*].source
  2. For each Image part: read CAS → base64 encode
  3. Emit VisionContentResolved event (timing, image count, bytes)
  4. Build multimodal Message via build_vision_message()
  5. Call provider.infer_vision() or provider.infer_vision_stream()
} else {
  // Existing text path (unchanged)
}
```

**Tests:**
- E2E: import PNG → infer with content → verify provider receives multimodal Message
- Template resolution: `{{with.photo.media[0].hash}}` → blake3 hash → CAS bytes → base64
- Error: CAS hash not found in content image
- Error: content image is not a supported format
- Cancellation during CAS resolution
- Streaming vision response

**Commit:** `feat(runtime): wire vision content resolution into infer executor`

### Task 1.6: Integration tests + example workflows

**Files:**
- Create: `src/ast/raw/tests_vision_parser.rs`
- Create: example workflow `examples/vision-describe.nika.yaml`

**Example workflow:**
```yaml
name: Describe Image
tasks:
  import_photo:
    invoke: nika:import
    args:
      path: ./photo.jpg

  describe:
    infer:
      model: claude-sonnet-4-6
      content:
        - type: image
          source: "{{with.photo.media[0].hash}}"
          detail: high
        - type: text
          text: "Describe this image. List all objects, colors, and the overall mood."
    depends_on: [import_photo]
    with:
      photo: $import_photo
    output:
      schema:
        type: object
        properties:
          description: { type: string }
          objects: { type: array, items: { type: string } }
          mood: { type: string }
```

**Commit:** `test(vision): parser tests + vision-describe example workflow`

---

## Phase 2: QR Validate + CAS Compression

### Task 2.1: nika:qr_validate (rxing 0.8.5)

**Files:**
- Create: `src/runtime/builtin/media/qr.rs`
- Modify: `src/runtime/builtin/media/mod.rs` — register under `media-qr`
- Modify: `Cargo.toml` — add `rxing = { version = "0.8", optional = true, default-features = false, features = ["qrcode"] }`

```rust
pub struct QrValidateOp;
// Input: { hash: "blake3:..." }
// Output: {
//   decoded: true, data: "https://qrcode-ai.com/...",
//   format: "QR_CODE", error_correction: "H",
//   module_count: 29, scan_score: 85,
//   barcodes_found: 1
// }
```

**Scan score algorithm:**
1. Decode attempt → base score (0 if fail, 50 if success)
2. Error correction level bonus: L+0, M+10, Q+20, H+30
3. Module contrast ratio (0-20 scale): analyze luminance difference between light/dark modules
4. Result capped at 100

**Tests (12+):**
- Decode valid QR code (generate with `qrcode` crate in test fixture)
- Decode from CAS hash (full adapter path)
- Multiple QR codes in one image
- Error: not a QR code (random image)
- Error: empty image
- Error: non-image data
- Fuzz: no panic on garbage
- Cancellation test
- Router dispatch test
- Adapter test
- Scan score varies with error correction level
- Pipeline: import → qr_validate

**Commit:** `feat(media): add nika:qr_validate — QR decode + scan scoring [media-qr]`

### Task 2.2: CAS zstd compression

**Files:**
- Modify: `src/media/store.rs` — transparent compress/decompress
- Modify: `Cargo.toml` — add `zstd = { version = "0.13", optional = true, default-features = false }`
- Add feature: `media-compression = ["dep:zstd"]`

**Design (validated by benchmarks: 3.5x on JSON, 1x passthrough on images):**
```rust
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];
const ZSTD_LEVEL: i32 = 3;  // optimal speed/ratio

// On store(): hash ORIGINAL → compress if text/json/svg → write
fn should_compress(data: &[u8]) -> bool {
  let mime = infer::get(data).map(|t| t.mime_type());
  !matches!(mime, Some(m) if m.starts_with("image/") || m.starts_with("audio/") || m.starts_with("video/"))
}

// On read(): detect magic → decompress if needed
fn transparent_read(data: Vec<u8>) -> Result<Vec<u8>> {
  if data.len() >= 4 && data[..4] == ZSTD_MAGIC {
    zstd::decode_all(Cursor::new(&data))
  } else {
    Ok(data)
  }
}
```

**Tests (10+):**
- Store + read JSON → round-trip exact
- Store + read PNG → passthrough (no compression)
- Store text, verify on-disk is smaller
- Dedup still works with compression
- Budget charged on original size (not compressed)
- Concurrent compress/decompress no race
- Already-compressed data not double-compressed
- Magic bytes detection edge cases
- Feature gate: works without media-compression (no compression applied)

**Commit:** `feat(media): CAS zstd compression for non-media blobs [media-compression]`

---

## Phase 3: Gemmes Rares (Differentiators)

### Task 3.1: nika:quality — Image Quality Assessment

**Files:**
- Create: `src/runtime/builtin/media/quality.rs`
- Use `dssim 3.4` for SSIM + `butteraugli 0.7` for perceptual quality

```rust
pub struct QualityOp;
// Input: { hash_a: "blake3:...", hash_b: "blake3:..." }  // original vs processed
// Output: { ssim: 0.95, dssim: 0.012, butteraugli_score: 1.2, quality_grade: "excellent" }
// Grades: excellent (<1.0), good (1.0-2.0), acceptable (2.0-4.0), poor (>4.0)
```

**Tests:** 8+ including identical images (SSIM=1.0), visually different, JPEG compression artifacts.

**Commit:** `feat(media): add nika:quality — image quality assessment [media-iqa]`

### Task 3.2: nika:verify — C2PA Manifest Verification

**Files:**
- Create: `src/runtime/builtin/media/verify.rs`
- Uses `c2pa::Reader::from_stream()` (proven in our readback tests)

```rust
pub struct VerifyOp;
// Input: { hash: "blake3:..." }
// Output: {
//   has_manifest: true, title: "...",
//   claim_generator: "Nika v0.34.0",
//   assertions: [{ label: "c2pa.actions", ... }],
//   digital_source_type: "trainedAlgorithmicMedia",
//   validation_status: "valid",  // or "invalid", "unknown"
//   eu_ai_act_compliant: true
// }
```

**EU AI Act compliance check:**
- Has C2PA manifest? ✓
- Has digital source type assertion? ✓
- Source type indicates AI involvement? ✓ (if ai.generated or ai.modified)

**Tests:** 10+ including sign→verify round-trip, unsigned image, corrupt manifest, all assertion types.

**Commit:** `feat(media): add nika:verify — C2PA verification + EU AI Act compliance [media-provenance]`

### Task 3.3: nika:convert — JPEG XL support

**Files:**
- Modify: `src/runtime/builtin/media/convert.rs` — add JXL decode + encode
- Use `jxl-oxide 0.12` for decode (LGPL-3.0 — OK for dynamic linking)
- Encoding: use `image` crate JXL feature if available, or defer encoding to PR5

**Note:** `jxl-encoder` is AGPL-3.0 — evaluate license before adopting for encoding. Decoding is safe (LGPL).

**Tests:** 6+ including JXL decode, format detection, round-trip.

**Commit:** `feat(media): JPEG XL decode support in nika:convert [media-jxl]`

### Task 3.4: nika:watermark — Invisible Watermarking

**Files:**
- Create: `src/runtime/builtin/media/watermark.rs`
- Use `blind_watermark 0.1.2` (DWT+DCT+SVD, MIT/Apache)

```rust
pub struct WatermarkEmbedOp;
// Input: { hash: "blake3:...", text: "nika-workflow-abc123" }
// Output: Binary (watermarked image, visually identical)

pub struct WatermarkExtractOp;
// Input: { hash: "blake3:...", length: 20 }
// Output: { text: "nika-workflow-abc123", confidence: 0.95 }
```

**Tests:** 8+ including embed→extract round-trip, survives JPEG compression, survives resize.

**Commit:** `feat(media): add nika:watermark — invisible DWT+DCT+SVD watermarking [media-watermark]`

---

## Phase 4: Monster Verification Wave

### Task 4.1: Full regression test battery

Run after each phase:
```bash
cargo test --lib -q                                                    # default
cargo test --lib --features "media-chart,media-phash,media-provenance,media-pdf,media-qr,media-compression,media-iqa,media-jxl,media-watermark" -q  # all features
cargo test --bin nika -q                                               # CLI
cargo clippy --all-features -- -D warnings                             # lint
cargo check --no-default-features --features media-chart               # isolated
cargo check --no-default-features --features media-phash               # isolated
cargo check --no-default-features --features media-provenance          # isolated
cargo check --no-default-features --features media-pdf                 # isolated
cargo check --no-default-features --features media-qr                  # isolated
cargo check --no-default-features --features media-compression         # isolated
```

### Task 4.2: Cross-tool pipeline E2E tests

```
import → chart → provenance → verify (sign chart, verify signature)
import → qr_validate → infer with vision (describe QR content)
import → quality → optimize → quality (before/after comparison)
import → watermark → compare (verify visual similarity after watermark)
import → thumbnail → convert(jxl) → dimensions (JPEG XL pipeline)
```

### Task 4.3: Telemetry + output architecture verification

- Every new tool emits start/success/failure via executor-level McpInvoke/McpResponse
- VisionContentResolved event emitted with timing data
- NDJSON trace output parseable by `nika trace` command
- JSON-LD output format for structured data (future)

### Task 4.4: Security hardening review

- Vision: base64 encoding doesn't leak CAS paths in API calls
- QR: no code execution from decoded QR data
- Watermark: extraction doesn't reveal hidden data from untrusted images
- Compression: zstd decompression bomb protection (max decompressed size)

### Task 4.5: Documentation update

- CLAUDE.md: update tool count, new features, security rules
- Example workflows for each new tool
- Error code ranges: extend for new tools
- API documentation with `cargo doc`

---

## File Change Map (from Architecture Mapper)

| # | File | Lines | Change | Risk |
|---|------|-------|--------|------|
| 1 | `src/ast/content.rs` (NEW) | — | ContentPart types (Raw, Analyzed, Runtime) | LOW |
| 2 | `src/ast/raw/action.rs` | 60-79 | Add `content` to RawInferAction | LOW |
| 3 | `src/ast/raw/parser.rs` | 437-477 | Parse `content:`, adjust prompt requirement | MEDIUM |
| 4 | `src/ast/analyzed/task.rs` | 129-151 | Add `content` to AnalyzedInferAction | LOW |
| 5 | `src/ast/analyzer/analyze.rs` | 670-680 | Map raw→analyzed content | LOW |
| 6 | `src/ast/lower.rs` | 162-178, 586 | Lower/unlower content | LOW |
| 7 | `src/ast/action.rs` | 57-207 | Add content to InferParams + Deserialize + validate | MEDIUM |
| 8 | `src/provider/rig.rs` | 339, 1040, 1182 | `infer_vision()` + `infer_vision_stream()` | HIGH |
| 9 | `src/runtime/executor/verbs.rs` | 44-464 | Vision code path, CAS resolution, events | HIGH |
| 10 | `src/event/log.rs` | ~500 | VisionContentResolved event | LOW |
| 11 | `src/runtime/builtin/media/qr.rs` (NEW) | — | QR validate tool | LOW |
| 12 | `src/runtime/builtin/media/quality.rs` (NEW) | — | IQA tool | LOW |
| 13 | `src/runtime/builtin/media/verify.rs` (NEW) | — | C2PA verify tool | LOW |
| 14 | `src/runtime/builtin/media/watermark.rs` (NEW) | — | Watermark tool | LOW |
| 15 | `src/media/store.rs` | store/read | Transparent zstd compression | MEDIUM |

**Implementation order:** 1 → 2 → 3 → 4 → 5 → 6 → 7 → 10 → 8 → 9 → 11-15

---

## New Dependencies

```toml
# Phase 1 — Vision (no new deps, rig-core 0.32 has everything)
base64 = "0.22"  # likely already a transitive dep

# Phase 2
rxing = { version = "0.8", optional = true, default-features = false, features = ["qrcode"] }
zstd = { version = "0.13", optional = true, default-features = false }

# Phase 3
butteraugli = { version = "0.7", optional = true }
dssim = { version = "3.4", optional = true }
jxl-oxide = { version = "0.12", optional = true }
blind_watermark = { version = "0.1", optional = true }

# Features
media-qr = ["dep:rxing", "dep:image"]
media-compression = ["dep:zstd"]
media-iqa = ["dep:butteraugli", "dep:dssim"]
media-jxl = ["dep:jxl-oxide"]
media-watermark = ["dep:blind_watermark"]
```

## Success Criteria

- `infer:` with `content:` produces vision responses on Claude + OpenAI + Pixtral
- CAS→base64→LLM bridge works with template resolution (the unique differentiator)
- `nika:qr_validate` decodes QR codes, returns scan score
- CAS compression: 3.5x on JSON/text, 1x passthrough on images
- `nika:quality`: SSIM/butteraugli scores for before/after
- `nika:verify`: C2PA readback + EU AI Act compliance flag
- `nika:watermark`: embed/extract round-trip survives JPEG compression
- All 6129+ existing tests pass
- All new features have 10+ tests each
- Clippy zero warnings (default + all features)
- All isolated feature combos compile (`--no-default-features --features X`)
- Total test count target: 6500+
