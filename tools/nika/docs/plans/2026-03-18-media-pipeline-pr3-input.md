# PR3: Media Input -- Vision & Audio Input for infer/agent Verbs

> **Version**: v4.1
> **Status**: FUTURE -- not yet scheduled
> **Date**: 2026-03-18
> **Branch**: `feat/media-input`
> **Baseline**: After PR1 + PR2 merged
> **Schema**: @0.12 -> @0.13 (new YAML syntax)
> **Commits**: ~10
> **Tests**: ~25-30 new

**Parent**: [Master Plan](./2026-03-18-media-pipeline-master-plan.md) | **Prev**: [PR2](./2026-03-18-media-pipeline-pr2-artifacts.md)

---

## Bugs Fixed (v4.0 -> v4.1)

| # | Bug | Fix |
|---|-----|-----|
| B1 | `ImageMediaType::from_mime()` does not exist in rig-core | Replace with manual `mime_to_image_type()` match function |
| B2 | `Message::user()` takes `Into<String>`, not `Vec<UserContent>` | Use `Message::User { content: OneOrMany::many(parts)? }` |
| B3 | Streaming path not addressed for vision requests | Document non-streaming fallback for vision (multi-part messages) |
| B4 | No provider capability check before vision call | Add pre-check; NIKA-325 if provider does not support Image |
| B5 | URL download has no size guard or timeout | Add `content_length()` pre-check, streaming + abort at BINARY_MAX_SIZE, 30s timeout |
| B6 | File existence check at analysis-time is wrong (file may not exist yet) | Move to runtime; NIKA-320 = runtime error in `resolve_media_input()` |
| B7 | Dependency table attributes PR2 items that are actually from PR1 | Fix: MediaRef, detect.rs, CasStore are from PR1 |
| B8 | Schema string uses `"nika/workflow@0.13"` instead of `"@0.13"` | Use `"@0.13"` consistently |
| B9 | Missing `additional_params: None` on rig-core agent constructors | Add to all agent builder chains |
| B10 | No spec for parsing `with:` media expressions | Add regex spec: `^(\w+)\.media\[(\d+)\]$` -> (task_id, index) |

---

## The Problem

After PR1 + PR2, Nika can **extract** binary media from MCP tool outputs and **store** it in CAS.
But it cannot **send** media as input to LLM vision/audio models. A user who wants to analyze
an image -- whether from a local file, a previous task's output, or a URL -- has no way to
attach it to an `infer:` or `agent:` prompt.

This PR adds the `media:` field to `infer:` and `agent:` verbs, enabling multimodal input
through the full AST pipeline (parse -> analyze -> lower -> execute -> provider).

---

## New YAML Syntax

```yaml
schema: "@0.13"

tasks:
  analyze:
    infer:
      prompt: "Describe this image in detail"
      media:
        - path: "./photo.jpg"                    # Local file
        - with: generate.media[0]                # From previous task (CAS)
        - url: "https://example.com/image.png"   # Remote URL
```

Three media source types:

| Source | Field | Description |
|--------|-------|-------------|
| Local file | `path:` | Resolved relative to workflow file |
| Task media | `with:` | References `TaskResult.media[N]` from a previous task |
| Remote URL | `url:` | Downloaded at runtime, MIME auto-detected |

Each `media:` entry specifies exactly ONE source (path, with, or url). Multiple entries
are allowed -- they are sent as separate content parts in the LLM request.

### `with:` Expression Parser Spec

The `with:` value must match this regex:

```
^(\w+)\.media\[(\d+)\]$
```

Capture groups:
- Group 1: `task_id` -- the source task name (alphanumeric + underscore)
- Group 2: `index` -- the media array index (non-negative integer)

Examples:
- `generate.media[0]` -> task_id=`generate`, index=`0`
- `step_2.media[3]` -> task_id=`step_2`, index=`3`
- `generate.media` -> INVALID (no index)
- `generate.media[0].path` -> INVALID (trailing field -- use full expression only in `with:` bindings, not in `media:` entries)

Parse at analysis time. Emit NIKA-321 on regex mismatch.

---

## Architecture: Full AST Pipeline

This touches **every phase** of the AST pipeline. The change flows through 5 layers:

```
YAML media: [{ path: "./photo.jpg" }]
  -> Phase 1 (parse):    RawMediaInput::Path("./photo.jpg")
  -> Phase 2 (analyze):  AnalyzedMediaInput::Path(validated path)
  -> Phase 3 (lower):    MediaInput::File(PathBuf)
  -> Phase 4 (runtime):  read file -> base64 -> detect MIME
  -> Phase 5 (provider): Message::User {
                            content: OneOrMany::many(vec![
                              UserContent::Image(Image {
                                data: DocumentSourceKind::Base64(encoded),
                                media_type: Some(ImageMediaType::JPEG),
                              }),
                              UserContent::text(prompt),
                            ])?
                          }
  -> Claude/GPT-4o/Gemini vision API
```

---

## Phase 1: Raw Parse

### Files changed

- `src/ast/raw/action.rs` -- new types
- `src/ast/raw/parser.rs` -- parse `media:` field

### New types in `src/ast/raw/action.rs`

```rust
/// A single media input source (one of path/with/url).
#[derive(Debug, Clone)]
pub enum RawMediaInput {
    /// Local file path: `path: "./photo.jpg"`
    Path(Spanned<String>),
    /// Reference to previous task media: `with: generate.media[0]`
    With(Spanned<String>),
    /// Remote URL: `url: "https://example.com/image.png"`
    Url(Spanned<String>),
}
```

### Changes to existing types

```rust
// RawInferAction gets new field:
pub struct RawInferAction {
    pub prompt: Spanned<String>,
    pub system: Option<Spanned<String>>,
    pub temperature: Option<Spanned<f64>>,
    pub max_tokens: Option<Spanned<u32>>,
    pub thinking: Option<Spanned<bool>>,
    pub thinking_budget: Option<Spanned<u32>>,
    /// Media inputs for vision/audio (schema @0.13+)
    pub media: Option<Spanned<Vec<Spanned<RawMediaInput>>>>,  // NEW
}

// RawAgentAction gets same field:
pub struct RawAgentAction {
    // ... existing fields ...
    /// Media inputs for initial prompt (schema @0.13+)
    pub media: Option<Spanned<Vec<Spanned<RawMediaInput>>>>,  // NEW
}
```

### Parser changes in `src/ast/raw/parser.rs`

Add `parse_media_inputs()` helper and call from `parse_infer_action()` / `parse_agent_action()`:

```rust
/// Parse media: field -- array of { path: | with: | url: } entries.
fn parse_media_inputs(
    file: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Option<Spanned<Vec<Spanned<RawMediaInput>>>>, ParseError> {
    let node = match map.get_node("media") {
        Some(node) => node,
        None => return Ok(None),
    };
    let span = node_to_span(file, node);

    let seq = match node {
        Node::Sequence(seq) => seq,
        _ => return Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span,
            message: "media must be a sequence".to_string(),
        }),
    };

    let mut items = Vec::new();
    for item in seq.iter() {
        let item_span = node_to_span(file, item);
        let m = match item {
            Node::Mapping(m) => m,
            _ => return Err(ParseError {
                kind: ParseErrorKind::InvalidType,
                span: item_span,
                message: "each media entry must be a mapping with exactly one of: path, with, url".to_string(),
            }),
        };

        // Exactly one of path/with/url
        let path = get_string_field(file, m, "path")?;
        let with = get_string_field(file, m, "with")?;
        let url = get_string_field(file, m, "url")?;

        let input = match (path, with, url) {
            (Some(p), None, None) => RawMediaInput::Path(p),
            (None, Some(w), None) => RawMediaInput::With(w),
            (None, None, Some(u)) => RawMediaInput::Url(u),
            _ => return Err(ParseError {
                kind: ParseErrorKind::InvalidType,
                span: item_span,
                message: "each media entry must have exactly one of: path, with, url".to_string(),
            }),
        };
        items.push(Spanned::new(input, item_span));
    }

    Ok(Some(Spanned::new(items, span)))
}
```

In `parse_infer_action()` (full form branch), add:
```rust
media: parse_media_inputs(file, m)?,
```

In `parse_agent_action()`, add:
```rust
media: parse_media_inputs(file, m)?,
```

**Commit**: `feat(ast): add RawMediaInput type + parse media: field in infer/agent`

---

## Phase 2: Analyze

### Files changed

- `src/ast/analyzed/task.rs` -- new types
- `src/ast/analyzer/analyze.rs` -- validation
- `src/ast/schema.rs` -- V13 + feature gate

### New types in `src/ast/analyzed/task.rs`

```rust
/// Validated media input source.
#[derive(Debug, Clone)]
pub enum AnalyzedMediaInput {
    /// Local file path (validated: well-formed, not traversal)
    /// NOTE: file existence is checked at RUNTIME, not analysis-time,
    /// because the file may be produced by a previous task.
    Path {
        path: String,
        span: Span,
    },
    /// Reference to previous task media (validated: task exists in DAG, expression parses)
    With {
        /// Full expression, e.g. "generate.media[0]"
        expression: String,
        /// Source task ID (extracted from expression via regex)
        task_id: TaskId,
        /// Media index (extracted from expression via regex)
        index: usize,
        span: Span,
    },
    /// Remote URL (validated: well-formed URL)
    Url {
        url: String,
        span: Span,
    },
}
```

### `with:` expression parsing in analyzer

```rust
use regex::Regex;
use std::sync::LazyLock;

/// Regex for parsing media `with:` expressions.
/// Matches: `task_id.media[index]`
/// Group 1: task_id (word chars)
/// Group 2: index (digits)
static MEDIA_WITH_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\w+)\.media\[(\d+)\]$").unwrap());

fn parse_media_with_expression(
    expr: &str,
    span: Span,
    task_id: &str,
) -> Result<(TaskId, usize), NikaError> {
    let caps = MEDIA_WITH_RE.captures(expr).ok_or_else(|| {
        NikaError::MediaInputInvalidRef {
            expression: expr.to_string(),
            reason: "expected format: <task>.media[<index>]".to_string(),
            task_id: task_id.to_string(),
        }
    })?;
    let ref_task = caps.get(1).unwrap().as_str();
    let index: usize = caps.get(2).unwrap().as_str().parse().map_err(|_| {
        NikaError::MediaInputInvalidRef {
            expression: expr.to_string(),
            reason: "media index must be a valid number".to_string(),
            task_id: task_id.to_string(),
        }
    })?;
    Ok((TaskId::new(ref_task), index))
}
```

### Changes to analyzed action types

```rust
// AnalyzedInferAction gets:
pub struct AnalyzedInferAction {
    // ... existing fields ...
    /// Media inputs for vision/audio
    pub media: Vec<AnalyzedMediaInput>,  // NEW (empty = no media)
    pub span: Span,
}

// AnalyzedAgentAction gets:
pub struct AnalyzedAgentAction {
    // ... existing fields ...
    /// Media inputs for initial prompt
    pub media: Vec<AnalyzedMediaInput>,  // NEW
    pub span: Span,
}
```

### Analyzer validation in `src/ast/analyzer/analyze.rs`

For each `RawMediaInput`:

1. **Path**: Validate the path is well-formed and not a traversal attack (no `..`).
   Do NOT check file existence -- that happens at runtime (NIKA-320 is a runtime error).
2. **With**: Parse the expression via `MEDIA_WITH_RE` regex, validate the referenced task
   exists in the task table, extract `task_id` and `index`. Emit `NIKA-321` for invalid
   expressions. Add the referenced task as an implicit dependency.
3. **Url**: Validate URL is well-formed (starts with `http://` or `https://`). Emit `NIKA-322`
   for invalid URLs. URL is NOT fetched at analysis time.

**Feature gate**: If `media:` is present but schema version < @0.13, emit `NIKA-323`:
```
[NIKA-323] media: field requires schema @0.13 or later (current: @0.12)
```

**Commit**: `feat(ast): add AnalyzedMediaInput + validation in analyzer`

### Schema version gate in `src/ast/schema.rs`

```rust
// Add V13 variant to SchemaVersion enum:
pub enum SchemaVersion {
    // ... existing V01..V12 ...
    /// @0.13
    V13,
}

// Add to parse():
"@0.13" => Some(Self::V13),

// Add to as_str():
Self::V13 => "@0.13",

// Add feature gate:
pub fn supports_media_input(&self) -> bool {
    self.supports(Self::V13)
}

// Update latest():
pub fn latest() -> Self { Self::V13 }

// Update upgrade_hint():
Self::V12 => Some("@0.13 adds media: field for vision/audio input"),
Self::V13 => None,
```

**Commit**: `feat(ast): add schema @0.13 with media input feature gate`

---

## Phase 3: Lower

### Files changed

- `src/ast/action.rs` -- new type on `InferParams`
- `src/ast/agent.rs` -- new field on `AgentParams`
- `src/ast/lower.rs` -- map media fields

### New type in `src/ast/action.rs`

```rust
/// Resolved media input for runtime consumption.
#[derive(Debug, Clone)]
pub enum MediaInput {
    /// Local file to read and base64-encode at runtime
    File(std::path::PathBuf),
    /// Media from a previous task's TaskResult.media[N]
    /// (task_id, media_index)
    TaskMedia(String, usize),
    /// Remote URL to download at runtime
    Url(String),
}
```

### Changes to `InferParams` and `AgentParams`

```rust
// src/ast/action.rs
pub struct InferParams {
    // ... existing fields ...
    /// Media inputs for vision/audio
    pub media: Vec<MediaInput>,  // NEW (empty = text-only, backward compat)
}

// src/ast/agent.rs
pub struct AgentParams {
    // ... existing fields ...
    /// Media inputs for initial agent prompt
    pub media: Vec<MediaInput>,  // NEW
}
```

### Lowering in `src/ast/lower.rs`

```rust
fn lower_infer(
    infer: AnalyzedInferAction,
    provider: Option<String>,
    model: Option<String>,
) -> InferParams {
    InferParams {
        prompt: infer.prompt,
        provider,
        model,
        temperature: infer.temperature,
        max_tokens: infer.max_tokens,
        system: infer.system,
        response_format: None,
        extended_thinking: infer.thinking,
        thinking_budget: infer.thinking_budget.map(u64::from),
        media: lower_media_inputs(infer.media),  // NEW
    }
}

fn lower_media_inputs(inputs: Vec<AnalyzedMediaInput>) -> Vec<MediaInput> {
    inputs.into_iter().map(|input| match input {
        AnalyzedMediaInput::Path { path, .. } => {
            MediaInput::File(std::path::PathBuf::from(path))
        }
        AnalyzedMediaInput::With { task_id, index, .. } => {
            // task_id was interned -- resolve back to string name
            // (task_table lookup happens in lower_task, name is on AnalyzedTask)
            MediaInput::TaskMedia(task_id.to_string(), index)
        }
        AnalyzedMediaInput::Url { url, .. } => {
            MediaInput::Url(url)
        }
    }).collect()
}
```

Same pattern for `lower_agent()`.

**NOTE**: The `Deserialize` impl on `InferParams` (the `InferParamsHelper` enum) must also add
the `media` field to both `Short` (always empty) and `Full` (optional) variants. The `Short`
variant (`infer: "prompt"`) cannot have media -- it defaults to `vec![]`.

**Commit**: `feat(ast): add MediaInput to InferParams + AgentParams + lower()`

---

## Phase 4: Runtime

### Files changed

- `src/runtime/executor/verbs.rs` -- resolve media inputs in `run_infer()`

### Media resolution in `run_infer()`

After template resolution of the prompt, resolve each `MediaInput` into a rig-core
`UserContent` value:

```rust
use rig::message::{UserContent, Image, DocumentSourceKind, ImageMediaType};

/// Resolve a MediaInput into base64 data + MIME type.
async fn resolve_media_input(
    input: &MediaInput,
    datastore: &RunContext,
) -> Result<(Vec<u8>, String), NikaError> {
    match input {
        MediaInput::File(path) => {
            // Check file existence at runtime (moved from analysis-time -- B6)
            if !path.exists() {
                return Err(NikaError::MediaInputFileNotFound {
                    path: path.display().to_string(),
                    task_id: String::new(), // filled by caller
                });
            }
            // Read file from disk
            let data = tokio::fs::read(path).await.map_err(|e| {
                NikaError::MediaInputError {
                    source: format!("file:{}", path.display()),
                    reason: format!("failed to read: {}", e),
                }
            })?;
            // Detect MIME from magic bytes (uses infer crate from PR1)
            let mime = crate::media::detect::detect_mime(&data, path.to_str())
                .unwrap_or_else(|| "application/octet-stream".to_string());
            Ok((data, mime))
        }
        MediaInput::TaskMedia(task_id, index) => {
            // Get MediaRef from previous task's result
            let result = datastore.get(task_id).ok_or_else(|| {
                NikaError::MediaInputError {
                    source: format!("task:{}.media[{}]", task_id, index),
                    reason: format!("task '{}' not found in results", task_id),
                }
            })?;
            let media_ref = result.media.get(*index).ok_or_else(|| {
                NikaError::MediaInputError {
                    source: format!("task:{}.media[{}]", task_id, index),
                    reason: format!(
                        "media index {} out of bounds (task has {} media items)",
                        index, result.media.len()
                    ),
                }
            })?;
            // Read from CAS store
            let data = tokio::fs::read(&media_ref.path).await.map_err(|e| {
                NikaError::MediaInputError {
                    source: format!("cas:{}", media_ref.path.display()),
                    reason: format!("failed to read from CAS: {}", e),
                }
            })?;
            Ok((data, media_ref.mime_type.clone()))
        }
        MediaInput::Url(url) => {
            // Download from URL with size guard and timeout (B5)
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|e| NikaError::MediaInputError {
                    source: format!("url:{}", url),
                    reason: format!("HTTP client error: {}", e),
                })?;

            let response = client.get(url).send().await.map_err(|e| {
                NikaError::MediaInputError {
                    source: format!("url:{}", url),
                    reason: format!("download failed: {}", e),
                }
            })?;

            // Pre-check Content-Length if available
            if let Some(content_length) = response.content_length() {
                if content_length > BINARY_MAX_SIZE {
                    return Err(NikaError::MediaInputError {
                        source: format!("url:{}", url),
                        reason: format!(
                            "Content-Length {} exceeds max size {}",
                            content_length, BINARY_MAX_SIZE
                        ),
                    });
                }
            }

            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();

            // Stream response body with size abort
            let mut data = Vec::new();
            let mut stream = response.bytes_stream();
            use futures::StreamExt;
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| NikaError::MediaInputError {
                    source: format!("url:{}", url),
                    reason: format!("failed to read response body: {}", e),
                })?;
                data.extend_from_slice(&chunk);
                if data.len() as u64 > BINARY_MAX_SIZE {
                    return Err(NikaError::MediaInputError {
                        source: format!("url:{}", url),
                        reason: format!(
                            "download size exceeds max {} bytes (aborted)",
                            BINARY_MAX_SIZE
                        ),
                    });
                }
            }

            // Cross-validate: detect MIME from magic bytes, prefer over Content-Type
            let detected = crate::media::detect::detect_mime(&data, None);
            let mime = detected.unwrap_or(content_type);
            Ok((data, mime))
        }
    }
}
```

### Converting to rig-core types

```rust
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;

/// Map MIME string to rig-core ImageMediaType.
/// rig-core does NOT have ImageMediaType::from_mime(), so we match manually.
fn mime_to_image_type(mime: &str) -> Option<ImageMediaType> {
    match mime {
        "image/jpeg" | "image/jpg" => Some(ImageMediaType::JPEG),
        "image/png" => Some(ImageMediaType::PNG),
        "image/gif" => Some(ImageMediaType::GIF),
        "image/webp" => Some(ImageMediaType::WEBP),
        "image/svg+xml" => Some(ImageMediaType::SVG),
        _ => None,
    }
}

/// Convert raw bytes + MIME into a rig-core UserContent value.
fn to_user_content(data: &[u8], mime: &str) -> UserContent {
    let b64 = BASE64.encode(data);

    if mime.starts_with("image/") {
        UserContent::Image(Image {
            data: DocumentSourceKind::Base64(b64),
            media_type: mime_to_image_type(mime),
            detail: None,  // Let the provider decide
        })
    } else if mime.starts_with("audio/") {
        UserContent::Audio(Audio {
            data: DocumentSourceKind::Base64(b64),
            media_type: Some(mime.to_string()),
        })
    } else {
        // Fallback: treat as generic document
        UserContent::Document(Document {
            data: DocumentSourceKind::Base64(b64),
            media_type: Some(mime.to_string()),
        })
    }
}
```

### Modified `run_infer()` flow

The key change: when `infer.media` is non-empty, use rig-core's multi-part message API
instead of the plain `&str` prompt.

**Streaming note**: Vision requests with multi-part messages use the **non-streaming fallback
path**. The streaming code path sends a plain `&str` prompt via `stream_prompt()`, which does
not support `UserContent` parts. When `infer.media` is non-empty, we always use the
non-streaming `prompt()` method. This is acceptable because vision responses are typically
shorter than long-form text generation. A future PR can add streaming support for multi-part
messages if needed.

```rust
pub(super) async fn run_infer(
    &self,
    task_id: &Arc<str>,
    infer: &InferParams,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
    output_policy: Option<&OutputPolicy>,
) -> Result<String, NikaError> {
    // ... existing prompt resolution + validation ...

    if infer.media.is_empty() {
        // Text-only path -- unchanged, full backward compatibility
        // ... existing infer_with_options() call (may stream) ...
    } else {
        // Vision/audio path -- resolve media inputs and use multi-part API
        // NOTE: always non-streaming (see streaming note above)

        // Check provider capability before attempting vision
        if !provider.supports_image_input() {
            return Err(NikaError::MediaInputUnsupportedType {
                mime_type: "image/*".to_string(),
                provider: provider.name().to_string(),
            });
        }

        let mut media_contents = Vec::new();
        for input in &infer.media {
            let (data, mime) = resolve_media_input(input, datastore).await?;
            media_contents.push(to_user_content(&data, &mime));
        }
        // Call provider with media (non-streaming)
        let result = provider
            .infer_with_media(&prompt, media_contents, &options)
            .await
            .map_err(|e| NikaError::InferError {
                task_id: task_id.to_string(),
                reason: e.to_string(),
            })?;
        Ok(result)
    }
}
```

**Commit**: `feat(runtime): resolve media inputs in run_infer()`

---

## Phase 5: Provider

### Files changed

- `src/provider/rig.rs` -- new `infer_with_media()` method

### Provider capability check

```rust
impl RigProvider {
    /// Check if this provider supports image input (vision).
    /// Most cloud providers do; Mistral native does not (yet).
    pub fn supports_image_input(&self) -> bool {
        match self {
            RigProvider::Claude(_) => true,    // Claude 3+ vision
            RigProvider::OpenAI(_) => true,    // GPT-4o vision
            RigProvider::Gemini(_) => true,    // Gemini Pro Vision
            RigProvider::Groq(_) => true,      // Llama 3.2 Vision via Groq
            RigProvider::DeepSeek(_) => true,  // DeepSeek VL2
            RigProvider::Xai(_) => true,       // Grok vision
            RigProvider::Mistral(_) => false,  // Mistral API: no vision yet
            #[cfg(feature = "native-inference")]
            RigProvider::Native(_) => false,   // Local GGUF: depends on model
        }
    }
}
```

### New method on `RigProvider`

```rust
impl RigProvider {
    /// Infer with media content (vision/audio).
    ///
    /// Builds a multi-part message with text prompt + media content parts,
    /// using rig-core's UserContent types.
    ///
    /// Always non-streaming: the streaming path does not support multi-part
    /// messages. Vision responses are typically short enough that this is fine.
    pub async fn infer_with_media(
        &self,
        prompt: &str,
        media: Vec<UserContent>,
        options: &InferOptions,
    ) -> Result<String, RigInferError> {
        use rig::message::{Message, UserContent};
        use rig::one_or_many::OneOrMany;

        let model_id = options
            .model
            .as_deref()
            .unwrap_or_else(|| self.default_model());
        let max_tokens = options.max_tokens.unwrap_or(8192);

        // Build multi-part user message: media parts + text prompt
        let mut parts: Vec<UserContent> = Vec::with_capacity(media.len() + 1);
        // Add media content first (models typically expect image before text)
        parts.extend(media);
        // Add text prompt
        parts.push(UserContent::text(prompt));

        // Construct Message::User with OneOrMany (not Message::user() which takes String)
        let user_message = Message::User {
            content: OneOrMany::many(parts).map_err(|_| {
                RigInferError::PromptError("media parts list cannot be empty".to_string())
            })?,
        };

        match self {
            RigProvider::Claude(client) => {
                let agent = client
                    .agent(model_id)
                    .max_tokens(max_tokens as u64)
                    .preamble_maybe(options.system.as_deref())
                    .additional_params(None)
                    .build();
                agent.prompt(user_message).await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::OpenAI(client) => {
                let agent = client
                    .agent(model_id)
                    .max_tokens(max_tokens as u64)
                    .preamble_maybe(options.system.as_deref())
                    .additional_params(None)
                    .build();
                agent.prompt(user_message).await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::Gemini(client) => {
                let agent = client
                    .agent(model_id)
                    .max_tokens(max_tokens as u64)
                    .preamble_maybe(options.system.as_deref())
                    .additional_params(None)
                    .build();
                agent.prompt(user_message).await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::Groq(client) => {
                let agent = client
                    .agent(model_id)
                    .max_tokens(max_tokens as u64)
                    .preamble_maybe(options.system.as_deref())
                    .additional_params(None)
                    .build();
                agent.prompt(user_message).await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::DeepSeek(client) => {
                let agent = client
                    .agent(model_id)
                    .max_tokens(max_tokens as u64)
                    .preamble_maybe(options.system.as_deref())
                    .additional_params(None)
                    .build();
                agent.prompt(user_message).await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::Xai(client) => {
                let agent = client
                    .agent(model_id)
                    .max_tokens(max_tokens as u64)
                    .preamble_maybe(options.system.as_deref())
                    .additional_params(None)
                    .build();
                agent.prompt(user_message).await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::Mistral(_) => {
                // Mistral does not support vision -- caller should check
                // supports_image_input() before calling this method.
                Err(RigInferError::PromptError(
                    "Mistral provider does not support vision input".to_string()
                ))
            }
            #[cfg(feature = "native-inference")]
            RigProvider::Native(runtime) => {
                // Native inference via mistral.rs
                // Supports vision via Qwen2VL, LLaVA architectures
                // ModelType::Vision already defined in core/models.rs
                runtime.infer_with_media(prompt, media, options).await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))
            }
        }
    }
}
```

### rig-core types reference (v0.32, already available)

```rust
// rig::message
pub enum UserContent {
    Text(Text),
    Image(Image),
    Audio(Audio),
    Video(Video),
    Document(Document),
    ToolResult(ToolResult),
}

pub struct Image {
    pub data: DocumentSourceKind,          // Base64, Url, Raw, String
    pub media_type: Option<ImageMediaType>, // JPEG, PNG, GIF, WEBP, etc.
    pub detail: Option<ImageDetail>,        // Low, High, Auto (OpenAI-specific)
}

pub enum DocumentSourceKind {
    Base64(String),
    Url(String),
    Raw(Vec<u8>),
    String(String),
    Unknown,
}

// rig::one_or_many
pub enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<T> OneOrMany<T> {
    /// Create from Vec. Returns Err if vec is empty.
    pub fn many(vec: Vec<T>) -> Result<Self, ()>;
}

// Message construction for multi-part:
pub enum Message {
    User { content: OneOrMany<UserContent> },
    Assistant { content: OneOrMany<AssistantContent> },
    // ...
}
// NOTE: Message::user(s) takes Into<String> -- only for text.
// For multi-part, use Message::User { content: OneOrMany::many(parts)? } directly.
```

### Native inference (mistral.rs)

`ModelType::Vision` is already defined in `src/core/models.rs` with variants for Qwen2VL
and LLaVA architectures. The `NativeRuntime` needs a corresponding `infer_with_media()` method
that converts `UserContent` parts into the mistral.rs vision input format.

This is lower priority since native vision models are less commonly used than cloud APIs.

**Commit**: `feat(provider): add vision inference to RigProvider`
**Commit**: `feat(provider): add vision support for native inference (mistral.rs)`

---

## Error Codes (NIKA-320..325)

| Code | Name | Description |
|------|------|-------------|
| 320 | MediaInputFileNotFound | Referenced media file does not exist (**runtime error**, not analysis-time) |
| 321 | MediaInputInvalidRef | Invalid `with:` media reference expression (does not match `^(\w+)\.media\[(\d+)\]$`) |
| 322 | MediaInputInvalidUrl | Invalid URL in media input |
| 323 | MediaInputRequiresSchema13 | `media:` field requires schema @0.13+ |
| 324 | MediaInputError | Runtime media resolution error (read, download, etc.) |
| 325 | MediaInputUnsupportedType | Provider does not support image/audio input (e.g. Mistral) |

```rust
// src/error.rs -- new variants

#[error("[NIKA-320] Media input file not found: '{path}' (task '{task_id}')")]
MediaInputFileNotFound {
    path: String,
    task_id: String,
},

#[error("[NIKA-321] Invalid media reference '{expression}': {reason} (task '{task_id}')")]
MediaInputInvalidRef {
    expression: String,
    reason: String,
    task_id: String,
},

#[error("[NIKA-322] Invalid media URL '{url}': {reason} (task '{task_id}')")]
MediaInputInvalidUrl {
    url: String,
    reason: String,
    task_id: String,
},

#[error("[NIKA-323] media: field requires schema @0.13 or later (current: {current})")]
MediaInputRequiresSchema {
    current: String,
},

#[error("[NIKA-324] Media input error for '{source}': {reason}")]
MediaInputError {
    source: String,
    reason: String,
},

#[error("[NIKA-325] Unsupported media type '{mime_type}' for provider '{provider}'")]
MediaInputUnsupportedType {
    mime_type: String,
    provider: String,
},
```

---

## Commit Sequence

```
 1. feat(ast): add RawMediaInput type + parse media: field in infer/agent
    - RawMediaInput enum (Path/With/Url)
    - parse_media_inputs() helper
    - RawInferAction.media + RawAgentAction.media fields

 2. feat(ast): add AnalyzedMediaInput + validation in analyzer
    - AnalyzedMediaInput enum with validation
    - Path: well-formed check only (NO file existence -- that's runtime)
    - With: regex parse `^(\w+)\.media\[(\d+)\]$` + task reference validation (NIKA-321)
    - Url: well-formed URL check (NIKA-322)
    - Implicit dependency extraction from with: references

 3. feat(ast): add schema @0.13 with media input feature gate
    - SchemaVersion::V13
    - supports_media_input() gate
    - NIKA-323 if media: used with schema < @0.13
    - Schema string: "@0.13" (not "nika/workflow@0.13")

 4. feat(ast): add MediaInput to InferParams + AgentParams + lower()
    - MediaInput enum (File/TaskMedia/Url)
    - InferParams.media + AgentParams.media fields
    - lower_media_inputs() mapping function
    - Deserialize impl updated for InferParams

 5. feat(runtime): resolve media inputs in run_infer()
    - resolve_media_input() -- File/TaskMedia/Url resolution
    - File existence check at runtime (NIKA-320)
    - URL download with content_length() pre-check, streaming + BINARY_MAX_SIZE abort, 30s timeout
    - mime_to_image_type() manual match (no ImageMediaType::from_mime())
    - to_user_content() -- bytes+MIME to rig-core UserContent
    - run_infer() branching: text-only (may stream) vs vision (non-streaming fallback)
    - Provider capability check before vision call (NIKA-325)
    - NIKA-324 for runtime resolution errors

 6. feat(provider): add vision inference to RigProvider
    - infer_with_media() on RigProvider
    - supports_image_input() capability check
    - Message::User { content: OneOrMany::many(parts)? } (not Message::user())
    - additional_params(None) on all agent builders
    - All 6 cloud providers (Claude, OpenAI, Gemini, Groq, DeepSeek, xAI)
    - Mistral returns error (no vision support)

 7. feat(provider): add vision support for native inference (mistral.rs)
    - NativeRuntime.infer_with_media()
    - Qwen2VL + LLaVA architecture support
    - ModelType::Vision routing

 8. test(ast): media input parsing + analysis + lowering
    - RawMediaInput parse tests (path, with, url, invalid)
    - AnalyzedMediaInput validation tests
    - with: expression regex tests
    - Schema @0.13 feature gate tests
    - MediaInput lowering tests

 9. test(runtime): vision inference integration
    - resolve_media_input tests (file, task, url)
    - File not found at runtime -> NIKA-320
    - URL size guard + timeout tests
    - mime_to_image_type tests
    - to_user_content MIME routing tests
    - Provider capability check -> NIKA-325
    - run_infer with media end-to-end (mock provider)

10. docs: update language reference for media: field
    - YAML syntax documentation
    - Supported MIME types
    - Schema @0.13 migration note
    - Streaming limitation note
```

---

## Key Design Decisions

| # | Decision | Choice | Rationale |
|---|----------|--------|-----------|
| D1 | Media field location | On `infer:` and `agent:` verbs | Only LLM verbs accept media input |
| D2 | Source types | path/with/url (3 variants) | Covers local, pipeline, and remote sources |
| D3 | Schema version | @0.13 (new syntax) | New YAML field requires schema bump |
| D4 | Text-only backward compat | Branch in run_infer() | Empty media vec = same path as before |
| D5 | Media ordering | Media before text in message | Models process vision tokens first |
| D6 | URL download | At runtime, not analysis | Don't slow down validation phase |
| D7 | MIME detection | Magic bytes (infer crate) | More reliable than Content-Type headers |
| D8 | rig-core multi-part | Message::User + OneOrMany | Message::user() only takes String |
| D9 | Error code range | NIKA-320..325 | After structured output (300) |
| D10 | Native vision | Separate commit | Lower priority, can be deferred further |
| D11 | File existence | Runtime check (NIKA-320) | File may not exist at analysis time |
| D12 | Streaming | Non-streaming fallback for vision | stream_prompt() does not support UserContent |
| D13 | Provider check | Pre-check before vision call | NIKA-325 early, not cryptic provider error |
| D14 | URL safety | content_length + streaming abort + 30s timeout | Prevent unbounded downloads |
| D15 | with: parser | Regex `^(\w+)\.media\[(\d+)\]$` | Simple, unambiguous, analyzable |

---

## Not In Scope

| Feature | Reason | Timeline |
|---------|--------|----------|
| Video input | rig-core has `UserContent::Video` but few providers support it | Future |
| Audio input to infer | Waiting for mistral.rs audio stabilization | Future |
| Streaming media | Would require chunked upload protocol | Future |
| Streaming vision responses | stream_prompt() does not support UserContent parts | Future PR |
| Media in `exec:`/`fetch:`/`invoke:` | These verbs don't interact with LLMs | N/A |
| `media:` on `output:` | Output media is handled by PR1+PR2 (extraction) | Already done |
| Inline base64 in YAML | Security risk + bad DX (huge YAML files) | Won't do |

---

## Verification Checklist

- [ ] `cargo test` -- all existing tests pass
- [ ] `cargo clippy -- -D warnings` -- zero warnings
- [ ] Schema `"@0.13"` parses correctly (not `"nika/workflow@0.13"`)
- [ ] Schema @0.12 workflows with `media:` emit NIKA-323
- [ ] Schema @0.12 workflows WITHOUT `media:` are unchanged (regression)
- [ ] `infer: "shorthand"` still works (no media, backward compat)
- [ ] `media: [{ path: "./photo.jpg" }]` parses to `RawMediaInput::Path`
- [ ] `media: [{ with: "gen.media[0]" }]` parses to `RawMediaInput::With`
- [ ] `media: [{ url: "https://..." }]` parses to `RawMediaInput::Url`
- [ ] `media: [{ path: "x", url: "y" }]` fails (multiple sources in one entry)
- [ ] `media: [{}]` fails (no source specified)
- [ ] Analyzer validates path is well-formed (no `..` traversal)
- [ ] Analyzer does NOT check file existence (that's runtime)
- [ ] File existence checked at runtime in resolve_media_input() -> NIKA-320
- [ ] Analyzer validates `with:` expression via regex `^(\w+)\.media\[(\d+)\]$`
- [ ] Analyzer validates task reference for `with:` inputs
- [ ] Analyzer adds implicit dependency for `with:` task references
- [ ] Analyzer validates URL format for `url:` inputs
- [ ] Lower maps all 3 variants correctly
- [ ] Runtime resolves `File` -- reads + base64 encodes + detects MIME
- [ ] Runtime resolves `TaskMedia` -- reads from CAS via MediaRef
- [ ] Runtime resolves `Url` -- downloads with content_length check + streaming abort + 30s timeout
- [ ] `mime_to_image_type()` maps jpeg/png/gif/webp/svg correctly
- [ ] `to_user_content()` routes image/* to `UserContent::Image`
- [ ] `to_user_content()` routes audio/* to `UserContent::Audio`
- [ ] Provider checks `supports_image_input()` before vision call
- [ ] Mistral provider returns NIKA-325 for vision requests
- [ ] Provider uses `Message::User { content: OneOrMany::many(parts)? }` (not `Message::user()`)
- [ ] All agent builders include `additional_params(None)`
- [ ] Vision requests use non-streaming fallback path
- [ ] Text-only `infer:` calls produce identical output (regression)
- [ ] Error codes NIKA-320..325 are all tested
- [ ] `InferParams` deserialization backward-compatible (media defaults to empty)

---

## Dependencies on PR1 + PR2

This PR requires the following from PR1 + PR2 to be merged:

| From | What | Used by |
|------|------|---------|
| PR1 | `MediaRef` type | `TaskMedia` resolution reads `TaskResult.media[N]` |
| PR1 | `src/media/detect.rs` (MIME detection) | `resolve_media_input()` for File and Url |
| PR1 | CAS store (`src/media/store.rs`) | `TaskMedia` reads from CAS path |
| PR1 | `ContentBlock` with image/audio/resource types | N/A (PR3 is input, not output) |
| PR2 | `ArtifactFormat::Binary` | N/A (not directly used, but validates pipeline) |
| PR2 | `base64` crate (direct dependency) | base64 encoding for rig-core |
| PR2 | `infer` crate (magic byte detection) | MIME detection for File and Url inputs |

---

## YAML Examples

### Vision analysis (local file)

```yaml
schema: "@0.13"
name: image-analysis

tasks:
  analyze:
    infer:
      prompt: "Describe this image in detail. What objects, colors, and composition do you see?"
      media:
        - path: "./assets/photo.jpg"
```

### Chain: generate then analyze (task media reference)

```yaml
schema: "@0.13"
name: generate-and-analyze

tasks:
  generate:
    invoke:
      tool: image_gen
      input:
        prompt: "A butterfly logo"
    # Media auto-captured into TaskResult.media[] by PR1+PR2

  analyze:
    depends_on: [generate]
    infer:
      prompt: "Rate this logo design from 1-10 and explain your reasoning."
      media:
        - with: generate.media[0]
```

### Multi-image comparison (URLs)

```yaml
schema: "@0.13"
name: compare-images

tasks:
  compare:
    infer:
      prompt: "Compare these two images. Which has better composition?"
      media:
        - url: "https://example.com/image-a.png"
        - url: "https://example.com/image-b.png"
```

### Agent with vision (mixed sources)

```yaml
schema: "@0.13"
name: agent-vision

tasks:
  research:
    agent:
      prompt: "Analyze this screenshot and the reference image. Identify UI improvements."
      media:
        - path: "./screenshots/current-ui.png"
        - url: "https://example.com/reference-design.png"
      tools: [web_search, screenshot]
      max_iterations: 5
```
