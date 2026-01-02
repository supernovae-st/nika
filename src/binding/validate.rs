//! Task ID Validation (v0.1)
//!
//! Task IDs must follow snake_case convention:
//! - Start with lowercase letter
//! - Contain only lowercase letters, digits, underscores
//! - No dashes, dots, or uppercase letters
//!
//! Rationale: Dots are reserved for path separator in `task.field.subfield`

use std::sync::LazyLock;

use regex::Regex;

use crate::error::NikaError;

/// Regex for valid task IDs: ^[a-z][a-z0-9_]*$
static TASK_ID_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9_]*$").expect("Invalid task ID regex"));

/// Validate a task ID
///
/// Valid task IDs:
/// - Start with lowercase letter
/// - Contain only lowercase letters, digits, underscores
///
/// Invalid:
/// - Dashes: `fetch-api` (use `fetch_api`)
/// - Uppercase: `myTask` (use `my_task`)
/// - Dots: `weather.api` (dots reserved for paths)
/// - Numbers first: `123task` (must start with letter)
pub fn validate_task_id(id: &str) -> Result<(), NikaError> {
    if !TASK_ID_REGEX.is_match(id) {
        return Err(NikaError::InvalidTaskId {
            id: id.to_string(),
            reason: "must be snake_case: start with lowercase letter, then lowercase letters, digits, or underscores only".into(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════
    // Valid task IDs
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn valid_simple() {
        assert!(validate_task_id("weather").is_ok());
    }

    #[test]
    fn valid_with_underscore() {
        assert!(validate_task_id("get_data").is_ok());
        assert!(validate_task_id("fetch_api").is_ok());
        assert!(validate_task_id("my_task_name").is_ok());
    }

    #[test]
    fn valid_with_numbers() {
        assert!(validate_task_id("task123").is_ok());
        assert!(validate_task_id("step2").is_ok());
        assert!(validate_task_id("v2_parser").is_ok());
    }

    #[test]
    fn valid_single_letter() {
        assert!(validate_task_id("a").is_ok());
        assert!(validate_task_id("x").is_ok());
    }

    // ═══════════════════════════════════════════════════════════════
    // Invalid task IDs - NIKA-055
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn reject_dash() {
        let result = validate_task_id("fetch-api");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
    }

    #[test]
    fn reject_uppercase() {
        let result = validate_task_id("myTask");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
    }

    #[test]
    fn reject_uppercase_start() {
        let result = validate_task_id("Task");
        assert!(result.is_err());
    }

    #[test]
    fn reject_all_uppercase() {
        let result = validate_task_id("TASK");
        assert!(result.is_err());
    }

    #[test]
    fn reject_dot() {
        let result = validate_task_id("weather.api");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
    }

    #[test]
    fn reject_number_start() {
        let result = validate_task_id("123task");
        assert!(result.is_err());
    }

    #[test]
    fn reject_underscore_start() {
        let result = validate_task_id("_private");
        assert!(result.is_err());
    }

    #[test]
    fn reject_empty() {
        let result = validate_task_id("");
        assert!(result.is_err());
    }

    #[test]
    fn reject_spaces() {
        let result = validate_task_id("my task");
        assert!(result.is_err());
    }

    #[test]
    fn reject_special_chars() {
        assert!(validate_task_id("task!").is_err());
        assert!(validate_task_id("task@name").is_err());
        assert!(validate_task_id("task#1").is_err());
        assert!(validate_task_id("task$").is_err());
    }
}
