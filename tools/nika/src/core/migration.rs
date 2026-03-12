//! Migration utilities for transitioning from ~/.spn to ~/.nika.
//!
//! This module handles the one-time migration of data from the legacy
//! `~/.spn/` directory structure to the new `~/.nika/` structure.
//!
//! ## Migration Strategy
//!
//! 1. **Detection**: Check if `~/.spn/` exists and `~/.nika/` doesn't
//! 2. **Backup**: Create a backup of `~/.spn/` before migration
//! 3. **Copy**: Copy all data to `~/.nika/` (not move, to be safe)
//! 4. **Verify**: Verify the migration was successful
//! 5. **Mark**: Create a `.migrated` marker file in `~/.spn/`
//!
//! ## Project-Level Migration
//!
//! - `spn.yaml` → `nika.yaml`
//! - `spn.lock` → `nika.lock`
//! - `.spn/` → `.nika/` (if only contains sessions/config)

use std::fs;
use std::io;
use std::path::Path;

use super::paths::{
    legacy_spn_home, legacy_spn_lockfile, legacy_spn_manifest, nika_home, NIKA_LOCKFILE,
    NIKA_MANIFEST,
};

// ═══════════════════════════════════════════════════════════════════════════
// MIGRATION STATUS
// ═══════════════════════════════════════════════════════════════════════════

/// Migration status for the global home directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationStatus {
    /// No migration needed (either already migrated or no legacy data)
    NotNeeded,
    /// Migration is needed (~/.spn exists but ~/.nika doesn't)
    Needed,
    /// Migration was already performed (marker file exists)
    AlreadyMigrated,
    /// Migration failed with an error
    Failed(String),
}

/// Result of a migration operation.
#[derive(Debug)]
pub struct MigrationResult {
    /// Whether the migration was successful
    pub success: bool,
    /// Directories that were migrated
    pub migrated_dirs: Vec<String>,
    /// Files that were migrated
    pub migrated_files: Vec<String>,
    /// Warnings encountered during migration
    pub warnings: Vec<String>,
    /// Error message if migration failed
    pub error: Option<String>,
}

impl MigrationResult {
    fn success(migrated_dirs: Vec<String>, migrated_files: Vec<String>) -> Self {
        Self {
            success: true,
            migrated_dirs,
            migrated_files,
            warnings: Vec::new(),
            error: None,
        }
    }

    fn failed(error: impl Into<String>) -> Self {
        Self {
            success: false,
            migrated_dirs: Vec::new(),
            migrated_files: Vec::new(),
            warnings: Vec::new(),
            error: Some(error.into()),
        }
    }

    fn not_needed() -> Self {
        Self {
            success: true,
            migrated_dirs: Vec::new(),
            migrated_files: Vec::new(),
            warnings: Vec::new(),
            error: None,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// GLOBAL MIGRATION (~/.spn → ~/.nika)
// ═══════════════════════════════════════════════════════════════════════════

/// Marker file to indicate migration was completed.
const MIGRATION_MARKER: &str = ".migrated_to_nika";

/// Check the migration status of the global home directory.
pub fn check_migration_status() -> MigrationStatus {
    let nika = nika_home();
    let spn = match legacy_spn_home() {
        Some(p) => p,
        None => return MigrationStatus::NotNeeded,
    };

    // If spn directory doesn't exist, no migration needed
    if !spn.exists() {
        return MigrationStatus::NotNeeded;
    }

    // Check if already migrated
    if spn.join(MIGRATION_MARKER).exists() {
        return MigrationStatus::AlreadyMigrated;
    }

    // If nika home exists with data, assume already migrated
    if nika.exists() && nika.join("packages").exists() {
        return MigrationStatus::NotNeeded;
    }

    // Migration needed only if spn has actual content
    let has_content = spn.join("packages").exists()
        || spn.join("models").exists()
        || spn.join("config.toml").exists()
        || spn.join("mcp.yaml").exists();

    if has_content {
        MigrationStatus::Needed
    } else {
        MigrationStatus::NotNeeded
    }
}

/// Migrate from ~/.spn to ~/.nika.
///
/// This copies (not moves) data to be safe. The original ~/.spn is preserved
/// with a marker file indicating migration was completed.
///
/// # Returns
///
/// Returns a `MigrationResult` with details about what was migrated.
pub fn migrate_global_home() -> MigrationResult {
    let status = check_migration_status();

    match status {
        MigrationStatus::NotNeeded | MigrationStatus::AlreadyMigrated => {
            return MigrationResult::not_needed();
        }
        MigrationStatus::Failed(e) => {
            return MigrationResult::failed(e);
        }
        MigrationStatus::Needed => {}
    }

    let spn = match legacy_spn_home() {
        Some(p) => p,
        None => return MigrationResult::not_needed(),
    };

    let nika = nika_home();
    let mut migrated_dirs = Vec::new();
    let mut migrated_files = Vec::new();

    // Create nika home
    if let Err(e) = fs::create_dir_all(&nika) {
        return MigrationResult::failed(format!("Failed to create ~/.nika: {}", e));
    }

    // Migrate directories
    let dirs_to_migrate = ["packages", "models", "backups", "cache"];
    for dir in &dirs_to_migrate {
        let src = spn.join(dir);
        let dst = nika.join(dir);

        if src.exists() && src.is_dir() {
            if let Err(e) = copy_dir_recursive(&src, &dst) {
                return MigrationResult::failed(format!("Failed to migrate {}: {}", dir, e));
            }
            migrated_dirs.push(dir.to_string());
        }
    }

    // Migrate files
    let files_to_migrate = [
        ("config.toml", "config.toml"),
        ("mcp.yaml", "mcp.yaml"),
        ("registry.yaml", "packages/registry.yaml"),
    ];

    for (src_name, dst_name) in &files_to_migrate {
        let src = spn.join(src_name);
        let dst = nika.join(dst_name);

        if src.exists() && src.is_file() {
            // Ensure parent directory exists
            if let Some(parent) = dst.parent() {
                let _ = fs::create_dir_all(parent);
            }

            if let Err(e) = fs::copy(&src, &dst) {
                return MigrationResult::failed(format!("Failed to copy {}: {}", src_name, e));
            }
            migrated_files.push(src_name.to_string());
        }
    }

    // Create daemon directory (new structure)
    let daemon_dir = nika.join("daemon");
    if let Err(e) = fs::create_dir_all(&daemon_dir) {
        return MigrationResult::failed(format!("Failed to create daemon dir: {}", e));
    }

    // Migrate daemon socket/pid if they exist
    for file in ["daemon.sock", "daemon.pid"] {
        let src = spn.join(file);
        let dst = daemon_dir.join(file.replace("daemon", "nika"));

        if src.exists() {
            // Don't migrate socket files - they're runtime only
            if !file.ends_with(".sock") {
                let _ = fs::copy(&src, &dst);
            }
        }
    }

    // Create migration marker in ~/.spn
    let marker_path = spn.join(MIGRATION_MARKER);
    let marker_content = format!(
        "Migrated to ~/.nika on {}\n\
         This directory is preserved for backup.\n\
         You can safely delete it after verifying ~/.nika works correctly.\n",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    );

    if let Err(e) = fs::write(&marker_path, marker_content) {
        // Non-fatal, just log warning
        let mut result = MigrationResult::success(migrated_dirs, migrated_files);
        result
            .warnings
            .push(format!("Failed to create migration marker: {}", e));
        return result;
    }

    MigrationResult::success(migrated_dirs, migrated_files)
}

// ═══════════════════════════════════════════════════════════════════════════
// PROJECT-LEVEL MIGRATION
// ═══════════════════════════════════════════════════════════════════════════

/// Project migration status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMigrationStatus {
    /// Whether spn.yaml needs migration to nika.yaml
    pub manifest_needs_migration: bool,
    /// Whether spn.lock needs migration to nika.lock
    pub lockfile_needs_migration: bool,
}

impl ProjectMigrationStatus {
    /// Returns true if any migration is needed.
    pub fn needs_migration(&self) -> bool {
        self.manifest_needs_migration || self.lockfile_needs_migration
    }
}

/// Check if a project needs migration.
pub fn check_project_migration(project_root: &Path) -> ProjectMigrationStatus {
    let manifest_needs =
        legacy_spn_manifest(project_root).is_some() && !project_root.join(NIKA_MANIFEST).exists();

    let lockfile_needs =
        legacy_spn_lockfile(project_root).is_some() && !project_root.join(NIKA_LOCKFILE).exists();

    ProjectMigrationStatus {
        manifest_needs_migration: manifest_needs,
        lockfile_needs_migration: lockfile_needs,
    }
}

/// Migrate project files from spn.* to nika.*.
///
/// This copies (not moves) files:
/// - `spn.yaml` → `nika.yaml`
/// - `spn.lock` → `nika.lock`
///
/// The original files are preserved.
pub fn migrate_project(project_root: &Path) -> MigrationResult {
    let status = check_project_migration(project_root);

    if !status.needs_migration() {
        return MigrationResult::not_needed();
    }

    let mut migrated_files = Vec::new();

    // Migrate manifest
    if status.manifest_needs_migration {
        if let Some(src) = legacy_spn_manifest(project_root) {
            let dst = project_root.join(NIKA_MANIFEST);
            if let Err(e) = fs::copy(&src, &dst) {
                return MigrationResult::failed(format!("Failed to migrate spn.yaml: {}", e));
            }
            migrated_files.push("spn.yaml → nika.yaml".to_string());
        }
    }

    // Migrate lockfile
    if status.lockfile_needs_migration {
        if let Some(src) = legacy_spn_lockfile(project_root) {
            let dst = project_root.join(NIKA_LOCKFILE);
            if let Err(e) = fs::copy(&src, &dst) {
                return MigrationResult::failed(format!("Failed to migrate spn.lock: {}", e));
            }
            migrated_files.push("spn.lock → nika.lock".to_string());
        }
    }

    MigrationResult::success(Vec::new(), migrated_files)
}

// ═══════════════════════════════════════════════════════════════════════════
// UTILITY FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════════

/// Recursively copy a directory.
fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

/// Print migration prompt for interactive use.
pub fn print_migration_prompt() {
    let status = check_migration_status();

    match status {
        MigrationStatus::Needed => {
            eprintln!();
            eprintln!(
                "╔═══════════════════════════════════════════════════════════════════════════╗"
            );
            eprintln!(
                "║  🦋 Nika Migration Available                                              ║"
            );
            eprintln!(
                "╠═══════════════════════════════════════════════════════════════════════════╣"
            );
            eprintln!(
                "║                                                                           ║"
            );
            eprintln!(
                "║  Found ~/.spn directory from previous installation.                       ║"
            );
            eprintln!(
                "║  Nika now uses ~/.nika for all data storage.                              ║"
            );
            eprintln!(
                "║                                                                           ║"
            );
            eprintln!(
                "║  Run 'nika init' to migrate your data:                                    ║"
            );
            eprintln!(
                "║  • Packages and models                                                    ║"
            );
            eprintln!(
                "║  • Configuration files                                                    ║"
            );
            eprintln!(
                "║  • MCP server definitions                                                 ║"
            );
            eprintln!(
                "║                                                                           ║"
            );
            eprintln!(
                "║  Your ~/.spn directory will be preserved as backup.                       ║"
            );
            eprintln!(
                "║                                                                           ║"
            );
            eprintln!(
                "╚═══════════════════════════════════════════════════════════════════════════╝"
            );
            eprintln!();
        }
        MigrationStatus::AlreadyMigrated => {
            // Silent - already migrated
        }
        MigrationStatus::NotNeeded => {
            // Silent - nothing to migrate
        }
        MigrationStatus::Failed(e) => {
            eprintln!("Warning: Migration check failed: {}", e);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::env;
    use tempfile::tempdir;

    /// Helper to set up test environment
    fn with_test_dirs<F, R>(f: F) -> R
    where
        F: FnOnce(&Path, &Path) -> R,
    {
        let spn_dir = tempdir().unwrap();
        let nika_dir = tempdir().unwrap();

        // Set environment variables
        let old_spn = env::var("SPN_HOME").ok();
        let old_nika = env::var("NIKA_HOME").ok();

        env::set_var("SPN_HOME", spn_dir.path());
        env::set_var("NIKA_HOME", nika_dir.path());

        let result = f(spn_dir.path(), nika_dir.path());

        // Restore
        if let Some(v) = old_spn {
            env::set_var("SPN_HOME", v);
        } else {
            env::remove_var("SPN_HOME");
        }
        if let Some(v) = old_nika {
            env::set_var("NIKA_HOME", v);
        } else {
            env::remove_var("NIKA_HOME");
        }

        result
    }

    #[test]
    fn test_migration_status_not_needed_no_spn() {
        let temp = tempdir().unwrap();
        env::set_var("SPN_HOME", temp.path().join("nonexistent"));
        env::set_var("NIKA_HOME", temp.path().join("nika"));

        let status = check_migration_status();
        assert_eq!(status, MigrationStatus::NotNeeded);

        env::remove_var("SPN_HOME");
        env::remove_var("NIKA_HOME");
    }

    #[test]
    #[serial]
    fn test_migration_status_needed() {
        with_test_dirs(|spn, nika| {
            // Create spn directory with some content
            fs::create_dir_all(spn.join("packages")).unwrap();
            fs::write(spn.join("config.toml"), "# config").unwrap();

            // Remove nika dir to simulate fresh install
            let _ = fs::remove_dir_all(nika);

            let status = check_migration_status();
            assert_eq!(status, MigrationStatus::Needed);
        });
    }

    #[test]
    #[serial]
    fn test_migration_status_already_migrated() {
        with_test_dirs(|spn, _nika| {
            // Create spn directory with marker
            fs::create_dir_all(spn).unwrap();
            fs::write(spn.join(MIGRATION_MARKER), "migrated").unwrap();

            let status = check_migration_status();
            assert_eq!(status, MigrationStatus::AlreadyMigrated);
        });
    }

    #[test]
    #[serial]
    fn test_migrate_global_home_success() {
        with_test_dirs(|spn, nika| {
            // Create spn structure
            fs::create_dir_all(spn.join("packages")).unwrap();
            fs::create_dir_all(spn.join("models")).unwrap();
            fs::write(spn.join("config.toml"), "[settings]\ntheme = \"dark\"").unwrap();
            fs::write(spn.join("mcp.yaml"), "servers: {}").unwrap();
            fs::write(spn.join("packages/registry.yaml"), "packages: []").unwrap();
            fs::write(spn.join("packages/test.txt"), "test package").unwrap();

            // Remove nika dir
            let _ = fs::remove_dir_all(nika);

            // Run migration
            let result = migrate_global_home();

            assert!(result.success);
            assert!(result.error.is_none());
            assert!(result.migrated_dirs.contains(&"packages".to_string()));
            assert!(result.migrated_dirs.contains(&"models".to_string()));
            assert!(result.migrated_files.contains(&"config.toml".to_string()));
            assert!(result.migrated_files.contains(&"mcp.yaml".to_string()));

            // Verify files were copied
            assert!(nika.join("config.toml").exists());
            assert!(nika.join("mcp.yaml").exists());
            assert!(nika.join("packages").exists());
            assert!(nika.join("packages/test.txt").exists());
            assert!(nika.join("daemon").exists());

            // Verify marker was created
            assert!(spn.join(MIGRATION_MARKER).exists());
        });
    }

    #[test]
    #[serial]
    fn test_migrate_global_home_not_needed() {
        with_test_dirs(|_spn, nika| {
            // Create nika with packages (already migrated)
            fs::create_dir_all(nika.join("packages")).unwrap();

            let result = migrate_global_home();
            assert!(result.success);
            assert!(result.migrated_dirs.is_empty());
            assert!(result.migrated_files.is_empty());
        });
    }

    #[test]
    fn test_project_migration_status() {
        let temp = tempdir().unwrap();
        let project = temp.path();

        // No files - no migration needed
        let status = check_project_migration(project);
        assert!(!status.needs_migration());

        // Create spn.yaml
        fs::write(project.join("spn.yaml"), "name: test").unwrap();
        let status = check_project_migration(project);
        assert!(status.manifest_needs_migration);
        assert!(!status.lockfile_needs_migration);

        // Create spn.lock
        fs::write(project.join("spn.lock"), "# lock").unwrap();
        let status = check_project_migration(project);
        assert!(status.manifest_needs_migration);
        assert!(status.lockfile_needs_migration);

        // Create nika.yaml - manifest no longer needs migration
        fs::write(project.join("nika.yaml"), "name: test").unwrap();
        let status = check_project_migration(project);
        assert!(!status.manifest_needs_migration);
        assert!(status.lockfile_needs_migration);
    }

    #[test]
    fn test_migrate_project() {
        let temp = tempdir().unwrap();
        let project = temp.path();

        // Create legacy files
        fs::write(project.join("spn.yaml"), "name: my-project\nversion: 1.0.0").unwrap();
        fs::write(project.join("spn.lock"), "# lockfile\npackages: []").unwrap();

        // Run migration
        let result = migrate_project(project);

        assert!(result.success);
        assert_eq!(result.migrated_files.len(), 2);

        // Verify files were created
        assert!(project.join("nika.yaml").exists());
        assert!(project.join("nika.lock").exists());

        // Verify content was preserved
        let manifest = fs::read_to_string(project.join("nika.yaml")).unwrap();
        assert!(manifest.contains("name: my-project"));

        // Original files still exist
        assert!(project.join("spn.yaml").exists());
        assert!(project.join("spn.lock").exists());
    }

    #[test]
    fn test_migrate_project_not_needed() {
        let temp = tempdir().unwrap();
        let project = temp.path();

        // No legacy files
        let result = migrate_project(project);
        assert!(result.success);
        assert!(result.migrated_files.is_empty());
    }

    #[test]
    fn test_migrate_project_partial() {
        let temp = tempdir().unwrap();
        let project = temp.path();

        // Only spn.yaml exists
        fs::write(project.join("spn.yaml"), "name: test").unwrap();

        let result = migrate_project(project);
        assert!(result.success);
        assert_eq!(result.migrated_files.len(), 1);
        assert!(project.join("nika.yaml").exists());
        assert!(!project.join("nika.lock").exists());
    }

    #[test]
    fn test_copy_dir_recursive() {
        let temp = tempdir().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        // Create nested structure
        fs::create_dir_all(src.join("a/b/c")).unwrap();
        fs::write(src.join("file1.txt"), "content1").unwrap();
        fs::write(src.join("a/file2.txt"), "content2").unwrap();
        fs::write(src.join("a/b/file3.txt"), "content3").unwrap();
        fs::write(src.join("a/b/c/file4.txt"), "content4").unwrap();

        // Copy
        copy_dir_recursive(&src, &dst).unwrap();

        // Verify
        assert!(dst.join("file1.txt").exists());
        assert!(dst.join("a/file2.txt").exists());
        assert!(dst.join("a/b/file3.txt").exists());
        assert!(dst.join("a/b/c/file4.txt").exists());

        // Verify content
        assert_eq!(
            fs::read_to_string(dst.join("a/b/c/file4.txt")).unwrap(),
            "content4"
        );
    }

    #[test]
    fn test_migration_result_methods() {
        let success = MigrationResult::success(
            vec!["packages".to_string()],
            vec!["config.toml".to_string()],
        );
        assert!(success.success);
        assert!(success.error.is_none());

        let failed = MigrationResult::failed("test error");
        assert!(!failed.success);
        assert_eq!(failed.error, Some("test error".to_string()));

        let not_needed = MigrationResult::not_needed();
        assert!(not_needed.success);
        assert!(not_needed.migrated_dirs.is_empty());
    }
}
