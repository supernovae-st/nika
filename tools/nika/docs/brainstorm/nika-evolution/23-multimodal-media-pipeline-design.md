# 23 -- Multimodal Media Pipeline Design (B+)

> Complete design for handling binary media (images, PDF, audio, video, documents)
> in Nika workflows. Approach B+: MediaRef first-class + CAS-ready blake3 layout.

**Date**: 2026-03-17 | **Nika version**: v0.27.0 | **rmcp**: 0.16.0 (latest: 1.2.0)

---

## Table of Contents

1. [Problem Statement](#1-problem-statement)
2. [Root Cause Analysis](#2-root-cause-analysis)
3. [Approach Selection](#3-approach-selection)
4. [B+ Design: MediaRef + CAS-Ready](#4-b-design-mediaref--cas-ready)
5. [Implementation: Layer by Layer](#5-implementation-layer-by-layer)
6. [YAML Workflow Examples](#6-yaml-workflow-examples)
7. [MCP Server Ecosystem](#7-mcp-server-ecosystem)
8. [Edge Cases & Mitigations](#8-edge-cases--mitigations)
9. [rmcp Upgrade Opportunity](#9-rmcp-upgrade-opportunity)
10. [Industry Validation](#10-industry-validation)
11. [Implementation Plan](#11-implementation-plan)
12. [Sources](#12-sources)

---

## 1. Problem Statement

Nika workflows cannot produce or consume binary content. When an MCP server returns an
image (base64 `ImageContent`), audio (`AudioContent`), or binary resource
(`BlobResourceContents`), the data is **silently discarded**. This blocks:

- Image generation workflows (QR Code AI hero images, charts, logos)
- PDF report generation (SEO audits, business reports)
- Audio generation (TTS narration, podcast segments)
- Chart/diagram generation (traffic graphs, architecture diagrams)
- Document generation (DOCX, XLSX, PPTX)
- Screenshot capture (site auditing)

40+ MCP servers exist across 8 media categories. The ecosystem is ready.
Nika is the bottleneck.

---

## 2. Root Cause Analysis

### 2.1 The Exact Lines Where Data Dies

**Bottleneck 1** -- `src/mcp/rmcp_adapter.rs` line 352:

```rust
// CURRENT: Only text is extracted. Image/Audio/Resource/ResourceLink are DROPPED.
let content: Vec<ContentBlock> = result
    .content
    .iter()
    .filter_map(|c| {
        c.as_text().map(|t| ContentBlock::text(t.text.clone()))
    })
    .collect();
```

rmcp 0.16 provides `as_image()`, `as_resource()`, `as_resource_link()` methods
on `RawContent`. They exist. They work. They are never called.

**Bottleneck 2** -- `src/runtime/executor/verbs.rs` line 918:

```rust
// CURRENT: All content blocks joined as text string. Non-text blocks produce "".
let text = tool_result.text();
serde_json::from_str(&text).unwrap_or_else(|_| Value::String(text))
```

Even if Bottleneck 1 is fixed, this line would still discard non-text content
by joining only text blocks.

### 2.2 Data Flow Diagram (Current -- Broken)

```
MCP Server returns:
  CallToolResult {
    content: [
      Text("Image generated. Seed: 12345"),    ← KEPT
      Image(data: "iVBOR...", mime: "image/png"), ← DROPPED
      Audio(data: "AAAA...", mime: "audio/mp3"),  ← DROPPED
    ]
  }
        │
        ▼
  rmcp_adapter.rs:352
  filter_map(|c| c.as_text()...)     ← Only Text variant extracted
        │
        ▼
  Nika ToolCallResult { content: [TextBlock("Image generated...")] }
        │
        ▼
  verbs.rs:918
  tool_result.text()                 ← Joins text blocks with \n
        │
        ▼
  TaskResult { output: Arc<Value::String("Image generated. Seed: 12345")> }
        │
        ▼
  Image data is GONE. Irrecoverable.
```

### 2.3 What's Available but Unused

| Layer | Type | Status |
|-------|------|--------|
| rmcp 0.16 `RawContent::Image` | `data: String` (base64), `mime_type: String` | Available, never called |
| rmcp 0.16 `RawContent::Audio` | `data: String` (base64), `mime_type: String` | Available, never called |
| rmcp 0.16 `RawContent::Resource` | `ResourceContents::BlobResourceContents` | Available, never called |
| rmcp 0.16 `RawContent::ResourceLink` | `uri: String`, `mime_type: Option<String>` | Available, never called |
| Nika `ContentBlock::data` | `Option<String>` (base64) | Defined, never populated |
| Nika `ContentBlock::mime_type` | `Option<String>` | Defined, never populated |
| Nika `ResourceContent::blob` | `Option<String>` (base64) | Defined, never populated |
| `base64` crate | In dependency tree via rmcp | Available, never used by Nika |

### 2.4 rmcp Types (Exact Definitions from v0.16.0 source)

```rust
// rmcp 0.16.0 -- crates/rmcp/src/model/content.rs

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RawContent {
    Text(RawTextContent),
    Image(RawImageContent),
    Resource(RawEmbeddedResource),
    Audio(RawAudioContent),
    ResourceLink(super::resource::RawResource),
}

pub type Content = Annotated<RawContent>;

// Image: base64 data + MIME type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawImageContent {
    pub data: String,           // base64-encoded image bytes
    pub mime_type: String,      // "image/png", "image/jpeg", etc.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<super::Meta>,
}

// Audio: base64 data + MIME type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawAudioContent {
    pub data: String,           // base64-encoded audio bytes
    pub mime_type: String,      // "audio/mp3", "audio/wav", etc.
}

// Embedded resource: text or blob
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawEmbeddedResource {
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<super::Meta>,
    pub resource: ResourceContents,
}

// Resource contents: text or binary
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(untagged)]
pub enum ResourceContents {
    TextResourceContents {
        uri: String,
        mime_type: Option<String>,
        text: String,
    },
    BlobResourceContents {
        uri: String,
        mime_type: Option<String>,
        blob: String,           // base64-encoded binary
    },
}

// Resource link: URI-only reference (no inline data)
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RawResource {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
    pub size: Option<u32>,
}

// Tool call result
pub struct CallToolResult {
    pub content: Vec<Content>,
    pub structured_content: Option<Value>,
    pub is_error: Option<bool>,
    pub meta: Option<Meta>,
}

// RawContent methods available:
impl RawContent {
    pub fn as_text(&self) -> Option<&RawTextContent>     // ← THE ONLY ONE CALLED
    pub fn as_image(&self) -> Option<&RawImageContent>   // ← NEVER CALLED
    pub fn as_resource(&self) -> Option<&RawEmbeddedResource> // ← NEVER CALLED
    pub fn as_resource_link(&self) -> Option<&RawResource>    // ← NEVER CALLED
}
```

---

## 3. Approach Selection

### 3.1 Three Approaches Evaluated

| Criterion | A (Patch & Pass) | **B+ (MediaRef + CAS)** | C (MediaStore Engine) |
|-----------|-------------------|--------------------------|------------------------|
| **Diff size** | ~200 lines | **~1200 lines** | ~2000 lines |
| **Time** | 2-3 days | **1-2 weeks** | 3-4 weeks |
| Backward compat | ✅ | **✅** | ✅ |
| MediaRef metadata | ❌ | **✅** | ✅ |
| nika check validation | ❌ | **✅** | ✅ |
| TUI preview | ❌ | **✅** | ✅ |
| CAS dedup | ❌ | **✅ (ready, not active)** | ✅ (active) |
| Content-addressed layout | ❌ | **✅ (blake3 layout)** | ✅ |
| Cloud storage (S3/GCS) | ❌ | **❌ (designed for)** | ✅ |
| Large file streaming | ❌ | **❌** | ✅ |
| YAGNI risk | None | **None** | High (S3/GCS) |
| Upgrade path | Rewrite → B | **Extend → C** | N/A |

### 3.2 Decision: B+ (MediaRef + CAS-Ready Layout)

**B+ = Approach B + CAS-ready blake3 directory layout + content-addressed naming.**

The CAS layout stores files as `{2hex}/{62hex}.{ext}`, which:
- Enables automatic dedup (same content = same hash = same file)
- Prevents directory bloat (fan-out avoids 10K+ files in one dir)
- Is forward-compatible with verified streaming (bao crate)
- Adds only ~50 lines over basic B (blake3 hash + path computation)

B+ gives 95% of C's value for 35% of the effort. The remaining 5% (S3, streaming)
can be added later by implementing the `MediaStore` trait over the same layout.

---

## 4. B+ Design: MediaRef + CAS-Ready

### 4.1 Core Struct: MediaRef

```rust
// NEW FILE: src/ast/media.rs

use serde::{Deserialize, Serialize};

/// A reference to binary media stored on disk.
/// Flows through bindings as a JSON object.
/// Access via: {{with.task.media[0].path}}, {{with.task.media[0].mime_type}}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MediaRef {
    /// Absolute path to the stored file
    pub path: String,

    /// MIME type (e.g., "image/png", "audio/mp3", "application/pdf")
    pub mime_type: String,

    /// File size in bytes
    pub size_bytes: u64,

    /// blake3 hash of file content (hex-encoded, 64 chars)
    pub checksum: String,

    /// Where this media came from
    pub source: MediaSource,
}

/// Origin of the media content
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MediaSource {
    /// From an MCP tool call (image gen, chart, etc.)
    McpTool {
        server: String,
        tool: String,
    },
    /// From an MCP resource read
    McpResource {
        server: String,
        uri: String,
    },
    /// Downloaded from a URL
    Download {
        url: String,
    },
    /// Generated by a task (future: nika builtins)
    Generated {
        task_id: String,
    },
}
```

### 4.2 Composite Task Output Format

When MCP returns non-text content, the task output becomes a composite JSON object:

```json
{
  "text": "Image generated successfully. Seed: 12345",
  "media": [
    {
      "path": "/abs/path/.nika/media/ab/cdef1234...5678.png",
      "mime_type": "image/png",
      "size_bytes": 245760,
      "checksum": "blake3:abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
      "source": {
        "type": "mcp_tool",
        "server": "image-gen",
        "tool": "generate_image"
      }
    }
  ]
}
```

**Backward compatibility rule:**

| MCP Response | Task Output | Breaking Change? |
|--------------|-------------|------------------|
| Text only | String or parsed JSON (unchanged) | **No** |
| Text + Image | `{ "text": "...", "media": [...] }` | New format |
| Image only | `{ "text": "", "media": [...] }` | New format |
| Resource (text) | String (unchanged) | **No** |
| Resource (blob) | `{ "text": "", "media": [...] }` | New format |
| ResourceLink | `{ "text": "", "media": [...] }` | New format |

Text-only responses are 100% backward compatible. The composite format only
appears when MCP returns non-text content, which never happened before (it was
silently dropped).

### 4.3 CAS-Ready Storage Layout

```
.nika/
└── media/
    ├── tmp/                           # Temp files for atomic writes
    │   └── write-{uuid}.tmp
    └── store/                         # Content-addressed store
        ├── ab/
        │   └── cdef1234...5678.png    # {blake3_hash}.{ext}
        ├── ff/
        │   └── 0012abcd...ef90.mp3
        └── 3a/
            └── 9876fedc...ba01.pdf
```

**Naming scheme**: `{blake3_hex[0..2]}/{blake3_hex[2..64]}.{extension}`

- First 2 hex chars = directory (256 possible dirs, fan-out)
- Remaining 62 hex chars = filename
- Extension from MIME type (`image/png` -> `.png`)
- **Automatic dedup**: Same content → same hash → same file → no duplicate write

**Why blake3**:
- 14x faster than SHA-256 (SIMD-accelerated)
- `to_hex()` returns stack-allocated `ArrayString<64>` (zero heap allocation)
- `update_mmap()` for efficient large file hashing
- Cryptographically secure (unlike xxHash3 used in current artifacts)
- CAS-ready: hash IS the filename
- Rust-native, no C dependencies

### 4.4 MediaProcessor (New Module)

```rust
// NEW FILE: src/media/mod.rs

use base64::Engine;
use blake3;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Processes binary media from MCP responses.
/// Decodes base64, detects MIME, stores in CAS layout, returns MediaRef.
pub struct MediaProcessor {
    /// Root directory for media storage (.nika/media/)
    store_dir: PathBuf,
    /// Maximum file size (default: 100MB)
    max_size: u64,
}

impl MediaProcessor {
    pub fn new(store_dir: PathBuf) -> Self {
        Self {
            store_dir,
            max_size: 100 * 1024 * 1024, // 100MB
        }
    }

    /// Save base64-encoded binary content to CAS store.
    /// Returns MediaRef with path, mime, size, checksum.
    pub async fn save_base64(
        &self,
        base64_data: &str,
        declared_mime: &str,
        source: MediaSource,
    ) -> Result<MediaRef, NikaError> {
        // 1. Decode base64
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(base64_data)
            .map_err(|e| NikaError::MediaDecodeError {
                reason: format!("Invalid base64: {}", e),
            })?;

        // 2. Check size limit
        if bytes.len() as u64 > self.max_size {
            return Err(NikaError::MediaSizeExceeded {
                size: bytes.len() as u64,
                max: self.max_size,
            });
        }

        // 3. Detect MIME type (magic bytes take priority over declared)
        let detected_mime = infer::get(&bytes)
            .map(|t| t.mime_type().to_string())
            .unwrap_or_else(|| declared_mime.to_string());

        // 4. Compute blake3 hash
        let hash = blake3::hash(&bytes);
        let hex = hash.to_hex();
        let checksum = format!("blake3:{}", hex);

        // 5. Determine extension from MIME
        let ext = mime_to_ext(&detected_mime);

        // 6. Compute CAS path: store/{2hex}/{62hex}.{ext}
        let dir_prefix = &hex[..2];
        let file_name = format!("{}.{}", &hex[2..], ext);
        let cas_dir = self.store_dir.join("store").join(dir_prefix);
        let cas_path = cas_dir.join(&file_name);

        // 7. Check if content already exists (dedup)
        if cas_path.exists() {
            // Content-addressed dedup: same hash = same file
            return Ok(MediaRef {
                path: cas_path.to_string_lossy().to_string(),
                mime_type: detected_mime,
                size_bytes: bytes.len() as u64,
                checksum,
                source,
            });
        }

        // 8. Atomic write: tmp file → rename
        fs::create_dir_all(&cas_dir).await?;
        let tmp_dir = self.store_dir.join("tmp");
        fs::create_dir_all(&tmp_dir).await?;
        let tmp_path = tmp_dir.join(format!("write-{}.tmp", uuid::Uuid::new_v4()));
        fs::write(&tmp_path, &bytes).await?;
        fs::rename(&tmp_path, &cas_path).await?;

        // 9. Return MediaRef
        Ok(MediaRef {
            path: cas_path.to_string_lossy().to_string(),
            mime_type: detected_mime,
            size_bytes: bytes.len() as u64,
            checksum,
            source,
        })
    }

    /// Register an existing file in the CAS store.
    /// Used for MCP servers that save to disk (ComfyUI, VOICEVOX).
    pub async fn register_file(
        &self,
        file_path: &Path,
        source: MediaSource,
    ) -> Result<MediaRef, NikaError> {
        let bytes = fs::read(file_path).await?;
        let hash = blake3::hash(&bytes);
        let hex = hash.to_hex();
        let checksum = format!("blake3:{}", hex);

        let mime = infer::get(&bytes)
            .map(|t| t.mime_type().to_string())
            .or_else(|| {
                mime_guess::from_path(file_path)
                    .first()
                    .map(|m| m.to_string())
            })
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let ext = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin");

        // CAS path
        let dir_prefix = &hex[..2];
        let file_name = format!("{}.{}", &hex[2..], ext);
        let cas_dir = self.store_dir.join("store").join(dir_prefix);
        let cas_path = cas_dir.join(&file_name);

        if !cas_path.exists() {
            fs::create_dir_all(&cas_dir).await?;
            fs::copy(file_path, &cas_path).await?;
        }

        Ok(MediaRef {
            path: cas_path.to_string_lossy().to_string(),
            mime_type: mime,
            size_bytes: bytes.len() as u64,
            checksum,
            source,
        })
    }
}

/// Map MIME type to file extension
fn mime_to_ext(mime: &str) -> &'static str {
    match mime {
        // Images
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "image/bmp" => "bmp",
        "image/tiff" => "tiff",
        "image/avif" => "avif",
        // Audio
        "audio/mpeg" => "mp3",
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/ogg" => "ogg",
        "audio/flac" => "flac",
        "audio/aac" => "aac",
        "audio/webm" => "weba",
        "audio/mp4" => "m4a",
        // Video
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "video/ogg" => "ogv",
        "video/avi" | "video/x-msvideo" => "avi",
        "video/quicktime" => "mov",
        // Documents
        "application/pdf" => "pdf",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => "docx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => "xlsx",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => "pptx",
        // Data
        "application/json" => "json",
        "application/xml" | "text/xml" => "xml",
        "text/csv" => "csv",
        "text/html" => "html",
        "text/markdown" => "md",
        // 3D
        "model/stl" => "stl",
        "model/gltf+json" => "gltf",
        "model/gltf-binary" => "glb",
        // Fallback
        _ => "bin",
    }
}
```

### 4.5 Data Flow Diagram (Fixed -- B+)

```
MCP Server returns:
  CallToolResult {
    content: [
      Text("Image generated. Seed: 12345"),
      Image(data: "iVBOR...", mime: "image/png"),
    ]
  }
        │
        ▼
┌─────────────────────────────────────────────────┐
│ LAYER 1: rmcp_adapter.rs (FIXED)                │
│                                                  │
│ for content_block in result.content:             │
│   match block.raw:                               │
│     RawContent::Text(t)  → texts.push(t.text)   │
│     RawContent::Image(i) → raw_media.push(...)   │
│     RawContent::Audio(a) → raw_media.push(...)   │
│     RawContent::Resource(r) → extract blob/text  │
│     RawContent::ResourceLink(r) → link_refs.push │
│                                                  │
│ Return: EnrichedToolResult { texts, raw_media }  │
└─────────────────────────────────────────────────┘
        │
        ▼
┌─────────────────────────────────────────────────┐
│ LAYER 2: verbs.rs invoke handler (FIXED)        │
│                                                  │
│ if raw_media.is_empty():                         │
│   → text only → same as today (backward compat)  │
│                                                  │
│ else:                                            │
│   → for each raw_media item:                     │
│     1. media_processor.save_base64(data, mime)   │
│     2. → blake3 hash → CAS path                 │
│     3. → decode base64 → write to CAS path      │
│     4. → create MediaRef                         │
│   → emit MediaSaved events                      │
│   → return { "text": "...", "media": [...] }     │
└─────────────────────────────────────────────────┘
        │
        ▼
┌─────────────────────────────────────────────────┐
│ LAYER 3: RunContext (UNCHANGED)                  │
│                                                  │
│ TaskResult { output: Arc<Value> }                │
│ Value is JSON — MediaRef serializes as JSON obj  │
│                                                  │
│ Binding access (existing JSONPath, zero changes):│
│   {{with.task.text}}           → "Image gen..."  │
│   {{with.task.media[0].path}}  → "/path/to/file" │
│   {{with.task.media[0].mime_type}} → "image/png" │
│   {{with.task.media[0].size_bytes}} → 245760     │
│   {{with.task.media[0].checksum}}  → "blake3:..." │
└─────────────────────────────────────────────────┘
        │
        ▼
┌─────────────────────────────────────────────────┐
│ LAYER 4: artifact_processor.rs (EXTENDED)        │
│                                                  │
│ output.artifact.path set?                        │
│   → Is source a MediaRef?                        │
│     → YES: Copy from CAS store to user path     │
│     → NO: Write text content (existing behavior) │
│                                                  │
│ ArtifactFormat::Auto (new default):              │
│   → Detect from source: MediaRef → binary copy   │
│   → Else: text write (current behavior)          │
└─────────────────────────────────────────────────┘
        │
        ▼
┌─────────────────────────────────────────────────┐
│ LAYER 5: writer.rs (EXTENDED)                    │
│                                                  │
│ write_binary(src: &Path, dst: &Path):            │
│   → atomic copy (tmp + rename)                   │
│   → returns WriteResult { path, size, format }   │
│                                                  │
│ write() for text (UNCHANGED)                     │
└─────────────────────────────────────────────────┘
        │
        ▼
  ✅ Binary file at user-specified path
  ✅ MediaRef in bindings for downstream tasks
  ✅ MediaSaved event in NDJSON trace
```

---

## 5. Implementation: Layer by Layer

### 5.1 Files to Create (3 new files)

| File | Purpose | Est. Lines |
|------|---------|------------|
| `src/ast/media.rs` | `MediaRef`, `MediaSource` structs | ~60 |
| `src/media/mod.rs` | `MediaProcessor`: decode, hash, store, detect | ~250 |
| `src/media/mime_map.rs` | `mime_to_ext()` MIME-to-extension mapping | ~80 |

### 5.2 Files to Modify (8 files)

| File | Change | Est. Lines |
|------|--------|------------|
| `src/mcp/rmcp_adapter.rs` | Extract ALL content types (image, audio, resource, resourcelink) | +60 |
| `src/mcp/types.rs` | Add `RawMediaBlock` enum for pre-storage media | +40 |
| `src/runtime/executor/verbs.rs` | Handle media blocks in invoke handler, call `MediaProcessor` | +80 |
| `src/ast/artifact.rs` | Add `Binary`, `Base64`, `Auto` to `ArtifactFormat` + `mime_type` field | +20 |
| `src/runtime/artifact_processor.rs` | Binary write path: detect MediaRef, copy from CAS | +60 |
| `src/io/writer.rs` | `write_binary()` method (atomic copy) | +30 |
| `src/event/log.rs` | `MediaSaved` event + `mime_type` in `ArtifactWritten` | +15 |
| `src/ast/mod.rs` | `pub mod media;` | +1 |

**Also touch:**

| File | Change |
|------|--------|
| `Cargo.toml` | Add `blake3`, `infer`, `mime_guess` dependencies |
| `src/lib.rs` | `pub mod media;` |
| `src/error.rs` | `MediaDecodeError`, `MediaSizeExceeded` NIKA-XXX codes |

### 5.3 New Dependencies

```toml
# Cargo.toml additions
blake3 = "1"            # Fast hashing (14x SHA-256, SIMD, CAS-ready)
infer = "0.16"          # MIME detection from magic bytes (~50 types)
mime_guess = "2"         # Extension ↔ MIME mapping (1200+ types)
# base64 v0.22.1 already in tree via rmcp
# uuid already in tree
```

### 5.4 New Error Codes

| Code | Name | When |
|------|------|------|
| NIKA-250 | `MediaDecodeError` | Invalid base64, corrupted binary data |
| NIKA-251 | `MediaSizeExceeded` | Binary content exceeds max_size (100MB default) |
| NIKA-252 | `MediaWriteError` | Failed to write media to CAS store |
| NIKA-253 | `MediaHashMismatch` | Integrity check failed (file corrupted) |
| NIKA-254 | `MediaMimeUnknown` | Could not determine MIME type |

### 5.5 New Events

```rust
// src/event/log.rs additions

EventKind::MediaSaved {
    task_id: Arc<str>,
    path: String,
    mime_type: String,
    size_bytes: u64,
    checksum: String,
    source: String,       // "mcp:server/tool" or "file:/path"
}

// Extend existing:
EventKind::ArtifactWritten {
    task_id: Arc<str>,
    path: String,
    size: u64,
    format: String,
    mime_type: Option<String>,  // NEW: "image/png" for binary artifacts
}
```

### 5.6 ArtifactFormat Extension

```rust
// src/ast/artifact.rs

pub enum ArtifactFormat {
    Text,       // Plain text (existing)
    Json,       // JSON pretty-printed (existing)
    Yaml,       // YAML formatted (existing)
    Binary,     // Raw binary copy (NEW)
    Base64,     // Decode base64 → binary file (NEW)
    Auto,       // Auto-detect from task output (NEW, becomes default)
}

pub struct ArtifactOutput {
    pub path: String,
    pub source: Option<String>,
    pub template: Option<String>,
    pub format: Option<ArtifactFormat>,
    pub mode: Option<ArtifactMode>,
    pub mime_type: Option<String>,  // NEW: for validation
}
```

### 5.7 rmcp Adapter Fix (The Critical Change)

```rust
// src/mcp/rmcp_adapter.rs -- REPLACEMENT for lines 347-359

/// Intermediate type for pre-storage media from MCP responses
pub enum RawMediaBlock {
    Image {
        data: String,       // base64
        mime_type: String,
    },
    Audio {
        data: String,       // base64
        mime_type: String,
    },
    Blob {
        data: String,       // base64
        mime_type: Option<String>,
        uri: String,
    },
    ResourceLink {
        uri: String,
        name: String,
        mime_type: Option<String>,
        size: Option<u32>,
    },
}

/// Extended tool result that preserves ALL content types
pub struct EnrichedToolResult {
    pub texts: Vec<String>,
    pub media: Vec<RawMediaBlock>,
    pub is_error: bool,
}

// Convert rmcp result to enriched result:
let mut texts = Vec::new();
let mut media = Vec::new();

for content in &result.content {
    match &content.raw {
        RawContent::Text(t) => {
            texts.push(t.text.clone());
        }
        RawContent::Image(i) => {
            media.push(RawMediaBlock::Image {
                data: i.data.clone(),
                mime_type: i.mime_type.clone(),
            });
        }
        RawContent::Audio(a) => {
            media.push(RawMediaBlock::Audio {
                data: a.data.clone(),
                mime_type: a.mime_type.clone(),
            });
        }
        RawContent::Resource(r) => {
            match &r.resource {
                ResourceContents::BlobResourceContents { blob, uri, mime_type, .. } => {
                    media.push(RawMediaBlock::Blob {
                        data: blob.clone(),
                        mime_type: mime_type.clone(),
                        uri: uri.clone(),
                    });
                }
                ResourceContents::TextResourceContents { text, .. } => {
                    texts.push(text.clone());
                }
            }
        }
        RawContent::ResourceLink(r) => {
            media.push(RawMediaBlock::ResourceLink {
                uri: r.uri.clone(),
                name: r.name.clone(),
                mime_type: r.mime_type.clone(),
                size: r.size,
            });
        }
    }
}

Ok(EnrichedToolResult {
    texts,
    media,
    is_error: result.is_error.unwrap_or(false),
})
```

### 5.8 Invoke Verb Handler Fix

```rust
// src/runtime/executor/verbs.rs -- invoke handler modification

let enriched = client.call_tool_enriched(tool, params, task_id, &self.event_log).await?;

let output = if enriched.media.is_empty() {
    // TEXT-ONLY: backward compatible path
    let text = enriched.texts.join("\n");
    serde_json::from_str(&text).unwrap_or_else(|_| Value::String(text))
} else {
    // MEDIA PRESENT: save to CAS store, build composite output
    let mut media_refs = Vec::new();

    for raw in &enriched.media {
        let media_ref = match raw {
            RawMediaBlock::Image { data, mime_type } => {
                media_processor.save_base64(
                    data,
                    mime_type,
                    MediaSource::McpTool {
                        server: server_name.clone(),
                        tool: tool.clone(),
                    },
                ).await?
            }
            RawMediaBlock::Audio { data, mime_type } => {
                media_processor.save_base64(
                    data,
                    mime_type,
                    MediaSource::McpTool {
                        server: server_name.clone(),
                        tool: tool.clone(),
                    },
                ).await?
            }
            RawMediaBlock::Blob { data, mime_type, uri } => {
                let mime = mime_type.as_deref().unwrap_or("application/octet-stream");
                media_processor.save_base64(
                    data,
                    mime,
                    MediaSource::McpResource {
                        server: server_name.clone(),
                        uri: uri.clone(),
                    },
                ).await?
            }
            RawMediaBlock::ResourceLink { uri, mime_type, .. } => {
                // ResourceLink: URI-only, no inline data
                // Store as reference, don't download
                MediaRef {
                    path: uri.clone(),  // Use URI as path
                    mime_type: mime_type.clone()
                        .unwrap_or_else(|| "application/octet-stream".to_string()),
                    size_bytes: 0,  // Unknown until downloaded
                    checksum: String::new(),
                    source: MediaSource::McpResource {
                        server: server_name.clone(),
                        uri: uri.clone(),
                    },
                }
            }
        };

        // Emit MediaSaved event
        event_log.emit(EventKind::MediaSaved {
            task_id: task_id.into(),
            path: media_ref.path.clone(),
            mime_type: media_ref.mime_type.clone(),
            size_bytes: media_ref.size_bytes,
            checksum: media_ref.checksum.clone(),
            source: format!("mcp:{}/{}", server_name, tool),
        });

        media_refs.push(media_ref);
    }

    // Build composite output
    let text = enriched.texts.join("\n");
    serde_json::json!({
        "text": text,
        "media": media_refs,
    })
};
```

---

## 6. YAML Workflow Examples

### 6.1 Image Generation (Stability AI)

```yaml
nika: "@0.12"

mcp:
  image-gen:
    command: npx
    args: ["-y", "mcp-server-stability-ai"]

tasks:
  generate-hero:
    invoke:
      tool: generate_image
      mcp: image-gen
      params:
        prompt: "Professional hero image for {{with.business}}"
        aspect_ratio: "16:9"
        output_format: png
    output:
      artifact:
        path: "./output/hero.png"
        # format: auto (default) — detects MediaRef, copies binary
```

### 6.2 PDF Report Generation

```yaml
nika: "@0.12"

mcp:
  pdf:
    command: npx
    args: ["-y", "@mcp-z/mcp-pdf"]

tasks:
  analyze:
    infer:
      prompt: "Analyze SEO metrics for {{with.url}}"
      model: claude-sonnet-4-6
      structured:
        schema: ./schemas/seo-analysis.json

  generate-report:
    invoke:
      tool: generate_pdf
      mcp: pdf
      params:
        layout:
          - type: heading
            text: "SEO Audit: {{with.analysis.domain}}"
          - type: paragraph
            text: "{{with.analysis.summary}}"
          - type: table
            data: "{{with.analysis.metrics}}"
    with:
      analysis: analyze
    output:
      artifact:
        path: "./output/seo-report.pdf"
```

### 6.3 Audio Narration (ElevenLabs TTS)

```yaml
nika: "@0.12"

mcp:
  tts:
    command: npx
    args: ["-y", "elevenlabs-mcp-enhanced"]

tasks:
  narrate:
    invoke:
      tool: text_to_speech
      mcp: tts
      params:
        text: "{{with.article}}"
        voice: "Rachel"
        model_id: "eleven_turbo_v2.5"
    output:
      artifact:
        path: "./output/narration.mp3"
```

### 6.4 Chart Generation (AntV)

```yaml
nika: "@0.12"

mcp:
  charts:
    command: npx
    args: ["-y", "@antv/mcp-server-chart"]

tasks:
  traffic-chart:
    invoke:
      tool: line_chart
      mcp: charts
      params:
        data: "{{with.traffic_data}}"
        title: "Monthly Traffic Trend"
        x_field: "month"
        y_field: "visits"
    output:
      artifact:
        path: "./output/charts/traffic.png"
```

### 6.5 Multi-Media Pipeline (The Full Vision)

```yaml
nika: "@0.12"

mcp:
  image-gen:
    command: npx
    args: ["-y", "mcp-server-stability-ai"]
  charts:
    command: npx
    args: ["-y", "@antv/mcp-server-chart"]
  pdf:
    command: npx
    args: ["-y", "@mcp-z/mcp-pdf"]
  tts:
    command: npx
    args: ["-y", "elevenlabs-mcp-enhanced"]
  novanet:
    command: cargo
    args: ["run", "--manifest-path", "../novanet/Cargo.toml", "--", "mcp"]

tasks:
  # 1. AI Analysis (text/JSON)
  analyze:
    infer:
      prompt: "Complete SEO audit for {{with.url}}"
      model: claude-sonnet-4-6
      structured:
        schema: ./schemas/seo-analysis.json

  # 2. Traffic Chart (PNG image via MCP)
  traffic-chart:
    invoke:
      tool: line_chart
      mcp: charts
      params:
        data: "{{with.analysis.traffic_data}}"
        title: "Traffic Trend"
    with:
      analysis: analyze
    output:
      artifact:
        path: "./output/charts/traffic.png"

  # 3. Hero Image (PNG via MCP)
  hero-image:
    invoke:
      tool: generate_image
      mcp: image-gen
      params:
        prompt: "Professional banner for {{with.analysis.business_name}}"
        aspect_ratio: "16:9"
    with:
      analysis: analyze
    output:
      artifact:
        path: "./output/images/hero.png"

  # 4. PDF Report (with embedded images)
  report:
    invoke:
      tool: generate_pdf
      mcp: pdf
      params:
        layout:
          - type: heading
            text: "{{with.analysis.business_name}} — SEO Audit"
          - type: image
            path: "{{with.chart.media[0].path}}"
          - type: paragraph
            text: "{{with.analysis.summary}}"
          - type: image
            path: "{{with.hero.media[0].path}}"
          - type: table
            data: "{{with.analysis.metrics}}"
    with:
      analysis: analyze
      chart: traffic-chart
      hero: hero-image
    output:
      artifact:
        path: "./output/report.pdf"

  # 5. Audio Narration (MP3 via MCP)
  narration:
    invoke:
      tool: text_to_speech
      mcp: tts
      params:
        text: "{{with.analysis.executive_summary}}"
        voice: "Rachel"
    with:
      analysis: analyze
    output:
      artifact:
        path: "./output/narration.mp3"

  # 6. Save to NovaNet Knowledge Graph
  save-to-novanet:
    invoke:
      tool: novanet_write
      mcp: novanet
      params:
        entity: "{{with.analysis.entity_slug}}"
        report_path: "{{with.report.media[0].path}}"
        hero_path: "{{with.hero.media[0].path}}"
        audio_path: "{{with.narration.media[0].path}}"
    with:
      analysis: analyze
      report: report
      hero: hero-image
      narration: narration
```

**Result**: One workflow produces 5 media files (chart PNG, hero PNG, PDF report,
MP3 narration, NovaNet entity update) from a single URL input.

### 6.6 Screenshot Pipeline (Firecrawl)

```yaml
nika: "@0.12"

mcp:
  firecrawl:
    command: npx
    args: ["-y", "@anthropic/firecrawl-mcp"]

tasks:
  capture-homepage:
    invoke:
      tool: firecrawl_scrape
      mcp: firecrawl
      params:
        url: "{{with.url}}"
        formats: ["screenshot"]
    output:
      artifact:
        path: "./output/screenshots/homepage.png"

  capture-mobile:
    invoke:
      tool: firecrawl_scrape
      mcp: firecrawl
      params:
        url: "{{with.url}}"
        formats: ["screenshot"]
        mobile: true
    output:
      artifact:
        path: "./output/screenshots/homepage-mobile.png"
```

---

## 7. MCP Server Ecosystem

### 7.1 Image Generation (7 servers)

| Server | Backend | Tools | Return Pattern |
|--------|---------|-------|----------------|
| **mcp-server-stability-ai** | Stability AI | 12 (generate, upscale, remove-bg, outpaint) | base64 ImageContent |
| **comfyui-mcp** | ComfyUI (local) | 31 (full SD ecosystem) | Save to disk + path |
| **mcp-image** | Google Gemini | generate, edit, prompt optimizer | base64 ImageContent |
| **mcp-fal-ai-image** | fal.ai | FLUX, SD, any fal model | URL return |
| **mcp-replicate** | Replicate | Any Replicate model | URL return |
| **replicate-mcp** | Replicate (official) | Search, run, manage models | URL return |
| **@ricardopera/mcp-image-server** | GPT Image 1 | Icon/asset generation | base64 |

### 7.2 PDF (4 servers)

| Server | Backend | Key Feature |
|--------|---------|-------------|
| **@mcp-z/mcp-pdf** | PDFKit (offline) | Layout-based PDF, JSON Resume |
| **pdfcrowd-mcp-pdf-export** | Pdfcrowd (cloud) | HTML-to-PDF, high fidelity |
| **mcp-pdf** | Various | Merge, split, forms, signatures, encryption |
| **doc-ops-mcp** | Various | Cross-format: PDF/DOCX/HTML/Markdown |

### 7.3 Audio/TTS (4 servers)

| Server | Backend | Key Feature |
|--------|---------|-------------|
| **elevenlabs-mcp-enhanced** | ElevenLabs | v3 models, multi-speaker, SFX |
| **@angelogiacco/elevenlabs-mcp-server** | ElevenLabs | Full OpenAPI surface |
| **@kajidog/mcp-tts-voicevox** | VOICEVOX | Embedded UI, WAV export, accents |
| **mcp-tts** | Various | Multi-engine TTS |

### 7.4 Charts & Diagrams (2 servers)

| Server | Charts | Output |
|--------|--------|--------|
| **@antv/mcp-server-chart** | 26+ types (bar, line, pie, flow, mind map, geo, network, pivot) | base64 PNG |
| **mcp-diagram-generator** | Draw.io, Mermaid, Excalidraw | SVG/PNG |

### 7.5 Documents (5 servers)

| Server | Format | Key Feature |
|--------|--------|-------------|
| **@docx-mcp/docx-mcp** | DOCX | Rich content blocks |
| **mcp-powerpoint** | PPTX | Slide creation |
| **@negokaz/excel-mcp-server** | XLSX | Read/write cells |
| **@piotr-agier/google-drive-mcp** | Google Workspace | Drive, Docs, Sheets |
| **doc-ops-mcp** | Multi-format | PDF/DOCX/HTML/MD conversion |

### 7.6 Other (3 servers)

| Server | Category | Key Feature |
|--------|----------|-------------|
| **mcp-video-analyzer** | Video (analysis) | Transcripts, frames, OCR |
| **mcp-3d-printer-server** | 3D | STL, Blender bridge, slicing |
| **figma-mcp** | Design | Figma read/write |

---

## 8. Edge Cases & Mitigations

### 8.1 Mixed Text + Image Response

**Scenario**: MCP returns both text and image content blocks.

**Handling**: Build composite output `{ "text": "...", "media": [...] }`.
Downstream binding: `{{with.task.text}}` for text, `{{with.task.media[0].path}}` for image.

### 8.2 Multiple Images in One Response

**Scenario**: ComfyUI returns 4 image variations.

**Handling**: All stored in CAS, returned as `media[0]`, `media[1]`, `media[2]`, `media[3]`.
Access: `{{with.task.media[2].path}}` for 3rd variation.

### 8.3 Identical Content (CAS Dedup)

**Scenario**: Two tasks generate the same image (e.g., same seed, same params).

**Handling**: blake3 hash is identical → same CAS path → second write is a no-op.
Both tasks reference the same file. Dedup is automatic and free.

### 8.4 Large Base64 Payload (>50MB)

**Scenario**: Video frame or high-res image returned as base64.

**Handling**:
- Check size limit BEFORE decoding (estimate: `base64_length * 3/4`)
- Decode with streaming if needed (base64::read::DecoderReader)
- Default max: 100MB, configurable in `.nika/config.yaml`
- Emit warning event for >10MB media

### 8.5 MCP Server Saves to Disk (ComfyUI, VOICEVOX)

**Scenario**: MCP tool saves file locally, returns path as text.

**Handling**: This comes through as text (existing behavior). The file path is a
normal string in the output. User passes it to downstream tasks as-is.

For explicit CAS registration, user can use `nika:register_media` builtin (future):
```yaml
- id: register
  invoke:
    tool: nika:register_media
    params:
      file_path: "{{with.comfyui_output}}"
```

### 8.6 Agent Loops with Image Gen Tools

**Scenario**: `agent:` verb loop calls image gen MCP tool on each turn.

**Handling**: Each tool call produces MediaRefs. The agent's conversation history
includes the tool results. For multimodal models (Claude, GPT-4o), image content
can be sent back in the conversation. For text-only models, only text is sent.

**Note**: This is a separate feature (agent multimodal support, docs 13/16/17).
The MediaRef system enables it but doesn't implement the agent-side handling.

### 8.7 Text-Only MCP Responses (Backward Compat)

**Scenario**: Existing workflows with text-only MCP tools (NovaNet, data APIs).

**Handling**: Zero change. `enriched.media.is_empty()` → takes the existing code path.
Output is `String` or `Value::Object` exactly as before. No regression.

### 8.8 ArtifactFormat::Auto Detection

**Scenario**: User has `output.artifact` but doesn't specify format.

**Detection logic**:
1. Is the task output a composite object with `media` array? → Copy first media file (binary)
2. Does `output.artifact.source` resolve to a MediaRef? → Copy that media file (binary)
3. Otherwise → Write text content (existing behavior)

### 8.9 Disk Space Management

**Scenario**: Many runs accumulate CAS files.

**Handling**:
- CAS files are shared across runs (dedup saves space)
- `nika trace clean --before 30d` already cleans old traces
- Add `nika media clean --before 30d` to garbage-collect unreferenced CAS files
- GC algorithm: mark-and-sweep (scan all traces, mark referenced media, delete rest)

---

## 9. rmcp Upgrade Opportunity

### 9.1 Current vs Latest

| | Nika (current) | Latest Available |
|--|----------------|------------------|
| **Version** | 0.16.0 | **1.2.0** |
| **Release date** | 2026-02-17 | 2026-03-11 |
| **Versions behind** | 7 | — |

### 9.2 What's New in rmcp 0.17 → 1.2.0

| Version | Key Changes | Relevance to Media |
|---------|-------------|---------------------|
| **0.17.0** | SDK conformance, trait-based tools, empty `CallToolResult` fix | Better tool call ergonomics |
| **1.0.0** | `#[non_exhaustive]` on model types, auth token fields | Forward-compat, safer API |
| **1.1.0** | OAuth 2.0 Client Credentials | Auth for cloud MCP servers |
| **1.2.0** | Missing constructors, ping before init | Stability fixes |

### 9.3 New Fields in rmcp 1.x CallToolResult

```rust
// rmcp 1.x
pub struct CallToolResult {
    pub content: Vec<Content>,
    pub structured_content: Option<Value>,  // NEW: structured JSON output
    pub is_error: Option<bool>,             // Changed: Option<bool> (was bool in 0.16)
    pub meta: Option<Meta>,                 // NEW: metadata
}
```

`structured_content` is interesting: MCP servers can return structured JSON alongside
content blocks. This could be used for metadata about generated media.

### 9.4 Recommended Strategy

1. **Phase 1** (with media pipeline): Stay on rmcp 0.16. The media fix doesn't need new APIs.
2. **Phase 2** (separate PR): Upgrade rmcp 0.16 → 1.2.0. Handle `#[non_exhaustive]`, `Option<bool>`, `structured_content`.
3. **Phase 3**: Leverage `structured_content` for media metadata from MCP servers.

Upgrading rmcp is orthogonal to the media pipeline work.

---

## 10. Industry Validation

### 10.1 Universal Consensus: "Pass Handles, Not Bytes"

| Framework | Pattern | Handle Type |
|-----------|---------|-------------|
| **n8n** | `BinaryData.Manager` with pluggable backends | `IBinaryData { id, mimeType, fileName }` |
| **Dify** | `File` model with `FileTransferMethod` enum | `File { storage_key, mime, size }` |
| **Temporal** | Content-addressed codec (sha256 → S3/GCS) | `Payload { hash, metadata }` |
| **Flyte** | `FlyteFile` type annotation → auto S3 upload | `FlyteFile { path, remote_path }` |
| **LangGraph** | base64 in state dict (acknowledged as not scalable) | — |
| **Nika B+** | `MediaRef` + blake3 CAS layout | `MediaRef { path, mime_type, size_bytes, checksum }` |

### 10.2 CAS Layout Precedent

| System | Hash | Layout | Fan-Out |
|--------|------|--------|---------|
| **Git** | SHA-1 (migrating to SHA-256) | `objects/{2}/{38}` | 256 dirs |
| **Docker/OCI** | SHA-256 | `sha256/{64}` (flat) | None |
| **Nix** | SHA-256 (base32) | `{hash}-{name}/` (flat) | None |
| **iroh-blobs** | blake3 | Flat files + redb metadata DB | None |
| **Nika B+** | blake3 | `store/{2}/{62}.{ext}` | 256 dirs |

Nika's layout matches Git's proven fan-out strategy with blake3's speed advantage.

### 10.3 Research Sources

Research was conducted via 8 parallel agents analyzing:
- rmcp 0.16.0 source code (Cargo cache + crates.io)
- rmcp 1.2.0 documentation (docs.rs, GitHub, CHANGELOG.md)
- MCP protocol schema (2025-03-26 and 2025-06-18 specifications)
- n8n binary handling (TypeScript source: `binary-data.service.ts`, `types.ts`)
- Dify file system (Python source: `models.py`, `enums.py`)
- Temporal payload codec (DataDog codec README, architecture docs)
- Flyte type system (Python FlyteFile documentation)
- blake3 crate API (docs.rs, GitHub, benchmarks)
- iroh-blobs architecture (source code analysis)
- infer crate (MIME magic bytes detection)
- 40+ MCP server packages (npm, GitHub)

---

## 11. Implementation Plan

### 11.1 PR Strategy (2 PRs)

**PR 1: "Fix MCP binary extraction"** (the critical unblock)
- Modify `rmcp_adapter.rs`: extract image/audio/resource/resourcelink blocks
- Modify `types.rs`: add `RawMediaBlock` enum
- Modify `verbs.rs`: handle media blocks, save to temp dir, return paths
- Add `media/mod.rs`: `MediaProcessor` with `save_base64()`
- Add `ast/media.rs`: `MediaRef`, `MediaSource`
- Add error codes: NIKA-250 through NIKA-254
- Add event: `MediaSaved`
- Add deps: `blake3`, `infer`, `mime_guess`
- Tests: unit tests for MediaProcessor, integration test with mock MCP server

**PR 2: "CAS layout + artifact binary support"**
- Implement CAS directory layout (`store/{2hex}/{62hex}.{ext}`)
- Add `ArtifactFormat::Binary`, `ArtifactFormat::Auto`
- Extend `artifact_processor.rs` with binary write path
- Extend `writer.rs` with `write_binary()`
- Extend `ArtifactOutput` with `mime_type` field
- Add `nika media clean` subcommand for GC
- Tests: CAS dedup test, artifact binary write test

### 11.2 Testing Strategy

| Test | Type | What It Validates |
|------|------|-------------------|
| `test_media_processor_save_base64` | Unit | Base64 decode → file write → MediaRef creation |
| `test_media_processor_dedup` | Unit | Same content → same CAS path → no duplicate write |
| `test_media_processor_large_file` | Unit | Reject files > max_size |
| `test_media_processor_mime_detection` | Unit | infer crate detects PNG, JPEG, PDF, MP3 |
| `test_cas_directory_layout` | Unit | Files stored as `store/{2}/{62}.{ext}` |
| `test_invoke_image_content` | Integration | MCP returns ImageContent → MediaRef in output |
| `test_invoke_audio_content` | Integration | MCP returns AudioContent → MediaRef in output |
| `test_invoke_blob_resource` | Integration | MCP returns BlobResourceContents → MediaRef |
| `test_invoke_mixed_content` | Integration | Text + Image → composite output |
| `test_invoke_text_only_unchanged` | Integration | Text-only → same as before (regression) |
| `test_artifact_binary_copy` | Integration | MediaRef → output.artifact → binary file |
| `test_artifact_auto_detect` | Integration | ArtifactFormat::Auto detects MediaRef |
| `test_binding_media_access` | Integration | `{{with.task.media[0].path}}` resolves correctly |
| `test_event_media_saved` | Integration | MediaSaved event emitted with correct metadata |

### 11.3 Estimated Size

| Component | New Lines | Modified Lines | Test Lines |
|-----------|-----------|----------------|------------|
| `src/ast/media.rs` | 60 | — | 30 |
| `src/media/mod.rs` | 250 | — | 150 |
| `src/media/mime_map.rs` | 80 | — | 40 |
| `src/mcp/rmcp_adapter.rs` | — | 60 | 50 |
| `src/mcp/types.rs` | — | 40 | 20 |
| `src/runtime/executor/verbs.rs` | — | 80 | 60 |
| `src/ast/artifact.rs` | — | 20 | 15 |
| `src/runtime/artifact_processor.rs` | — | 60 | 40 |
| `src/io/writer.rs` | — | 30 | 20 |
| `src/event/log.rs` | — | 15 | 10 |
| `src/error.rs` | — | 10 | 5 |
| **Total** | **~390** | **~315** | **~440** |
| **Grand total** | | **~1145 lines** | |

---

## 12. Sources

### 12.1 Nika Codebase (Verified March 2026)

| File | Lines Read | Key Finding |
|------|-----------|-------------|
| `src/mcp/rmcp_adapter.rs:352` | Full file | **THE BOTTLENECK**: only `as_text()` called |
| `src/mcp/types.rs` | Full file | ContentBlock has image/blob fields, never populated |
| `src/runtime/executor/verbs.rs:918` | Full file | `tool_result.text()` discards non-text |
| `src/ast/artifact.rs` | Full file | ArtifactFormat: Text/Json/Yaml only |
| `src/runtime/artifact_processor.rs` | Full file | String-only write pipeline |
| `src/io/writer.rs` | Full file | `write_atomic()` writes UTF-8 string bytes |
| `src/event/log.rs` | Full file | 34 events, no media-specific events |
| `src/store/run_context.rs` | Full file | TaskResult uses `Arc<Value>` (JSON-compatible) |
| `Cargo.toml:139` | Deps | `rmcp = { version = "0.16", features = ["client", "transport-child-process"] }` |

### 12.2 rmcp SDK (Verified from Source)

| Source | Version | Key Finding |
|--------|---------|-------------|
| `~/.cargo/registry/src/.../rmcp-0.16.0/` | 0.16.0 | Full Content enum with 5 variants |
| `docs.rs/rmcp/0.16.0` | 0.16.0 | `as_image()`, `as_resource()` available but unused by Nika |
| `crates.io/crates/rmcp` | 1.2.0 | 7 versions behind, `structured_content` added in 1.x |
| `github.com/modelcontextprotocol/rust-sdk` | main | Full CHANGELOG, content type definitions |

### 12.3 MCP Protocol Schema

| Schema | Version | Key Finding |
|--------|---------|-------------|
| `schema/2025-03-26/schema.json` | 2025-03-26 | ImageContent, AudioContent, BlobResourceContents |
| `schema/2025-06-18/schema.json` | 2025-06-18 | Added ResourceLink (URI-only), `_meta`, `structuredContent` |

### 12.4 Industry Research

| Source | Tool | Key Finding |
|--------|------|-------------|
| n8n source (`binary-data.service.ts`) | Firecrawl scrape | Pluggable BinaryData.Manager, filesystem/S3 backends |
| Dify source (`models.py`, `enums.py`) | Firecrawl scrape | File model, never base64 between nodes |
| Temporal DataDog codec | Firecrawl scrape | CAS with sha256, transparent codec pattern |
| Flyte docs | Perplexity | FlyteFile type annotation → auto S3 upload |
| blake3 docs.rs | Perplexity | `to_hex()` stack-allocated, `update_mmap()` for files |
| iroh-blobs source | Firecrawl scrape | Actor-based CAS, redb metadata, tag-based GC |
| 40+ MCP servers | npm/GitHub | Catalog by media type (see section 7) |

### 12.5 Related Brainstorm Documents

| Doc | Relevance |
|-----|-----------|
| [13 Multimodal Worker Architectures](./13-multimodal-worker-architectures.md) | Worker-level multimodal approach |
| [16 Multimodal Builtin Research](./16-multimodal-builtin-research.md) | mistral.rs capabilities |
| [17 Smart Router & Multimodal Deep](./17-smart-router-multimodal-deep.md) | Complete tool inventory |
| [18 MCP Multimodal Ecosystem](./18-mcp-multimodal-ecosystem-march2026.md) | MCP server catalog |
| [22 New Features Proposals](./22-new-features-proposals-march2026.md) | B2 (Binary Artifacts) proposal |

---

<div align="center">

[← 22 New Features Proposals](./22-new-features-proposals-march2026.md) · [00 Index →](./00-README.md)

</div>
