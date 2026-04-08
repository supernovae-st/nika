// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Binding Spec — YAML types for explicit data binding
//!
//! Unified syntax: `alias: task.path [?? default]`
//!
//! Examples:
//! - `forecast: weather.summary` -> simple path (eager)
//! - `temp: weather.data.temp ?? 20` -> with numeric default
//! - `name: user.profile ?? "Anonymous"` -> with string default (quoted)
//! - `cfg: x ?? {"a": 1}` -> with object default
//!
//! Extended syntax for lazy bindings:
//! - `alias: { path: task.result, lazy: true }` -> deferred resolution
//! - `alias: { path: task.result, lazy: true, default: "fallback" }` -> lazy with default

use rustc_hash::FxHashMap;
use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::Deserialize;
use serde_json::Value;
use std::fmt;

use crate::error::CoreError;

use super::transform::TransformExpr;
use super::types::{BindingPath, BindingType};

/// Binding spec - map of alias to entry (YAML `with:` block)
pub type BindingSpec = FxHashMap<String, BindingEntry>;

/// Unified with entry - supports both string and extended object syntax
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
pub struct BindingEntry {
    /// Full path: "task.field.subfield" or "task" for entire output
    pub path: String,
    /// Optional default value (JSON literal)
    pub default: Option<Value>,
    /// Lazy flag - if true, resolution is deferred until first access
    pub lazy: bool,
}

impl BindingEntry {
    /// Create a new BindingEntry with just a path (eager resolution)
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            default: None,
            lazy: false,
        }
    }

    /// Create a new BindingEntry with path and default (eager resolution)
    pub fn with_default(path: impl Into<String>, default: Value) -> Self {
        Self {
            path: path.into(),
            default: Some(default),
            lazy: false,
        }
    }

    /// Create a new lazy BindingEntry (deferred resolution)
    pub fn new_lazy(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            default: None,
            lazy: true,
        }
    }

    /// Create a new lazy BindingEntry with default (deferred resolution)
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

    /// Normalize a binding path by stripping the `$` prefix if present.
    ///
    /// This enables implicit output reference syntax where `$task` is
    /// syntactic sugar for `task`. The RunContext.resolve_path() function
    /// already handles resolving bare task IDs to their full output.
    ///
    /// # Examples
    ///
    /// ```
    /// use nika_core::binding::BindingEntry;
    ///
    /// assert_eq!(BindingEntry::normalize_path("$task1"), "task1");
    /// assert_eq!(BindingEntry::normalize_path("task1"), "task1");
    /// assert_eq!(BindingEntry::normalize_path("$my_task"), "my_task");
    /// assert_eq!(BindingEntry::normalize_path("task.field"), "task.field");
    /// assert_eq!(BindingEntry::normalize_path("$task.field"), "task.field");
    /// ```
    #[inline]
    pub fn normalize_path(path: &str) -> &str {
        path.strip_prefix('$').unwrap_or(path)
    }
}

/// Parse a binding entry string into BindingEntry (eager resolution)
///
/// Syntax: `task.path [?? default]`
/// - If `??` found outside quotes, splits into path and default
/// - Default is parsed as JSON literal (strings must be quoted)
/// - String syntax always produces eager bindings (lazy=false)
pub fn parse_binding_entry(s: &str) -> Result<BindingEntry, CoreError> {
    let s = s.trim();

    if s.is_empty() {
        return Err(CoreError::InvalidPath {
            path: String::new(),
        });
    }

    match find_operator_outside_quotes(s, "??") {
        Some(idx) => {
            let path = s[..idx].trim();

            if path.is_empty() {
                return Err(CoreError::InvalidPath {
                    path: s.to_string(),
                });
            }

            let default_str = s[idx + 2..].trim();
            let default =
                serde_json::from_str(default_str).map_err(|e| CoreError::InvalidDefault {
                    raw: default_str.to_string(),
                    reason: e.to_string(),
                })?;

            Ok(BindingEntry {
                path: path.to_string(),
                default: Some(default),
                lazy: false,
            })
        }
        None => Ok(BindingEntry {
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

/// Custom deserializer for BindingEntry
///
/// Accepts two formats:
/// 1. String: `task.path [?? default]` → eager binding
/// 2. Object: `{path: "task.path", lazy: true, default: ...}` → lazy binding
impl<'de> Deserialize<'de> for BindingEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(BindingEntryVisitor)
    }
}

struct BindingEntryVisitor;

impl<'de> Visitor<'de> for BindingEntryVisitor {
    type Value = BindingEntry;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter
            .write_str("a string 'task.path [?? default]' or an object {path, lazy?, default?}")
    }

    /// Handle string format: "task.path [?? default]" (eager)
    /// Applies normalize_path() to strip $ prefix from implicit output syntax ($task → task)
    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let mut entry = parse_binding_entry(value).map_err(|e| de::Error::custom(e.to_string()))?;
        entry.path = BindingEntry::normalize_path(&entry.path).to_string();
        Ok(entry)
    }

    /// Handle object format: {path, lazy?, default?}
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
        let path = BindingEntry::normalize_path(&path).to_string();

        Ok(BindingEntry {
            path,
            default,
            lazy: lazy.unwrap_or(false),
        })
    }
}

// ═══════════════════════════════════════════════════════════════
// WithEntry -- new binding system
// ═══════════════════════════════════════════════════════════════

/// A single binding entry in the `with:` block
///
/// Supports two YAML forms:
///
/// **String form** (most common):
/// ```yaml
/// with:
///   result: $step1
///   title: $step1.title | upper
///   count: $step1.items | length ?? 0
/// ```
///
/// **Object form** (for complex cases):
/// ```yaml
/// with:
///   summary:
///     from: $step1.abstract
///     type: string
///     transform: lower | trim
///     default: "No abstract"
///     lazy: true
/// ```
#[derive(Debug, Clone)]
pub struct WithEntry {
    /// Parsed source path (e.g., $step1.data.items)
    pub source: BindingPath,
    /// Type constraint (default: Any)
    pub binding_type: BindingType,
    /// Default value if source is null/missing (applied AFTER transforms)
    pub default: Option<Value>,
    /// Defer resolution until first access
    pub lazy: bool,
    /// Transform pipeline to apply after resolution
    pub transform: Option<TransformExpr>,
}

/// Map of alias -> WithEntry (YAML `with:` block)
pub type WithSpec = FxHashMap<String, WithEntry>;

/// Error parsing a WithEntry string (NIKA-155)
#[derive(Debug, Clone, PartialEq)]
pub struct WithEntryParseError {
    pub input: String,
    pub reason: String,
}

impl fmt::Display for WithEntryParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[NIKA-155] WithEntry parse error in '{}': {}",
            self.input, self.reason
        )
    }
}

impl std::error::Error for WithEntryParseError {}

impl WithEntry {
    /// Create a simple WithEntry from a BindingPath (no transforms, no default)
    pub fn simple(source: BindingPath) -> Self {
        Self {
            source,
            binding_type: BindingType::default(),
            default: None,
            lazy: false,
            transform: None,
        }
    }

    /// Create a WithEntry with a default value
    pub fn with_default(source: BindingPath, default: Value) -> Self {
        Self {
            source,
            binding_type: BindingType::default(),
            default: Some(default),
            lazy: false,
            transform: None,
        }
    }

    /// Extract the task ID if this binding references a task output.
    ///
    /// Returns `Some(task_id)` for Task sources, `None` for Context/Input/Env/LoopVar.
    pub fn task_id(&self) -> Option<&str> {
        use super::types::BindingSource;
        match &self.source.source {
            BindingSource::Task(id) => Some(id),
            _ => None,
        }
    }

    /// Check if this binding is lazy (deferred resolution)
    pub fn is_lazy(&self) -> bool {
        self.lazy
    }
}

/// Parse a with-entry from its string form
///
/// Grammar:
/// ```text
///   entry     := path ("|" transform)* ("??" default)?
///   path      := "$" identifier ("." identifier | "[" index "]")*
///   transform := name | name "(" args ")"
///   default   := json_value
/// ```
///
/// Examples:
/// ```text
///   "$step1"                          → simple task ref
///   "$step1.data"                     → task ref with field access
///   "$step1.data ?? fallback"         → with JSON default
///   "$step1.data | upper"             → with transform
///   "$step1.data | sort | unique"     → transform chain
///   "$step1.data | sort | first(3) ?? []"  → chain + default
/// ```
pub fn parse_with_entry(input: &str) -> Result<WithEntry, WithEntryParseError> {
    let input_trimmed = input.trim();

    if input_trimmed.is_empty() {
        return Err(WithEntryParseError {
            input: input.to_string(),
            reason: "empty input".to_string(),
        });
    }

    // Step 1: Split off the default (" ?? ") -- must be outside quotes
    let (path_and_transforms, default_value) = split_default(input_trimmed)?;

    if path_and_transforms.is_empty() {
        return Err(WithEntryParseError {
            input: input.to_string(),
            reason: "empty path before '??'".to_string(),
        });
    }

    // Step 2: Split path from transforms by "|"
    let (path_str, transform_str) = split_transforms(path_and_transforms);

    let path_str = path_str.trim();
    if path_str.is_empty() {
        return Err(WithEntryParseError {
            input: input.to_string(),
            reason: "empty path".to_string(),
        });
    }

    // Step 3: Parse the path as a BindingPath
    let source = BindingPath::parse(path_str).map_err(|e| WithEntryParseError {
        input: input.to_string(),
        reason: e.reason,
    })?;

    // Step 4: Parse transforms (if any)
    let transform = if let Some(t_str) = transform_str {
        let t_str = t_str.trim();
        if t_str.is_empty() {
            return Err(WithEntryParseError {
                input: input.to_string(),
                reason: "empty transform after '|'".to_string(),
            });
        }
        Some(
            TransformExpr::parse(t_str).map_err(|e| WithEntryParseError {
                input: input.to_string(),
                reason: e.reason,
            })?,
        )
    } else {
        None
    };

    // Step 5: Parse default value (if any) -- must be valid JSON
    let default = match default_value {
        Some(d_str) => {
            let d_str = d_str.trim();
            if d_str.is_empty() {
                return Err(WithEntryParseError {
                    input: input.to_string(),
                    reason: "empty default value after '??'".to_string(),
                });
            }
            let val: Value = serde_json::from_str(d_str).map_err(|e| WithEntryParseError {
                input: input.to_string(),
                reason: format!("invalid default JSON: {e}"),
            })?;
            Some(val)
        }
        None => None,
    };

    Ok(WithEntry {
        source,
        binding_type: BindingType::default(),
        default,
        lazy: false,
        transform,
    })
}

/// Split the input at " ?? " to separate the path+transforms from the default value.
///
/// Respects double-quoted strings: `$x | default("a ?? b") ?? "fallback"`
/// would split at the final `??`, not the one inside `default()`.
fn split_default(s: &str) -> Result<(&str, Option<&str>), WithEntryParseError> {
    // Find the LAST " ?? " outside of quotes and parens
    // We scan left-to-right tracking quote/paren state, recording each valid `??` position.
    // The rightmost `??` outside quotes becomes the split point.
    let mut in_quotes = false;
    let mut escape_next = false;
    let mut paren_depth: u32 = 0;
    let mut last_default_pos: Option<usize> = None;
    let bytes = s.as_bytes();

    let mut i = 0;
    while i < bytes.len() {
        if escape_next {
            escape_next = false;
            i += 1;
            continue;
        }

        match bytes[i] {
            b'\\' => {
                escape_next = true;
            }
            b'"' => {
                in_quotes = !in_quotes;
            }
            b'(' if !in_quotes => {
                paren_depth = paren_depth.saturating_add(1);
            }
            b')' if !in_quotes => {
                paren_depth = paren_depth.saturating_sub(1);
            }
            b'?' if !in_quotes && paren_depth == 0 => {
                // Check for `??`
                if i + 1 < bytes.len() && bytes[i + 1] == b'?' {
                    last_default_pos = Some(i);
                    i += 2; // skip both `?`
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }

    match last_default_pos {
        Some(pos) => {
            let path_part = &s[..pos].trim_end();
            let default_part = &s[pos + 2..].trim_start();
            Ok((path_part, Some(default_part)))
        }
        None => Ok((s, None)),
    }
}

/// Split the path+transforms at the FIRST `|` outside quotes and parens.
///
/// Returns `(path, Some(transforms))` or `(path, None)`.
fn split_transforms(s: &str) -> (&str, Option<&str>) {
    let mut in_quotes = false;
    let mut escape_next = false;
    let mut paren_depth: u32 = 0;
    let bytes = s.as_bytes();

    for (i, &b) in bytes.iter().enumerate() {
        if escape_next {
            escape_next = false;
            continue;
        }

        match b {
            b'\\' => escape_next = true,
            b'"' => in_quotes = !in_quotes,
            b'(' if !in_quotes => paren_depth = paren_depth.saturating_add(1),
            b')' if !in_quotes => paren_depth = paren_depth.saturating_sub(1),
            b'|' if !in_quotes && paren_depth == 0 => {
                return (&s[..i], Some(&s[i + 1..]));
            }
            _ => {}
        }
    }

    (s, None)
}

/// Custom deserializer for WithEntry
///
/// Accepts two YAML formats:
/// 1. String: `"$step1.data | upper ?? fallback"` → parsed by `parse_with_entry`
/// 2. Object: `{ from: "$step1", type: string, transform: "upper", default: "x", lazy: true }`
impl<'de> Deserialize<'de> for WithEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(WithEntryVisitor)
    }
}

struct WithEntryVisitor;

impl<'de> Visitor<'de> for WithEntryVisitor {
    type Value = WithEntry;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(
            "a string '$path | transform ?? default' or an object \
             { from, type?, transform?, default?, lazy? }",
        )
    }

    /// Handle string form: "$step1.data | upper ?? fallback"
    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        parse_with_entry(value).map_err(|e| de::Error::custom(e.to_string()))
    }

    /// Handle object form: { from, type?, transform?, default?, lazy? }
    fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        // Use a helper struct for the object form
        #[derive(Deserialize)]
        struct WithEntryObject {
            from: String,
            #[serde(rename = "type", default)]
            binding_type: BindingType,
            #[serde(default)]
            transform: Option<String>,
            #[serde(default)]
            default: Option<Value>,
            #[serde(default)]
            lazy: bool,
        }

        let obj = WithEntryObject::deserialize(de::value::MapAccessDeserializer::new(map))?;

        // Parse `from:` as BindingPath
        let source = BindingPath::parse(&obj.from)
            .map_err(|e| de::Error::custom(format!("[NIKA-155] invalid 'from' path: {e}")))?;

        // Parse `transform:` as TransformExpr (if present)
        let transform = match obj.transform {
            Some(ref t_str) if !t_str.trim().is_empty() => Some(
                TransformExpr::parse(t_str.trim())
                    .map_err(|e| de::Error::custom(format!("[NIKA-155] invalid transform: {e}")))?,
            ),
            _ => None,
        };

        Ok(WithEntry {
            source,
            binding_type: obj.binding_type,
            default: obj.default,
            lazy: obj.lazy,
            transform,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::transform::TransformOp;
    use super::super::types::BindingSource;
    use super::*;
    use serde_json::json;
    use serde_saphyr as serde_yaml;

    // ═══════════════════════════════════════════════════════════════
    // parse_binding_entry() tests - TDD: write these first
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn parse_simple_path() {
        let entry = parse_binding_entry("weather.summary").unwrap();
        assert_eq!(entry.path, "weather.summary");
        assert_eq!(entry.default, None);
    }

    #[test]
    fn parse_simple_task_only() {
        let entry = parse_binding_entry("weather").unwrap();
        assert_eq!(entry.path, "weather");
        assert_eq!(entry.default, None);
    }

    #[test]
    fn parse_nested_path() {
        let entry = parse_binding_entry("weather.data.temperature.celsius").unwrap();
        assert_eq!(entry.path, "weather.data.temperature.celsius");
        assert_eq!(entry.default, None);
    }

    #[test]
    fn parse_with_default_number() {
        let entry = parse_binding_entry("x.y ?? 0").unwrap();
        assert_eq!(entry.path, "x.y");
        assert_eq!(entry.default, Some(json!(0)));
    }

    #[test]
    fn parse_with_default_negative_number() {
        let entry = parse_binding_entry("score ?? -1").unwrap();
        assert_eq!(entry.path, "score");
        assert_eq!(entry.default, Some(json!(-1)));
    }

    #[test]
    fn parse_with_default_float() {
        let entry = parse_binding_entry("rate ?? 0.5").unwrap();
        assert_eq!(entry.path, "rate");
        assert_eq!(entry.default, Some(json!(0.5)));
    }

    #[test]
    fn parse_with_default_string() {
        let entry = parse_binding_entry(r#"x.y ?? "Anon""#).unwrap();
        assert_eq!(entry.path, "x.y");
        assert_eq!(entry.default, Some(json!("Anon")));
    }

    #[test]
    fn parse_with_default_empty_string() {
        let entry = parse_binding_entry(r#"name ?? """#).unwrap();
        assert_eq!(entry.path, "name");
        assert_eq!(entry.default, Some(json!("")));
    }

    #[test]
    fn parse_with_default_bool_true() {
        let entry = parse_binding_entry("enabled ?? true").unwrap();
        assert_eq!(entry.path, "enabled");
        assert_eq!(entry.default, Some(json!(true)));
    }

    #[test]
    fn parse_with_default_bool_false() {
        let entry = parse_binding_entry("enabled ?? false").unwrap();
        assert_eq!(entry.path, "enabled");
        assert_eq!(entry.default, Some(json!(false)));
    }

    #[test]
    fn parse_with_default_null() {
        let entry = parse_binding_entry("value ?? null").unwrap();
        assert_eq!(entry.path, "value");
        assert_eq!(entry.default, Some(json!(null)));
    }

    #[test]
    fn parse_with_default_object() {
        let entry = parse_binding_entry(r#"x ?? {"a": 1, "b": 2}"#).unwrap();
        assert_eq!(entry.path, "x");
        assert_eq!(entry.default, Some(json!({"a": 1, "b": 2})));
    }

    #[test]
    fn parse_with_default_array() {
        let entry = parse_binding_entry(r#"tags ?? ["untagged"]"#).unwrap();
        assert_eq!(entry.path, "tags");
        assert_eq!(entry.default, Some(json!(["untagged"])));
    }

    #[test]
    fn parse_with_default_nested_object() {
        let entry = parse_binding_entry(r#"cfg ?? {"debug": false, "nested": {"a": 1}}"#).unwrap();
        assert_eq!(entry.path, "cfg");
        assert_eq!(
            entry.default,
            Some(json!({"debug": false, "nested": {"a": 1}}))
        );
    }

    #[test]
    fn parse_quotes_in_default() {
        // The ?? inside quotes should be ignored
        let entry = parse_binding_entry(r#"x ?? "What?? Really??""#).unwrap();
        assert_eq!(entry.path, "x");
        assert_eq!(entry.default, Some(json!("What?? Really??")));
    }

    #[test]
    fn parse_escaped_quotes_in_default() {
        let entry = parse_binding_entry(r#"x ?? "He said \"hello\"""#).unwrap();
        assert_eq!(entry.path, "x");
        assert_eq!(entry.default, Some(json!("He said \"hello\"")));
    }

    #[test]
    fn parse_with_whitespace() {
        let entry = parse_binding_entry("  weather.summary  ").unwrap();
        assert_eq!(entry.path, "weather.summary");
    }

    #[test]
    fn parse_with_whitespace_around_operator() {
        let entry = parse_binding_entry("x  ??  0").unwrap();
        assert_eq!(entry.path, "x");
        assert_eq!(entry.default, Some(json!(0)));
    }

    // ═══════════════════════════════════════════════════════════════
    // Error cases - TDD: these should fail appropriately
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn parse_reject_unquoted_string() {
        // "Anonymous" without quotes is invalid JSON
        let result = parse_binding_entry("x ?? Anonymous");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-056"));
    }

    #[test]
    fn parse_reject_empty_path() {
        let result = parse_binding_entry("");
        assert!(result.is_err());
    }

    #[test]
    fn parse_reject_only_operator() {
        let result = parse_binding_entry("??");
        assert!(result.is_err());
    }

    #[test]
    fn parse_reject_empty_path_with_default() {
        let result = parse_binding_entry("?? 0");
        assert!(result.is_err());
    }

    #[test]
    fn parse_reject_invalid_json_default() {
        // Missing closing brace
        let result = parse_binding_entry(r#"x ?? {"a": 1"#);
        assert!(result.is_err());
    }

    // ═══════════════════════════════════════════════════════════════
    // task_id() extraction tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn task_id_simple() {
        let entry = BindingEntry::new("weather");
        assert_eq!(entry.task_id(), "weather");
    }

    #[test]
    fn task_id_with_path() {
        let entry = BindingEntry::new("weather.summary");
        assert_eq!(entry.task_id(), "weather");
    }

    #[test]
    fn task_id_with_nested_path() {
        let entry = BindingEntry::new("weather.data.temp.celsius");
        assert_eq!(entry.task_id(), "weather");
    }

    // ═══════════════════════════════════════════════════════════════
    // YAML deserialization tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn yaml_parse_simple() {
        let yaml = "forecast: weather.summary";
        let spec: BindingSpec = serde_yaml::from_str(yaml).unwrap();
        let entry = spec.get("forecast").unwrap();
        assert_eq!(entry.path, "weather.summary");
        assert_eq!(entry.default, None);
    }

    #[test]
    fn yaml_parse_with_default() {
        let yaml = r#"temp: weather.temp ?? 20"#;
        let spec: BindingSpec = serde_yaml::from_str(yaml).unwrap();
        let entry = spec.get("temp").unwrap();
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
        let spec: BindingSpec = serde_yaml::from_str(yaml).unwrap();

        let forecast = spec.get("forecast").unwrap();
        assert_eq!(forecast.path, "weather.summary");
        assert_eq!(forecast.default, None);

        let temp = spec.get("temp").unwrap();
        assert_eq!(temp.path, "weather.temp");
        assert_eq!(temp.default, Some(json!(20)));

        let name = spec.get("name").unwrap();
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
        let spec: BindingSpec = serde_yaml::from_str(yaml).unwrap();

        let cfg = spec.get("cfg").unwrap();
        assert_eq!(cfg.default, Some(json!({"debug": false})));

        let tags = spec.get("tags").unwrap();
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

    // ═══════════════════════════════════════════════════════════════
    // normalize_path() tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_normalize_path_strips_dollar_prefix() {
        assert_eq!(BindingEntry::normalize_path("$task1"), "task1");
        assert_eq!(BindingEntry::normalize_path("task1"), "task1");
        assert_eq!(BindingEntry::normalize_path("$my_task"), "my_task");
        assert_eq!(BindingEntry::normalize_path("task.field"), "task.field");
        assert_eq!(BindingEntry::normalize_path("$task.field"), "task.field");
    }

    // ═══════════════════════════════════════════════════════════════
    // Deserialization normalization tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_binding_entry_deserialize_normalizes_dollar_prefix_shorthand() {
        // Shorthand: "$task1" → BindingEntry { path: "task1", ... }
        let entry: BindingEntry = serde_yaml::from_str("\"$task1\"").unwrap();
        assert_eq!(entry.path, "task1");

        // Without prefix should also work
        let entry: BindingEntry = serde_yaml::from_str("\"task1\"").unwrap();
        assert_eq!(entry.path, "task1");
    }

    #[test]
    fn test_binding_entry_deserialize_normalizes_dollar_prefix_full_form() {
        // Full form with $ prefix in path field
        let entry: BindingEntry = serde_yaml::from_str(
            r#"
            path: "$my_task"
            default: "fallback"
            "#,
        )
        .unwrap();
        assert_eq!(entry.path, "my_task");
        assert_eq!(
            entry.default.as_ref().map(|v| v.as_str()),
            Some(Some("fallback"))
        );

        // Without prefix should also work
        let entry: BindingEntry = serde_yaml::from_str(
            r#"
            path: "my_task"
            lazy: true
            "#,
        )
        .unwrap();
        assert_eq!(entry.path, "my_task");
        assert!(entry.lazy);
    }

    // ═══════════════════════════════════════════════════════════════
    // Comprehensive edge case tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_normalize_path_edge_cases() {
        // Multiple $ prefixes - only strip first
        assert_eq!(BindingEntry::normalize_path("$$task"), "$task");
        assert_eq!(BindingEntry::normalize_path("$$$task"), "$$task");

        // $ in middle or end - should NOT be stripped
        assert_eq!(BindingEntry::normalize_path("ta$sk"), "ta$sk");
        assert_eq!(BindingEntry::normalize_path("task$"), "task$");
        assert_eq!(BindingEntry::normalize_path("ta$sk$"), "ta$sk$");

        // Nested field access with $ prefix
        assert_eq!(
            BindingEntry::normalize_path("$task.field.subfield"),
            "task.field.subfield"
        );
        assert_eq!(
            BindingEntry::normalize_path("$task.nested.deep.path"),
            "task.nested.deep.path"
        );

        // Empty string and edge cases
        assert_eq!(BindingEntry::normalize_path(""), "");
        assert_eq!(BindingEntry::normalize_path("$"), "");
        assert_eq!(BindingEntry::normalize_path("$$"), "$");

        // Just dots
        assert_eq!(BindingEntry::normalize_path("$."), ".");
        assert_eq!(BindingEntry::normalize_path("$.."), "..");
        assert_eq!(BindingEntry::normalize_path(".task"), ".task");
        assert_eq!(BindingEntry::normalize_path("$.task"), ".task");

        // Unicode paths (should work fine)
        assert_eq!(BindingEntry::normalize_path("$résultat"), "résultat");
        assert_eq!(BindingEntry::normalize_path("$задача"), "задача");

        // Whitespace handling (normalize_path doesn't trim)
        assert_eq!(BindingEntry::normalize_path("$ task"), " task");
        assert_eq!(BindingEntry::normalize_path("$task "), "task ");
    }

    #[test]
    fn test_binding_entry_deserialize_nested_field_access() {
        // Shorthand form with nested field access
        let entry: BindingEntry = serde_yaml::from_str("\"$research.summary.title\"").unwrap();
        assert_eq!(entry.path, "research.summary.title");

        // Full form with nested field access
        let entry: BindingEntry = serde_yaml::from_str(
            r#"
            path: "$agent_result.response.data.items"
            lazy: true
            "#,
        )
        .unwrap();
        assert_eq!(entry.path, "agent_result.response.data.items");
        assert!(entry.lazy);
    }

    #[test]
    fn test_binding_entry_deserialize_multiple_dollar_signs() {
        // Multiple $ at start - only first stripped
        let entry: BindingEntry = serde_yaml::from_str("\"$$task\"").unwrap();
        assert_eq!(entry.path, "$task");

        let entry: BindingEntry = serde_yaml::from_str("\"$$$triple\"").unwrap();
        assert_eq!(entry.path, "$$triple");

        // Full form with multiple $
        let entry: BindingEntry = serde_yaml::from_str(
            r#"
            path: "$$escaped_var"
            "#,
        )
        .unwrap();
        assert_eq!(entry.path, "$escaped_var");
    }

    #[test]
    fn test_binding_entry_deserialize_dollar_in_middle() {
        // $ in middle should be preserved
        let entry: BindingEntry = serde_yaml::from_str("\"task$name\"").unwrap();
        assert_eq!(entry.path, "task$name");

        let entry: BindingEntry = serde_yaml::from_str("\"$task$name\"").unwrap();
        assert_eq!(entry.path, "task$name");

        // Full form
        let entry: BindingEntry = serde_yaml::from_str(
            r#"
            path: "result$2"
            "#,
        )
        .unwrap();
        assert_eq!(entry.path, "result$2");
    }

    #[test]
    fn test_binding_entry_deserialize_special_characters() {
        // Underscores and numbers (common in task IDs)
        let entry: BindingEntry = serde_yaml::from_str("\"$task_123\"").unwrap();
        assert_eq!(entry.path, "task_123");

        let entry: BindingEntry = serde_yaml::from_str("\"$_private_task\"").unwrap();
        assert_eq!(entry.path, "_private_task");

        // Hyphens
        let entry: BindingEntry = serde_yaml::from_str("\"$task-name\"").unwrap();
        assert_eq!(entry.path, "task-name");

        // Mixed
        let entry: BindingEntry = serde_yaml::from_str("\"$task_1-result.field_2\"").unwrap();
        assert_eq!(entry.path, "task_1-result.field_2");
    }

    #[test]
    fn test_binding_entry_deserialize_with_all_options() {
        // Full form with $ prefix and all options set
        let entry: BindingEntry = serde_yaml::from_str(
            r#"
            path: "$complex_task.nested.value"
            default: "default_value"
            lazy: true
            "#,
        )
        .unwrap();
        assert_eq!(entry.path, "complex_task.nested.value");
        assert_eq!(
            entry.default.as_ref().map(|v| v.as_str()),
            Some(Some("default_value"))
        );
        assert!(entry.lazy);
    }

    #[test]
    fn test_binding_entry_equivalence_with_and_without_dollar() {
        // These should produce identical BindingEntry instances
        let with_dollar: BindingEntry = serde_yaml::from_str("\"$my_task\"").unwrap();
        let without_dollar: BindingEntry = serde_yaml::from_str("\"my_task\"").unwrap();

        assert_eq!(with_dollar.path, without_dollar.path);
        assert_eq!(with_dollar.default, without_dollar.default);
        assert_eq!(with_dollar.lazy, without_dollar.lazy);
    }

    #[test]
    fn test_binding_entry_deserialize_real_workflow_patterns() {
        // Pattern: Simple task reference
        let entry: BindingEntry = serde_yaml::from_str("\"$get_context\"").unwrap();
        assert_eq!(entry.path, "get_context");

        // Pattern: Output field access
        let entry: BindingEntry = serde_yaml::from_str("\"$generate.content\"").unwrap();
        assert_eq!(entry.path, "generate.content");

        // Pattern: Agent result access
        let entry: BindingEntry =
            serde_yaml::from_str("\"$research_agent.findings.summary\"").unwrap();
        assert_eq!(entry.path, "research_agent.findings.summary");

        // Pattern: Lazy binding with default for optional task output
        let entry: BindingEntry = serde_yaml::from_str(
            r#"
            path: "$optional_step.result"
            default: null
            lazy: true
            "#,
        )
        .unwrap();
        assert_eq!(entry.path, "optional_step.result");
        assert_eq!(entry.default, Some(serde_json::Value::Null));
        assert!(entry.lazy);
    }

    // ═══════════════════════════════════════════════════════════════
    // WithEntry -- parse_with_entry() tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_parse_simple() {
        let entry = parse_with_entry("$step1").unwrap();
        assert_eq!(entry.source, BindingPath::parse("$step1").unwrap());
        assert_eq!(entry.default, None);
        assert_eq!(entry.transform, None);
        assert!(!entry.lazy);
        assert_eq!(entry.binding_type, BindingType::Any);
    }

    #[test]
    fn with_parse_field_access() {
        let entry = parse_with_entry("$step1.output").unwrap();
        assert_eq!(entry.source, BindingPath::parse("$step1.output").unwrap());
        assert_eq!(entry.default, None);
        assert_eq!(entry.transform, None);
    }

    #[test]
    fn with_parse_deep_path() {
        let entry = parse_with_entry("$step1.data.items[0].name").unwrap();
        assert_eq!(
            entry.source,
            BindingPath::parse("$step1.data.items[0].name").unwrap()
        );
    }

    #[test]
    fn with_parse_default_string() {
        let entry = parse_with_entry(r#"$step1 ?? "fallback""#).unwrap();
        assert_eq!(entry.source, BindingPath::parse("$step1").unwrap());
        assert_eq!(entry.default, Some(json!("fallback")));
        assert_eq!(entry.transform, None);
    }

    #[test]
    fn with_parse_default_number() {
        let entry = parse_with_entry("$step1 ?? 42").unwrap();
        assert_eq!(entry.default, Some(json!(42)));
    }

    #[test]
    fn with_parse_default_float() {
        let entry = parse_with_entry("$step1.score ?? 0.5").unwrap();
        assert_eq!(entry.default, Some(json!(0.5)));
    }

    #[test]
    fn with_parse_default_bool() {
        let entry = parse_with_entry("$step1.enabled ?? true").unwrap();
        assert_eq!(entry.default, Some(json!(true)));
    }

    #[test]
    fn with_parse_default_null() {
        let entry = parse_with_entry("$step1.val ?? null").unwrap();
        assert_eq!(entry.default, Some(json!(null)));
    }

    #[test]
    fn with_parse_default_array() {
        let entry = parse_with_entry("$step1 ?? []").unwrap();
        assert_eq!(entry.default, Some(json!([])));
    }

    #[test]
    fn with_parse_default_object() {
        let entry = parse_with_entry(r#"$step1 ?? {"key": "val"}"#).unwrap();
        assert_eq!(entry.default, Some(json!({"key": "val"})));
    }

    #[test]
    fn with_parse_transform_single() {
        let entry = parse_with_entry("$step1 | upper").unwrap();
        assert_eq!(entry.source, BindingPath::parse("$step1").unwrap());
        assert!(entry.transform.is_some());
        let t = entry.transform.unwrap();
        assert_eq!(t.ops.len(), 1);
        assert_eq!(t.ops[0], TransformOp::Upper);
    }

    #[test]
    fn with_parse_transform_chain() {
        let entry = parse_with_entry("$step1.items | sort | unique").unwrap();
        let t = entry.transform.unwrap();
        assert_eq!(t.ops.len(), 2);
        assert_eq!(t.ops[0], TransformOp::Sort);
        assert_eq!(t.ops[1], TransformOp::Unique);
    }

    #[test]
    fn with_parse_transform_with_args() {
        let entry = parse_with_entry("$step1.items | first(3)").unwrap();
        let t = entry.transform.unwrap();
        assert_eq!(t.ops.len(), 1);
        assert_eq!(t.ops[0], TransformOp::FirstN(3));
    }

    #[test]
    fn with_parse_transform_and_default() {
        let entry = parse_with_entry("$step1.items | length ?? 0").unwrap();
        assert!(entry.transform.is_some());
        let t = entry.transform.unwrap();
        assert_eq!(t.ops.len(), 1);
        assert_eq!(t.ops[0], TransformOp::Length);
        assert_eq!(entry.default, Some(json!(0)));
    }

    #[test]
    fn with_parse_full_chain_and_default() {
        let entry = parse_with_entry(r#"$step1.items | sort | first(3) ?? []"#).unwrap();
        let t = entry.transform.unwrap();
        assert_eq!(t.ops.len(), 2);
        assert_eq!(t.ops[0], TransformOp::Sort);
        assert_eq!(t.ops[1], TransformOp::FirstN(3));
        assert_eq!(entry.default, Some(json!([])));
    }

    #[test]
    fn with_parse_context_ref() {
        let entry = parse_with_entry("$context.files.brand").unwrap();
        match &entry.source.source {
            BindingSource::Context(path) => assert_eq!(path.as_ref(), "files.brand"),
            _ => panic!("expected Context source"),
        }
    }

    #[test]
    fn with_parse_input_ref() {
        let entry = parse_with_entry("$inputs.locale").unwrap();
        match &entry.source.source {
            BindingSource::Input(path) => assert_eq!(path.as_ref(), "locale"),
            _ => panic!("expected Input source"),
        }
    }

    #[test]
    fn with_parse_env_ref() {
        let entry = parse_with_entry("$env.API_URL").unwrap();
        match &entry.source.source {
            BindingSource::Env(path) => assert_eq!(path.as_ref(), "API_URL"),
            _ => panic!("expected Env source"),
        }
    }

    #[test]
    fn with_parse_whitespace_tolerance() {
        let entry = parse_with_entry("  $step1  |  upper  ").unwrap();
        assert_eq!(entry.source, BindingPath::parse("$step1").unwrap());
        let t = entry.transform.unwrap();
        assert_eq!(t.ops[0], TransformOp::Upper);
    }

    #[test]
    fn with_parse_whitespace_around_default() {
        let entry = parse_with_entry("$step1  ??  42").unwrap();
        assert_eq!(entry.default, Some(json!(42)));
    }

    // ═══════════════════════════════════════════════════════════════
    // WithEntry error cases
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_parse_empty_string_error() {
        let result = parse_with_entry("");
        assert!(result.is_err());
        assert!(result.unwrap_err().reason.contains("empty"));
    }

    #[test]
    fn with_parse_no_dollar_error() {
        let result = parse_with_entry("step1");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.reason.contains("must start with '$'"));
    }

    #[test]
    fn with_parse_pipe_only_error() {
        let result = parse_with_entry("| upper");
        assert!(result.is_err());
    }

    #[test]
    fn with_parse_default_only_error() {
        let result = parse_with_entry("?? 42");
        assert!(result.is_err());
    }

    #[test]
    fn with_parse_trailing_pipe_error() {
        let result = parse_with_entry("$step1 |");
        assert!(result.is_err());
        assert!(result.unwrap_err().reason.contains("empty transform"));
    }

    #[test]
    fn with_parse_invalid_json_default_error() {
        let result = parse_with_entry(r#"$step1 ?? {broken"#);
        assert!(result.is_err());
        assert!(result.unwrap_err().reason.contains("invalid default JSON"));
    }

    #[test]
    fn with_parse_unquoted_string_default_error() {
        let result = parse_with_entry("$step1 ?? Anonymous");
        assert!(result.is_err());
    }

    #[test]
    fn with_parse_empty_default_error() {
        // "$step1 ??" with nothing after
        let result = parse_with_entry("$step1 ??");
        assert!(result.is_err());
        assert!(result.unwrap_err().reason.contains("empty default"));
    }

    #[test]
    fn with_parse_unknown_transform_error() {
        let result = parse_with_entry("$step1 | nonexistent_transform");
        assert!(result.is_err());
    }

    // ═══════════════════════════════════════════════════════════════
    // WithEntry: default inside transform parens (edge case)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_parse_default_inside_parens_ignored() {
        // The ?? inside default() should NOT be treated as the default separator
        let entry = parse_with_entry(r#"$step1 | default("a ?? b")"#).unwrap();
        assert!(entry.transform.is_some());
        assert_eq!(entry.default, None); // No ?? outside parens
    }

    #[test]
    fn with_parse_default_after_transform_with_inner_qq() {
        // default("a ?? b") ?? "fallback"
        let entry = parse_with_entry(r#"$step1 | default("a ?? b") ?? "fallback""#).unwrap();
        assert!(entry.transform.is_some());
        assert_eq!(entry.default, Some(json!("fallback")));
    }

    // ═══════════════════════════════════════════════════════════════
    // WithEntry helpers
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_entry_task_id() {
        let entry = parse_with_entry("$step1.data.name").unwrap();
        assert_eq!(entry.task_id(), Some("step1"));
    }

    #[test]
    fn with_entry_task_id_context() {
        let entry = parse_with_entry("$context.files.brand").unwrap();
        assert_eq!(entry.task_id(), None);
    }

    #[test]
    fn with_entry_simple_constructor() {
        let path = BindingPath::parse("$step1.data").unwrap();
        let entry = WithEntry::simple(path.clone());
        assert_eq!(entry.source, path);
        assert_eq!(entry.default, None);
        assert!(!entry.lazy);
        assert_eq!(entry.transform, None);
    }

    #[test]
    fn with_entry_with_default_constructor() {
        let path = BindingPath::parse("$step1").unwrap();
        let entry = WithEntry::with_default(path.clone(), json!(42));
        assert_eq!(entry.source, path);
        assert_eq!(entry.default, Some(json!(42)));
    }

    // ═══════════════════════════════════════════════════════════════
    // WithEntry YAML deserialization (string form)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_deser_string_simple() {
        let entry: WithEntry = serde_yaml::from_str("\"$step1\"").unwrap();
        assert_eq!(entry.source, BindingPath::parse("$step1").unwrap());
    }

    #[test]
    fn with_deser_string_with_transform() {
        let entry: WithEntry = serde_yaml::from_str("\"$step1.name | upper\"").unwrap();
        assert_eq!(entry.source, BindingPath::parse("$step1.name").unwrap());
        let t = entry.transform.unwrap();
        assert_eq!(t.ops[0], TransformOp::Upper);
    }

    #[test]
    fn with_deser_string_with_default() {
        let entry: WithEntry = serde_yaml::from_str(r#""$step1 ?? 42""#).unwrap();
        assert_eq!(entry.default, Some(json!(42)));
    }

    // ═══════════════════════════════════════════════════════════════
    // WithEntry YAML deserialization (object form)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_deser_object_minimal() {
        let entry: WithEntry = serde_yaml::from_str(
            r#"
            from: "$step1"
            "#,
        )
        .unwrap();
        assert_eq!(entry.source, BindingPath::parse("$step1").unwrap());
        assert_eq!(entry.binding_type, BindingType::Any);
        assert_eq!(entry.default, None);
        assert!(!entry.lazy);
        assert_eq!(entry.transform, None);
    }

    #[test]
    fn with_deser_object_typed() {
        let entry: WithEntry = serde_yaml::from_str(
            r#"
            from: "$step1.name"
            type: string
            "#,
        )
        .unwrap();
        assert_eq!(entry.binding_type, BindingType::String);
    }

    #[test]
    fn with_deser_object_with_transform() {
        let entry: WithEntry = serde_yaml::from_str(
            r#"
            from: "$step1.text"
            transform: "upper | trim"
            "#,
        )
        .unwrap();
        let t = entry.transform.unwrap();
        assert_eq!(t.ops.len(), 2);
        assert_eq!(t.ops[0], TransformOp::Upper);
        assert_eq!(t.ops[1], TransformOp::Trim);
    }

    #[test]
    fn with_deser_object_full() {
        let entry: WithEntry = serde_yaml::from_str(
            r#"
            from: "$step1.abstract"
            type: string
            transform: "lower | trim"
            default: "No abstract"
            lazy: true
            "#,
        )
        .unwrap();
        assert_eq!(entry.source, BindingPath::parse("$step1.abstract").unwrap());
        assert_eq!(entry.binding_type, BindingType::String);
        assert!(entry.transform.is_some());
        assert_eq!(entry.default, Some(json!("No abstract")));
        assert!(entry.lazy);
    }

    #[test]
    fn with_deser_object_lazy() {
        let entry: WithEntry = serde_yaml::from_str(
            r#"
            from: "$step1.result"
            lazy: true
            "#,
        )
        .unwrap();
        assert!(entry.lazy);
    }

    // ═══════════════════════════════════════════════════════════════
    // WithSpec deserialization (YAML map)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_deser_spec_empty() {
        let spec: WithSpec = serde_yaml::from_str("{}").unwrap();
        assert!(spec.is_empty());
    }

    #[test]
    fn with_deser_spec_single() {
        let spec: WithSpec = serde_yaml::from_str(r#"result: "$step1""#).unwrap();
        assert_eq!(spec.len(), 1);
        let entry = spec.get("result").unwrap();
        assert_eq!(entry.source, BindingPath::parse("$step1").unwrap());
    }

    #[test]
    fn with_deser_spec_mixed() {
        let yaml = r#"
result: "$step1"
title: "$step1.title | upper"
summary:
  from: "$step1.abstract"
  type: string
  transform: "lower | trim"
  default: "N/A"
  lazy: true
"#;
        let spec: WithSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.len(), 3);

        // String form: simple ref
        let result = spec.get("result").unwrap();
        assert_eq!(result.source, BindingPath::parse("$step1").unwrap());
        assert_eq!(result.transform, None);

        // String form: with transform
        let title = spec.get("title").unwrap();
        assert_eq!(title.source, BindingPath::parse("$step1.title").unwrap());
        let t = title.transform.as_ref().unwrap();
        assert_eq!(t.ops[0], TransformOp::Upper);

        // Object form: full
        let summary = spec.get("summary").unwrap();
        assert_eq!(
            summary.source,
            BindingPath::parse("$step1.abstract").unwrap()
        );
        assert_eq!(summary.binding_type, BindingType::String);
        assert!(summary.lazy);
        assert_eq!(summary.default, Some(json!("N/A")));
    }

    // ═══════════════════════════════════════════════════════════════
    // WithEntry object form error cases
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_deser_object_missing_from_error() {
        let result: Result<WithEntry, _> = serde_yaml::from_str(
            r#"
            type: string
            "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn with_deser_object_invalid_path_error() {
        let result: Result<WithEntry, _> = serde_yaml::from_str(
            r#"
            from: "step1"
            "#,
        );
        // No $ prefix -> error
        assert!(result.is_err());
    }

    #[test]
    fn with_deser_object_invalid_transform_error() {
        let result: Result<WithEntry, _> = serde_yaml::from_str(
            r#"
            from: "$step1"
            transform: "nonexistent_op"
            "#,
        );
        assert!(result.is_err());
    }

    // ═══════════════════════════════════════════════════════════════
    // split_default() edge cases
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn split_default_no_default() {
        let (path, def) = split_default("$step1 | upper").unwrap();
        assert_eq!(path, "$step1 | upper");
        assert_eq!(def, None);
    }

    #[test]
    fn split_default_simple() {
        let (path, def) = split_default("$step1 ?? 42").unwrap();
        assert_eq!(path, "$step1");
        assert_eq!(def, Some("42"));
    }

    #[test]
    fn split_default_inside_parens_ignored() {
        let (path, def) = split_default(r#"$step1 | default("a ?? b")"#).unwrap();
        assert_eq!(path, r#"$step1 | default("a ?? b")"#);
        assert_eq!(def, None);
    }

    #[test]
    fn split_default_after_parens() {
        let (path, def) = split_default(r#"$step1 | default("inner") ?? "outer""#).unwrap();
        assert_eq!(path, r#"$step1 | default("inner")"#);
        assert_eq!(def, Some(r#""outer""#));
    }

    // ═══════════════════════════════════════════════════════════════
    // split_transforms() edge cases
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn split_transforms_no_pipe() {
        let (path, t) = split_transforms("$step1");
        assert_eq!(path, "$step1");
        assert_eq!(t, None);
    }

    #[test]
    fn split_transforms_single() {
        let (path, t) = split_transforms("$step1 | upper");
        assert_eq!(path, "$step1 ");
        assert_eq!(t, Some(" upper"));
    }

    #[test]
    fn split_transforms_chain() {
        // First pipe splits path from rest of transforms
        let (path, t) = split_transforms("$step1 | sort | unique");
        assert_eq!(path, "$step1 ");
        assert_eq!(t, Some(" sort | unique"));
    }

    // ═══════════════════════════════════════════════════════════════
    // WithEntryParseError display
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_entry_parse_error_display() {
        let err = WithEntryParseError {
            input: "$bad".to_string(),
            reason: "test error".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("NIKA-155"));
        assert!(msg.contains("$bad"));
        assert!(msg.contains("test error"));
    }

    // ═══════════════════════════════════════════════════════════════
    // WithEntry: binding pattern tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_simple_path() {
        let entry = parse_with_entry("$step1").unwrap();
        assert_eq!(entry.task_id(), Some("step1"));
    }

    #[test]
    fn with_deep_path() {
        let entry = parse_with_entry("$step1.data.name").unwrap();
        assert_eq!(entry.task_id(), Some("step1"));
        assert_eq!(entry.source.segments.len(), 2);
    }

    #[test]
    fn with_default_string() {
        let entry = parse_with_entry(r#"$step1 ?? "N/A""#).unwrap();
        assert_eq!(entry.default, Some(json!("N/A")));
    }
}
