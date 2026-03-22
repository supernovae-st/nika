//! Loaded context data from workflow `context:` block
//!
//! This module defines the pure data structure for loaded context files.
//! The loading logic lives in `runtime::context_loader`.

use rustc_hash::FxHashMap;
use serde_json::Value;

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
