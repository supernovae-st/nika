//! Agent Guardrails Configuration (v0.23)
//!
//! Guardrails validate agent outputs before accepting them as complete.
//! They provide additional checks beyond confidence-based routing.
//!
//! ## Guardrail Types
//!
//! - **length**: Validates word/character count bounds
//! - **schema**: Validates JSON output against a JSON Schema
//! - **regex**: Validates output matches a pattern
//! - **llm** (v0.25): Uses secondary LLM call for validation
//!
//! ## Example
//!
//! ```yaml
//! agent:
//!   prompt: "Summarize the article"
//!   guardrails:
//!     - type: length
//!       min_words: 50
//!       max_words: 200
//!     - type: schema
//!       json_schema:
//!         type: object
//!         properties:
//!           summary: { type: string }
//!           key_points: { type: array }
//!         required: [summary]
//!     - type: regex
//!       pattern: "^Summary:"
//!       message: "Output must start with 'Summary:'"
//! ```

use serde::Deserialize;
use serde_json::Value as JsonValue;

// ═══════════════════════════════════════════════════════════════════════════
// GuardrailConfig
// ═══════════════════════════════════════════════════════════════════════════

/// Configuration for a single guardrail.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum GuardrailConfig {
    /// Length-based validation (word/character counts)
    Length(LengthGuardrail),

    /// JSON Schema validation
    Schema(SchemaGuardrail),

    /// Regex pattern matching
    Regex(RegexGuardrail),
}

impl GuardrailConfig {
    /// Get the guardrail type as a string.
    pub fn guardrail_type(&self) -> &'static str {
        match self {
            GuardrailConfig::Length(_) => "length",
            GuardrailConfig::Schema(_) => "schema",
            GuardrailConfig::Regex(_) => "regex",
        }
    }

    /// Get the guardrail ID (for logging/events).
    pub fn id(&self) -> String {
        match self {
            GuardrailConfig::Length(g) => g.id.clone().unwrap_or_else(|| "length".to_string()),
            GuardrailConfig::Schema(g) => g.id.clone().unwrap_or_else(|| "schema".to_string()),
            GuardrailConfig::Regex(g) => g.id.clone().unwrap_or_else(|| "regex".to_string()),
        }
    }

    /// Validate the guardrail configuration.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            GuardrailConfig::Length(g) => g.validate(),
            GuardrailConfig::Schema(g) => g.validate(),
            GuardrailConfig::Regex(g) => g.validate(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// LengthGuardrail
// ═══════════════════════════════════════════════════════════════════════════

/// Validates output length in words or characters.
#[derive(Debug, Clone, Deserialize)]
pub struct LengthGuardrail {
    /// Optional guardrail ID for logging
    #[serde(default)]
    pub id: Option<String>,

    /// Minimum word count
    #[serde(default)]
    pub min_words: Option<u32>,

    /// Maximum word count
    #[serde(default)]
    pub max_words: Option<u32>,

    /// Minimum character count
    #[serde(default)]
    pub min_chars: Option<u32>,

    /// Maximum character count
    #[serde(default)]
    pub max_chars: Option<u32>,

    /// Custom error message on failure
    #[serde(default)]
    pub message: Option<String>,
}

impl LengthGuardrail {
    /// Validate the guardrail configuration.
    pub fn validate(&self) -> Result<(), String> {
        // At least one constraint should be set
        if self.min_words.is_none()
            && self.max_words.is_none()
            && self.min_chars.is_none()
            && self.max_chars.is_none()
        {
            return Err(
                "length guardrail requires at least one of: min_words, max_words, min_chars, max_chars".to_string()
            );
        }

        // min should be <= max
        if let (Some(min), Some(max)) = (self.min_words, self.max_words) {
            if min > max {
                return Err(format!(
                    "length guardrail: min_words ({}) > max_words ({})",
                    min, max
                ));
            }
        }

        if let (Some(min), Some(max)) = (self.min_chars, self.max_chars) {
            if min > max {
                return Err(format!(
                    "length guardrail: min_chars ({}) > max_chars ({})",
                    min, max
                ));
            }
        }

        Ok(())
    }

    /// Check if the output passes this guardrail.
    pub fn check(&self, output: &str) -> GuardrailResult {
        let word_count = output.split_whitespace().count() as u32;
        let char_count = output.chars().count() as u32;

        // Check min_words
        if let Some(min) = self.min_words {
            if word_count < min {
                return GuardrailResult::failed(
                    self.id.clone().unwrap_or_else(|| "length".to_string()),
                    "length",
                    self.message.clone().unwrap_or_else(|| {
                        format!("Output has {} words, minimum is {}", word_count, min)
                    }),
                );
            }
        }

        // Check max_words
        if let Some(max) = self.max_words {
            if word_count > max {
                return GuardrailResult::failed(
                    self.id.clone().unwrap_or_else(|| "length".to_string()),
                    "length",
                    self.message.clone().unwrap_or_else(|| {
                        format!("Output has {} words, maximum is {}", word_count, max)
                    }),
                );
            }
        }

        // Check min_chars
        if let Some(min) = self.min_chars {
            if char_count < min {
                return GuardrailResult::failed(
                    self.id.clone().unwrap_or_else(|| "length".to_string()),
                    "length",
                    self.message.clone().unwrap_or_else(|| {
                        format!("Output has {} chars, minimum is {}", char_count, min)
                    }),
                );
            }
        }

        // Check max_chars
        if let Some(max) = self.max_chars {
            if char_count > max {
                return GuardrailResult::failed(
                    self.id.clone().unwrap_or_else(|| "length".to_string()),
                    "length",
                    self.message.clone().unwrap_or_else(|| {
                        format!("Output has {} chars, maximum is {}", char_count, max)
                    }),
                );
            }
        }

        GuardrailResult::passed(
            self.id.clone().unwrap_or_else(|| "length".to_string()),
            "length",
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SchemaGuardrail
// ═══════════════════════════════════════════════════════════════════════════

/// Validates JSON output against a JSON Schema.
#[derive(Debug, Clone, Deserialize)]
pub struct SchemaGuardrail {
    /// Optional guardrail ID for logging
    #[serde(default)]
    pub id: Option<String>,

    /// JSON Schema to validate against
    pub json_schema: JsonValue,

    /// Custom error message on failure
    #[serde(default)]
    pub message: Option<String>,
}

impl SchemaGuardrail {
    /// Validate the guardrail configuration.
    pub fn validate(&self) -> Result<(), String> {
        // Verify json_schema is an object
        if !self.json_schema.is_object() {
            return Err("schema guardrail: json_schema must be an object".to_string());
        }
        Ok(())
    }

    /// Check if the output passes this guardrail.
    ///
    /// Note: Full JSON Schema validation requires jsonschema crate.
    /// For now, we just verify the output is valid JSON with required fields.
    pub fn check(&self, output: &str) -> GuardrailResult {
        // Try to parse as JSON
        let parsed: Result<JsonValue, _> = serde_json::from_str(output);
        let json = match parsed {
            Ok(v) => v,
            Err(e) => {
                return GuardrailResult::failed(
                    self.id.clone().unwrap_or_else(|| "schema".to_string()),
                    "schema",
                    self.message
                        .clone()
                        .unwrap_or_else(|| format!("Invalid JSON: {}", e)),
                );
            }
        };

        // Check required fields if specified in schema
        if let Some(required) = self.json_schema.get("required").and_then(|r| r.as_array()) {
            if let Some(obj) = json.as_object() {
                for field in required {
                    if let Some(field_name) = field.as_str() {
                        if !obj.contains_key(field_name) {
                            return GuardrailResult::failed(
                                self.id.clone().unwrap_or_else(|| "schema".to_string()),
                                "schema",
                                self.message.clone().unwrap_or_else(|| {
                                    format!("Missing required field: {}", field_name)
                                }),
                            );
                        }
                    }
                }
            } else {
                return GuardrailResult::failed(
                    self.id.clone().unwrap_or_else(|| "schema".to_string()),
                    "schema",
                    self.message
                        .clone()
                        .unwrap_or_else(|| "Expected JSON object".to_string()),
                );
            }
        }

        GuardrailResult::passed(
            self.id.clone().unwrap_or_else(|| "schema".to_string()),
            "schema",
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// RegexGuardrail
// ═══════════════════════════════════════════════════════════════════════════

/// Validates output matches a regex pattern.
#[derive(Debug, Clone, Deserialize)]
pub struct RegexGuardrail {
    /// Optional guardrail ID for logging
    #[serde(default)]
    pub id: Option<String>,

    /// Regex pattern to match
    pub pattern: String,

    /// If true, the pattern must NOT match (negative lookahead)
    #[serde(default)]
    pub negate: bool,

    /// Custom error message on failure
    #[serde(default)]
    pub message: Option<String>,
}

impl RegexGuardrail {
    /// Validate the guardrail configuration.
    pub fn validate(&self) -> Result<(), String> {
        // Verify pattern is a valid regex
        regex::Regex::new(&self.pattern)
            .map_err(|e| format!("regex guardrail: invalid pattern '{}': {}", self.pattern, e))?;
        Ok(())
    }

    /// Check if the output passes this guardrail.
    pub fn check(&self, output: &str) -> GuardrailResult {
        let re = match regex::Regex::new(&self.pattern) {
            Ok(r) => r,
            Err(e) => {
                return GuardrailResult::failed(
                    self.id.clone().unwrap_or_else(|| "regex".to_string()),
                    "regex",
                    format!("Invalid regex pattern: {}", e),
                );
            }
        };

        let matches = re.is_match(output);
        let passed = if self.negate { !matches } else { matches };

        if passed {
            GuardrailResult::passed(
                self.id.clone().unwrap_or_else(|| "regex".to_string()),
                "regex",
            )
        } else {
            let default_msg = if self.negate {
                format!("Output must NOT match pattern: {}", self.pattern)
            } else {
                format!("Output must match pattern: {}", self.pattern)
            };
            GuardrailResult::failed(
                self.id.clone().unwrap_or_else(|| "regex".to_string()),
                "regex",
                self.message.clone().unwrap_or(default_msg),
            )
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// GuardrailResult
// ═══════════════════════════════════════════════════════════════════════════

/// Result of a guardrail check.
#[derive(Debug, Clone)]
pub struct GuardrailResult {
    /// Whether the guardrail passed
    pub passed: bool,

    /// Guardrail ID
    pub guardrail_id: String,

    /// Guardrail type (length, schema, regex)
    pub guardrail_type: String,

    /// Error message if failed
    pub message: Option<String>,
}

impl GuardrailResult {
    /// Create a passed result.
    pub fn passed(guardrail_id: String, guardrail_type: &str) -> Self {
        Self {
            passed: true,
            guardrail_id,
            guardrail_type: guardrail_type.to_string(),
            message: None,
        }
    }

    /// Create a failed result.
    pub fn failed(guardrail_id: String, guardrail_type: &str, message: String) -> Self {
        Self {
            passed: false,
            guardrail_id,
            guardrail_type: guardrail_type.to_string(),
            message: Some(message),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// GuardrailRunner - Run all guardrails
// ═══════════════════════════════════════════════════════════════════════════

/// Run all guardrails against an output.
///
/// Returns a vector of results, one per guardrail.
pub fn run_guardrails(guardrails: &[GuardrailConfig], output: &str) -> Vec<GuardrailResult> {
    guardrails
        .iter()
        .map(|g| match g {
            GuardrailConfig::Length(lg) => lg.check(output),
            GuardrailConfig::Schema(sg) => sg.check(output),
            GuardrailConfig::Regex(rg) => rg.check(output),
        })
        .collect()
}

/// Check if all guardrails passed.
pub fn all_guardrails_passed(results: &[GuardrailResult]) -> bool {
    results.iter().all(|r| r.passed)
}

/// Get the first failed guardrail result.
pub fn first_failed_guardrail(results: &[GuardrailResult]) -> Option<&GuardrailResult> {
    results.iter().find(|r| !r.passed)
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde_yaml;

    // ========================================================================
    // LengthGuardrail Tests
    // ========================================================================

    #[test]
    fn test_length_guardrail_min_words_pass() {
        let guardrail = LengthGuardrail {
            id: Some("len1".to_string()),
            min_words: Some(5),
            max_words: None,
            min_chars: None,
            max_chars: None,
            message: None,
        };

        let result = guardrail.check("This has exactly five words here.");
        assert!(result.passed);
    }

    #[test]
    fn test_length_guardrail_min_words_fail() {
        let guardrail = LengthGuardrail {
            id: Some("len1".to_string()),
            min_words: Some(10),
            max_words: None,
            min_chars: None,
            max_chars: None,
            message: None,
        };

        let result = guardrail.check("Too short");
        assert!(!result.passed);
        assert!(result.message.unwrap().contains("2 words"));
    }

    #[test]
    fn test_length_guardrail_max_words_pass() {
        let guardrail = LengthGuardrail {
            id: None,
            min_words: None,
            max_words: Some(10),
            min_chars: None,
            max_chars: None,
            message: None,
        };

        let result = guardrail.check("This is short");
        assert!(result.passed);
    }

    #[test]
    fn test_length_guardrail_max_words_fail() {
        let guardrail = LengthGuardrail {
            id: None,
            min_words: None,
            max_words: Some(3),
            min_chars: None,
            max_chars: None,
            message: None,
        };

        let result = guardrail.check("This has more than three words");
        assert!(!result.passed);
    }

    #[test]
    fn test_length_guardrail_chars() {
        let guardrail = LengthGuardrail {
            id: None,
            min_words: None,
            max_words: None,
            min_chars: Some(10),
            max_chars: Some(20),
            message: None,
        };

        // 15 chars - should pass
        let result = guardrail.check("Hello, World!!!");
        assert!(result.passed);

        // Too short
        let result = guardrail.check("Hi");
        assert!(!result.passed);

        // Too long
        let result = guardrail.check("This is way too long for the limit");
        assert!(!result.passed);
    }

    #[test]
    fn test_length_guardrail_custom_message() {
        let guardrail = LengthGuardrail {
            id: None,
            min_words: Some(100),
            max_words: None,
            min_chars: None,
            max_chars: None,
            message: Some("Response too brief".to_string()),
        };

        let result = guardrail.check("Short");
        assert!(!result.passed);
        assert_eq!(result.message.unwrap(), "Response too brief");
    }

    #[test]
    fn test_length_guardrail_validation() {
        // Valid config
        let guardrail = LengthGuardrail {
            id: None,
            min_words: Some(10),
            max_words: Some(100),
            min_chars: None,
            max_chars: None,
            message: None,
        };
        assert!(guardrail.validate().is_ok());

        // Invalid: no constraints
        let guardrail = LengthGuardrail {
            id: None,
            min_words: None,
            max_words: None,
            min_chars: None,
            max_chars: None,
            message: None,
        };
        assert!(guardrail.validate().is_err());

        // Invalid: min > max
        let guardrail = LengthGuardrail {
            id: None,
            min_words: Some(100),
            max_words: Some(10),
            min_chars: None,
            max_chars: None,
            message: None,
        };
        assert!(guardrail.validate().is_err());
    }

    // ========================================================================
    // SchemaGuardrail Tests
    // ========================================================================

    #[test]
    fn test_schema_guardrail_valid_json() {
        let guardrail = SchemaGuardrail {
            id: Some("schema1".to_string()),
            json_schema: serde_json::json!({
                "type": "object",
                "required": ["name"]
            }),
            message: None,
        };

        let result = guardrail.check(r#"{"name": "test", "value": 42}"#);
        assert!(result.passed);
    }

    #[test]
    fn test_schema_guardrail_missing_required() {
        let guardrail = SchemaGuardrail {
            id: None,
            json_schema: serde_json::json!({
                "type": "object",
                "required": ["name", "value"]
            }),
            message: None,
        };

        let result = guardrail.check(r#"{"name": "test"}"#);
        assert!(!result.passed);
        assert!(result.message.unwrap().contains("value"));
    }

    #[test]
    fn test_schema_guardrail_invalid_json() {
        let guardrail = SchemaGuardrail {
            id: None,
            json_schema: serde_json::json!({"type": "object"}),
            message: None,
        };

        let result = guardrail.check("not valid json {");
        assert!(!result.passed);
        assert!(result.message.unwrap().contains("Invalid JSON"));
    }

    #[test]
    fn test_schema_guardrail_not_object() {
        let guardrail = SchemaGuardrail {
            id: None,
            json_schema: serde_json::json!({
                "type": "object",
                "required": ["x"]
            }),
            message: None,
        };

        let result = guardrail.check(r#"[1, 2, 3]"#);
        assert!(!result.passed);
        assert!(result.message.unwrap().contains("Expected JSON object"));
    }

    // ========================================================================
    // RegexGuardrail Tests
    // ========================================================================

    #[test]
    fn test_regex_guardrail_match() {
        let guardrail = RegexGuardrail {
            id: Some("regex1".to_string()),
            pattern: r"^Summary:".to_string(),
            negate: false,
            message: None,
        };

        let result = guardrail.check("Summary: This is the summary.");
        assert!(result.passed);
    }

    #[test]
    fn test_regex_guardrail_no_match() {
        let guardrail = RegexGuardrail {
            id: None,
            pattern: r"^Summary:".to_string(),
            negate: false,
            message: Some("Must start with Summary:".to_string()),
        };

        let result = guardrail.check("This doesn't start with Summary");
        assert!(!result.passed);
        assert_eq!(result.message.unwrap(), "Must start with Summary:");
    }

    #[test]
    fn test_regex_guardrail_negate() {
        let guardrail = RegexGuardrail {
            id: None,
            pattern: r"TODO|FIXME".to_string(),
            negate: true, // Must NOT contain TODO or FIXME
            message: None,
        };

        // Pass - no TODO/FIXME
        let result = guardrail.check("This is clean code");
        assert!(result.passed);

        // Fail - contains TODO
        let result = guardrail.check("This has a TODO in it");
        assert!(!result.passed);
    }

    #[test]
    fn test_regex_guardrail_validation() {
        // Valid regex
        let guardrail = RegexGuardrail {
            id: None,
            pattern: r"^\w+$".to_string(),
            negate: false,
            message: None,
        };
        assert!(guardrail.validate().is_ok());

        // Invalid regex
        let guardrail = RegexGuardrail {
            id: None,
            pattern: r"[invalid(".to_string(),
            negate: false,
            message: None,
        };
        assert!(guardrail.validate().is_err());
    }

    // ========================================================================
    // GuardrailConfig Parsing Tests
    // ========================================================================

    #[test]
    fn test_parse_length_guardrail() {
        let yaml = r#"
type: length
id: len1
min_words: 50
max_words: 200
"#;
        let config: GuardrailConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.guardrail_type(), "length");
        assert_eq!(config.id(), "len1");

        if let GuardrailConfig::Length(g) = config {
            assert_eq!(g.min_words, Some(50));
            assert_eq!(g.max_words, Some(200));
        } else {
            panic!("Expected Length guardrail");
        }
    }

    #[test]
    fn test_parse_schema_guardrail() {
        let yaml = r#"
type: schema
json_schema:
  type: object
  required:
    - name
    - value
"#;
        let config: GuardrailConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.guardrail_type(), "schema");

        if let GuardrailConfig::Schema(g) = config {
            assert!(g.json_schema.is_object());
        } else {
            panic!("Expected Schema guardrail");
        }
    }

    #[test]
    fn test_parse_regex_guardrail() {
        let yaml = r#"
type: regex
pattern: "^Result:"
negate: false
message: "Must start with Result:"
"#;
        let config: GuardrailConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.guardrail_type(), "regex");

        if let GuardrailConfig::Regex(g) = config {
            assert_eq!(g.pattern, "^Result:");
            assert!(!g.negate);
        } else {
            panic!("Expected Regex guardrail");
        }
    }

    // ========================================================================
    // Runner Tests
    // ========================================================================

    #[test]
    fn test_run_guardrails_all_pass() {
        let guardrails = vec![
            GuardrailConfig::Length(LengthGuardrail {
                id: Some("len".to_string()),
                min_words: Some(3),
                max_words: None,
                min_chars: None,
                max_chars: None,
                message: None,
            }),
            GuardrailConfig::Regex(RegexGuardrail {
                id: Some("re".to_string()),
                pattern: r"\w+".to_string(),
                negate: false,
                message: None,
            }),
        ];

        let output = "This is a valid response";
        let results = run_guardrails(&guardrails, output);

        assert_eq!(results.len(), 2);
        assert!(all_guardrails_passed(&results));
        assert!(first_failed_guardrail(&results).is_none());
    }

    #[test]
    fn test_run_guardrails_one_fails() {
        let guardrails = vec![
            GuardrailConfig::Length(LengthGuardrail {
                id: Some("len".to_string()),
                min_words: Some(100), // Will fail
                max_words: None,
                min_chars: None,
                max_chars: None,
                message: None,
            }),
            GuardrailConfig::Regex(RegexGuardrail {
                id: Some("re".to_string()),
                pattern: r"\w+".to_string(), // Will pass
                negate: false,
                message: None,
            }),
        ];

        let output = "Short response";
        let results = run_guardrails(&guardrails, output);

        assert_eq!(results.len(), 2);
        assert!(!all_guardrails_passed(&results));

        let failed = first_failed_guardrail(&results).unwrap();
        assert_eq!(failed.guardrail_id, "len");
        assert_eq!(failed.guardrail_type, "length");
    }
}
