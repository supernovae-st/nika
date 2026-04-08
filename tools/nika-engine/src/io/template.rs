// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Template Resolver - Variable Interpolation for Artifact Paths
//!
//! Provides variable interpolation for artifact output paths.
//! Supports task context, timestamps, and custom formats.
//!
//! # Supported Variables
//!
//! | Variable | Description | Example |
//! |----------|-------------|---------|
//! | `{{task_id}}` | Current task ID | `generate_report` |
//! | `{{workflow_name}}` | Workflow name | `my-workflow` |
//! | `{{workflow}}` | Alias for workflow_name | `my-workflow` |
//! | `{{date}}` | Current date (ISO) | `2024-01-15` |
//! | `{{time}}` | Current time (ISO) | `14-30-00` |
//! | `{{timestamp}}` | Unix timestamp | `1705329000` |
//! | `{{uuid}}` | Random UUID v4 | `550e8400-e29b-41d4-a716-446655440000` |
//!
//! # Example
//!
//! ```ignore
//! use nika::io::template::TemplateResolver;
//!
//! let resolver = TemplateResolver::new("generate_report", "my-workflow");
//! let path = resolver.resolve("{{task_id}}/{{date}}/output.json")?;
//! // Returns: "generate_report/2024-01-15/output.json"
//! ```

use std::collections::HashMap;

use chrono::{DateTime, Local};
use uuid::Uuid;

use crate::error::NikaError;
use crate::error_domains::BindingError;

/// Characters that are forbidden in custom variable values
/// to prevent path traversal attacks
const FORBIDDEN_VAR_CHARS: &[char] = &['/', '\\', '\0'];

/// Patterns that are forbidden in custom variable values
const FORBIDDEN_VAR_PATTERNS: &[&str] = &["..", "~"];

/// Validate a custom variable value for security
///
/// Rejects values that could be used for path traversal attacks.
fn validate_var_value(key: &str, value: &str) -> Result<(), NikaError> {
    // Empty values are allowed (they just produce empty output)
    if value.is_empty() {
        return Ok(());
    }

    // Check for forbidden characters
    for c in FORBIDDEN_VAR_CHARS {
        if value.contains(*c) {
            return Err(BindingError::TemplateError {
                template: format!("{{{{{}}}}}", key),
                reason: format!(
                    "Variable value contains forbidden character '{}': path traversal risk",
                    c
                ),
            }
            .into());
        }
    }

    // Check for forbidden patterns
    for pattern in FORBIDDEN_VAR_PATTERNS {
        if value.contains(pattern) {
            return Err(BindingError::TemplateError {
                template: format!("{{{{{}}}}}", key),
                reason: format!(
                    "Variable value contains forbidden pattern '{}': path traversal risk",
                    pattern
                ),
            }
            .into());
        }
    }

    Ok(())
}

/// Template variable resolver for artifact paths
#[derive(Debug)]
pub struct TemplateResolver {
    /// Current task ID
    task_id: String,
    /// Workflow name
    workflow_name: String,
    /// Current timestamp for consistent date/time across all variables
    timestamp: DateTime<Local>,
    /// Additional custom variables
    custom_vars: HashMap<String, String>,
}

impl TemplateResolver {
    /// Create a new template resolver
    pub fn new(task_id: impl Into<String>, workflow_name: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            workflow_name: workflow_name.into(),
            timestamp: Local::now(),
            custom_vars: HashMap::new(),
        }
    }

    /// Add a custom variable
    ///
    /// # Errors
    ///
    /// Returns `NikaError::TemplateError` if the key is empty or if the value
    /// contains path traversal patterns (e.g., `..`, `/`, `\`).
    #[cfg(test)]
    pub fn with_var(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self, NikaError> {
        let key = key.into();
        let value = value.into();

        // Reject empty variable names
        if key.is_empty() {
            return Err(BindingError::TemplateError {
                template: "{{}}".to_string(),
                reason: "Variable name cannot be empty".to_string(),
            }
            .into());
        }

        // Validate value for path traversal
        validate_var_value(&key, &value)?;

        self.custom_vars.insert(key, value);
        Ok(self)
    }

    /// Add multiple custom variables
    ///
    /// # Errors
    ///
    /// Returns `NikaError::TemplateError` if any key is empty or if any value
    /// contains path traversal patterns.
    pub fn with_vars(mut self, vars: HashMap<String, String>) -> Result<Self, NikaError> {
        for (key, value) in &vars {
            if key.is_empty() {
                return Err(BindingError::TemplateError {
                    template: "{{}}".to_string(),
                    reason: "Variable name cannot be empty".to_string(),
                }
                .into());
            }
            validate_var_value(key, value)?;
        }
        self.custom_vars.extend(vars);
        Ok(self)
    }

    /// Set a specific timestamp (useful for testing)
    #[cfg(test)]
    pub fn with_timestamp(mut self, timestamp: DateTime<Local>) -> Self {
        self.timestamp = timestamp;
        self
    }

    /// Resolve all template variables in a path string
    ///
    /// # Arguments
    ///
    /// * `template` - Path template with `{{var}}` placeholders
    ///
    /// # Returns
    ///
    /// Resolved path string with all variables substituted
    ///
    /// # Errors
    ///
    /// Returns `NikaError::TemplateError` if an unknown variable is referenced
    pub fn resolve(&self, template: &str) -> Result<String, NikaError> {
        let mut result = template.to_string();
        let mut pos = 0;

        while let Some(start) = result[pos..].find("{{") {
            let start = pos + start;
            let Some(end) = result[start..].find("}}") else {
                // Unclosed template, treat as literal
                break;
            };
            let end = start + end + 2;

            let var_name = &result[start + 2..end - 2].trim();
            let value = self.resolve_variable(var_name)?;

            result.replace_range(start..end, &value);
            pos = start + value.len();
        }

        Ok(result)
    }

    /// Resolve a single variable name to its value
    fn resolve_variable(&self, var_name: &str) -> Result<String, NikaError> {
        // Check for date format specifier: date.FORMAT
        if let Some(format) = var_name.strip_prefix("date.") {
            return Ok(self.format_date(format));
        }

        // Check for time format specifier: time.FORMAT
        if let Some(format) = var_name.strip_prefix("time.") {
            return Ok(self.format_time(format));
        }

        // Built-in variables
        match var_name {
            "task_id" => Ok(self.task_id.clone()),
            // "workflow" alias for "workflow_name" (shorter, intuitive)
            "workflow_name" | "workflow" => Ok(self.workflow_name.clone()),
            "date" => Ok(self.timestamp.format("%Y-%m-%d").to_string()),
            "time" => Ok(self.timestamp.format("%H-%M-%S").to_string()),
            "timestamp" => Ok(self.timestamp.timestamp().to_string()),
            "uuid" => Ok(Uuid::new_v4().to_string()),
            _ => {
                // Check custom variables
                if let Some(value) = self.custom_vars.get(var_name) {
                    return Ok(value.clone());
                }

                Err(BindingError::TemplateError {
                    template: format!("{{{{{}}}}}", var_name),
                    reason: format!("Unknown template variable: {}", var_name),
                }
                .into())
            }
        }
    }

    /// Format date with custom format string
    ///
    /// Supported format specifiers:
    /// - `YYYY` - 4-digit year
    /// - `MM` - 2-digit month
    /// - `DD` - 2-digit day
    fn format_date(&self, format: &str) -> String {
        let mut result = format.to_string();
        result = result.replace("YYYY", &self.timestamp.format("%Y").to_string());
        result = result.replace("MM", &self.timestamp.format("%m").to_string());
        result = result.replace("DD", &self.timestamp.format("%d").to_string());
        result
    }

    /// Format time with custom format string
    ///
    /// Supported format specifiers:
    /// - `HH` - 2-digit hour (24h)
    /// - `mm` - 2-digit minute
    /// - `ss` - 2-digit second
    fn format_time(&self, format: &str) -> String {
        let mut result = format.to_string();
        result = result.replace("HH", &self.timestamp.format("%H").to_string());
        result = result.replace("mm", &self.timestamp.format("%M").to_string());
        result = result.replace("ss", &self.timestamp.format("%S").to_string());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixed_resolver() -> TemplateResolver {
        let ts = Local.with_ymd_and_hms(2024, 1, 15, 14, 30, 45).unwrap();
        TemplateResolver::new("test_task", "test_workflow").with_timestamp(ts)
    }

    #[test]
    fn test_resolve_task_id() {
        let resolver = fixed_resolver();
        let result = resolver.resolve("{{task_id}}/output.json").unwrap();
        assert_eq!(result, "test_task/output.json");
    }

    #[test]
    fn test_resolve_workflow_name() {
        let resolver = fixed_resolver();
        let result = resolver
            .resolve("{{workflow_name}}/{{task_id}}.json")
            .unwrap();
        assert_eq!(result, "test_workflow/test_task.json");
    }

    #[test]
    fn test_resolve_date() {
        let resolver = fixed_resolver();
        let result = resolver.resolve("{{date}}/output.json").unwrap();
        assert_eq!(result, "2024-01-15/output.json");
    }

    #[test]
    fn test_resolve_time() {
        let resolver = fixed_resolver();
        let result = resolver.resolve("{{time}}.json").unwrap();
        assert_eq!(result, "14-30-45.json");
    }

    #[test]
    fn test_resolve_timestamp() {
        let resolver = fixed_resolver();
        let result = resolver.resolve("{{timestamp}}.json").unwrap();
        // Just verify it's a number
        assert!(result.ends_with(".json"));
        let ts_str = result.strip_suffix(".json").unwrap();
        assert!(ts_str.parse::<i64>().is_ok());
    }

    #[test]
    fn test_resolve_uuid() {
        let resolver = fixed_resolver();
        let result = resolver.resolve("{{uuid}}.json").unwrap();
        // Just verify format (UUID v4 is random)
        assert!(result.ends_with(".json"));
        let uuid_str = result.strip_suffix(".json").unwrap();
        assert!(Uuid::parse_str(uuid_str).is_ok());
    }

    #[test]
    fn test_resolve_date_format() {
        let resolver = fixed_resolver();
        let result = resolver.resolve("{{date.YYYY-MM-DD}}.json").unwrap();
        assert_eq!(result, "2024-01-15.json");
    }

    #[test]
    fn test_resolve_date_format_custom() {
        let resolver = fixed_resolver();
        let result = resolver.resolve("{{date.YYYY/MM/DD}}.json").unwrap();
        assert_eq!(result, "2024/01/15.json");
    }

    #[test]
    fn test_resolve_time_format() {
        let resolver = fixed_resolver();
        let result = resolver.resolve("{{time.HH-mm-ss}}.json").unwrap();
        assert_eq!(result, "14-30-45.json");
    }

    #[test]
    fn test_resolve_custom_var() {
        let resolver = fixed_resolver().with_var("entity", "qr-code").unwrap();
        let result = resolver.resolve("{{entity}}/{{task_id}}.json").unwrap();
        assert_eq!(result, "qr-code/test_task.json");
    }

    #[test]
    fn test_resolve_multiple_vars() {
        let mut vars = HashMap::new();
        vars.insert("locale".to_string(), "fr-FR".to_string());
        vars.insert("version".to_string(), "v1".to_string());

        let resolver = fixed_resolver().with_vars(vars).unwrap();
        let result = resolver
            .resolve("{{locale}}/{{version}}/{{task_id}}.json")
            .unwrap();
        assert_eq!(result, "fr-FR/v1/test_task.json");
    }

    #[test]
    fn test_var_path_traversal_rejected() {
        let result = fixed_resolver().with_var("entity", "../escape");
        assert!(result.is_err());
        let err = result.unwrap_err();
        if let NikaError::TemplateError { reason, .. } = err {
            assert!(reason.contains("path traversal"));
        } else {
            panic!("Expected TemplateError");
        }
    }

    #[test]
    fn test_var_slash_rejected() {
        let result = fixed_resolver().with_var("path", "a/b/c");
        assert!(result.is_err());
        let err = result.unwrap_err();
        if let NikaError::TemplateError { reason, .. } = err {
            assert!(reason.contains("forbidden character"));
        } else {
            panic!("Expected TemplateError");
        }
    }

    #[test]
    fn test_empty_var_name_rejected() {
        let result = fixed_resolver().with_var("", "value");
        assert!(result.is_err());
        let err = result.unwrap_err();
        if let NikaError::TemplateError { reason, .. } = err {
            assert!(reason.contains("empty"));
        } else {
            panic!("Expected TemplateError");
        }
    }

    #[test]
    fn test_empty_var_value_allowed() {
        let resolver = fixed_resolver().with_var("empty", "").unwrap();
        let result = resolver.resolve("prefix{{empty}}suffix").unwrap();
        assert_eq!(result, "prefixsuffix");
    }

    #[test]
    fn test_resolve_unknown_var() {
        let resolver = fixed_resolver();
        let result = resolver.resolve("{{unknown}}/output.json");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, NikaError::TemplateError { .. }));
    }

    #[test]
    fn test_resolve_unclosed_template() {
        let resolver = fixed_resolver();
        // Unclosed template is treated as literal
        let result = resolver.resolve("{{task_id/output.json").unwrap();
        assert_eq!(result, "{{task_id/output.json");
    }

    #[test]
    fn test_resolve_no_templates() {
        let resolver = fixed_resolver();
        let result = resolver.resolve("simple/path/output.json").unwrap();
        assert_eq!(result, "simple/path/output.json");
    }

    #[test]
    fn test_resolve_whitespace_in_var() {
        let resolver = fixed_resolver();
        let result = resolver.resolve("{{ task_id }}/output.json").unwrap();
        assert_eq!(result, "test_task/output.json");
    }

    #[test]
    fn test_resolve_complex_path() {
        let resolver = fixed_resolver().with_var("locale", "es-MX").unwrap();
        let result = resolver
            .resolve("{{workflow_name}}/{{date}}/{{locale}}/{{task_id}}_{{time}}.json")
            .unwrap();
        assert_eq!(
            result,
            "test_workflow/2024-01-15/es-MX/test_task_14-30-45.json"
        );
    }

    #[test]
    fn test_resolve_workflow_alias() {
        // {{workflow}} is an alias for {{workflow_name}}
        let resolver = fixed_resolver();
        let result = resolver.resolve("{{workflow}}/{{task_id}}.json").unwrap();
        assert_eq!(result, "test_workflow/test_task.json");
    }
}
