//! References Handler
//!
//! Finds all references to a task ID across the workflow:
//! - `- id: <task_id>` — the definition itself
//! - `depends_on: [task_id]` — dependency references
//! - `with: { alias: $task_id }` — binding references (note the `$` prefix)
//! - Template references in strings (partial, best-effort)

#[cfg(feature = "lsp")]
use tower_lsp_server::ls_types::*;

/// Extract the task ID the cursor is currently on.
///
/// Checks the following patterns (in order):
/// 1. `- id: <name>` — task definition line
/// 2. `depends_on: [<name>, ...]` — inline array entry
/// 3. `depends_on: <name>` — scalar form
/// 4. `- <name>` — multi-line `depends_on` array item
/// 5. `alias: $<name>` — with-block binding reference
#[cfg(feature = "lsp")]
pub fn find_task_at_cursor(text: &str, position: Position) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let line_idx = position.line as usize;
    if line_idx >= lines.len() {
        return None;
    }
    let line = lines[line_idx];
    let col = position.character as usize;
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
/// Scans for:
/// - `- id: <task_id>` — the definition
/// - `depends_on:` arrays (inline and multi-line) containing `task_id`
/// - `with:` block values referencing `$task_id` (with or without `$`)
/// - Template expressions `{{$task_id}}` and `{{with.<alias>}}` where alias binds to `task_id`
#[cfg(feature = "lsp")]
pub fn find_task_references(text: &str, task_id: &str) -> Vec<Range> {
    let mut ranges = Vec::new();
    let lines: Vec<&str> = text.lines().collect();

    // Collect aliases that bind to this task (alias: $task_id)
    let aliases = collect_aliases_for_task(text, task_id);

    for (line_num, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // 1. Definition: `- id: <task_id>` or `id: <task_id>`
        find_id_references(trimmed, line, line_num, task_id, &mut ranges);

        // 2. depends_on references (inline array and scalar)
        find_depends_on_references(trimmed, line, line_num, task_id, &mut ranges);

        // 3. Multi-line depends_on: `- <task_id>`
        find_multiline_dep_references(text, &lines, line_num, task_id, &mut ranges);

        // 4. with: block references: `alias: $<task_id>`
        find_with_references(trimmed, line, line_num, task_id, &mut ranges);

        // 5. Template references: `{{$task_id}}`
        find_template_dollar_references(line, line_num, task_id, &mut ranges);

        // 6. Template references via aliases: `{{with.<alias>}}`
        for alias in &aliases {
            find_template_alias_references(line, line_num, alias, &mut ranges);
        }
    }

    // Deduplicate ranges (same range could be found by multiple scanners)
    ranges.sort_by(|a, b| {
        a.start
            .line
            .cmp(&b.start.line)
            .then(a.start.character.cmp(&b.start.character))
    });
    ranges.dedup();

    ranges
}

// ============================================================================
// Cursor extraction helpers
// ============================================================================

/// Extract task ID from `- id: <name>` or `id: <name>` line when cursor is on the name.
#[cfg(feature = "lsp")]
fn extract_id_at_cursor(trimmed: &str, line: &str, col: usize) -> Option<String> {
    let stripped = trimmed
        .strip_prefix("- id:")
        .or_else(|| trimmed.strip_prefix("id:"))?;
    let value = stripped.trim().trim_matches('"').trim_matches('\'');
    if value.is_empty() {
        return None;
    }
    // Find position of the value in the original line
    let val_start = line.find(value)?;
    let val_end = val_start + value.len();
    if col >= val_start && col < val_end {
        Some(value.to_string())
    } else {
        None
    }
}

/// Extract task ID from `depends_on: [a, b]` or `depends_on: a` when cursor is on a name.
#[cfg(feature = "lsp")]
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
///
/// Looks at previous lines to verify we are inside a `depends_on:` block.
#[cfg(feature = "lsp")]
fn extract_multiline_depends_on_at_cursor(
    text: &str,
    line_idx: usize,
    trimmed: &str,
    col: usize,
) -> Option<String> {
    // Must be a YAML list item: `- <name>`
    let item = trimmed.strip_prefix("- ")?;
    let task_id = item.trim().trim_matches('"').trim_matches('\'');
    if task_id.is_empty() || task_id.contains(':') {
        return None;
    }

    // Verify this list item belongs to a `depends_on:` block
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
#[cfg(feature = "lsp")]
fn extract_with_ref_at_cursor(trimmed: &str, line: &str, col: usize) -> Option<String> {
    let colon_pos = trimmed.find(':')?;
    let value = trimmed[colon_pos + 1..].trim();

    // Must start with $ to be a task reference
    let task_id = value.strip_prefix('$')?;
    // Handle path syntax: $task_id.field -> task_id
    let task_id = task_id.split('.').next().unwrap_or(task_id).trim();
    if task_id.is_empty() {
        return None;
    }

    // Find position of the task_id (after $) in the original line
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

/// Find `- id: <task_id>` or `id: <task_id>` on a line and add the range of the task_id.
#[cfg(feature = "lsp")]
fn find_id_references(
    trimmed: &str,
    line: &str,
    line_num: usize,
    task_id: &str,
    ranges: &mut Vec<Range>,
) {
    let id_value = trimmed
        .strip_prefix("- id:")
        .or_else(|| trimmed.strip_prefix("id:"));
    if let Some(stripped) = id_value {
        let value = stripped.trim().trim_matches('"').trim_matches('\'');
        if value == task_id {
            if let Some(start_col) = find_value_column(line, task_id) {
                ranges.push(make_range(line_num, start_col, task_id.len()));
            }
        }
    }
}

/// Find `depends_on:` references to `task_id` (inline array and scalar).
#[cfg(feature = "lsp")]
fn find_depends_on_references(
    trimmed: &str,
    line: &str,
    line_num: usize,
    task_id: &str,
    ranges: &mut Vec<Range>,
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
            if let Some(start_col) = find_value_column(line, task_id) {
                ranges.push(make_range(line_num, start_col, task_id.len()));
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
                ranges.push(make_range(line_num, abs_start, task_id.len()));
            }
            offset += part.len() + 1;
        }
    }
}

/// Find multi-line depends_on references: items like `- task_id` under a `depends_on:` block.
#[cfg(feature = "lsp")]
fn find_multiline_dep_references(
    text: &str,
    lines: &[&str],
    line_num: usize,
    task_id: &str,
    ranges: &mut Vec<Range>,
) {
    let trimmed = lines[line_num].trim();

    // Check for `- <name>` pattern (list item without colon in the name)
    if let Some(item) = trimmed.strip_prefix("- ") {
        let dep = item.trim().trim_matches('"').trim_matches('\'');
        if dep == task_id && !dep.contains(':') && is_inside_depends_on_block(text, line_num) {
            if let Some(start_col) = find_value_column(lines[line_num], task_id) {
                ranges.push(make_range(line_num, start_col, task_id.len()));
            }
        }
    }
}

/// Find `alias: $task_id` references in with: blocks.
#[cfg(feature = "lsp")]
fn find_with_references(
    trimmed: &str,
    line: &str,
    line_num: usize,
    task_id: &str,
    ranges: &mut Vec<Range>,
) {
    if let Some(colon_pos) = trimmed.find(':') {
        let value = trimmed[colon_pos + 1..].trim();

        // Check for $task_id or $task_id.field
        if let Some(after_dollar) = value.strip_prefix('$') {
            let ref_id = after_dollar
                .split('.')
                .next()
                .unwrap_or(after_dollar)
                .trim();
            if ref_id == task_id {
                // Find the $ position in the original line and point to the task_id after it
                if let Some(dollar_idx) = line.rfind('$') {
                    let start_col = dollar_idx + 1;
                    ranges.push(make_range(line_num, start_col, task_id.len()));
                }
            }
        }
    }
}

/// Find `{{$task_id}}` template references.
#[cfg(feature = "lsp")]
fn find_template_dollar_references(
    line: &str,
    line_num: usize,
    task_id: &str,
    ranges: &mut Vec<Range>,
) {
    let pattern = format!("{{{{${}", task_id);
    let mut search_start = 0;
    while let Some(pos) = line[search_start..].find(&pattern) {
        let abs_pos = search_start + pos;
        // The task_id starts after `{{$`
        let id_start = abs_pos + 3; // skip `{{$`
                                    // Verify it ends with `}}` or `.something}}`
        let after = &line[id_start + task_id.len()..];
        if after.starts_with("}}") || after.starts_with('.') {
            ranges.push(make_range(line_num, id_start, task_id.len()));
        }
        search_start = abs_pos + pattern.len();
    }
}

/// Find `{{with.<alias>}}` template references.
#[cfg(feature = "lsp")]
fn find_template_alias_references(
    line: &str,
    line_num: usize,
    alias: &str,
    ranges: &mut Vec<Range>,
) {
    let pattern = format!("{{{{with.{}", alias);
    let mut search_start = 0;
    while let Some(pos) = line[search_start..].find(&pattern) {
        let abs_pos = search_start + pos;
        // The alias starts after `{{with.`
        let alias_start = abs_pos + 7; // skip `{{with.`
                                       // Verify it ends with `}}` or `.field}}` or `|`
        let after = &line[alias_start + alias.len()..];
        if after.starts_with("}}")
            || after.starts_with('.')
            || after.starts_with(' ')
            || after.starts_with('|')
        {
            ranges.push(make_range(line_num, alias_start, alias.len()));
        }
        search_start = abs_pos + pattern.len();
    }
}

// ============================================================================
// Utility helpers
// ============================================================================

/// Check if a given line index is inside a `depends_on:` multi-line block.
///
/// Walks backwards from the line to find a `depends_on:` header, verifying
/// indentation is consistent with being a child list item.
#[cfg(feature = "lsp")]
fn is_inside_depends_on_block(text: &str, line_idx: usize) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    if line_idx >= lines.len() {
        return false;
    }

    let target_indent = lines[line_idx].len() - lines[line_idx].trim_start().len();

    // Walk backwards to find the depends_on: header
    for i in (0..line_idx).rev() {
        let prev = lines[i];
        let prev_trimmed = prev.trim();
        let prev_indent = prev.len() - prev_trimmed.len();

        // Found a depends_on: header at a lesser indent level
        if prev_trimmed == "depends_on:" && prev_indent < target_indent {
            return true;
        }

        // Found a depends_on with inline value — not a multi-line block
        if prev_trimmed.starts_with("depends_on:") && prev_trimmed != "depends_on:" {
            return false;
        }

        // Hit something at a lesser indent that is not depends_on — bail
        if prev_indent < target_indent && !prev_trimmed.is_empty() {
            return false;
        }
    }

    false
}

/// Collect all with-block aliases that bind to a given task_id.
///
/// Scans for patterns like `alias: $task_id` and returns the alias names.
#[cfg(feature = "lsp")]
fn collect_aliases_for_task(text: &str, task_id: &str) -> Vec<String> {
    let mut aliases = Vec::new();
    let dollar_ref = format!("${}", task_id);

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(colon_pos) = trimmed.find(':') {
            let key = trimmed[..colon_pos].trim();
            let value = trimmed[colon_pos + 1..].trim();

            // Match `alias: $task_id` or `alias: $task_id.field`
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
#[cfg(feature = "lsp")]
fn find_value_column(line: &str, needle: &str) -> Option<usize> {
    // For `id: task_id` or `depends_on: task_id`, find the needle after `:` if possible
    if let Some(colon_pos) = line.find(':') {
        let after_colon = &line[colon_pos + 1..];
        if let Some(pos) = after_colon.find(needle) {
            return Some(colon_pos + 1 + pos);
        }
    }
    // Fallback: find anywhere in line
    line.find(needle)
}

/// Build an LSP `Range` from a line number, start column, and length.
#[cfg(feature = "lsp")]
fn make_range(line: usize, start_col: usize, len: usize) -> Range {
    Range {
        start: Position {
            line: line as u32,
            character: start_col as u32,
        },
        end: Position {
            line: line as u32,
            character: (start_col + len) as u32,
        },
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build a Position.
    #[cfg(feature = "lsp")]
    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    // ── find_task_at_cursor tests ──────────────────────────────────────

    #[test]
    #[cfg(feature = "lsp")]
    fn test_cursor_on_id_definition() {
        let text = "tasks:\n  - id: step1\n    infer: \"Hello\"";
        // Cursor on "step1" in `- id: step1` (line 1, col 9 = start of "step1")
        let result = find_task_at_cursor(text, pos(1, 9));
        assert_eq!(result, Some("step1".to_string()));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_cursor_on_depends_on_entry() {
        let text = r#"tasks:
  - id: step1
    infer: "Hello"
  - id: step2
    depends_on: [step1]
    infer: "World""#;
        // Cursor on "step1" inside depends_on (line 4)
        let line4 = "    depends_on: [step1]";
        let step1_start = line4.find("step1").unwrap();
        let result = find_task_at_cursor(text, pos(4, step1_start as u32));
        assert_eq!(result, Some("step1".to_string()));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_cursor_on_with_dollar_ref() {
        let text = r#"tasks:
  - id: step1
    infer: "Hello"
  - id: step2
    with:
      input: $step1
    infer: "World""#;
        // Cursor on "$step1" in with block (line 5)
        let line5 = "      input: $step1";
        let dollar_pos = line5.find('$').unwrap();
        let result = find_task_at_cursor(text, pos(5, dollar_pos as u32));
        assert_eq!(result, Some("step1".to_string()));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_cursor_not_on_task_id() {
        let text = "schema: nika/workflow@0.12\nworkflow: test";
        let result = find_task_at_cursor(text, pos(0, 0));
        assert!(result.is_none());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_cursor_empty_document() {
        let text = "";
        let result = find_task_at_cursor(text, pos(0, 0));
        assert!(result.is_none());
    }

    // ── find_task_references tests ─────────────────────────────────────

    #[test]
    #[cfg(feature = "lsp")]
    fn test_definition_and_depends_on_reference() {
        let text = r#"tasks:
  - id: step1
    infer: "Hello"
  - id: step2
    depends_on: [step1]
    infer: "World""#;
        let refs = find_task_references(text, "step1");
        // Should find: definition (line 1) + depends_on (line 4)
        assert_eq!(refs.len(), 2, "Expected 2 references, got: {:?}", refs);
        assert_eq!(refs[0].start.line, 1); // - id: step1
        assert_eq!(refs[1].start.line, 4); // depends_on: [step1]
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_with_block_dollar_reference() {
        let text = r#"tasks:
  - id: generate
    infer: "Generate"
  - id: process
    with:
      data: $generate
    infer: "Process""#;
        let refs = find_task_references(text, "generate");
        // Should find: definition (line 1) + with ref (line 5)
        assert_eq!(refs.len(), 2, "Expected 2 references, got: {:?}", refs);
        assert_eq!(refs[0].start.line, 1); // - id: generate
        assert_eq!(refs[1].start.line, 5); // data: $generate
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_multiple_references() {
        let text = r#"tasks:
  - id: fetch
    exec: "curl http://example.com"
  - id: transform
    depends_on: [fetch]
    with:
      raw: $fetch
    infer: "Transform {{with.raw}}"
  - id: publish
    depends_on: [fetch, transform]
    infer: "Publish""#;
        let refs = find_task_references(text, "fetch");
        // definition (line 1) + depends_on in transform (line 4) + with ref (line 6)
        // + depends_on in publish (line 9) + template alias `raw` (line 7)
        assert!(
            refs.len() >= 4,
            "Expected at least 4 references to 'fetch', got {}: {:?}",
            refs.len(),
            refs
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_task_with_no_references_only_definition() {
        let text = r#"tasks:
  - id: lonely
    infer: "I am alone"
  - id: other
    infer: "Something else""#;
        let refs = find_task_references(text, "lonely");
        // Should find only the definition itself
        assert_eq!(
            refs.len(),
            1,
            "Expected 1 reference (definition only), got: {:?}",
            refs
        );
        assert_eq!(refs[0].start.line, 1);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_empty_document_no_references() {
        let text = "";
        let refs = find_task_references(text, "step1");
        assert!(refs.is_empty());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_task_id_not_found() {
        let text = r#"tasks:
  - id: step1
    infer: "Hello""#;
        let refs = find_task_references(text, "nonexistent");
        assert!(refs.is_empty());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_multiline_depends_on() {
        let text = r#"tasks:
  - id: alpha
    infer: "A"
  - id: beta
    infer: "B"
  - id: gamma
    depends_on:
      - alpha
      - beta
    infer: "C""#;
        let refs = find_task_references(text, "alpha");
        // definition (line 1) + multi-line depends_on entry (line 7)
        assert_eq!(refs.len(), 2, "Expected 2 references, got: {:?}", refs);
        assert_eq!(refs[0].start.line, 1); // - id: alpha
        assert_eq!(refs[1].start.line, 7); // - alpha
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_template_dollar_reference() {
        let text = r#"tasks:
  - id: data
    infer: "Get data"
  - id: process
    infer: "Process {{$data}}""#;
        let refs = find_task_references(text, "data");
        // definition (line 1) + template {{$data}} (line 4)
        assert_eq!(refs.len(), 2, "Expected 2 references, got: {:?}", refs);
        assert_eq!(refs[0].start.line, 1);
        assert_eq!(refs[1].start.line, 4);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_template_alias_reference() {
        let text = r#"tasks:
  - id: step1
    infer: "Hello"
  - id: step2
    with:
      result: $step1
    infer: "Process {{with.result}}""#;
        let refs = find_task_references(text, "step1");
        // definition (line 1) + with ref (line 5) + template via alias (line 6)
        assert_eq!(refs.len(), 3, "Expected 3 references, got: {:?}", refs);
        assert_eq!(refs[0].start.line, 1); // - id: step1
        assert_eq!(refs[1].start.line, 5); // result: $step1
        assert_eq!(refs[2].start.line, 6); // {{with.result}}
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_depends_on_scalar_form() {
        let text = r#"tasks:
  - id: first
    infer: "A"
  - id: second
    depends_on: first
    infer: "B""#;
        let refs = find_task_references(text, "first");
        assert_eq!(refs.len(), 2, "Expected 2 references, got: {:?}", refs);
        assert_eq!(refs[0].start.line, 1); // - id: first
        assert_eq!(refs[1].start.line, 4); // depends_on: first
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_range_columns_are_precise() {
        let text = "tasks:\n  - id: step1\n    depends_on: [step1]";
        let refs = find_task_references(text, "step1");
        assert_eq!(refs.len(), 2);

        // Check the definition range points exactly to "step1" in "- id: step1"
        let def_range = &refs[0];
        let line1 = "  - id: step1";
        let expected_start = line1.find("step1").unwrap() as u32;
        assert_eq!(def_range.start.character, expected_start);
        assert_eq!(
            def_range.end.character,
            expected_start + "step1".len() as u32
        );

        // Check the depends_on range
        let dep_range = &refs[1];
        let line2 = "    depends_on: [step1]";
        let expected_dep_start = line2.find('[').unwrap() as u32 + 1;
        assert_eq!(dep_range.start.character, expected_dep_start);
        assert_eq!(
            dep_range.end.character,
            expected_dep_start + "step1".len() as u32
        );
    }

    // ── is_inside_depends_on_block tests ───────────────────────────────

    #[test]
    #[cfg(feature = "lsp")]
    fn test_is_inside_depends_on_block_true() {
        let text = r#"  - id: gamma
    depends_on:
      - alpha
      - beta"#;
        assert!(is_inside_depends_on_block(text, 2)); // "- alpha"
        assert!(is_inside_depends_on_block(text, 3)); // "- beta"
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_is_inside_depends_on_block_false() {
        let text = r#"  - id: gamma
    with:
      data: $alpha
    infer: "test""#;
        assert!(!is_inside_depends_on_block(text, 2)); // "data: $alpha"
    }

    // ── collect_aliases_for_task tests ─────────────────────────────────

    #[test]
    #[cfg(feature = "lsp")]
    fn test_collect_aliases() {
        let text = r#"  - id: step2
    with:
      result: $step1
      extra: $step1.output
    infer: "test""#;
        let aliases = collect_aliases_for_task(text, "step1");
        assert!(aliases.contains(&"result".to_string()));
        assert!(aliases.contains(&"extra".to_string()));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_collect_aliases_no_match() {
        let text = r#"  - id: step2
    with:
      data: $other_task
    infer: "test""#;
        let aliases = collect_aliases_for_task(text, "step1");
        assert!(aliases.is_empty());
    }
}
