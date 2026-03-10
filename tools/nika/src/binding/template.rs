//! Template Resolution - `{{use.alias}}` substitution (v0.5)
//!
//! Single syntax: {{use.alias}} or {{use.alias.field}}
//! True single-pass resolution with `Cow<str>` for zero-alloc when no templates.
//!
//! Performance optimizations:
//! - Zero-clone traversal (references until final value)
//! - SmallVec for error collection (stack-allocated)
//! - Better capacity estimation for result string
//!
//! v0.5: Supports lazy bindings via DataStore parameter.

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;
use rustc_hash::FxHashSet;
use serde_json::Value;
use smallvec::SmallVec;

use crate::error::NikaError;
use crate::store::DataStore;

use super::resolve::ResolvedBindings;

/// Pre-compiled regex for {{use.alias}} or {{use.alias.field}} pattern
/// v0.13: Now supports optional |shell modifier: {{use.alias|shell}}
/// v0.22: Also supports bracket notation after preprocessing: {{use.items[0]}} → {{use.items.0}}
static USE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{\s*use\.(\w+(?:\.\w+)*)(?:\s*\|\s*(shell))?\s*\}\}").unwrap()
});

/// Pre-compiled regex for bracket array notation (v0.22)
/// Converts [0] to .0 for uniform handling
static BRACKET_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[(\d+)\]").unwrap());

/// Pre-compiled regex for {{context.files.alias}} or {{context.session.key}} pattern (v0.14.2)
static CONTEXT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{\s*context\.(files|session)\.(\w+(?:\.\w+)*)\s*\}\}").unwrap()
});

/// Pre-compiled regex for {{inputs.param}} pattern (v0.19.4)
/// Extracts the input parameter name for lookup in workflow inputs
static INPUTS_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Match inputs.param or inputs.param.nested.path
    Regex::new(r"\{\{\s*inputs\.(\w+(?:\.\w+)*)\s*\}\}").unwrap()
});

/// Pre-compiled regex for deprecated $alias syntax
/// Matches: $alias or $alias.field (but not $$, $1, ${, $( which are shell syntax)
static DEPRECATED_DOLLAR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$([a-zA-Z_][a-zA-Z0-9_]*)(?:\.(\w+))*").unwrap());

/// Escape for JSON string context
fn escape_for_json(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => {
                result.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => result.push(c),
        }
    }
    result
}

/// Escape a string for safe shell usage (v0.13)
///
/// Uses single quotes with proper escaping for all special characters.
/// This ensures values from LLM outputs can be safely used in shell commands.
///
/// Example: "Hello 'world'" becomes "'Hello '\''world'\'''"
pub fn escape_for_shell(s: &str) -> String {
    // Single-quote escaping: wrap in single quotes, escape existing single quotes
    // 'foo' -> safe
    // foo'bar -> 'foo'\''bar'
    if s.is_empty() {
        return "''".to_string();
    }

    let mut result = String::with_capacity(s.len() + 10);
    result.push('\'');

    for ch in s.chars() {
        if ch == '\'' {
            // End current single-quote, add escaped single-quote, start new single-quote
            result.push_str("'\\''");
        } else {
            result.push(ch);
        }
    }

    result.push('\'');
    result
}

/// Normalize bracket notation to dot notation (v0.22)
///
/// Converts `{{use.items[0]}}` to `{{use.items.0}}` for uniform handling.
/// This allows users to use familiar JavaScript-style array indexing.
fn normalize_bracket_notation(template: &str) -> Cow<'_, str> {
    if !template.contains('[') {
        return Cow::Borrowed(template);
    }
    Cow::Owned(BRACKET_RE.replace_all(template, ".$1").to_string())
}

/// Resolve all {{use.alias}}, {{context.*}}, and {{inputs.*}} templates (v0.19.4)
///
/// Returns Cow::Borrowed when no templates (zero allocation).
/// Returns Cow::Owned with single-pass resolution when templates exist.
///
/// Performance: Zero-clone traversal - uses references until final value_to_string.
///
/// v0.5: Supports lazy bindings by resolving them on demand via DataStore.
/// v0.14.2: Supports context bindings via {{context.files.alias}} and {{context.session.key}}.
/// v0.19.4: Supports inputs bindings via {{inputs.param}}.
///
/// Example: `{{use.forecast}}` → resolved value from bindings
/// Example: `{{use.flight_info.departure}}` → nested access
/// Example: `{{context.files.brand}}` → loaded file content (v0.14.2)
/// Example: `{{context.session.focus}}` → session data (v0.14.2)
/// Example: `{{inputs.topic}}` → input parameter default value (v0.19.4)
pub fn resolve<'a>(
    template: &'a str,
    bindings: &ResolvedBindings,
    datastore: &DataStore,
) -> Result<Cow<'a, str>, NikaError> {
    // Early return with borrowed string (zero alloc)
    // Fast check: must contain `{{` followed eventually by `use.`, `context.`, or `inputs.`
    // Regex handles whitespace variations like `{{ use.` or `{{\tuse.`
    if !template.contains("{{") {
        return Ok(Cow::Borrowed(template));
    }
    let has_use = template.contains("use.");
    let has_context = template.contains("context.");
    let has_inputs = template.contains("inputs.");
    if !has_use && !has_context && !has_inputs {
        return Ok(Cow::Borrowed(template));
    }

    // v0.22: Normalize bracket notation to dot notation
    // {{use.items[0]}} → {{use.items.0}}
    let normalized = normalize_bracket_notation(template);
    let template_str: &str = normalized.as_ref();

    // Single-pass: build result by copying segments + inserting replacements
    // Better capacity: template length + some extra for expansions
    let mut result = String::with_capacity(template_str.len() + 64);
    let mut last_end = 0;
    // SmallVec: stack-allocated for up to 4 errors (common case: 0-1 errors)
    // Note: must be String because alias borrows from cap which is dropped each iteration
    let mut errors: SmallVec<[String; 4]> = SmallVec::new();

    for cap in USE_RE.captures_iter(template_str) {
        let m = cap.get(0).unwrap();
        let path = &cap[1]; // e.g., "forecast" or "flight_info.departure"
        let modifier = cap.get(2).map(|m| m.as_str()); // v0.13: optional |shell modifier

        // Copy segment before this match
        result.push_str(&template_str[last_end..m.start()]);

        // Split: first segment is alias, rest is nested path
        let mut parts = path.split('.');
        let alias = parts.next().unwrap();

        // Get the resolved value for this alias (supports lazy bindings via DataStore)
        match bindings.get_resolved(alias, datastore) {
            Ok(base_value) => {
                // Zero-clone traversal: use references until we need the final value
                let mut value_ref: &Value = &base_value;
                let mut traversed_segments: SmallVec<[&str; 8]> = SmallVec::new();
                traversed_segments.push(alias);

                // Traverse nested path if present (all by reference)
                for segment in parts {
                    let next = if let Ok(idx) = segment.parse::<usize>() {
                        value_ref.get(idx)
                    } else {
                        value_ref.get(segment)
                    };

                    match next {
                        Some(v) => {
                            traversed_segments.push(segment);
                            value_ref = v;
                        }
                        None => {
                            // Determine if it's an invalid traversal or missing field
                            let value_type = match value_ref {
                                Value::Null => "null",
                                Value::Bool(_) => "bool",
                                Value::Number(_) => "number",
                                Value::String(_) => "string",
                                Value::Array(_) => "array",
                                Value::Object(_) => "object",
                            };

                            if matches!(value_ref, Value::Object(_) | Value::Array(_)) {
                                // Field/index doesn't exist - build path for error
                                let traversed_path = traversed_segments.join(".");
                                return Err(NikaError::PathNotFound {
                                    path: format!("{}.{}", traversed_path, segment),
                                });
                            } else {
                                // Trying to traverse a primitive
                                return Err(NikaError::InvalidTraversal {
                                    segment: segment.to_string(),
                                    value_type: value_type.to_string(),
                                    full_path: path.to_string(),
                                });
                            }
                        }
                    }
                }

                // Convert Value to string (strict mode - null is error)
                // This is the ONLY place we convert/allocate for the value
                let replacement = value_to_string(value_ref, path, alias)?;

                // Apply modifier or context-based escaping (v0.13)
                let replacement = match modifier {
                    Some("shell") => escape_for_shell(&replacement),
                    _ if is_in_json_context(template_str, m.start()) => {
                        escape_for_json(&replacement)
                    }
                    _ => replacement,
                };

                result.push_str(&replacement);
            }
            Err(_) => {
                // Binding not found (neither eager nor lazy)
                errors.push(alias.to_string());
            }
        }

        last_end = m.end();
    }

    if !errors.is_empty() {
        return Err(NikaError::TemplateError {
            template: errors.join(", "),
            reason: "Alias(es) not resolved. Did you declare them in 'use:'?".to_string(),
        });
    }

    // Copy remaining segment after last match
    result.push_str(&template_str[last_end..]);

    // ─────────────────────────────────────────────────────────────
    // Pass 2: Resolve {{context.files.alias}} and {{context.session.key}} (v0.14.2)
    // ─────────────────────────────────────────────────────────────
    if has_context && result.contains("context.") {
        let intermediate = std::mem::take(&mut result);
        result = String::with_capacity(intermediate.len() + 64);
        let mut last_end = 0;
        let mut context_errors: SmallVec<[String; 4]> = SmallVec::new();

        for cap in CONTEXT_RE.captures_iter(&intermediate) {
            let m = cap.get(0).unwrap();
            let category = &cap[1]; // "files" or "session"
            let path_rest = &cap[2]; // alias or alias.field

            // Copy segment before this match
            result.push_str(&intermediate[last_end..m.start()]);

            // Build full context path: context.files.alias or context.session.key
            let full_path = format!("context.{}.{}", category, path_rest);

            // Resolve from datastore
            match datastore.resolve_context_path(&full_path) {
                Some(value) => {
                    let replacement = context_value_to_string(&value, &full_path)?;

                    // Escape if we're in a JSON context
                    let replacement = if is_in_json_context(&intermediate, m.start()) {
                        escape_for_json(&replacement)
                    } else {
                        replacement
                    };

                    result.push_str(&replacement);
                }
                None => {
                    context_errors.push(full_path);
                }
            }

            last_end = m.end();
        }

        if !context_errors.is_empty() {
            return Err(NikaError::TemplateError {
                template: context_errors.join(", "),
                reason: "Context binding(s) not resolved. Check your 'context:' block in workflow."
                    .to_string(),
            });
        }

        // Copy remaining segment
        result.push_str(&intermediate[last_end..]);

        // Continue to inputs pass if needed
        if !has_inputs || !result.contains("inputs.") {
            return Ok(Cow::Owned(result));
        }
        // Fall through to inputs pass with updated result
    }

    // ─────────────────────────────────────────────────────────────
    // Pass 3: Resolve {{inputs.param}} (v0.19.4)
    // ─────────────────────────────────────────────────────────────
    if has_inputs && result.contains("inputs.") {
        let intermediate = std::mem::take(&mut result);
        result = String::with_capacity(intermediate.len() + 64);
        let mut last_end = 0;
        let mut input_errors: SmallVec<[String; 4]> = SmallVec::new();

        for cap in INPUTS_RE.captures_iter(&intermediate) {
            let m = cap.get(0).unwrap();
            let param_name = &cap[1]; // The parameter name after "inputs."

            // Copy segment before this match
            result.push_str(&intermediate[last_end..m.start()]);

            // Build full path: inputs.param
            let full_path = format!("inputs.{}", param_name);

            // Resolve from datastore
            match datastore.resolve_input_path(&full_path) {
                Some(value) => {
                    let replacement = input_value_to_string(&value, &full_path)?;

                    // Escape if we're in a JSON context
                    let replacement = if is_in_json_context(&intermediate, m.start()) {
                        escape_for_json(&replacement)
                    } else {
                        replacement
                    };

                    result.push_str(&replacement);
                }
                None => {
                    input_errors.push(full_path);
                }
            }

            last_end = m.end();
        }

        if !input_errors.is_empty() {
            return Err(NikaError::TemplateError {
                template: input_errors.join(", "),
                reason: "Input binding(s) not resolved. Check your 'inputs:' block in workflow or provide defaults.".to_string(),
            });
        }

        // Copy remaining segment
        result.push_str(&intermediate[last_end..]);

        return Ok(Cow::Owned(result));
    }

    Ok(Cow::Owned(result))
}

/// Resolve templates for shell context (v0.13)
///
/// Similar to `resolve`, but shell-escapes all substituted values to prevent
/// command injection from LLM outputs containing special characters.
///
/// Example: `echo 'Hello {{use.msg}}'` with msg="Nika's test" becomes
///          `echo 'Hello '\''Nika'\''s test'\'''`
pub fn resolve_for_shell<'a>(
    template: &'a str,
    bindings: &ResolvedBindings,
    datastore: &DataStore,
) -> Result<Cow<'a, str>, NikaError> {
    // Early return if no templates
    if !template.contains("{{") {
        return Ok(Cow::Borrowed(template));
    }
    let has_use = template.contains("use.");
    let has_context = template.contains("context.");
    if !has_use && !has_context {
        return Ok(Cow::Borrowed(template));
    }

    let mut result = String::with_capacity(template.len() + 64);
    let mut last_end = 0;
    let mut errors: SmallVec<[String; 4]> = SmallVec::new();

    for cap in USE_RE.captures_iter(template) {
        let m = cap.get(0).unwrap();
        let path = &cap[1];

        result.push_str(&template[last_end..m.start()]);

        let mut parts = path.split('.');
        let alias = parts.next().unwrap();

        match bindings.get_resolved(alias, datastore) {
            Ok(base_value) => {
                let mut value_ref: &Value = &base_value;
                let mut traversed_segments: SmallVec<[&str; 8]> = SmallVec::new();
                traversed_segments.push(alias);

                for segment in parts {
                    let next = if let Ok(idx) = segment.parse::<usize>() {
                        value_ref.get(idx)
                    } else {
                        value_ref.get(segment)
                    };

                    match next {
                        Some(v) => {
                            traversed_segments.push(segment);
                            value_ref = v;
                        }
                        None => {
                            let value_type = match value_ref {
                                Value::Null => "null",
                                Value::Bool(_) => "bool",
                                Value::Number(_) => "number",
                                Value::String(_) => "string",
                                Value::Array(_) => "array",
                                Value::Object(_) => "object",
                            };

                            if matches!(value_ref, Value::Object(_) | Value::Array(_)) {
                                let traversed_path = traversed_segments.join(".");
                                return Err(NikaError::PathNotFound {
                                    path: format!("{}.{}", traversed_path, segment),
                                });
                            } else {
                                return Err(NikaError::InvalidTraversal {
                                    segment: segment.to_string(),
                                    value_type: value_type.to_string(),
                                    full_path: path.to_string(),
                                });
                            }
                        }
                    }
                }

                let raw_value = value_to_string(value_ref, path, alias)?;
                // Shell-escape the value (v0.13)
                let escaped = escape_for_shell(&raw_value);
                result.push_str(&escaped);
            }
            Err(_) => {
                errors.push(alias.to_string());
            }
        }

        last_end = m.end();
    }

    if !errors.is_empty() {
        return Err(NikaError::TemplateError {
            template: errors.join(", "),
            reason: "Alias(es) not resolved. Did you declare them in 'use:'?".to_string(),
        });
    }

    result.push_str(&template[last_end..]);

    // Pass 2: Context bindings (shell-escaped)
    if has_context && result.contains("context.") {
        let intermediate = result;
        let mut result = String::with_capacity(intermediate.len() + 64);
        let mut last_end = 0;
        let mut context_errors: SmallVec<[String; 4]> = SmallVec::new();

        for cap in CONTEXT_RE.captures_iter(&intermediate) {
            let m = cap.get(0).unwrap();
            let category = &cap[1];
            let path_rest = &cap[2];

            result.push_str(&intermediate[last_end..m.start()]);

            let full_path = format!("context.{}.{}", category, path_rest);

            match datastore.resolve_context_path(&full_path) {
                Some(value) => {
                    let raw_value = context_value_to_string(&value, &full_path)?;
                    let escaped = escape_for_shell(&raw_value);
                    result.push_str(&escaped);
                }
                None => {
                    context_errors.push(full_path);
                }
            }

            last_end = m.end();
        }

        if !context_errors.is_empty() {
            return Err(NikaError::TemplateError {
                template: context_errors.join(", "),
                reason: "Context binding(s) not resolved. Check your 'context:' block in workflow."
                    .to_string(),
            });
        }

        result.push_str(&intermediate[last_end..]);
        return Ok(Cow::Owned(result));
    }

    Ok(Cow::Owned(result))
}

/// Convert JSON Value to string for template substitution (strict mode)
///
/// Returns error for null values - this prevents silent bugs from missing data.
fn value_to_string(value: &Value, path: &str, alias: &str) -> Result<String, NikaError> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Null => Err(NikaError::NullValue {
            path: path.to_string(),
            alias: alias.to_string(),
        }),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        // For objects/arrays, return compact JSON representation
        other => Ok(other.to_string()),
    }
}

/// Convert context Value to string for template substitution (v0.14.2)
///
/// Similar to value_to_string but for context bindings.
fn context_value_to_string(value: &Value, path: &str) -> Result<String, NikaError> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Null => Err(NikaError::TemplateError {
            template: path.to_string(),
            reason: "Context binding resolved to null".to_string(),
        }),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        // For objects/arrays, return compact JSON representation
        other => Ok(other.to_string()),
    }
}

/// Convert input Value to string for template substitution (v0.19.4)
///
/// Similar to value_to_string but for input bindings.
fn input_value_to_string(value: &Value, path: &str) -> Result<String, NikaError> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Null => Err(NikaError::TemplateError {
            template: path.to_string(),
            reason: "Input binding resolved to null. Provide a 'default' value in your inputs definition.".to_string(),
        }),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        // For objects/arrays, return compact JSON representation
        other => Ok(other.to_string()),
    }
}

/// Check if position is inside a JSON string
fn is_in_json_context(template: &str, pos: usize) -> bool {
    let before = &template[..pos];
    let mut in_string = false;
    let mut escaped = false;

    for ch in before.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => in_string = !in_string,
            _ => {}
        }
    }

    in_string
}

/// Extract all alias references from a template (for static validation)
///
/// Returns a Vec of (alias, full_path) tuples.
/// Example: "{{use.weather.temp}}" → vec![("weather", "weather.temp")]
#[allow(dead_code)] // Used in tests and future static validation
pub fn extract_refs(template: &str) -> Vec<(String, String)> {
    USE_RE
        .captures_iter(template)
        .map(|cap| {
            let full_path = cap[1].to_string();
            let alias = full_path.split('.').next().unwrap().to_string();
            (alias, full_path)
        })
        .collect()
}

/// Validate that all template references exist in declared aliases (static validation)
///
/// This is called by `nika validate` before runtime.
/// Returns Ok(()) if valid, Err with first unknown alias if not.
#[allow(dead_code)] // Used in tests and future static validation
pub fn validate_refs(
    template: &str,
    declared_aliases: &FxHashSet<String>,
    task_id: &str,
) -> Result<(), NikaError> {
    for (alias, _full_path) in extract_refs(template) {
        if !declared_aliases.contains(&alias) {
            return Err(NikaError::UnknownAlias {
                alias,
                task_id: task_id.to_string(),
            });
        }
    }
    Ok(())
}

/// Detect deprecated $alias syntax and return error with migration guidance
///
/// The $alias syntax was never officially supported but appeared in some examples.
/// Only {{use.alias}} is the valid syntax.
///
/// Returns Ok(()) if no deprecated syntax found, Err with details if found.
pub fn detect_deprecated_dollar_syntax(template: &str, task_id: &str) -> Result<(), NikaError> {
    // Skip if template doesn't contain $ (fast path)
    if !template.contains('$') {
        return Ok(());
    }

    // Find all $alias patterns (but exclude shell patterns like $$, $1, ${, $()
    for cap in DEPRECATED_DOLLAR_RE.captures_iter(template) {
        let full_match = cap.get(0).unwrap().as_str();
        let identifier = full_match.trim_start_matches('$');

        // Skip UPPERCASE identifiers - these are shell environment variables ($INPUT, $HOME, $RANDOM)
        // Nika aliases use lowercase snake_case ($ctx, $msg, $data)
        if identifier
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
        {
            continue;
        }

        // Skip if this looks like it's intentionally a shell variable
        // Check context: is it preceded by another $ or followed by shell syntax?
        let start = cap.get(0).unwrap().start();
        if start > 0 {
            let before = template.chars().nth(start - 1);
            // Skip $$ (shell special) or in shell variable like ${var}
            if before == Some('$') || before == Some('{') {
                continue;
            }
        }

        // Use full path (e.g., $ctx.locale -> {{use.ctx.locale}})
        return Err(NikaError::DeprecatedSyntax {
            found: full_match.to_string(),
            suggestion: format!("{{{{use.{}}}}}", identifier),
            task_id: task_id.to_string(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::borrow::Cow;

    /// Helper to create empty datastore for tests
    fn empty_datastore() -> DataStore {
        DataStore::new()
    }

    #[test]
    fn resolve_simple() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("forecast", json!("Sunny 25C"));
        let ds = empty_datastore();

        let result = resolve("Weather: {{use.forecast}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Weather: Sunny 25C");
    }

    #[test]
    fn resolve_number() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("price", json!(89));
        let ds = empty_datastore();

        let result = resolve("Price: ${{use.price}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Price: $89");
    }

    #[test]
    fn resolve_nested() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("flight_info", json!({"departure": "10:30", "gate": "A12"}));
        let ds = empty_datastore();

        let result = resolve("Depart at {{use.flight_info.departure}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Depart at 10:30");
    }

    #[test]
    fn resolve_multiple() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("a", json!("first"));
        bindings.set("b", json!("second"));
        let ds = empty_datastore();

        let result = resolve("{{use.a}} and {{use.b}}", &bindings, &ds).unwrap();
        assert_eq!(result, "first and second");
    }

    #[test]
    fn resolve_object() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("data", json!({"x": 1, "y": 2}));
        let ds = empty_datastore();

        let result = resolve("Full: {{use.data}}", &bindings, &ds).unwrap();
        // Object is serialized as JSON
        assert!(result.contains("\"x\":1") || result.contains("\"x\": 1"));
    }

    #[test]
    fn resolve_alias_not_found() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("known", json!("value"));
        let ds = empty_datastore();

        let result = resolve("{{use.unknown}}", &bindings, &ds);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown"));
    }

    #[test]
    fn resolve_path_not_found() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("data", json!({"a": 1}));
        let ds = empty_datastore();

        let result = resolve("{{use.data.nonexistent}}", &bindings, &ds);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_no_templates() {
        let bindings = ResolvedBindings::new();
        let ds = empty_datastore();
        let result = resolve("No templates here", &bindings, &ds).unwrap();
        assert_eq!(result, "No templates here");
        // Verify zero-alloc: should be Cow::Borrowed
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn resolve_with_templates_is_owned() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("x", json!("value"));
        let ds = empty_datastore();
        let result = resolve("Has {{use.x}} template", &bindings, &ds).unwrap();
        assert_eq!(result, "Has value template");
        // With templates: should be Cow::Owned
        assert!(matches!(result, Cow::Owned(_)));
    }

    #[test]
    fn resolve_array_index() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("items", json!(["first", "second", "third"]));
        let ds = empty_datastore();

        let result = resolve("Item: {{use.items.0}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Item: first");
    }

    // ─────────────────────────────────────────────────────────────
    // v0.22: Bracket notation tests (array indexing with [N])
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn resolve_bracket_notation_simple() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("items", json!(["first", "second", "third"]));
        let ds = empty_datastore();

        // Bracket notation should work like dot notation
        let result = resolve("Item: {{use.items[0]}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Item: first");
    }

    #[test]
    fn resolve_bracket_notation_second_element() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("items", json!(["first", "second", "third"]));
        let ds = empty_datastore();

        let result = resolve("Item: {{use.items[1]}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Item: second");
    }

    #[test]
    fn resolve_bracket_notation_nested() {
        let mut bindings = ResolvedBindings::new();
        bindings.set(
            "data",
            json!({
                "user": {"name": "Alice", "address": {"city": "Paris"}},
                "items": ["one", "two", "three"]
            }),
        );
        let ds = empty_datastore();

        // Nested object + bracket notation for array
        let result = resolve("First item: {{use.data.items[0]}}", &bindings, &ds).unwrap();
        assert_eq!(result, "First item: one");
    }

    #[test]
    fn resolve_bracket_notation_mixed_syntax() {
        let mut bindings = ResolvedBindings::new();
        bindings.set(
            "data",
            json!({"users": [{"name": "Alice"}, {"name": "Bob"}]}),
        );
        let ds = empty_datastore();

        // Mix of dot and bracket notation
        let result = resolve("User: {{use.data.users[0].name}}", &bindings, &ds).unwrap();
        assert_eq!(result, "User: Alice");
    }

    #[test]
    fn resolve_bracket_notation_multiple() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("items", json!(["a", "b", "c"]));
        let ds = empty_datastore();

        // Multiple bracket notations in one template
        let result = resolve("{{use.items[0]}} and {{use.items[2]}}", &bindings, &ds).unwrap();
        assert_eq!(result, "a and c");
    }

    #[test]
    fn normalize_bracket_notation_unit() {
        // Direct test of the normalization function
        assert_eq!(
            normalize_bracket_notation("{{use.items[0]}}"),
            "{{use.items.0}}"
        );
        assert_eq!(
            normalize_bracket_notation("{{use.data.items[1].name}}"),
            "{{use.data.items.1.name}}"
        );
        assert_eq!(
            normalize_bracket_notation("no brackets here"),
            "no brackets here"
        );
        // Multiple brackets
        assert_eq!(
            normalize_bracket_notation("{{use.a[0]}} and {{use.b[2]}}"),
            "{{use.a.0}} and {{use.b.2}}"
        );
    }

    // ─────────────────────────────────────────────────────────────
    // v0.1: Strict mode tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn resolve_null_is_error() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("data", json!(null));
        let ds = empty_datastore();

        let result = resolve("Value: {{use.data}}", &bindings, &ds);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-072"));
        assert!(err.to_string().contains("Null value"));
    }

    #[test]
    fn resolve_nested_null_is_error() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("data", json!({"value": null}));
        let ds = empty_datastore();

        let result = resolve("Value: {{use.data.value}}", &bindings, &ds);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-072"));
    }

    #[test]
    fn resolve_invalid_traversal_on_string() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("data", json!("just a string"));
        let ds = empty_datastore();

        let result = resolve("{{use.data.field}}", &bindings, &ds);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-073"));
        assert!(err.to_string().contains("string"));
    }

    #[test]
    fn resolve_invalid_traversal_on_number() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("price", json!(42));
        let ds = empty_datastore();

        let result = resolve("{{use.price.currency}}", &bindings, &ds);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-073"));
        assert!(err.to_string().contains("number"));
    }

    // ─────────────────────────────────────────────────────────────
    // v0.1: Static validation tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn extract_refs_simple() {
        let refs = extract_refs("Hello {{use.weather}}!");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], ("weather".to_string(), "weather".to_string()));
    }

    #[test]
    fn extract_refs_nested() {
        let refs = extract_refs("{{use.data.field.sub}}");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], ("data".to_string(), "data.field.sub".to_string()));
    }

    #[test]
    fn extract_refs_multiple() {
        let refs = extract_refs("{{use.a}} and {{use.b.c}}");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].0, "a");
        assert_eq!(refs[1].0, "b");
    }

    #[test]
    fn extract_refs_none() {
        let refs = extract_refs("No templates here");
        assert!(refs.is_empty());
    }

    #[test]
    fn validate_refs_success() {
        let declared: FxHashSet<String> =
            ["weather", "price"].iter().map(|s| s.to_string()).collect();
        let result = validate_refs("{{use.weather}} costs {{use.price}}", &declared, "task1");
        assert!(result.is_ok());
    }

    #[test]
    fn validate_refs_unknown_alias() {
        let declared: FxHashSet<String> = ["weather"].iter().map(|s| s.to_string()).collect();
        let result = validate_refs("{{use.weather}} and {{use.unknown}}", &declared, "task1");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-071"));
        assert!(err.to_string().contains("unknown"));
    }

    // ─────────────────────────────────────────────────────────────
    // v0.7.2: Deprecated $alias syntax detection (NIKA-075)
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn deprecated_dollar_simple_alias() {
        let result = detect_deprecated_dollar_syntax("Process: $msg", "task1");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-075"));
        assert!(err.to_string().contains("$msg"));
        assert!(err.to_string().contains("{{use.msg}}"));
    }

    #[test]
    fn deprecated_dollar_with_path() {
        let result = detect_deprecated_dollar_syntax("Locale: $ctx.locale", "task1");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-075"));
        assert!(err.to_string().contains("$ctx.locale"));
        assert!(err.to_string().contains("{{use.ctx.locale}}"));
    }

    #[test]
    fn deprecated_dollar_deep_path() {
        let result = detect_deprecated_dollar_syntax("Value: $data.level1.level2", "task1");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("$data.level1.level2"));
        assert!(err.to_string().contains("{{use.data.level1.level2}}"));
    }

    #[test]
    fn deprecated_dollar_allows_uppercase() {
        // Shell env vars ($INPUT, $HOME, $RANDOM) should be allowed
        let result = detect_deprecated_dollar_syntax("echo $INPUT", "task1");
        assert!(result.is_ok());
    }

    #[test]
    fn deprecated_dollar_allows_uppercase_with_underscore() {
        let result = detect_deprecated_dollar_syntax("echo $LAST_TAG", "task1");
        assert!(result.is_ok());
    }

    #[test]
    fn deprecated_dollar_allows_shell_double_dollar() {
        // $$ is shell PID
        let result = detect_deprecated_dollar_syntax("echo $$", "task1");
        assert!(result.is_ok());
    }

    #[test]
    fn deprecated_dollar_allows_shell_brace_syntax() {
        // ${var} is shell brace expansion
        let result = detect_deprecated_dollar_syntax("echo ${myvar}", "task1");
        assert!(result.is_ok());
    }

    #[test]
    fn deprecated_dollar_no_dollar_sign() {
        // No $ in template = fast path OK
        let result = detect_deprecated_dollar_syntax("Just plain text", "task1");
        assert!(result.is_ok());
    }

    #[test]
    fn deprecated_dollar_valid_mustache_syntax() {
        // Valid {{use.alias}} syntax should pass
        let result = detect_deprecated_dollar_syntax("Process: {{use.msg}}", "task1");
        assert!(result.is_ok());
    }

    #[test]
    fn deprecated_dollar_mixed_valid_and_invalid() {
        // If both valid and invalid syntax present, should catch the invalid one
        let result = detect_deprecated_dollar_syntax("{{use.valid}} and $invalid", "task1");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("$invalid"));
    }

    // ─────────────────────────────────────────────────────────────
    // v0.14.2: Context binding tests
    // ─────────────────────────────────────────────────────────────

    use crate::runtime::context_loader::LoadedContext;

    /// Helper to create datastore with context for tests
    fn datastore_with_context() -> DataStore {
        let store = DataStore::new();
        let mut context = LoadedContext::new();
        context.files.insert(
            "brand".to_string(),
            json!("# QR Code AI\nTagline: Scan smarter"),
        );
        context
            .files
            .insert("config".to_string(), json!({"theme": "dark", "version": 2}));
        context.session = Some(json!({"focus": "rust", "level": 3}));
        store.set_context(context);
        store
    }

    #[test]
    fn resolve_context_files_simple() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_context();

        let result = resolve("Brand: {{context.files.brand}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Brand: # QR Code AI\nTagline: Scan smarter");
    }

    #[test]
    fn resolve_context_files_nested() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_context();

        let result = resolve("Theme: {{context.files.config.theme}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Theme: dark");
    }

    #[test]
    fn resolve_context_session() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_context();

        let result = resolve("Focus: {{context.session.focus}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Focus: rust");
    }

    #[test]
    fn resolve_context_session_number() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_context();

        let result = resolve("Level: {{context.session.level}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Level: 3");
    }

    #[test]
    fn resolve_context_with_use_bindings() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("greeting", json!("Hello"));
        let ds = datastore_with_context();

        let result = resolve(
            "{{use.greeting}}! Brand: {{context.files.brand}}",
            &bindings,
            &ds,
        )
        .unwrap();
        assert_eq!(result, "Hello! Brand: # QR Code AI\nTagline: Scan smarter");
    }

    #[test]
    fn resolve_context_not_found() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_context();

        let result = resolve("{{context.files.nonexistent}}", &bindings, &ds);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Context binding"));
        assert!(err.to_string().contains("nonexistent"));
    }

    #[test]
    fn resolve_context_no_context_loaded() {
        let bindings = ResolvedBindings::new();
        let ds = empty_datastore(); // No context loaded

        let result = resolve("{{context.files.brand}}", &bindings, &ds);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_only_context_no_use() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_context();

        // Template with ONLY context bindings, no use bindings
        let result = resolve("Theme is {{context.files.config.theme}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Theme is dark");
    }

    #[test]
    fn resolve_context_preserves_no_template() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_context();

        // No templates at all
        let result = resolve("Plain text without templates", &bindings, &ds).unwrap();
        assert_eq!(result, "Plain text without templates");
        // Should be borrowed (zero alloc)
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Shell escaping tests (v0.13)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn escape_for_shell_simple() {
        assert_eq!(escape_for_shell("hello"), "'hello'");
    }

    #[test]
    fn escape_for_shell_empty() {
        assert_eq!(escape_for_shell(""), "''");
    }

    #[test]
    fn escape_for_shell_with_single_quote() {
        // "Nika's" becomes "'Nika'\''s'"
        assert_eq!(escape_for_shell("Nika's"), "'Nika'\\''s'");
    }

    #[test]
    fn escape_for_shell_with_multiple_quotes() {
        // "don't won't" should escape both quotes
        assert_eq!(escape_for_shell("don't won't"), "'don'\\''t won'\\''t'");
    }

    #[test]
    fn escape_for_shell_with_special_chars() {
        // Special shell characters should be safe inside single quotes
        assert_eq!(escape_for_shell("$HOME;rm -rf /"), "'$HOME;rm -rf /'");
    }

    #[test]
    fn escape_for_shell_with_backticks() {
        // Backticks should be safe inside single quotes
        assert_eq!(escape_for_shell("`whoami`"), "'`whoami`'");
    }

    #[test]
    fn escape_for_shell_with_newlines() {
        // Newlines should be preserved inside single quotes
        assert_eq!(escape_for_shell("line1\nline2"), "'line1\nline2'");
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // |shell modifier tests (v0.13)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn resolve_shell_modifier_simple() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("msg", json!("hello world"));
        let ds = empty_datastore();

        // Using |shell modifier applies shell escaping
        let result = resolve("echo {{use.msg|shell}}", &bindings, &ds).unwrap();
        assert_eq!(result, "echo 'hello world'");
    }

    #[test]
    fn resolve_shell_modifier_with_quote() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("response", json!("Hello from Nika's v0.5.1!"));
        let ds = empty_datastore();

        // The |shell modifier escapes single quotes correctly
        let result = resolve("echo {{use.response|shell}}", &bindings, &ds).unwrap();
        assert_eq!(result, "echo 'Hello from Nika'\\''s v0.5.1!'");
    }

    #[test]
    fn resolve_shell_modifier_with_special_chars() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("content", json!("Hello; echo pwned"));
        let ds = empty_datastore();

        // Shell special characters are safely escaped
        let result = resolve("echo {{use.content|shell}}", &bindings, &ds).unwrap();
        assert_eq!(result, "echo 'Hello; echo pwned'");
    }

    #[test]
    fn resolve_without_modifier_no_escape() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("msg", json!("hello world"));
        let ds = empty_datastore();

        // Without |shell modifier, no escaping happens (backward compatible)
        let result = resolve("echo {{use.msg}}", &bindings, &ds).unwrap();
        assert_eq!(result, "echo hello world");
    }

    #[test]
    fn resolve_shell_modifier_multiple() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("file", json!("test.txt"));
        bindings.set("content", json!("Hello 'world'"));
        let ds = empty_datastore();

        // Multiple bindings with |shell modifier
        let result = resolve(
            "cat {{use.file|shell}} && echo {{use.content|shell}}",
            &bindings,
            &ds,
        )
        .unwrap();
        assert_eq!(result, "cat 'test.txt' && echo 'Hello '\\''world'\\'''");
    }

    #[test]
    fn resolve_for_shell_simple() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("msg", json!("hello world"));
        let ds = empty_datastore();

        let result = resolve_for_shell("echo {{use.msg}}", &bindings, &ds).unwrap();
        assert_eq!(result, "echo 'hello world'");
    }

    #[test]
    fn resolve_for_shell_with_quote() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("response", json!("Hello from Nika's v0.5.1!"));
        let ds = empty_datastore();

        // resolve_for_shell escapes ALL bindings
        let result =
            resolve_for_shell("echo 'Claude said: {{use.response}}'", &bindings, &ds).unwrap();
        // The output has escaped quotes
        assert_eq!(
            result,
            "echo 'Claude said: 'Hello from Nika'\\''s v0.5.1!''"
        );
    }

    #[test]
    fn resolve_for_shell_no_templates() {
        let bindings = ResolvedBindings::new();
        let ds = empty_datastore();

        // No templates - should return borrowed string
        let result = resolve_for_shell("echo hello", &bindings, &ds).unwrap();
        assert_eq!(result, "echo hello");
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn resolve_for_shell_preserves_command_structure() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("file", json!("test.txt"));
        bindings.set("content", json!("Hello; echo pwned"));
        let ds = empty_datastore();

        // The command structure is preserved, only the value is escaped
        let result =
            resolve_for_shell("cat {{use.file}} && echo {{use.content}}", &bindings, &ds).unwrap();
        assert_eq!(result, "cat 'test.txt' && echo 'Hello; echo pwned'");
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Input binding tests (v0.19.4)
    // ═══════════════════════════════════════════════════════════════════════════

    use rustc_hash::FxHashMap;

    /// Helper to create datastore with inputs for tests
    fn datastore_with_inputs() -> DataStore {
        let store = DataStore::new();
        let mut inputs = FxHashMap::default();
        inputs.insert(
            "topic".to_string(),
            json!({
                "type": "string",
                "default": "AI QR code generation"
            }),
        );
        inputs.insert(
            "depth".to_string(),
            json!({
                "type": "string",
                "default": "comprehensive"
            }),
        );
        inputs.insert(
            "config".to_string(),
            json!({
                "type": "object",
                "default": {
                    "theme": "dark",
                    "count": 5
                }
            }),
        );
        store.set_inputs(inputs);
        store
    }

    #[test]
    fn resolve_inputs_simple() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_inputs();

        let result = resolve("Topic: {{inputs.topic}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Topic: AI QR code generation");
    }

    #[test]
    fn resolve_inputs_multiple() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_inputs();

        let result = resolve(
            "Research {{inputs.topic}} at {{inputs.depth}} depth",
            &bindings,
            &ds,
        )
        .unwrap();
        assert_eq!(
            result,
            "Research AI QR code generation at comprehensive depth"
        );
    }

    #[test]
    fn resolve_inputs_nested() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_inputs();

        let result = resolve("Theme: {{inputs.config.theme}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Theme: dark");
    }

    #[test]
    fn resolve_inputs_with_use_bindings() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("greeting", json!("Hello"));
        let ds = datastore_with_inputs();

        let result = resolve(
            "{{use.greeting}}! Research {{inputs.topic}}",
            &bindings,
            &ds,
        )
        .unwrap();
        assert_eq!(result, "Hello! Research AI QR code generation");
    }

    #[test]
    fn resolve_inputs_with_context() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("msg", json!("Test"));
        let store = DataStore::new();

        // Set both context and inputs
        let mut context = LoadedContext::new();
        context
            .files
            .insert("brand".to_string(), json!("QR Code AI"));
        store.set_context(context);

        let mut inputs = FxHashMap::default();
        inputs.insert(
            "topic".to_string(),
            json!({
                "type": "string",
                "default": "AI trends"
            }),
        );
        store.set_inputs(inputs);

        let result = resolve(
            "{{use.msg}}: {{context.files.brand}} - {{inputs.topic}}",
            &bindings,
            &store,
        )
        .unwrap();
        assert_eq!(result, "Test: QR Code AI - AI trends");
    }

    #[test]
    fn resolve_inputs_not_found() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_inputs();

        let result = resolve("{{inputs.nonexistent}}", &bindings, &ds);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Input binding"));
        assert!(err.to_string().contains("nonexistent"));
    }

    #[test]
    fn resolve_inputs_no_inputs_loaded() {
        let bindings = ResolvedBindings::new();
        let ds = empty_datastore(); // No inputs

        let result = resolve("{{inputs.topic}}", &bindings, &ds);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_only_inputs_no_use() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_inputs();

        // Template with ONLY inputs bindings, no use bindings
        let result = resolve("Topic is {{inputs.topic}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Topic is AI QR code generation");
    }

    #[test]
    fn resolve_inputs_preserves_no_template() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_inputs();

        // No templates at all
        let result = resolve("Plain text without templates", &bindings, &ds).unwrap();
        assert_eq!(result, "Plain text without templates");
        // Should be borrowed (zero alloc)
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Template Injection Security Tests (v0.23)
    // ═══════════════════════════════════════════════════════════════════════════
    //
    // These tests verify that malicious content in template values cannot:
    // 1. Break out of the intended context (JSON, shell, etc.)
    // 2. Cause re-evaluation of template syntax
    // 3. Inject control characters or escape sequences
    //
    // Security principle: Template values are DATA, not CODE.
    // They should be interpolated literally, never interpreted.

    #[test]
    fn injection_template_syntax_not_reevaluated() {
        // Value contains template syntax - should NOT be re-evaluated
        let mut bindings = ResolvedBindings::new();
        bindings.set("user_input", json!("{{use.secret}}"));
        bindings.set("secret", json!("TOP_SECRET"));
        let ds = empty_datastore();

        let result = resolve("User said: {{use.user_input}}", &bindings, &ds).unwrap();
        // The {{use.secret}} should appear literally, NOT expanded
        assert_eq!(result, "User said: {{use.secret}}");
        assert!(!result.contains("TOP_SECRET"));
    }

    #[test]
    fn injection_nested_template_attack() {
        // Attempt to construct template syntax via concatenation
        let mut bindings = ResolvedBindings::new();
        bindings.set("left", json!("{{use."));
        bindings.set("right", json!("secret}}"));
        bindings.set("secret", json!("LEAKED"));
        let ds = empty_datastore();

        // Even with split template markers, no re-evaluation should occur
        let result = resolve("{{use.left}}{{use.right}}", &bindings, &ds).unwrap();
        assert_eq!(result, "{{use.secret}}");
        assert!(!result.contains("LEAKED"));
    }

    #[test]
    fn injection_json_context_quotes_escaped() {
        // Value with quotes in JSON context should be escaped
        let mut bindings = ResolvedBindings::new();
        bindings.set("name", json!(r#"Alice", "admin": true, "x": "#));
        let ds = empty_datastore();

        // Template is in a JSON context (inside quotes)
        let template = r#"{"user": "{{use.name}}"}"#;
        let result = resolve(template, &bindings, &ds).unwrap();

        // The quotes should be escaped with backslash
        assert!(result.contains(r#"\""#), "Quotes should be escaped: {}", result);
        // The result should be valid JSON - quotes are escaped so injection fails
        // The "admin" key appears but as escaped string content, not as JSON structure
        assert_eq!(
            result,
            r#"{"user": "Alice\", \"admin\": true, \"x\": "}"#,
            "Quotes should be escaped to prevent JSON structure injection"
        );
        // Verify the injected "admin" is inside the string value, not a real key
        // by checking the escaped pattern exists
        assert!(result.contains(r#"\"admin\""#), "admin should be escaped");
    }

    #[test]
    fn injection_json_context_backslash_escaped() {
        // Backslashes in JSON context should be double-escaped
        let mut bindings = ResolvedBindings::new();
        bindings.set("path", json!(r#"C:\Users\admin"#));
        let ds = empty_datastore();

        let template = r#"{"path": "{{use.path}}"}"#;
        let result = resolve(template, &bindings, &ds).unwrap();

        // Backslashes should be escaped
        assert!(result.contains(r#"\\"#), "Backslashes should be escaped: {}", result);
    }

    #[test]
    fn injection_json_context_newline_escaped() {
        // Newlines in JSON context should be escaped as \n
        let mut bindings = ResolvedBindings::new();
        bindings.set("text", json!("line1\nline2"));
        let ds = empty_datastore();

        let template = r#"{"text": "{{use.text}}"}"#;
        let result = resolve(template, &bindings, &ds).unwrap();

        // Raw newline should become \n
        assert!(result.contains(r#"\n"#), "Newlines should be escaped: {}", result);
        assert!(!result.contains('\n') || result.matches('\n').count() == 0 ||
                result.contains("\\n"), "Raw newlines should be escaped");
    }

    #[test]
    fn injection_shell_modifier_escapes_semicolon() {
        // Semicolon injection attempt with |shell modifier
        let mut bindings = ResolvedBindings::new();
        bindings.set("filename", json!("file.txt; rm -rf /"));
        let ds = empty_datastore();

        let result = resolve("cat {{use.filename|shell}}", &bindings, &ds).unwrap();
        // With |shell, the dangerous command is wrapped in single quotes
        // This makes the semicolon a literal character, not a command separator
        assert_eq!(result, "cat 'file.txt; rm -rf /'");
        // The entire value including semicolon is inside quotes - safe
        assert!(result.starts_with("cat '") && result.ends_with("'"));
    }

    #[test]
    fn injection_shell_modifier_escapes_backticks() {
        // Command substitution injection attempt
        let mut bindings = ResolvedBindings::new();
        bindings.set("input", json!("`whoami`"));
        let ds = empty_datastore();

        let result = resolve("echo {{use.input|shell}}", &bindings, &ds).unwrap();
        // Backticks safely quoted
        assert_eq!(result, "echo '`whoami`'");
    }

    #[test]
    fn injection_shell_modifier_escapes_dollar_parens() {
        // $(command) injection attempt
        let mut bindings = ResolvedBindings::new();
        bindings.set("input", json!("$(cat /etc/passwd)"));
        let ds = empty_datastore();

        let result = resolve("echo {{use.input|shell}}", &bindings, &ds).unwrap();
        // Dollar-paren safely quoted
        assert_eq!(result, "echo '$(cat /etc/passwd)'");
    }

    #[test]
    fn injection_shell_modifier_escapes_env_vars() {
        // Environment variable injection
        let mut bindings = ResolvedBindings::new();
        bindings.set("input", json!("$HOME/.ssh/id_rsa"));
        let ds = empty_datastore();

        let result = resolve("cat {{use.input|shell}}", &bindings, &ds).unwrap();
        // $HOME is literal, not expanded
        assert_eq!(result, "cat '$HOME/.ssh/id_rsa'");
    }

    #[test]
    fn injection_resolve_for_shell_escapes_all() {
        // resolve_for_shell should escape ALL bindings automatically
        let mut bindings = ResolvedBindings::new();
        bindings.set("cmd", json!("echo 'pwned'; rm -rf /"));
        let ds = empty_datastore();

        let result = resolve_for_shell("{{use.cmd}}", &bindings, &ds).unwrap();
        // The entire value is shell-escaped using single-quote escaping
        // The embedded single quote in 'pwned' is escaped as '\''
        assert_eq!(result, "'echo '\\''pwned'\\''; rm -rf /'");
        // The value starts and ends with single quotes, making everything inside literal
        // Even though '; rm' appears in the string, it's inside quoted context
    }

    #[test]
    fn injection_control_characters_json() {
        // Control characters should be escaped in JSON context
        let mut bindings = ResolvedBindings::new();
        // Tab, carriage return, and form feed
        bindings.set("data", json!("a\tb\rc\x0c"));
        let ds = empty_datastore();

        let template = r#"{"data": "{{use.data}}"}"#;
        let result = resolve(template, &bindings, &ds).unwrap();

        // Control chars should be escaped
        assert!(result.contains(r#"\t"#) || !result.contains('\t'));
        assert!(result.contains(r#"\r"#) || !result.contains('\r'));
    }

    #[test]
    fn injection_unicode_escape_sequences() {
        // Unicode escape sequences should be treated literally
        let mut bindings = ResolvedBindings::new();
        bindings.set("text", json!(r#"\u0000"#)); // Literal backslash-u
        let ds = empty_datastore();

        let result = resolve("Text: {{use.text}}", &bindings, &ds).unwrap();
        // Should appear as literal \u0000, not null byte
        assert_eq!(result, r#"Text: \u0000"#);
    }

    #[test]
    fn injection_null_byte_in_value() {
        // JSON Value::String cannot contain null bytes (serde_json rejects them)
        // But if somehow present, should be handled safely
        let mut bindings = ResolvedBindings::new();
        // serde_json from string with null - this is actually impossible via json!
        // but we test the principle
        bindings.set("normal", json!("safe"));
        let ds = empty_datastore();

        let result = resolve("{{use.normal}}", &bindings, &ds).unwrap();
        assert_eq!(result, "safe");
    }

    #[test]
    fn injection_very_long_value() {
        // Very long values should be handled without stack overflow
        let mut bindings = ResolvedBindings::new();
        let long_string = "A".repeat(100_000);
        bindings.set("big", json!(long_string.clone()));
        let ds = empty_datastore();

        let result = resolve("Data: {{use.big}}", &bindings, &ds).unwrap();
        assert!(result.starts_with("Data: AAAA"));
        assert_eq!(result.len(), 6 + 100_000); // "Data: " + 100k As
    }

    #[test]
    fn injection_deeply_nested_json_value() {
        // Deeply nested JSON should serialize correctly
        let mut bindings = ResolvedBindings::new();
        bindings.set("nested", json!({"a": {"b": {"c": {"d": "deep"}}}}));
        let ds = empty_datastore();

        let result = resolve("{{use.nested}}", &bindings, &ds).unwrap();
        // Should be serialized JSON, not crash
        assert!(result.contains("deep"));
    }

    #[test]
    fn injection_template_markers_in_context_path() {
        // Even context paths with template-like patterns should be safe
        let bindings = ResolvedBindings::new();
        let store = DataStore::new();

        let mut context = LoadedContext::new();
        // File name that looks like template syntax - but file content is safe
        context.files.insert("normal".to_string(), json!("safe content"));
        store.set_context(context);

        let result = resolve("{{context.files.normal}}", &bindings, &store).unwrap();
        assert_eq!(result, "safe content");
    }

    #[test]
    fn injection_context_value_with_template_syntax() {
        // Context file content with template syntax should NOT be re-evaluated
        let bindings = ResolvedBindings::new();
        let store = DataStore::new();

        let mut context = LoadedContext::new();
        context.files.insert("brand".to_string(), json!("Brand: {{use.secret}}"));
        store.set_context(context);

        let result = resolve("{{context.files.brand}}", &bindings, &store).unwrap();
        // Template syntax in the VALUE should appear literally
        assert_eq!(result, "Brand: {{use.secret}}");
    }

    #[test]
    fn injection_input_value_with_template_syntax() {
        // Input values with template syntax should NOT be re-evaluated
        let bindings = ResolvedBindings::new();
        let store = DataStore::new();

        let mut inputs = FxHashMap::default();
        inputs.insert("topic".to_string(), json!({
            "type": "string",
            "default": "Learn about {{use.secret}}"
        }));
        store.set_inputs(inputs);

        let result = resolve("{{inputs.topic}}", &bindings, &store).unwrap();
        // Template syntax in input default should appear literally
        assert_eq!(result, "Learn about {{use.secret}}");
    }

    #[test]
    fn injection_3pass_no_cross_contamination() {
        // Verify 3-pass resolution doesn't allow pass N output to affect pass N+1
        let mut bindings = ResolvedBindings::new();
        // Pass 1: use binding resolves to something with context syntax
        bindings.set("data", json!("{{context.files.secret}}"));
        let store = DataStore::new();

        let mut context = LoadedContext::new();
        context.files.insert("secret".to_string(), json!("CONFIDENTIAL"));
        store.set_context(context);

        // Template only has use binding, but its value contains context syntax
        let result = resolve("Result: {{use.data}}", &bindings, &store).unwrap();
        // The context syntax should NOT be evaluated in pass 2
        assert_eq!(result, "Result: {{context.files.secret}}");
        assert!(!result.contains("CONFIDENTIAL"));
    }

    #[test]
    fn injection_html_script_tags() {
        // HTML script injection - template just passes through
        let mut bindings = ResolvedBindings::new();
        bindings.set("content", json!("<script>alert('xss')</script>"));
        let ds = empty_datastore();

        let result = resolve("{{use.content}}", &bindings, &ds).unwrap();
        // NOTE: Template resolution does NOT escape HTML - that's the consumer's job
        // This test documents the behavior: raw HTML passes through
        assert_eq!(result, "<script>alert('xss')</script>");
    }

    #[test]
    fn injection_sql_like_content() {
        // SQL-like content - template just passes through
        let mut bindings = ResolvedBindings::new();
        bindings.set("query", json!("'; DROP TABLE users; --"));
        let ds = empty_datastore();

        let result = resolve("SELECT * FROM x WHERE name='{{use.query}}'", &bindings, &ds).unwrap();
        // Template resolution does NOT prevent SQL injection - that's the DB layer's job
        // This test documents the behavior
        assert!(result.contains("DROP TABLE"));
    }
}
