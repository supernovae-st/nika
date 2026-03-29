//! RecordSpec — AST node for the `record:` task field.
//!
//! Supports shorthand `record: true` and full form:
//! ```yaml
//! record:
//!   compress: true
//!   retain: [stats, findings]
//!   max_tokens: 300
//!   confidence_threshold: 0.7
//! ```

use serde::{Deserialize, Serialize};

/// Default max tokens for record compression.
fn default_max_tokens() -> u32 {
    500
}

/// Specification for task output compression into a Record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordSpec {
    /// Whether to compress this task's output.
    #[serde(default)]
    pub compress: bool,

    /// Fields to retain in the compressed Record (e.g., ["stats", "findings"]).
    #[serde(default)]
    pub retain: Vec<String>,

    /// Maximum tokens for the compressed summary.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,

    /// Minimum confidence threshold — below this, the Record is skipped.
    #[serde(default)]
    pub confidence_threshold: f64,
}

impl Default for RecordSpec {
    fn default() -> Self {
        Self {
            compress: false,
            retain: vec![],
            max_tokens: default_max_tokens(),
            confidence_threshold: 0.0,
        }
    }
}

impl RecordSpec {
    /// Shorthand: `record: true` → `RecordSpec { compress: true, ..default }`
    pub fn shorthand_true() -> Self {
        Self {
            compress: true,
            ..Default::default()
        }
    }

    /// Validate the spec. Returns error message if invalid.
    pub fn validate(&self) -> Result<(), String> {
        if self.max_tokens == 0 {
            return Err("record max_tokens must be > 0".to_string());
        }
        if self.max_tokens > 4096 {
            return Err(format!(
                "record max_tokens {} exceeds maximum 4096",
                self.max_tokens
            ));
        }
        if !(0.0..=1.0).contains(&self.confidence_threshold) {
            return Err(format!(
                "record confidence_threshold must be 0.0-1.0, got {}",
                self.confidence_threshold
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shorthand_true() {
        let spec = RecordSpec::shorthand_true();
        assert!(spec.compress);
        assert_eq!(spec.max_tokens, 500);
        assert!(spec.retain.is_empty());
    }

    #[test]
    fn test_default_no_compress() {
        let spec = RecordSpec::default();
        assert!(!spec.compress);
    }

    #[test]
    fn test_validate_ok() {
        let spec = RecordSpec::shorthand_true();
        assert!(spec.validate().is_ok());
    }

    #[test]
    fn test_validate_zero_max_tokens() {
        let mut spec = RecordSpec::shorthand_true();
        spec.max_tokens = 0;
        assert!(spec
            .validate()
            .unwrap_err()
            .contains("max_tokens must be > 0"));
    }

    #[test]
    fn test_validate_too_many_tokens() {
        let mut spec = RecordSpec::shorthand_true();
        spec.max_tokens = 5000;
        assert!(spec.validate().unwrap_err().contains("exceeds maximum"));
    }

    #[test]
    fn test_validate_bad_confidence() {
        let mut spec = RecordSpec::shorthand_true();
        spec.confidence_threshold = 1.5;
        assert!(spec
            .validate()
            .unwrap_err()
            .contains("confidence_threshold"));
    }

    #[test]
    fn test_deserialize_full_form() {
        let json = r#"{"compress": true, "retain": ["stats"], "max_tokens": 300, "confidence_threshold": 0.7}"#;
        let spec: RecordSpec = serde_json::from_str(json).unwrap();
        assert!(spec.compress);
        assert_eq!(spec.retain, vec!["stats"]);
        assert_eq!(spec.max_tokens, 300);
        assert!((spec.confidence_threshold - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn test_deserialize_minimal() {
        let json = r#"{"compress": true}"#;
        let spec: RecordSpec = serde_json::from_str(json).unwrap();
        assert!(spec.compress);
        assert_eq!(spec.max_tokens, 500); // default
    }

    #[test]
    fn test_serialization_roundtrip() {
        let spec = RecordSpec {
            compress: true,
            retain: vec!["a".into(), "b".into()],
            max_tokens: 200,
            confidence_threshold: 0.5,
        };
        let json = serde_json::to_string(&spec).unwrap();
        let spec2: RecordSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(spec, spec2);
    }
}
