//! Context Loader - Load files at workflow start (v0.9)
//!
//! Loads files declared in the `context:` block at workflow start.
//! Files are made available via `{{context.files.alias}}` bindings.
//!
//! # Supported file types
//!
//! - `.json`: Parsed as JSON object
//! - `.yaml`, `.yml`: Parsed as YAML object
//! - `.md`, `.txt`, etc.: Loaded as string
//! - Glob patterns (`*.md`): Loaded as array of strings
//!
//! # Example
//!
//! ```yaml
//! context:
//!   files:
//!     brand: ./context/brand.md        # String
//!     persona: ./context/persona.json  # JSON object
//!     examples: ./context/*.md         # Array of strings
//!   session: .nika/sessions/prev.json  # Session restore
//! ```

use crate::serde_yaml;
use std::path::Path;

use globset::GlobBuilder;
use ignore::WalkBuilder;
use rustc_hash::FxHashMap;
use serde_json::Value;

use crate::ast::context::ContextConfig;
use crate::error::NikaError;

/// Validate that a path stays within the project boundary
///
/// Prevents path traversal attacks where context file paths could escape
/// the project directory using `../` or symlinks.
///
/// # Security
///
/// This is a security-critical function. The canonical path of the loaded
/// file must be under the canonical base path to prevent:
/// - Reading sensitive files outside project (e.g., `/etc/passwd`)
/// - Loading files from parent directories
/// - Symlink attacks pointing outside project
fn validate_path_boundary(base_path: &Path, target_path: &Path) -> Result<(), NikaError> {
    let canonical_base = base_path
        .canonicalize()
        .unwrap_or_else(|_| base_path.to_path_buf());
    let canonical_target = target_path
        .canonicalize()
        .map_err(|e| NikaError::ContextLoadError {
            alias: String::new(),
            path: target_path.display().to_string(),
            reason: format!("Cannot resolve path: {}", e),
        })?;

    if !canonical_target.starts_with(&canonical_base) {
        return Err(NikaError::ContextLoadError {
            alias: String::new(),
            path: target_path.display().to_string(),
            reason: format!(
                "Path traversal detected: '{}' is outside project boundary '{}'",
                target_path.display(),
                base_path.display()
            ),
        });
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// LOADED CONTEXT
// ═══════════════════════════════════════════════════════════════════════════

/// Loaded context from workflow `context:` block
///
/// Contains all files loaded at workflow start, keyed by alias.
#[derive(Debug, Clone, Default)]
pub struct LoadedContext {
    /// Loaded files by alias
    ///
    /// - Single files: `Value::String` (text) or `Value::Object` (JSON/YAML)
    /// - Glob patterns: `Value::Array` of strings
    pub files: FxHashMap<String, Value>,

    /// Loaded session data (if any)
    pub session: Option<Value>,
}

impl LoadedContext {
    /// Create an empty LoadedContext
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a file by alias
    pub fn get_file(&self, alias: &str) -> Option<&Value> {
        self.files.get(alias)
    }

    /// Get session data
    pub fn get_session(&self) -> Option<&Value> {
        self.session.as_ref()
    }

    /// Check if context is empty
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.session.is_none()
    }

    /// Get number of loaded files
    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CONTEXT LOADER
// ═══════════════════════════════════════════════════════════════════════════

/// Load context files at workflow start
///
/// # Arguments
///
/// * `config` - Context configuration from workflow
/// * `base_path` - Base directory for resolving relative paths
///
/// # Returns
///
/// Loaded context with all files resolved
///
/// # Errors
///
/// Returns `NikaError` if:
/// - File not found
/// - Invalid JSON/YAML
/// - Invalid glob pattern
pub async fn load_context(
    config: &ContextConfig,
    base_path: &Path,
) -> Result<LoadedContext, NikaError> {
    let mut context = LoadedContext::new();

    // Load each file entry
    for (alias, path_pattern) in &config.files {
        let value = if is_glob_pattern(path_pattern) {
            // Glob pattern → array of strings (validation happens inside)
            load_glob_files(path_pattern, base_path).await?
        } else {
            // Single file - validate path boundary
            let full_path = base_path.join(path_pattern);
            validate_path_boundary(base_path, &full_path)?;
            load_single_file(&full_path).await?
        };
        context.files.insert(alias.to_string(), value);
    }

    // Load session if specified
    if let Some(session_path) = &config.session {
        let full_path = base_path.join(session_path);
        // Security: Validate session path stays within project
        if full_path.exists() {
            validate_path_boundary(base_path, &full_path)?;
            let content = tokio::fs::read_to_string(&full_path).await.map_err(|e| {
                NikaError::ContextLoadError {
                    alias: "session".to_string(),
                    path: full_path.display().to_string(),
                    reason: e.to_string(),
                }
            })?;
            let session: Value =
                serde_json::from_str(&content).map_err(|e| NikaError::ContextLoadError {
                    alias: "session".to_string(),
                    path: full_path.display().to_string(),
                    reason: format!("Invalid JSON: {}", e),
                })?;
            context.session = Some(session);
        }
    }

    Ok(context)
}

/// Check if a path pattern contains glob characters
fn is_glob_pattern(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?') || pattern.contains('[')
}

/// Load a single file
async fn load_single_file(path: &Path) -> Result<Value, NikaError> {
    let content =
        tokio::fs::read_to_string(path)
            .await
            .map_err(|e| NikaError::ContextLoadError {
                alias: String::new(),
                path: path.display().to_string(),
                reason: e.to_string(),
            })?;

    // Parse based on extension
    match path.extension().and_then(|e| e.to_str()) {
        Some("json") => serde_json::from_str(&content).map_err(|e| NikaError::ContextLoadError {
            alias: String::new(),
            path: path.display().to_string(),
            reason: format!("Invalid JSON: {}", e),
        }),
        Some("yaml") | Some("yml") => {
            serde_yaml::from_str(&content).map_err(|e| NikaError::ContextLoadError {
                alias: String::new(),
                path: path.display().to_string(),
                reason: format!("Invalid YAML: {}", e),
            })
        }
        _ => Ok(Value::String(content)),
    }
}

/// Load files matching a glob pattern
async fn load_glob_files(pattern: &str, base_path: &Path) -> Result<Value, NikaError> {
    // Extract directory and pattern parts
    // e.g., "./context/*.md" → base = "./context", pattern = "*.md"
    let pattern_path = Path::new(pattern);
    let parent = pattern_path.parent().unwrap_or(Path::new("."));
    let file_pattern = pattern_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("*");

    let search_dir = base_path.join(parent);

    // Security: Validate glob search directory stays within project
    if search_dir.exists() {
        validate_path_boundary(base_path, &search_dir)?;
    }

    // Build glob matcher
    let glob = GlobBuilder::new(file_pattern)
        .literal_separator(true)
        .build()
        .map_err(|e| NikaError::ContextLoadError {
            alias: String::new(),
            path: pattern.to_string(),
            reason: format!("Invalid glob pattern: {}", e),
        })?
        .compile_matcher();

    // Walk directory and collect matches
    let mut results = Vec::new();

    if !search_dir.exists() {
        return Ok(Value::Array(Vec::new()));
    }

    let walker = WalkBuilder::new(&search_dir)
        .hidden(false)
        .max_depth(Some(1)) // Only immediate directory
        .build();

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };

        if glob.is_match(file_name) {
            let content =
                tokio::fs::read_to_string(path)
                    .await
                    .map_err(|e| NikaError::ContextLoadError {
                        alias: String::new(),
                        path: path.display().to_string(),
                        reason: e.to_string(),
                    })?;
            results.push(Value::String(content));
        }
    }

    Ok(Value::Array(results))
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;

    #[test]
    fn test_loaded_context_default() {
        let context = LoadedContext::default();
        assert!(context.is_empty());
        assert_eq!(context.file_count(), 0);
    }

    #[test]
    fn test_loaded_context_get_file() {
        let mut context = LoadedContext::new();
        context
            .files
            .insert("test".to_string(), Value::String("content".to_string()));

        assert!(context.get_file("test").is_some());
        assert!(context.get_file("nonexistent").is_none());
    }

    #[test]
    fn test_is_glob_pattern() {
        assert!(is_glob_pattern("*.md"));
        assert!(is_glob_pattern("**/*.rs"));
        assert!(is_glob_pattern("file?.txt"));
        assert!(is_glob_pattern("[abc].txt"));
        assert!(!is_glob_pattern("file.txt"));
        assert!(!is_glob_pattern("./context/brand.md"));
    }

    #[tokio::test]
    async fn test_load_single_file_text() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        fs::write(&file_path, "# Hello World").await.unwrap();

        let result = load_single_file(&file_path).await.unwrap();
        assert_eq!(result, Value::String("# Hello World".to_string()));
    }

    #[tokio::test]
    async fn test_load_single_file_json() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.json");
        fs::write(&file_path, r#"{"name": "test", "value": 42}"#)
            .await
            .unwrap();

        let result = load_single_file(&file_path).await.unwrap();
        assert!(result.is_object());
        assert_eq!(result["name"], "test");
        assert_eq!(result["value"], 42);
    }

    #[tokio::test]
    async fn test_load_single_file_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.yaml");
        fs::write(&file_path, "name: test\nvalue: 42")
            .await
            .unwrap();

        let result = load_single_file(&file_path).await.unwrap();
        assert!(result.is_object());
        assert_eq!(result["name"], "test");
        assert_eq!(result["value"], 42);
    }

    #[tokio::test]
    async fn test_load_glob_files() {
        let temp_dir = TempDir::new().unwrap();
        let context_dir = temp_dir.path().join("context");
        fs::create_dir(&context_dir).await.unwrap();

        // Create test files
        fs::write(context_dir.join("file1.md"), "# File 1")
            .await
            .unwrap();
        fs::write(context_dir.join("file2.md"), "# File 2")
            .await
            .unwrap();
        fs::write(context_dir.join("other.txt"), "Other file")
            .await
            .unwrap();

        let result = load_glob_files("context/*.md", temp_dir.path())
            .await
            .unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2);
    }

    #[tokio::test]
    async fn test_load_context_full() {
        let temp_dir = TempDir::new().unwrap();

        // Create files
        fs::write(temp_dir.path().join("brand.md"), "# Brand Guide")
            .await
            .unwrap();
        fs::write(temp_dir.path().join("persona.json"), r#"{"name": "Agent"}"#)
            .await
            .unwrap();

        let config = ContextConfig {
            files: {
                let mut m = FxHashMap::default();
                m.insert("brand".to_string(), "brand.md".to_string());
                m.insert("persona".to_string(), "persona.json".to_string());
                m
            },
            session: None,
        };

        let context = load_context(&config, temp_dir.path()).await.unwrap();
        assert_eq!(context.file_count(), 2);
        assert!(context.get_file("brand").is_some());
        assert!(context.get_file("persona").is_some());
    }

    #[tokio::test]
    async fn test_load_context_with_session() {
        let temp_dir = TempDir::new().unwrap();
        let sessions_dir = temp_dir.path().join(".nika/sessions");
        fs::create_dir_all(&sessions_dir).await.unwrap();
        fs::write(
            sessions_dir.join("prev.json"),
            r#"{"focus_areas": ["rust", "ai"]}"#,
        )
        .await
        .unwrap();

        let config = ContextConfig {
            files: FxHashMap::default(),
            session: Some(".nika/sessions/prev.json".to_string()),
        };

        let context = load_context(&config, temp_dir.path()).await.unwrap();
        assert!(context.session.is_some());
        let session = context.session.as_ref().unwrap();
        assert!(session["focus_areas"].is_array());
    }

    #[tokio::test]
    async fn test_load_context_missing_session_ok() {
        let temp_dir = TempDir::new().unwrap();

        let config = ContextConfig {
            files: FxHashMap::default(),
            session: Some(".nika/sessions/nonexistent.json".to_string()),
        };

        // Missing session file is OK (not an error)
        let context = load_context(&config, temp_dir.path()).await.unwrap();
        assert!(context.session.is_none());
    }

    #[tokio::test]
    async fn test_load_context_missing_file_error() {
        let temp_dir = TempDir::new().unwrap();

        let config = ContextConfig {
            files: {
                let mut m = FxHashMap::default();
                m.insert("missing".to_string(), "nonexistent.md".to_string());
                m
            },
            session: None,
        };

        let result = load_context(&config, temp_dir.path()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_load_context_path_traversal_detection() {
        let temp_dir = TempDir::new().unwrap();

        // Create a project subdirectory
        let project_dir = temp_dir.path().join("project");
        fs::create_dir(&project_dir).await.unwrap();

        // Create a secret file outside project
        fs::write(temp_dir.path().join("secret.md"), "# Secret content")
            .await
            .unwrap();

        // Try to load file using path traversal
        let config = ContextConfig {
            files: {
                let mut m = FxHashMap::default();
                m.insert("secret".to_string(), "../secret.md".to_string());
                m
            },
            session: None,
        };

        let result = load_context(&config, &project_dir).await;
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("Path traversal") || err_str.contains("outside project"),
            "Expected path traversal error, got: {}",
            err_str
        );
    }

    #[test]
    fn test_validate_path_boundary() {
        let temp_dir = TempDir::new().unwrap();
        let project = temp_dir.path().join("project");
        std::fs::create_dir_all(&project).unwrap();

        // Create files
        let valid_file = project.join("valid.md");
        std::fs::write(&valid_file, "test").unwrap();

        let outside_file = temp_dir.path().join("outside.md");
        std::fs::write(&outside_file, "test").unwrap();

        // Valid path within boundary
        assert!(validate_path_boundary(&project, &valid_file).is_ok());

        // Invalid path outside boundary
        let result = validate_path_boundary(&project, &outside_file);
        assert!(result.is_err());
    }

    // =========================================================================
    // v0.17.5: Additional Error Path Tests
    // =========================================================================

    #[tokio::test]
    async fn test_load_single_file_invalid_json() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("invalid.json");
        fs::write(&file_path, "{ not valid json ]").await.unwrap();

        let result = load_single_file(&file_path).await;
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("Invalid JSON"),
            "Expected Invalid JSON error, got: {}",
            err_str
        );
    }

    #[tokio::test]
    async fn test_load_single_file_invalid_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("invalid.yaml");
        // Invalid YAML: indentation error
        fs::write(&file_path, "name: test\n  bad: indent\n key: value")
            .await
            .unwrap();

        let result = load_single_file(&file_path).await;
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("Invalid YAML"),
            "Expected Invalid YAML error, got: {}",
            err_str
        );
    }

    #[tokio::test]
    async fn test_load_single_file_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nonexistent.txt");

        let result = load_single_file(&file_path).await;
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("No such file") || err_str.contains("nonexistent"),
            "Expected file not found error, got: {}",
            err_str
        );
    }

    #[tokio::test]
    async fn test_load_context_invalid_session_json() {
        let temp_dir = TempDir::new().unwrap();
        let sessions_dir = temp_dir.path().join(".nika/sessions");
        fs::create_dir_all(&sessions_dir).await.unwrap();
        fs::write(sessions_dir.join("bad.json"), "{ invalid json }")
            .await
            .unwrap();

        let config = ContextConfig {
            files: FxHashMap::default(),
            session: Some(".nika/sessions/bad.json".to_string()),
        };

        let result = load_context(&config, temp_dir.path()).await;
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("Invalid JSON"),
            "Expected Invalid JSON error for session, got: {}",
            err_str
        );
    }

    #[tokio::test]
    async fn test_load_glob_files_nonexistent_directory() {
        let temp_dir = TempDir::new().unwrap();

        // Try to glob in a directory that doesn't exist
        let result = load_glob_files("nonexistent_dir/*.md", temp_dir.path()).await;
        // Should return empty array, not error
        assert!(result.is_ok());
        let arr = result.unwrap();
        assert!(arr.is_array());
        assert_eq!(arr.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_load_glob_files_no_matches() {
        let temp_dir = TempDir::new().unwrap();
        let context_dir = temp_dir.path().join("context");
        fs::create_dir(&context_dir).await.unwrap();

        // Create a file that doesn't match the pattern
        fs::write(context_dir.join("file.txt"), "content")
            .await
            .unwrap();

        let result = load_glob_files("context/*.md", temp_dir.path()).await;
        assert!(result.is_ok());
        let arr = result.unwrap().as_array().unwrap().clone();
        assert_eq!(arr.len(), 0);
    }

    #[test]
    fn test_validate_path_boundary_nonexistent_target() {
        let temp_dir = TempDir::new().unwrap();
        let project = temp_dir.path().join("project");
        std::fs::create_dir_all(&project).unwrap();

        // Try to validate a path that doesn't exist
        let nonexistent = project.join("does_not_exist.txt");
        let result = validate_path_boundary(&project, &nonexistent);
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("Cannot resolve path"),
            "Expected cannot resolve error, got: {}",
            err_str
        );
    }

    #[tokio::test]
    async fn test_load_context_error_contains_alias() {
        let temp_dir = TempDir::new().unwrap();

        let config = ContextConfig {
            files: {
                let mut m = FxHashMap::default();
                m.insert("my_alias".to_string(), "nonexistent.md".to_string());
                m
            },
            session: None,
        };

        let result = load_context(&config, temp_dir.path()).await;
        assert!(result.is_err());
        // The error should include the file path for debugging
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("nonexistent.md"),
            "Error should include file path, got: {}",
            err_str
        );
    }
}
