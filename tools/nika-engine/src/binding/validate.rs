// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Task ID Validation
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
        let result = validate_task_id("weather");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let result = validate_task_id("w");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn valid_with_underscore() {
        let result = validate_task_id("get_data");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let result = validate_task_id("fetch_api");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let result = validate_task_id("my_task_name");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let result = validate_task_id("a_");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let result = validate_task_id("a__b");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn valid_with_numbers() {
        let result = validate_task_id("task123");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let result = validate_task_id("step2");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let result = validate_task_id("v2_parser");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let result = validate_task_id("a0");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let result = validate_task_id("a123456789");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn valid_single_letter() {
        let result = validate_task_id("a");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let result = validate_task_id("x");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let result = validate_task_id("z");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn valid_all_lowercase_boundaries() {
        let result = validate_task_id("abcdefghijklmnopqrstuvwxyz");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let result = validate_task_id("a0123456789");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    // ═══════════════════════════════════════════════════════════════
    // Invalid task IDs - NIKA-055 (detailed error messages)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn reject_empty() {
        let err = validate_task_id("").unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
        assert!(err.to_string().contains("cannot be empty"));
    }

    #[test]
    fn reject_number_start() {
        let err = validate_task_id("123task").unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
        assert!(err.to_string().contains("start with lowercase letter"));
    }

    #[test]
    fn reject_uppercase_start() {
        let err = validate_task_id("Task").unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
    }

    #[test]
    fn reject_all_uppercase() {
        let err = validate_task_id("TASK").unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
    }

    #[test]
    fn reject_uppercase_middle() {
        let err = validate_task_id("myTask").unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
    }

    #[test]
    fn reject_underscore_start() {
        let err = validate_task_id("_private").unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
        assert!(err.to_string().contains("start with lowercase letter"));
    }

    #[test]
    fn reject_dash() {
        let err = validate_task_id("fetch-api").unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
        // Should suggest fetch_api
    }

    #[test]
    fn reject_dot() {
        let err = validate_task_id("weather.api").unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
        // Dots are reserved for path traversal
    }

    #[test]
    fn reject_spaces() {
        let result = validate_task_id("my task");
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
        let result = validate_task_id(" weather");
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
        let result = validate_task_id("weather ");
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
        let result = validate_task_id("my  task");
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
    }

    #[test]
    fn reject_special_chars() {
        let result = validate_task_id("task!");
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
        let result = validate_task_id("task@name");
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
        let result = validate_task_id("task#1");
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
        let result = validate_task_id("task$");
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
        let result = validate_task_id("task%name");
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
        let result = validate_task_id("task&more");
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
        let result = validate_task_id("task(x)");
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
        let result = validate_task_id("task=value");
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
        let result = validate_task_id("task+more");
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
        let result = validate_task_id("task[0]");
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
        let result = validate_task_id("task{x}");
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
        let result = validate_task_id("task|pipe");
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
        let result = validate_task_id("task\\slash");
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
        let result = validate_task_id("task;semicolon");
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
        let result = validate_task_id("task:colon");
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
        let result = validate_task_id("task'quote");
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
        let result = validate_task_id("task\u{00a0}nbsp"); // non-breaking space
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
    }

    #[test]
    fn reject_emoji_and_unicode() {
        let result = validate_task_id("task😀");
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
        let result = validate_task_id("tâche"); // French accented character
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
        let result = validate_task_id("任務"); // Japanese
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
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
