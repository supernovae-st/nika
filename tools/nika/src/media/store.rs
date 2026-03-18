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

impl CasStore {
    /// Create a CAS store at the given root directory.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Create a CAS store at the workspace default location.
    ///
    /// Respects `NIKA_MEDIA_STORE` env var as override.
    /// Otherwise uses `{workspace_root}/.nika/media/store/`.
    pub fn workspace_default(workspace_root: &Path) -> Self {
        if let Ok(override_path) = std::env::var("NIKA_MEDIA_STORE") {
            return Self::new(PathBuf::from(override_path));
        }
        Self::new(workspace_root.join(".nika").join("media").join("store"))
    }

    /// Store binary data in the CAS (async).
    ///
    /// 1. Compute blake3 hash
    /// 2. Derive path: `{root}/{hash[0..2]}/{hash[2..]}` (no extension)
    /// 3. Create parent directories (async)
    /// 4. Write via `io::atomic::write_fail()` (O_EXCL atomic check+create)
    /// 5. If AlreadyExists, return deduplicated=true
    /// 6. Read-back verify only for files >= VERIFY_THRESHOLD
    pub async fn store(
        &self,
        data: &[u8],
    ) -> Result<StoreResult, MediaError> {
        let process_start = std::time::Instant::now();

        let raw_hash = blake3::hash(data).to_hex().to_string();
        let size = data.len() as u64;

        let prefixed_hash = format!("{HASH_PREFIX}{raw_hash}");

        let dir = self.root.join(&raw_hash[..2]);
        let final_path = dir.join(&raw_hash[2..]);

        // Create parent directories (async)
        tokio::fs::create_dir_all(&dir).await.map_err(|e| MediaError::MediaStoreWrite {
            path: dir.clone(),
            source: e,
        })?;

        // Atomic write via io::atomic::write_fail (O_EXCL)
        match crate::io::atomic::write_fail(&final_path, data).await {
            Ok(()) => {
                // New file stored
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return Ok(StoreResult {
                    hash: prefixed_hash,
                    path: final_path,
                    size,
                    deduplicated: true,
                    verified: true,
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

        // Read-back verification only for files >= 1MB
        let verified = if size >= VERIFY_THRESHOLD {
            let stored = tokio::fs::read(&final_path).await.map_err(|e| {
                MediaError::MediaStoreWrite {
                    path: final_path.clone(),
                    source: e,
                }
            })?;
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
            true
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
        if raw.len() < 3 {
            return false;
        }
        let path = self.root.join(&raw[..2]).join(&raw[2..]);
        path.exists()
    }

    /// Read file data by hash (async).
    pub async fn read(&self, hash: &str) -> Result<Vec<u8>, MediaError> {
        let raw = strip_hash_prefix(hash);
        if raw.len() < 3 {
            return Err(MediaError::MediaNotFound {
                hash: hash.to_string(),
            });
        }
        let path = self.root.join(&raw[..2]).join(&raw[2..]);
        tokio::fs::read(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                MediaError::MediaNotFound {
                    hash: hash.to_string(),
                }
            } else {
                MediaError::MediaStoreWrite {
                    path,
                    source: e,
                }
            }
        })
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
fn strip_hash_prefix(hash: &str) -> &str {
    hash.strip_prefix(HASH_PREFIX).unwrap_or(hash)
}
