# Media Pipeline Master Plan v4.3

**Version:** v4.3 (2026-03-18, final review pass)
**Supersedes:** v4.2 (2026-03-18), v4.1 (2026-03-18), v4.0 (2026-03-18), v3.1 (2026-03-17)
**Baseline:** Nika v0.30.5, 5,221 tests, 32 EventKind variants, 68 error codes
**Schema:** @0.12 (additive, no bump for PR1-PR2), @0.13 for PR3 (new YAML syntax)
**rmcp:** 0.16.0 (pinned by rig-core 0.32)

---

## The Problem

```rust
// rmcp_adapter.rs:372-380 -- filter_map only extracts text via as_text(), drops Image/Audio/Resource/ResourceLink
// verbs.rs:928-933 -- tool_result.text() joins only text blocks, ignores binary content
```

**Impact:** 40+ MCP servers return binary data. Nika silently drops ALL of it.

---

## Bugs Fixed in v4.2

56+ issues found across 3 PRs by 44 agents deployed across 5 rounds.

### BLOCKERS (6 -- 3 original + 3 new in v4.2)

| # | PR | Issue | Fix |
|---|-----|-------|-----|
| B1 | PR1 | `ContentBlock::Text(String)` fails serde with `tag = "type"` -- internally tagged enums require struct variants | Changed to `Text { text: String }` with `#[serde(rename = "text")]` |
| B2 | PR1 | `DashMap` API: `entry()` returns `RefMut`, not the value -- `media_staging.entry(key).or_default().push(ref)` does not compile | Use `media_staging.entry(key).or_default().push(media_ref)` with correct DashMap `Entry` API |
| B3 | PR1 | Media refs passed via `TaskResult.media` requires changing `execute()` return type, rippling through all verbs | Switched to side-channel: `RunContext.media_staging` DashMap avoids return type change |
| B4 | PR1 | `self.datastore` in `run_invoke()` -- `datastore` is NOT accessed via `self` in executor, it is passed as a parameter or via `RunContext` | Use `datastore` (local/param) or `ctx.datastore`, not `self.datastore` |
| B5 | PR1 | `workspace_root` not available where `CasStore::workspace_default()` is called -- must be threaded from `Runner` through `RunContext` to executor | Add `workspace_root: PathBuf` to `RunContext`, populated from `Runner::run()` which already knows the workflow path |
| B6 | PR1 | `MediaError::code()` must return `&'static str` to match `NikaError::code()` signature, but plan had `String` | Use `&'static str` return type: `pub fn code(&self) -> &'static str` with static string literals per variant |

### HIGH (7)

| # | PR | Issue | Fix |
|---|-----|-------|-----|
| H1 | PR1 | ~~`CasStore::store()` uses synchronous `std::fs` I/O on async runtime~~ **RESOLVED: io::atomic is fully async** | Use `io::atomic::write_fail()` which uses `tokio::fs` internally -- no `spawn_blocking` needed |
| H2 | PR1 | `RawResource` has no `.name` field in rmcp 0.16 -- `l.name.clone()` does not compile | Read `name` from `annotations` if available, else `None` |
| H3 | PR1 | `MediaError::MimeDetectionFailed` with code 251 is unreachable -- `detect_mime()` never returns `Err`, it falls back to `application/octet-stream` | Remove unreachable error variant, or make `detect_mime()` return `Err` when fallback is unacceptable |
| H4 | PR2 | `parse_duration()` uses `SystemTime::now() - duration` which can panic on underflow | Use `SystemTime::now().checked_sub(duration)` or `duration_since` with error handling |
| H5 | PR1 | `CasStore::workspace_default()` uses relative path `.nika/media/store/` -- breaks if CWD differs from workspace root | Accept `workspace_root: &Path` parameter, or read `NIKA_MEDIA_STORE` env override |
| H6 | PR2 | `TemplateResolver::new(&request.vars)` -- `TemplateResolver` may not accept `&HashMap<String, String>` directly | Verify `TemplateResolver` constructor signature and adapt |
| H7 | PR2 | `validate_artifact_path()` takes `(&artifact_path, &self.base_dir)` but actual signature may differ | Verify `validate_artifact_path` signature in existing codebase before use |

### CRITICAL in PR3 (5)

| # | Issue | Fix |
|---|-------|-----|
| C1 | `ImageMediaType::from_mime(mime)` does not exist in rig-core 0.32 | Use `ImageMediaType::from_str(mime)` or manual match on MIME string |
| C2 | `Message::user(parts)` -- rig-core `Message::user()` may not accept `Vec<UserContent>` directly | Check rig-core API: may need `Message::User { content: OneOrMany::many(parts) }` |
| C3 | Streaming not addressed -- `infer_with_media()` uses non-streaming `.prompt()` | Document as known limitation; streaming media input is future work |
| C4 | Mistral native vision assumes `mistral.rs` supports vision input -- unverified | Gate behind `#[cfg(feature = "native-inference")]` + runtime capability check |
| C5 | URL media input has no size limit -- downloading arbitrary URLs is a DoS vector | Add `MEDIA_URL_MAX_SIZE` (50 MB default) + `Content-Length` pre-check |

---

## Major Discoveries (v4.2)

### 1. CAS Redesign: `io::atomic::write_fail()` replaces tempfile

The existing `io::atomic` module (`src/io/atomic.rs`) already provides fully async file operations
using `tokio::fs` internally. The CAS store does NOT need `spawn_blocking` or the `tempfile` crate
at runtime.

**CAS write = hash + `write_fail()`:**

```rust
pub async fn store(&self, data: &[u8], extension: &str) -> Result<StoreResult, MediaError> {
    let hash = blake3::hash(data).to_hex().to_string();
    let filename = format!("{}.{}", hash, extension);
    let path = self.root.join(&filename);

    // O_EXCL dedup: if file exists, it's already correct (content-addressed)
    match io::atomic::write_fail(&path, data).await {
        Ok(()) => { /* new file stored */ }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => { /* dedup hit */ }
        Err(e) => return Err(MediaError::MediaStoreWrite { ... }),
    }

    // Read-back verification
    let stored = tokio::fs::read(&path).await?;
    let verify_hash = blake3::hash(&stored).to_hex().to_string();
    if verify_hash != hash {
        return Err(MediaError::HashMismatch { ... });
    }

    Ok(StoreResult { hash, path, size: data.len() as u64, extension: extension.to_string() })
}
```

**Why this is better than tempfile:**
- `write_fail()` uses `create_new(true)` (O_EXCL) for atomic check+create -- same TOCTOU safety
- Already tested for binary content (1MB test in `io::atomic::tests`)
- Fully async (tokio::fs), no `spawn_blocking` needed
- Zero new runtime dependency (tempfile only used in `#[cfg(test)]` via dev-dependencies)

### 2. `DocumentSourceKind::Raw` Not Supported by Providers

Deep audit of rig-core 0.32 reveals that `DocumentSourceKind::Raw(Vec<u8>)` is NOT supported
by any provider adapter. All providers require either `Base64` or `Url`:

```rust
// rig-core DocumentSourceKind enum
pub enum DocumentSourceKind {
    Base64(String),    // Supported by all providers
    Url(String),       // Supported by Claude, OpenAI, Gemini
    Raw(Vec<u8>),      // NOT serialized by any provider -- silently dropped!
}
```

**Implication for PR3:** Nika must always re-encode CAS bytes to Base64 before passing to rig-core.
Never use `DocumentSourceKind::Raw`.

### 3. Provider Vision Matrix (rig-core 0.32 audit)

| Provider | Base64 Images | URL Images | Audio | Vision Models |
|----------|:------------:|:----------:|:-----:|---------------|
| **Claude** (Anthropic) | Yes | Yes | No | claude-3.5-sonnet, claude-3-opus, claude-4-* |
| **OpenAI** | Yes | Yes | Yes (whisper) | gpt-4o, gpt-4-turbo, gpt-4.1, o4-mini |
| **Gemini** (Google) | Yes | Yes | Yes | gemini-2.0-flash, gemini-2.5-pro |
| **Mistral** | **BUG** | **BUG** | No | pixtral-large, pixtral-12b |
| **xAI** (Grok) | Yes | Yes | No | grok-2-vision |

### 4. Mistral Vision Bug in rig-core 0.32

rig-core 0.32 **silently drops images** for Mistral. The Mistral adapter does not serialize
`UserContent::Image` variants -- they are filtered out during message construction.

**Impact:** A user sending `infer: { media: [...] }` to a Mistral model would get a response
as if no image was provided, with zero error.

**Mitigation (PR3):** Error at workflow validation time. The analyzer must check the provider
and reject `media:` blocks targeting Mistral cloud models:

```
NIKA-XXX: media input not supported for Mistral cloud provider (rig-core limitation).
Use mistralrs native backend with pixtral models instead.
```

### 5. mistralrs 0.7 Vision Ready

The native `mistralrs` 0.7 backend fully supports vision, unlike the cloud rig-core adapter:

- **16 vision architectures:** Phi3V, Idefics2/3, LLaVA, LLaVANext, Qwen2VL, Pixtral, etc.
- **VisionModelBuilder:** Dedicated builder with `.with_chat_template()` and `.with_vision()`
- **VisionMessages API:** `VisionMessages::new().add_image_message(role, text, image)`
- **Already in Nika:** `mistralrs` is behind `#[cfg(feature = "native-inference")]`

This means Mistral vision works through the native backend even though cloud is broken.

### 6. Fully Async CAS (No spawn_blocking)

The `io::atomic` module uses `tokio::fs` throughout:
- `tokio::fs::File::create()` / `OpenOptions::new().create_new(true).open()`
- `tokio::io::AsyncWriteExt::write_all()`
- `file.flush().await` + `file.sync_all().await`

Blake3 hashing is CPU-only but extremely fast (14x SHA-256). For a 100MB file:
- SHA-256: ~300ms (would need spawn_blocking)
- BLAKE3: ~21ms (fast enough to stay on async thread)

For the P0 media types (images, audio, PDFs), files are typically 1-50MB. BLAKE3 hashing
at ~4.7 GB/s means even 50MB takes ~10ms -- well within async-safe territory.

---

## Architecture: B+ Design (Defense-in-Depth)

| Layer | PR | Check | Description |
|-------|-----|-------|-------------|
| 1 | PR1 | Extraction | Count verification (rmcp blocks vs extracted blocks) |
| 2 | PR1 | Decode | Base64 output verification |
| 3 | PR1 | Storage | CAS read-back hash verification |
| 4 | PR2 | Artifact | Copy size verification |
| 5 | PR2 | Workflow | E2E integrity check at workflow end |

---

## ContentBlock Definition (CORRECTED)

The `Text(String)` tuple variant is **incompatible** with `#[serde(tag = "type")]` -- internally
tagged enums require struct variants so serde can inject the `"type"` field into the JSON object.

```rust
/// Content block in MCP tool results.
///
/// Represents one of five content types returned by MCP tools:
/// text, image (base64), audio (base64), embedded resource, or resource link.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Plain text content
    #[serde(rename = "text")]
    Text {
        text: String,
    },

    /// Base64-encoded image with MIME type
    #[serde(rename = "image")]
    Image {
        data: String,
        mime_type: String,
    },

    /// Base64-encoded audio with MIME type
    #[serde(rename = "audio")]
    Audio {
        data: String,
        mime_type: String,
    },

    /// Embedded resource content (text or blob)
    #[serde(rename = "resource")]
    Resource {
        resource: ResourceContent,
    },

    /// Resource link (URI reference without inline data)
    #[serde(rename = "resource_link")]
    ResourceLink {
        uri: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
    },
}
```

**Why struct variants:** serde internally tagged enums serialize as `{"type": "text", "text": "hello"}`.
A tuple variant `Text(String)` would try to inject `"type"` into a bare string, which fails at runtime.
Every variant must be a struct (or newtype wrapping a struct) for `tag = "type"` to work.

---

## Media Refs Architecture (Side-Channel)

Media references flow through a side-channel to avoid changing the `execute()` return type,
which would ripple through all 5 verb implementations.

```
run_invoke()
  -> MediaProcessor::process() returns Vec<MediaRef>
  -> ctx.datastore.set_media(task_id, media_refs)  // side-channel write (NOT self.datastore)
  ...
runner (after task completes)
  -> ctx.datastore.take_media(task_id)              // side-channel read
  -> TaskResult.media = taken_refs
```

### Implementation: `RunContext.media_staging`

```rust
use dashmap::DashMap;

pub struct RunContext {
    // ... existing fields ...

    /// Workspace root path, threaded from Runner for CAS store location.
    pub workspace_root: PathBuf,

    /// Side-channel for media refs produced by invoke/agent tasks.
    /// Written by run_invoke() after MediaProcessor completes.
    /// Read (and drained) by the runner after task execution.
    pub media_staging: DashMap<String, Vec<MediaRef>>,
}

impl RunContext {
    /// Store media refs for a task (called by run_invoke).
    pub fn set_media(&self, task_id: &str, refs: Vec<MediaRef>) {
        if !refs.is_empty() {
            self.media_staging.insert(task_id.to_string(), refs);
        }
    }

    /// Take media refs for a task (called by runner, drains the entry).
    pub fn take_media(&self, task_id: &str) -> Vec<MediaRef> {
        self.media_staging
            .remove(task_id)
            .map(|(_, v)| v)
            .unwrap_or_default()
    }
}
```

**Why side-channel:** The `execute()` method returns `Result<serde_json::Value, NikaError>` across
all 5 verbs. Changing it to `Result<(Value, Vec<MediaRef>), NikaError>` would require updating
`run_infer`, `run_exec`, `run_fetch`, `run_invoke`, and `run_agent` -- plus all test call sites.
The DashMap side-channel is zero-cost for non-media tasks (no allocation, no insert).

**B4 fix:** In executor functions, `datastore` is accessed via `ctx.datastore` or as a function
parameter -- NOT via `self.datastore`. The runner owns `self.datastore`; executors receive it
through `RunContext`.

### Integration Point: `runner.rs execute_task_iteration()`

```
After:  let tr = make_task_result(output, policy, duration).await;
Add:    let tr = tr.with_media(datastore.take_media(&task_id));
Before: IterationResult construction
```

This is the exact splice point where media refs collected via the side-channel during task
execution are attached to the `TaskResult` before it enters the iteration result pipeline.

---

## CAS Store Requirements (UPDATED v4.2)

### Fully Async via `io::atomic`

All CAS store operations use the existing `io::atomic` module. No `spawn_blocking` needed.
No `tempfile` runtime dependency needed.

```rust
use crate::io::atomic;

impl CasStore {
    /// Store data in the CAS. Returns existing entry on dedup hit.
    ///
    /// Uses `io::atomic::write_fail()` (O_EXCL) for atomic dedup:
    /// - New content: write succeeds, read-back verifies hash
    /// - Duplicate: AlreadyExists error, skip write (content-addressed guarantee)
    pub async fn store(
        &self,
        data: &[u8],
        extension: &str,
    ) -> Result<StoreResult, MediaError> {
        let hash = blake3::hash(data).to_hex().to_string();
        let filename = format!("{}.{}", hash, extension);
        let path = self.root.join(&filename);

        // Ensure store directory exists
        tokio::fs::create_dir_all(&self.root).await.map_err(|e| {
            MediaError::MediaStoreWrite {
                path: self.root.display().to_string(),
                reason: format!("create_dir_all: {e}"),
            }
        })?;

        // O_EXCL atomic write -- dedup is free
        match atomic::write_fail(&path, data).await {
            Ok(()) => { /* new file stored */ }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // Content-addressed: same hash = same content, skip write
                return Ok(StoreResult {
                    hash,
                    path,
                    size: data.len() as u64,
                    extension: extension.to_string(),
                    was_dedup: true,
                });
            }
            Err(e) => {
                return Err(MediaError::MediaStoreWrite {
                    path: path.display().to_string(),
                    reason: e.to_string(),
                });
            }
        }

        // Read-back verification (Layer 3 defense)
        let stored = tokio::fs::read(&path).await.map_err(|e| {
            MediaError::MediaStoreWrite {
                path: path.display().to_string(),
                reason: format!("read-back: {e}"),
            }
        })?;
        let verify_hash = blake3::hash(&stored).to_hex().to_string();
        if verify_hash != hash {
            return Err(MediaError::HashMismatch {
                expected: hash,
                actual: verify_hash,
            });
        }

        Ok(StoreResult {
            hash,
            path,
            size: data.len() as u64,
            extension: extension.to_string(),
            was_dedup: false,
        })
    }
}
```

### Workspace Root Parameter (B5 fix)

```rust
impl CasStore {
    /// Create a CAS store at the workspace default location.
    ///
    /// Uses `NIKA_MEDIA_STORE` env var if set, otherwise `{workspace_root}/.nika/media/store/`.
    /// `workspace_root` is threaded from Runner -> RunContext -> executor.
    pub fn workspace_default(workspace_root: &Path) -> Self {
        let root = std::env::var("NIKA_MEDIA_STORE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| workspace_root.join(".nika").join("media").join("store"));
        Self::new(root)
    }
}
```

### CasEntry Includes Extension

```rust
#[derive(Debug, Clone)]
pub struct CasEntry {
    pub hash: String,
    pub path: PathBuf,
    pub size: u64,
    pub extension: String,  // file extension without dot
}
```

---

## Dependency Impact (v4.2 audit)

### Summary

```
Genuinely new:     blake3 1.8 +mmap (~200KB), infer 0.19 (~50KB)
Zero-cost promote: base64 0.22, mime 0.3, mime_guess 2.0, bytes 1
Already direct:    sha2 0.10, xxhash-rust 0.8, reqwest 0.12, tokio 1.50
NOT needed:        tempfile (runtime), image (pixel ops)
```

### Detail

| Crate | Version | Status | Purpose |
|-------|---------|--------|---------|
| **blake3** | 1.8, features = ["mmap"] | **NEW dep** | CAS hashing, `Hash::to_hex()` -> `ArrayString<64>`, 14x faster than SHA-256, mmap for read-back verify |
| **infer** | 0.19 (0.19.0) | **NEW dep** | Magic byte MIME detection, ~50KB, no dependencies |
| base64 | 0.22 (0.22.1) | Zero-cost promote | Already transitive via rig-core/reqwest. Engine API, MCP uses STANDARD with padding |
| mime | 0.3 | Zero-cost promote | Already transitive via reqwest/hyper |
| mime_guess | 2.0 (2.0.5) | Zero-cost promote | Already transitive via actix-web in some configs. Extension-based fallback |
| bytes | 1 | Zero-cost promote | Already transitive via tokio/hyper/reqwest |
| sha2 | 0.10 | Already direct | Used elsewhere in Nika (not for CAS) |
| xxhash-rust | 0.8 | Already direct | Used for fast non-crypto hashing |
| reqwest | 0.12 | Already direct | HTTP client (fetch verb) |
| tokio | 1.50 | Already direct | Async runtime |
| ~~tempfile~~ | ~~3.27~~ | **NOT needed at runtime** | `io::atomic::write_fail()` replaces tempfile for CAS. tempfile stays as dev-dependency for tests only |
| ~~image~~ | -- | **NOT needed** | No pixel operations required. MIME detection via `infer` magic bytes is sufficient |

**Net impact:** +2 genuinely new crates (~250KB combined). Everything else is already in the dependency tree.

---

## Provider Vision Matrix (PR3 reference)

### Cloud Providers via rig-core 0.32

| Provider | Base64 | URL | Audio Input | DocumentSourceKind::Raw | Notes |
|----------|:------:|:---:|:-----------:|:-----------------------:|-------|
| Claude (Anthropic) | Yes | Yes | No | **Dropped** | Full vision support |
| OpenAI | Yes | Yes | Yes (whisper) | **Dropped** | gpt-4o, gpt-4.1, o4-mini |
| Gemini (Google) | Yes | Yes | Yes | **Dropped** | gemini-2.0-flash, gemini-2.5-pro |
| Mistral | **BUG: silently dropped** | **BUG: silently dropped** | No | **Dropped** | rig-core does not serialize images for Mistral |
| xAI (Grok) | Yes | Yes | No | **Dropped** | grok-2-vision |

### Native Provider via mistralrs 0.7

| Feature | Status | Notes |
|---------|--------|-------|
| Vision architectures | 16 supported | Phi3V, Idefics2/3, LLaVA, LLaVANext, Qwen2VL, Pixtral, etc. |
| VisionModelBuilder | Available | `.with_chat_template()`, `.with_vision()` |
| VisionMessages API | Available | `.add_image_message(role, text, image)` |
| GGUF vision models | Available | pixtral-12b, llava variants |

### PR3 Rules

1. **Always use Base64 or URL** -- never `DocumentSourceKind::Raw` (silently dropped by all providers)
2. **Error on Mistral cloud + media** -- must fail at analyzer phase, not silently at runtime
3. **mistralrs native is the Mistral vision path** -- works correctly, unlike cloud adapter
4. **Re-encode from CAS** -- read bytes from CAS store, base64-encode, pass as `DocumentSourceKind::Base64`

---

## PR Structure

### PR1: `feat/media-pipeline` (Extraction + Processing)

- **Branch:** `feat/media-pipeline`
- **Scope:** ContentBlock enum + rmcp extraction + `src/media/` module + integration
- **Commits:** ~16
- **Tests:** ~40 new
- **New deps:** `blake3 = { version = "1.8", features = ["mmap"] }`, infer 0.19 (genuinely new) + base64 0.22, mime_guess 2.0 (zero-cost promote)
- **Error codes:** NIKA-251..256 (6 new)
- **Events:** 32 -> 36 (4 new: MediaExtracted, MediaProcessed, MediaStored, MediaStoreFailed)

#### Phase A: Type Foundation

ContentBlock enum (struct variants with serde rename), rmcp extraction, helpers.

#### Phase B: Media Module

Errors (with `code() -> &'static str`), types, detect, store (with `io::atomic::write_fail`), processor.

#### Phase C: Integration

RunContext.media_staging side-channel + workspace_root threading, events, verbs.rs wiring.

#### Phase D: Tests

Unit, integration, snapshot tests for the full pipeline.

### PR2: `feat/media-artifacts` (Artifacts + CLI + E2E)

- **Branch:** `feat/media-artifacts`
- **Scope:** Binary format + write_binary + CLI + E2E check
- **Commits:** ~8
- **Tests:** ~15 new
- **Events:** 36 -> 37 (MediaCleanup)

### PR3: `feat/media-input` (Future -- Media INPUT to infer/agent)

- **Branch:** `feat/media-input`
- **Scope:** AST pipeline (parser -> analyzer -> lower -> executor -> provider)
- **Schema:** @0.13 (new YAML syntax: `infer: { media: [...] }`)
- **Touches:** RawInferAction, AnalyzedInferAction, InferParams, RigProvider
- rig-core supports `UserContent::Image` with `DocumentSourceKind::Base64` (NOT Raw)
- **Mistral guard:** Analyzer must reject `media:` blocks targeting Mistral cloud (NIKA-XXX error)
- **Native path:** mistralrs 0.7 VisionMessages API for Mistral vision via native backend

---

## Error Codes (NIKA-251..256)

| Code | Name | Description |
|------|------|-------------|
| 251 | MimeDetectionFailed | MIME type could not be determined |
| 252 | UnsupportedMediaType | Media type not supported |
| 253 | MediaNotFound | Referenced media not found in store |
| 254 | HashMismatch | CAS read-back hash does not match |
| 255 | MediaStoreWrite | Failed to write to media store |
| 256 | Base64DecodeFailed | Base64 decoding failed |
| 257-259 | Reserved | |

### B6 fix: `MediaError::code()` signature

All error types in Nika use `&'static str` for `code()`:

```rust
// NikaError::code() -> &'static str  (src/error.rs:600)
// AnalyzerError::code() -> &'static str  (src/ast/analyzer/errors.rs:194)
// ParseError::code() -> &'static str  (src/ast/raw/parser.rs:50)

// MediaError must follow the same pattern:
impl MediaError {
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
}
```

**Note:** `ToolError::code()` returns `String` (src/tools/mod.rs:174) -- this is the outlier.
MediaError should follow the NikaError pattern (`&'static str`), not the ToolError pattern.

---

## New Module Structure

```
src/media/
├── mod.rs           # exports
├── types.rs         # MediaRef, MediaType, MediaBlock, StoreResult
├── detect.rs        # MIME detection (infer + mime_guess)
├── processor.rs     # MediaProcessor pipeline
├── store.rs         # CAS operations (blake3 + io::atomic::write_fail)
└── error.rs         # NIKA-251..256 (code() -> &'static str)
```

---

## Real-World Media Types

### P0 Types (6 -- cover 95%+ of MCP server output)

| MIME Type | Extension | Category | Source Servers | `infer` Detection |
|-----------|-----------|----------|----------------|:-----------------:|
| `image/png` | .png | Image | Stability, AntV, Firecrawl, Gemini, GPT Image | Yes (`89 50 4E 47`) |
| `image/jpeg` | .jpg | Image | Stability (optional) | Yes (`FF D8 FF`) |
| `image/webp` | .webp | Image | Stability, fal.ai | Yes (`RIFF....WEBP`) |
| `audio/mpeg` | .mp3 | Audio | ElevenLabs, Kokoro | Yes (`ID3` or `FF FB`) |
| `audio/x-wav` | .wav | Audio | VOICEVOX | Yes (`RIFF....WAVE`) |
| `application/pdf` | .pdf | Document | mcp-pdf, pdfcrowd, doc-ops | Yes (`%PDF`) |

### SVG Gap

SVG (`image/svg+xml`) is text-based with no magic bytes. `infer` cannot detect it.
Mitigation: trust declared `mime_type` from MCP server, or check for `<svg` / `<?xml` prefix
in decoded bytes.

### Non-Standard Aliases

Servers may send non-standard MIME types. The `mime_to_ext()` map handles both forms:

| Non-Standard | Standard | Extension |
|-------------|----------|-----------|
| `audio/mp3` | `audio/mpeg` | mp3 |
| `image/jpg` | `image/jpeg` | jpg |
| `audio/wav` | `audio/x-wav` | wav |

---

## Competitive Matrix

```
Feature        MCP Binary  CAS     Verify  MIME    Streaming
--------------------------------------------------------------
n8n            ---         ---     ---     ---     Yes
Dify           ---(no MCP) ---     ---     ---     ---
LangGraph      ---         ---     ---     ---     ---
Temporal       N/A         SHA256  ---     ---     Yes
Nika           Yes(5types) BLAKE3  Yes     Yes     Planned
```

**Nika advantages:**
- Only engine with exhaustive MCP binary extraction (compiler-enforced 5-type match)
- Only engine with CAS read-back verification (Layer 3)
- BLAKE3 hashing (14x faster than SHA-256)
- 3-layer MIME detection (magic bytes -> server hint -> extension guess)
- No other workflow engine in this space combines all four capabilities

---

## Builtin Tools Research Update

### Verified Crate Versions (2026-03-18)

| Crate | Version | Purpose | Status |
|-------|---------|---------|--------|
| whisper-rs | 0.16.0 | Speech-to-text via whisper.cpp | Stable |
| candle | 0.9.2 | ML inference (Hugging Face) | Stable |
| rten | 0.24.0 | ONNX runtime for Rust | Stable |
| **fastembed** | **5.12.1** | Lightweight embedding by Qdrant | **NEW** -- lighter than candle for embeddings |
| **hnswlib-rs** | **0.10.0** | HNSW approximate nearest neighbor | **REPLACES** instant-distance (unmaintained) |

### Changes from v4.0

- **Added:** `fastembed` v5.12.1 -- Qdrant's lightweight embedding crate, better suited for
  builtin `embed:` tool than full candle inference
- **Replaced:** `instant-distance` with `hnswlib-rs` v0.10.0 -- instant-distance has not been
  updated since 2023, hnswlib-rs is actively maintained and wraps the battle-tested hnswlib C++

---

## Key Decisions

| # | Decision | Choice | Rationale |
|---|----------|--------|-----------|
| D1 | PR structure | 2 PRs (merged extraction+processing) | No throwaway code |
| D2 | ContentBlock | Enum with **struct variants** + `#[serde(tag = "type")]` | Internally tagged enums require struct variants; `Text(String)` fails serde |
| D3 | Binary data flow | Side-channel via `RunContext.media_staging` DashMap | Avoids changing `execute()` return type across all 5 verbs |
| D4 | CAS hashing | blake3 (Thibaut-confirmed) | 14x faster than SHA-256, ideal for CAS addressing |
| D5 | Atomic writes | **`io::atomic::write_fail()`** (O_EXCL) | Replaces tempfile. Fully async. Zero new runtime dep. Already tested for binary |
| D6 | Serde format | Internally tagged (`tag="type"`) | MCP JSON compat |
| D7 | Schema version | @0.12 for PR1-PR2 | No new YAML syntax |
| D8 | Artifact limits | Per-format (10MB text, 100MB binary) | Media files are larger |
| D9 | CAS location | Workspace-local (`.nika/media/`) | Isolation by default |
| D10 | __media bridge | Eliminated | Direct MediaProcessor |
| D11 | BinaryWriteRequest | Uses `PathBuf source` (not `Vec<u8>`) for large files | Avoids memory duplication -- reads from CAS path instead of holding bytes in memory |
| D12 | MediaRef field name | `size_bytes` (not `size`) | Explicit unit avoids confusion with string length or other sizes |
| D13 | Media ref transport | `RunContext.media_staging` DashMap side-channel | Zero-cost for non-media tasks, no return type change needed |
| D14 | CAS async strategy | No `spawn_blocking` -- `io::atomic` + BLAKE3 are async-safe | BLAKE3 at 4.7 GB/s makes even 50MB hash ~10ms, well within async budget |
| D15 | Provider image encoding | Always `DocumentSourceKind::Base64` | `Raw(Vec<u8>)` silently dropped by all rig-core provider adapters |
| D16 | Mistral cloud vision | Error at analyzer phase | rig-core 0.32 silently drops images for Mistral -- must fail loudly |
| D17 | Mistral native vision | mistralrs 0.7 VisionMessages API | 16 architectures, works correctly unlike cloud adapter |
| D18 | `MediaError::code()` | Returns `&'static str` | Matches `NikaError::code()`, `AnalyzerError::code()`, `ParseError::code()` signatures |
| D19 | workspace_root threading | `RunContext.workspace_root: PathBuf` | Threaded from Runner (knows workflow path) to executor (needs CAS store path) |
| D20 | for_each media | Per-iteration only, no parent aggregation | Binary merge has no semantics |

---

## Backward Compatibility

ALL changes are additive. Zero breaking changes:

- Text-only workflows unchanged
- `ToolCallResult.text()` still works (enum variant match on `Text { text }`)
- `TaskResult` keeps `output` field, adds `media` field
- `ArtifactFormat` keeps existing variants, adds `Binary`
- `execute()` return type unchanged (side-channel for media)

---

## Competitive Advantages

1. Exhaustive extraction (compiler-enforced match on all 5 rmcp variants)
2. BLAKE3 CAS (14x faster than SHA-256)
3. Read-back verification (unique -- no other engine does this)
4. 5-layer defense-in-depth (unique)
5. Type-safe enum with struct variants (Rust advantage)
6. Atomic CAS writes via `io::atomic::write_fail()` (O_EXCL dedup, fully async, zero new dep)
7. Native binding integration (same `with:` syntax for media refs)
8. 3-layer MIME detection (magic bytes -> server hint -> extension guess)
9. Zero-cost side-channel (DashMap -- no allocation for text-only tasks)
10. Provider-aware vision gating (Mistral bug caught at compile time, not silently at runtime)

---

## Future Optimizations

- **TeeWriter streaming:** Decode + hash + write in a single pass using a custom `AsyncWrite` tee.
  Peak memory drops to 8KB (one buffer) instead of holding the full decoded blob. Best suited for
  large audio/PDF files (10MB+).
- **Bytes type for zero-copy sharing:** Use `bytes::Bytes` for media data passed between processor
  stages to avoid cloning large buffers.
- **MIME declare-then-verify security pattern:** Accept the server-declared MIME type optimistically,
  then verify against magic bytes after decode. Reject on mismatch to prevent content-type confusion
  attacks.
- **`update_mmap` for read-back verify:** Use `blake3`'s mmap feature (`blake3::Hasher::update_mmap`)
  for read-back verification of large CAS files, avoiding a full `tokio::fs::read` allocation.

---

## Verification History

- **v3.1:** 8 agents, 15 bugs found and fixed
- **v4.0:** 15 agents, baselines verified, line numbers updated
- **v4.1:** 8 review agents, 38 issues found and fixed (3 BLOCKERS, 7 HIGH, 5 CRITICAL in PR3)
- **v4.2:** 7 deep audit agents, 6 major discoveries, 3 new blockers fixed (B4-B6), dependency audit complete, provider vision matrix added, CAS redesigned around io::atomic
- **v4.3:** 6 final review agents, 5 remaining issues fixed, TeeWriter pattern identified for future optimization, for_each media policy defined, integration points verified
