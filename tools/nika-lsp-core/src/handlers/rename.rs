// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Rename handler — task ID refactoring.
//!
//! Leverages the existing `references.rs` to find all references to a task ID,
//! then generates text edits to rename them all.

use super::references::{find_task_at_offset, find_task_references};

/// A range of text (byte offsets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameRange {
    pub start: u32,
    pub end: u32,
}

/// A text replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub start: u32,
    pub end: u32,
    pub new_text: String,
}

/// Check if cursor is on a renameable identifier.
///
/// Returns the range of the task ID at the cursor position, or `None` if
/// the cursor is not on a task identifier.
pub fn prepare_rename(text: &str, offset: u32) -> Option<RenameRange> {
    let task_id = find_task_at_offset(text, offset)?;

    // Find the exact position of this task_id at the cursor offset.
    // Search backward from offset to find the start of the identifier.
    let text_bytes = text.as_bytes();
    let offset = offset as usize;

    // Scan backward to find the start of the identifier
    let mut start = offset;
    while start > 0
        && (text_bytes[start - 1].is_ascii_alphanumeric()
            || text_bytes[start - 1] == b'_'
            || text_bytes[start - 1] == b'-')
    {
        start -= 1;
    }

    // Scan forward to find the end of the identifier
    let mut end = offset;
    while end < text_bytes.len()
        && (text_bytes[end].is_ascii_alphanumeric()
            || text_bytes[end] == b'_'
            || text_bytes[end] == b'-')
    {
        end += 1;
    }

    // Verify the range matches the task_id
    let range_text = &text[start..end];
    if range_text == task_id {
        Some(RenameRange {
            start: start as u32,
            end: end as u32,
        })
    } else {
        // Cursor might be on a $ref or in depends_on — try to find the task_id nearby
        // by searching the current line
        let line_start = text[..offset].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let line_end = text[offset..]
            .find('\n')
            .map(|p| offset + p)
            .unwrap_or(text.len());
        let line = &text[line_start..line_end];

        if let Some(pos) = line.find(&task_id) {
            let abs_start = (line_start + pos) as u32;
            let abs_end = abs_start + task_id.len() as u32;
            Some(RenameRange {
                start: abs_start,
                end: abs_end,
            })
        } else {
            None
        }
    }
}

/// Validate a new task ID name.
fn is_valid_task_id(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && name
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
}

/// Rename a task ID at the cursor position.
///
/// Returns text edits for all references, sorted by descending offset
/// (so they can be applied in order without offset shifting).
/// Returns empty vec if cursor is not on a task ID or new name is invalid.
pub fn rename(text: &str, offset: u32, new_name: &str) -> Vec<TextEdit> {
    if !is_valid_task_id(new_name) {
        return vec![];
    }

    let task_id = match find_task_at_offset(text, offset) {
        Some(id) => id,
        None => return vec![],
    };

    let refs = find_task_references(text, &task_id);

    let mut edits: Vec<TextEdit> = refs
        .into_iter()
        .map(|r| TextEdit {
            start: r.start_offset,
            end: r.end_offset,
            new_text: new_name.to_string(),
        })
        .collect();

    // Sort by descending offset for safe sequential application
    edits.sort_by(|a, b| b.start.cmp(&a.start));
    edits
}

/// Apply text edits to produce the result (for testing).
#[cfg(test)]
fn apply_edits(text: &str, edits: &[TextEdit]) -> String {
    let mut result = text.to_string();
    // Edits must be sorted descending by start offset
    for edit in edits {
        result.replace_range(edit.start as usize..edit.end as usize, &edit.new_text);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_rename_on_task_id() {
        let text = "tasks:\n  - id: fetch_data\n    infer: \"hello\"";
        let offset = text.find("fetch_data").unwrap() as u32 + 2;
        let result = prepare_rename(text, offset);
        assert!(result.is_some());
        let range = result.unwrap();
        assert_eq!(
            &text[range.start as usize..range.end as usize],
            "fetch_data"
        );
    }

    #[test]
    fn prepare_rename_on_non_identifier_returns_none() {
        let text = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: test";
        let offset = text.find("schema").unwrap() as u32;
        assert!(prepare_rename(text, offset).is_none());
    }

    #[test]
    fn rename_updates_all_references() {
        let text = "tasks:\n  - id: step1\n    infer: \"hello\"\n  - id: step2\n    depends_on: [step1]\n    with:\n      data: $step1\n    infer: \"{{with.data}}\"\n";
        let offset = text.find("step1").unwrap() as u32;
        let edits = rename(text, offset, "fetch_data");
        // Should find: id definition, depends_on ref, with $ref
        assert!(
            edits.len() >= 3,
            "Expected >= 3 edits, got {}: {:?}",
            edits.len(),
            edits
        );
        // Apply edits and verify
        let result = apply_edits(text, &edits);
        assert!(
            !result.contains("step1"),
            "Old name should be gone: {}",
            result
        );
        assert!(
            result.contains("fetch_data"),
            "New name should be present: {}",
            result
        );
    }

    #[test]
    fn rename_invalid_name_returns_empty() {
        let text = "tasks:\n  - id: step1\n    infer: \"test\"";
        let offset = text.find("step1").unwrap() as u32;
        // Names with spaces are invalid
        let edits = rename(text, offset, "invalid name");
        assert!(edits.is_empty());
    }

    #[test]
    fn rename_preserves_yaml_structure() {
        let text = "tasks:\n  - id: old\n    infer: \"hello\"\n  - id: consumer\n    depends_on: [old]\n    with:\n      data: $old\n    infer: \"{{with.data}}\"\n";
        let offset = text.find(": old").unwrap() as u32 + 2;
        let edits = rename(text, offset, "new_name");
        let result = apply_edits(text, &edits);
        // Verify YAML structure is intact
        assert!(result.contains("- id: new_name"));
        assert!(result.contains("depends_on: [new_name]"));
        assert!(result.contains("$new_name"));
    }

    #[test]
    fn is_valid_task_id_checks() {
        assert!(is_valid_task_id("step1"));
        assert!(is_valid_task_id("fetch_data"));
        assert!(is_valid_task_id("a-b-c"));
        assert!(is_valid_task_id("_private"));
        assert!(!is_valid_task_id(""));
        assert!(!is_valid_task_id("invalid name"));
        assert!(!is_valid_task_id("123start"));
        assert!(!is_valid_task_id("has.dot"));
    }

    #[test]
    fn rename_no_task_at_offset_returns_empty() {
        let text = "schema: \"nika/workflow@0.12\"";
        let edits = rename(text, 0, "new_name");
        assert!(edits.is_empty());
    }
}
