//! Backup Implementation
//!
//! Handles creation, listing, and restoration of backups.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::core::paths::nika_home;
use crate::error::{NikaError, Result};

/// Backup directory name.
const BACKUP_DIR: &str = "backups";

/// Backup manifest filename.
const MANIFEST_FILE: &str = "manifest.json";

/// Default number of backups to keep.
const DEFAULT_KEEP_COUNT: usize = 10;

/// Files to include in backup.
const BACKUP_FILES: &[&str] = &["config.toml", "mcp.yaml", "sync.yaml"];

/// Directories to include in backup.
const BACKUP_DIRS: &[&str] = &["sessions"];

/// Backup options.
#[derive(Debug, Clone)]
pub struct BackupOptions {
    /// Include sessions in backup.
    pub include_sessions: bool,
    /// Custom backup name.
    pub name: Option<String>,
    /// Number of backups to keep when pruning.
    pub keep_count: usize,
}

impl Default for BackupOptions {
    fn default() -> Self {
        Self {
            include_sessions: true,
            name: None,
            keep_count: DEFAULT_KEEP_COUNT,
        }
    }
}

/// Information about a backup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    /// Backup ID (timestamp-based).
    pub id: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Custom name (if provided).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Files included.
    pub files: Vec<String>,
    /// Total size in bytes.
    pub size_bytes: u64,
}

/// Backup manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupManifest {
    /// Backup metadata.
    info: BackupInfo,
    /// Nika version that created this backup.
    nika_version: String,
}

/// Create a new backup.
pub fn create_backup(options: &BackupOptions) -> Result<BackupInfo> {
    let nika_dir = nika_home();
    let backup_dir = nika_dir.join(BACKUP_DIR);

    // Ensure backup directory exists
    fs::create_dir_all(&backup_dir).map_err(|e| NikaError::IoPathError {
        path: backup_dir.clone(),
        operation: "create backup directory".to_string(),
        source: e,
    })?;

    // Generate backup ID
    let now = Utc::now();
    let id = now.format("%Y%m%d_%H%M%S").to_string();
    let backup_path = backup_dir.join(&id);

    // Create backup subdirectory
    fs::create_dir_all(&backup_path).map_err(|e| NikaError::IoPathError {
        path: backup_path.clone(),
        operation: "create backup subdirectory".to_string(),
        source: e,
    })?;

    let mut files_backed_up = Vec::new();
    let mut total_size: u64 = 0;

    // Copy config files
    for file in BACKUP_FILES {
        let src = nika_dir.join(file);
        if src.exists() {
            let dst = backup_path.join(file);
            fs::copy(&src, &dst).map_err(|e| NikaError::IoPathError {
                path: src.clone(),
                operation: "copy to backup".to_string(),
                source: e,
            })?;
            if let Ok(meta) = fs::metadata(&dst) {
                total_size += meta.len();
            }
            files_backed_up.push(file.to_string());
        }
    }

    // Copy directories (if requested)
    if options.include_sessions {
        for dir in BACKUP_DIRS {
            let src = nika_dir.join(dir);
            if src.exists() && src.is_dir() {
                let dst = backup_path.join(dir);
                copy_dir_recursive(&src, &dst)?;
                total_size += dir_size(&dst);
                files_backed_up.push(format!("{}/", dir));
            }
        }
    }

    // Create backup info
    let info = BackupInfo {
        id: id.clone(),
        created_at: now,
        name: options.name.clone(),
        files: files_backed_up,
        size_bytes: total_size,
    };

    // Write manifest
    let manifest = BackupManifest {
        info: info.clone(),
        nika_version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let manifest_path = backup_path.join(MANIFEST_FILE);
    let manifest_json =
        serde_json::to_string_pretty(&manifest).map_err(|e| NikaError::SyncError {
            message: format!("Failed to serialize manifest: {}", e),
        })?;
    fs::write(&manifest_path, manifest_json).map_err(|e| NikaError::IoPathError {
        path: manifest_path,
        operation: "write backup manifest".to_string(),
        source: e,
    })?;

    Ok(info)
}

/// List available backups.
pub fn list_backups() -> Result<Vec<BackupInfo>> {
    let backup_dir = nika_home().join(BACKUP_DIR);

    if !backup_dir.exists() {
        return Ok(Vec::new());
    }

    let mut backups = Vec::new();

    let entries = fs::read_dir(&backup_dir).map_err(|e| NikaError::IoPathError {
        path: backup_dir.clone(),
        operation: "list backups".to_string(),
        source: e,
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let manifest_path = path.join(MANIFEST_FILE);
            if manifest_path.exists() {
                if let Ok(content) = fs::read_to_string(&manifest_path) {
                    if let Ok(manifest) = serde_json::from_str::<BackupManifest>(&content) {
                        backups.push(manifest.info);
                    }
                }
            }
        }
    }

    // Sort by creation time (newest first)
    backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(backups)
}

/// Restore from a backup.
pub fn restore_backup(backup_id: Option<&str>) -> Result<BackupInfo> {
    let backup_dir = nika_home().join(BACKUP_DIR);

    // Find the backup to restore
    let backup_path = match backup_id {
        Some(id) => {
            let path = backup_dir.join(id);
            if !path.exists() {
                return Err(NikaError::SyncError {
                    message: format!("Backup '{}' not found", id),
                });
            }
            path
        }
        None => {
            // Use most recent backup
            let backups = list_backups()?;
            if backups.is_empty() {
                return Err(NikaError::SyncError {
                    message: "No backups available".to_string(),
                });
            }
            backup_dir.join(&backups[0].id)
        }
    };

    // Read manifest
    let manifest_path = backup_path.join(MANIFEST_FILE);
    let content = fs::read_to_string(&manifest_path).map_err(|e| NikaError::IoPathError {
        path: manifest_path.clone(),
        operation: "read backup manifest".to_string(),
        source: e,
    })?;
    let manifest: BackupManifest =
        serde_json::from_str(&content).map_err(|e| NikaError::SyncError {
            message: format!("Failed to parse backup manifest: {}", e),
        })?;

    let nika_dir = nika_home();

    // Restore files
    for file in BACKUP_FILES {
        let src = backup_path.join(file);
        if src.exists() {
            let dst = nika_dir.join(file);
            fs::copy(&src, &dst).map_err(|e| NikaError::IoPathError {
                path: dst.clone(),
                operation: "restore file".to_string(),
                source: e,
            })?;
        }
    }

    // Restore directories
    for dir in BACKUP_DIRS {
        let src = backup_path.join(dir);
        if src.exists() && src.is_dir() {
            let dst = nika_dir.join(dir);
            // Remove existing directory first
            if dst.exists() {
                fs::remove_dir_all(&dst).map_err(|e| NikaError::IoPathError {
                    path: dst.clone(),
                    operation: "remove existing directory".to_string(),
                    source: e,
                })?;
            }
            copy_dir_recursive(&src, &dst)?;
        }
    }

    Ok(manifest.info)
}

/// Prune old backups, keeping the most recent.
pub fn prune_backups(keep_count: usize) -> Result<usize> {
    let mut backups = list_backups()?;

    if backups.len() <= keep_count {
        return Ok(0);
    }

    // Sort by creation time (newest first)
    backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let backup_dir = nika_home().join(BACKUP_DIR);
    let mut pruned = 0;

    // Remove excess backups
    for backup in backups.iter().skip(keep_count) {
        let path = backup_dir.join(&backup.id);
        if path.exists() {
            fs::remove_dir_all(&path).map_err(|e| NikaError::IoPathError {
                path: path.clone(),
                operation: "remove old backup".to_string(),
                source: e,
            })?;
            pruned += 1;
        }
    }

    Ok(pruned)
}

/// Copy directory recursively.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).map_err(|e| NikaError::IoPathError {
        path: dst.to_path_buf(),
        operation: "create directory".to_string(),
        source: e,
    })?;

    let entries = fs::read_dir(src).map_err(|e| NikaError::IoPathError {
        path: src.to_path_buf(),
        operation: "read directory".to_string(),
        source: e,
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        let dest_path = dst.join(entry.file_name());

        if path.is_dir() {
            copy_dir_recursive(&path, &dest_path)?;
        } else {
            fs::copy(&path, &dest_path).map_err(|e| NikaError::IoPathError {
                path: path.clone(),
                operation: "copy file".to_string(),
                source: e,
            })?;
        }
    }

    Ok(())
}

/// Calculate directory size.
fn dir_size(path: &Path) -> u64 {
    let mut size = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                size += dir_size(&path);
            } else if let Ok(meta) = fs::metadata(&path) {
                size += meta.len();
            }
        }
    }
    size
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_backup_options_default() {
        let opts = BackupOptions::default();
        assert!(opts.include_sessions);
        assert!(opts.name.is_none());
        assert_eq!(opts.keep_count, DEFAULT_KEEP_COUNT);
    }

    #[test]
    fn test_backup_info_serialization() {
        let info = BackupInfo {
            id: "20240115_103000".to_string(),
            created_at: Utc::now(),
            name: Some("test backup".to_string()),
            files: vec!["config.toml".to_string(), "mcp.yaml".to_string()],
            size_bytes: 1024,
        };

        let json = serde_json::to_string(&info).unwrap();
        let parsed: BackupInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, info.id);
        assert_eq!(parsed.name, info.name);
        assert_eq!(parsed.files.len(), 2);
    }

    #[test]
    fn test_copy_dir_recursive() {
        let temp = tempdir().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        // Create source structure
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::write(src.join("file1.txt"), "content1").unwrap();
        fs::write(src.join("sub/file2.txt"), "content2").unwrap();

        // Copy
        copy_dir_recursive(&src, &dst).unwrap();

        // Verify
        assert!(dst.join("file1.txt").exists());
        assert!(dst.join("sub/file2.txt").exists());
        assert_eq!(
            fs::read_to_string(dst.join("file1.txt")).unwrap(),
            "content1"
        );
        assert_eq!(
            fs::read_to_string(dst.join("sub/file2.txt")).unwrap(),
            "content2"
        );
    }

    #[test]
    fn test_dir_size() {
        let temp = tempdir().unwrap();
        let dir = temp.path().join("test");

        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("file1.txt"), "12345").unwrap();
        fs::write(dir.join("file2.txt"), "1234567890").unwrap();

        let size = dir_size(&dir);
        assert_eq!(size, 15); // 5 + 10 bytes
    }

    #[test]
    fn test_list_backups_empty() {
        // Use temp directory that won't have backups
        // Note: This test relies on NIKA_HOME not being set
        // In real usage, this would list actual backups
        let _ = list_backups(); // Just verify it doesn't panic
    }

    #[test]
    fn test_backup_manifest() {
        let info = BackupInfo {
            id: "test".to_string(),
            created_at: Utc::now(),
            name: None,
            files: vec!["config.toml".to_string()],
            size_bytes: 100,
        };

        let manifest = BackupManifest {
            info: info.clone(),
            nika_version: "0.27.0".to_string(),
        };

        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: BackupManifest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.nika_version, "0.27.0");
        assert_eq!(parsed.info.id, "test");
    }
}
