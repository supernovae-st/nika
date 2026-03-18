# PR1: Media Pipeline Core -- Extraction + Processing (v4.3)

> **Branch:** `feat/media-pipeline`
> **Date:** 2026-03-18
> **Baseline:** Nika v0.30.5, 5,221 tests, 32 EventKind variants
> **Supersedes:** `pr1-extraction.md` + `pr2-processor.md` (2026-03-17)
> **Parent:** [Master Plan v4.0](./2026-03-18-media-pipeline-master-plan.md)

**Scope:** ContentBlock struct-to-enum, rmcp extraction fix, `src/media/` module (errors, types,
detect, store, processor), TaskResult.media, 4 new events, invoke verb wiring. No throwaway code.

**Stats:**
- Commits: 16
- Tests: ~45 new (including concurrent CAS stress test)
- New deps: 2 genuinely new (blake3, infer) + 4 zero-cost promotes (base64, mime_guess, mime, bytes)
- Error codes: NIKA-251..256 (6 new)
- Events: 32 -> 36 (4 new)
- New files: 6 (`src/media/{mod,error,types,detect,store,processor}.rs`)
- Modified files: 8

---

## Discoveries in v4.2

6 discoveries from codebase deep-dive, applied throughout this revision.

### D1: Reuse `io::atomic::write_fail()` for CAS

The v4.1 plan used `tempfile::NamedTempFile::persist_noclobber()` for atomic CAS writes.
Discovery: `crate::io::atomic::write_fail(path, &bytes)` already exists in the codebase. It is
async, binary-safe, and uses `create_new(true)` (O_EXCL) for atomic check+create. This means:

- `write_fail` on `Ok(())` = new file stored
- `write_fail` on `Err(AlreadyExists)` = dedup success
- No `tempfile` crate needed at runtime
- CAS store becomes **fully async** (no `spawn_blocking` needed)

### D2: Minimize deps -- promote transitive instead of adding new

Old plan: blake3 + infer + mime_guess + base64 (4 new).
New plan:

| Dep | Version | Status |
|-----|---------|--------|
| `blake3` | 1.8 + `mmap` feature | Genuinely new (Thibaut chose for speed). `mmap` enables `hasher.update_mmap(path)` for efficient read-back verify. |
| `infer` | 0.19 | Genuinely new (magic bytes MIME) |
| `base64` | 0.22 | Zero-cost promote from transitive |
| `mime_guess` | 2.0 | Zero-cost promote from transitive |
| `mime` | 0.3 | Zero-cost promote from transitive |
| `bytes` | 1 | Zero-cost promote from transitive |
| `tempfile` | -- | **NOT NEEDED** at runtime (`io::atomic` handles it) |

### D3: `datastore` is a parameter, not `self.datastore` (B4)

`run_invoke(&self, task_id, invoke, bindings, datastore)` takes `datastore: &RunContext`
as a parameter. The v4.1 plan wrote `self.datastore.set_media(...)` which does not compile.
Fix: use `datastore.set_media(task_id, media_refs)`.

### D4: `TaskExecutor` has no `workspace_root` field (B5)

`TaskExecutor` does not have a `workspace_root` field. It captures `working_dir` from
`std::env::current_dir()` during construction and stores it inside `ToolContext` (private).
Fix: resolve workspace root via `std::env::current_dir()` at CAS construction time, same
pattern as `TaskExecutor::with_policy()`. `NIKA_MEDIA_STORE` env var still takes priority.

### D5: `MediaError::code()` must return `&'static str` (B6)

`NikaError::code()` returns `&'static str` (e.g., `"NIKA-001"`). The v4.1 plan had
`MediaError::code()` returning `u16`, making the `From` impl impossible without format!().
Fix: `code()` returns `&'static str` like `"NIKA-251"`.

### D6: CAS store is fully async

Since `io::atomic::write_fail` uses `tokio::fs` internally, the CAS store methods are
natively async. Read-back verify uses `tokio::fs::read` (also async). No `spawn_blocking`
needed anywhere in the media pipeline.

---

## Bugs Fixed in v4.1 + v4.2

17 bugs found by deep review, all fixed in this revision.

### Blockers (6)

| ID | Bug | Fix | Commits affected |
|----|-----|-----|-----------------|
| B1 | `ContentBlock::Text(String)` -- serde internally tagged cannot deserialize newtype(primitive). All 5 variants must be struct-like or newtype-wrapping-struct. Also missing `#[serde(rename = "mimeType")]` on `mime_type` fields, missing `#[serde(skip_serializing_if)]` on Option fields. | Changed `Text(String)` to `Text { text: String }`. Added `#[serde(rename = "mimeType")]` on Image, Audio, ResourceLink `mime_type` fields. Added `#[serde(skip_serializing_if = "Option::is_none")]` on all Option fields. Updated ALL constructors, pattern matches, and helpers. | 1, 3, 8, 12, 13 |
| B2 | `RunContext.results` is `DashMap` not `RwLock<HashMap>` -- `self.results.read().get(task_id)` does not compile. | Changed to `self.results.get(task_id).map(\|r\| r.value().clone())` | 10 |
| B3 | `run_invoke` returns `String` (text output), cannot carry `Vec<MediaRef>` back to runner. Media refs are lost. | Added `media_staging: Arc<DashMap<Arc<str>, Vec<MediaRef>>>` side-channel to `RunContext`. `run_invoke` calls `set_media()`, runner calls `take_media()` after building `TaskResult`. | 10, 12 |
| B4 | v4.1 wrote `self.datastore.set_media(...)` but `datastore` is a **parameter** to `run_invoke`, not a field on `TaskExecutor`. | Changed to `datastore.set_media(task_id, media_refs)` -- uses the `datastore: &RunContext` parameter. | 12 |
| B5 | v4.1 wrote `self.workspace_root` but `TaskExecutor` has **no such field**. It captures `working_dir` from `current_dir()` at construction into a private `ToolContext`. | Changed to `std::env::current_dir().unwrap_or_else(\|\| PathBuf::from("."))` at CAS construction time, same pattern as `TaskExecutor::with_policy()`. `NIKA_MEDIA_STORE` env var still overrides. | 12 |
| B6 | v4.1 had `MediaError::code()` returning `u16` but `NikaError::code()` returns `&'static str` (e.g., `"NIKA-001"`). The `From` impl and the match arm `Self::MediaError(e) => e.code()` would not compile. | Changed `code()` to return `&'static str`: `MimeDetectionFailed => "NIKA-251"`, etc. | 4 |

### High (5)

| ID | Bug | Fix | Commits affected |
|----|-----|-----|-----------------|
| H1 | ~~CAS `store()` does sync filesystem I/O inside async executor.~~ **Resolved by D1**: CAS now uses `io::atomic::write_fail` which is natively async. | CAS store is fully async via `io::atomic`. No `spawn_blocking` needed. | 7, 12 |
| H2 | `RawResource.name` is `String` not `Option<String>` in rmcp 0.16 -- `l.name.clone()` returns `String`, not `Option<String>`. | Changed to `name: Some(l.name.clone())` in the ResourceLink extraction. | 2 |
| H3 | `detect_mime` returns `Ok(Fallback)` when both magic bytes and server hint fail -- but `MimeDetectionFailed` error exists and is unreachable. | When both magic bytes fail AND server_mime is `None` or `"application/octet-stream"`, return `Err(MimeDetectionFailed)` instead of `Ok(Fallback)`. Removed `Fallback` variant from `DetectionSource`. | 6, 8 |
| H4 | `now.checked_duration_since(modified).ok().flatten()` -- `checked_duration_since` returns `Option<Duration>`, not `Result`. Double-unwrap via `.ok().flatten()` is wrong. | Changed to `now.duration_since(modified).ok()` which handles clock skew gracefully. | 7 |
| H5 | `CasStore::workspace_default()` uses hardcoded relative path `.nika/media/store/` -- breaks if cwd differs from workspace root. | Changed to `workspace_default(workspace_root: &Path)` with `NIKA_MEDIA_STORE` env var override. Caller resolves via `current_dir()` (B5). | 7, 12 |

### Medium (6)

| ID | Bug | Fix | Commits affected |
|----|-----|-----|-----------------|
| M1 | `CasEntry` missing `extension` field -- cannot reconstruct full filename from entry. | Added `extension: String` field to `CasEntry`, parsed from filename in `list()`. | 7 |
| M2 | `MediaBlock` type defined but never used -- dead code. | Removed `MediaBlock` from `types.rs` and `mod.rs` re-exports. | 5, 9 |
| M3 | `MediaStored` event missing timing info -- no way to measure pipeline latency. | Added `pipeline_ms: u64` field to `MediaStored` event. | 11, 12 |
| M4 | `process_all` silently swallows errors -- caller cannot distinguish "no media" from "all failed". | Changed return type to `Vec<Result<(MediaRef, StoreResult), (usize, MediaError)>>` where `usize` is the block index. | 8, 12 |
| M5 | No test for concurrent CAS writes -- race condition in atomic write path untested. | Added concurrent CAS write stress test (10 tasks, same data) to test plan. | 14 |
| M6 | `MediaRef.size` field name ambiguous (base64 size? decoded size? disk size?). | Renamed to `size_bytes` for consistency with event fields. | 5, 8, 11, 12 |

---

## Phase A: Type Foundation (Commits 1-3)

### Commit 1: `refactor(mcp): ContentBlock struct -> enum (5 variants)`

Convert `ContentBlock` from a flat struct with stringly-typed `content_type` field into a proper
enum with 5 variants. Pure refactor -- zero behavior change.

**File: `src/mcp/types.rs`** -- Replace the struct definition (lines 363-437):

```rust
/// Content block in MCP tool results.
///
/// Represents one of five content types returned by MCP tools:
/// text, image (base64), audio (base64), embedded resource, or resource link.
///
/// Uses internally tagged serde representation. All variants are struct-like
/// (not newtype-wrapping-primitive) because `#[serde(tag = "type")]` requires
/// the inner type to be a map/struct for deserialization.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Plain text content
    Text {
        text: String,
    },

    /// Base64-encoded image with MIME type
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },

    /// Base64-encoded audio with MIME type
    Audio {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },

    /// Embedded resource content (text or blob)
    Resource(ResourceContent),

    /// Resource link (URI reference without inline data)
    ResourceLink {
        uri: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", rename = "mimeType")]
        mime_type: Option<String>,
    },
}
```

**Constructor methods** (keep the same API surface for call sites):

```rust
impl ContentBlock {
    /// Create a text content block.
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// Create an image content block.
    pub fn image(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self::Image {
            data: data.into(),
            mime_type: mime_type.into(),
        }
    }

    /// Create an audio content block.
    pub fn audio(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self::Audio {
            data: data.into(),
            mime_type: mime_type.into(),
        }
    }

    /// Create a resource content block.
    pub fn resource(resource: ResourceContent) -> Self {
        Self::Resource(resource)
    }

    /// Create a resource link content block.
    pub fn resource_link(
        uri: impl Into<String>,
        name: Option<String>,
        mime_type: Option<String>,
    ) -> Self {
        Self::ResourceLink {
            uri: uri.into(),
            name,
            mime_type,
        }
    }

    /// Check if this is a text block.
    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text { .. })
    }

    /// Check if this is an image block.
    pub fn is_image(&self) -> bool {
        matches!(self, Self::Image { .. })
    }

    /// Check if this is an audio block.
    pub fn is_audio(&self) -> bool {
        matches!(self, Self::Audio { .. })
    }

    /// Check if this is a resource block.
    pub fn is_resource(&self) -> bool {
        matches!(self, Self::Resource(_))
    }

    /// Check if this is a resource link block.
    pub fn is_resource_link(&self) -> bool {
        matches!(self, Self::ResourceLink { .. })
    }
}
```

**ToolCallResult methods** -- rewrite `text()` and `first_text()` for enum:

```rust
impl ToolCallResult {
    // ... success(), error() unchanged ...

    /// Extract all text content from the result.
    ///
    /// Joins all text blocks with newlines.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Extract the first text block, if any.
    pub fn first_text(&self) -> Option<&str> {
        self.content.iter().find_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
    }
}
```

**27 call sites across 5 files to update:**

| File | Count | Changes |
|------|-------|---------|
| `src/mcp/types.rs` | 15 | Definition, constructors, predicates, `ToolCallResult.text()`, `ToolCallResult.first_text()`, all tests in `mod tests` |
| `src/mcp/client.rs` | 6 | Lines 1285, 1308, 1319, 1329 (mock constructors use `ContentBlock::text()`), line 2178 (test), cache type references |
| `src/mcp/rmcp_adapter.rs` | 3 | Line 378 (`ContentBlock::text()` in filter_map), content construction |
| `src/mcp/mod.rs` | 1 | Re-export (unchanged -- `ContentBlock` name stays the same) |
| `src/tui/chat_agent.rs` | 2 | Lines 685-701: `format_block` closure must use `match` instead of struct field access |

**`src/tui/chat_agent.rs` lines 685-701** -- replace `format_block` closure:

```rust
let format_block = |c: crate::mcp::types::ContentBlock| -> String {
    match c {
        ContentBlock::Text { text } => text,
        ContentBlock::Image { data, mime_type } => {
            format!("[Image: {} bytes, {}]", data.len(), mime_type)
        }
        ContentBlock::Audio { data, mime_type } => {
            format!("[Audio: {} bytes, {}]", data.len(), mime_type)
        }
        ContentBlock::Resource(res) => {
            res.text
                .unwrap_or_else(|| format!("[Resource: {}]", res.uri))
        }
        ContentBlock::ResourceLink { uri, name, .. } => {
            if let Some(n) = name {
                format!("[ResourceLink: {} ({})]", uri, n)
            } else {
                format!("[ResourceLink: {}]", uri)
            }
        }
    }
};
```

**Test updates in `src/mcp/types.rs`** -- existing tests that access struct fields must switch to
pattern matching or constructor-based assertions. For example:

```rust
// BEFORE (struct field access):
assert_eq!(block.text, Some("Hello".to_string()));
assert_eq!(block.data, Some("SGVsbG8=".to_string()));

// AFTER (pattern match with struct variant):
assert!(matches!(block, ContentBlock::Text { ref text } if text == "Hello"));
assert!(matches!(block, ContentBlock::Image { ref data, .. } if data == "SGVsbG8="));
```

All existing tests must pass. This is a pure refactor with zero behavior change.

---

### Commit 2: `fix(mcp): extract all 5 content types from rmcp results`

**File: `src/mcp/rmcp_adapter.rs`**, function `call_tool()`, lines 372-380.

**Current code** (text-only extraction):

```rust
let content: Vec<ContentBlock> = result
    .content
    .iter()
    .filter_map(|c| {
        c.as_text().map(|t| ContentBlock::text(t.text.clone()))
    })
    .collect();
```

**Replace with exhaustive match on all 5 RawContent variants:**

```rust
use rmcp::model::RawContent;
use rmcp::model::resource::ResourceContents;

let content: Vec<ContentBlock> = result
    .content
    .iter()
    .map(|c| {
        match &**c {  // Deref Annotated<RawContent> -> &RawContent
            RawContent::Text(t) => ContentBlock::text(t.text.clone()),
            RawContent::Image(i) => ContentBlock::image(
                i.data.clone(),
                i.mime_type.clone(),
            ),
            RawContent::Audio(a) => ContentBlock::Audio {
                data: a.data.clone(),
                mime_type: a.mime_type.clone(),
            },
            RawContent::Resource(r) => {
                // ResourceContents is an UNTAGGED enum -- must match both variants
                match &r.resource {
                    ResourceContents::TextResourceContents {
                        text, uri, mime_type, ..
                    } => {
                        let mut rc = ResourceContent::new(uri.to_string());
                        if let Some(mime) = mime_type {
                            rc = rc.with_mime_type(mime.clone());
                        }
                        rc = rc.with_text(text.clone());
                        ContentBlock::resource(rc)
                    }
                    ResourceContents::BlobResourceContents {
                        blob, uri, mime_type, ..
                    } => {
                        let mut rc = ResourceContent::new(uri.to_string());
                        if let Some(mime) = mime_type {
                            rc = rc.with_mime_type(mime.clone());
                        }
                        rc = rc.with_blob(blob.clone());
                        ContentBlock::resource(rc)
                    }
                }
            }
            // [H2 FIX] RawResource.name is String (not Option<String>) in rmcp 0.16
            // Wrap in Some() when mapping to our Option<String> field
            RawContent::ResourceLink(l) => ContentBlock::ResourceLink {
                uri: l.uri.clone(),
                name: Some(l.name.clone()),
                mime_type: l.mime_type.clone(),
            },
        }
    })
    .collect();
```

**IMPORTANT notes:**
- `Audio` has **NO `as_audio()` accessor** in rmcp 0.16 -- must pattern match directly
- `ResourceContents` is an **UNTAGGED enum** with `TextResourceContents` and
  `BlobResourceContents` variants (not a struct)
- `&**c` dereferences `Annotated<RawContent>` -> `&RawContent` via rmcp's `Deref` impl
- Exhaustive `match` ensures compile-time coverage of all 5 variants
- **[H2]** `RawResource.name` is `String` not `Option<String>` -- use `Some(l.name.clone())`

**Layer 1 extraction verification** (add after the `.collect()`):

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

---

### Commit 3: `feat(mcp): add ToolCallResult media helpers`

**File: `src/mcp/types.rs`** -- add to `impl ToolCallResult`:

```rust
impl ToolCallResult {
    // ... existing: success(), error(), text(), first_text() ...

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

    /// Get all non-text content blocks (images, audio, resources, resource links).
    pub fn media_blocks(&self) -> Vec<&ContentBlock> {
        self.content.iter().filter(|b| !b.is_text()).collect()
    }
}
```

**File: `src/mcp/types.rs`** -- add to `impl ResourceContent`:

> **Note:** `ResourceContent` already has a `blob: Option<String>` field (line 456 of types.rs).
> The `with_blob()` builder method below is NEW, but the field itself is EXISTING.

```rust
impl ResourceContent {
    // ... existing: new(), with_mime_type(), with_text() ...

    /// Set the blob content (base64-encoded binary).
    /// NOTE: The `blob` field already exists on ResourceContent.
    /// This builder is new -- it mirrors the existing `with_text()` pattern.
    pub fn with_blob(mut self, blob: impl Into<String>) -> Self {
        self.blob = Some(blob.into());
        self
    }

    /// Set the MIME type if value is Some.
    pub fn with_optional_mime(mut self, mime_type: Option<String>) -> Self {
        if mime_type.is_some() {
            self.mime_type = mime_type;
        }
        self
    }
}
```

---

## Phase B: Media Module (Commits 4-9)

### Commit 4: `feat(media): add error types NIKA-251..256`

**New file: `src/media/error.rs`**

```rust
//! Media pipeline error types (NIKA-251..256)

use std::fmt;
use std::path::PathBuf;

/// Media pipeline errors.
///
/// Error codes NIKA-251 through NIKA-256 cover the media extraction,
/// detection, storage, and processing pipeline.
#[derive(Debug, Clone)]
pub enum MediaError {
    /// NIKA-251: MIME type could not be determined from magic bytes or server hint
    MimeDetectionFailed {
        /// Number of bytes inspected
        inspected_bytes: usize,
        /// Server-provided MIME hint (if any)
        server_hint: Option<String>,
    },

    /// NIKA-252: Media type is recognized but not supported for processing
    UnsupportedMediaType {
        /// Detected MIME type
        mime_type: String,
        /// Reason for rejection
        reason: String,
    },

    /// NIKA-253: Referenced media hash not found in CAS store
    MediaNotFound {
        /// blake3 hash that was looked up
        hash: String,
    },

    /// NIKA-254: CAS read-back verification failed -- stored data does not match original hash
    HashMismatch {
        /// Expected hash
        expected: String,
        /// Actual hash from re-read
        actual: String,
    },

    /// NIKA-255: Failed to write media file to CAS store
    MediaStoreWrite {
        /// Intended file path
        path: PathBuf,
        /// OS-level error
        source: std::io::Error,
    },

    /// NIKA-256: Base64 decoding failed for media content block
    Base64DecodeFailed {
        /// Source description (e.g., "Image block from task gen_image")
        source: String,
        /// Decode error message
        reason: String,
    },
}

impl MediaError {
    /// Get the NIKA error code for this error.
    ///
    /// [B6] Returns `&'static str` to match `NikaError::code()` signature.
    pub fn code(&self) -> &'static str {
        match self {
            Self::MimeDetectionFailed { .. } => "NIKA-251",
            Self::UnsupportedMediaType { .. } => "NIKA-252",
            Self::MediaNotFound { .. } => "NIKA-253",
            Self::HashMismatch { .. } => "NIKA-254",
            Self::MediaStoreWrite { .. } => "NIKA-255",
            Self::Base64DecodeFailed { .. } => "NIKA-256",
        }
    }

    /// Check if this error is potentially recoverable through retry.
    pub fn is_recoverable(&self) -> bool {
        matches!(self, Self::MediaStoreWrite { .. })
    }
}

impl fmt::Display for MediaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MimeDetectionFailed {
                inspected_bytes,
                server_hint,
            } => {
                write!(f, "NIKA-251: MIME detection failed ({inspected_bytes} bytes inspected")?;
                if let Some(hint) = server_hint {
                    write!(f, ", server hint: {hint}")?;
                }
                write!(f, ")")
            }
            Self::UnsupportedMediaType { mime_type, reason } => {
                write!(f, "NIKA-252: unsupported media type '{mime_type}': {reason}")
            }
            Self::MediaNotFound { hash } => {
                write!(f, "NIKA-253: media not found in store: {hash}")
            }
            Self::HashMismatch { expected, actual } => {
                write!(
                    f,
                    "NIKA-254: CAS hash mismatch (expected {expected}, got {actual})"
                )
            }
            Self::MediaStoreWrite { path, source } => {
                write!(f, "NIKA-255: failed to write media store: {}: {source}", path.display())
            }
            Self::Base64DecodeFailed { source, reason } => {
                write!(f, "NIKA-256: base64 decode failed for {source}: {reason}")
            }
        }
    }
}

impl std::error::Error for MediaError {}
```

**File: `src/error.rs`** -- add variant + From impl + code() arm:

```rust
// In NikaError enum:
/// Media pipeline error (NIKA-251..256)
#[error(transparent)]
MediaError(#[from] crate::media::error::MediaError),

// In NikaError::code() match:
// [B6] MediaError::code() returns &'static str, matching NikaError::code() signature
Self::MediaError(e) => e.code(),
```

---

### Commit 5: `feat(media): add MediaRef and MediaType types`

**New file: `src/media/types.rs`**

```rust
//! Media pipeline types: MediaRef, MediaType

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Reference to a media file stored in the CAS.
///
/// This is the primary type used in `TaskResult.media` and `with:` bindings.
/// Serializes to JSON for use in `{{with.task_id.media[0].hash}}` templates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MediaRef {
    /// blake3 hash (64 hex characters)
    pub hash: String,

    /// Detected MIME type (e.g., "image/png", "audio/wav")
    pub mime_type: String,

    /// File size in bytes (decoded, not base64)
    /// [M6] Named `size_bytes` for consistency with event fields
    pub size_bytes: u64,

    /// Absolute path to the stored file
    pub path: PathBuf,

    /// File extension without dot (e.g., "png", "wav")
    pub extension: String,

    /// Task ID that produced this media
    pub created_by: String,
}

/// Broad media type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    Image,
    Audio,
    Document,
    Unknown,
}

impl MediaType {
    /// Classify a MIME type string into a MediaType.
    pub fn from_mime(mime: &str) -> Self {
        if mime.starts_with("image/") {
            Self::Image
        } else if mime.starts_with("audio/") {
            Self::Audio
        } else if mime.starts_with("application/pdf")
            || mime.starts_with("application/vnd.openxmlformats")
            || mime.starts_with("application/msword")
        {
            Self::Document
        } else {
            Self::Unknown
        }
    }
}
```

**[M2]** `MediaBlock` type removed -- it was dead code (never constructed or consumed). The
`MediaProcessor` operates directly on `ContentBlock` -> `MediaRef` without an intermediate type.

---

### Commit 6: `feat(media): implement MIME detection`

**New file: `src/media/detect.rs`**

```rust
//! MIME detection pipeline: magic bytes -> server hint fallback
//!
//! [H3] When both magic bytes AND server hint fail, returns
//! `Err(MimeDetectionFailed)` instead of a silent fallback.
//! This makes the error code NIKA-251 reachable.

use super::error::MediaError;

/// Result of MIME detection.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectedMime {
    /// Detected MIME type (e.g., "image/png")
    pub mime_type: String,

    /// File extension without dot (e.g., "png")
    pub extension: String,

    /// How the MIME type was determined
    pub source: DetectionSource,
}

/// How the MIME type was determined.
///
/// [H3] Removed `Fallback` variant -- unknown data returns `Err(MimeDetectionFailed)`
/// instead of silently falling back to application/octet-stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectionSource {
    /// Determined via magic byte inspection (highest confidence)
    MagicBytes,

    /// Determined via file extension lookup
    Extension,

    /// Accepted from server-provided Content-Type hint
    ServerHint,
}

/// Detect MIME type from binary data with optional server hint.
///
/// Detection pipeline (in order):
/// 1. Magic byte inspection via `infer` crate (first 8192 bytes)
/// 2. If server_mime provided and magic bytes fail, accept server hint
/// 3. If both fail, return `Err(MimeDetectionFailed)` [H3]
///
/// When both magic bytes and server hint are available, cross-validates:
/// if they disagree, prefers magic bytes (more reliable than server headers).
pub fn detect_mime(
    data: &[u8],
    server_mime: Option<&str>,
) -> Result<DetectedMime, MediaError> {
    let inspect_len = data.len().min(8192);
    let sample = &data[..inspect_len];

    // Layer 1: Magic byte detection
    if let Some(kind) = infer::get(sample) {
        let mime_type = kind.mime_type().to_string();
        let extension = kind.extension().to_string();

        // Cross-validate with server hint if available
        if let Some(server) = server_mime {
            if server != mime_type {
                tracing::debug!(
                    detected = %mime_type,
                    server = %server,
                    "MIME mismatch: magic bytes disagree with server hint, using magic bytes"
                );
            }
        }

        return Ok(DetectedMime {
            mime_type,
            extension,
            source: DetectionSource::MagicBytes,
        });
    }

    // Layer 2: Accept server hint if magic bytes failed
    if let Some(server) = server_mime {
        if server != "application/octet-stream" {
            let extension = mime_to_extension(server);
            return Ok(DetectedMime {
                mime_type: server.to_string(),
                extension,
                source: DetectionSource::ServerHint,
            });
        }
    }

    // [H3] Layer 3: Both magic bytes and server hint failed -- return error
    // This makes NIKA-251 reachable instead of silently falling back.
    Err(MediaError::MimeDetectionFailed {
        inspected_bytes: inspect_len,
        server_hint: server_mime.map(|s| s.to_string()),
    })
}

/// Convert a MIME type to a file extension.
///
/// Returns sanitized extension (alphanumeric + hyphen only).
pub fn mime_to_extension(mime: &str) -> String {
    // Try mime_guess first
    let guesses = mime_guess::get_mime_extensions_str(mime);
    if let Some(exts) = guesses {
        if let Some(ext) = exts.first() {
            return sanitize_extension(ext);
        }
    }

    // Manual fallback for common types mime_guess might miss
    let ext = match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "audio/mpeg" => "mp3",
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/ogg" => "ogg",
        "audio/flac" => "flac",
        "application/pdf" => "pdf",
        "application/json" => "json",
        "text/plain" => "txt",
        "text/html" => "html",
        _ => "bin",
    };

    sanitize_extension(ext)
}

/// Sanitize extension to prevent path traversal.
/// Only allows alphanumeric characters and hyphens.
fn sanitize_extension(ext: &str) -> String {
    ext.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect::<String>()
        .to_lowercase()
}
```

---

### Commit 7: `feat(media): implement async CAS store with blake3 + io::atomic`

**New file: `src/media/store.rs`**

```rust
//! Content-Addressable Storage (CAS) with blake3 hashing and io::atomic writes.
//!
//! Layout: `{root}/{hash[0..2]}/{hash[2..]}.{extension}`
//! Example: `.nika/media/store/a1/b2c3d4...f0.png`
//!
//! [D1] Uses `crate::io::atomic::write_fail()` for atomic CAS writes.
//! This is natively async (tokio::fs internally), binary-safe, and uses
//! O_EXCL for atomic check+create. No tempfile crate needed at runtime.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::error::MediaError;

/// Result of a CAS store operation.
#[derive(Debug, Clone)]
pub struct StoreResult {
    /// blake3 hash (64 hex chars)
    pub hash: String,

    /// Final path where the file is stored
    pub path: PathBuf,

    /// File size in bytes
    pub size: u64,

    /// True if this file was already in the store (deduplicated)
    pub deduplicated: bool,

    /// True if read-back verification passed (Layer 3 defense)
    pub verified: bool,

    /// [M3] Pipeline latency in milliseconds (decode -> detect -> hash -> store -> verify)
    pub pipeline_ms: u64,
}

/// Entry in the CAS store.
///
/// [M1] Includes `extension` field for full filename reconstruction.
#[derive(Debug, Clone)]
pub struct CasEntry {
    /// blake3 hash
    pub hash: String,

    /// File path
    pub path: PathBuf,

    /// File size in bytes
    pub size: u64,

    /// File extension without dot (e.g., "png", "wav")
    pub extension: String,
}

/// Result of a cleanup operation.
#[derive(Debug, Clone)]
pub struct CleanResult {
    /// Number of files removed
    pub removed: u64,

    /// Total bytes freed
    pub bytes_freed: u64,
}

/// Content-Addressable Storage backed by blake3.
///
/// [D1] All write operations are async, using `crate::io::atomic::write_fail()`
/// which provides O_EXCL atomic check+create via tokio::fs. No spawn_blocking needed.
pub struct CasStore {
    root: PathBuf,
}

impl CasStore {
    /// Create a CAS store at the given root directory.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Create a CAS store at the workspace default location.
    ///
    /// [H5] Accepts `workspace_root` to avoid relying on cwd.
    /// Respects `NIKA_MEDIA_STORE` env var as override.
    ///
    /// Resolution order:
    /// 1. `NIKA_MEDIA_STORE` env var (absolute path)
    /// 2. `{workspace_root}/.nika/media/store/`
    pub fn workspace_default(workspace_root: &Path) -> Self {
        if let Ok(override_path) = std::env::var("NIKA_MEDIA_STORE") {
            return Self::new(PathBuf::from(override_path));
        }
        Self::new(workspace_root.join(".nika").join("media").join("store"))
    }

    /// Store binary data in the CAS (async).
    ///
    /// [D1] Uses `crate::io::atomic::write_fail()` for atomic writes.
    /// Fully async -- no spawn_blocking needed.
    ///
    /// 1. Compute blake3 hash
    /// 2. Derive path: `{root}/{hash[0..2]}/{hash[2..]}.{extension}`
    /// 3. Create parent directories (async)
    /// 4. Write via `io::atomic::write_fail()` (O_EXCL atomic check+create)
    /// 5. If AlreadyExists, return deduplicated=true
    /// 6. Read-back verify: re-read file, re-hash, compare (Layer 3, async)
    pub async fn store(
        &self,
        data: &[u8],
        extension: &str,
    ) -> Result<StoreResult, MediaError> {
        // [M3] Measure full pipeline latency (hash + write + verify)
        let process_start = std::time::Instant::now();

        let hash = blake3::hash(data).to_hex().to_string();
        let size = data.len() as u64;

        let dir = self.root.join(&hash[..2]);
        let filename = format!("{}.{}", &hash[2..], extension);
        let final_path = dir.join(&filename);

        // Create parent directories (async)
        tokio::fs::create_dir_all(&dir).await.map_err(|e| MediaError::MediaStoreWrite {
            path: dir.clone(),
            source: e,
        })?;

        // [D1] Atomic write via io::atomic::write_fail (O_EXCL)
        match crate::io::atomic::write_fail(&final_path, data).await {
            Ok(()) => {
                // New file stored -- proceed to read-back verify
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // Another thread/process stored the same content -- dedup success
                return Ok(StoreResult {
                    hash,
                    path: final_path,
                    size,
                    deduplicated: true,
                    verified: true, // existing files assumed verified from prior store
                    pipeline_ms: process_start.elapsed().as_millis() as u64,
                });
            }
            Err(e) => {
                return Err(MediaError::MediaStoreWrite {
                    path: final_path,
                    source: e,
                });
            }
        }

        // Layer 3: Read-back verification (async)
        let read_back = tokio::fs::read(&final_path).await.map_err(|e| MediaError::MediaStoreWrite {
            path: final_path.clone(),
            source: e,
        })?;

        let verify_hash = blake3::hash(&read_back).to_hex().to_string();
        if verify_hash != hash {
            // Critical: stored data is corrupted -- remove and error
            let _ = tokio::fs::remove_file(&final_path).await;
            return Err(MediaError::HashMismatch {
                expected: hash,
                actual: verify_hash,
            });
        }

        Ok(StoreResult {
            hash,
            path: final_path,
            size,
            deduplicated: false,
            verified: true,
            pipeline_ms: process_start.elapsed().as_millis() as u64,
        })
    }

    /// Check if a hash exists in the store.
    pub fn exists(&self, hash: &str) -> bool {
        if hash.len() < 3 {
            return false;
        }
        let dir = self.root.join(&hash[..2]);
        // Check if any file in the shard dir starts with the rest of the hash
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&hash[2..])
                {
                    return true;
                }
            }
        }
        false
    }

    /// Read file data by hash (async).
    pub async fn read(&self, hash: &str) -> Result<Vec<u8>, MediaError> {
        if hash.len() < 3 {
            return Err(MediaError::MediaNotFound {
                hash: hash.to_string(),
            });
        }
        let dir = self.root.join(&hash[..2]);
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&hash[2..])
                {
                    return tokio::fs::read(entry.path()).await.map_err(|e| {
                        MediaError::MediaStoreWrite {
                            path: entry.path(),
                            source: e,
                        }
                    });
                }
            }
        }
        Err(MediaError::MediaNotFound {
            hash: hash.to_string(),
        })
    }

    /// List all entries in the store.
    ///
    /// [M1] Each entry includes the `extension` field parsed from the filename.
    pub fn list(&self) -> Vec<CasEntry> {
        let mut entries = Vec::new();
        let Ok(shards) = std::fs::read_dir(&self.root) else {
            return entries;
        };
        for shard in shards.flatten() {
            let shard_name = shard.file_name().to_string_lossy().to_string();
            if shard_name.len() != 2 || !shard.path().is_dir() {
                continue;
            }
            let Ok(files) = std::fs::read_dir(shard.path()) else {
                continue;
            };
            for file in files.flatten() {
                let file_name = file.file_name().to_string_lossy().to_string();
                // Hash = shard_prefix + filename_stem
                let stem = file_name.split('.').next().unwrap_or(&file_name);
                let hash = format!("{}{}", shard_name, stem);
                let size = file.metadata().map(|m| m.len()).unwrap_or(0);
                // [M1] Extract extension from filename (everything after first dot)
                let extension = file_name
                    .split('.')
                    .nth(1)
                    .unwrap_or("")
                    .to_string();
                entries.push(CasEntry {
                    hash,
                    path: file.path(),
                    size,
                    extension,
                });
            }
        }
        entries
    }

    /// Remove all files from the store.
    pub fn clean_all(&self) -> CleanResult {
        let mut removed = 0u64;
        let mut bytes_freed = 0u64;
        for entry in self.list() {
            if let Ok(meta) = std::fs::metadata(&entry.path) {
                bytes_freed += meta.len();
            }
            if std::fs::remove_file(&entry.path).is_ok() {
                removed += 1;
            } else {
                tracing::warn!(path = %entry.path.display(), "failed to remove CAS file");
            }
        }
        CleanResult {
            removed,
            bytes_freed,
        }
    }

    /// Remove files older than the given duration.
    ///
    /// [H4] Uses `now.duration_since(modified).ok()` which returns `None` on
    /// clock skew instead of the incorrect `.ok().flatten()` chain.
    pub fn clean_older_than(&self, duration: Duration) -> CleanResult {
        let mut removed = 0u64;
        let mut bytes_freed = 0u64;
        let now = std::time::SystemTime::now();
        for entry in self.list() {
            let Ok(meta) = std::fs::metadata(&entry.path) else {
                continue;
            };
            let Ok(modified) = meta.modified() else {
                continue;
            };
            // [H4] duration_since returns Err on clock skew, .ok() converts to Option<Duration>
            let Some(age) = now.duration_since(modified).ok() else {
                continue;
            };
            if age > duration {
                bytes_freed += meta.len();
                if std::fs::remove_file(&entry.path).is_ok() {
                    removed += 1;
                } else {
                    tracing::warn!(path = %entry.path.display(), "failed to remove CAS file");
                }
            }
        }
        CleanResult {
            removed,
            bytes_freed,
        }
    }
}
```

---

### Commit 8: `feat(media): implement MediaProcessor pipeline`

**New file: `src/media/processor.rs`**

```rust
//! MediaProcessor: ContentBlock -> decode -> detect -> hash -> store -> MediaRef
//!
//! [D1] The processor is fully async -- CasStore::store() uses io::atomic internally.

use crate::mcp::types::ContentBlock;

use super::detect::detect_mime;
use super::error::MediaError;
use super::store::{CasStore, StoreResult};
use super::types::MediaRef;

/// Maximum base64 input size (100 MB encoded, ~75 MB decoded).
/// Guards against OOM from pathologically large MCP responses.
const MAX_BASE64_INPUT_BYTES: usize = 100 * 1024 * 1024;

/// Processes MCP content blocks into stored media files.
pub struct MediaProcessor {
    store: CasStore,
}

impl MediaProcessor {
    /// Create a new processor with the given CAS store.
    pub fn new(store: CasStore) -> Self {
        Self { store }
    }

    /// Process a single content block (async).
    ///
    /// Returns `Ok(None)` for text blocks (no media to process).
    /// Returns `Ok(Some((MediaRef, StoreResult)))` for media blocks.
    /// Returns `Err` on decode/detect/store failures.
    pub async fn process(
        &self,
        block: &ContentBlock,
        task_id: &str,
    ) -> Result<Option<(MediaRef, StoreResult)>, MediaError> {
        match block {
            // [B1] Text is now a struct variant
            ContentBlock::Text { .. } => Ok(None),

            ContentBlock::Image { data, mime_type } => {
                self.process_base64(data, Some(mime_type.as_str()), task_id).await
            }

            ContentBlock::Audio { data, mime_type } => {
                self.process_base64(data, Some(mime_type.as_str()), task_id).await
            }

            ContentBlock::Resource(rc) => {
                if let Some(blob) = &rc.blob {
                    self.process_base64(
                        blob,
                        rc.mime_type.as_deref(),
                        task_id,
                    ).await
                } else {
                    // Text-only resource, no binary to store
                    Ok(None)
                }
            }

            ContentBlock::ResourceLink { uri, .. } => {
                // ResourceLink is a URI pointer, not inline data.
                // No download in PR1 -- just log and skip.
                tracing::debug!(uri = %uri, "ResourceLink skipped (no inline data)");
                Ok(None)
            }
        }
    }

    /// Process all content blocks, collecting results per block (async).
    ///
    /// [M4] Returns `Vec<Result<..., (usize, MediaError)>>` so the caller can
    /// distinguish "no media found" (empty vec) from "all blocks failed"
    /// (vec of Err). The `usize` is the block index for error attribution.
    ///
    /// Skips text blocks silently (they are not included in the output vec).
    pub async fn process_all(
        &self,
        blocks: &[ContentBlock],
        task_id: &str,
    ) -> Vec<Result<(MediaRef, StoreResult), (usize, MediaError)>> {
        let mut results = Vec::new();
        for (i, block) in blocks.iter().enumerate() {
            // Skip text blocks entirely -- they are not media
            if block.is_text() {
                continue;
            }
            match self.process(block, task_id).await {
                Ok(Some(pair)) => results.push(Ok(pair)),
                Ok(None) => {} // resource with no blob, or resource_link
                Err(e) => {
                    tracing::warn!(
                        task_id = %task_id,
                        block_index = i,
                        error = %e,
                        "Failed to process media block"
                    );
                    results.push(Err((i, e)));
                }
            }
        }
        results
    }

    /// Decode base64 data, detect MIME, and store in CAS (async).
    async fn process_base64(
        &self,
        base64_data: &str,
        server_mime: Option<&str>,
        task_id: &str,
    ) -> Result<Option<(MediaRef, StoreResult)>, MediaError> {
        // Guard: reject empty data
        if base64_data.is_empty() {
            tracing::warn!(task_id = %task_id, "Empty base64 data, skipping");
            return Ok(None);
        }

        // Guard: reject oversized data
        if base64_data.len() > MAX_BASE64_INPUT_BYTES {
            return Err(MediaError::UnsupportedMediaType {
                mime_type: server_mime.unwrap_or("unknown").to_string(),
                reason: format!(
                    "base64 input exceeds maximum size ({} bytes > {} bytes)",
                    base64_data.len(),
                    MAX_BASE64_INPUT_BYTES
                ),
            });
        }

        // Decode base64
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(base64_data)
            .map_err(|e| MediaError::Base64DecodeFailed {
                source: format!("task {task_id}"),
                reason: e.to_string(),
            })?;

        // Detect MIME type
        // [H3] Returns Err(MimeDetectionFailed) if both magic bytes and server hint fail
        let detected = detect_mime(&decoded, server_mime)?;

        // Store in CAS (async via io::atomic)
        let store_result = self.store.store(&decoded, &detected.extension).await?;

        // [M6] size_bytes instead of size
        let media_ref = MediaRef {
            hash: store_result.hash.clone(),
            mime_type: detected.mime_type,
            size_bytes: store_result.size,
            path: store_result.path.clone(),
            extension: detected.extension,
            created_by: task_id.to_string(),
        };

        Ok(Some((media_ref, store_result)))
    }
}
```

---

### Commit 9: `feat(media): register module + add dependencies`

**New file: `src/media/mod.rs`**

```rust
//! Media pipeline: extraction, detection, storage, and processing.
//!
//! Handles binary content (images, audio, documents) from MCP tool results.
//! Uses Content-Addressable Storage (CAS) with blake3 hashing.
//!
//! ## Pipeline
//!
//! ```text
//! ContentBlock (base64) -> decode -> detect MIME -> blake3 hash -> CAS store -> MediaRef
//! ```
//!
//! ## Module Structure
//!
//! - [`error`]: Error types NIKA-251..256
//! - [`types`]: MediaRef, MediaType
//! - [`detect`]: MIME detection (magic bytes + server hint)
//! - [`store`]: CAS operations (blake3 + io::atomic writes)
//! - [`processor`]: MediaProcessor pipeline

pub mod detect;
pub mod error;
pub mod processor;
pub mod store;
pub mod types;

// Re-exports
// [M2] MediaBlock removed -- dead code, never constructed or consumed
pub use detect::{detect_mime, DetectedMime, DetectionSource};
pub use error::MediaError;
pub use processor::MediaProcessor;
pub use store::{CasStore, CleanResult, StoreResult};
pub use types::{MediaRef, MediaType};
```

**File: `src/lib.rs`** -- add after `pub mod mcp;` (line 43):

```rust
pub mod media;
```

**File: `Cargo.toml`** -- add to `[dependencies]`:

```toml
# Media pipeline (2 genuinely new + 4 zero-cost promotes from transitive)
blake3 = { version = "1.8", features = ["mmap"] }
infer = "0.19"
base64 = "0.22"
mime_guess = "2.0"
mime = "0.3"
bytes = "1"
# NOTE: tempfile NOT needed at runtime -- io::atomic handles CAS writes
```

---

## Phase C: Integration (Commits 10-12)

### Commit 10: `feat(store): add media field to TaskResult + media staging`

**File: `src/store/run_context.rs`**

Add `media` field to `TaskResult` struct (line 40):

```rust
/// Task execution result (unified storage)
#[derive(Debug, Clone)]
pub struct TaskResult {
    /// Output as JSON Value (Arc for O(1) cloning of large JSON structures)
    pub output: Arc<Value>,
    /// Execution duration
    pub duration: Duration,
    /// Success or failure status
    pub status: TaskOutcome,
    /// Media files produced by this task (empty for non-media tasks)
    pub media: Vec<crate::media::MediaRef>,
}
```

Update ALL existing constructors to initialize `media: Vec::new()`:

```rust
impl TaskResult {
    pub fn success(output: impl Into<Value>, duration: Duration) -> Self {
        Self {
            output: Arc::new(output.into()),
            duration,
            status: TaskOutcome::Success,
            media: Vec::new(),
        }
    }

    pub fn success_str(output: impl Into<String>, duration: Duration) -> Self {
        Self {
            output: Arc::new(Value::String(output.into())),
            duration,
            status: TaskOutcome::Success,
            media: Vec::new(),
        }
    }

    pub fn failed(error: impl Into<String>, duration: Duration) -> Self {
        Self {
            output: Arc::new(Value::Null),
            duration,
            status: TaskOutcome::Failed(error.into()),
            media: Vec::new(),
        }
    }

    pub fn dependency_failed(dependency: impl Into<String>) -> Self {
        Self {
            output: Arc::new(Value::Null),
            duration: Duration::ZERO,
            status: TaskOutcome::DependencyFailed {
                dependency: dependency.into(),
            },
            media: Vec::new(),
        }
    }

    pub fn skipped(reason: impl Into<String>) -> Self {
        Self {
            output: Arc::new(Value::Null),
            duration: Duration::ZERO,
            status: TaskOutcome::Skipped {
                reason: reason.into(),
            },
            media: Vec::new(),
        }
    }

    /// Attach media references to this result.
    pub fn with_media(mut self, media: Vec<crate::media::MediaRef>) -> Self {
        self.media = media;
        self
    }
}
```

**[B3] Media staging side-channel** -- add to `RunContext`:

```rust
use dashmap::DashMap;
use crate::media::MediaRef;

/// In RunContext struct definition, add:
pub struct RunContext {
    // ... existing fields ...

    /// [B3] Side-channel for media refs produced by invoke tasks.
    /// `run_invoke` returns String (text output) -- media refs are staged here
    /// and picked up by the runner after building TaskResult.
    media_staging: Arc<DashMap<Arc<str>, Vec<MediaRef>>>,
}

impl RunContext {
    /// Stage media refs for a task (called from run_invoke).
    pub fn set_media(&self, task_id: &Arc<str>, media: Vec<MediaRef>) {
        if !media.is_empty() {
            self.media_staging.insert(Arc::clone(task_id), media);
        }
    }

    /// Take staged media refs for a task (called from runner after building TaskResult).
    /// Returns empty vec if no media was staged.
    pub fn take_media(&self, task_id: &Arc<str>) -> Vec<MediaRef> {
        self.media_staging
            .remove(task_id)
            .map(|(_, v)| v)
            .unwrap_or_default()
    }
}

// In RunContext::new(), add:
//   media_staging: Arc::new(DashMap::new()),
```

Update `resolve_path()` (line 243) to handle `media` resolution:

```rust
pub fn resolve_path(&self, path: &str) -> Option<Value> {
    let mut parts = path.splitn(2, '.');
    let task_id = parts.next()?;

    // Check for media resolution: "task_id.media" or "task_id.media[0].hash"
    let output = self.get_output(task_id)?;

    let Some(remaining) = parts.next() else {
        return Some((*output).clone());
    };

    // Intercept media paths
    if remaining == "media" || remaining.starts_with("media.") || remaining.starts_with("media[") {
        // [B2] RunContext.results is DashMap, not RwLock<HashMap>
        let result = self.results.get(task_id)?.value().clone();
        if result.media.is_empty() {
            return Some(Value::Array(vec![]));
        }
        let media_json = serde_json::to_value(&result.media).ok()?;
        if remaining == "media" {
            return Some(media_json);
        }
        // Strip "media" prefix and resolve rest
        let media_remaining = &remaining[5..]; // skip "media"
        if media_remaining.starts_with('.') {
            return crate::binding::jsonpath::resolve(&media_json, &media_remaining[1..])
                .ok()
                .flatten();
        }
        if media_remaining.starts_with('[') {
            return crate::binding::jsonpath::resolve(&media_json, media_remaining)
                .ok()
                .flatten();
        }
        return Some(media_json);
    }

    // Standard path resolution
    match crate::binding::jsonpath::resolve(&output, remaining) {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!(
                path = %remaining,
                error = %e,
                "JSONPath resolution failed for task output"
            );
            None
        }
    }
}
```

#### for_each Media Policy

Media refs are stored PER ITERATION under indexed task IDs (e.g., `step[0]`, `step[1]`).
They are NOT aggregated into the parent task's result.

- `{{with.step[0].media[0].path}}` -- works (per-iteration access)
- `{{with.step.media[0].path}}` -- empty (parent has no media)

Rationale: binary data aggregation (merging images/audio) has no meaningful semantics.
Each iteration's media stands alone. Users wanting all media should iterate with
`for_each` over the results array.

---

### Commit 11: `feat(event): add 4 media events (32 -> 36)`

**File: `src/event/log.rs`**

Add 4 new variants to `EventKind` enum (after `StructuredOutputSuccess`, before closing `}`):

```rust
    // ═══════════════════════════════════════════
    // MEDIA EVENTS
    // ═══════════════════════════════════════════
    /// Media content blocks extracted from MCP tool result
    MediaExtracted {
        task_id: Arc<str>,
        /// Number of non-text content blocks found
        block_count: u32,
        /// Content types found (e.g., ["image", "audio"])
        content_types: Vec<String>,
    },

    /// Single media block processed (decoded + detected)
    MediaProcessed {
        task_id: Arc<str>,
        /// blake3 hash of the decoded content
        hash: String,
        /// Detected MIME type
        mime_type: String,
        /// [M6] File size in bytes (decoded)
        size_bytes: u64,
    },

    /// Media file stored in CAS
    ///
    /// [M3] Includes `pipeline_ms` for measuring end-to-end pipeline latency
    /// (decode + detect + hash + store + verify).
    MediaStored {
        task_id: Arc<str>,
        /// blake3 hash
        hash: String,
        /// File path in CAS store
        path: String,
        /// [M6] File size in bytes
        size_bytes: u64,
        /// Whether read-back verification passed
        verified: bool,
        /// Whether this was a dedup hit
        deduplicated: bool,
        /// [M3] Pipeline latency in milliseconds (decode -> store)
        pipeline_ms: u64,
    },

    /// Media storage failed
    MediaStoreFailed {
        task_id: Arc<str>,
        /// blake3 hash (if available)
        hash: String,
        /// Error description
        reason: String,
    },
```

Update `task_id()` match -- add all 4 before the `=> Some(task_id)` arm:

```rust
            | Self::MediaExtracted { task_id, .. }
            | Self::MediaProcessed { task_id, .. }
            | Self::MediaStored { task_id, .. }
            | Self::MediaStoreFailed { task_id, .. }
```

Update doc comments:
- `src/event/log.rs` line 5: `32 variants` -> `36 variants`
- `src/event/mod.rs` line 6: `34 variants` -> `36 variants` (note: mod.rs was already slightly off)

Update `all_32_variants()` test helper -- rename to `all_variants()` and add 4 new entries:

```rust
/// Helper: build one instance of every EventKind variant (all 36)
fn all_variants() -> Vec<EventKind> {
    let mut v = vec![
        // ... existing 32 variants unchanged ...
        // Media (4) -- PR1
        EventKind::MediaExtracted {
            task_id: "gen_img".into(),
            block_count: 2,
            content_types: vec!["image".into(), "audio".into()],
        },
        EventKind::MediaProcessed {
            task_id: "gen_img".into(),
            hash: "a1b2c3d4".into(),
            mime_type: "image/png".into(),
            size_bytes: 4096,
        },
        EventKind::MediaStored {
            task_id: "gen_img".into(),
            hash: "a1b2c3d4".into(),
            path: ".nika/media/store/a1/b2c3d4.png".into(),
            size_bytes: 4096,
            verified: true,
            deduplicated: false,
            pipeline_ms: 42,
        },
        EventKind::MediaStoreFailed {
            task_id: "gen_img".into(),
            hash: "a1b2c3d4".into(),
            reason: "disk full".into(),
        },
    ];
    v
}
```

Update `wave2_variant_count_is_32` test:

```rust
#[test]
fn wave2_variant_count_is_36() {
    let variants = all_variants();
    assert_eq!(
        variants.len(),
        36,
        "EventKind should have exactly 36 variants"
    );
}
```

Update `wave2_all_variants_serialize_deserialize_roundtrip` to use `all_variants()`.

---

### Commit 12: `feat(runtime): wire MediaProcessor into invoke verb`

**File: `src/runtime/executor/verbs.rs`**, in `run_invoke()` after line 933
(after `let text = tool_result.text();`).

**Current code (lines 928-933):**

```rust
// Extract text and try to parse as JSON
let text = tool_result.text();
serde_json::from_str(&text).unwrap_or_else(|_| {
    tracing::trace!(task = %task_id, "MCP tool returned non-JSON text, wrapping as string");
    serde_json::Value::String(text)
})
```

**Replace with:**

```rust
// Process media content (if any)
if tool_result.has_media() {
    let media_blocks = tool_result.media_blocks();
    let content_types: Vec<String> = media_blocks
        .iter()
        .map(|b| match b {
            // [B1] Text uses struct variant
            ContentBlock::Text { .. } => unreachable!("media_blocks filters text"),
            ContentBlock::Image { .. } => "image".to_string(),
            ContentBlock::Audio { .. } => "audio".to_string(),
            ContentBlock::Resource(_) => "resource".to_string(),
            ContentBlock::ResourceLink { .. } => "resource_link".to_string(),
        })
        .collect();

    self.event_log.emit(EventKind::MediaExtracted {
        task_id: Arc::clone(task_id),
        block_count: media_blocks.len() as u32,
        content_types,
    });

    // [B5] Resolve workspace root via current_dir(), same pattern as TaskExecutor::with_policy()
    // NIKA_MEDIA_STORE env var takes priority inside workspace_default()
    let workspace_root = std::env::current_dir().unwrap_or_else(|_| {
        tracing::warn!("Failed to get current directory for CAS, using /tmp");
        std::path::PathBuf::from("/tmp")
    });
    let store = CasStore::workspace_default(&workspace_root);
    let processor = MediaProcessor::new(store);

    // [D1/D6] CAS is fully async via io::atomic -- no spawn_blocking needed
    let process_results = processor.process_all(&tool_result.content, &task_id.to_string()).await;

    // [M4] process_all returns Vec<Result<..., (usize, MediaError)>>
    // Separate successes from failures, emit events for each
    let mut media_refs = Vec::new();
    for result in process_results {
        match result {
            Ok((media_ref, store_result)) => {
                self.event_log.emit(EventKind::MediaProcessed {
                    task_id: Arc::clone(task_id),
                    hash: media_ref.hash.clone(),
                    mime_type: media_ref.mime_type.clone(),
                    // [M6] size_bytes
                    size_bytes: media_ref.size_bytes,
                });
                self.event_log.emit(EventKind::MediaStored {
                    task_id: Arc::clone(task_id),
                    hash: media_ref.hash.clone(),
                    path: media_ref.path.display().to_string(),
                    // [M6] size_bytes
                    size_bytes: media_ref.size_bytes,
                    verified: store_result.verified,
                    deduplicated: store_result.deduplicated,
                    // [M3] pipeline_ms -- measured per-block in process_one
                    pipeline_ms: store_result.pipeline_ms,
                });
                media_refs.push(media_ref);
            }
            Err((block_index, error)) => {
                self.event_log.emit(EventKind::MediaStoreFailed {
                    task_id: Arc::clone(task_id),
                    hash: String::new(),
                    reason: format!("block {block_index}: {error}"),
                });
            }
        }
    }

    // [B4] Stage media refs in side-channel for runner to pick up
    // Note: `datastore` is a parameter to run_invoke, NOT self.datastore
    datastore.set_media(task_id, media_refs);
}

// Text output flows as before (backward compat)
let text = tool_result.text();
let result = serde_json::from_str(&text).unwrap_or_else(|_| {
    tracing::trace!(task = %task_id, "MCP tool returned non-JSON text, wrapping as string");
    serde_json::Value::String(text)
});
```

Then, in the **runner** (where `TaskResult` is constructed for invoke results), pick up staged media.

**IMPORTANT:** The runner does NOT call `TaskResult::success()` directly for invoke results.
It calls `make_task_result()` (defined in `output.rs`), which handles structured output and
output schema extraction. The `.with_media()` call must happen AFTER `make_task_result` returns:

```rust
// In runner.rs execute_task_iteration(), after line ~803:
// make_task_result is defined in output.rs -- handles structured output, output.schema, etc.
let tr = make_task_result(final_output, effective_output.as_ref(), duration).await;
// [B3] Attach media refs from staging side-channel AFTER make_task_result
let tr = tr.with_media(datastore.take_media(&task_id));
```

**Key points:**
- **[B3]** Media refs flow via `RunContext.media_staging` side-channel, not return value.
- **[B4]** Uses `datastore` parameter (not `self.datastore` -- TaskExecutor has no such field).
- **Runner calls `make_task_result()` (output.rs), NOT `TaskResult::success()` directly.**
  `make_task_result` handles structured output extraction, output schema validation, etc.
  `.with_media()` is chained after it returns the `TaskResult`.
- **[B5]** Resolves workspace root via `current_dir()` at CAS construction time.
- **[D1/D6]** CAS is fully async via `io::atomic`. No `spawn_blocking` needed.
- **[H5]** `CasStore::workspace_default(&workspace_root)` with env var override.
- **[M4]** `process_all` returns per-block `Result`, failures emit `MediaStoreFailed` events.
- Text output preserved identically (backward compat).
- Events emitted inline for each processed block.

**Required imports at top of `verbs.rs`:**

```rust
use crate::media::{CasStore, MediaProcessor};
use crate::mcp::types::ContentBlock;
```

---

## Phase D: Tests (Commits 13-16)

### Commit 13: `test(mcp): ContentBlock enum + media helpers (~10 tests)`

**File: `src/mcp/types.rs`** -- add to existing `#[cfg(test)] mod tests`:

```rust
// ═══════════════════════════════════════════════════════════════
// ContentBlock Enum Tests
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_content_block_text_enum() {
    let block = ContentBlock::text("Hello");
    assert!(block.is_text());
    assert!(!block.is_image());
    assert!(!block.is_audio());
    assert!(!block.is_resource());
    assert!(!block.is_resource_link());
    // [B1] Struct variant pattern match
    assert!(matches!(block, ContentBlock::Text { ref text } if text == "Hello"));
}

#[test]
fn test_content_block_image_enum() {
    let block = ContentBlock::image("SGVsbG8=", "image/png");
    assert!(block.is_image());
    assert!(!block.is_text());
    assert!(matches!(
        block,
        ContentBlock::Image { ref data, ref mime_type }
        if data == "SGVsbG8=" && mime_type == "image/png"
    ));
}

#[test]
fn test_content_block_audio_enum() {
    let block = ContentBlock::audio("b64audio", "audio/wav");
    assert!(block.is_audio());
    assert!(!block.is_text());
    assert!(!block.is_image());
    assert!(matches!(
        block,
        ContentBlock::Audio { ref data, ref mime_type }
        if data == "b64audio" && mime_type == "audio/wav"
    ));
}

#[test]
fn test_content_block_resource_link_enum() {
    let block = ContentBlock::resource_link(
        "file:///tmp/test.txt",
        Some("test.txt".to_string()),
        Some("text/plain".to_string()),
    );
    assert!(block.is_resource_link());
    assert!(!block.is_text());
    assert!(!block.is_resource());
}

#[test]
fn test_content_block_serde_roundtrip_all_variants() {
    // [B1] This test validates the fix: internally tagged serde with struct variants
    let blocks = vec![
        ContentBlock::text("hello"),
        ContentBlock::image("b64", "image/png"),
        ContentBlock::audio("b64", "audio/wav"),
        ContentBlock::resource(ResourceContent::new("file:///test").with_text("content")),
        ContentBlock::resource_link("file:///link", None, None),
    ];
    for (i, block) in blocks.iter().enumerate() {
        let json = serde_json::to_string(block)
            .unwrap_or_else(|e| panic!("variant {i} failed to serialize: {e}"));
        let back: ContentBlock = serde_json::from_str(&json)
            .unwrap_or_else(|e| panic!("variant {i} failed to deserialize: {e}\nJSON: {json}"));
        assert_eq!(*block, back, "variant {i} roundtrip mismatch");
    }
}

#[test]
fn test_content_block_text_json_format() {
    // [B1] Verify the JSON shape for Text is {"type":"text","text":"..."} not {"type":"text","Text":"..."}
    let block = ContentBlock::text("hello world");
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "text");
    assert_eq!(json["text"], "hello world");
}

#[test]
fn test_content_block_image_json_has_mime_type_camel_case() {
    // [B1] Verify mimeType is camelCase in JSON (not mime_type)
    let block = ContentBlock::image("b64data", "image/png");
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "image");
    assert_eq!(json["mimeType"], "image/png");
    assert!(json.get("mime_type").is_none(), "should use mimeType not mime_type");
}

#[test]
fn test_content_block_resource_link_skips_none_fields() {
    // [B1] Verify skip_serializing_if works for Option fields
    let block = ContentBlock::resource_link("file:///link", None, None);
    let json = serde_json::to_string(&block).unwrap();
    assert!(!json.contains("name"), "None name should be skipped");
    assert!(!json.contains("mimeType"), "None mimeType should be skipped");
}

// ═══════════════════════════════════════════════════════════════
// ToolCallResult Media Helper Tests
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_has_media_false_for_text_only() {
    let result = ToolCallResult::success(vec![ContentBlock::text("hello")]);
    assert!(!result.has_media());
    assert!(result.images().is_empty());
    assert!(result.audio_blocks().is_empty());
    assert!(result.media_blocks().is_empty());
}

#[test]
fn test_has_media_true_for_mixed_content() {
    let result = ToolCallResult::success(vec![
        ContentBlock::text("description"),
        ContentBlock::image("b64", "image/png"),
        ContentBlock::audio("b64", "audio/wav"),
    ]);
    assert!(result.has_media());
    assert_eq!(result.images().len(), 1);
    assert_eq!(result.audio_blocks().len(), 1);
    assert_eq!(result.media_blocks().len(), 2);
}

#[test]
fn test_text_preserved_with_media() {
    let result = ToolCallResult::success(vec![
        ContentBlock::text("First line"),
        ContentBlock::image("b64data", "image/png"),
        ContentBlock::text("Second line"),
    ]);
    assert_eq!(result.text(), "First line\nSecond line");
    assert_eq!(result.first_text(), Some("First line"));
    assert!(result.has_media());
}

#[test]
fn test_empty_content_vec() {
    let result = ToolCallResult::success(vec![]);
    assert!(!result.has_media());
    assert_eq!(result.text(), "");
    assert!(result.images().is_empty());
    assert!(result.media_blocks().is_empty());
}
```

---

### Commit 14: `test(media): MIME, CAS, processor (~25 tests)`

**New file or inline tests in each media submodule.**

Tests to include:

```
detect.rs tests:
- detect_mime: PNG magic bytes (89 50 4E 47) -> image/png
- detect_mime: JPEG magic bytes (FF D8 FF) -> image/jpeg
- detect_mime: WAV magic bytes (52 49 46 46) -> audio/wav
- detect_mime: unknown bytes -> Err(MimeDetectionFailed) [H3]
- detect_mime: unknown bytes + server hint "application/octet-stream" -> Err(MimeDetectionFailed) [H3]
- detect_mime: unknown bytes + server hint "image/png" -> Ok(ServerHint)
- detect_mime: magic bytes preferred over conflicting server hint
- mime_to_extension: common MIME types map correctly
- sanitize_extension: rejects path traversal characters

store.rs tests (async):
- store + read roundtrip: write data, read back, compare
- dedup: store same data twice -> second returns deduplicated=true
- exists: true after store, false before
- list: returns stored entries with extension field [M1]
- clean_all: removes all files
- clean_older_than: respects age threshold [H4]
- store: empty dir created on first write
- read: nonexistent hash -> MediaNotFound
- workspace_default: uses workspace_root param [H5]
- workspace_default: respects NIKA_MEDIA_STORE env var [H5]
- [M5] concurrent_cas_writes: 10 tasks store identical data -> all succeed, exactly 1 non-dedup

processor.rs tests (async):
- process: text block -> None [B1 struct variant]
- process: image block -> decodes, detects, stores, returns MediaRef with size_bytes [M6]
- process: invalid base64 -> NIKA-256
- process: empty data -> returns None
- process: oversized data -> rejection
- process_all: mixed blocks -> Vec<Result<...>> with successes and failures [M4]
- process_all: all blocks fail -> vec of Err, not empty vec [M4]
- process_all: text blocks excluded from output vec [M4]
```

**[M5] Concurrent CAS write stress test (async):**

```rust
#[tokio::test]
async fn concurrent_cas_writes_dedup_correctly() {
    use std::sync::Arc;
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(CasStore::new(dir.path()));

    let data: Vec<u8> = b"identical content for all tasks".to_vec();

    let handles: Vec<_> = (0..10)
        .map(|_| {
            let store = Arc::clone(&store);
            let data = data.clone();
            tokio::spawn(async move { store.store(&data, "bin").await })
        })
        .collect();

    let results: Vec<StoreResult> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|h| h.unwrap().unwrap())
        .collect();

    // All should have the same hash
    let hash = &results[0].hash;
    assert!(results.iter().all(|r| &r.hash == hash));

    // Exactly one should be non-deduplicated (the first writer)
    let non_dedup_count = results.iter().filter(|r| !r.deduplicated).count();
    assert_eq!(non_dedup_count, 1, "exactly one writer should be non-dedup");

    // All should be verified
    assert!(results.iter().all(|r| r.verified));
}
```

---

### Commit 15: `test(event): media events (~5 tests)`

**File: `src/event/log.rs`** -- add to test module:

```rust
#[test]
fn test_media_extracted_serde_roundtrip() {
    let event = EventKind::MediaExtracted {
        task_id: "gen_img".into(),
        block_count: 2,
        content_types: vec!["image".into(), "audio".into()],
    };
    let json = serde_json::to_string(&event).unwrap();
    let back: EventKind = serde_json::from_str(&json).unwrap();
    assert_eq!(event, back);
}

#[test]
fn test_media_processed_task_id() {
    let event = EventKind::MediaProcessed {
        task_id: "t1".into(),
        hash: "abc123".into(),
        mime_type: "image/png".into(),
        size_bytes: 4096,
    };
    assert_eq!(event.task_id(), Some("t1"));
}

#[test]
fn test_media_stored_serde_roundtrip() {
    // [M3] Includes pipeline_ms field
    let event = EventKind::MediaStored {
        task_id: "t1".into(),
        hash: "abc123".into(),
        path: ".nika/media/store/ab/c123.png".into(),
        size_bytes: 4096,
        verified: true,
        deduplicated: false,
        pipeline_ms: 42,
    };
    let json = serde_json::to_string(&event).unwrap();
    let back: EventKind = serde_json::from_str(&json).unwrap();
    assert_eq!(event, back);
}

#[test]
fn test_media_store_failed_task_id() {
    let event = EventKind::MediaStoreFailed {
        task_id: "t1".into(),
        hash: "abc123".into(),
        reason: "disk full".into(),
    };
    assert_eq!(event.task_id(), Some("t1"));
}

#[test]
fn wave2_variant_count_is_36() {
    let variants = all_variants();
    assert_eq!(
        variants.len(),
        36,
        "EventKind should have exactly 36 variants"
    );
}
```

---

### Commit 16: `test(runtime): invoke + media integration (~5 tests)`

**File: `src/runtime/executor/verbs.rs`** (test module) or integration test:

```
- Mock MCP tool returns image content -> MediaProcessor runs async [D1] -> TaskResult has media vec via staging [B3]
- Mock MCP tool returns text only -> no media processing, media vec empty
- Mixed text+image response -> text output preserved, image stored in CAS
- Tool error response -> no media processing attempted
- TaskResult.media accessible via resolve_path("task_id.media") [B2 DashMap API]
- Media refs have size_bytes field [M6]
```

---

## Commit Sequence Summary

| # | Commit message | Phase | Files |
|---|---------------|-------|-------|
| 1 | `refactor(mcp): ContentBlock struct -> enum (5 variants)` | A | types.rs, client.rs, rmcp_adapter.rs, chat_agent.rs |
| 2 | `fix(mcp): extract all 5 content types from rmcp results` | A | rmcp_adapter.rs |
| 3 | `feat(mcp): add ToolCallResult media helpers` | A | types.rs |
| 4 | `feat(media): add error types NIKA-251..256` | B | media/error.rs, error.rs |
| 5 | `feat(media): add MediaRef and MediaType types` | B | media/types.rs |
| 6 | `feat(media): implement MIME detection` | B | media/detect.rs |
| 7 | `feat(media): implement async CAS store with blake3 + io::atomic` | B | media/store.rs |
| 8 | `feat(media): implement MediaProcessor pipeline` | B | media/processor.rs |
| 9 | `feat(media): register module + add dependencies` | B | media/mod.rs, lib.rs, Cargo.toml |
| 10 | `feat(store): add media field to TaskResult + media staging` | C | store/run_context.rs |
| 11 | `feat(event): add 4 media events (32 -> 36)` | C | event/log.rs, event/mod.rs |
| 12 | `feat(runtime): wire MediaProcessor into invoke verb` | C | runtime/executor/verbs.rs |
| 13 | `test(mcp): ContentBlock enum + media helpers` | D | mcp/types.rs |
| 14 | `test(media): MIME, CAS, processor` | D | media/{detect,store,processor}.rs |
| 15 | `test(event): media events` | D | event/log.rs |
| 16 | `test(runtime): invoke + media integration` | D | runtime/executor/verbs.rs |

---

## Verification Checklist

- [ ] `cargo test` -- all 5,221 existing tests pass + ~45 new
- [ ] `cargo clippy -- -D warnings` -- zero warnings
- [ ] **[B1]** ContentBlock enum: all 5 variants are struct-like (not newtype-primitive), serde roundtrips
- [ ] **[B1]** JSON has `mimeType` (camelCase) not `mime_type`, Option fields skip when None
- [ ] **[B2]** DashMap API used for `results` access (`.get().value().clone()`)
- [ ] **[B3]** Media refs flow via `media_staging` side-channel, not return value
- [ ] **[B4]** `datastore.set_media()` uses the parameter, not `self.datastore`
- [ ] **[B5]** Workspace root resolved via `current_dir()`, not `self.workspace_root`
- [ ] **[B6]** `MediaError::code()` returns `&'static str` (e.g., `"NIKA-251"`)
- [ ] **[H1/D1]** CAS is fully async via `io::atomic` -- no `spawn_blocking` anywhere
- [ ] **[H2]** `RawResource.name` mapped as `Some(l.name.clone())`
- [ ] **[H3]** `detect_mime` returns `Err(MimeDetectionFailed)` when both sources fail
- [ ] **[H4]** `clean_older_than` uses `now.duration_since(modified).ok()`
- [ ] **[H5]** `CasStore::workspace_default(workspace_root)` with env var override
- [ ] **[M1]** `CasEntry` has `extension` field, `list()` parses it
- [ ] **[M2]** `MediaBlock` type removed (dead code)
- [ ] **[M3]** `MediaStored` event has `pipeline_ms` timing field
- [ ] **[M4]** `process_all` returns `Vec<Result<..., (usize, MediaError)>>`
- [ ] **[M5]** Concurrent CAS stress test passes (10 async tasks, 1 non-dedup)
- [ ] **[M6]** `MediaRef.size_bytes` used consistently everywhere
- [ ] Exhaustive `match &**c` on RawContent -- 5 variants handled
- [ ] ResourceContents untagged enum -- both Text and Blob matched
- [ ] Audio handled (same shape as Image: data + mime_type)
- [ ] ResourceLink mapped to own variant (not flattened to text)
- [ ] `tool_result.text()` backward compat -- joins Text variants only
- [ ] `tool_result.has_media()` / `images()` / `audio_blocks()` / `media_blocks()` work
- [ ] MIME detection: magic bytes, server hint fallback, error on total failure
- [ ] CAS store: async atomic writes, dedup, read-back verify, clean
- [ ] MediaProcessor: full async pipeline base64 -> decode -> detect -> hash -> store -> MediaRef
- [ ] MediaProcessor: empty/oversized/invalid base64 guards
- [ ] TaskResult.media field initialized as empty in all constructors
- [ ] `resolve_path("task_id.media")` returns media JSON
- [ ] 4 new EventKind variants: MediaExtracted, MediaProcessed, MediaStored, MediaStoreFailed
- [ ] `EventKind::task_id()` updated for all 4 new variants
- [ ] Variant count test: 36
- [ ] verbs.rs: MediaProcessor wired in async, events emitted, text output unchanged
- [ ] No `__media` bridge -- direct integration via staging side-channel
- [ ] Text-only workflows produce identical output (regression)
- [ ] No `tempfile` crate needed at runtime (only in dev-dependencies for tests)
- [ ] Deps: 2 genuinely new + 4 zero-cost promotes from transitive

---

## Key Differences from Old PR1 + PR2

| Aspect | Old PR1 + PR2 | This PR |
|--------|--------------|---------|
| ContentBlock | Kept as struct, added audio/is_audio | **Enum with 5 struct variants** (invalid states unrepresentable) |
| Serde tag | `Text(String)` newtype (broken with internally tagged) | **`Text { text: String }` struct variant** (B1 fix) |
| Media bridge | `__media` JSON key in output (throwaway) | **`media_staging` DashMap side-channel** (B3 fix) |
| ResourceLink | Flattened to text string | **Own enum variant** (preserves structure) |
| Integration | PR1 extracted, PR2 processed | **Single PR** does both |
| Serde format | Stringly typed `content_type` field | **Internally tagged** (`#[serde(tag = "type")]`) with `mimeType` rename |
| Events | 1 new (MediaExtracted) in PR1, 3 more in PR2 | **4 new events** in single PR (with `pipeline_ms` timing) |
| Error codes | Spread across 2 PRs | **All 6 codes** in single PR |
| Dependencies | Added in PR2 only | **2 new + 4 zero-cost promotes** in single PR |
| CAS I/O | Called sync in async context, then `spawn_blocking` | **Fully async** via `io::atomic::write_fail()` (D1 fix) |
| CAS writes | `tempfile::persist_noclobber()` | **`io::atomic::write_fail()`** -- reuses existing crate infra |
| MIME fallback | Silent `application/octet-stream` | **`Err(MimeDetectionFailed)`** (H3 fix) |
| `datastore` access | `self.datastore` (wrong) | **`datastore` parameter** (B4 fix) |
| `workspace_root` | `self.workspace_root` (wrong) | **`current_dir()`** at CAS construction (B5 fix) |
| Error code type | `u16` | **`&'static str`** matching `NikaError::code()` (B6 fix) |
| `MediaError::MediaStoreWrite` | `path: String, reason: String` | **`path: PathBuf, source: std::io::Error`** (proper error chain) |

---

## rmcp 0.16.0 API Reference (for implementer)

```rust
// Import paths
use rmcp::model::RawContent;           // 5-variant enum
use rmcp::model::Content;              // = Annotated<RawContent>, Deref -> RawContent
use rmcp::model::RawTextContent;       // .text: String
use rmcp::model::RawImageContent;      // .data: String, .mime_type: String
use rmcp::model::RawAudioContent;      // .data: String, .mime_type: String (NO as_audio()!)
use rmcp::model::RawEmbeddedResource;  // .resource: ResourceContents
use rmcp::model::resource::ResourceContents;  // UNTAGGED ENUM: Text | Blob
use rmcp::model::RawResource;          // ResourceLink: .uri, .name (String!), .title, .description, .mime_type

// Deref pattern: &*c or &**c to get &RawContent from &Content for matching
// Audio has NO as_audio() accessor -- must pattern match directly
// ResourceContents is UNTAGGED: #[serde(untagged)] enum with Text and Blob variants
// [H2] RawResource.name is String, NOT Option<String> -- wrap in Some() when mapping
```

---

## Future Optimizations (not in PR1 scope)

1. **TeeWriter streaming** -- Single-pass decode+hash+write via `base64::DecoderReader` +
   `blake3::Hasher` (`io::Write`) + `std::fs::File` in `spawn_blocking`. Reduces peak memory
   from ~1.75x to ~8KB for large media. Worth implementing when media > 50MB becomes common.

2. **Bytes type** -- Replace `Vec<u8>` with `bytes::Bytes` for zero-copy sharing across
   concurrent `for_each` tasks. Already a transitive dep.

3. **MIME security** -- Declare-then-verify pattern: compare MCP server-declared MIME vs
   magic-byte-detected MIME. Category allowlisting (image/audio/video/pdf only).
   Reject executable/script content types.
