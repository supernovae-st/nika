// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Content-Addressable Storage (CAS) with blake3 hashing and io::atomic writes.
//!
//! Layout: `{root}/{hash[0..2]}/{hash[2..]}` (NO extension in filename).
//! Hashes are prefixed: "blake3:af1349..." for algorithm identification.
//! Uses `crate::io::atomic::write_fail()` for atomic CAS writes (O_EXCL).

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::error::MediaError;

/// blake3 hash algorithm prefix for MediaRef.hash values.
const HASH_PREFIX: &str = "blake3:";

/// Only verify files at or above this size (1MB).
const VERIFY_THRESHOLD: u64 = 1024 * 1024;

/// Maximum raw data size accepted by CAS store (100MB, defense-in-depth).
const MAX_STORE_SIZE: usize = 100 * 1024 * 1024;

/// Zstd magic bytes (for reference only — we use CAS_ZSTD_MARKER for detection).
#[cfg(feature = "media-compression")]
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

/// CAS-internal 4-byte magic prefix for framed blobs.
///
/// Layout: `b"NK"` + compression flag (1 byte) + version (1 byte, currently 0x00).
///
/// Using a multi-byte magic prevents legacy blobs (pre-framing) from being
/// misinterpreted: no real file format starts with exactly `NK\x00` or `NK\x01`.
///
/// - Compressed:   `[N][K][0x01][0x00]` + zstd data
/// - Uncompressed: `[N][K][0x00][0x00]` + raw data
/// - Legacy:       anything not starting with `b"NK"` -- returned as-is.
#[cfg(feature = "media-compression")]
const CAS_MAGIC: &[u8; 2] = b"NK";

/// Compression flag byte at offset 2 of the framing header: zstd-compressed.
#[cfg(feature = "media-compression")]
const CAS_FLAG_ZSTD: u8 = 0x01;

/// Compression flag byte at offset 2 of the framing header: raw/uncompressed.
#[cfg(feature = "media-compression")]
const CAS_FLAG_RAW: u8 = 0x00;

/// Framing version byte at offset 3 (currently always 0x00).
#[cfg(feature = "media-compression")]
const CAS_FRAMING_VERSION: u8 = 0x00;

/// Length of the CAS framing header in bytes.
#[cfg(feature = "media-compression")]
const CAS_HEADER_LEN: usize = 4;

/// Zstd compression level (3 = optimal speed/ratio for CAS workloads).
#[cfg(feature = "media-compression")]
const ZSTD_LEVEL: i32 = 3;

/// Maximum decompressed size to prevent zstd decompression bombs (200MB).
#[cfg(feature = "media-compression")]
const MAX_DECOMPRESS_SIZE: u64 = 200 * 1024 * 1024;

/// Result of a CAS store operation.
#[derive(Debug, Clone)]
pub struct StoreResult {
    /// Algorithm-prefixed hash (e.g., "blake3:af1349...")
    pub hash: String,

    /// Final path where the file is stored
    pub path: PathBuf,

    /// File size in bytes
    pub size: u64,

    /// True if this file was already in the store (deduplicated)
    pub deduplicated: bool,

    /// True if read-back verification passed (or skipped for small files)
    pub verified: bool,

    /// Pipeline latency in milliseconds (hash -> store -> verify)
    pub pipeline_ms: u64,
}

/// Entry in the CAS store.
#[derive(Debug, Clone)]
pub struct CasEntry {
    /// Algorithm-prefixed hash (e.g., "blake3:af1349...")
    pub hash: String,

    /// File path
    pub path: PathBuf,

    /// File size in bytes
    pub size: u64,
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
/// All write operations are async, using `crate::io::atomic::write_fail()`
/// which provides O_EXCL atomic check+create via tokio::fs.
/// Filenames are hash-only (no extension).
pub struct CasStore {
    root: PathBuf,
}

/// Check if data should be compressed (skip already-compressed media formats).
///
/// Images, audio, and video are already entropy-coded — compressing them
/// wastes CPU for <2% size reduction. Text, JSON, SVG, YAML compress well.
#[cfg(feature = "media-compression")]
fn should_compress(data: &[u8]) -> bool {
    if data.len() < 64 {
        return false; // Too small to benefit from compression
    }
    // Skip if data is already zstd-compressed (prevents double-compression)
    if data.len() >= 4 && data[..4] == ZSTD_MAGIC {
        return false;
    }
    let mime = infer::get(data).map(|t| t.mime_type());
    !matches!(mime, Some(m) if m.starts_with("image/")
        || m.starts_with("audio/")
        || m.starts_with("video/")
        || m == "application/zip"
        || m == "application/gzip"
        || m == "application/x-bzip2"
        || m == "application/x-xz"
    )
}

/// Compress data with zstd if beneficial.
///
/// Always returns framed data with a 4-byte header:
/// - Compressed: `[N][K][0x01][0x00][zstd-data...]` when compression saves space
/// - Uncompressed: `[N][K][0x00][0x00][raw-data...]` when compression is not beneficial
///
/// The 4-byte magic header eliminates false-positive decompression: on read,
/// `transparent_decompress` checks for the `b"NK"` prefix deterministically instead
/// of pattern-matching against zstd magic bytes in user data.
#[cfg(feature = "media-compression")]
fn compress_if_beneficial(data: &[u8]) -> Vec<u8> {
    match zstd::encode_all(std::io::Cursor::new(data), ZSTD_LEVEL) {
        Ok(compressed) if compressed.len() + CAS_HEADER_LEN < data.len() => {
            // Prefix with CAS 4-byte header to distinguish from raw zstd user data
            let mut framed = Vec::with_capacity(CAS_HEADER_LEN + compressed.len());
            framed.extend_from_slice(CAS_MAGIC);
            framed.push(CAS_FLAG_ZSTD);
            framed.push(CAS_FRAMING_VERSION);
            framed.extend_from_slice(&compressed);
            framed
        }
        _ => {
            // Compression didn't help or failed — store with raw header
            let mut framed = Vec::with_capacity(CAS_HEADER_LEN + data.len());
            framed.extend_from_slice(CAS_MAGIC);
            framed.push(CAS_FLAG_RAW);
            framed.push(CAS_FRAMING_VERSION);
            framed.extend_from_slice(data);
            framed
        }
    }
}

/// Prepend the 4-byte raw header to data.
///
/// Used when `should_compress` returns false (media formats, small data).
/// Ensures every blob stored under `media-compression` has a deterministic
/// 4-byte header, eliminating false-positive decompression.
#[cfg(feature = "media-compression")]
fn frame_uncompressed(data: &[u8]) -> Vec<u8> {
    let mut framed = Vec::with_capacity(CAS_HEADER_LEN + data.len());
    framed.extend_from_slice(CAS_MAGIC);
    framed.push(CAS_FLAG_RAW);
    framed.push(CAS_FRAMING_VERSION);
    framed.extend_from_slice(data);
    framed
}

/// Transparently decompress data based on CAS 4-byte framing header.
///
/// Three cases:
/// 1. **Framed compressed** (`NK\x01\x00`): Strip header, decompress zstd payload.
/// 2. **Framed uncompressed** (`NK\x00\x00`): Strip header, return raw bytes.
/// 3. **Legacy** (no `NK` prefix): No framing header — return data as-is
///    for backward compatibility with blobs written before framing was added.
#[cfg(feature = "media-compression")]
fn transparent_decompress(data: Vec<u8>) -> Result<Vec<u8>, MediaError> {
    if data.len() < CAS_HEADER_LEN || &data[..2] != CAS_MAGIC {
        // Legacy blob or empty — return as-is for backward compat
        return Ok(data);
    }

    let flag = data[2];

    match flag {
        CAS_FLAG_ZSTD => {
            // Framed compressed: [N][K][0x01][0x00][zstd-data...]
            let zstd_data = &data[CAS_HEADER_LEN..];
            let cursor = std::io::Cursor::new(zstd_data);
            let mut decoder = zstd::Decoder::new(cursor).map_err(|e| MediaError::MediaStoreIo {
                path: PathBuf::from("<zstd-decompress>"),
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
            })?;

            let mut output = Vec::new();
            let mut limited = std::io::Read::take(&mut decoder, MAX_DECOMPRESS_SIZE);
            std::io::Read::read_to_end(&mut limited, &mut output).map_err(|e| {
                MediaError::MediaStoreIo {
                    path: PathBuf::from("<zstd-decompress>"),
                    source: e,
                }
            })?;

            // SECURITY: detect if decompression was truncated (decompression bomb)
            let mut probe = [0u8; 1];
            if std::io::Read::read(&mut decoder, &mut probe).unwrap_or(0) > 0 {
                return Err(MediaError::Base64InputTooLarge {
                    size: MAX_DECOMPRESS_SIZE as usize + 1,
                    max: MAX_DECOMPRESS_SIZE as usize,
                });
            }

            Ok(output)
        }
        CAS_FLAG_RAW => {
            // Framed uncompressed: [N][K][0x00][0x00][raw-data...]
            Ok(data[CAS_HEADER_LEN..].to_vec())
        }
        _ => {
            // Unknown flag — strip header, return data (forward compat)
            tracing::warn!(flag, "Unknown CAS framing flag, stripping header");
            Ok(data[CAS_HEADER_LEN..].to_vec())
        }
    }
}

impl CasStore {
    /// Create a CAS store at the given root directory.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Create a CAS store at the workspace default location.
    ///
    /// Respects `NIKA_MEDIA_STORE` env var as override (canonicalized).
    /// Otherwise uses `{workspace_root}/.nika/media/store/`.
    pub fn workspace_default(workspace_root: &Path) -> Self {
        if let Ok(override_path) = std::env::var("NIKA_MEDIA_STORE") {
            let path = PathBuf::from(&override_path);
            // Canonicalize if the path exists (resolves symlinks, `..`, etc.).
            // If it doesn't exist yet, use the raw path -- store() will create it.
            let resolved = path.canonicalize().unwrap_or(path);
            return Self::new(resolved);
        }
        Self::new(workspace_root.join(".nika").join("media").join("store"))
    }

    /// The root directory of this CAS store.
    ///
    /// Used by the runner for lockfile placement and by the CLI for GC checks.
    /// The lockfile MUST be placed inside the actual store root so that
    /// `NIKA_MEDIA_STORE` overrides are respected.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Store binary data in the CAS (async).
    ///
    /// 1. Compute blake3 hash
    /// 2. Derive path: `{root}/{hash[0..2]}/{hash[2..]}` (no extension)
    /// 3. Create parent directories (async)
    /// 4. Write via `io::atomic::write_fail()` (O_EXCL atomic check+create)
    /// 5. If AlreadyExists, return deduplicated=true
    /// 6. Read-back verify only for files >= VERIFY_THRESHOLD
    pub async fn store(&self, data: &[u8]) -> Result<StoreResult, MediaError> {
        // Defense-in-depth: reject empty data at the CAS layer (D28)
        if data.is_empty() {
            return Err(MediaError::EmptyMediaContent {
                task_id: "(cas-direct)".to_string(),
            });
        }

        // Defense-in-depth: reject oversized data at the CAS layer (D23)
        if data.len() > MAX_STORE_SIZE {
            return Err(MediaError::Base64InputTooLarge {
                size: data.len(),
                max: MAX_STORE_SIZE,
            });
        }

        let process_start = std::time::Instant::now();

        // Hash ORIGINAL data before any compression (preserves dedup semantics)
        let raw_hash = blake3::hash(data).to_hex().to_string();
        let size = data.len() as u64;

        let prefixed_hash = format!("{HASH_PREFIX}{raw_hash}");

        let dir = self.root.join(&raw_hash[..2]);
        let final_path = dir.join(&raw_hash[2..]);

        // Optionally compress non-media data for storage efficiency.
        // With media-compression enabled, ALL data gets a 4-byte framing header:
        // - Compressible data: [NK][0x01][0x00][zstd-data...] or [NK][0x00][0x00][raw-data...]
        // - Non-compressible data (media/small): [NK][0x00][0x00][raw-data...]
        // This eliminates false-positive decompression of user data that
        // happens to start with bytes that collide with the old 1-byte markers.
        #[cfg(feature = "media-compression")]
        let framed;
        #[cfg(feature = "media-compression")]
        let write_data: &[u8] = if should_compress(data) {
            framed = compress_if_beneficial(data);
            &framed
        } else {
            // Not compressible (media/small) — still add framing byte
            framed = frame_uncompressed(data);
            &framed
        };
        #[cfg(not(feature = "media-compression"))]
        let write_data: &[u8] = data;

        // Create parent directories (async)
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| MediaError::MediaStoreIo {
                path: dir.clone(),
                source: e,
            })?;

        // Atomic write via O_EXCL (create_new). On success: new file.
        // On AlreadyExists: dedup hit. On other error: clean up partial file.
        match write_fail_if_exists(&final_path, write_data).await {
            Ok(()) => {
                // New file stored successfully
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // Content-addressed: same hash = same content, skip write.
                return Ok(StoreResult {
                    hash: prefixed_hash,
                    path: final_path,
                    size,
                    deduplicated: true,
                    verified: false,
                    pipeline_ms: process_start.elapsed().as_millis() as u64,
                });
            }
            Err(e) => {
                // CRITICAL: write_fail may leave a partial file on disk-full/IO error.
                // Clean it up to prevent permanent CAS corruption.
                let _ = tokio::fs::remove_file(&final_path).await;
                return Err(MediaError::MediaStoreIo {
                    path: final_path,
                    source: e,
                });
            }
        }

        // Read-back verification only for files >= 1MB (original size)
        // Small files: fsync guarantees integrity, verified=false to indicate skipped
        let verified = if size >= VERIFY_THRESHOLD {
            let stored =
                tokio::fs::read(&final_path)
                    .await
                    .map_err(|e| MediaError::MediaStoreIo {
                        path: final_path.clone(),
                        source: e,
                    })?;
            // Decompress if needed before verifying hash (hash is of original data)
            #[cfg(feature = "media-compression")]
            let stored = transparent_decompress(stored)?;
            let verify_hash = blake3::hash(&stored).to_hex().to_string();
            if verify_hash != raw_hash {
                let _ = tokio::fs::remove_file(&final_path).await;
                return Err(MediaError::HashMismatch {
                    expected: prefixed_hash,
                    actual: format!("{HASH_PREFIX}{verify_hash}"),
                });
            }
            true
        } else {
            false // small file: verification skipped (fsync sufficient)
        };

        Ok(StoreResult {
            hash: prefixed_hash,
            path: final_path,
            size,
            deduplicated: false,
            verified,
            pipeline_ms: process_start.elapsed().as_millis() as u64,
        })
    }

    /// Check if a hash exists in the store.
    pub fn exists(&self, hash: &str) -> bool {
        let raw = strip_hash_prefix(hash);
        if raw.len() < 3 || validate_hash_hex(raw).is_err() {
            return false;
        }
        let path = self.root.join(&raw[..2]).join(&raw[2..]);
        path.exists()
    }

    /// Read file data by hash (async).
    ///
    /// Transparently decompresses zstd-compressed blobs when the
    /// `media-compression` feature is enabled.
    pub async fn read(&self, hash: &str) -> Result<Vec<u8>, MediaError> {
        let raw = strip_hash_prefix(hash);
        if raw.len() < 3 {
            return Err(MediaError::MediaNotFound {
                hash: hash.to_string(),
            });
        }
        // SECURITY: validate hex-only to prevent path traversal
        validate_hash_hex(raw)?;
        let path = self.root.join(&raw[..2]).join(&raw[2..]);
        let data = tokio::fs::read(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                MediaError::MediaNotFound {
                    hash: hash.to_string(),
                }
            } else {
                MediaError::MediaStoreIo { path, source: e }
            }
        })?;

        // Transparent decompression when media-compression is enabled
        #[cfg(feature = "media-compression")]
        let data = transparent_decompress(data)?;

        Ok(data)
    }

    /// Read a CAS file directly by path, with transparent decompression.
    ///
    /// Unlike `read()` which takes a hash and resolves the path internally,
    /// this method reads from a known CAS file path. Used by the artifact
    /// writer to produce decompressed output files (Bug 21 fix).
    ///
    /// Returns the original user data with any CAS framing/compression stripped.
    pub async fn read_raw(path: &Path) -> Result<Vec<u8>, MediaError> {
        let data = tokio::fs::read(path)
            .await
            .map_err(|e| MediaError::MediaStoreIo {
                path: path.to_path_buf(),
                source: e,
            })?;

        // Transparent decompression when media-compression is enabled
        #[cfg(feature = "media-compression")]
        let data = transparent_decompress(data)?;

        Ok(data)
    }

    /// List all entries in the store.
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
                let raw_hash = format!("{}{}", shard_name, file_name);
                let size = file.metadata().map(|m| m.len()).unwrap_or(0);
                entries.push(CasEntry {
                    hash: format!("{HASH_PREFIX}{raw_hash}"),
                    path: file.path(),
                    size,
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

/// Strip the algorithm prefix from a hash string, if present.
/// Also validates that the remainder is hex-only to prevent path traversal.
fn strip_hash_prefix(hash: &str) -> &str {
    hash.strip_prefix(HASH_PREFIX).unwrap_or(hash)
}

/// Validate that a raw hash string contains only hex characters.
/// Prevents path traversal via crafted hash strings like "../../etc/passwd".
fn validate_hash_hex(raw: &str) -> Result<(), MediaError> {
    if !raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(MediaError::MediaNotFound {
            hash: format!("{HASH_PREFIX}{raw}"),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[tokio::test]
    async fn store_and_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let data = b"hello media pipeline";
        let result = store.store(data).await.unwrap();

        assert!(result.hash.starts_with("blake3:"));
        assert!(!result.deduplicated);
        // Small files: verified=false (read-back skipped, fsync sufficient)
        assert!(!result.verified);
        assert_eq!(result.size, data.len() as u64);

        let read_back = store.read(&result.hash).await.unwrap();
        assert_eq!(read_back, data);
    }

    #[tokio::test]
    async fn store_dedup_same_content() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let data = b"identical content";
        let r1 = store.store(data).await.unwrap();
        let r2 = store.store(data).await.unwrap();

        assert_eq!(r1.hash, r2.hash);
        assert!(!r1.deduplicated);
        assert!(r2.deduplicated);
    }

    #[tokio::test]
    async fn exists_after_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let data = b"existence check";
        let result = store.store(data).await.unwrap();

        assert!(store.exists(&result.hash));
        assert!(!store.exists("blake3:nonexistent"));
    }

    #[tokio::test]
    async fn read_nonexistent_hash_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let result = store
            .read("blake3:abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890")
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn hash_only_filename_no_extension() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let data = b"no extension in path";
        let result = store.store(data).await.unwrap();

        // Path should have NO extension
        assert!(
            result.path.extension().is_none(),
            "CAS path should have no extension: {:?}",
            result.path
        );
    }

    #[tokio::test]
    async fn hash_has_blake3_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let result = store.store(b"prefix test").await.unwrap();
        assert!(
            result.hash.starts_with("blake3:"),
            "hash should have blake3: prefix, got: {}",
            result.hash
        );
    }

    #[tokio::test]
    async fn list_returns_stored_entries() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        store.store(b"file one").await.unwrap();
        store.store(b"file two").await.unwrap();

        let entries = store.list();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.hash.starts_with("blake3:")));
    }

    #[tokio::test]
    async fn clean_all_removes_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        store.store(b"data one").await.unwrap();
        store.store(b"data two").await.unwrap();

        let clean = store.clean_all();
        assert_eq!(clean.removed, 2);
        assert_eq!(store.list().len(), 0);
    }

    #[test]
    fn workspace_default_uses_workspace_root() {
        let root = std::path::PathBuf::from("/tmp/test-workspace");
        let store = CasStore::workspace_default(&root);
        // Just verify it constructs without panic
        assert!(!store.exists("blake3:nonexistent"));
    }

    #[tokio::test]
    async fn store_rejects_empty_data() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let result = store.store(b"").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), "NIKA-258");
    }

    #[tokio::test]
    async fn store_rejects_oversized_data() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let big = vec![0u8; MAX_STORE_SIZE + 1];
        let result = store.store(&big).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), "NIKA-257");
    }

    #[tokio::test]
    async fn store_accepts_exactly_max_store_size() {
        // Boundary test: exactly MAX_STORE_SIZE bytes should be accepted
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let data = vec![0xAB_u8; MAX_STORE_SIZE];
        let result = store.store(&data).await;
        assert!(
            result.is_ok(),
            "exactly MAX_STORE_SIZE should be accepted, got: {:?}",
            result.err()
        );
        let sr = result.unwrap();
        assert_eq!(sr.size, MAX_STORE_SIZE as u64);
        assert!(!sr.deduplicated);
        // 100MB is above verify threshold (1MB), so it should be verified
        assert!(
            sr.verified,
            "100MB file should trigger read-back verification"
        );

        // Verify read-back matches
        let read_back = store.read(&sr.hash).await.unwrap();
        assert_eq!(read_back.len(), MAX_STORE_SIZE);
        assert!(
            read_back.iter().all(|&b| b == 0xAB),
            "data corruption: not all bytes are 0xAB"
        );
    }

    #[tokio::test]
    async fn concurrent_cas_writes_dedup_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let store = std::sync::Arc::new(CasStore::new(dir.path()));

        let data: Vec<u8> = b"identical content for all tasks".to_vec();

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let store = std::sync::Arc::clone(&store);
                let data = data.clone();
                tokio::spawn(async move { store.store(&data).await })
            })
            .collect();

        let results: Vec<StoreResult> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|h| h.unwrap().unwrap())
            .collect();

        // All should have the same hash
        let hash = &results[0].hash;
        assert!(hash.starts_with("blake3:"));
        assert!(results.iter().all(|r| &r.hash == hash));

        // Exactly one should be non-deduplicated
        let non_dedup_count = results.iter().filter(|r| !r.deduplicated).count();
        assert_eq!(non_dedup_count, 1, "exactly one writer should be non-dedup");
    }

    // ═══════════════════════════════════════════════════════════════
    // NIKA_MEDIA_STORE env var + root() accessor + path validation
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn root_accessor_returns_store_root() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        assert_eq!(store.root(), dir.path());
    }

    #[test]
    #[serial]
    fn workspace_default_without_env_uses_nika_media_store_path() {
        // Ensure NIKA_MEDIA_STORE is NOT set for this test
        let saved = std::env::var("NIKA_MEDIA_STORE").ok();
        std::env::remove_var("NIKA_MEDIA_STORE");

        let workspace = PathBuf::from("/tmp/test-workspace");
        let store = CasStore::workspace_default(&workspace);
        let expected = workspace.join(".nika").join("media").join("store");
        assert_eq!(store.root(), expected.as_path());

        // Restore env var if it was set
        if let Some(val) = saved {
            std::env::set_var("NIKA_MEDIA_STORE", val);
        }
    }

    #[test]
    #[serial]
    fn workspace_default_respects_nika_media_store_env() {
        let dir = tempfile::tempdir().unwrap();
        let override_path = dir.path().join("custom-store");

        let saved = std::env::var("NIKA_MEDIA_STORE").ok();
        std::env::set_var("NIKA_MEDIA_STORE", override_path.to_str().unwrap());

        let store = CasStore::workspace_default(Path::new("/ignored/workspace"));

        // The override path doesn't exist yet, so canonicalize falls back to raw path
        assert_eq!(store.root(), override_path.as_path());

        match saved {
            Some(val) => std::env::set_var("NIKA_MEDIA_STORE", val),
            None => std::env::remove_var("NIKA_MEDIA_STORE"),
        }
    }

    #[test]
    #[serial]
    fn workspace_default_canonicalizes_existing_override_path() {
        let dir = tempfile::tempdir().unwrap();
        // Create a subdirectory so canonicalize has something to resolve
        let actual = dir.path().join("store");
        std::fs::create_dir_all(&actual).unwrap();

        // Construct a path with `..` that resolves to the same place
        let dotdot_path = dir.path().join("store").join("..").join("store");

        let saved = std::env::var("NIKA_MEDIA_STORE").ok();
        std::env::set_var("NIKA_MEDIA_STORE", dotdot_path.to_str().unwrap());

        let store = CasStore::workspace_default(Path::new("/ignored"));

        // After canonicalization, the `..` should be resolved
        let resolved = store.root().to_path_buf();
        assert!(
            !resolved.to_str().unwrap().contains(".."),
            "path should be canonicalized, got: {}",
            resolved.display()
        );
        // The canonical paths should match
        assert_eq!(
            resolved.canonicalize().unwrap(),
            actual.canonicalize().unwrap(),
            "resolved path should point to the same directory"
        );

        match saved {
            Some(val) => std::env::set_var("NIKA_MEDIA_STORE", val),
            None => std::env::remove_var("NIKA_MEDIA_STORE"),
        }
    }

    #[tokio::test]
    #[serial]
    async fn env_override_store_is_fully_functional() {
        let dir = tempfile::tempdir().unwrap();
        let override_path = dir.path().join("custom-cas");

        let saved = std::env::var("NIKA_MEDIA_STORE").ok();
        std::env::set_var("NIKA_MEDIA_STORE", override_path.to_str().unwrap());

        let store = CasStore::workspace_default(Path::new("/ignored/workspace"));

        // Store data -- should create directories inside the custom path
        let data = b"env override test data";
        let result = store.store(data).await.unwrap();

        assert!(result.hash.starts_with("blake3:"));
        assert!(
            result.path.starts_with(&override_path),
            "stored file should be inside override path, got: {}",
            result.path.display()
        );

        // Read back
        let read_back = store.read(&result.hash).await.unwrap();
        assert_eq!(read_back, data);

        // List
        let entries = store.list();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].path.starts_with(&override_path));

        // Exists
        assert!(store.exists(&result.hash));

        // Clean
        let clean = store.clean_all();
        assert_eq!(clean.removed, 1);
        assert_eq!(store.list().len(), 0);

        match saved {
            Some(val) => std::env::set_var("NIKA_MEDIA_STORE", val),
            None => std::env::remove_var("NIKA_MEDIA_STORE"),
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // SECURITY: CAS hash-to-path safety tests
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn cas_path_is_always_within_root() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let payloads: Vec<&[u8]> = vec![
            b"payload one",
            b"payload two",
            b"\x00\x01\x02\xff\xfe\xfd",
            b"../../../etc/passwd",
        ];

        let canonical_root = dir.path().canonicalize().unwrap();

        for payload in payloads {
            let result = store.store(payload).await.unwrap();

            let canonical_path = result.path.canonicalize().unwrap();
            assert!(
                canonical_path.starts_with(&canonical_root),
                "CAS file {:?} escapes root {:?}",
                canonical_path,
                canonical_root,
            );

            let shard = result
                .path
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_string_lossy();
            let filename = result.path.file_name().unwrap().to_string_lossy();

            assert_eq!(shard.len(), 2, "Shard directory must be 2 hex chars");
            assert!(
                shard.chars().all(|c| c.is_ascii_hexdigit()),
                "Shard '{}' contains non-hex chars",
                shard
            );
            assert!(
                filename.chars().all(|c| c.is_ascii_hexdigit()),
                "Filename '{}' contains non-hex chars",
                filename
            );
        }
    }

    #[test]
    fn cas_hash_prefix_strip_safety() {
        assert_eq!(strip_hash_prefix("blake3:abcdef"), "abcdef");
        assert_eq!(strip_hash_prefix("abcdef"), "abcdef");
        // Theoretical adversarial input -- in practice blake3 only outputs hex
        assert_eq!(strip_hash_prefix("blake3:../../etc"), "../../etc");
    }

    #[test]
    fn cas_exists_rejects_short_hash() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        assert!(!store.exists("ab"));
        assert!(!store.exists("a"));
        assert!(!store.exists(""));
        assert!(!store.exists("blake3:ab"));
        assert!(!store.exists("blake3:a"));
    }

    #[tokio::test]
    async fn cas_read_rejects_short_hash() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let result = store.read("ab").await;
        assert!(result.is_err());
        let result = store.read("blake3:ab").await;
        assert!(result.is_err());
    }

    // ═══════════════════════════════════════════════════════════════
    // ZSTD COMPRESSION TESTS (media-compression feature)
    // ═══════════════════════════════════════════════════════════════

    #[cfg(feature = "media-compression")]
    mod compression_tests {
        use super::*;

        #[tokio::test]
        async fn store_read_json_roundtrip_with_compression() {
            let dir = tempfile::tempdir().unwrap();
            let store = CasStore::new(dir.path());

            let json = br#"{"name":"test","items":[1,2,3,4,5],"nested":{"a":"b"}}"#;
            let result = store.store(json).await.unwrap();
            let read_back = store.read(&result.hash).await.unwrap();
            assert_eq!(
                read_back, json,
                "JSON round-trip must preserve data exactly"
            );
        }

        #[tokio::test]
        async fn store_png_passes_through_uncompressed() {
            let dir = tempfile::tempdir().unwrap();
            let store = CasStore::new(dir.path());

            // PNG magic bytes — should NOT be compressed
            let mut png_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
            png_data.extend_from_slice(&[0u8; 100]); // pad to make it meaningful

            let result = store.store(&png_data).await.unwrap();

            // Read raw on-disk data (bypass transparent decompress)
            let raw = strip_hash_prefix(&result.hash);
            let path = dir.path().join(&raw[..2]).join(&raw[2..]);
            let on_disk = tokio::fs::read(&path).await.unwrap();

            // PNG should have 4-byte raw header (not compressed, but framed)
            assert_eq!(&on_disk[..2], CAS_MAGIC, "PNG should have NK magic prefix");
            assert_eq!(
                on_disk[2], CAS_FLAG_RAW,
                "PNG should have raw flag, not zstd"
            );
            assert_eq!(
                on_disk[3], CAS_FRAMING_VERSION,
                "PNG should have version 0x00"
            );
            // After the 4-byte header, the original PNG data follows
            assert_eq!(
                &on_disk[CAS_HEADER_LEN..],
                &png_data[..],
                "PNG data should follow the 4-byte framing header"
            );

            // Read-back should still work (strips marker)
            let read_back = store.read(&result.hash).await.unwrap();
            assert_eq!(read_back, png_data);
        }

        #[tokio::test]
        async fn store_text_is_compressed_on_disk() {
            let dir = tempfile::tempdir().unwrap();
            let store = CasStore::new(dir.path());

            // Highly compressible text (repeated pattern)
            let text: Vec<u8> = "hello world! ".repeat(100).into_bytes();
            let result = store.store(&text).await.unwrap();

            // Read raw on-disk data
            let raw = strip_hash_prefix(&result.hash);
            let path = dir.path().join(&raw[..2]).join(&raw[2..]);
            let on_disk = tokio::fs::read(&path).await.unwrap();

            // Should be CAS-compressed (4-byte header + zstd magic)
            assert_eq!(&on_disk[..2], CAS_MAGIC, "should have NK magic prefix");
            assert_eq!(on_disk[2], CAS_FLAG_ZSTD, "should have zstd flag");
            assert_eq!(on_disk[3], CAS_FRAMING_VERSION, "should have version 0x00");
            assert_eq!(
                &on_disk[CAS_HEADER_LEN..CAS_HEADER_LEN + 4],
                &ZSTD_MAGIC,
                "text should be zstd-compressed after 4-byte header"
            );
            assert!(on_disk.len() < text.len(), "compressed should be smaller");

            // Transparent read should return original
            let read_back = store.read(&result.hash).await.unwrap();
            assert_eq!(read_back, text);
        }

        #[tokio::test]
        async fn dedup_works_with_compression() {
            let dir = tempfile::tempdir().unwrap();
            let store = CasStore::new(dir.path());

            let data = b"deduplicate me please".repeat(10);
            let r1 = store.store(&data).await.unwrap();
            let r2 = store.store(&data).await.unwrap();

            assert_eq!(r1.hash, r2.hash, "same content must produce same hash");
            assert!(!r1.deduplicated);
            assert!(r2.deduplicated, "second store should detect dedup");
        }

        #[tokio::test]
        async fn budget_charged_on_original_size() {
            let dir = tempfile::tempdir().unwrap();
            let store = CasStore::new(dir.path());

            let text: Vec<u8> = "budget test ".repeat(100).into_bytes();
            let original_size = text.len() as u64;
            let result = store.store(&text).await.unwrap();

            // StoreResult.size should reflect ORIGINAL size, not compressed
            assert_eq!(
                result.size, original_size,
                "size should be original data length"
            );
        }

        #[tokio::test]
        async fn already_zstd_data_roundtrips_correctly() {
            let dir = tempfile::tempdir().unwrap();
            let store = CasStore::new(dir.path());

            // Pre-compress some data with zstd (simulating user-stored zstd files)
            let original = b"pre-compressed data content here that is long enough to be over sixty-four bytes for threshold!";
            let pre_compressed =
                zstd::encode_all(std::io::Cursor::new(original.as_slice()), ZSTD_LEVEL).unwrap();
            assert!(pre_compressed.len() >= 4 && pre_compressed[..4] == ZSTD_MAGIC);

            // should_compress should detect zstd magic and skip
            assert!(
                !should_compress(&pre_compressed),
                "zstd data should not be re-compressed"
            );

            // Store and read back — user-stored zstd is stored with
            // [NK][0x00][0x00] raw header (should_compress returns false for
            // data starting with zstd magic). On read, the 4-byte header is
            // stripped and original data is returned.
            let result = store.store(&pre_compressed).await.unwrap();
            let read_back = store.read(&result.hash).await.unwrap();

            assert_eq!(
                read_back, pre_compressed,
                "user-stored zstd data should round-trip exactly"
            );
        }

        #[tokio::test]
        async fn concurrent_compressed_writes() {
            let dir = tempfile::tempdir().unwrap();
            let store = std::sync::Arc::new(CasStore::new(dir.path()));

            let data: Vec<u8> = "concurrent compression test ".repeat(50).into_bytes();
            let handles: Vec<_> = (0..5)
                .map(|_| {
                    let store = std::sync::Arc::clone(&store);
                    let data = data.clone();
                    tokio::spawn(async move { store.store(&data).await })
                })
                .collect();

            let results: Vec<StoreResult> = futures::future::join_all(handles)
                .await
                .into_iter()
                .map(|h| h.unwrap().unwrap())
                .collect();

            // All hashes must match
            let hash = &results[0].hash;
            assert!(results.iter().all(|r| &r.hash == hash));

            // Read back must be original
            let read_back = store.read(hash).await.unwrap();
            assert_eq!(read_back, data);
        }

        #[test]
        fn should_compress_text_yes() {
            // Plain text ≥ 64 bytes (no magic bytes → infer returns None → not media)
            let text = b"hello world this is some text that should compress, adding more to be over 64 bytes for the threshold";
            assert!(text.len() >= 64, "fixture must be >= 64 bytes");
            assert!(should_compress(text));
        }

        #[test]
        fn should_compress_json_yes() {
            let json =
                br#"{"key":"value","list":[1,2,3],"nested":{"a":"b","c":"d","e":"f","g":"h"}}"#;
            assert!(json.len() >= 64, "fixture must be >= 64 bytes");
            assert!(should_compress(json));
        }

        #[test]
        fn should_compress_png_no() {
            let mut png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
            png.extend_from_slice(&[0u8; 100]);
            assert!(!should_compress(&png));
        }

        #[test]
        fn should_compress_jpeg_no() {
            let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0];
            jpeg.extend_from_slice(&[0u8; 100]);
            assert!(!should_compress(&jpeg));
        }

        #[test]
        fn should_compress_small_data_no() {
            assert!(
                !should_compress(b"tiny"),
                "data < 64 bytes should skip compression"
            );
        }

        #[test]
        fn cas_marker_framing_compressed() {
            // compress_if_beneficial prepends 4-byte header when beneficial
            let data = b"test data that is long enough to be over sixty-four bytes threshold for compression!";
            let framed = compress_if_beneficial(data);
            assert_eq!(&framed[..2], CAS_MAGIC, "must start with NK magic");
            if framed[2] == CAS_FLAG_ZSTD {
                // Compression was beneficial — should have header + zstd magic
                assert_eq!(
                    &framed[CAS_HEADER_LEN..CAS_HEADER_LEN + 4],
                    &ZSTD_MAGIC,
                    "zstd magic after header"
                );
                assert!(framed.len() < data.len(), "compressed should be smaller");
            } else {
                // Compression not beneficial — should have raw flag
                assert_eq!(framed[2], CAS_FLAG_RAW);
                assert_eq!(&framed[CAS_HEADER_LEN..], data.as_slice());
            }
        }

        #[test]
        fn cas_marker_framing_uncompressed() {
            // compress_if_beneficial returns [NK][flag][ver][raw] when compression is not beneficial
            // Use incompressible random-looking data
            let data: Vec<u8> = (0..=255).cycle().take(100).collect();
            let framed = compress_if_beneficial(&data);
            // Whether compressed or not, it must have a 4-byte header starting with NK
            assert_eq!(
                &framed[..2],
                CAS_MAGIC,
                "framed data must start with NK magic"
            );
            assert!(
                framed[2] == CAS_FLAG_ZSTD || framed[2] == CAS_FLAG_RAW,
                "framed data must have a valid compression flag"
            );
        }

        #[test]
        fn frame_uncompressed_roundtrips() {
            let data = b"hello world";
            let framed = frame_uncompressed(data);
            assert_eq!(&framed[..2], CAS_MAGIC);
            assert_eq!(framed[2], CAS_FLAG_RAW);
            assert_eq!(framed[3], CAS_FRAMING_VERSION);
            assert_eq!(&framed[CAS_HEADER_LEN..], data.as_slice());
            let decompressed = transparent_decompress(framed).unwrap();
            assert_eq!(decompressed, data);
        }

        // ═══════════════════════════════════════════════════════════
        // Bug 6 regression: false-positive decompression prevention
        // ═══════════════════════════════════════════════════════════

        #[tokio::test]
        async fn bug6_small_data_starting_with_cas_marker_and_zstd_magic() {
            // Bug 6: Data < 64 bytes that starts with [0x01][0x28][0xB5][0x2F][0xFD]
            // was falsely treated as CAS-compressed. With framing, all data stored
            // under media-compression gets a deterministic marker byte, so this
            // user data now roundtrips correctly.
            let dir = tempfile::tempdir().unwrap();
            let store = CasStore::new(dir.path());

            // Craft the exact problematic pattern: CAS_ZSTD_MARKER + ZSTD_MAGIC + payload
            let mut evil_data: Vec<u8> = vec![0x01, 0x28, 0xB5, 0x2F, 0xFD];
            evil_data.extend_from_slice(b"this is user data, not compressed!");
            assert!(evil_data.len() < 64, "must be below compression threshold");

            let result = store.store(&evil_data).await.unwrap();
            let read_back = store.read(&result.hash).await.unwrap();

            assert_eq!(
                read_back, evil_data,
                "Bug 6 regression: small data starting with [0x01][zstd-magic] \
                 must NOT be falsely decompressed"
            );
        }

        #[tokio::test]
        async fn bug6_large_data_starting_with_cas_marker_and_zstd_magic() {
            // Same as above but >= 64 bytes (compression eligible).
            // The data starts with the CAS marker + zstd magic pattern but
            // is NOT actually compressed. With framing, it roundtrips correctly.
            let dir = tempfile::tempdir().unwrap();
            let store = CasStore::new(dir.path());

            let mut evil_data: Vec<u8> = vec![0x01, 0x28, 0xB5, 0x2F, 0xFD];
            evil_data.extend_from_slice(&[0xAB; 200]); // compressible padding
            assert!(evil_data.len() >= 64, "must be above compression threshold");

            let result = store.store(&evil_data).await.unwrap();
            let read_back = store.read(&result.hash).await.unwrap();

            assert_eq!(
                read_back, evil_data,
                "Bug 6 regression: large data starting with [0x01][zstd-magic] \
                 must NOT be falsely decompressed"
            );
        }

        #[test]
        fn bug6_transparent_decompress_three_cases() {
            // Verify the three-case logic in transparent_decompress:

            // Case 1: Framed compressed (NK\x01\x00) — decompress
            let original = b"hello world! ".repeat(20);
            let compressed =
                zstd::encode_all(std::io::Cursor::new(original.as_slice()), ZSTD_LEVEL).unwrap();
            let mut framed_compressed = Vec::new();
            framed_compressed.extend_from_slice(CAS_MAGIC);
            framed_compressed.push(CAS_FLAG_ZSTD);
            framed_compressed.push(CAS_FRAMING_VERSION);
            framed_compressed.extend_from_slice(&compressed);
            let result = transparent_decompress(framed_compressed).unwrap();
            assert_eq!(result, original, "case 1: compressed must decompress");

            // Case 2: Framed uncompressed (NK\x00\x00) — strip header
            let raw_data = b"raw user data bytes";
            let mut framed_raw = Vec::new();
            framed_raw.extend_from_slice(CAS_MAGIC);
            framed_raw.push(CAS_FLAG_RAW);
            framed_raw.push(CAS_FRAMING_VERSION);
            framed_raw.extend_from_slice(raw_data);
            let result = transparent_decompress(framed_raw).unwrap();
            assert_eq!(result, raw_data, "case 2: uncompressed must strip header");

            // Case 3: Legacy (no NK prefix) — return as-is
            let legacy = vec![0x89, 0x50, 0x4E, 0x47]; // PNG magic
            let result = transparent_decompress(legacy.clone()).unwrap();
            assert_eq!(result, legacy, "case 3: legacy must return as-is");

            // Case 3b: Legacy data that starts with 0x00 or 0x01 (old single-byte markers)
            // With 4-byte magic, these are correctly treated as legacy
            let legacy_false_positive = vec![0x01, 0x28, 0xB5, 0x2F, 0xFD, 0xAA, 0xBB];
            let result = transparent_decompress(legacy_false_positive.clone()).unwrap();
            assert_eq!(
                result, legacy_false_positive,
                "case 3: data starting with 0x01 but no NK prefix — must be legacy"
            );

            // Case 3c: Data starting with 0x00 (old no-compression marker)
            let legacy_null = vec![0x00, 0x50, 0x4E, 0x47, 0xAA, 0xBB];
            let result = transparent_decompress(legacy_null.clone()).unwrap();
            assert_eq!(
                result, legacy_null,
                "case 3: data starting with 0x00 but no NK prefix — must be legacy"
            );
        }

        #[test]
        fn bug6_empty_data_decompress() {
            let result = transparent_decompress(vec![]).unwrap();
            assert!(result.is_empty(), "empty data must return empty");
        }

        #[test]
        fn bug4_legacy_blobs_with_old_single_byte_markers_not_corrupted() {
            // Bug 4: Legacy blobs starting with 0x00 or 0x01 were misinterpreted
            // as framed data with the old 1-byte marker scheme. The 4-byte NK magic
            // eliminates this: only data starting with b"NK" is treated as framed.

            // Old 0x00 marker — now treated as legacy
            let data_with_null = vec![0x00, 0xFF, 0xFE, 0xFD];
            let result = transparent_decompress(data_with_null.clone()).unwrap();
            assert_eq!(
                result, data_with_null,
                "0x00-prefixed legacy blob must not be stripped"
            );

            // Old 0x01 marker — now treated as legacy
            let data_with_one = vec![0x01, 0x28, 0xB5, 0x2F, 0xFD];
            let result = transparent_decompress(data_with_one.clone()).unwrap();
            assert_eq!(
                result, data_with_one,
                "0x01-prefixed legacy blob must not be decompressed"
            );

            // Data shorter than 4 bytes — must be legacy
            let short = vec![0x4E, 0x4B]; // "NK" but only 2 bytes
            let result = transparent_decompress(short.clone()).unwrap();
            assert_eq!(result, short, "data shorter than header must be legacy");
        }

        #[tokio::test]
        async fn bug6_read_raw_decompresses_correctly() {
            // Verify that CasStore::read_raw (used by artifact writer)
            // correctly strips framing.
            let dir = tempfile::tempdir().unwrap();
            let store = CasStore::new(dir.path());

            // Store user data that happens to start with problematic bytes
            let mut user_data: Vec<u8> = vec![0x01, 0x28, 0xB5, 0x2F, 0xFD];
            user_data.extend_from_slice(b"not actually compressed!!!");
            assert!(user_data.len() < 64);

            let result = store.store(&user_data).await.unwrap();

            // read_raw must also return original data
            let via_read_raw = CasStore::read_raw(&result.path).await.unwrap();
            assert_eq!(
                via_read_raw, user_data,
                "read_raw must strip framing and return original data"
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cas_store_uses_o_excl_prevents_symlink_attack() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let data = b"symlink attack test data";
        let raw_hash = blake3::hash(data).to_hex().to_string();
        let shard_dir = dir.path().join(&raw_hash[..2]);
        std::fs::create_dir_all(&shard_dir).unwrap();
        let final_path = shard_dir.join(&raw_hash[2..]);

        let decoy = dir.path().join("decoy");
        std::fs::write(&decoy, b"decoy content").unwrap();
        symlink(&decoy, &final_path).unwrap();

        let result = store.store(data).await.unwrap();
        assert!(
            result.deduplicated,
            "Symlink at CAS path must be treated as existing file (O_EXCL semantics)"
        );
    }
}

/// Write data to a file, failing if the file already exists (O_EXCL semantics).
/// Uses spawn_blocking to avoid blocking the Tokio worker thread.
#[allow(clippy::items_after_test_module)]
async fn write_fail_if_exists(path: &Path, data: impl AsRef<[u8]>) -> std::io::Result<()> {
    let path = path.to_owned();
    let data = data.as_ref().to_vec();
    tokio::task::spawn_blocking(move || {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.write_all(&data)?;
        file.sync_all()?;
        Ok(())
    })
    .await
    .map_err(std::io::Error::other)?
}
