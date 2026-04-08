// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Filesystem Utilities - atomic write operations
//!
//! Provides safe, atomic file write operations to prevent data corruption
//! on crash or interrupt. Uses temp file + rename pattern.
//!
//! # Design Decisions
//! - **TOCTOU-safe**: No exists() checks before idempotent operations
//! - **Atomic rename**: Guaranteed atomic on POSIX within same filesystem
//! - **fsync before rename**: Ensures data durability before visibility
//! - **Async cleanup**: Non-blocking error recovery

use std::io::{self, Write};
use std::path::Path;

/// Maximum file size for preview operations (512KB)
pub const MAX_PREVIEW_SIZE: u64 = 512 * 1024;

/// Atomically write content to a file using temp+rename pattern.
///
/// This ensures data integrity even on crash/interrupt:
/// 1. Write to temp file (path.tmp.nika)
/// 2. Sync to disk (fsync)
/// 3. Atomic rename to target path
/// 4. Cleanup temp file on error
///
/// # Safety
/// - TOCTOU-safe: uses idempotent create_dir_all without existence check
/// - Atomic on POSIX systems within same filesystem
///
/// # Example
/// ```ignore
/// use nika::util::fs::atomic_write;
///
/// atomic_write("/path/to/config.toml", b"[config]\nkey = \"value\"\n")?;
/// ```
pub fn atomic_write(path: impl AsRef<Path>, content: &[u8]) -> io::Result<()> {
    let path = path.as_ref();
    let temp_path = path.with_extension("tmp.nika");

    // Create parent directories (idempotent, no TOCTOU race)
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Write to temp file with fsync
    let mut file = std::fs::File::create(&temp_path)?;
    file.write_all(content)?;
    file.flush()?;
    file.sync_all()?; // Ensure data hits disk

    // Atomic rename (on POSIX systems, this is atomic within same filesystem)
    std::fs::rename(&temp_path, path).inspect_err(|_| {
        // Cleanup temp file on rename failure
        let _ = std::fs::remove_file(&temp_path);
    })
}

/// Check if a file size is within preview limits.
///
/// Returns `Ok(size)` if within limits, or `Err(actual_size)` if too large.
pub fn check_preview_size(path: impl AsRef<Path>) -> Result<u64, u64> {
    let metadata = std::fs::metadata(path.as_ref()).map_err(|_| 0u64)?;
    let size = metadata.len();
    if size > MAX_PREVIEW_SIZE {
        Err(size)
    } else {
        Ok(size)
    }
}

/// Format file size for human display.
///
/// # Examples
/// ```ignore
/// assert_eq!(format_size(1024), "1.0 KB");
/// assert_eq!(format_size(1_048_576), "1.0 MB");
/// ```
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_atomic_write_creates_file() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.txt");

        atomic_write(&path, b"Hello, World!").unwrap();

        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "Hello, World!");
    }

    #[test]
    fn test_atomic_write_no_temp_file_left() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.txt");
        let temp_path = path.with_extension("tmp.nika");

        atomic_write(&path, b"content").unwrap();

        assert!(!temp_path.exists(), "Temp file should be cleaned up");
    }

    #[test]
    fn test_atomic_write_creates_parent_dirs() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("nested").join("dir").join("file.txt");

        atomic_write(&path, b"nested content").unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_atomic_write_overwrites_existing() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.txt");

        atomic_write(&path, b"first").unwrap();
        atomic_write(&path, b"second").unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "second");
    }

    #[test]
    fn test_check_preview_size_ok() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("small.txt");
        std::fs::write(&path, "small content").unwrap();

        let result = check_preview_size(&path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_preview_size_too_large() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("large.txt");
        let large_content = "x".repeat((MAX_PREVIEW_SIZE + 1) as usize);
        std::fs::write(&path, large_content).unwrap();

        let result = check_preview_size(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 bytes");
        assert_eq!(format_size(512), "512 bytes");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1_048_576), "1.0 MB");
        assert_eq!(format_size(1_073_741_824), "1.0 GB");
    }
}
