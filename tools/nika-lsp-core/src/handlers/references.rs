// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! References handler — protocol-agnostic.
//!
//! Finds all references to a task ID across the workflow:
//! - `- id: <task_id>` — the definition itself
//! - `depends_on: [task_id]` — dependency references (inline array)
//! - `depends_on: task_id` — dependency references (scalar)
//! - `depends_on:` multi-line `- task_id` items
//! - `with: { alias: $task_id }` — binding references
//! - Template expressions `{{$task_id}}` and `{{with.<alias>}}` where alias maps to `$task_id`

/// A single reference location, expressed as byte offsets into the document.
///
/// The tower-lsp shim converts these to `ls_types::Range` via `LineIndex`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceEntry {
    /// Start byte offset of the reference in the document.
    pub start_offset: u32,
    /// End byte offset (exclusive).
    pub end_offset: u32,
}

/// Find the task ID at the given byte offset in the document.
///
/// Converts the byte offset to a line/column, then checks the current line for:
/// 1. `- id: <name>` — task definition
/// 2. `depends_on: [<name>, ...]` — inline array entry
/// 3. `depends_on: <name>` — scalar form
/// 4. `- <name>` — multi-line `depends_on` array item
/// 5. `alias: $<name>` — with-block binding reference
pub fn find_task_at_offset(text: &str, offset: u32) -> Option<String> {
    let offset = offset as usize;
    if offset > text.len() {
        return None;
    }

    let (line_idx, col) = offset_to_line_col(text, offset);
    let lines: Vec<&str> = text.lines().collect();
    if line_idx >= lines.len() {
        return None;
    }
    let line = lines[line_idx];
    let trimmed = line.trim();

    // 1. Cursor on `- id: <name>` or `id: <name>`
    if let Some(task_id) = extract_id_at_cursor(trimmed, line, col) {
        return Some(task_id);
    }

    // 2. Cursor on a name inside `depends_on: [...]` or `depends_on: <name>`
    if let Some(task_id) = extract_depends_on_at_cursor(trimmed, line, col) {
        return Some(task_id);
    }

    // 3. Cursor on a multi-line depends_on entry `- <name>`
    if let Some(task_id) = extract_multiline_depends_on_at_cursor(text, line_idx, trimmed, col) {
        return Some(task_id);
    }

    // 4. Cursor on `$<name>` in a with: block binding
    if let Some(task_id) = extract_with_ref_at_cursor(trimmed, line, col) {
        return Some(task_id);
    }

    None
}

/// Find all references to a task ID in the document.
///
/// Scans every line for:
/// - `- id: <task_id>` — the definition
/// - `depends_on:` arrays (inline and multi-line) containing `task_id`
/// - `with:` block values referencing `$task_id`
/// - Template expressions `{{$task_id}}` and `{{with.<alias>}}` where alias binds to `task_id`
///
/// Returns byte-offset ranges sorted and deduplicated.
pub fn find_task_references(text: &str, task_id: &str) -> Vec<ReferenceEntry> {
    let mut refs = Vec::new();
    let lines: Vec<&str> = text.lines().collect();

    // Collect aliases that bind to this task (alias: $task_id)
    let aliases = collect_aliases_for_task(text, task_id);

    let mut line_offset: usize = 0;
    for (line_num, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // 1. Definition: `- id: <task_id>` or `id: <task_id>`
        find_id_references(trimmed, line, line_offset, task_id, &mut refs);

        // 2. depends_on references (inline array and scalar)
        find_depends_on_references(trimmed, line, line_offset, task_id, &mut refs);

        // 3. Multi-line depends_on: `- <task_id>`
        find_multiline_dep_references(text, &lines, line_num, line_offset, task_id, &mut refs);

        // 4. with: block references: `alias: $<task_id>`
        find_with_references(trimmed, line, line_offset, task_id, &mut refs);

        // 5. Template references: `{{$task_id}}`
        find_template_dollar_references(line, line_offset, task_id, &mut refs);

        // 6. Template references via aliases: `{{with.<alias>}}`
        for alias in &aliases {
            find_template_alias_references(line, line_offset, alias, &mut refs);
        }

        // Advance past the line + its actual line ending (\r\n or \n)
        let line_end = line_offset + line.len();
        let bytes = text.as_bytes();
        line_offset =
            if bytes.get(line_end) == Some(&b'\r') && bytes.get(line_end + 1) == Some(&b'\n') {
                line_end + 2
            } else if bytes.get(line_end) == Some(&b'\n') {
                line_end + 1
            } else {
                line_end
            };

        // Suppress unused variable warning
        let _ = line_num;
    }

    // Deduplicate
    refs.sort_by(|a, b| a.start_offset.cmp(&b.start_offset));
    refs.dedup();

    refs
}

// ============================================================================
// Offset conversion
// ============================================================================

/// Convert a byte offset to (line_index, column_index) — both 0-based.
fn offset_to_line_col(text: &str, offset: usize) -> (usize, usize) {
    let clamped = offset.min(text.len());
    let mut line = 0;
    let mut line_start = 0;
    for (i, ch) in text[..clamped].char_indices() {
        if ch == '\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    (line, clamped - line_start)
}

// ============================================================================
// Cursor extraction helpers
// ============================================================================

/// Extract task ID from `- id: <name>` or `id: <name>` line when cursor is on the name.
fn extract_id_at_cursor(trimmed: &str, line: &str, col: usize) -> Option<String> {
    let stripped = trimmed
        .strip_prefix("- id:")
        .or_else(|| trimmed.strip_prefix("id:"))?;
    let value = stripped.trim().trim_matches('"').trim_matches('\'');
    if value.is_empty() {
        return None;
    }
    let val_start = line.find(value)?;
    let val_end = val_start + value.len();
    if col >= val_start && col < val_end {
        Some(value.to_string())
    } else {
        None
    }
}

/// Extract task ID from `depends_on: [a, b]` or `depends_on: a` when cursor is on a name.
fn extract_depends_on_at_cursor(trimmed: &str, line: &str, col: usize) -> Option<String> {
    let stripped = trimmed.strip_prefix("depends_on:")?;
    let value = stripped.trim();

    if value.is_empty() {
        return None;
    }

    // Scalar form: `depends_on: step1`
    if !value.starts_with('[') {
        let task_id = value.trim_matches('"').trim_matches('\'');
        if task_id.is_empty() {
            return None;
        }
        let val_start = line.find(task_id)?;
        let val_end = val_start + task_id.len();
        if col >= val_start && col < val_end {
            return Some(task_id.to_string());
        }
        return None;
    }

    // Array form: `depends_on: [step1, step2]`
    let bracket_start = line.find('[')? + 1;
    let bracket_end = line.find(']')?;
    let inner = &line[bracket_start..bracket_end];

    let mut offset = bracket_start;
    for part in inner.split(',') {
        let ref_name = part.trim().trim_matches('"').trim_matches('\'');
        if ref_name.is_empty() {
            offset += part.len() + 1;
            continue;
        }
        let ref_start_in_part = part.find(ref_name).unwrap_or(0);
        let abs_start = offset + ref_start_in_part;
        let abs_end = abs_start + ref_name.len();
        if col >= abs_start && col < abs_end {
            return Some(ref_name.to_string());
        }
        offset += part.len() + 1;
    }

    None
}

/// Extract task ID from a multi-line depends_on entry `- <name>`.
fn extract_multiline_depends_on_at_cursor(
    text: &str,
    line_idx: usize,
    trimmed: &str,
    col: usize,
) -> Option<String> {
    let item = trimmed.strip_prefix("- ")?;
    let task_id = item.trim().trim_matches('"').trim_matches('\'');
    if task_id.is_empty() || task_id.contains(':') {
        return None;
    }

    if !is_inside_depends_on_block(text, line_idx) {
        return None;
    }

    let lines: Vec<&str> = text.lines().collect();
    let line = lines[line_idx];
    let val_start = line.find(task_id)?;
    let val_end = val_start + task_id.len();
    if col >= val_start && col < val_end {
        Some(task_id.to_string())
    } else {
        None
    }
}

/// Extract task ID from `alias: $<name>` when cursor is on the `$<name>` part.
fn extract_with_ref_at_cursor(trimmed: &str, line: &str, col: usize) -> Option<String> {
    let colon_pos = trimmed.find(':')?;
    let value = trimmed[colon_pos + 1..].trim();

    let task_id = value.strip_prefix('$')?;
    // Handle path syntax: $task_id.field -> task_id
    let task_id = task_id.split('.').next().unwrap_or(task_id).trim();
    if task_id.is_empty() {
        return None;
    }

    let dollar_pos = line.rfind('$')?;
    let val_start = dollar_pos + 1;
    let val_end = val_start + task_id.len();
    if col >= dollar_pos && col < val_end {
        Some(task_id.to_string())
    } else {
        None
    }
}

// ============================================================================
// Reference scanning helpers
// ============================================================================

/// Find `- id: <task_id>` or `id: <task_id>` on a line and emit a reference entry.
fn find_id_references(
    trimmed: &str,
    line: &str,
    line_byte_offset: usize,
    task_id: &str,
    refs: &mut Vec<ReferenceEntry>,
) {
    let id_value = trimmed
        .strip_prefix("- id:")
        .or_else(|| trimmed.strip_prefix("id:"));
    if let Some(stripped) = id_value {
        let value = stripped.trim().trim_matches('"').trim_matches('\'');
        if value == task_id {
            if let Some(col) = find_value_column(line, task_id) {
                let start = line_byte_offset + col;
                refs.push(ReferenceEntry {
                    start_offset: start as u32,
                    end_offset: (start + task_id.len()) as u32,
                });
            }
        }
    }
}

/// Find `depends_on:` references to `task_id` (inline array and scalar).
fn find_depends_on_references(
    trimmed: &str,
    line: &str,
    line_byte_offset: usize,
    task_id: &str,
    refs: &mut Vec<ReferenceEntry>,
) {
    let stripped = match trimmed.strip_prefix("depends_on:") {
        Some(s) => s,
        None => return,
    };
    let value = stripped.trim();

    if value.is_empty() {
        return;
    }

    // Scalar form
    if !value.starts_with('[') {
        let dep = value.trim_matches('"').trim_matches('\'');
        if dep == task_id {
            if let Some(col) = find_value_column(line, task_id) {
                let start = line_byte_offset + col;
                refs.push(ReferenceEntry {
                    start_offset: start as u32,
                    end_offset: (start + task_id.len()) as u32,
                });
            }
        }
        return;
    }

    // Array form: [a, b, c]
    if let (Some(bracket_start), Some(bracket_end)) = (line.find('['), line.find(']')) {
        let inner = &line[bracket_start + 1..bracket_end];
        let mut offset = bracket_start + 1;
        for part in inner.split(',') {
            let ref_name = part.trim().trim_matches('"').trim_matches('\'');
            if ref_name == task_id {
                let ref_start_in_part = part.find(ref_name).unwrap_or(0);
                let abs_start = offset + ref_start_in_part;
                let start = line_byte_offset + abs_start;
                refs.push(ReferenceEntry {
                    start_offset: start as u32,
                    end_offset: (start + task_id.len()) as u32,
                });
            }
            offset += part.len() + 1;
        }
    }
}

/// Find multi-line depends_on references: items like `- task_id` under a `depends_on:` block.
fn find_multiline_dep_references(
    text: &str,
    lines: &[&str],
    line_num: usize,
    line_byte_offset: usize,
    task_id: &str,
    refs: &mut Vec<ReferenceEntry>,
) {
    let trimmed = lines[line_num].trim();

    if let Some(item) = trimmed.strip_prefix("- ") {
        let dep = item.trim().trim_matches('"').trim_matches('\'');
        if dep == task_id && !dep.contains(':') && is_inside_depends_on_block(text, line_num) {
            if let Some(col) = find_value_column(lines[line_num], task_id) {
                let start = line_byte_offset + col;
                refs.push(ReferenceEntry {
                    start_offset: start as u32,
                    end_offset: (start + task_id.len()) as u32,
                });
            }
        }
    }
}

/// Find `alias: $task_id` references in with: blocks.
fn find_with_references(
    trimmed: &str,
    line: &str,
    line_byte_offset: usize,
    task_id: &str,
    refs: &mut Vec<ReferenceEntry>,
) {
    if let Some(colon_pos) = trimmed.find(':') {
        let value = trimmed[colon_pos + 1..].trim();

        if let Some(after_dollar) = value.strip_prefix('$') {
            let ref_id = after_dollar
                .split('.')
                .next()
                .unwrap_or(after_dollar)
                .trim();
            if ref_id == task_id {
                if let Some(dollar_idx) = line.rfind('$') {
                    let start = line_byte_offset + dollar_idx + 1;
                    refs.push(ReferenceEntry {
                        start_offset: start as u32,
                        end_offset: (start + task_id.len()) as u32,
                    });
                }
            }
        }
    }
}

/// Find `{{$task_id}}` template references.
fn find_template_dollar_references(
    line: &str,
    line_byte_offset: usize,
    task_id: &str,
    refs: &mut Vec<ReferenceEntry>,
) {
    let pattern = format!("{{{{${}", task_id);
    let mut search_start = 0;
    while let Some(pos) = line[search_start..].find(&pattern) {
        let abs_pos = search_start + pos;
        let id_start = abs_pos + 3; // skip `{{$`
        let after = &line[id_start + task_id.len()..];
        if after.starts_with("}}") || after.starts_with('.') {
            let start = line_byte_offset + id_start;
            refs.push(ReferenceEntry {
                start_offset: start as u32,
                end_offset: (start + task_id.len()) as u32,
            });
        }
        search_start = abs_pos + pattern.len();
    }
}

/// Find `{{with.<alias>}}` template references.
fn find_template_alias_references(
    line: &str,
    line_byte_offset: usize,
    alias: &str,
    refs: &mut Vec<ReferenceEntry>,
) {
    let pattern = format!("{{{{with.{}", alias);
    let mut search_start = 0;
    while let Some(pos) = line[search_start..].find(&pattern) {
        let abs_pos = search_start + pos;
        let alias_start = abs_pos + 7; // skip `{{with.`
        let after = &line[alias_start + alias.len()..];
        if after.starts_with("}}")
            || after.starts_with('.')
            || after.starts_with(' ')
            || after.starts_with('|')
        {
            let start = line_byte_offset + alias_start;
            refs.push(ReferenceEntry {
                start_offset: start as u32,
                end_offset: (start + alias.len()) as u32,
            });
        }
        search_start = abs_pos + pattern.len();
    }
}

// ============================================================================
// Utility helpers
// ============================================================================

/// Check if a given line index is inside a `depends_on:` multi-line block.
fn is_inside_depends_on_block(text: &str, line_idx: usize) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    if line_idx >= lines.len() {
        return false;
    }

    let target_indent = lines[line_idx].len() - lines[line_idx].trim_start().len();

    for i in (0..line_idx).rev() {
        let prev = lines[i];
        let prev_trimmed = prev.trim();
        let prev_indent = prev.len() - prev_trimmed.len();

        if prev_trimmed == "depends_on:" && prev_indent < target_indent {
            return true;
        }

        if prev_trimmed.starts_with("depends_on:") && prev_trimmed != "depends_on:" {
            return false;
        }

        if prev_indent < target_indent && !prev_trimmed.is_empty() {
            return false;
        }
    }

    false
}

/// Collect all with-block aliases that bind to a given task_id.
fn collect_aliases_for_task(text: &str, task_id: &str) -> Vec<String> {
    let mut aliases = Vec::new();
    let dollar_ref = format!("${}", task_id);

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(colon_pos) = trimmed.find(':') {
            let key = trimmed[..colon_pos].trim();
            let value = trimmed[colon_pos + 1..].trim();

            let dot_ref = format!("{}.", dollar_ref);
            if (value == dollar_ref || value.starts_with(&dot_ref))
                && !key.is_empty()
                && key != "id"
                && key != "depends_on"
                && key != "model"
                && key != "provider"
            {
                aliases.push(key.to_string());
            }
        }
    }

    aliases
}

/// Find the column where `needle` appears in `line`, preferring the position
/// after the last colon (for `id: <value>` patterns).
fn find_value_column(line: &str, needle: &str) -> Option<usize> {
    if let Some(colon_pos) = line.find(':') {
        let after_colon = &line[colon_pos + 1..];
        if let Some(pos) = after_colon.find(needle) {
            return Some(colon_pos + 1 + pos);
        }
    }
    line.find(needle)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── offset_to_line_col ───────────────────────────────────────────────

    #[test]
    fn offset_to_line_col_first_line() {
        let text = "hello\nworld";
        assert_eq!(offset_to_line_col(text, 0), (0, 0));
        assert_eq!(offset_to_line_col(text, 4), (0, 4));
    }

    #[test]
    fn offset_to_line_col_second_line() {
        let text = "hello\nworld";
        assert_eq!(offset_to_line_col(text, 6), (1, 0)); // 'w' of "world"
        assert_eq!(offset_to_line_col(text, 9), (1, 3));
    }

    #[test]
    fn offset_to_line_col_past_end() {
        let text = "ab\ncd";
        assert_eq!(offset_to_line_col(text, 100), (1, 2)); // clamped to end
    }

    // ── find_task_at_offset ──────────────────────────────────────────────

    #[test]
    fn cursor_on_id_definition() {
        let text = "tasks:\n  - id: step1\n    infer: \"Hello\"";
        // "step1" starts at byte offset of line 1 ("  - id: step1")
        //  line 0: "tasks:\n" = 7 bytes
        //  line 1: "  - id: step1" -> "step1" at col 8
        let offset = 7 + 8; // byte 15
        let result = find_task_at_offset(text, offset as u32);
        assert_eq!(result, Some("step1".to_string()));
    }

    #[test]
    fn cursor_on_depends_on_inline() {
        let text = "tasks:\n  - id: step1\n    infer: \"Hello\"\n  - id: step2\n    depends_on: [step1]\n    infer: \"World\"";
        // Find the "step1" in depends_on line
        let dep_line_start = text.find("depends_on: [step1]").unwrap();
        let step1_in_dep = text[dep_line_start..].find("step1").unwrap();
        let offset = dep_line_start + step1_in_dep;
        let result = find_task_at_offset(text, offset as u32);
        assert_eq!(result, Some("step1".to_string()));
    }

    #[test]
    fn cursor_on_depends_on_scalar() {
        let text = "tasks:\n  - id: first\n    infer: \"A\"\n  - id: second\n    depends_on: first\n    infer: \"B\"";
        let dep_start = text.find("depends_on: first").unwrap();
        let first_pos = dep_start + "depends_on: ".len();
        let result = find_task_at_offset(text, first_pos as u32);
        assert_eq!(result, Some("first".to_string()));
    }

    #[test]
    fn cursor_on_with_dollar_ref() {
        let text = "tasks:\n  - id: step1\n    infer: \"Hello\"\n  - id: step2\n    with:\n      input: $step1\n    infer: \"World\"";
        let dollar_pos = text.find("$step1").unwrap();
        let result = find_task_at_offset(text, dollar_pos as u32);
        assert_eq!(result, Some("step1".to_string()));
    }

    #[test]
    fn cursor_on_multiline_depends_on() {
        let text = "tasks:\n  - id: alpha\n    infer: \"A\"\n  - id: gamma\n    depends_on:\n      - alpha\n    infer: \"C\"";
        // Find "alpha" in the "- alpha" line (not the id line)
        let dep_block = text.find("      - alpha").unwrap();
        let alpha_pos = dep_block + "      - ".len();
        let result = find_task_at_offset(text, alpha_pos as u32);
        assert_eq!(result, Some("alpha".to_string()));
    }

    #[test]
    fn cursor_not_on_task_id() {
        let text = "schema: nika/workflow@0.12\nworkflow: test";
        let result = find_task_at_offset(text, 0);
        assert!(result.is_none());
    }

    #[test]
    fn cursor_empty_document() {
        let result = find_task_at_offset("", 0);
        assert!(result.is_none());
    }

    #[test]
    fn cursor_past_end() {
        let text = "tasks:\n  - id: step1";
        let result = find_task_at_offset(text, 9999);
        assert!(result.is_none());
    }

    // ── find_task_references ─────────────────────────────────────────────

    #[test]
    fn refs_definition_and_depends_on() {
        let text = "tasks:\n  - id: step1\n    infer: \"Hello\"\n  - id: step2\n    depends_on: [step1]\n    infer: \"World\"";
        let refs = find_task_references(text, "step1");
        assert_eq!(refs.len(), 2, "Expected 2 references, got: {:?}", refs);
        // First ref: definition
        let def_slice = &text[refs[0].start_offset as usize..refs[0].end_offset as usize];
        assert_eq!(def_slice, "step1");
        // Second ref: depends_on
        let dep_slice = &text[refs[1].start_offset as usize..refs[1].end_offset as usize];
        assert_eq!(dep_slice, "step1");
        // They should be different positions
        assert_ne!(refs[0].start_offset, refs[1].start_offset);
    }

    #[test]
    fn refs_with_block_dollar() {
        let text = "tasks:\n  - id: generate\n    infer: \"Generate\"\n  - id: process\n    with:\n      data: $generate\n    infer: \"Process\"";
        let refs = find_task_references(text, "generate");
        assert_eq!(refs.len(), 2, "Expected 2 references, got: {:?}", refs);
        let def_slice = &text[refs[0].start_offset as usize..refs[0].end_offset as usize];
        assert_eq!(def_slice, "generate");
        let with_slice = &text[refs[1].start_offset as usize..refs[1].end_offset as usize];
        assert_eq!(with_slice, "generate");
    }

    #[test]
    fn refs_multiline_depends_on() {
        let text = "tasks:\n  - id: alpha\n    infer: \"A\"\n  - id: beta\n    infer: \"B\"\n  - id: gamma\n    depends_on:\n      - alpha\n      - beta\n    infer: \"C\"";
        let refs = find_task_references(text, "alpha");
        assert_eq!(refs.len(), 2, "Expected 2 references, got: {:?}", refs);
    }

    #[test]
    fn refs_depends_on_scalar() {
        let text = "tasks:\n  - id: first\n    infer: \"A\"\n  - id: second\n    depends_on: first\n    infer: \"B\"";
        let refs = find_task_references(text, "first");
        assert_eq!(refs.len(), 2, "Expected 2 references, got: {:?}", refs);
    }

    #[test]
    fn refs_template_dollar() {
        let text = "tasks:\n  - id: data\n    infer: \"Get data\"\n  - id: process\n    infer: \"Process {{$data}}\"";
        let refs = find_task_references(text, "data");
        assert_eq!(refs.len(), 2, "Expected 2 references, got: {:?}", refs);
    }

    #[test]
    fn refs_template_alias() {
        let text = "tasks:\n  - id: step1\n    infer: \"Hello\"\n  - id: step2\n    with:\n      result: $step1\n    infer: \"Process {{with.result}}\"";
        let refs = find_task_references(text, "step1");
        // definition + with ref + template alias
        assert_eq!(refs.len(), 3, "Expected 3 references, got: {:?}", refs);
    }

    #[test]
    fn refs_no_references_only_definition() {
        let text = "tasks:\n  - id: lonely\n    infer: \"I am alone\"\n  - id: other\n    infer: \"Something else\"";
        let refs = find_task_references(text, "lonely");
        assert_eq!(refs.len(), 1);
    }

    #[test]
    fn refs_unknown_task_empty() {
        let text = "tasks:\n  - id: step1\n    infer: \"Hello\"";
        let refs = find_task_references(text, "nonexistent");
        assert!(refs.is_empty());
    }

    #[test]
    fn refs_empty_document() {
        let refs = find_task_references("", "step1");
        assert!(refs.is_empty());
    }

    #[test]
    fn refs_offsets_are_precise() {
        let text = "tasks:\n  - id: step1\n    depends_on: [step1]";
        let refs = find_task_references(text, "step1");
        assert_eq!(refs.len(), 2);

        for r in &refs {
            let slice = &text[r.start_offset as usize..r.end_offset as usize];
            assert_eq!(slice, "step1", "Reference should point exactly at 'step1'");
        }
    }

    // ── is_inside_depends_on_block ───────────────────────────────────────

    #[test]
    fn inside_depends_on_block_true() {
        let text = "  - id: gamma\n    depends_on:\n      - alpha\n      - beta";
        assert!(is_inside_depends_on_block(text, 2));
        assert!(is_inside_depends_on_block(text, 3));
    }

    #[test]
    fn inside_depends_on_block_false() {
        let text = "  - id: gamma\n    with:\n      data: $alpha\n    infer: \"test\"";
        assert!(!is_inside_depends_on_block(text, 2));
    }

    // ── collect_aliases_for_task ──────────────────────────────────────────

    #[test]
    fn collect_aliases() {
        let text = "  - id: step2\n    with:\n      result: $step1\n      extra: $step1.output\n    infer: \"test\"";
        let aliases = collect_aliases_for_task(text, "step1");
        assert!(aliases.contains(&"result".to_string()));
        assert!(aliases.contains(&"extra".to_string()));
    }

    #[test]
    fn collect_aliases_no_match() {
        let text = "  - id: step2\n    with:\n      data: $other_task\n    infer: \"test\"";
        let aliases = collect_aliases_for_task(text, "step1");
        assert!(aliases.is_empty());
    }
}
