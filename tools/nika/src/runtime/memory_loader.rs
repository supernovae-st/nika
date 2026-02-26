//! Memory Loader - Load files at workflow start (v0.6)
//!
//! Loads files declared in the `memory:` block at workflow start.
//! Files are made available via `{{memory.files.alias}}` bindings.
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
//! memory:
//!   files:
//!     brand: ./context/brand.md        # String
//!     persona: ./context/persona.json  # JSON object
//!     examples: ./context/*.md         # Array of strings
//!   session: .nika/sessions/prev.json  # Session restore
//! ```

use std::path::Path;

use globset::GlobBuilder;
use ignore::WalkBuilder;
use rustc_hash::FxHashMap;
use serde_json::Value;

use crate::ast::memory::MemoryConfig;
use crate::error::NikaError;

// ═══════════════════════════════════════════════════════════════════════════
// LOADED MEMORY
// ═══════════════════════════════════════════════════════════════════════════

/// Loaded memory context from workflow `memory:` block
///
/// Contains all files loaded at workflow start, keyed by alias.
#[derive(Debug, Clone, Default)]
pub struct LoadedMemory {
    /// Loaded files by alias
    ///
    /// - Single files: `Value::String` (text) or `Value::Object` (JSON/YAML)
    /// - Glob patterns: `Value::Array` of strings
    pub files: FxHashMap<String, Value>,

    /// Loaded session data (if any)
    pub session: Option<Value>,
}

impl LoadedMemory {
    /// Create an empty LoadedMemory
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

    /// Check if memory is empty
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.session.is_none()
    }

    /// Get number of loaded files
    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// MEMORY LOADER
// ═══════════════════════════════════════════════════════════════════════════

/// Load memory files at workflow start
///
/// # Arguments
///
/// * `config` - Memory configuration from workflow
/// * `base_path` - Base directory for resolving relative paths
///
/// # Returns
///
/// Loaded memory with all files resolved
///
/// # Errors
///
/// Returns `NikaError` if:
/// - File not found
/// - Invalid JSON/YAML
/// - Invalid glob pattern
pub async fn load_memory(
    config: &MemoryConfig,
    base_path: &Path,
) -> Result<LoadedMemory, NikaError> {
    let mut memory = LoadedMemory::new();

    // Load each file entry
    for (alias, path_pattern) in &config.files {
        let value = if is_glob_pattern(path_pattern) {
            // Glob pattern → array of strings
            load_glob_files(path_pattern, base_path).await?
        } else {
            // Single file
            let full_path = base_path.join(path_pattern);
            load_single_file(&full_path).await?
        };
        memory.files.insert(alias.to_string(), value);
    }

    // Load session if specified
    if let Some(session_path) = &config.session {
        let full_path = base_path.join(session_path);
        if full_path.exists() {
            let content = tokio::fs::read_to_string(&full_path).await.map_err(|e| {
                NikaError::MemoryLoadError {
                    alias: "session".to_string(),
                    path: full_path.display().to_string(),
                    reason: e.to_string(),
                }
            })?;
            let session: Value =
                serde_json::from_str(&content).map_err(|e| NikaError::MemoryLoadError {
                    alias: "session".to_string(),
                    path: full_path.display().to_string(),
                    reason: format!("Invalid JSON: {}", e),
                })?;
            memory.session = Some(session);
        }
    }

    Ok(memory)
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
            .map_err(|e| NikaError::MemoryLoadError {
                alias: String::new(),
                path: path.display().to_string(),
                reason: e.to_string(),
            })?;

    // Parse based on extension
    match path.extension().and_then(|e| e.to_str()) {
        Some("json") => serde_json::from_str(&content).map_err(|e| NikaError::MemoryLoadError {
            alias: String::new(),
            path: path.display().to_string(),
            reason: format!("Invalid JSON: {}", e),
        }),
        Some("yaml") | Some("yml") => {
            serde_yaml::from_str(&content).map_err(|e| NikaError::MemoryLoadError {
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

    // Build glob matcher
    let glob = GlobBuilder::new(file_pattern)
        .literal_separator(true)
        .build()
        .map_err(|e| NikaError::MemoryLoadError {
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
                    .map_err(|e| NikaError::MemoryLoadError {
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
    fn test_loaded_memory_default() {
        let memory = LoadedMemory::default();
        assert!(memory.is_empty());
        assert_eq!(memory.file_count(), 0);
    }

    #[test]
    fn test_loaded_memory_get_file() {
        let mut memory = LoadedMemory::new();
        memory
            .files
            .insert("test".to_string(), Value::String("content".to_string()));

        assert!(memory.get_file("test").is_some());
        assert!(memory.get_file("nonexistent").is_none());
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
    async fn test_load_memory_full() {
        let temp_dir = TempDir::new().unwrap();

        // Create files
        fs::write(temp_dir.path().join("brand.md"), "# Brand Guide")
            .await
            .unwrap();
        fs::write(temp_dir.path().join("persona.json"), r#"{"name": "Agent"}"#)
            .await
            .unwrap();

        let config = MemoryConfig {
            files: {
                let mut m = FxHashMap::default();
                m.insert("brand".to_string(), "brand.md".to_string());
                m.insert("persona".to_string(), "persona.json".to_string());
                m
            },
            session: None,
        };

        let memory = load_memory(&config, temp_dir.path()).await.unwrap();
        assert_eq!(memory.file_count(), 2);
        assert!(memory.get_file("brand").is_some());
        assert!(memory.get_file("persona").is_some());
    }

    #[tokio::test]
    async fn test_load_memory_with_session() {
        let temp_dir = TempDir::new().unwrap();
        let sessions_dir = temp_dir.path().join(".nika/sessions");
        fs::create_dir_all(&sessions_dir).await.unwrap();
        fs::write(
            sessions_dir.join("prev.json"),
            r#"{"focus_areas": ["rust", "ai"]}"#,
        )
        .await
        .unwrap();

        let config = MemoryConfig {
            files: FxHashMap::default(),
            session: Some(".nika/sessions/prev.json".to_string()),
        };

        let memory = load_memory(&config, temp_dir.path()).await.unwrap();
        assert!(memory.session.is_some());
        let session = memory.session.as_ref().unwrap();
        assert!(session["focus_areas"].is_array());
    }

    #[tokio::test]
    async fn test_load_memory_missing_session_ok() {
        let temp_dir = TempDir::new().unwrap();

        let config = MemoryConfig {
            files: FxHashMap::default(),
            session: Some(".nika/sessions/nonexistent.json".to_string()),
        };

        // Missing session file is OK (not an error)
        let memory = load_memory(&config, temp_dir.path()).await.unwrap();
        assert!(memory.session.is_none());
    }

    #[tokio::test]
    async fn test_load_memory_missing_file_error() {
        let temp_dir = TempDir::new().unwrap();

        let config = MemoryConfig {
            files: {
                let mut m = FxHashMap::default();
                m.insert("missing".to_string(), "nonexistent.md".to_string());
                m
            },
            session: None,
        };

        let result = load_memory(&config, temp_dir.path()).await;
        assert!(result.is_err());
    }
}
