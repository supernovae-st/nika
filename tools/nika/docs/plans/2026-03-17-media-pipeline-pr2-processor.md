# PR2: MediaProcessor + CAS Store + MediaRef (v3.1)

> **Branch**: `feat/media-processor`
> **Scope**: New `src/media/` module with detection, hashing, CAS storage, MediaRef type, and read-back verification.
> **Tests**: ~22-28 tests (including proptest)
> **Commits**: ~8-10
> **Depends on**: PR1 merged to main

**Parent**: [Master Plan](./2026-03-17-media-pipeline-master-plan.md) | **Prev**: [PR1](./2026-03-17-media-pipeline-pr1-extraction.md) | **Next**: [PR3](./2026-03-17-media-pipeline-pr3-artifacts.md)

---

## New Dependencies

Add to `Cargo.toml` `[dependencies]`:

```toml
# Media pipeline (PR2)
blake3 = "1.8"           # CAS hashing -- Hash::to_hex() returns ArrayString<64>, zero alloc
infer = "0.19"           # Magic byte MIME detection -- 7 modules, custom matchers
mime_guess = "2.0"       # Extension <-> MIME fallback mapping
base64 = "0.22"          # Already transitive via rmcp, make direct for stability
```

**Rationale for each**:
- **blake3**: Fastest cryptographic hash in Rust. `to_hex()` returns stack-allocated `ArrayString<64>` (no heap). Content-addressable storage key.
- **infer v0.19** (NOT 0.16): Magic byte detection. Modules: `app`, `archive`, `audio`, `book`, `doc`, `font`, `image`, `odf`, `text`, `video`. Custom matcher support.
- **mime_guess v2**: Fallback when magic bytes fail (e.g., SVG which is XML). Extension -> MIME bidirectional.
- **base64 v0.22**: Decode image data from MCP tools. Already in dependency tree via rmcp.

---

## Module Structure

```
src/media/
+-- mod.rs           # pub mod + re-exports
+-- types.rs         # MediaRef, MediaType, MediaBlock
+-- detect.rs        # MIME detection pipeline
+-- processor.rs     # MediaProcessor: decode -> detect -> hash -> store
+-- store.rs         # CAS store: write, read, exists, list, clean
+-- error.rs         # NIKA-251..256 media error variants
```

---

## Tasks

### Task 1: Create media error types

**File**: `src/media/error.rs` (NEW)

```rust
use thiserror::Error;
use miette::Diagnostic;

/// Media-specific errors (NIKA-251..256)
/// NOTE: NIKA-250 is taken by ContextLoadError. Media errors start at 251.
#[derive(Debug, Error, Diagnostic)]
pub enum MediaError {
    /// NIKA-251: MIME type could not be determined
    #[error("NIKA-251: MIME detection failed for {context}")]
    #[diagnostic(code(nika::media::mime_detection))]
    MimeDetectionFailed {
        context: String,
    },

    /// NIKA-252: Media type recognized but not supported
    #[error("NIKA-252: Unsupported media type '{mime_type}'")]
    #[diagnostic(code(nika::media::unsupported_type))]
    UnsupportedMediaType {
        mime_type: String,
    },

    /// NIKA-253: CAS store lookup found no file for hash
    #[error("NIKA-253: Media not found for hash {hash}")]
    #[diagnostic(code(nika::media::not_found))]
    MediaNotFound {
        hash: String,
    },

    /// NIKA-254: blake3 hash verification failed (read-back check)
    #[error("NIKA-254: Hash mismatch for {path}: expected {expected}, got {actual}")]
    #[diagnostic(code(nika::media::hash_mismatch))]
    HashMismatch {
        path: String,
        expected: String,
        actual: String,
    },

    /// NIKA-255: CAS store write failed
    #[error("NIKA-255: Failed to write media to store: {reason}")]
    #[diagnostic(code(nika::media::store_write))]
    MediaStoreWrite {
        reason: String,
        #[source]
        source: std::io::Error,
    },

    /// NIKA-256: Base64 decode failed
    #[error("NIKA-256: Base64 decode failed: {reason}")]
    #[diagnostic(code(nika::media::base64_decode))]
    Base64DecodeFailed {
        reason: String,
    },
}
```

**Commit**: `feat(media): add NIKA-251..256 media error types`

Also add `MediaError::code()` method to the enum:

```rust
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

And add `NikaError::MediaError` variant + exhaustive match arms in `src/error.rs`:

```rust
// In error.rs, add variant (NIKA-250 is ContextLoadError, media starts at 251):
MediaError {
    #[source]
    source: crate::media::error::MediaError,
},

// CRITICAL: Add to ALL exhaustive match methods:
// 1. code() — delegate to inner:
Self::MediaError { source } => source.code(),

// 2. is_recoverable() — MediaStoreWrite could be transient:
Self::MediaError { source } => matches!(source, crate::media::MediaError::MediaStoreWrite { .. }),

// 3. fix_suggestion() — add appropriate suggestions
```

### Task 2: Create MediaRef and MediaType types

**File**: `src/media/types.rs` (NEW)

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Media type categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Image,
    Audio,
    Video,
    Document,
    Archive,
    Other,
}

impl MediaType {
    /// Infer MediaType from MIME string
    pub fn from_mime(mime: &str) -> Self {
        match mime.split('/').next() {
            Some("image") => Self::Image,
            Some("audio") => Self::Audio,
            Some("video") => Self::Video,
            Some("application") => {
                if mime.contains("pdf")
                    || mime.contains("document")
                    || mime.contains("msword")
                    || mime.contains("spreadsheet")
                {
                    Self::Document
                } else if mime.contains("zip")
                    || mime.contains("tar")
                    || mime.contains("gzip")
                    || mime.contains("compressed")
                {
                    Self::Archive
                } else {
                    Self::Other
                }
            }
            _ => Self::Other,
        }
    }
}

/// Reference to a media file stored in the CAS
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MediaRef {
    /// blake3 hex hash (64 chars, content-addressable key)
    pub hash: String,

    /// Detected MIME type (e.g., "image/png")
    pub mime_type: String,

    /// High-level media category
    pub media_type: MediaType,

    /// File size in bytes (decoded, not base64)
    pub size_bytes: u64,

    /// Absolute path to file in CAS store
    pub path: PathBuf,

    /// File extension (e.g., "png")
    pub extension: String,
}

/// A block of media data before it's stored (intermediate form)
pub struct MediaBlock {
    /// Raw binary data (decoded from base64)
    pub data: Vec<u8>,

    /// MIME type hint from source (may be overridden by detection)
    pub mime_hint: Option<String>,

    /// Source URI (for resource-type content)
    pub source_uri: Option<String>,
}
```

**Commit**: `feat(media): add MediaRef, MediaType, and MediaBlock types`

### Task 3: Implement MIME detection

**File**: `src/media/detect.rs` (NEW)

```rust
use super::error::MediaError;

/// Detect MIME type from binary data with fallback chain:
/// 1. infer (magic bytes) -- most reliable
/// 2. mime_guess (from extension hint) -- fallback
/// 3. "application/octet-stream" -- last resort
///
/// Cross-validates server-provided MIME vs detected MIME when both available.
pub fn detect_mime(
    data: &[u8],
    extension_hint: Option<&str>,
    server_mime: Option<&str>,
) -> Result<(String, Option<String>), MediaError> {
    // Strategy 1: Magic bytes (most reliable)
    let detected = infer::get(data).map(|kind| kind.mime_type().to_string());

    // Strategy 2: Extension hint -> mime_guess
    let guessed = extension_hint.and_then(|ext| {
        mime_guess::from_ext(ext).first().map(|m| m.to_string())
    });

    // Determine final MIME (prefer magic bytes > server > guess > fallback)
    let final_mime = detected.clone()
        .or_else(|| server_mime.map(|s| s.to_string()))
        .or(guessed)
        .unwrap_or_else(|| {
            if !data.is_empty() {
                "application/octet-stream".to_string()
            } else {
                String::new()
            }
        });

    if final_mime.is_empty() {
        return Err(MediaError::MimeDetectionFailed {
            context: "empty data with no extension or server hint".to_string(),
        });
    }

    // Cross-validate: warn if server MIME differs from detected
    let mismatch_warning = if let (Some(ref det), Some(srv)) = (&detected, server_mime) {
        if det != srv {
            tracing::warn!(
                detected = %det,
                server = %srv,
                "Server MIME differs from detected MIME, using detected"
            );
            Some(format!("server={}, detected={}", srv, det))
        } else {
            None
        }
    } else {
        None
    };

    Ok((final_mime, mismatch_warning))
}

/// Get file extension from MIME type
pub fn mime_to_extension(mime: &str) -> String {
    // Use mime_guess for MIME -> extension mapping
    let exts = mime_guess::get_mime_extensions_str(mime);
    if let Some(ext_list) = exts {
        // Prefer common extensions
        let preferred = ["png", "jpg", "gif", "webp", "svg", "mp3", "wav",
                         "mp4", "pdf", "zip"];
        for p in &preferred {
            if ext_list.contains(p) {
                return p.to_string();
            }
        }
        return ext_list[0].to_string();
    }

    // Fallback: extract from MIME subtype, SANITIZED to prevent path traversal
    let raw = mime.split('/').last().unwrap_or("bin");
    // Only keep alphanumeric + hyphen — strips '/', '..', etc.
    let sanitized: String = raw.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if sanitized.is_empty() { "bin".to_string() } else { sanitized }
}
```

**Commit**: `feat(media): implement MIME detection with infer + mime_guess + cross-validation`

### Task 4: Implement CAS store with read-back verification

**File**: `src/media/store.rs` (NEW)

```rust
use std::path::{Path, PathBuf};
use tokio::fs;
use super::error::MediaError;

/// Content-Addressable Store for media files
///
/// Layout: `{root}/{hash[0..2]}/{hash[2..64]}.{ext}`
/// Example: `.nika/media/store/a1/b2c3d4...f5.png`
///
/// - 256-directory fan-out (00..ff) via first 2 hex chars
/// - Deduplication: same content -> same hash -> same file
/// - Uses Nika's existing io::atomic::write_atomic for crash safety
/// - READ-BACK VERIFICATION: re-hash stored file after write
pub struct CasStore {
    root: PathBuf,
}

/// Result of a CAS store operation
pub struct StoreResult {
    /// Path to the stored file
    pub path: PathBuf,
    /// Whether this was a dedup hit (file already existed)
    pub deduplicated: bool,
    /// Whether read-back verification passed
    pub verified: bool,
}

impl CasStore {
    /// Create a new CAS store at the given root directory
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Default store location: `.nika/media/store/`
    pub fn workspace_default() -> Self {
        Self::new(".nika/media/store")
    }

    /// Compute blake3 hash of data, returning 64-char hex string
    pub fn hash(data: &[u8]) -> String {
        blake3::hash(data).to_hex().to_string()
    }

    /// Build CAS path from hash and extension
    pub fn path_for(&self, hash: &str, extension: &str) -> PathBuf {
        let prefix = &hash[..2];   // First 2 hex chars = directory
        let rest = &hash[2..];     // Remaining 62 chars = filename
        self.root.join(prefix).join(format!("{}.{}", rest, extension))
    }

    /// Check if content already exists in store (dedup check)
    pub async fn exists(&self, hash: &str, extension: &str) -> bool {
        let path = self.path_for(hash, extension);
        fs::metadata(&path).await.is_ok()
    }

    /// Store binary data with read-back verification (Layer 3 defense-in-depth)
    ///
    /// - Dedup: if hash exists, verifies integrity and returns existing path
    /// - Atomic: uses io::atomic::write_atomic (temp -> flush -> sync -> rename)
    /// - Verify: re-reads stored file, re-hashes, compares to expected
    /// - Creates parent directories as needed
    pub async fn store(
        &self,
        data: &[u8],
        hash: &str,
        extension: &str,
    ) -> Result<StoreResult, MediaError> {
        let path = self.path_for(hash, extension);

        // Dedup: already stored, verify integrity and return
        if self.exists(hash, extension).await {
            let verified = self.verify_integrity(&path, hash).await?;
            return Ok(StoreResult {
                path,
                deduplicated: true,
                verified,
            });
        }

        // Create parent directory (the 2-char prefix dir)
        let parent = path.parent().expect("CAS path always has parent");
        fs::create_dir_all(parent).await.map_err(|e| {
            MediaError::MediaStoreWrite {
                reason: format!("failed to create directory: {}", parent.display()),
                source: e,
            }
        })?;

        // Atomic write using Nika's existing pattern
        crate::io::atomic::write_atomic(&path, data)
            .await
            .map_err(|e| MediaError::MediaStoreWrite {
                reason: format!("atomic write failed: {}", path.display()),
                source: e,
            })?;

        // READ-BACK VERIFICATION (Layer 3 defense-in-depth)
        // Re-read the file and re-hash to verify write integrity
        let verified = self.verify_integrity(&path, hash).await?;

        Ok(StoreResult {
            path,
            deduplicated: false,
            verified,
        })
    }

    /// Verify stored file integrity by re-reading and re-hashing
    async fn verify_integrity(&self, path: &Path, expected_hash: &str) -> Result<bool, MediaError> {
        let stored_data = fs::read(path).await.map_err(|e| {
            MediaError::MediaStoreWrite {
                reason: format!("read-back verification failed: {}", path.display()),
                source: e,
            }
        })?;
        let actual_hash = Self::hash(&stored_data);
        if actual_hash != expected_hash {
            return Err(MediaError::HashMismatch {
                path: path.display().to_string(),
                expected: expected_hash.to_string(),
                actual: actual_hash,
            });
        }
        Ok(true)
    }

    /// Decode verification (Layer 2 defense-in-depth)
    /// Check that decoded data is non-empty when source was non-empty
    pub fn verify_decode(source_len: usize, decoded: &[u8]) -> bool {
        if source_len > 0 && decoded.is_empty() {
            tracing::error!(
                source_len,
                "Base64 decode produced empty output from non-empty input"
            );
            return false;
        }
        true
    }

    /// List all stored files, returning (hash, extension, size, path)
    pub async fn list(&self) -> std::io::Result<Vec<(String, String, u64, PathBuf)>> {
        let mut entries = Vec::new();

        // Handle case where store directory doesn't exist yet
        if !self.root.exists() {
            return Ok(entries);
        }

        let mut prefix_dirs = fs::read_dir(&self.root).await?;

        while let Some(prefix_entry) = prefix_dirs.next_entry().await? {
            if !prefix_entry.file_type().await?.is_dir() {
                continue;
            }
            let prefix = prefix_entry.file_name().to_string_lossy().to_string();
            let mut files = fs::read_dir(prefix_entry.path()).await?;

            while let Some(file_entry) = files.next_entry().await? {
                let filename = file_entry.file_name().to_string_lossy().to_string();
                if let Some((rest, ext)) = filename.rsplit_once('.') {
                    let hash = format!("{}{}", prefix, rest);
                    let meta = file_entry.metadata().await?;
                    entries.push((hash, ext.to_string(), meta.len(), file_entry.path()));
                }
            }
        }
        Ok(entries)
    }

    /// Remove files older than the given duration
    pub async fn clean_older_than(
        &self,
        max_age: std::time::Duration,
    ) -> std::io::Result<(u32, u64)> {
        let mut removed = 0u32;
        let mut freed = 0u64;
        let cutoff = std::time::SystemTime::now()
            .checked_sub(max_age)
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

        let entries = self.list().await?;
        for (_, _, size, path) in entries {
            if let Ok(meta) = fs::metadata(&path).await {
                if let Ok(modified) = meta.modified() {
                    if modified < cutoff {
                        if fs::remove_file(&path).await.is_ok() {
                            removed += 1;
                            freed += size;
                        }
                    }
                }
            }
        }
        Ok((removed, freed))
    }

    /// Remove all stored files
    pub async fn clean_all(&self) -> std::io::Result<(u32, u64)> {
        let entries = self.list().await?;
        let mut removed = 0u32;
        let mut freed = 0u64;
        for (_, _, size, path) in entries {
            match fs::remove_file(&path).await {
                Ok(()) => {
                    removed += 1;
                    freed += size;
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "Failed to remove CAS file");
                }
            }
        }
        Ok((removed, freed))
    }
}
```

**Commit**: `feat(media): implement CAS store with blake3 hashing, atomic writes, and read-back verification`

### Task 5: Implement MediaProcessor

**File**: `src/media/processor.rs` (NEW)

```rust
use base64::Engine;
use super::{
    detect,
    error::MediaError,
    store::CasStore,
    types::{MediaBlock, MediaRef, MediaType},
};
use crate::mcp::types::ContentBlock;

/// Processes raw MCP content blocks into stored media references
pub struct MediaProcessor {
    store: CasStore,
}

impl MediaProcessor {
    pub fn new(store: CasStore) -> Self {
        Self { store }
    }

    /// Process a ContentBlock, returning MediaRef only (convenience)
    pub async fn process_block(
        &self,
        block: &ContentBlock,
    ) -> Result<Option<MediaRef>, MediaError> {
        Ok(self.process_block_with_result(block).await?.map(|(r, _)| r))
    }

    /// Process a ContentBlock, returning (MediaRef, StoreResult) for telemetry.
    /// Returns None for text-only blocks (they don't need media processing).
    pub async fn process_block_with_result(
        &self,
        block: &ContentBlock,
    ) -> Result<Option<(MediaRef, super::store::StoreResult)>, MediaError> {
        match block.content_type.as_str() {
            // Image and Audio have identical shape: data (base64) + mime_type
            "image" | "audio" => {
                let data_b64 = block.data.as_ref().ok_or_else(|| {
                    MediaError::Base64DecodeFailed {
                        reason: format!("{} block has no data field", block.content_type),
                    }
                })?;
                let mime_hint = block.mime_type.as_deref();
                let media_block = self.decode_base64(data_b64, mime_hint, None)?;
                Ok(Some(self.store_block(media_block).await?))
            }
            "resource" => {
                if let Some(ref resource) = block.resource {
                    if let Some(ref blob) = resource.blob {
                        // Binary resource
                        let mime_hint = resource.mime_type.as_deref();
                        let source_uri = Some(resource.uri.clone());
                        let media_block = self.decode_base64(blob, mime_hint, source_uri)?;
                        Ok(Some(self.store_block(media_block).await?))
                    } else {
                        Ok(None)  // Text-only resource -- not media
                    }
                } else {
                    Ok(None)
                }
            }
            "text" => Ok(None),  // Text blocks are not media
            _ => Ok(None),       // Unknown types skipped
        }
    }

    /// Process all blocks, returning MediaRefs (convenience wrapper)
    pub async fn process_all(
        &self,
        blocks: &[ContentBlock],
    ) -> Result<Vec<MediaRef>, MediaError> {
        Ok(self.process_all_with_results(blocks).await?
            .into_iter().map(|(r, _)| r).collect())
    }

    /// Process all blocks, returning (MediaRef, StoreResult) pairs for telemetry
    pub async fn process_all_with_results(
        &self,
        blocks: &[ContentBlock],
    ) -> Result<Vec<(MediaRef, super::store::StoreResult)>, MediaError> {
        let mut results = Vec::new();
        for block in blocks {
            if let Some(result) = self.process_block_with_result(block).await? {
                results.push(result);
            }
        }
        Ok(refs)
    }

    /// Maximum raw base64 input size (100MB base64 ≈ 75MB decoded).
    /// Prevents OOM from malicious MCP servers sending huge payloads.
    const MAX_BASE64_INPUT_BYTES: usize = 100 * 1024 * 1024; // 100MB

    /// Decode base64 data into a MediaBlock
    fn decode_base64(
        &self,
        data_b64: &str,
        mime_hint: Option<&str>,
        source_uri: Option<String>,
    ) -> Result<MediaBlock, MediaError> {
        // Guard: reject oversized base64 BEFORE allocating decode buffer (OOM protection)
        if data_b64.len() > Self::MAX_BASE64_INPUT_BYTES {
            return Err(MediaError::Base64DecodeFailed {
                reason: format!(
                    "base64 input too large: {} bytes (max {})",
                    data_b64.len(),
                    Self::MAX_BASE64_INPUT_BYTES,
                ),
            });
        }

        let data = base64::engine::general_purpose::STANDARD
            .decode(data_b64)
            .map_err(|e| MediaError::Base64DecodeFailed {
                reason: e.to_string(),
            })?;

        // Layer 2 defense-in-depth: verify decode produced output
        if !CasStore::verify_decode(data_b64.len(), &data) {
            return Err(MediaError::Base64DecodeFailed {
                reason: format!(
                    "decode produced empty output from {} byte input",
                    data_b64.len()
                ),
            });
        }

        Ok(MediaBlock {
            data,
            mime_hint: mime_hint.map(|s| s.to_string()),
            source_uri,
        })
    }

    /// Store a MediaBlock in CAS and return (MediaRef, StoreResult)
    async fn store_block(&self, block: MediaBlock) -> Result<(MediaRef, super::store::StoreResult), MediaError> {
        // Guard: reject empty content — a 0-byte file is never valid media
        // (prevents phantom CAS files when server MIME is provided but data is empty)
        if block.data.is_empty() {
            return Err(MediaError::Base64DecodeFailed {
                reason: "decoded media content is empty (0 bytes)".to_string(),
            });
        }

        // Detect MIME type with cross-validation
        let ext_hint = block.mime_hint.as_deref().and_then(|m| {
            m.split('/').last()
        });
        let server_mime = block.mime_hint.as_deref();
        let (mime_type, _mismatch) = detect::detect_mime(
            &block.data, ext_hint, server_mime
        )?;
        let extension = detect::mime_to_extension(&mime_type);
        let media_type = MediaType::from_mime(&mime_type);

        // Hash content
        let hash = CasStore::hash(&block.data);
        let size_bytes = block.data.len() as u64;

        // Store in CAS with read-back verification
        let store_result = self.store.store(&block.data, &hash, &extension).await?;

        let media_ref = MediaRef {
            hash,
            mime_type,
            media_type,
            size_bytes,
            path: store_result.path.clone(),
            extension,
        };
        Ok((media_ref, store_result))
    }
}
```

**Commit**: `feat(media): implement MediaProcessor with decode -> detect -> hash -> store pipeline`

### Task 6: Create module root and register in lib.rs

**File**: `src/media/mod.rs` (NEW)

```rust
//! Media pipeline for binary content from MCP tools
//!
//! Processes Image, Audio, and Resource content blocks from MCP tool results
//! into content-addressed storage with MediaRef references.
//!
//! # Architecture
//!
//! ```text
//! ContentBlock (from rmcp) -> MediaProcessor -> CasStore -> MediaRef
//!     decode base64          detect MIME       hash+store   reference
//! ```

pub mod detect;
pub mod error;
pub mod processor;
pub mod store;
pub mod types;

// Re-exports
pub use error::MediaError;
pub use processor::MediaProcessor;
pub use store::CasStore;
pub use types::{MediaBlock, MediaRef, MediaType};
```

**File**: `src/lib.rs` -- Add after `pub mod tools;`:

```rust
pub mod media;
```

**Commit**: `feat(media): register media module in lib.rs`

### Task 7: Add MediaRef to TaskResult

**File**: `src/store/run_context.rs`

> **Source of truth** (verified): `TaskResult` has 3 fields: `output: Arc<Value>`, `duration: Duration`, `status: TaskOutcome`. No media field yet.

Add media field to `TaskResult`:

```rust
pub struct TaskResult {
    pub output: Arc<Value>,
    pub duration: Duration,
    pub status: TaskOutcome,
    /// Media references from MCP tool binary content (images, audio, etc.)
    /// Default empty -- zero impact on existing text-only workflows.
    pub media: Vec<crate::media::MediaRef>,
}
```

Update ALL constructors to initialize `media: Vec::new()`:

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

    pub fn failed(reason: impl Into<String>, duration: Duration) -> Self {
        Self {
            output: Arc::new(Value::Null),
            duration,
            status: TaskOutcome::Failed(reason.into()),
            media: Vec::new(),
        }
    }

    pub fn dependency_failed(dependency: impl Into<String>) -> Self {
        Self {
            output: Arc::new(Value::Null),
            duration: Duration::ZERO,
            status: TaskOutcome::DependencyFailed { dependency: dependency.into() },
            media: Vec::new(),
        }
    }

    pub fn skipped(reason: impl Into<String>) -> Self {
        Self {
            output: Arc::new(Value::Null),
            duration: Duration::ZERO,
            status: TaskOutcome::Skipped { reason: reason.into() },
            media: Vec::new(),
        }
    }

    /// Attach media references to this result
    pub fn with_media(mut self, media: Vec<crate::media::MediaRef>) -> Self {
        self.media = media;
        self
    }
}
```

**Also extend `RunContext.resolve_path()`** (line 243-263) to handle `"media"` segment:

```rust
// In resolve_path(), after splitting task_id from path:
// Check if remaining path starts with "media"
pub fn resolve_path(&self, path: &str) -> Option<Value> {
    let (task_id, remaining) = path.split_once('.').unwrap_or((path, ""));
    let result = self.results.get(task_id)?;

    if remaining.is_empty() {
        return Some((*result.output).clone());
    }

    // NEW: Intercept "media" path to access TaskResult.media field
    if remaining == "media" || remaining.starts_with("media[") || remaining.starts_with("media.") {
        let media_json: Vec<Value> = result.media.iter().map(|m| {
            serde_json::json!({
                "hash": m.hash,
                "mime_type": m.mime_type,
                "media_type": m.media_type,
                "size_bytes": m.size_bytes,
                "path": m.path.display().to_string(),
                "extension": m.extension,
            })
        }).collect();
        let media_value = Value::Array(media_json);

        // Strip "media" prefix and apply remaining jsonpath
        let rest = remaining.strip_prefix("media").unwrap_or("");
        if rest.is_empty() {
            return Some(media_value);
        }
        // Normalize bracket notation: "media[0].path" -> "[0].path" -> jsonpath
        return crate::binding::jsonpath::resolve(&media_value, rest).ok().flatten();
    }

    // Existing: jsonpath into output
    crate::binding::jsonpath::resolve(&result.output, remaining).ok().flatten()
}
```

This enables `with:` bindings like:
```yaml
with:
  img_path: generate.media[0].path
  img_ext: generate.media[0].extension
```
Which resolve via `{{with.img_path}}` in templates — consistent with Nika's existing template architecture.

**Commit**: `feat(store): add media field to TaskResult + resolve_path() media support`

### Task 8: Add EventKind variants for media processing

**File**: `src/event/log.rs`

Add 3 new variants to `EventKind` (after `MediaExtracted` from PR1):

```rust
/// Media binary content was processed and stored in CAS
MediaProcessed {
    task_id: Arc<str>,
    hash: String,
    mime_type: String,
    media_type: String,          // "image"|"audio"|"document"
    size_bytes: u64,
    server_mime: Option<String>, // MCP server's claim (for cross-validation)
},

/// Media file stored in CAS with verification result
MediaStored {
    task_id: Arc<str>,
    hash: String,
    path: String,
    size_bytes: u64,
    verified: bool,              // read-back hash verification passed
    deduplicated: bool,          // file already existed in CAS
},

/// Media store operation failed
MediaStoreFailed {
    task_id: Arc<str>,
    hash: String,
    reason: String,
},
```

**CRITICAL**: Add ALL 3 to `EventKind::task_id()` match arm:
```rust
| Self::MediaProcessed { task_id, .. }
| Self::MediaStored { task_id, .. }
| Self::MediaStoreFailed { task_id, .. }
```

Update `count_all_variants` test (33 from PR1 -> 36 after PR2).

**Commit**: `feat(event): add MediaProcessed, MediaStored, MediaStoreFailed events`

### Task 9: Wire MediaProcessor into invoke verb

**File**: `src/runtime/executor/verbs.rs`

Replace the `__media` bridge from PR1 with proper MediaProcessor integration:

```rust
// In the invoke verb handler, after getting tool_result:
let media_refs = if tool_result.has_media() {
    let store = CasStore::workspace_default();
    let processor = MediaProcessor::new(store);

    // NOTE: process_all_with_results returns (MediaRef, StoreResult) pairs
    // so we can thread verified/deduplicated to telemetry events.
    match processor.process_all_with_results(&tool_result.content).await {
        Ok(results) => {
            let refs: Vec<MediaRef> = results.iter().map(|(r, _)| r.clone()).collect();
            // Emit events for each stored media
            for (media_ref, store_result) in &results {
                self.event_log.emit(EventKind::MediaProcessed {
                    task_id: task_id.clone(),
                    hash: media_ref.hash.clone(),
                    mime_type: media_ref.mime_type.clone(),
                    media_type: format!("{:?}", media_ref.media_type).to_lowercase(),
                    size_bytes: media_ref.size_bytes,
                    server_mime: None, // Server MIME available via ContentBlock.mime_type
                });
                self.event_log.emit(EventKind::MediaStored {
                    task_id: task_id.clone(),
                    hash: media_ref.hash.clone(),
                    path: media_ref.path.display().to_string(),
                    size_bytes: media_ref.size_bytes,
                    verified: store_result.verified,
                    deduplicated: store_result.deduplicated,
                });
            }
            refs
        }
        Err(e) => {
            self.event_log.emit(EventKind::MediaStoreFailed {
                task_id: task_id.clone(),
                hash: String::new(),
                reason: e.to_string(),
            });
            return Err(NikaError::MediaError { source: e });
        }
    }
} else {
    Vec::new()
};

// ... later when building TaskResult:
// REMOVE __media bridge from output (PR1 temporary code)
// Replace with proper MediaRef storage:
let result = TaskResult::success(output, elapsed).with_media(media_refs);
```

**Commit**: `feat(runtime): wire MediaProcessor into invoke verb, remove __media bridge`

### Task 10: Tests

#### proptest for binary edge cases (`src/media/detect.rs` tests):

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn detect_mime_never_panics(data in proptest::collection::vec(any::<u8>(), 0..10000)) {
        // Should never panic, even on random data
        let _ = detect_mime(&data, None, None);
    }

    #[test]
    fn detect_mime_with_random_ext(
        data in proptest::collection::vec(any::<u8>(), 1..1000),
        ext in "[a-z]{1,5}"
    ) {
        let result = detect_mime(&data, Some(&ext), None);
        // Should always return Ok for non-empty data
        assert!(result.is_ok());
    }
}
```

#### Unit tests for MIME detection:

```rust
#[test]
fn test_detect_png() {
    // PNG magic bytes: 89 50 4E 47
    let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    let (mime, _) = detect_mime(&png_header, None, None).unwrap();
    assert_eq!(mime, "image/png");
}

#[test]
fn test_detect_jpeg() {
    // JPEG magic bytes: FF D8 FF
    let jpg_header = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
    let (mime, _) = detect_mime(&jpg_header, None, None).unwrap();
    assert!(mime.contains("jpeg") || mime.contains("jpg"));
}

#[test]
fn test_cross_validation_mismatch() {
    // PNG magic bytes but server says JPEG
    let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    let (mime, warning) = detect_mime(&png_header, None, Some("image/jpeg")).unwrap();
    assert_eq!(mime, "image/png");  // Magic bytes win
    assert!(warning.is_some());     // Mismatch logged
}

#[test]
fn test_fallback_to_octet_stream() {
    let unknown = [0x00, 0x01, 0x02, 0x03];
    let (mime, _) = detect_mime(&unknown, None, None).unwrap();
    assert_eq!(mime, "application/octet-stream");
}

#[test]
fn test_empty_data_fails() {
    let result = detect_mime(&[], None, None);
    assert!(result.is_err());
}
```

#### Unit tests for CAS store:

```rust
#[tokio::test]
async fn test_store_and_retrieve() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path().join("store"));
    let data = b"hello world";
    let hash = CasStore::hash(data);

    let result = store.store(data, &hash, "txt").await.unwrap();
    assert!(result.path.exists());
    assert!(!result.deduplicated);
    assert!(result.verified);

    // Verify content
    let stored = tokio::fs::read(&result.path).await.unwrap();
    assert_eq!(stored, data);
}

#[tokio::test]
async fn test_dedup_same_content() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path().join("store"));
    let data = b"duplicate me";
    let hash = CasStore::hash(data);

    let first = store.store(data, &hash, "bin").await.unwrap();
    let second = store.store(data, &hash, "bin").await.unwrap();

    assert_eq!(first.path, second.path);
    assert!(!first.deduplicated);
    assert!(second.deduplicated);  // Second store hits dedup
    assert!(second.verified);      // Still verified on dedup path
}

#[tokio::test]
async fn test_hash_deterministic() {
    let data = b"deterministic test";
    let h1 = CasStore::hash(data);
    let h2 = CasStore::hash(data);
    assert_eq!(h1, h2);
    assert_eq!(h1.len(), 64); // blake3 hex = 64 chars
}

#[tokio::test]
async fn test_path_layout() {
    let store = CasStore::new("/tmp/cas");
    let hash = "a1b2c3d4e5f6".to_string() + &"0".repeat(52); // 64 chars
    let path = store.path_for(&hash, "png");
    assert!(path.to_string_lossy().contains("a1/"));
    assert!(path.to_string_lossy().ends_with(".png"));
}

#[tokio::test]
async fn test_verify_decode() {
    assert!(CasStore::verify_decode(10, &[1, 2, 3])); // non-empty -> non-empty = OK
    assert!(CasStore::verify_decode(0, &[]));          // empty -> empty = OK
    assert!(!CasStore::verify_decode(10, &[]));         // non-empty -> empty = BAD
}

#[tokio::test]
async fn test_list_empty_store() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path().join("nonexistent"));
    let entries = store.list().await.unwrap();
    assert!(entries.is_empty());
}

#[tokio::test]
async fn test_clean_all() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path().join("store"));
    store.store(b"img1", &CasStore::hash(b"img1"), "png").await.unwrap();
    store.store(b"img2", &CasStore::hash(b"img2"), "jpg").await.unwrap();
    store.store(b"img3", &CasStore::hash(b"img3"), "gif").await.unwrap();

    let (removed, _) = store.clean_all().await.unwrap();
    assert_eq!(removed, 3);
    assert!(store.list().await.unwrap().is_empty());
}
```

#### MediaType tests:

```rust
#[test]
fn test_media_type_from_mime() {
    assert_eq!(MediaType::from_mime("image/png"), MediaType::Image);
    assert_eq!(MediaType::from_mime("audio/mp3"), MediaType::Audio);
    assert_eq!(MediaType::from_mime("video/mp4"), MediaType::Video);
    assert_eq!(MediaType::from_mime("application/pdf"), MediaType::Document);
    assert_eq!(MediaType::from_mime("application/zip"), MediaType::Archive);
    assert_eq!(MediaType::from_mime("application/octet-stream"), MediaType::Other);
    assert_eq!(MediaType::from_mime("text/plain"), MediaType::Other);
}
```

#### insta snapshots for MediaRef serialization:

```rust
#[test]
fn test_media_ref_json_snapshot() {
    let media_ref = MediaRef {
        hash: "a1b2c3d4e5f6".to_string() + &"0".repeat(52),
        mime_type: "image/png".to_string(),
        media_type: MediaType::Image,
        size_bytes: 12345,
        path: PathBuf::from(".nika/media/store/a1/b2c3d4e5f6" .to_string() + &"0".repeat(52) + ".png"),
        extension: "png".to_string(),
    };
    insta::assert_json_snapshot!(media_ref);
}
```

#### Integration test with full pipeline:

```rust
#[tokio::test]
async fn test_full_pipeline_image_to_cas() {
    // Minimal valid PNG (1x1 transparent pixel)
    let png_1x1: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1x1
        0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, // RGBA
        0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, // IDAT
        0x78, 0x9C, 0x62, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0xE5,
        0x27, 0xDE, 0xFC,
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, // IEND
        0xAE, 0x42, 0x60, 0x82,
    ];
    let b64 = base64::engine::general_purpose::STANDARD.encode(png_1x1);

    let block = ContentBlock::image(b64, "image/png".into());
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path().join("store"));
    let processor = MediaProcessor::new(store);

    let refs = processor.process_all(&[block]).await.unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].mime_type, "image/png");
    assert_eq!(refs[0].media_type, MediaType::Image);
    assert_eq!(refs[0].extension, "png");
    assert!(refs[0].path.exists());

    // Verify content integrity (end-to-end)
    let stored = tokio::fs::read(&refs[0].path).await.unwrap();
    assert_eq!(stored, png_1x1);

    // Verify hash matches
    let expected_hash = CasStore::hash(png_1x1);
    assert_eq!(refs[0].hash, expected_hash);
}

#[tokio::test]
async fn test_pipeline_text_only_produces_no_media() {
    let block = ContentBlock::text("just text".into());
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path().join("store"));
    let processor = MediaProcessor::new(store);

    let refs = processor.process_all(&[block]).await.unwrap();
    assert!(refs.is_empty());
}

#[tokio::test]
async fn test_pipeline_audio_stored_in_cas() {
    // Minimal valid WAV header (44 bytes RIFF header)
    let wav_header: &[u8] = &[
        0x52, 0x49, 0x46, 0x46, // "RIFF"
        0x24, 0x00, 0x00, 0x00, // chunk size
        0x57, 0x41, 0x56, 0x45, // "WAVE"
        0x66, 0x6D, 0x74, 0x20, // "fmt "
        0x10, 0x00, 0x00, 0x00, // subchunk1 size
        0x01, 0x00, 0x01, 0x00, // PCM, mono
        0x44, 0xAC, 0x00, 0x00, // 44100 Hz
        0x88, 0x58, 0x01, 0x00, // byte rate
        0x02, 0x00, 0x10, 0x00, // block align, bits per sample
        0x64, 0x61, 0x74, 0x61, // "data"
        0x00, 0x00, 0x00, 0x00, // data size
    ];
    let b64 = base64::engine::general_purpose::STANDARD.encode(wav_header);

    let block = ContentBlock::audio(b64, "audio/wav".into());
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path().join("store"));
    let processor = MediaProcessor::new(store);

    let refs = processor.process_all(&[block]).await.unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].media_type, MediaType::Audio);
    assert!(refs[0].path.exists());
}

#[tokio::test]
async fn test_pipeline_invalid_base64_fails() {
    let block = ContentBlock::image("not-valid-base64!!!".into(), "image/png".into());
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path().join("store"));
    let processor = MediaProcessor::new(store);

    let result = processor.process_all(&[block]).await;
    assert!(result.is_err());
    // Verify it's a Base64DecodeFailed error
    match result.unwrap_err() {
        MediaError::Base64DecodeFailed { .. } => {},
        other => panic!("Expected Base64DecodeFailed, got: {:?}", other),
    }
}
```

#### Event telemetry tests:

```rust
#[test]
fn test_media_processed_event_task_id() {
    let event = EventKind::MediaProcessed {
        task_id: "gen_image".into(),
        hash: "abc123".into(),
        mime_type: "image/png".into(),
        media_type: "image".into(),
        size_bytes: 1024,
        server_mime: Some("image/png".into()),
    };
    assert_eq!(event.task_id(), Some("gen_image"));
}

#[test]
fn test_media_stored_event_serde() {
    let event = EventKind::MediaStored {
        task_id: "t1".into(),
        hash: "abc123".into(),
        path: ".nika/media/store/ab/c123.png".into(),
        size_bytes: 1024,
        verified: true,
        deduplicated: false,
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"media_stored\""));
    assert!(json.contains("\"verified\":true"));
    assert!(json.contains("\"deduplicated\":false"));
}

#[test]
fn test_media_store_failed_event() {
    let event = EventKind::MediaStoreFailed {
        task_id: "t1".into(),
        hash: "def456".into(),
        reason: "disk full".into(),
    };
    assert_eq!(event.task_id(), Some("t1"));
}
```

**Commits**:
- `test(media): add proptest for MIME detection edge cases`
- `test(media): add MIME detection unit tests with cross-validation`
- `test(media): add CAS store unit tests with dedup + read-back verification`
- `test(media): add MediaType + insta snapshot tests`
- `test(media): add full pipeline integration test`
- `test(event): add MediaProcessed/Stored/Failed event tests`

---

## Verification Checklist

- [ ] `cargo test` -- all tests pass (existing + new)
- [ ] `cargo clippy -- -D warnings` -- zero warnings
- [ ] blake3 hash is deterministic (same input -> same hash, 64 hex chars)
- [ ] CAS dedup works (store same image twice -> 1 file, second returns deduplicated=true)
- [ ] io::atomic::write_atomic used for CAS writes (crash safety)
- [ ] READ-BACK verification passes (re-hash stored file matches expected)
- [ ] Layer 2 decode verification catches empty output from non-empty input
- [ ] infer v0.19 detects PNG, JPEG, GIF, WebP, PDF
- [ ] mime_guess fallback works when magic bytes fail (e.g., SVG)
- [ ] Cross-validation warns when server MIME differs from detected MIME
- [ ] base64 decode handles standard + URL-safe variants
- [ ] proptest finds no panics on random binary data
- [ ] MediaRef serializes correctly (insta snapshot)
- [ ] TaskResult.media field is backward-compatible (empty vec = no change)
- [ ] All 3 new EventKind variants added to task_id() match arm
- [ ] Events emitted for each stored media file (MediaProcessed + MediaStored)
- [ ] MediaStoreFailed event emitted on error path
- [ ] `__media` bridge from PR1 REMOVED (replaced by proper MediaRef)
- [ ] Merge to main, delete `feat/media-processor` branch

---

## Commit Sequence

```
1.  feat(media): add NIKA-251..256 media error types
2.  feat(media): add MediaRef, MediaType, and MediaBlock types
3.  feat(media): implement MIME detection with infer + mime_guess + cross-validation
4.  feat(media): implement CAS store with blake3, atomic writes, read-back verification
5.  feat(media): implement MediaProcessor pipeline
6.  feat(media): register media module in lib.rs
7.  feat(store): add media field to TaskResult + resolve_path() media support
8.  feat(event): add MediaProcessed, MediaStored, MediaStoreFailed events
9.  feat(runtime): wire MediaProcessor into invoke verb, remove __media bridge
10. test(media): add proptest + detection + CAS + pipeline + event tests
```

---

## Corrections from v1 + v2

| Item | v1/v2 | v3 (correct) |
|------|-------|-------------|
| CAS store verification | No read-back | **Added** read-back re-hash after write (Layer 3) |
| Decode verification | Not mentioned | **Added** empty-output-from-non-empty-input check (Layer 2) |
| MIME cross-validation | Not mentioned | **Added** server MIME vs detected MIME comparison with warning |
| detect_mime signature | `(data, ext_hint)` | **Added** `server_mime` param for cross-validation |
| CAS store.store() return | `PathBuf` | **Changed** to `StoreResult { path, deduplicated, verified }` |
| Event variants | MediaStored + MediaCleanup (2) | **Changed** to MediaProcessed + MediaStored + MediaStoreFailed (3) |
| MediaCleanup event | In PR2 | **Moved** to PR3 (belongs with CLI clean command) |
| EventKind task_id() | Not mentioned | **Added** explicit requirement to update match arm for all 3 variants |
| CAS list on empty dir | Would panic | **Fixed** with root.exists() guard |
| Event variant count | 32 | **Updated**: 33 (PR1) -> 36 (PR2) |
| Error codes | NIKA-250..255 | **Shifted** to NIKA-251..256 (NIKA-250 taken by ContextLoadError) |
| resolve_path() | No media support | **Extended** to intercept "media" segment and resolve TaskResult.media |
| Template access | Direct `{{task.media}}` | **Via `with:` bindings** (consistent with Nika template architecture) |
| process_block match | Only "image" arm | **Fixed v3.1**: `"image" \| "audio"` — audio has identical shape (data + mime_type), was silently dropped by `_ => Ok(None)` |

---

## Codebase Discrepancies (v3 → v3.1, verified by deep audit agent)

Critical findings from deep codebase verification that MUST be accounted for during implementation:

| # | Finding | Impact | How to Handle |
|:-:|---------|--------|---------------|
| 1 | `io::atomic::write_atomic()` does NOT call `create_dir_all()` | CAS writes to new prefix dirs will fail with NotFound | Plan already adds `create_dir_all` before `write_atomic` in `store.rs` ✓ |
| 2 | `jsonpath::resolve()` returns `Result<Option<Value>, NikaError>`, not `Option<Value>` | Plan code using `.ok().flatten()` is correct ✓ | No change needed — plan already handles this correctly |
| 3 | `run_invoke()` returns `Ok(String)` — the `serde_json::Value` is internal (before line 980) | MediaProcessor must intercept BEFORE `Ok(result.to_string())` at line 980 | Task 9 integration point must be before the final `to_string()` call |
| 4 | `NikaError::code()` match is **exhaustive** (lines 598-706) | Every new NikaError variant must have a `code()` match arm or it won't compile | Task 1: Add `code()` arm for `MediaError` variant in error.rs |
| 5 | `NikaError::is_recoverable()` uses `matches!` macro (lines 709-722) | New variants should be added if retryable (media store write could be transient) | Task 1: Add `MediaStoreWrite` to recoverable list |
| 6 | `task_id` in `run_invoke()` is `&Arc<str>`, not `&str` or `String` | `.clone()` on `Arc<str>` is cheap (ref count bump) — plan code is correct ✓ | No change needed |
| 7 | `resolve_path()` uses `splitn(2, '.')` in codebase, plan uses `split_once('.')` | Functionally identical — `split_once` is cleaner | Plan can use `split_once` — it works the same way |
| 8 | CLI dispatch uses `cli::module::handle_command` pattern with nested subcommands | PR3's `nika media` command must follow `cli::media::MediaAction` convention | PR3 Task 5 must create `src/cli/media.rs` with `handle_media_command()` |
| 9 | `NikaError` uses **both** `thiserror` and `miette` (`#[derive(Error, Debug, Diagnostic)]`) | `MediaError` must also derive both, and the `From` impl must produce a variant with `#[diagnostic]` | Task 1: MediaError already derives both ✓ |
| 10 | Runner stores results via `self.datastore.insert(Arc::clone(&store_id), task_result)` | `with_media()` must be called before `insert()` — the attach point is in runner.rs, not verbs.rs | Task 9: Clarify that media attachment flows through `TaskResult::with_media()` after `run_invoke` returns |

### Implementation Notes from Deep Verify

**Task 1 addition** — Add to `NikaError::code()`:
```rust
Self::MediaError { source } => source.code(), // delegate to MediaError's own code method
```

**Task 9 clarification** — Integration point in `run_invoke()`:
```rust
// Line ~980 of verbs.rs -- BEFORE Ok(result.to_string()):
// 1. Check if tool_result has media (via has_media())
// 2. Run MediaProcessor on content blocks
// 3. Return (text_output_string, media_refs) tuple
// 4. Caller in runner.rs attaches media via .with_media()
```

**CAS write pattern** — Consider `persist_noclobber` for race safety:
```rust
// Instead of: check exists → write_atomic
// Use: write to NamedTempFile → persist_noclobber (race-safe)
// AlreadyExists error = another writer stored it first = dedup success
```
