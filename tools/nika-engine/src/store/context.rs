// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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

    /// Loaded skills by alias (from `skills:` block)
    ///
    /// Available as `{{skills.NAME}}` in templates.
    pub skills: FxHashMap<String, Value>,
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

    /// Get a skill by alias
    pub fn get_skill(&self, alias: &str) -> Option<&Value> {
        self.skills.get(alias)
    }

    /// Check if context is empty
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.session.is_none() && self.skills.is_empty()
    }

    /// Get number of loaded files
    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}
