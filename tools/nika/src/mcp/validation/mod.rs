//! MCP Validation Module
//!
//! Provides 3-layer validation for MCP tool parameters:
//!
//! 1. **Schema Discovery** (schema_cache.rs) - Cache schemas from list_tools()
//! 2. **Pre-call Validation** (validator.rs) - Validate before calling
//! 3. **Error Enhancement** (enhancer.rs) - Better error messages
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika::mcp::validation::{ValidationConfig, McpValidator};
//!
//! let validator = McpValidator::new(ValidationConfig::default());
//! validator.cache().populate("novanet", &tools)?;
//!
//! let result = validator.validate("novanet", "novanet_generate", &params);
//! if !result.is_valid {
//!     for error in result.errors {
//!         eprintln!("{}", error.message);
//!     }
//! }
//! ```
//!
//! ## Error Codes
//!
//! - NIKA-107: McpValidationFailed - Pre-call validation found errors
//! - NIKA-108: McpSchemaError - Failed to compile/parse tool schema

pub mod enhancer;
pub mod schema_cache;
pub mod validator;

// Re-exports
pub use enhancer::ErrorEnhancer;
pub use schema_cache::{CacheStats, CachedSchema, ToolSchemaCache};
pub use validator::{McpValidator, ValidationError, ValidationErrorKind, ValidationResult};

/// Configuration for MCP parameter validation
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    /// Enable pre-call validation (default: true)
    pub pre_validate: bool,

    /// Enable error enhancement (default: true)
    pub enhance_errors: bool,

    /// Max edit distance for "did you mean" suggestions (default: 3)
    pub suggestion_distance: usize,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            pre_validate: true,
            enhance_errors: true,
            suggestion_distance: 3,
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_config_default() {
        let config = ValidationConfig::default();
        assert!(config.pre_validate);
        assert!(config.enhance_errors);
        assert_eq!(config.suggestion_distance, 3);
    }

    #[test]
    fn test_validation_config_custom() {
        let config = ValidationConfig {
            pre_validate: false,
            enhance_errors: true,
            suggestion_distance: 5,
        };
        assert!(!config.pre_validate);
        assert!(config.enhance_errors);
        assert_eq!(config.suggestion_distance, 5);
    }
}
