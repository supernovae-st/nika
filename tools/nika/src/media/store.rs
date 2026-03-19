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
    pub async fn store(
        &self,
        data: &[u8],
    ) -> Result<StoreResult, MediaError> {
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

        let raw_hash = blake3::hash(data).to_hex().to_string();
        let size = data.len() as u64;

        let prefixed_hash = format!("{HASH_PREFIX}{raw_hash}");

        let dir = self.root.join(&raw_hash[..2]);
        let final_path = dir.join(&raw_hash[2..]);

        // Create parent directories (async)
        tokio::fs::create_dir_all(&dir).await.map_err(|e| MediaError::MediaStoreIo {
            path: dir.clone(),
            source: e,
        })?;

        // Atomic write via O_EXCL (create_new). On success: new file.
        // On AlreadyExists: dedup hit. On other error: clean up partial file.
        match crate::io::atomic::write_fail(&final_path, data).await {
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

        // Read-back verification only for files >= 1MB
        // Small files: fsync guarantees integrity, verified=false to indicate skipped
        let verified = if size >= VERIFY_THRESHOLD {
            let stored = tokio::fs::read(&final_path).await.map_err(|e| {
                MediaError::MediaStoreIo {
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
                MediaError::MediaStoreIo {
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

#[cfg(test)]
mod tests {
    use super::*;

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

        let result = store.read("blake3:abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn hash_only_filename_no_extension() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let data = b"no extension in path";
        let result = store.store(data).await.unwrap();

        // Path should have NO extension
        assert!(result.path.extension().is_none(),
            "CAS path should have no extension: {:?}", result.path);
    }

    #[tokio::test]
    async fn hash_has_blake3_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let result = store.store(b"prefix test").await.unwrap();
        assert!(result.hash.starts_with("blake3:"),
            "hash should have blake3: prefix, got: {}", result.hash);
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
        assert!(result.is_ok(), "exactly MAX_STORE_SIZE should be accepted, got: {:?}", result.err());
        let sr = result.unwrap();
        assert_eq!(sr.size, MAX_STORE_SIZE as u64);
        assert!(!sr.deduplicated);
        // 100MB is above verify threshold (1MB), so it should be verified
        assert!(sr.verified, "100MB file should trigger read-back verification");

        // Verify read-back matches
        let read_back = store.read(&sr.hash).await.unwrap();
        assert_eq!(read_back.len(), MAX_STORE_SIZE);
        assert!(read_back.iter().all(|&b| b == 0xAB),
            "data corruption: not all bytes are 0xAB");
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
