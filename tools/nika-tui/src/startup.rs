// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Startup Verification Module
//!
//! Ensures all required resources are available before TUI initialization.
//! Called early in `run_unified()` to fail fast with clear error messages.
//!
//! ## Verification Sequence
//!
//! ```text
//! verify_startup()
//! ├── ensure_directories()    → .nika/, sessions/, traces/
//! ├── verify_schema()         → nika-workflow.schema.json readable
//! ├── load_config_graceful()  → config.toml with fallback
//! └── verify_project_access() → project directory readable
//! ```

use std::path::PathBuf;
use std::time::{Duration, Instant};

use nika_engine::error::{NikaError, Result};

// ═══════════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════════

/// Root directory for Nika configuration and state
pub const NIKA_DIR: &str = ".nika";

/// Directory for session persistence files
pub const SESSIONS_DIR: &str = "sessions";

/// Directory for execution trace files
pub const TRACES_DIR: &str = "traces";

// ═══════════════════════════════════════════════════════════════════════════════
// P0-1: Directory Creation
// ═══════════════════════════════════════════════════════════════════════════════

/// Report from directory verification
#[derive(Debug, Default, Clone)]
pub struct DirectoryReport {
    /// Path to .nika directory
    pub nika_dir: Option<PathBuf>,
    /// Directories that were created
    pub created: Vec<String>,
    /// Directories that already existed
    pub existed: Vec<String>,
    /// Errors encountered
    pub errors: Vec<String>,
}

impl DirectoryReport {
    /// Returns true if no errors occurred
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Ensure all required directories exist, creating them if needed.
///
/// Creates:
/// - `.nika/` - Root config directory
/// - `.nika/sessions/` - Session persistence
/// - `.nika/traces/` - Execution traces
///
/// This function is idempotent - safe to call multiple times.
pub fn ensure_directories() -> Result<DirectoryReport> {
    let cwd = std::env::current_dir().map_err(|e| NikaError::StartupError {
        phase: "directory_check".into(),
        reason: format!("Cannot access current directory: {}", e),
    })?;
    ensure_directories_in(&cwd)
}

/// Ensure directories exist in a specific base directory (for testing)
pub fn ensure_directories_in(base_dir: &std::path::Path) -> Result<DirectoryReport> {
    let nika_dir = base_dir.join(NIKA_DIR);
    let sessions_dir = nika_dir.join(SESSIONS_DIR);
    let traces_dir = nika_dir.join(TRACES_DIR);

    let mut report = DirectoryReport {
        nika_dir: Some(nika_dir.clone()),
        ..Default::default()
    };

    // Create directories (idempotent via create_dir_all)
    for (path, name) in [
        (&nika_dir, NIKA_DIR),
        (&sessions_dir, SESSIONS_DIR),
        (&traces_dir, TRACES_DIR),
    ] {
        if path.exists() {
            report.existed.push(name.to_string());
        } else {
            match std::fs::create_dir_all(path) {
                Ok(_) => report.created.push(name.to_string()),
                Err(e) => report.errors.push(format!("{}: {}", name, e)),
            }
        }
    }

    Ok(report)
}

// ═══════════════════════════════════════════════════════════════════════════════
// P0-2: Schema Verification
// ═══════════════════════════════════════════════════════════════════════════════

/// Report from schema verification
#[derive(Debug, Default, Clone)]
pub struct SchemaReport {
    /// Whether the schema was loaded successfully
    pub schema_loaded: bool,
    /// Path to the schema file
    pub schema_path: Option<PathBuf>,
    /// Error message if loading failed
    pub error: Option<String>,
}

impl SchemaReport {
    /// Returns true if schema was loaded successfully
    pub fn is_ok(&self) -> bool {
        self.schema_loaded && self.error.is_none()
    }
}

/// Verify schema file is readable
pub fn verify_schema() -> Result<SchemaReport> {
    use nika_engine::ast::schema_validator::WorkflowSchemaValidator;

    let mut report = SchemaReport::default();

    // Try to create validator (loads schema internally)
    match WorkflowSchemaValidator::new() {
        Ok(_validator) => {
            report.schema_loaded = true;
            // Get schema path from validator if available
            report.schema_path = Some(PathBuf::from("schemas/nika-workflow.schema.json"));
        }
        Err(e) => {
            report.error = Some(format!("Schema load failed: {}", e));
        }
    }

    Ok(report)
}

// ═══════════════════════════════════════════════════════════════════════════════
// P0-3: Config Validation
// ═══════════════════════════════════════════════════════════════════════════════

/// Source of configuration
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    /// Using default configuration
    #[default]
    Default,
    /// Loaded from file
    File,
}

/// Report from config loading
#[derive(Debug, Clone)]
pub struct ConfigReport {
    /// Whether config was loaded (always true due to fallback)
    pub loaded: bool,
    /// Source of the configuration
    pub source: ConfigSource,
    /// Warning message if fallback was used
    pub warning: Option<String>,
    /// Path to config file (if loaded from file)
    pub config_path: Option<PathBuf>,
}

impl Default for ConfigReport {
    fn default() -> Self {
        Self {
            loaded: false,
            source: ConfigSource::Default,
            warning: None,
            config_path: None,
        }
    }
}

impl ConfigReport {
    /// Config loading always succeeds (falls back to defaults)
    pub fn is_ok(&self) -> bool {
        self.loaded
    }
}

/// Load config with graceful fallback to defaults.
///
/// Never fails - if config file is missing or invalid, uses defaults
/// and records a warning.
pub fn load_config_graceful() -> ConfigReport {
    let cwd = std::env::current_dir().ok();
    load_config_graceful_in(cwd.as_deref())
}

/// Load config from a specific base directory (for testing)
pub fn load_config_graceful_in(base_dir: Option<&std::path::Path>) -> ConfigReport {
    // Try nika.toml first (new standard), then .nika/config.toml (legacy)
    let config_path = base_dir.and_then(|dir| {
        let nika_toml = dir.join("nika.toml");
        if nika_toml.exists() {
            return Some(nika_toml);
        }
        let legacy = dir.join(NIKA_DIR).join("config.toml");
        if legacy.exists() {
            return Some(legacy);
        }
        None
    });

    let mut report = ConfigReport::default();

    // Check if config file exists
    if let Some(ref path) = config_path {
        if path.exists() {
            // Try to load config
            match std::fs::read_to_string(path) {
                Ok(content) => {
                    // Try to parse TOML (basic validation)
                    if content.trim().is_empty() || content.contains('[') {
                        report.loaded = true;
                        report.source = ConfigSource::File;
                        report.config_path = Some(path.clone());
                    } else {
                        report.loaded = true;
                        report.source = ConfigSource::Default;
                        report.warning = Some("Config file exists but appears invalid".into());
                    }
                }
                Err(e) => {
                    report.loaded = true;
                    report.source = ConfigSource::Default;
                    report.warning = Some(format!("Cannot read config file: {}", e));
                }
            }
        } else {
            // No config file - use defaults
            report.loaded = true;
            report.source = ConfigSource::Default;
            // No warning - missing config is normal for new projects
        }
    } else {
        report.loaded = true;
        report.source = ConfigSource::Default;
    }

    report
}

// ═══════════════════════════════════════════════════════════════════════════════
// P0-5: Project Access
// ═══════════════════════════════════════════════════════════════════════════════

/// Report from project access verification
#[derive(Debug, Default, Clone)]
pub struct ProjectReport {
    /// Path to project directory
    pub project_dir: Option<PathBuf>,
    /// Whether directory is readable
    pub readable: bool,
    /// Number of files in directory
    pub file_count: usize,
    /// Number of .nika.yaml workflow files found
    pub workflow_count: usize,
    /// Error message if access failed
    pub error: Option<String>,
}

impl ProjectReport {
    /// Returns true if project is accessible
    pub fn is_ok(&self) -> bool {
        self.readable && self.error.is_none()
    }
}

/// Verify project directory is accessible
pub fn verify_project_access() -> Result<ProjectReport> {
    let cwd = std::env::current_dir().map_err(|e| NikaError::StartupError {
        phase: "project_access".into(),
        reason: format!("Cannot access current directory: {}", e),
    })?;
    verify_project_access_in(&cwd)
}

/// Verify project access in a specific directory (for testing)
pub fn verify_project_access_in(project_dir: &std::path::Path) -> Result<ProjectReport> {
    // Use struct initialization to avoid clippy::field_reassign_with_default
    let mut report = ProjectReport {
        project_dir: Some(project_dir.to_path_buf()),
        ..Default::default()
    };

    // Check if we can read the directory
    match std::fs::read_dir(project_dir) {
        Ok(entries) => {
            report.readable = true;
            report.file_count = entries.count();
        }
        Err(e) => {
            report.error = Some(format!("Cannot read project directory: {}", e));
            return Ok(report);
        }
    }

    // Count .nika.yaml files (non-recursive for speed)
    if let Ok(entries) = std::fs::read_dir(project_dir) {
        report.workflow_count = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .is_some_and(|ext| ext == "yaml" || ext == "yml")
            })
            .filter(|e| {
                e.path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".nika.yaml") || n.ends_with(".nika.yml"))
            })
            .count();
    }

    Ok(report)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Combined Startup Report
// ═══════════════════════════════════════════════════════════════════════════════

/// Combined startup verification report
#[derive(Debug, Clone)]
pub struct StartupReport {
    /// Directory creation report
    pub directories: DirectoryReport,
    /// Schema verification report
    pub schema: SchemaReport,
    /// Config loading report
    pub config: ConfigReport,
    /// Project access report
    pub project: ProjectReport,
    /// When verification started
    pub started_at: Instant,
    /// How long verification took
    pub duration: Duration,
}

impl Default for StartupReport {
    fn default() -> Self {
        Self {
            directories: DirectoryReport::default(),
            schema: SchemaReport::default(),
            config: ConfigReport::default(),
            project: ProjectReport::default(),
            started_at: Instant::now(),
            duration: Duration::ZERO,
        }
    }
}

impl StartupReport {
    /// Returns true if all critical verifications passed
    pub fn is_ok(&self) -> bool {
        self.directories.is_ok() && self.schema.is_ok() && self.project.is_ok()
        // config always "ok" due to fallback
    }

    /// Collect all warnings from verification
    pub fn warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        if let Some(w) = &self.config.warning {
            warnings.push(format!("Config: {}", w));
        }

        for err in &self.directories.errors {
            warnings.push(format!("Directory: {}", err));
        }

        if let Some(err) = &self.schema.error {
            warnings.push(format!("Schema: {}", err));
        }

        if let Some(err) = &self.project.error {
            warnings.push(format!("Project: {}", err));
        }

        warnings
    }

    /// Get summary string for logging
    pub fn summary(&self) -> String {
        let status = if self.is_ok() { "OK" } else { "FAILED" };
        format!(
            "Startup {} in {:?} (dirs: {}, schema: {}, config: {}, project: {})",
            status,
            self.duration,
            if self.directories.is_ok() {
                "✓"
            } else {
                "✗"
            },
            if self.schema.is_ok() { "✓" } else { "✗" },
            if self.config.is_ok() { "✓" } else { "✗" },
            if self.project.is_ok() { "✓" } else { "✗" },
        )
    }
}

/// Run all startup verifications
///
/// Schema uses OnceLock internally — first call compiles, subsequent calls are O(1).
pub fn verify_startup() -> Result<StartupReport> {
    let started_at = Instant::now();

    let directories = ensure_directories()?;
    let schema = verify_schema()?;
    let config = load_config_graceful();
    let project = verify_project_access()?;

    Ok(StartupReport {
        directories,
        schema,
        config,
        project,
        started_at,
        duration: started_at.elapsed(),
    })
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ─────────────────────────────────────────────────────────────────────────
    // P0-1: Directory Creation Tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_ensure_directories_creates_all() {
        let temp = TempDir::new().unwrap();
        let report = ensure_directories_in(temp.path()).unwrap();

        assert!(report.is_ok());
        assert!(temp.path().join(".nika").exists());
        assert!(temp.path().join(".nika/sessions").exists());
        assert!(temp.path().join(".nika/traces").exists());
        assert_eq!(report.created.len(), 3);
        assert!(report.existed.is_empty());
    }

    #[test]
    fn test_ensure_directories_idempotent() {
        let temp = TempDir::new().unwrap();
        let report1 = ensure_directories_in(temp.path()).unwrap();
        let report2 = ensure_directories_in(temp.path()).unwrap();

        assert!(report1.is_ok());
        assert!(report2.is_ok());
        // Second call should find existing dirs
        assert_eq!(report2.existed.len(), 3);
        assert!(report2.created.is_empty());
    }

    #[test]
    fn test_ensure_directories_partial_exist() {
        let temp = TempDir::new().unwrap();
        // Create only .nika but not subdirs
        std::fs::create_dir(temp.path().join(".nika")).unwrap();

        let report = ensure_directories_in(temp.path()).unwrap();

        assert!(report.is_ok());
        assert!(report.existed.contains(&NIKA_DIR.to_string()));
        assert!(report.created.contains(&SESSIONS_DIR.to_string()));
        assert!(report.created.contains(&TRACES_DIR.to_string()));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // P0-2: Schema Verification Tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_verify_schema_report_structure() {
        // This test verifies the report structure, not actual schema loading
        // (schema loading depends on project structure)
        let report = SchemaReport::default();
        assert!(!report.is_ok()); // Default is not loaded
        assert!(!report.schema_loaded);
        assert!(report.error.is_none());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // P0-3: Config Loading Tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_load_config_graceful_missing_file() {
        let temp = TempDir::new().unwrap();
        let report = load_config_graceful_in(Some(temp.path()));

        assert!(report.is_ok());
        assert_eq!(report.source, ConfigSource::Default);
        assert!(report.warning.is_none()); // Missing file is not a warning
    }

    #[test]
    fn test_load_config_graceful_with_nika_toml() {
        let temp = TempDir::new().unwrap();
        // Create nika.toml (new standard)
        std::fs::write(
            temp.path().join("nika.toml"),
            "[project]\nname = \"test\"\n",
        )
        .unwrap();

        let report = load_config_graceful_in(Some(temp.path()));

        assert!(report.is_ok());
        assert_eq!(report.source, ConfigSource::File);
        assert!(report.config_path.is_some());
        // Should find nika.toml, not .nika/config.toml
        assert!(report.config_path.as_ref().unwrap().ends_with("nika.toml"));
    }

    #[test]
    fn test_load_config_graceful_legacy_fallback() {
        let temp = TempDir::new().unwrap();
        // Create .nika/config.toml (legacy — no nika.toml)
        std::fs::create_dir(temp.path().join(".nika")).unwrap();
        std::fs::write(
            temp.path().join(".nika/config.toml"),
            "[editor]\ntheme = \"dark\"\n",
        )
        .unwrap();

        let report = load_config_graceful_in(Some(temp.path()));

        assert!(report.is_ok());
        assert_eq!(report.source, ConfigSource::File);
        assert!(report.config_path.is_some());
    }

    #[test]
    fn test_load_config_graceful_empty_file() {
        let temp = TempDir::new().unwrap();
        // Create empty nika.toml
        std::fs::write(temp.path().join("nika.toml"), "").unwrap();

        let report = load_config_graceful_in(Some(temp.path()));

        assert!(report.is_ok());
        // Empty file is valid TOML
        assert_eq!(report.source, ConfigSource::File);
    }

    #[test]
    fn test_load_config_graceful_none_base_dir() {
        let report = load_config_graceful_in(None);

        assert!(report.is_ok());
        assert_eq!(report.source, ConfigSource::Default);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // P0-5: Project Access Tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_verify_project_access_readable() {
        let temp = TempDir::new().unwrap();
        // Create some files
        std::fs::write(temp.path().join("test.txt"), "hello").unwrap();

        let report = verify_project_access_in(temp.path()).unwrap();

        assert!(report.is_ok());
        assert!(report.readable);
        assert!(report.file_count > 0);
    }

    #[test]
    fn test_verify_project_access_workflow_count() {
        let temp = TempDir::new().unwrap();
        // Create workflow files
        std::fs::write(temp.path().join("hello.nika.yaml"), "schema: test").unwrap();
        std::fs::write(temp.path().join("world.nika.yaml"), "schema: test").unwrap();
        std::fs::write(temp.path().join("other.yaml"), "not a workflow").unwrap();

        let report = verify_project_access_in(temp.path()).unwrap();

        assert!(report.is_ok());
        assert_eq!(report.workflow_count, 2);
    }

    #[test]
    fn test_verify_project_access_empty_dir() {
        let temp = TempDir::new().unwrap();
        let report = verify_project_access_in(temp.path()).unwrap();

        assert!(report.is_ok());
        assert!(report.readable);
        assert_eq!(report.workflow_count, 0);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Combined Startup Report Tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_startup_report_is_ok_all_pass() {
        let report = StartupReport {
            directories: DirectoryReport {
                nika_dir: Some(PathBuf::from(".nika")),
                created: vec!["sessions".into()],
                existed: vec![],
                errors: vec![],
            },
            schema: SchemaReport {
                schema_loaded: true,
                schema_path: Some(PathBuf::from("schema.json")),
                error: None,
            },
            config: ConfigReport {
                loaded: true,
                source: ConfigSource::Default,
                warning: None,
                config_path: None,
            },
            project: ProjectReport {
                project_dir: Some(PathBuf::from(".")),
                readable: true,
                file_count: 10,
                workflow_count: 2,
                error: None,
            },
            started_at: Instant::now(),
            duration: Duration::from_millis(50),
        };

        assert!(report.is_ok());
        assert!(report.warnings().is_empty());
    }

    #[test]
    fn test_startup_report_collects_warnings() {
        let report = StartupReport {
            directories: DirectoryReport {
                errors: vec!["permission denied".into()],
                ..Default::default()
            },
            config: ConfigReport {
                loaded: true,
                warning: Some("using defaults".into()),
                ..Default::default()
            },
            schema: SchemaReport {
                error: Some("schema not found".into()),
                ..Default::default()
            },
            project: ProjectReport {
                readable: true,
                ..Default::default()
            },
            started_at: Instant::now(),
            duration: Duration::ZERO,
        };

        let warnings = report.warnings();
        assert_eq!(warnings.len(), 3);
        assert!(warnings.iter().any(|w| w.contains("permission denied")));
        assert!(warnings.iter().any(|w| w.contains("using defaults")));
        assert!(warnings.iter().any(|w| w.contains("schema not found")));
    }

    #[test]
    fn test_startup_report_summary() {
        let report = StartupReport {
            directories: DirectoryReport {
                nika_dir: Some(PathBuf::from(".nika")),
                ..Default::default()
            },
            schema: SchemaReport {
                schema_loaded: true,
                ..Default::default()
            },
            config: ConfigReport {
                loaded: true,
                ..Default::default()
            },
            project: ProjectReport {
                readable: true,
                ..Default::default()
            },
            started_at: Instant::now(),
            duration: Duration::from_millis(25),
        };

        let summary = report.summary();
        assert!(summary.contains("OK"));
        assert!(summary.contains("✓"));
    }
}
