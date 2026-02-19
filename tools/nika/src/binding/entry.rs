//! Wiring Spec - YAML types for explicit data wiring (v0.5)
//!
//! Unified syntax: `alias: task.path [?? default]`
//!
//! Examples:
//! - `forecast: weather.summary` -> simple path (eager)
//! - `temp: weather.data.temp ?? 20` -> with numeric default
//! - `name: user.profile ?? "Anonymous"` -> with string default (quoted)
//! - `cfg: x ?? {"a": 1}` -> with object default
//!
//! Extended syntax for lazy bindings (v0.5 MVP 8):
//! - `alias: { path: task.result, lazy: true }` -> deferred resolution
//! - `alias: { path: task.result, lazy: true, default: "fallback" }` -> lazy with default

use rustc_hash::FxHashMap;
use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::Deserialize;
use serde_json::Value;
use std::fmt;

use crate::error::NikaError;

/// Wiring spec - map of alias to entry (YAML `use:` block)
pub type WiringSpec = FxHashMap<String, UseEntry>;

/// Unified use entry - supports both string and extended object syntax (v0.5)
///
/// String syntax: `task.path [?? default]`
/// - path: "task.field.subfield" or "task" for entire output
/// - default: Optional JSON literal after ??
///
/// Extended syntax (YAML object):
/// - path: "task.field" (required)
/// - lazy: bool (optional, default false)
/// - default: JSON value (optional)
#[derive(Debug, Clone, PartialEq)]
pub struct UseEntry {
    /// Full path: "task.field.subfield" or "task" for entire output
    pub path: String,
    /// Optional default value (JSON literal)
    pub default: Option<Value>,
    /// Lazy flag - if true, resolution is deferred until first access (v0.5)
    pub lazy: bool,
}

impl UseEntry {
    /// Create a new UseEntry with just a path (eager resolution)
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            default: None,
            lazy: false,
        }
    }

    /// Create a new UseEntry with path and default (eager resolution)
    pub fn with_default(path: impl Into<String>, default: Value) -> Self {
        Self {
            path: path.into(),
            default: Some(default),
            lazy: false,
        }
    }

    /// Create a new lazy UseEntry (deferred resolution, v0.5)
    pub fn new_lazy(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            default: None,
            lazy: true,
        }
    }

    /// Create a new lazy UseEntry with default (deferred resolution, v0.5)
    pub fn lazy_with_default(path: impl Into<String>, default: Value) -> Self {
        Self {
            path: path.into(),
            default: Some(default),
            lazy: true,
        }
    }

    /// Check if this binding is lazy (deferred resolution)
    pub fn is_lazy(&self) -> bool {
        self.lazy
    }

    /// Extract the task ID from the path (first segment before '.')
    pub fn task_id(&self) -> &str {
        self.path.split('.').next().unwrap_or(&self.path)
    }
}

/// Parse a use entry string into UseEntry (eager resolution)
///
/// Syntax: `task.path [?? default]`
/// - If `??` found outside quotes, splits into path and default
/// - Default is parsed as JSON literal (strings must be quoted)
/// - String syntax always produces eager bindings (lazy=false)
pub fn parse_use_entry(s: &str) -> Result<UseEntry, NikaError> {
    let s = s.trim();

    if s.is_empty() {
        return Err(NikaError::InvalidPath {
            path: String::new(),
        });
    }

    match find_operator_outside_quotes(s, "??") {
        Some(idx) => {
            let path = s[..idx].trim();

            if path.is_empty() {
                return Err(NikaError::InvalidPath {
                    path: s.to_string(),
                });
            }

            let default_str = s[idx + 2..].trim();
            let default =
                serde_json::from_str(default_str).map_err(|e| NikaError::InvalidDefault {
                    raw: default_str.to_string(),
                    reason: e.to_string(),
                })?;

            Ok(UseEntry {
                path: path.to_string(),
                default: Some(default),
                lazy: false,
            })
        }
        None => Ok(UseEntry {
            path: s.to_string(),
            default: None,
            lazy: false,
        }),
    }
}

/// Find the position of an operator outside of quoted strings
///
/// Handles double-quoted strings ("...") and ignores operator inside quotes.
/// Example: `x ?? "What?? Really??"` -> finds first ?? at position 2
fn find_operator_outside_quotes(s: &str, op: &str) -> Option<usize> {
    let mut in_quotes = false;
    let mut escape_next = false;
    let mut byte_pos = 0;

    for ch in s.chars() {
        if escape_next {
            escape_next = false;
        } else if ch == '\\' {
            escape_next = true;
        } else if ch == '"' {
            in_quotes = !in_quotes;
        } else if !in_quotes && s[byte_pos..].starts_with(op) {
            return Some(byte_pos);
        }

        byte_pos += ch.len_utf8();
    }

    None
}

/// Custom deserializer for UseEntry (v0.5)
///
/// Accepts two formats:
/// 1. String: `task.path [?? default]` → eager binding
/// 2. Object: `{path: "task.path", lazy: true, default: ...}` → lazy binding (v0.5)
impl<'de> Deserialize<'de> for UseEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(UseEntryVisitor)
    }
}

struct UseEntryVisitor;

impl<'de> Visitor<'de> for UseEntryVisitor {
    type Value = UseEntry;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter
            .write_str("a string 'task.path [?? default]' or an object {path, lazy?, default?}")
    }

    /// Handle string format: "task.path [?? default]" (eager)
    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        parse_use_entry(value).map_err(|e| de::Error::custom(e.to_string()))
    }

    /// Handle object format: {path, lazy?, default?} (v0.5 extended syntax)
    fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let mut path: Option<String> = None;
        let mut lazy: Option<bool> = None;
        let mut default: Option<Value> = None;

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "path" => {
                    if path.is_some() {
                        return Err(de::Error::duplicate_field("path"));
                    }
                    path = Some(map.next_value()?);
                }
                "lazy" => {
                    if lazy.is_some() {
                        return Err(de::Error::duplicate_field("lazy"));
                    }
                    lazy = Some(map.next_value()?);
                }
                "default" => {
                    if default.is_some() {
                        return Err(de::Error::duplicate_field("default"));
                    }
                    default = Some(map.next_value()?);
                }
                _ => {
                    // Ignore unknown fields for forward compatibility
                    let _ = map.next_value::<de::IgnoredAny>()?;
                }
            }
        }

        let path = path.ok_or_else(|| de::Error::missing_field("path"))?;

        Ok(UseEntry {
            path,
            default,
            lazy: lazy.unwrap_or(false),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ═══════════════════════════════════════════════════════════════
    // parse_use_entry() tests - TDD: write these first
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn parse_simple_path() {
        let entry = parse_use_entry("weather.summary").unwrap();
        assert_eq!(entry.path, "weather.summary");
        assert_eq!(entry.default, None);
    }

    #[test]
    fn parse_simple_task_only() {
        let entry = parse_use_entry("weather").unwrap();
        assert_eq!(entry.path, "weather");
        assert_eq!(entry.default, None);
    }

    #[test]
    fn parse_nested_path() {
        let entry = parse_use_entry("weather.data.temperature.celsius").unwrap();
        assert_eq!(entry.path, "weather.data.temperature.celsius");
        assert_eq!(entry.default, None);
    }

    #[test]
    fn parse_with_default_number() {
        let entry = parse_use_entry("x.y ?? 0").unwrap();
        assert_eq!(entry.path, "x.y");
        assert_eq!(entry.default, Some(json!(0)));
    }

    #[test]
    fn parse_with_default_negative_number() {
        let entry = parse_use_entry("score ?? -1").unwrap();
        assert_eq!(entry.path, "score");
        assert_eq!(entry.default, Some(json!(-1)));
    }

    #[test]
    fn parse_with_default_float() {
        let entry = parse_use_entry("rate ?? 0.5").unwrap();
        assert_eq!(entry.path, "rate");
        assert_eq!(entry.default, Some(json!(0.5)));
    }

    #[test]
    fn parse_with_default_string() {
        let entry = parse_use_entry(r#"x.y ?? "Anon""#).unwrap();
        assert_eq!(entry.path, "x.y");
        assert_eq!(entry.default, Some(json!("Anon")));
    }

    #[test]
    fn parse_with_default_empty_string() {
        let entry = parse_use_entry(r#"name ?? """#).unwrap();
        assert_eq!(entry.path, "name");
        assert_eq!(entry.default, Some(json!("")));
    }

    #[test]
    fn parse_with_default_bool_true() {
        let entry = parse_use_entry("enabled ?? true").unwrap();
        assert_eq!(entry.path, "enabled");
        assert_eq!(entry.default, Some(json!(true)));
    }

    #[test]
    fn parse_with_default_bool_false() {
        let entry = parse_use_entry("enabled ?? false").unwrap();
        assert_eq!(entry.path, "enabled");
        assert_eq!(entry.default, Some(json!(false)));
    }

    #[test]
    fn parse_with_default_null() {
        let entry = parse_use_entry("value ?? null").unwrap();
        assert_eq!(entry.path, "value");
        assert_eq!(entry.default, Some(json!(null)));
    }

    #[test]
    fn parse_with_default_object() {
        let entry = parse_use_entry(r#"x ?? {"a": 1, "b": 2}"#).unwrap();
        assert_eq!(entry.path, "x");
        assert_eq!(entry.default, Some(json!({"a": 1, "b": 2})));
    }

    #[test]
    fn parse_with_default_array() {
        let entry = parse_use_entry(r#"tags ?? ["untagged"]"#).unwrap();
        assert_eq!(entry.path, "tags");
        assert_eq!(entry.default, Some(json!(["untagged"])));
    }

    #[test]
    fn parse_with_default_nested_object() {
        let entry = parse_use_entry(r#"cfg ?? {"debug": false, "nested": {"a": 1}}"#).unwrap();
        assert_eq!(entry.path, "cfg");
        assert_eq!(
            entry.default,
            Some(json!({"debug": false, "nested": {"a": 1}}))
        );
    }

    #[test]
    fn parse_quotes_in_default() {
        // The ?? inside quotes should be ignored
        let entry = parse_use_entry(r#"x ?? "What?? Really??""#).unwrap();
        assert_eq!(entry.path, "x");
        assert_eq!(entry.default, Some(json!("What?? Really??")));
    }

    #[test]
    fn parse_escaped_quotes_in_default() {
        let entry = parse_use_entry(r#"x ?? "He said \"hello\"""#).unwrap();
        assert_eq!(entry.path, "x");
        assert_eq!(entry.default, Some(json!("He said \"hello\"")));
    }

    #[test]
    fn parse_with_whitespace() {
        let entry = parse_use_entry("  weather.summary  ").unwrap();
        assert_eq!(entry.path, "weather.summary");
    }

    #[test]
    fn parse_with_whitespace_around_operator() {
        let entry = parse_use_entry("x  ??  0").unwrap();
        assert_eq!(entry.path, "x");
        assert_eq!(entry.default, Some(json!(0)));
    }

    // ═══════════════════════════════════════════════════════════════
    // Error cases - TDD: these should fail appropriately
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn parse_reject_unquoted_string() {
        // "Anonymous" without quotes is invalid JSON
        let result = parse_use_entry("x ?? Anonymous");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-056"));
    }

    #[test]
    fn parse_reject_empty_path() {
        let result = parse_use_entry("");
        assert!(result.is_err());
    }

    #[test]
    fn parse_reject_only_operator() {
        let result = parse_use_entry("??");
        assert!(result.is_err());
    }

    #[test]
    fn parse_reject_empty_path_with_default() {
        let result = parse_use_entry("?? 0");
        assert!(result.is_err());
    }

    #[test]
    fn parse_reject_invalid_json_default() {
        // Missing closing brace
        let result = parse_use_entry(r#"x ?? {"a": 1"#);
        assert!(result.is_err());
    }

    // ═══════════════════════════════════════════════════════════════
    // task_id() extraction tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn task_id_simple() {
        let entry = UseEntry::new("weather");
        assert_eq!(entry.task_id(), "weather");
    }

    #[test]
    fn task_id_with_path() {
        let entry = UseEntry::new("weather.summary");
        assert_eq!(entry.task_id(), "weather");
    }

    #[test]
    fn task_id_with_nested_path() {
        let entry = UseEntry::new("weather.data.temp.celsius");
        assert_eq!(entry.task_id(), "weather");
    }

    // ═══════════════════════════════════════════════════════════════
    // YAML deserialization tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn yaml_parse_simple() {
        let yaml = "forecast: weather.summary";
        let wiring: WiringSpec = serde_yaml::from_str(yaml).unwrap();
        let entry = wiring.get("forecast").unwrap();
        assert_eq!(entry.path, "weather.summary");
        assert_eq!(entry.default, None);
    }

    #[test]
    fn yaml_parse_with_default() {
        let yaml = r#"temp: weather.temp ?? 20"#;
        let wiring: WiringSpec = serde_yaml::from_str(yaml).unwrap();
        let entry = wiring.get("temp").unwrap();
        assert_eq!(entry.path, "weather.temp");
        assert_eq!(entry.default, Some(json!(20)));
    }

    #[test]
    fn yaml_parse_multiple_entries() {
        let yaml = r#"
forecast: weather.summary
temp: weather.temp ?? 20
name: user.name ?? "Anonymous"
"#;
        let wiring: WiringSpec = serde_yaml::from_str(yaml).unwrap();

        let forecast = wiring.get("forecast").unwrap();
        assert_eq!(forecast.path, "weather.summary");
        assert_eq!(forecast.default, None);

        let temp = wiring.get("temp").unwrap();
        assert_eq!(temp.path, "weather.temp");
        assert_eq!(temp.default, Some(json!(20)));

        let name = wiring.get("name").unwrap();
        assert_eq!(name.path, "user.name");
        assert_eq!(name.default, Some(json!("Anonymous")));
    }

    #[test]
    fn yaml_parse_complex_defaults() {
        // Note: Complex JSON defaults need to be quoted in YAML
        // because {} and [] have special meaning in YAML
        let yaml = r#"
cfg: 'settings ?? {"debug": false}'
tags: 'meta.tags ?? ["default"]'
"#;
        let wiring: WiringSpec = serde_yaml::from_str(yaml).unwrap();

        let cfg = wiring.get("cfg").unwrap();
        assert_eq!(cfg.default, Some(json!({"debug": false})));

        let tags = wiring.get("tags").unwrap();
        assert_eq!(tags.default, Some(json!(["default"])));
    }

    // ═══════════════════════════════════════════════════════════════
    // find_operator_outside_quotes() tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn find_op_simple() {
        assert_eq!(find_operator_outside_quotes("a ?? b", "??"), Some(2));
    }

    #[test]
    fn find_op_no_match() {
        assert_eq!(find_operator_outside_quotes("a.b.c", "??"), None);
    }

    #[test]
    fn find_op_inside_quotes_ignored() {
        // The ?? inside quotes should be ignored
        let s = r#"x ?? "What?? Really??""#;
        assert_eq!(find_operator_outside_quotes(s, "??"), Some(2));
    }

    #[test]
    fn find_op_only_inside_quotes() {
        let s = r#""a ?? b""#;
        assert_eq!(find_operator_outside_quotes(s, "??"), None);
    }

    #[test]
    fn find_op_multiple_operators() {
        // Should find first one outside quotes
        let s = "a ?? b ?? c";
        assert_eq!(find_operator_outside_quotes(s, "??"), Some(2));
    }

    #[test]
    fn find_op_with_escaped_quote() {
        let s = r#"x ?? "He said \"??\"""#;
        assert_eq!(find_operator_outside_quotes(s, "??"), Some(2));
    }
}
