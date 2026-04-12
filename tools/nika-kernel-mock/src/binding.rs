// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MockBindingStore — in-memory binding store for resolve.rs tests.
//!
//! Builder pattern: `MockBindingStore::new().with_output("task", json!({...}))`.
//! Implements `BindingStore` from nika-kernel so resolve.rs tests can run
//! without depending on the engine's RunContext.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::Value;

use nika_kernel::binding::{BindingStore, BindingStoreError};

/// In-memory mock of the BindingStore trait for unit tests.
///
/// Stores task outputs, failed tasks, inputs, context files, vault
/// credentials, and records as simple HashMaps. No media staging,
/// no DashMap concurrency, no vault encryption.
///
/// # Builder pattern
///
/// ```ignore
/// let store = MockBindingStore::new()
///     .with_output("weather", json!({"temp": 22}))
///     .with_output("news", json!({"headline": "..."}))
///     .with_failed("broken_task")
///     .with_input("topic", json!("AI"))
///     .with_context("readme", json!("# README"))
///     .with_vault("stripe", "api_key", "sk_live_xxx");
/// ```
#[derive(Debug, Clone, Default)]
pub struct MockBindingStore {
    /// Task outputs keyed by task_id.
    outputs: HashMap<String, Arc<Value>>,
    /// Task IDs that are marked as failed.
    failed: HashSet<String>,
    /// Input defaults keyed by param name (without "inputs." prefix).
    inputs: HashMap<String, Value>,
    /// Context file contents keyed by alias.
    context_files: HashMap<String, Value>,
    /// Session data (for context.session bindings).
    session: Option<Value>,
    /// Vault credentials keyed by (service, field).
    vault: HashMap<(String, String), String>,
    /// Compression records keyed by task_id (opaque JSON).
    records: HashMap<String, Value>,
    /// Skills content keyed by alias.
    skills: HashMap<String, Value>,
    /// Environment variables keyed by name (no fallback to std::env).
    env: HashMap<String, String>,
}

impl MockBindingStore {
    /// Create an empty mock store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a task output.
    pub fn with_output(mut self, task_id: &str, output: Value) -> Self {
        self.outputs
            .insert(task_id.to_string(), Arc::new(output));
        self
    }

    /// Mark a task as failed.
    pub fn with_failed(mut self, task_id: &str) -> Self {
        self.failed.insert(task_id.to_string());
        self
    }

    /// Add an input default value.
    ///
    /// The param name should NOT include the "inputs." prefix.
    /// e.g., `with_input("topic", json!("AI"))` for `inputs.topic`.
    pub fn with_input(mut self, param: &str, value: Value) -> Self {
        self.inputs.insert(param.to_string(), value);
        self
    }

    /// Add a context file.
    ///
    /// The alias should NOT include the "context.files." prefix.
    pub fn with_context(mut self, alias: &str, value: Value) -> Self {
        self.context_files.insert(alias.to_string(), value);
        self
    }

    /// Set the session data for `context.session` bindings.
    pub fn with_session(mut self, value: Value) -> Self {
        self.session = Some(value);
        self
    }

    /// Add a vault credential.
    pub fn with_vault(mut self, service: &str, field: &str, value: &str) -> Self {
        self.vault.insert(
            (service.to_string(), field.to_string()),
            value.to_string(),
        );
        self
    }

    /// Add a compression record (opaque JSON value).
    pub fn with_record(mut self, task_id: &str, record: Value) -> Self {
        self.records.insert(task_id.to_string(), record);
        self
    }

    /// Add a skill content.
    pub fn with_skill(mut self, alias: &str, value: Value) -> Self {
        self.skills.insert(alias.to_string(), value);
        self
    }

    /// Add an environment variable value (tests do not read `std::env`).
    pub fn with_env(mut self, name: &str, value: &str) -> Self {
        self.env.insert(name.to_string(), value.to_string());
        self
    }
}

impl BindingStore for MockBindingStore {
    fn get_output(&self, task_id: &str) -> Option<Arc<Value>> {
        self.outputs.get(task_id).cloned()
    }

    fn is_failed(&self, task_id: &str) -> bool {
        self.failed.contains(task_id)
    }

    fn resolve_path(&self, path: &str) -> Option<Value> {
        let mut parts = path.splitn(2, '.');
        let task_id = parts.next()?;

        let output = self.outputs.get(task_id)?;

        let Some(remaining) = parts.next() else {
            return Some((**output).clone());
        };

        // Simple dot-path navigation for mock (no media staging)
        navigate_json(output, remaining)
    }

    fn get_record(&self, task_id: &str) -> Option<Value> {
        self.records.get(task_id).cloned()
    }

    fn resolve_input_path(&self, path: &str) -> Option<Value> {
        // Expected format: "inputs.param" or "inputs.param.field"
        let parts: Vec<&str> = path.split('.').collect();
        if parts.len() < 2 || parts[0] != "inputs" {
            return None;
        }

        let param = parts[1];
        let value = self.inputs.get(param)?;

        // Extract default from full-form input definitions
        let default_value = if let Some(obj) = value.as_object() {
            if let Some(default_val) = obj.get("default").or_else(|| obj.get("value")) {
                default_val.clone()
            } else if obj.contains_key("type") || obj.contains_key("description") {
                return None;
            } else {
                value.clone()
            }
        } else {
            value.clone()
        };

        if parts.len() == 2 {
            Some(default_value)
        } else {
            // Navigate nested path
            let remaining = parts[2..].join(".");
            navigate_json(&default_value, &remaining)
        }
    }

    fn resolve_context_path(&self, path: &str) -> Option<Value> {
        // Expected format: "context.files.alias", "context.session", etc.
        let parts: Vec<&str> = path.split('.').collect();
        if parts.len() < 2 {
            return None;
        }

        match parts[1] {
            "files" => {
                if parts.len() < 3 {
                    return None;
                }
                let alias = parts[2];
                let value = self.context_files.get(alias)?;
                if parts.len() == 3 {
                    Some(value.clone())
                } else {
                    let remaining = parts[3..].join(".");
                    navigate_json(value, &remaining)
                }
            }
            "session" => {
                let session = self.session.as_ref()?;
                if parts.len() == 2 {
                    Some(session.clone())
                } else {
                    let remaining = parts[2..].join(".");
                    navigate_json(session, &remaining)
                }
            }
            // Shorthand: context.<alias> → context.files.<alias>
            alias => {
                let value = self.context_files.get(alias)?;
                if parts.len() == 2 {
                    Some(value.clone())
                } else {
                    let remaining = parts[2..].join(".");
                    navigate_json(value, &remaining)
                }
            }
        }
    }

    fn resolve_skills_path(&self, path: &str) -> Option<Value> {
        let parts: Vec<&str> = path.split('.').collect();
        if parts.is_empty() {
            return None;
        }
        let alias = parts[0];
        let value = self.skills.get(alias)?;
        if parts.len() == 1 {
            Some(value.clone())
        } else {
            let remaining = parts[1..].join(".");
            navigate_json(value, &remaining)
        }
    }

    fn vault_get_credential(
        &self,
        service: &str,
        field: &str,
    ) -> Result<Option<String>, BindingStoreError> {
        Ok(self
            .vault
            .get(&(service.to_string(), field.to_string()))
            .cloned())
    }

    fn resolve_env(&self, var_name: &str) -> Option<String> {
        self.env
            .get(var_name)
            .cloned()
            .filter(|v| !v.trim().is_empty())
    }
}

/// Simple dot-path navigation through a JSON value.
///
/// Handles "field.sub.nested" paths with basic array index support "[N]".
///
/// # Limitations vs production
///
/// This is a minimal walker, not a full replacement for nika-core's jsonpath:
/// - Does NOT auto-parse JSON-encoded strings (unlike `navigate_segments`
///   in resolve.rs which calls `jsonpath::try_parse_json_str`). Tests that
///   store `json!("{\"key\":\"val\"}")` and navigate `task.key` will get
///   `None` here, but the production path would return `Some(json!("val"))`.
/// - ASCII-only field names before `[` (uses byte slicing, not char indices).
/// - No support for escaped dots or nested brackets like `items[0][1]`.
///
/// For tests that need the full jsonpath behavior, use `RunContext` instead.
fn navigate_json(value: &Value, path: &str) -> Option<Value> {
    let mut current = value;
    for segment in path.split('.') {
        // Check for array index: "items[0]" or just "[0]"
        if let Some(bracket_pos) = segment.find('[') {
            let field = &segment[..bracket_pos];
            if !field.is_empty() {
                current = current.as_object()?.get(field)?;
            }
            // Parse index
            let idx_str = segment[bracket_pos + 1..].strip_suffix(']')?;
            let idx: usize = idx_str.parse().ok()?;
            current = current.as_array()?.get(idx)?;
        } else {
            current = current.as_object()?.get(segment)?;
        }
    }
    Some(current.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_store_returns_none() {
        let store = MockBindingStore::new();
        assert!(store.get_output("missing").is_none());
        assert!(!store.is_failed("missing"));
        assert!(store.resolve_path("missing").is_none());
        assert!(store.get_record("missing").is_none());
    }

    #[test]
    fn with_output_roundtrip() {
        let store = MockBindingStore::new()
            .with_output("weather", json!({"temp": 22, "city": "Paris"}));

        let output = store.get_output("weather").unwrap();
        assert_eq!(*output, json!({"temp": 22, "city": "Paris"}));
    }

    #[test]
    fn resolve_path_dot_notation() {
        let store = MockBindingStore::new()
            .with_output("weather", json!({"data": {"temp": 22}}));

        assert_eq!(
            store.resolve_path("weather.data.temp"),
            Some(json!(22))
        );
        assert_eq!(
            store.resolve_path("weather"),
            Some(json!({"data": {"temp": 22}}))
        );
        assert!(store.resolve_path("weather.missing").is_none());
    }

    #[test]
    fn is_failed_tracking() {
        let store = MockBindingStore::new()
            .with_output("ok", json!("fine"))
            .with_failed("broken");

        assert!(!store.is_failed("ok"));
        assert!(store.is_failed("broken"));
    }

    #[test]
    fn resolve_input_path() {
        let store = MockBindingStore::new()
            .with_input("topic", json!("AI workflows"));

        assert_eq!(
            store.resolve_input_path("inputs.topic"),
            Some(json!("AI workflows"))
        );
        assert!(store.resolve_input_path("inputs.missing").is_none());
    }

    #[test]
    fn resolve_input_full_form() {
        let store = MockBindingStore::new()
            .with_input("count", json!({"type": "number", "default": 5}));

        assert_eq!(
            store.resolve_input_path("inputs.count"),
            Some(json!(5))
        );
    }

    #[test]
    fn resolve_context_files() {
        let store = MockBindingStore::new()
            .with_context("brand", json!("Brand Guidelines v2"));

        assert_eq!(
            store.resolve_context_path("context.files.brand"),
            Some(json!("Brand Guidelines v2"))
        );
        // Shorthand
        assert_eq!(
            store.resolve_context_path("context.brand"),
            Some(json!("Brand Guidelines v2"))
        );
    }

    #[test]
    fn resolve_context_session() {
        let store = MockBindingStore::new()
            .with_session(json!({"focus": "testing", "mode": "dev"}));

        assert_eq!(
            store.resolve_context_path("context.session"),
            Some(json!({"focus": "testing", "mode": "dev"}))
        );
        assert_eq!(
            store.resolve_context_path("context.session.focus"),
            Some(json!("testing"))
        );
    }

    #[test]
    fn vault_credential_lookup() {
        let store = MockBindingStore::new()
            .with_vault("stripe", "api_key", "sk_live_xxx");

        let cred = store.vault_get_credential("stripe", "api_key").unwrap();
        assert_eq!(cred, Some("sk_live_xxx".to_string()));

        let missing = store.vault_get_credential("stripe", "missing").unwrap();
        assert_eq!(missing, None);
    }

    #[test]
    fn record_roundtrip() {
        let record = json!({"summary": "Test summary", "confidence": 0.95});
        let store = MockBindingStore::new()
            .with_record("task1", record.clone());

        assert_eq!(store.get_record("task1"), Some(record));
        assert!(store.get_record("missing").is_none());
    }

    #[test]
    fn builder_chain() {
        let store = MockBindingStore::new()
            .with_output("step1", json!({"result": "ok"}))
            .with_output("step2", json!(42))
            .with_failed("step3")
            .with_input("topic", json!("AI"))
            .with_context("readme", json!("# Hello"))
            .with_vault("aws", "key", "AKIA...");

        assert!(store.get_output("step1").is_some());
        assert!(store.get_output("step2").is_some());
        assert!(store.is_failed("step3"));
        assert_eq!(store.resolve_input_path("inputs.topic"), Some(json!("AI")));
        assert_eq!(
            store.resolve_context_path("context.files.readme"),
            Some(json!("# Hello"))
        );
        assert_eq!(
            store.vault_get_credential("aws", "key").unwrap(),
            Some("AKIA...".to_string())
        );
    }

    #[test]
    fn skills_path_resolution() {
        let store = MockBindingStore::new()
            .with_skill("tone", json!({"style": "formal", "voice": "active"}));

        assert_eq!(
            store.resolve_skills_path("tone"),
            Some(json!({"style": "formal", "voice": "active"}))
        );
        assert_eq!(
            store.resolve_skills_path("tone.style"),
            Some(json!("formal"))
        );
        assert!(store.resolve_skills_path("missing").is_none());
    }

    #[test]
    fn env_resolution() {
        let store = MockBindingStore::new()
            .with_env("API_KEY", "sk-live-xxx")
            .with_env("EMPTY", "   ");

        assert_eq!(store.resolve_env("API_KEY"), Some("sk-live-xxx".to_string()));
        assert!(store.resolve_env("EMPTY").is_none());
        assert!(store.resolve_env("MISSING").is_none());
    }

    #[test]
    fn array_index_in_path() {
        let store = MockBindingStore::new()
            .with_output("data", json!({"items": [{"name": "a"}, {"name": "b"}]}));

        assert_eq!(
            store.resolve_path("data.items[0].name"),
            Some(json!("a"))
        );
        assert_eq!(
            store.resolve_path("data.items[1].name"),
            Some(json!("b"))
        );
        assert!(store.resolve_path("data.items[99].name").is_none());
    }
}
