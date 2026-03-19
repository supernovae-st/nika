# PR4 — Vision + Innovations Master Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add multimodal vision support to `infer:`, QR validation, CAS compression, and cutting-edge differentiators that position Nika ahead of every agentic framework.

**Architecture:** Three phases. Phase 1 rewires the provider layer for multimodal, adds `content:` to the AST, and lights up vision across 6 providers. Phase 2 adds QR validate + CAS compression. Phase 3 adds gemmes rares (IQA, C2PA verify, JPEG XL).

**Tech Stack:** rig-core 0.32 (native multimodal), rxing 0.8.5, zstd 0.13, jxl-oxide, butteraugli, c2pa 0.78 Reader API

---

## Research Findings (from 14 Opus agents)

### Architecture Ratings
| Layer | Rating | Notes |
|-------|--------|-------|
| MediaOp Trait + Adapter | **ELEGANT** | Best tool abstraction in any agentic framework |
| CAS (blake3 + O_EXCL + shard) | **ELEGANT** | Ahead of LangChain/CrewAI/AutoGen (they have none) |
| Compute Pool (rayon + oneshot) | **GOOD** | Correct pattern, caps at 4 threads |
| Security (5 defense layers) | **ELEGANT** | No gaps found by 3 independent reviewers |
| Provider Architecture | **ADEQUATE** | 28+ identical match arms, text-only — needs refactor |

### Key Discovery: rig-core 0.32 has FULL vision support
```rust
// Already works — no HTTP bypass needed
UserContent::image_base64(b64_data, Some(ImageMediaType::PNG), Some(ImageDetail::Auto))
```
- Anthropic: requires media_type, converts to `Content::Image { source: ImageSource::Base64 }`
- OpenAI: requires media_type + detail, builds `data:image/png;base64,...` data URI
- Mistral/Pixtral: OpenAI-compatible format
- Gemini: `inlineData` format
- rig-core 0.33.0 available (published 2026-03-17)

### Competitive Position
| Feature | Nika | LangChain | CrewAI | AutoGen | Dify |
|---------|:----:|:---------:|:------:|:-------:|:----:|
| CAS media storage | YES | no | no | no | no |
| C2PA provenance | YES | no | no | no | no |
| Pipeline composition | YES | partial | no | no | partial |
| Vision in workflows | **PR4** | yes | no | yes | yes |
| CAS→Vision bridge | **PR4** | no | no | no | no |
| Image quality scoring | **PR4** | no | no | no | no |
| C2PA verification | **PR4** | no | no | no | no |

---

## Phase 1: Vision Support (P0 — Critical)

### Task 1.1: Add ContentPart to AST

**Files:**
- Create: `src/ast/analyzed/content.rs`
- Modify: `src/ast/analyzed/types.rs` — add `content: Option<Vec<ContentPart>>` to InferParams
- Modify: `src/ast/raw/parser.rs` — parse `content:` array in `infer:` blocks
- Modify: `src/ast/analyzer/` — validate content items
- Modify: `src/ast/lower.rs` — lower ContentPart to runtime types

```rust
/// A part of a multimodal prompt message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
  /// Plain text
  Text { text: String },
  /// Image from CAS hash (resolved at template time)
  Image {
    source: String,           // CAS hash or template {{with.x.media[0].hash}}
    #[serde(default)]
    detail: ImageDetail,      // auto | low | high
  },
  /// Image from URL
  ImageUrl {
    url: String,
    #[serde(default)]
    detail: ImageDetail,
  },
}
```

**YAML syntax:**
```yaml
describe:
  infer:
    model: claude-sonnet-4-6
    content:
      - type: image
        source: "{{with.photo.media[0].hash}}"
        detail: high
      - type: text
        text: "Describe this image in detail."
  depends_on: [import_photo]
  with:
    photo: $import_photo
```

Backward compatible: `prompt:` remains valid as sugar for single text part.

### Task 1.2: Build multimodal message from ContentParts

**Files:**
- Modify: `src/provider/rig.rs` — add `build_vision_message()` function
- Modify: `src/runtime/executor/verbs.rs` — resolve CAS hashes in content parts, call vision path

```rust
fn build_vision_message(
  parts: &[ContentPart],
  cas: &CasStore,
) -> Result<rig::message::Message, NikaError> {
  let mut content_blocks = Vec::new();
  for part in parts {
    match part {
      ContentPart::Text { text } => {
        content_blocks.push(UserContent::text(text));
      }
      ContentPart::Image { source, detail } => {
        let data = cas.read(source).await?;
        let mime = infer::get(&data).map(|t| t.mime_type());
        let media_type = match mime {
          Some("image/png") => ImageMediaType::PNG,
          Some("image/jpeg") => ImageMediaType::JPEG,
          Some("image/webp") => ImageMediaType::WEBP,
          Some("image/gif") => ImageMediaType::GIF,
          _ => return Err(invalid_args("infer", "unsupported image format for vision")),
        };
        let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
        content_blocks.push(UserContent::image_base64(b64, Some(media_type), Some(*detail)));
      }
      ContentPart::ImageUrl { url, detail } => {
        content_blocks.push(UserContent::image_url(url, None, Some(*detail)));
      }
    }
  }
  Ok(Message::User {
    content: OneOrMany::many(content_blocks).expect("non-empty"),
  })
}
```

### Task 1.3: Tests for vision pipeline

**Files:**
- Create: `src/ast/analyzer/tests_vision.rs`
- Modify: `src/runtime/executor/tests/` — E2E vision tests (mock provider)

Test cases:
- Parse `content:` YAML with text + image parts
- Validate image CAS hash exists in analyzer
- Backward compat: `prompt:` still works
- Error: vision on provider without support (DeepSeek)
- E2E: import image → infer with content → verify message construction

---

## Phase 2: QR Validate + CAS Compression

### Task 2.1: nika:qr_validate (rxing)

**Files:**
- Create: `src/runtime/builtin/media/qr.rs`
- Modify: `src/runtime/builtin/media/mod.rs` — register under `media-qr`
- Modify: `Cargo.toml` — add `rxing = { version = "0.8", optional = true }`

```rust
pub struct QrValidateOp;
// Returns: { decoded: bool, data: "...", format: "QR_CODE",
//            error_correction: "H", module_count: 29, scan_score: 85 }
```

Scan score algorithm:
1. Decode QR → pass/fail
2. Error correction level (L/M/Q/H) → 25/50/75/100 points
3. Module contrast ratio → 0-100 scale
4. Quiet zone detection → bonus points

### Task 2.2: CAS zstd compression

**Files:**
- Modify: `src/media/store.rs` — transparent compress on store(), decompress on read()
- Modify: `Cargo.toml` — add `zstd = { version = "0.13", optional = true }`

Design:
- Hash ORIGINAL data (before compression) for CAS key
- Skip compression for image/*, audio/*, video/* MIME types
- Compress text, JSON, SVG, YAML, PDF with zstd level 3
- Detect zstd magic bytes (0x28B52FFD) on read for transparent decompress
- Feature gate: `media-compression`

---

## Phase 3: Gemmes Rares (Differentiators)

### Task 3.1: nika:quality — Image Quality Assessment

**Files:**
- Create: `src/runtime/builtin/media/quality.rs`
- `butteraugli` crate for perceptual quality scoring
- Returns: `{ ssim: 0.95, psnr: 38.2, butteraugli_score: 1.2 }`
- Use case: validate pipeline output quality after optimization/compression

### Task 3.2: nika:verify — C2PA Manifest Verification

**Files:**
- Create: `src/runtime/builtin/media/verify.rs`
- Uses `c2pa::Reader::from_stream()` (already proven in our readback tests)
- Returns: `{ valid: true, title: "...", assertions: [...], signer: "...", trust_status: "..." }`
- EU AI Act compliance: verify AI-generated content has proper provenance
- Differentiator: sign (nika:provenance) AND verify (nika:verify) in same pipeline

### Task 3.3: nika:convert — JPEG XL support

**Files:**
- Modify: `src/runtime/builtin/media/convert.rs` — add JXL output format
- `jxl-oxide` for decode, `jxl-enc` or `image` JXL feature for encode
- 52% savings over JPEG with lossless recompression
- Chrome 145 just re-enabled JXL (February 2026)

### Task 3.4: nika:transcribe — Audio Transcription (future)

- `candle-transformers` Whisper implementation
- Audio→text natively in Rust, no external API
- Feature gate: `media-transcribe`
- Deferred to PR5 (model distribution same issue as OCR)

---

## Execution Order

```
Phase 1 (Vision) ─── 3-4 hours ─── HIGHEST VALUE
  Task 1.1: ContentPart AST
  Task 1.2: Multimodal message builder
  Task 1.3: Vision tests

Phase 2 (QR + Compression) ─── 2 hours ─── HIGH VALUE
  Task 2.1: QR validate (rxing)
  Task 2.2: CAS zstd compression

Phase 3 (Gemmes Rares) ─── 2-3 hours ─── DIFFERENTIATORS
  Task 3.1: Image quality scoring
  Task 3.2: C2PA verification
  Task 3.3: JPEG XL in convert
```

## New Dependencies

```toml
# Phase 1 — Vision (no new deps, rig-core 0.32 has everything)
base64 = "0.22"  # likely already a dep

# Phase 2
rxing = { version = "0.8", optional = true, default-features = false, features = ["qrcode"] }
zstd = { version = "0.13", optional = true, default-features = false }

# Phase 3
butteraugli = { version = "0.1", optional = true }  # IQA
jxl-oxide = { version = "0.11", optional = true }    # JPEG XL decode

# Features
media-qr = ["dep:rxing", "dep:image"]
media-compression = ["dep:zstd"]
media-iqa = ["dep:butteraugli"]
media-jxl = ["dep:jxl-oxide"]
```

## Success Criteria

- `infer:` with `content:` produces correct vision responses on Claude + OpenAI + Pixtral
- `nika:qr_validate` decodes standard QR codes, returns error correction level + scan score
- CAS compression: 3x ratio on JSON/text, 1x passthrough on images
- `nika:quality`: SSIM/PSNR/butteraugli scores for before/after comparison
- `nika:verify`: reads back C2PA manifests, validates trust chain
- All 6129+ existing tests still pass
- Clippy zero warnings
