// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Atomic file operations for safe writes
//!
//! Shared by WriteTool and ArtifactWriter.
//! All functions use tokio::fs for async I/O.
//!
//! # Guarantees
//!
//! - **Atomicity**: temp file → flush → sync → rename
//! - **Durability**: `sync_all()` ensures data reaches disk
//! - **Safety**: cleanup on failure, no partial writes
//!
//! # Usage
//!
//! ```ignore
//! use nika::io::atomic;
//!
//! // Atomic overwrite
//! atomic::write_atomic(path, content).await?;
//!
//! // Append to file
//! atomic::write_append(path, content).await?;
//!
//! // Generate unique filename if exists
//! let actual_path = atomic::write_unique(path, content).await?;
//!
//! // Fail if file exists (atomic check)
//! atomic::write_fail(path, content).await?;
//! ```

use std::path::{Path, PathBuf};
use tokio::fs::{self, File, OpenOptions};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════════════
// ATOMIC WRITE FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════════

/// Atomic write: temp file → flush → sync → rename
///
/// # Guarantees
///
/// - All-or-nothing semantics
/// - Data reaches disk before file appears
/// - No partial writes on crash
///
/// # Arguments
///
/// * `path` - Target file path
/// * `content` - Content to write
///
/// # Errors
///
/// Returns `std::io::Error` if:
/// - Cannot create temp file
/// - Cannot write content
/// - Cannot rename (different filesystem)
pub async fn write_atomic(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));

    // Unique temp filename: .nika-tmp-{pid}-{uuid}
    let temp_path = parent.join(format!(
        ".nika-tmp-{}-{}",
        std::process::id(),
        Uuid::new_v4().simple()
    ));

    // Write to temp file
    let mut file = File::create(&temp_path).await?;
    file.write_all(content).await?;
    file.flush().await?;
    file.sync_all().await?;
    drop(file); // Ensure closed before rename

    // Atomic rename (POSIX guarantees atomicity on same filesystem)
    match fs::rename(&temp_path, path).await {
        Ok(()) => Ok(()),
        Err(e) => {
            // Best-effort cleanup
            let _ = fs::remove_file(&temp_path).await;
            Err(e)
        }
    }
}

/// Write with append semantics
///
/// Creates file if it doesn't exist, appends if it does.
/// Uses flush + sync for durability.
///
/// # Arguments
///
/// * `path` - Target file path
/// * `content` - Content to append
pub async fn write_append(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;

    file.write_all(content).await?;
    file.flush().await?;
    file.sync_all().await?;
    Ok(())
}

/// Write with unique filename if exists
///
/// If `path` exists, generates `path-1.ext`, `path-2.ext`, etc.
/// Returns the actual path written.
///
/// # Arguments
///
/// * `path` - Preferred file path
/// * `content` - Content to write
///
/// # Returns
///
/// The actual path where content was written.
///
/// # Errors
///
/// Returns error if:
/// - Cannot find unique name after 1000 attempts
/// - Write operation fails
pub async fn write_unique(path: &Path, content: &[u8]) -> std::io::Result<PathBuf> {
    // Try original path first (atomic via O_EXCL — no TOCTOU)
    match write_fail(path, content).await {
        Ok(()) => return Ok(path.to_path_buf()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }

    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let parent = path.parent().unwrap_or(Path::new("."));

    for i in 1..1000 {
        let new_path = parent.join(format!("{}-{}{}", stem, i, ext));
        match write_fail(&new_path, content).await {
            Ok(()) => return Ok(new_path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "Could not generate unique filename after 1000 attempts",
    ))
}

/// Write with fail-if-exists semantics (atomic check + create)
///
/// Uses `create_new(true)` for atomic check+create (O_EXCL on POSIX).
/// This avoids TOCTOU race conditions.
///
/// # Arguments
///
/// * `path` - Target file path (must not exist)
/// * `content` - Content to write
///
/// # Errors
///
/// Returns `ErrorKind::AlreadyExists` if file exists.
pub async fn write_fail(path: &Path, content: &[u8]) -> std::io::Result<()> {
    // Use create_new for atomic check + create (avoids TOCTOU race)
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true) // Fails if exists - atomic!
        .open(path)
        .await?;

    file.write_all(content).await?;
    file.flush().await?;
    file.sync_all().await?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ───────────────────────────────────────────────────────────────────────
    // write_atomic tests
    // ───────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_write_atomic_creates_file() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.txt");

        write_atomic(&path, b"Hello, World!").await.unwrap();

        let content = fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "Hello, World!");
    }

    #[tokio::test]
    async fn test_write_atomic_overwrites_existing() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.txt");

        // Create initial file
        fs::write(&path, "original").await.unwrap();

        // Overwrite
        write_atomic(&path, b"updated").await.unwrap();

        let content = fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "updated");
    }

    #[tokio::test]
    async fn test_write_atomic_no_temp_file_left() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.txt");

        write_atomic(&path, b"content").await.unwrap();

        // Check no .nika-tmp-* files remain
        let entries: Vec<_> = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(".nika-tmp-"))
            .collect();

        assert!(entries.is_empty(), "Temp files should be cleaned up");
    }

    #[tokio::test]
    async fn test_write_atomic_binary_content() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("binary.bin");

        let binary_data: Vec<u8> = (0..=255).collect();
        write_atomic(&path, &binary_data).await.unwrap();

        let content = fs::read(&path).await.unwrap();
        assert_eq!(content, binary_data);
    }

    // ───────────────────────────────────────────────────────────────────────
    // write_append tests
    // ───────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_write_append_creates_new_file() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("log.txt");

        write_append(&path, b"line 1\n").await.unwrap();

        let content = fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "line 1\n");
    }

    #[tokio::test]
    async fn test_write_append_appends_to_existing() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("log.txt");

        write_append(&path, b"line 1\n").await.unwrap();
        write_append(&path, b"line 2\n").await.unwrap();
        write_append(&path, b"line 3\n").await.unwrap();

        let content = fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "line 1\nline 2\nline 3\n");
    }

    // ───────────────────────────────────────────────────────────────────────
    // write_unique tests
    // ───────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_write_unique_uses_original_if_available() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("data.json");

        let actual = write_unique(&path, b"{}").await.unwrap();

        assert_eq!(actual, path);
        assert!(path.exists());
    }

    #[tokio::test]
    async fn test_write_unique_generates_suffix() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("data.json");

        // Create original
        fs::write(&path, "original").await.unwrap();

        // Should create data-1.json
        let actual = write_unique(&path, b"new").await.unwrap();

        assert_eq!(actual, temp_dir.path().join("data-1.json"));
        assert!(actual.exists());

        // Original unchanged
        let original_content = fs::read_to_string(&path).await.unwrap();
        assert_eq!(original_content, "original");
    }

    #[tokio::test]
    async fn test_write_unique_increments_suffix() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("file.txt");

        // Create file.txt, file-1.txt, file-2.txt
        fs::write(&path, "0").await.unwrap();
        fs::write(temp_dir.path().join("file-1.txt"), "1")
            .await
            .unwrap();
        fs::write(temp_dir.path().join("file-2.txt"), "2")
            .await
            .unwrap();

        // Should create file-3.txt
        let actual = write_unique(&path, b"3").await.unwrap();

        assert_eq!(actual, temp_dir.path().join("file-3.txt"));
    }

    #[tokio::test]
    async fn test_write_unique_no_extension() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("README");

        // Create original
        fs::write(&path, "original").await.unwrap();

        // Should create README-1 (no extension)
        let actual = write_unique(&path, b"new").await.unwrap();

        assert_eq!(actual, temp_dir.path().join("README-1"));
    }

    // ───────────────────────────────────────────────────────────────────────
    // write_fail tests
    // ───────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_write_fail_creates_new_file() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("new.txt");

        write_fail(&path, b"content").await.unwrap();

        let content = fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "content");
    }

    #[tokio::test]
    async fn test_write_fail_errors_if_exists() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("existing.txt");

        // Create file first
        fs::write(&path, "existing").await.unwrap();

        // Should fail
        let result = write_fail(&path, b"new").await;

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );

        // Original unchanged
        let content = fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "existing");
    }

    #[tokio::test]
    async fn test_write_fail_atomic_check() {
        // This test verifies that write_fail uses O_EXCL (create_new)
        // which provides atomic check-and-create semantics
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("race.txt");

        // Two concurrent writes - only one should succeed
        let path1 = path.clone();
        let path2 = path.clone();

        let (r1, r2) = tokio::join!(
            write_fail(&path1, b"writer 1"),
            write_fail(&path2, b"writer 2"),
        );

        // Exactly one should succeed
        let successes = [r1.is_ok(), r2.is_ok()];
        assert_eq!(
            successes.iter().filter(|&&x| x).count(),
            1,
            "Exactly one writer should succeed"
        );
    }

    // ───────────────────────────────────────────────────────────────────────
    // Edge cases
    // ───────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_write_atomic_empty_content() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("empty.txt");

        write_atomic(&path, b"").await.unwrap();

        let content = fs::read(&path).await.unwrap();
        assert!(content.is_empty());
    }

    #[tokio::test]
    async fn test_write_atomic_large_content() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("large.bin");

        // 1MB of data
        let large_content: Vec<u8> = (0..1024 * 1024).map(|i| (i % 256) as u8).collect();
        write_atomic(&path, &large_content).await.unwrap();

        let content = fs::read(&path).await.unwrap();
        assert_eq!(content.len(), 1024 * 1024);
        assert_eq!(content, large_content);
    }

    #[tokio::test]
    async fn test_write_to_nested_existing_dir() {
        let temp_dir = TempDir::new().unwrap();
        let nested = temp_dir.path().join("a/b/c");
        fs::create_dir_all(&nested).await.unwrap();

        let path = nested.join("file.txt");
        write_atomic(&path, b"nested").await.unwrap();

        assert!(path.exists());
    }
}
