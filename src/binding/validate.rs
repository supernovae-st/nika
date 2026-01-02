//! Task ID Validation (v0.1)
//!
//! Task IDs must follow snake_case convention:
//! - Start with lowercase letter
//! - Contain only lowercase letters, digits, underscores
//! - No dashes, dots, or uppercase letters
//!
//! Rationale: Dots are reserved for path separator in `task.field.subfield`
//!
//! Performance: Manual validation without regex for O(n) single-pass check with no allocations.
//! Regex would have higher startup cost and memory overhead for a simple pattern.

use crate::error::NikaError;

/// Validate a task ID without regex overhead
///
/// Manual implementation for optimal performance:
/// - O(n) single-pass validation
/// - Zero allocations
/// - No regex compilation overhead
///
/// Valid task IDs:
/// - Start with lowercase letter [a-z]
/// - Contain only lowercase letters, digits, underscores [a-z0-9_]*
///
/// Invalid patterns:
/// - Dashes: `fetch-api` (use `fetch_api`)
/// - Uppercase: `myTask` (use `my_task`)
/// - Dots: `weather.api` (dots reserved for paths)
/// - Numbers first: `123task` (must start with letter)
/// - Leading underscore: `_private` (not idiomatic)
pub fn validate_task_id(id: &str) -> Result<(), NikaError> {
  // Empty check
  if id.is_empty() {
    return Err(NikaError::InvalidTaskId {
      id: id.to_string(),
      reason: "cannot be empty".into(),
    });
  }

  // First character: must be [a-z]
  let first = id.as_bytes()[0];
  if !first.is_ascii_lowercase() {
    return Err(NikaError::InvalidTaskId {
      id: id.to_string(),
      reason: "must start with lowercase letter (a-z), then lowercase letters, digits, or underscores".into(),
    });
  }

  // Remaining characters: must be [a-z0-9_]
  for &byte in &id.as_bytes()[1..] {
    if !byte.is_ascii_lowercase() && !byte.is_ascii_digit() && byte != b'_' {
      return Err(NikaError::InvalidTaskId {
        id: id.to_string(),
        reason: "must start with lowercase letter (a-z), then lowercase letters, digits, or underscores".into(),
      });
    }
  }

  Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════
    // Valid task IDs - boundary and common cases
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn valid_simple() {
        assert!(validate_task_id("weather").is_ok());
        assert!(validate_task_id("w").is_ok());
    }

    #[test]
    fn valid_with_underscore() {
        assert!(validate_task_id("get_data").is_ok());
        assert!(validate_task_id("fetch_api").is_ok());
        assert!(validate_task_id("my_task_name").is_ok());
        assert!(validate_task_id("a_").is_ok());
        assert!(validate_task_id("a__b").is_ok());
    }

    #[test]
    fn valid_with_numbers() {
        assert!(validate_task_id("task123").is_ok());
        assert!(validate_task_id("step2").is_ok());
        assert!(validate_task_id("v2_parser").is_ok());
        assert!(validate_task_id("a0").is_ok());
        assert!(validate_task_id("a123456789").is_ok());
    }

    #[test]
    fn valid_single_letter() {
        assert!(validate_task_id("a").is_ok());
        assert!(validate_task_id("x").is_ok());
        assert!(validate_task_id("z").is_ok());
    }

    #[test]
    fn valid_all_lowercase_boundaries() {
        assert!(validate_task_id("abcdefghijklmnopqrstuvwxyz").is_ok());
        assert!(validate_task_id("a0123456789").is_ok());
    }

    // ═══════════════════════════════════════════════════════════════
    // Invalid task IDs - NIKA-055 (detailed error messages)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn reject_empty() {
        let result = validate_task_id("");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
        assert!(err.to_string().contains("cannot be empty"));
    }

    #[test]
    fn reject_number_start() {
        let result = validate_task_id("123task");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
        assert!(err.to_string().contains("start with lowercase letter"));
    }

    #[test]
    fn reject_uppercase_start() {
        let result = validate_task_id("Task");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
    }

    #[test]
    fn reject_all_uppercase() {
        let result = validate_task_id("TASK");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
    }

    #[test]
    fn reject_uppercase_middle() {
        let result = validate_task_id("myTask");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
    }

    #[test]
    fn reject_underscore_start() {
        let result = validate_task_id("_private");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
        assert!(err.to_string().contains("start with lowercase letter"));
    }

    #[test]
    fn reject_dash() {
        let result = validate_task_id("fetch-api");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
        // Should suggest fetch_api
    }

    #[test]
    fn reject_dot() {
        let result = validate_task_id("weather.api");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
        // Dots are reserved for path traversal
    }

    #[test]
    fn reject_spaces() {
        assert!(validate_task_id("my task").is_err());
        assert!(validate_task_id(" weather").is_err());
        assert!(validate_task_id("weather ").is_err());
        assert!(validate_task_id("my  task").is_err());
    }

    #[test]
    fn reject_special_chars() {
        assert!(validate_task_id("task!").is_err());
        assert!(validate_task_id("task@name").is_err());
        assert!(validate_task_id("task#1").is_err());
        assert!(validate_task_id("task$").is_err());
        assert!(validate_task_id("task%name").is_err());
        assert!(validate_task_id("task&more").is_err());
        assert!(validate_task_id("task(x)").is_err());
        assert!(validate_task_id("task=value").is_err());
        assert!(validate_task_id("task+more").is_err());
        assert!(validate_task_id("task[0]").is_err());
        assert!(validate_task_id("task{x}").is_err());
        assert!(validate_task_id("task|pipe").is_err());
        assert!(validate_task_id("task\\slash").is_err());
        assert!(validate_task_id("task;semicolon").is_err());
        assert!(validate_task_id("task:colon").is_err());
        assert!(validate_task_id("task'quote").is_err());
        assert!(validate_task_id("task\u{00a0}nbsp").is_err()); // non-breaking space
    }

    #[test]
    fn reject_emoji_and_unicode() {
        assert!(validate_task_id("task😀").is_err());
        assert!(validate_task_id("tâche").is_err()); // French accented character
        assert!(validate_task_id("任務").is_err()); // Japanese
    }

    #[test]
    fn error_message_contains_nika_code() {
        let result = validate_task_id("invalid-name");
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
    }

    #[test]
    fn error_message_includes_invalid_id() {
        let invalid_id = "my-invalid-task";
        let result = validate_task_id(invalid_id);
        let err = result.unwrap_err();
        assert!(err.to_string().contains(invalid_id));
    }
}
