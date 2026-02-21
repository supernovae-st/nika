//! Test Fixtures for Nika
//!
//! Common constants and helper functions for tests.
//! Avoids magic strings scattered across test files.
//!
//! # Usage
//!
//! ```rust,ignore
//! use nika::test_fixtures::*;
//!
//! let params = json!({
//!     "entity": TEST_ENTITY_KEY,
//!     "locale": TEST_LOCALE_FR,
//! });
//! ```

use serde_json::{json, Value};

// ═══════════════════════════════════════════════════════════════════════════
// ENTITY KEYS
// ═══════════════════════════════════════════════════════════════════════════

/// Default test entity key (QR Code AI domain)
pub const TEST_ENTITY_KEY: &str = "qr-code";

/// Alternative test entity key
pub const TEST_ENTITY_KEY_ALT: &str = "barcode";

// ═══════════════════════════════════════════════════════════════════════════
// LOCALES (BCP-47)
// ═══════════════════════════════════════════════════════════════════════════

/// French (France) locale
pub const TEST_LOCALE_FR: &str = "fr-FR";

/// English (US) locale
pub const TEST_LOCALE_EN: &str = "en-US";

/// German (Germany) locale
pub const TEST_LOCALE_DE: &str = "de-DE";

/// Spanish (Mexico) locale
pub const TEST_LOCALE_ES: &str = "es-MX";

/// Japanese locale
pub const TEST_LOCALE_JA: &str = "ja-JP";

/// Chinese (Simplified) locale
pub const TEST_LOCALE_ZH: &str = "zh-CN";

/// All test locales as array
pub const TEST_LOCALES: &[&str] = &[
    TEST_LOCALE_FR,
    TEST_LOCALE_EN,
    TEST_LOCALE_DE,
    TEST_LOCALE_ES,
    TEST_LOCALE_JA,
    TEST_LOCALE_ZH,
];

// ═══════════════════════════════════════════════════════════════════════════
// MCP SERVERS & TOOLS
// ═══════════════════════════════════════════════════════════════════════════

/// Default MCP server name
pub const TEST_MCP_SERVER: &str = "novanet";

/// Mock MCP server name
pub const TEST_MCP_SERVER_MOCK: &str = "mock";

/// NovaNet describe tool
pub const TEST_TOOL_DESCRIBE: &str = "novanet_describe";

/// NovaNet generate tool
pub const TEST_TOOL_GENERATE: &str = "novanet_generate";

/// NovaNet traverse tool
pub const TEST_TOOL_TRAVERSE: &str = "novanet_traverse";

/// NovaNet search tool
pub const TEST_TOOL_SEARCH: &str = "novanet_search";

/// NovaNet assemble tool
pub const TEST_TOOL_ASSEMBLE: &str = "novanet_assemble";

// ═══════════════════════════════════════════════════════════════════════════
// WORKFLOW TEMPLATES
// ═══════════════════════════════════════════════════════════════════════════

/// Minimal valid workflow YAML
pub const TEST_WORKFLOW_MINIMAL: &str = r#"
schema: nika/workflow@0.5
workflow: test-workflow
provider: mock
tasks:
  - id: task1
    infer: "Hello world"
"#;

/// Workflow with MCP invoke
pub const TEST_WORKFLOW_WITH_MCP: &str = r#"
schema: nika/workflow@0.5
workflow: test-mcp-workflow
provider: mock
mcp:
  novanet:
    command: "echo mock"
tasks:
  - id: describe
    invoke:
      server: novanet
      tool: novanet_describe
      params:
        entity: "qr-code"
"#;

// ═══════════════════════════════════════════════════════════════════════════
// JSON HELPERS
// ═══════════════════════════════════════════════════════════════════════════

/// Create standard entity params JSON
pub fn entity_params(entity: &str) -> Value {
    json!({ "entity": entity })
}

/// Create entity + locale params JSON
pub fn entity_locale_params(entity: &str, locale: &str) -> Value {
    json!({
        "entity": entity,
        "locale": locale
    })
}

/// Create standard describe params for default entity
pub fn describe_params() -> Value {
    entity_params(TEST_ENTITY_KEY)
}

/// Create standard generate params for default entity + locale
pub fn generate_params() -> Value {
    entity_locale_params(TEST_ENTITY_KEY, TEST_LOCALE_FR)
}

/// Create traverse params
pub fn traverse_params(start_key: &str, arc_kinds: &[&str]) -> Value {
    json!({
        "start_key": start_key,
        "arc_kinds": arc_kinds,
        "direction": "outgoing"
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// TASK IDS
// ═══════════════════════════════════════════════════════════════════════════

/// Default test task ID
pub const TEST_TASK_ID: &str = "test-task-1";

/// Alternative test task ID
pub const TEST_TASK_ID_ALT: &str = "test-task-2";

/// Parent task ID for nested tests
pub const TEST_TASK_PARENT: &str = "parent-task";

/// Child task ID for nested tests
pub const TEST_TASK_CHILD: &str = "child-task";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_params() {
        let params = entity_params(TEST_ENTITY_KEY);
        assert_eq!(params["entity"], TEST_ENTITY_KEY);
    }

    #[test]
    fn test_entity_locale_params() {
        let params = entity_locale_params(TEST_ENTITY_KEY, TEST_LOCALE_FR);
        assert_eq!(params["entity"], TEST_ENTITY_KEY);
        assert_eq!(params["locale"], TEST_LOCALE_FR);
    }

    #[test]
    fn test_traverse_params() {
        let params = traverse_params(TEST_ENTITY_KEY, &["HAS_NATIVE"]);
        assert_eq!(params["start_key"], TEST_ENTITY_KEY);
        assert!(params["arc_kinds"]
            .as_array()
            .unwrap()
            .contains(&json!("HAS_NATIVE")));
    }

    #[test]
    fn test_locales_array() {
        assert_eq!(TEST_LOCALES.len(), 6);
        assert!(TEST_LOCALES.contains(&TEST_LOCALE_FR));
        assert!(TEST_LOCALES.contains(&TEST_LOCALE_EN));
    }
}
