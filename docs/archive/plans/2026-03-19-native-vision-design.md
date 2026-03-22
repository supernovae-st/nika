# Native Vision Support — Design & Implementation Plan

> **Date:** 2026-03-19
> **Scope:** Add vision/multimodal inference to `provider: native` via mistral.rs
> **Crate:** mistralrs 0.7.0 (VisionModelBuilder + ISQ)
> **Estimated LOC:** ~1300 across 9 files
> **Feature gate:** `native-inference` (existing, no new flag)

---

## Architecture Decision: VisionPart Provider-Agnostic Type

### Problem

`run_infer_vision()` builds `Vec<rig::UserContent>` (base64 strings) — cloud-only.
Native needs `Vec<DynamicImage>` (decoded pixels). Incompatible types.

### Decision: Option B — VisionPart intermediate type

```
YAML content: → ContentPart → CAS read → VisionPart (raw bytes)
                                            │
                          ┌─────────────────┼─────────────────┐
                          ▼                 ▼                 ▼
                      Cloud Path        Native Path       Future
                      VisionPart        VisionPart        (embed?)
                      → base64          → DynamicImage
                      → UserContent     → VisionMessages
                      → rig API         → mistral.rs
```

Base64 encoding DESCENDS from executor to provider layer.
Executor stays provider-agnostic.

### Critical Discovery: GGUF Cannot Do Vision

mistral.rs `GgufModelBuilder` → `ModelCategory::Text` (hardcoded).
Vision requires `VisionModelBuilder` + ISQ from HuggingFace safetensors.
Same `Model` type produced — inference API identical after loading.

---

## Target Vision Models (ISQ, not GGUF)

| Model | Params | VRAM (Q4K) | Min Hardware | Architecture |
|-------|--------|-----------|-------------|-------------|
| **Gemma 3 4B** | 4B | ~3 GB | M1 8GB | `Gemma3` |
| **Qwen2.5-VL 3B** | 3B | ~2.5 GB | M1 8GB | `Qwen2_5VL` |
| **Qwen2.5-VL 7B** | 7B | ~5 GB | M1 Pro 16GB | `Qwen2_5VL` |
| **Gemma 3 12B** | 12B | ~8 GB | M2 16GB | `Gemma3` |
| **Mistral Small 3.1** | 24B | ~16 GB | M3 Max 36GB | `Mistral3` |

---

## Phase 1 — Core Types (~200 LOC)

### File: `src/core/backend.rs`

#### VisionPart enum

```rust
#[derive(Debug, Clone)]
pub enum VisionPart {
    Text(String),
    Image {
        bytes: Vec<u8>,
        media_type: String,
        detail: ImageDetail,
    },
    ImageUrl {
        url: String,
        detail: ImageDetail,
    },
}
```

Helpers: `text()`, `image()`, `image_auto()`, `image_url()`, `is_image()`, `byte_size()`

#### NativeModelKind enum

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NativeModelKind {
    TextGguf,
    VisionHf {
        model_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        isq: Option<IsqLevel>,
    },
}
```

Default: `TextGguf` (backward compatible).

#### IsqLevel enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IsqLevel {
    Q2K, Q3K, Q4K, Q4_0, Q5K, Q5_0, Q6K, Q8_0, HQQ4, HQQ8,
}
```

With `from_str_lossy()`, `memory_multiplier()`, `Display`.
Conversion to `mistralrs::IsqType` behind feature gate in runtime.rs.

#### LoadConfig extension

```rust
pub struct LoadConfig {
    #[serde(default)]
    pub model_kind: NativeModelKind,  // NEW — default TextGguf
    pub gpu_ids: Vec<u32>,
    pub gpu_layers: i32,
    pub context_size: Option<u32>,
    pub keep_alive: bool,
}
```

---

## Phase 2 — InferenceBackend Trait Extension (~100 LOC)

### File: `src/provider/native/traits.rs`

Add 3 methods with **default implementations** (zero breakage):

```rust
fn supports_vision(&self) -> bool { false }

fn infer_vision(
    &self,
    parts: &[VisionPart],
    options: ChatOptions,
) -> impl Future<Output = Result<ChatResponse, NativeError>> + Send {
    async { Err(NativeError::VisionNotSupported { reason: "..." }) }
}

fn infer_vision_stream(
    &self,
    parts: &[VisionPart],
    options: ChatOptions,
) -> impl Future<Output = Result<impl Stream<...>, NativeError>> + Send {
    async { Err(...) }
}
```

### File: `src/provider/native/error.rs`

Add 4 variants:

```rust
VisionNotSupported { reason: String },
VisionModelLoadFailed { model_id: String, reason: String },
InvalidImageData { reason: String },
VisionContentTooLarge { reason: String },
```

---

## Phase 3 — NativeRuntime Vision Implementation (~400 LOC)

### File: `src/provider/native/runtime.rs`

#### 3.1 — Add `is_vision: bool` field

```rust
pub struct NativeRuntime {
    model: Option<Arc<RwLock<Model>>>,
    is_vision: bool,  // NEW — set during load()
    tasks: Arc<Mutex<Vec<TrackedTask>>>,
}
```

#### 3.2 — Dual-path load()

```rust
async fn load(&mut self, model_path: PathBuf, config: LoadConfig) -> Result<(), NativeError> {
    match &config.model_kind {
        NativeModelKind::TextGguf => {
            // EXISTING: GgufModelBuilder path (unchanged)
            let model = GgufModelBuilder::new(parent, vec![filename])
                .with_logging()
                .with_paged_attn(|| { ... })?
                .build()
                .await?;
            self.model = Some(Arc::new(RwLock::new(model)));
            self.is_vision = false;
        }
        NativeModelKind::VisionHf { model_id, isq } => {
            // NEW: VisionModelBuilder path
            let mut builder = VisionModelBuilder::new(model_id)
                .with_logging();

            if let Some(level) = isq {
                builder = builder.with_isq(isq_to_mistralrs(*level));
            }

            let context_size = config.context_size.unwrap_or(4096);
            builder = builder.with_paged_attn(|| {
                PagedAttentionMetaBuilder::default()
                    .with_block_size(32)
                    .with_gpu_memory(MemoryGpuConfig::ContextSize(context_size as usize))
                    .build()
            })?;

            let model = builder.build().await.map_err(|e| {
                NativeError::VisionModelLoadFailed {
                    model_id: model_id.clone(),
                    reason: e.to_string(),
                }
            })?;

            self.model = Some(Arc::new(RwLock::new(model)));
            self.is_vision = true;
        }
    }
    Ok(())
}
```

#### 3.3 — infer_vision() implementation

```rust
async fn infer_vision(
    &self,
    parts: &[VisionPart],
    options: ChatOptions,
) -> Result<ChatResponse, NativeError> {
    if !self.is_vision {
        return Err(NativeError::VisionNotSupported {
            reason: "Loaded model is text-only (GGUF). Load a vision model to use vision.".into(),
        });
    }

    let model_lock = self.model.as_ref().ok_or(NativeError::ModelNotLoaded)?;
    let model = model_lock.read().await;

    // Decode images from raw bytes → DynamicImage
    let (prompt, images) = decode_vision_parts(parts)?;

    // Build VisionMessages
    let messages = VisionMessages::new()
        .add_image_message(TextMessageRole::User, &prompt, images, &model)
        .map_err(|e| NativeError::InvalidImageData { reason: e.to_string() })?;

    // Apply sampling params
    let request = apply_sampling_params(RequestBuilder::from(messages), &options);

    // Send request
    let response = model.send_chat_request(request).await
        .map_err(|e| NativeError::InferenceFailed { reason: e.to_string() })?;

    parse_chat_completion(response)
}
```

#### 3.4 — Helper: decode_vision_parts()

```rust
fn decode_vision_parts(parts: &[VisionPart]) -> Result<(String, Vec<DynamicImage>), NativeError> {
    let mut prompt_parts = Vec::new();
    let mut images = Vec::new();

    for part in parts {
        match part {
            VisionPart::Text(text) => prompt_parts.push(text.as_str()),
            VisionPart::Image { bytes, .. } => {
                let img = image::load_from_memory(bytes)
                    .map_err(|e| NativeError::InvalidImageData {
                        reason: format!("Failed to decode image: {e}"),
                    })?;
                images.push(img);
            }
            VisionPart::ImageUrl { url, .. } => {
                return Err(NativeError::VisionNotSupported {
                    reason: format!("Native vision does not support URL images. Pre-fetch the image: {url}"),
                });
            }
        }
    }

    Ok((prompt_parts.join("\n"), images))
}
```

#### 3.5 — infer_vision_stream()

Same pattern as existing `infer_stream()`:
- Build VisionMessages + RequestBuilder
- `model.stream_chat_request(request)` → mpsc channel
- TrackedTask + CancellationToken + chunk timeout
- Extracted into shared `spawn_stream_task()` helper

---

## Phase 4 — Refactor Executor + RigProvider (~200 LOC)

### File: `src/runtime/executor/verbs.rs` — run_infer_vision()

**Before:** builds `Vec<rig::UserContent>` with base64 encoding
**After:** builds `Vec<VisionPart>` with raw bytes (no base64)

```rust
// In run_infer_vision():
let mut vision_parts = Vec::new();

if !prompt.is_empty() {
    vision_parts.push(VisionPart::text(prompt));
}

for part in content_parts {
    match part {
        ContentPart::Text { text } => {
            vision_parts.push(VisionPart::text(resolve_template(text)?));
        }
        ContentPart::Image { source, detail } => {
            let hash = resolve_template(source)?;
            let bytes = self.cas.read(&hash).await?;
            let media_type = detect_media_type(&bytes);
            vision_parts.push(VisionPart::image(bytes, media_type, detail));
        }
        ContentPart::ImageUrl { url, detail } => {
            let resolved_url = resolve_template(url)?;
            // SSRF validation
            validate_url_scheme(&resolved_url)?;
            vision_parts.push(VisionPart::image_url(resolved_url, detail));
        }
    }
}

// Emit VisionContentResolved event (unchanged)
// Call provider.infer_vision(vision_parts, model, system, max_tokens)
```

### File: `src/provider/rig.rs` — infer_vision() signature change

```rust
pub async fn infer_vision(
    &self,
    parts: Vec<VisionPart>,  // WAS: Vec<rig::UserContent>
    model: Option<&str>,
    system: Option<&str>,
    max_tokens: Option<u32>,
) -> Result<String, RigInferError> {
    // Convert VisionPart → rig::UserContent for cloud providers
    let user_content = vision_parts_to_user_content(parts)?;
    let message = Message::User { content: OneOrMany::many(user_content)? };

    match self {
        // Cloud providers: unchanged (use vision_prompt! macro)
        RigProvider::Claude(c) => vision_prompt!(c),
        RigProvider::OpenAI(c) => vision_prompt!(c),
        // ...

        // Native: delegate to NativeRuntime::infer_vision()
        #[cfg(feature = "native-inference")]
        RigProvider::Native(runtime) => {
            let options = ChatOptions {
                max_tokens: max_tokens.map(|t| t as usize),
                ..Default::default()
            };
            let response = runtime.infer_vision(&parts, options).await
                .map_err(|e| RigInferError::VisionNotSupported(e.to_string()))?;
            Ok(response.message.content)
        }

        RigProvider::DeepSeek(_) => Err(RigInferError::VisionNotSupported(...)),
    }
}

/// Convert VisionPart → rig::UserContent (for cloud providers only)
fn vision_parts_to_user_content(parts: Vec<VisionPart>) -> Result<Vec<UserContent>, RigInferError> {
    parts.into_iter().map(|part| match part {
        VisionPart::Text(text) => Ok(UserContent::text(text)),
        VisionPart::Image { bytes, media_type, detail } => {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            let rig_media = parse_rig_media_type(&media_type);
            let rig_detail = map_detail(detail);
            Ok(UserContent::image_base64(b64, rig_media, rig_detail))
        }
        VisionPart::ImageUrl { url, detail } => {
            let rig_detail = map_detail(detail);
            Ok(UserContent::image_url(url, None, rig_detail))
        }
    }).collect()
}
```

---

## Phase 5 — CLI & Config (~150 LOC)

### File: `src/cli/model.rs` (or `src/cli/provider.rs`)

```bash
# Load a vision model with ISQ
nika model load --vision "Qwen/Qwen2.5-VL-7B-Instruct" --isq q4k

# Load with auto-detect (ModelBuilder)
nika model load --hf "google/gemma-3-4b-it" --isq q4k

# Status shows vision capability
nika model status
# → Model: Qwen2.5-VL-7B-Instruct (vision, Q4K ISQ, 5.2 GB)
```

### File: `src/config.rs`

NativeModelKind serializes to YAML provider config:

```yaml
# ~/.nika/config.yaml
providers:
  native:
    model_kind:
      kind: vision_hf
      model_id: "Qwen/Qwen2.5-VL-7B-Instruct"
      isq: q4k
    context_size: 4096
```

---

## Phase 6 — Tests (~400 LOC)

### Unit Tests

| Test | File | What |
|------|------|------|
| `vision_part_constructors` | `core/backend.rs` | VisionPart::text/image/image_url |
| `native_model_kind_serde` | `core/backend.rs` | NativeModelKind YAML round-trip |
| `isq_level_from_str` | `core/backend.rs` | IsqLevel parsing (q4k, Q8_0, etc.) |
| `isq_level_memory_multiplier` | `core/backend.rs` | Memory estimation accuracy |
| `load_config_default_backward_compat` | `core/backend.rs` | Default is TextGguf |
| `vision_not_supported_default` | `traits.rs` | Default impl returns error |
| `supports_vision_default_false` | `traits.rs` | Default is false |
| `vision_parts_to_user_content` | `rig.rs` | VisionPart → UserContent conversion |
| `decode_vision_parts_text_only` | `runtime.rs` | Text extraction |
| `decode_vision_parts_image_url_rejected` | `runtime.rs` | URL not supported by native |
| `text_gguf_rejects_vision` | `runtime.rs` | TextGguf model → VisionNotSupported |

### Integration Tests (when HF model available)

| Test | What |
|------|------|
| `native_vision_load_gemma3_4b` | Load Gemma 3 4B with Q4K ISQ |
| `native_vision_infer_describe_image` | Send PNG → get description |
| `native_vision_stream_response` | Streaming vision with chunks |
| `native_vision_cancellation` | Cancel during inference |

---

## Execution Order

```
Phase 1 (core types)         — No deps, can start immediately
Phase 2 (trait extension)    — Depends on Phase 1 (VisionPart type)
Phase 3 (NativeRuntime)      — Depends on Phase 2 (trait methods)
Phase 4 (executor refactor)  — Depends on Phase 1 (VisionPart type)
                               Can parallel with Phase 3
Phase 5 (CLI/config)         — Depends on Phase 1 (NativeModelKind)
                               Can parallel with Phase 3-4
Phase 6 (tests)              — After all phases

Parallelism:
  Phase 1 → Phase 2 → Phase 3
                    └→ Phase 4 (parallel with 3)
                    └→ Phase 5 (parallel with 3-4)
                         → Phase 6
```

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| `add_image_message` requires `&Model` | Acquire read lock, build message, release before inference |
| HuggingFace download is large (15GB+ unquantized) | ISQ quantizes at load time; document first-run latency |
| Memory pressure with vision + text models loaded | One model at a time (unload text before loading vision) |
| `image::load_from_memory` panic on malformed input | Use existing `decode_image_safe()` from media/safety.rs |
| Breaking change to `infer_vision` signature in rig.rs | Phase 4 changes both caller and callee in same PR |
| VisionMessages defaults to deterministic sampling | Always convert to RequestBuilder with explicit params |

---

## Cargo.toml Changes

```toml
# Existing (unchanged):
native-inference = ["dep:mistralrs", "dep:async-stream"]
mistralrs = { version = "0.7", optional = true }

# Add image dep to native-inference (needed for DynamicImage decode):
native-inference = ["dep:mistralrs", "dep:async-stream", "dep:image"]
```

The `image` crate is already in Cargo.toml as optional (used by media-thumbnail/media-phash).
Adding it to `native-inference` features enables `image::load_from_memory()` in the vision path.

---

## mistral.rs API Reference (from source v0.7.0)

### VisionModelBuilder

```rust
VisionModelBuilder::new(model_id: impl ToString) -> Self
    .with_isq(isq: IsqType) -> Self           // Explicit ISQ level
    .with_loader_type(VisionLoaderType) -> Self // Override auto-detect
    .with_logging() -> Self
    .with_paged_attn(|| PagedAttentionMetaBuilder::default().build()) -> Result<Self>
    .build() -> Result<Model>                   // async, downloads from HF
```

### VisionMessages

```rust
VisionMessages::new() -> Self
    .add_message(TextMessageRole::User, "text") -> Self
    .add_image_message(
        TextMessageRole::User,
        "Describe this",
        Vec<DynamicImage>,     // image::DynamicImage
        &Model,                 // Required for model-specific prefixer
    ) -> Result<Self>
```

### Key: No `with_auto_isq()` — only `with_isq(IsqType)`.

### Response

```rust
model.send_chat_request(request) -> Result<ChatCompletionResponse>
model.stream_chat_request(request) -> Result<Stream>

// Stream yields:
Response::Chunk(ChatCompletionChunkResponse { choices[0].delta.content })
Response::Done(ChatCompletionResponse { choices[0].message.content, usage })
Response::ModelError(String, ChatCompletionResponse)
Response::InternalError(Box<dyn Error>)
```
