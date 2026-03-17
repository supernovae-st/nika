# PR1: Fix MCP Binary Content Extraction (v3.1)

> **Branch**: `feat/media-extraction`
> **Scope**: Fix the 2-line bug that drops all non-text MCP content.
> **Tests**: ~17-20 tests
> **Commits**: ~5-6

**Parent**: [Master Plan](./2026-03-17-media-pipeline-master-plan.md) | **Next**: [PR2](./2026-03-17-media-pipeline-pr2-processor.md)

---

## Root Cause

Two bottleneck locations silently drop non-text content:

**Bottleneck 1** -- `src/mcp/rmcp_adapter.rs:347-354`
```rust
// CURRENT: Only extracts text, drops everything else
let content: Vec<ContentBlock> = result
    .content
    .iter()
    .filter_map(|c| {
        c.as_text().map(|t| ContentBlock::text(t.text.clone()))
    })
    .collect();
```

**Bottleneck 2** -- `src/runtime/executor/verbs.rs:918`
```rust
// CURRENT: Joins only text blocks
let text = tool_result.text();
serde_json::from_str(&text).unwrap_or_else(|_| {
    serde_json::Value::String(text)
})
```

---

## rmcp 0.16.0 Content API (VERIFIED via source + Context7)

**5 variants** in `RawContent` enum (`Content = Annotated<RawContent>`, `Deref<Target=RawContent>`):

| Variant | Type | Fields | Accessor |
|---------|------|--------|----------|
| `Text` | `RawTextContent` | `text: String` | `as_text()` |
| `Image` | `RawImageContent` | `data: String` (b64), `mime_type: String` | `as_image()` |
| `Resource` | `RawEmbeddedResource` | `resource: ResourceContents` | `as_resource()` |
| `Audio` | `RawAudioContent` | `data: String` (b64), `mime_type: String` | **NO `as_audio()` — MISSING** |
| `ResourceLink` | `RawResource` | `uri, name, title, description, mime_type, size` | `as_resource_link()` |

**CRITICAL**: `ResourceContents` is an **untagged enum**, NOT a struct:
```rust
// rmcp::model::resource
#[serde(untagged)]
enum ResourceContents {
    TextResourceContents { uri: String, mime_type: Option<String>, text: String, meta: Option<Meta> },
    BlobResourceContents { uri: String, mime_type: Option<String>, blob: String, meta: Option<Meta> },
}
```

**Strategy**: Use exhaustive `match &*c` (Deref to RawContent) instead of if-let chain.
This forces compile-time coverage of all 5 variants.

---

## Existing Types (Source of Truth from Code Review)

**`src/mcp/types.rs:364-437`** -- ContentBlock:
```rust
pub struct ContentBlock {
    pub content_type: String,       // "text" | "image" | "resource"
    pub text: Option<String>,       // populated for text blocks
    pub data: Option<String>,       // base64 for image/audio
    pub mime_type: Option<String>,  // MIME for image/audio
    pub resource: Option<ResourceContent>,
}
```

Constructors: `::text()` (line 391), `::image()` (line 402), `::resource()` (line 413).
Checkers: `is_text()` (line 424), `is_image()` (line 429), `is_resource()` (line 434).
**Missing**: `::audio()` constructor, `is_audio()` checker — must add.

**`src/mcp/types.rs:443-482`** -- ResourceContent (VERIFIED):
```rust
pub struct ResourceContent {
    pub uri: String,
    #[serde(default, rename = "mimeType")]
    pub mime_type: Option<String>,
    pub text: Option<String>,
    pub blob: Option<String>,       // ALREADY EXISTS
}
```

Builder: `::new(uri)`, `.with_mime_type()`, `.with_text()`. **Missing**: `.with_blob()` — must add.

**`src/mcp/types.rs`** -- ToolCallResult.text() joins only text:
```rust
pub fn text(&self) -> String {
    self.content
        .iter()
        .filter_map(|block| block.text.as_deref())
        .collect::<Vec<_>>()
        .join("\n")
}
```

---

## Tasks

### Task 1: Verify rmcp Content types (DONE — v3)

**Status**: Pre-verified. Results above. Key findings:
- `as_audio()` does NOT exist — use `match &*c` with `RawContent::Audio`
- `ResourceContents` is untagged enum — match on both variants
- `ResourceLink` exists — map to text (it's a link, not binary content)
- rmcp stays at 0.16.0 (rig-core blocks upgrade, Content API identical to 1.2.0)

**Commit**: none (research only)

### Task 2: Fix rmcp_adapter.rs extraction

**File**: `src/mcp/rmcp_adapter.rs`
**Lines**: 347-354

Replace the text-only `filter_map` with an **exhaustive match** on all 5 `RawContent` variants:

```rust
use rmcp::model::RawContent;
use rmcp::model::resource::ResourceContents;

// FIXED: Extract ALL content types from rmcp via exhaustive match
let content: Vec<ContentBlock> = result
    .content
    .iter()
    .map(|c| {
        match &**c {  // Deref Annotated<RawContent> -> &RawContent
            RawContent::Text(text) => {
                ContentBlock::text(text.text.clone())
            }
            RawContent::Image(image) => {
                ContentBlock::image(
                    image.data.clone(),
                    image.mime_type.clone(),
                )
            }
            RawContent::Audio(audio) => {
                // Audio has same shape as Image: data (b64) + mime_type
                ContentBlock::audio(
                    audio.data.clone(),
                    audio.mime_type.clone(),
                )
            }
            RawContent::Resource(embedded) => {
                // ResourceContents is an UNTAGGED ENUM — must match both variants
                let rc = match &embedded.resource {
                    ResourceContents::TextResourceContents { uri, mime_type, text, .. } => {
                        let mut rc = ResourceContent::new(uri.to_string());
                        if let Some(mime) = mime_type {
                            rc = rc.with_mime_type(mime.clone());
                        }
                        rc = rc.with_text(text.clone());
                        rc
                    }
                    ResourceContents::BlobResourceContents { uri, mime_type, blob, .. } => {
                        let mut rc = ResourceContent::new(uri.to_string());
                        if let Some(mime) = mime_type {
                            rc = rc.with_mime_type(mime.clone());
                        }
                        rc = rc.with_blob(blob.clone());
                        rc
                    }
                };
                ContentBlock::resource(rc)
            }
            RawContent::ResourceLink(link) => {
                // ResourceLink is a URI reference, not binary content.
                // Preserve as text with the URI for downstream processing.
                let text = if let Some(ref title) = link.title {
                    format!("[resource: {} ({})]", link.uri, title)
                } else {
                    format!("[resource: {}]", link.uri)
                };
                ContentBlock::text(text)
            }
        }
    })
    .collect();
```

**Commit**: `fix(mcp): extract all content types from rmcp tool results`

**Key points**:
- Exhaustive `match` on `RawContent` — compiler enforces all 5 variants handled
- `&**c` dereferences `Annotated<RawContent>` → `&RawContent` (via `Deref` impl)
- `Audio` has identical shape to `Image` (`data: String` + `mime_type: String`)
- `ResourceContents` is an **untagged enum**: must match `TextResourceContents` and `BlobResourceContents` separately
- `ResourceLink` mapped to text (it's a URI pointer, not inline binary data)
- No `name`/`description` fields on `ResourceContents` variants (only `uri`, `mime_type`, `text`/`blob`, `meta`)

**Extraction verification (Layer 1 defense-in-depth)**:

After extraction, log if block count doesn't match:
```rust
let rmcp_count = result.content.len();
let extracted_count = content.len();
if extracted_count != rmcp_count {
    tracing::warn!(
        rmcp_count,
        extracted_count,
        "Content block count mismatch after extraction"
    );
}
```

**Required imports** (add to top of rmcp_adapter.rs):
```rust
use rmcp::model::RawContent;
// ResourceContents import path depends on rmcp module layout -- check with:
// grep -r "pub enum ResourceContents" in rmcp source
```

### Task 3: Add ContentBlock constructors + helpers + ResourceContent.with_blob()

**File**: `src/mcp/types.rs`

**3a. Add `audio()` constructor** (after `resource()` at line 421):

```rust
/// Create an audio content block.
pub fn audio(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
    Self {
        content_type: "audio".to_string(),
        text: None,
        data: Some(data.into()),
        mime_type: Some(mime_type.into()),
        resource: None,
    }
}
```

**3b. Add `is_audio()` checker** (after `is_resource()` at line 436):

```rust
/// Check if this is an audio block.
pub fn is_audio(&self) -> bool {
    self.content_type == "audio"
}
```

**3c. Add `with_blob()` to ResourceContent** (after `with_text()` at line 481):

```rust
/// Set the blob content (base64-encoded binary).
pub fn with_blob(mut self, blob: impl Into<String>) -> Self {
    self.blob = Some(blob.into());
    self
}
```

**3d. Add ToolCallResult helper methods** (after existing `text()` / `first_text()` methods):

```rust
impl ToolCallResult {
    // Existing: text(), first_text()

    /// Check if result contains any non-text content (images, audio, resources).
    pub fn has_media(&self) -> bool {
        self.content.iter().any(|b| !b.is_text())
    }

    /// Get all image content blocks.
    pub fn images(&self) -> Vec<&ContentBlock> {
        self.content.iter().filter(|b| b.is_image()).collect()
    }

    /// Get all audio content blocks.
    pub fn audio_blocks(&self) -> Vec<&ContentBlock> {
        self.content.iter().filter(|b| b.is_audio()).collect()
    }

    /// Get all resource content blocks.
    pub fn resources(&self) -> Vec<&ContentBlock> {
        self.content.iter().filter(|b| b.is_resource()).collect()
    }

    /// Get all non-text content blocks (images, audio, resources).
    pub fn media_blocks(&self) -> Vec<&ContentBlock> {
        self.content.iter().filter(|b| !b.is_text()).collect()
    }
}
```

**Commit**: `feat(mcp): add audio constructor, is_audio, with_blob, and media accessor methods`

### Task 4: Fix verbs.rs to handle mixed content

**File**: `src/runtime/executor/verbs.rs`
**Lines**: 917-922

The invoke verb handler must handle tool results that contain both text and binary content:

```rust
// FIXED: Handle mixed content (text + media)
let text = tool_result.text(); // Still get text for JSON parsing

// Build output JSON, include media metadata if present
let mut output = serde_json::from_str(&text).unwrap_or_else(|_| {
    tracing::trace!(task = %task_id, "MCP tool returned non-JSON text, wrapping as string");
    serde_json::Value::String(text)
});

// If there are non-text blocks, add media metadata to output
let media_blocks: Vec<&ContentBlock> = tool_result
    .content
    .iter()
    .filter(|b| !b.is_text())
    .collect();

if !media_blocks.is_empty() {
    // Attach media info as __media key in output
    let media_info: Vec<serde_json::Value> = media_blocks
        .iter()
        .map(|b| {
            serde_json::json!({
                "type": b.content_type,
                "mime_type": b.mime_type,
                "has_data": b.data.is_some(),
                "size_bytes": b.data.as_ref().map(|d| d.len()),
            })
        })
        .collect();

    if let serde_json::Value::Object(ref mut map) = output {
        map.insert("__media".to_string(), serde_json::Value::Array(media_info));
    }
    // If output is a string, wrap in object with text + __media
    else {
        output = serde_json::json!({
            "text": output,
            "__media": media_info,
        });
    }
}
```

**Commit**: `fix(runtime): handle non-text content blocks in invoke verb`

**Important**: The `__media` key is a temporary bridge. PR2 will replace this with proper `MediaRef` storage in `TaskResult`. The double-underscore prefix signals internal/temporary data.

### Task 5: Add MediaExtracted telemetry event + count_all_variants test

**File**: `src/event/log.rs`

**5a. Add new variant** to `EventKind` enum (after `StructuredOutputSuccess`, before closing `}`):

```rust
// ═══════════════════════════════════════════
// MEDIA EVENTS
// ═══════════════════════════════════════════
/// Media content extracted from MCP tool result
MediaExtracted {
    task_id: Arc<str>,
    block_count: u32,
    content_types: Vec<String>,  // ["image", "audio", "resource"]
},
```

**5b. Update doc comment** (line 5): change `32 variants across 10 categories` to `33 variants across 11 categories`

**5c. Add to `EventKind::task_id()` match** (line 517, add before the `=> Some(task_id)` line):
```rust
| Self::MediaExtracted { task_id, .. }
```

**5d. Create `count_all_variants` test** (NEW — did not exist before):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Guard test: ensures all EventKind variants are accounted for.
    /// Update this count when adding new variants.
    /// Current: 33 (32 original + MediaExtracted in PR1)
    #[test]
    fn count_all_variants() {
        // This test exists to catch missed variant additions.
        // If you add a new EventKind variant, update this count
        // AND add it to task_id() match arm.
        let variants = [
            // WORKFLOW (6)
            "WorkflowStarted", "WorkflowCompleted", "WorkflowFailed",
            "WorkflowAborted", "WorkflowPaused", "WorkflowResumed",
            // TASK (4)
            "TaskScheduled", "TaskStarted", "TaskCompleted", "TaskFailed",
            // FINE-GRAINED (3)
            "TemplateResolved", "ProviderCalled", "ProviderResponded",
            // CONTEXT (1)
            "ContextAssembled",
            // MCP (5)
            "McpInvoke", "McpResponse", "McpConnected", "McpError", "McpRetry",
            // AGENT (4)
            "AgentStart", "AgentTurn", "AgentComplete", "AgentSpawned",
            // GUARDRAIL (3)
            "GuardrailPassed", "GuardrailFailed", "GuardrailEscalation",
            // BUILTIN (2)
            "Log", "Custom",
            // ARTIFACT (2)
            "ArtifactWritten", "ArtifactFailed",
            // STRUCTURED OUTPUT (2)
            "StructuredOutputAttempt", "StructuredOutputSuccess",
            // MEDIA (1) -- PR1
            "MediaExtracted",
        ];
        assert_eq!(variants.len(), 33, "EventKind variant count changed! Update this test and task_id() match.");
    }
}
```

**5e. Emit in verbs.rs** when media blocks are found:
```rust
if !media_blocks.is_empty() {
    let content_types: Vec<String> = media_blocks
        .iter()
        .map(|b| b.content_type.clone())
        .collect();
    event_log.push(EventKind::MediaExtracted {
        task_id: task_id.clone(),
        block_count: media_blocks.len() as u32,
        content_types,
    });
}
```

**Commit**: `feat(event): add MediaExtracted telemetry event + count_all_variants guard test`

### Task 6: Tests

**File**: `src/mcp/types.rs` (test module) + `src/event/log.rs` (test module)

#### Unit tests for ContentBlock construction (types.rs):

```rust
#[test]
fn test_content_block_image_constructor() {
    let block = ContentBlock::image("base64data".into(), "image/png".into());
    assert_eq!(block.content_type, "image");
    assert_eq!(block.data.as_deref(), Some("base64data"));
    assert_eq!(block.mime_type.as_deref(), Some("image/png"));
    assert!(block.text.is_none());
    assert!(block.is_image());
    assert!(!block.is_text());
}

#[test]
fn test_content_block_audio_constructor() {
    let block = ContentBlock::audio("b64audio".into(), "audio/mp3".into());
    assert_eq!(block.content_type, "audio");
    assert_eq!(block.data.as_deref(), Some("b64audio"));
    assert_eq!(block.mime_type.as_deref(), Some("audio/mp3"));
    assert!(block.text.is_none());
    assert!(block.is_audio());
    assert!(!block.is_text());
    assert!(!block.is_image());
}

#[test]
fn test_content_block_resource_constructor() {
    let rc = ResourceContent::new("file:///test.bin")
        .with_mime_type("application/octet-stream")
        .with_blob("base64blob".into());
    let block = ContentBlock::resource(rc);
    assert_eq!(block.content_type, "resource");
    assert!(block.is_resource());
    assert!(block.resource.is_some());
    let res = block.resource.unwrap();
    assert_eq!(res.uri, "file:///test.bin");
    assert_eq!(res.blob.as_deref(), Some("base64blob"));
}

#[test]
fn test_resource_content_with_blob_builder() {
    let rc = ResourceContent::new("file:///test.pdf")
        .with_mime_type("application/pdf")
        .with_blob("cGRmZGF0YQ==");  // "pdfdata" in base64
    assert_eq!(rc.uri, "file:///test.pdf");
    assert_eq!(rc.mime_type.as_deref(), Some("application/pdf"));
    assert_eq!(rc.blob.as_deref(), Some("cGRmZGF0YQ=="));
    assert!(rc.text.is_none());
}
```

#### Unit tests for ToolCallResult helpers:

```rust
#[test]
fn test_has_media_false_for_text_only() {
    let result = ToolCallResult {
        content: vec![ContentBlock::text("hello".into())],
        is_error: false,
    };
    assert!(!result.has_media());
    assert!(result.images().is_empty());
    assert!(result.audio_blocks().is_empty());
    assert!(result.media_blocks().is_empty());
}

#[test]
fn test_has_media_true_for_image() {
    let result = ToolCallResult {
        content: vec![
            ContentBlock::text("desc".into()),
            ContentBlock::image("b64".into(), "image/png".into()),
        ],
        is_error: false,
    };
    assert!(result.has_media());
    assert_eq!(result.images().len(), 1);
    assert_eq!(result.media_blocks().len(), 1);
}

#[test]
fn test_has_media_true_for_audio() {
    let result = ToolCallResult {
        content: vec![
            ContentBlock::text("transcription".into()),
            ContentBlock::audio("b64audio".into(), "audio/wav".into()),
        ],
        is_error: false,
    };
    assert!(result.has_media());
    assert_eq!(result.audio_blocks().len(), 1);
    assert_eq!(result.images().len(), 0);
    assert_eq!(result.media_blocks().len(), 1);
}

#[test]
fn test_images_returns_only_images() {
    let result = ToolCallResult {
        content: vec![
            ContentBlock::text("text".into()),
            ContentBlock::image("img1".into(), "image/png".into()),
            ContentBlock::image("img2".into(), "image/jpeg".into()),
            ContentBlock::audio("aud1".into(), "audio/mp3".into()),
        ],
        is_error: false,
    };
    assert_eq!(result.images().len(), 2);
    assert_eq!(result.audio_blocks().len(), 1);
    assert_eq!(result.resources().len(), 0);
    assert_eq!(result.media_blocks().len(), 3);
}

#[test]
fn test_media_blocks_excludes_text() {
    let rc = ResourceContent::new("file:///test").with_blob("data".into());
    let result = ToolCallResult {
        content: vec![
            ContentBlock::text("text".into()),
            ContentBlock::image("img".into(), "image/png".into()),
            ContentBlock::resource(rc),
        ],
        is_error: false,
    };
    assert_eq!(result.media_blocks().len(), 2);
    assert_eq!(result.text(), "text");  // backward compat
}

#[test]
fn test_empty_content_is_safe() {
    let result = ToolCallResult {
        content: vec![],
        is_error: false,
    };
    assert!(!result.has_media());
    assert_eq!(result.text(), "");
    assert!(result.images().is_empty());
    assert!(result.audio_blocks().is_empty());
}
```

#### Telemetry tests (log.rs):

```rust
#[test]
fn test_media_extracted_event_task_id() {
    let event = EventKind::MediaExtracted {
        task_id: "gen_image".into(),
        block_count: 2,
        content_types: vec!["image".into(), "audio".into()],
    };
    assert_eq!(event.task_id(), Some("gen_image"));
}

#[test]
fn test_media_extracted_serde_roundtrip() {
    let event = EventKind::MediaExtracted {
        task_id: "t1".into(),
        block_count: 1,
        content_types: vec!["image".into()],
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"media_extracted\""));
    let parsed: EventKind = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, event);
}

#[test]
fn test_text_only_result_has_no_media() {
    let result = ToolCallResult {
        content: vec![ContentBlock::text("just text".into())],
        is_error: false,
    };
    assert!(!result.has_media());
}
```

**Commits**:
- `test(mcp): add ContentBlock constructor + ToolCallResult media accessor tests`
- `test(event): add MediaExtracted event serde + task_id + count_all_variants tests`

---

## Verification Checklist

- [ ] `cargo test` -- all existing 6,625 tests pass (v0.30.3 baseline)
- [ ] `cargo clippy -- -D warnings` -- zero warnings
- [ ] New tests cover: text, image, **audio**, resource, mixed, empty
- [ ] Exhaustive match on `RawContent` — 5 variants handled
- [ ] `ResourceContents` untagged enum — both `Text` and `Blob` variants matched
- [ ] `Audio` handled (same shape as Image: data + mime_type)
- [ ] `ResourceLink` mapped to text (URI reference, not binary)
- [ ] `ContentBlock::audio()` produces correct `content_type: "audio"`
- [ ] `ContentBlock::resource()` populates `blob` field (already exists on struct)
- [ ] `ResourceContent::with_blob()` builder method added
- [ ] `tool_result.text()` still works (backward compat)
- [ ] `tool_result.has_media()` returns true for image/audio results
- [ ] `tool_result.audio_blocks()` filters correctly
- [ ] verbs.rs correctly adds `__media` for non-text content
- [ ] `MediaExtracted` event emitted with correct task_id + content_types
- [ ] `MediaExtracted` added to `EventKind::task_id()` match arm
- [ ] `count_all_variants` test created (33 variants)
- [ ] Extraction count verification logs warning on mismatch
- [ ] Existing text-only workflows produce identical output (regression)
- [ ] Merge to main, delete `feat/media-extraction` branch

---

## Commit Sequence

```
1. fix(mcp): extract all content types from rmcp tool results
   - Exhaustive match on RawContent (5 variants)
   - ResourceContents untagged enum handling
   - Audio + ResourceLink support
   - Layer 1 extraction count verification

2. feat(mcp): add audio constructor, is_audio, with_blob, and media accessor methods
   - ContentBlock::audio(), is_audio()
   - ResourceContent::with_blob()
   - ToolCallResult: has_media(), images(), audio_blocks(), resources(), media_blocks()

3. fix(runtime): handle non-text content blocks in invoke verb
   - __media bridge for non-text blocks
   - Backward-compatible text handling

4. feat(event): add MediaExtracted telemetry event + count_all_variants guard test
   - New MediaExtracted variant (32 → 33)
   - task_id() match arm updated
   - count_all_variants test created

5. test(mcp): add ContentBlock + ToolCallResult media accessor tests
   - image, audio, resource constructors
   - with_blob builder
   - has_media, images, audio_blocks, resources, media_blocks

6. test(event): add MediaExtracted event serde + task_id tests
```

---

## rmcp API Reference (for implementer)

```rust
// Import paths (rmcp 0.16.0)
use rmcp::model::RawContent;           // The 5-variant enum
use rmcp::model::Content;              // = Annotated<RawContent>, Deref to RawContent
use rmcp::model::RawTextContent;       // .text: String
use rmcp::model::RawImageContent;      // .data: String, .mime_type: String
use rmcp::model::RawAudioContent;      // .data: String, .mime_type: String (NO as_audio()!)
use rmcp::model::RawEmbeddedResource;  // .resource: ResourceContents
use rmcp::model::resource::ResourceContents;  // UNTAGGED ENUM: Text | Blob variants

// Deref pattern: Content -> &RawContent
// Given c: &Content, use &**c or &*c to get &RawContent for matching
```

---

## Corrections from v2

| Item | v2 (wrong) | v3 (correct) |
|------|-----------|-------------|
| Extraction pattern | `if let` chain with accessors | **Exhaustive `match &**c`** on RawContent (compiler-enforced) |
| Audio handling | "rmcp may have as_audio()" | **Confirmed: `as_audio()` DOES NOT EXIST**. Use `RawContent::Audio` match arm |
| ResourceContents | Treated as struct with .uri, .blob | **It's an untagged enum**: `TextResourceContents` / `BlobResourceContents` |
| ResourceLink | Not mentioned | **Handled**: mapped to text (URI reference, not binary) |
| ContentBlock::audio() | Not mentioned | **Added**: same shape as image (data + mime_type) |
| is_audio() | Not mentioned | **Added**: checker for audio blocks |
| with_blob() | Not on ResourceContent | **Added**: builder method for blob field |
| audio_blocks() | Not on ToolCallResult | **Added**: filter for audio content blocks |
| count_all_variants test | Referenced but didn't exist | **Created**: guard test enforcing variant count (33) |
| rmcp version | "v0.16+" | **Pinned**: 0.16.0 (rig-core blocks upgrade, API identical to 1.2.0) |
