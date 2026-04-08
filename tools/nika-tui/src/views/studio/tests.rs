// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::syntax::YamlHighlight;
use super::*;

// ═══════════════════════════════════════════════════════════════════════════
// Schema validation tests (TDD)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_yaml_editor_panel_schema_validation_valid_workflow() {
    let mut view = YamlEditorPanel::new();

    // Valid Nika workflow YAML
    let valid_yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: step1
    infer: "Hello world""#;

    view.buffer = TextBuffer::from_content(valid_yaml);
    view.validate();

    assert!(view.validation.yaml_valid, "YAML should be valid");
    assert!(view.validation.schema_valid, "Schema should be valid");
    assert!(view.validation.errors.is_empty(), "No errors expected");
}

#[test]
fn test_yaml_editor_panel_schema_validation_invalid_schema() {
    let mut view = YamlEditorPanel::new();

    // Invalid Nika workflow - missing required 'tasks' field
    let invalid_yaml = r#"schema: "nika/workflow@0.12"
unknown_field: "should fail""#;

    view.buffer = TextBuffer::from_content(invalid_yaml);
    view.validate();

    assert!(view.validation.yaml_valid, "YAML syntax is valid");
    assert!(!view.validation.schema_valid, "Schema should be invalid");
    assert!(!view.validation.errors.is_empty(), "Should have errors");
}

#[test]
fn test_yaml_editor_panel_schema_validation_missing_schema_field() {
    let mut view = YamlEditorPanel::new();

    // Missing required 'schema' field
    let yaml = r#"tasks:
  - id: step1
    infer: "Hello""#;

    view.buffer = TextBuffer::from_content(yaml);
    view.validate();

    assert!(view.validation.yaml_valid, "YAML syntax is valid");
    assert!(!view.validation.schema_valid, "Schema should be invalid");
    assert!(
        view.validation.errors.iter().any(|e| e.contains("schema")),
        "Error should mention 'schema'"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// TextBuffer tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_text_buffer_default() {
    let buffer = TextBuffer::default();
    assert_eq!(buffer.lines().len(), 1);
    assert_eq!(buffer.lines()[0], "");
    assert_eq!(buffer.cursor(), (0, 0));
}

#[test]
fn test_text_buffer_from_content() {
    let buffer = TextBuffer::from_content("line1\nline2\nline3");
    assert_eq!(buffer.lines().len(), 3);
    assert_eq!(buffer.lines()[0], "line1");
    assert_eq!(buffer.lines()[1], "line2");
    assert_eq!(buffer.lines()[2], "line3");
}

#[test]
fn test_text_buffer_from_empty_content() {
    let buffer = TextBuffer::from_content("");
    assert_eq!(buffer.lines().len(), 1);
    assert_eq!(buffer.lines()[0], "");
}

#[test]
fn test_text_buffer_content() {
    let buffer = TextBuffer::from_content("a\nb\nc");
    assert_eq!(buffer.content(), "a\nb\nc");
}

#[test]
fn test_text_buffer_cursor_movement() {
    let mut buffer = TextBuffer::from_content("abc\ndef\nghi");

    // Move down
    buffer.cursor_down();
    assert_eq!(buffer.cursor(), (1, 0));

    // Move right
    buffer.cursor_right();
    buffer.cursor_right();
    assert_eq!(buffer.cursor(), (1, 2));

    // Move up (cursor col should clamp)
    buffer.cursor_up();
    assert_eq!(buffer.cursor(), (0, 2));

    // Move left
    buffer.cursor_left();
    assert_eq!(buffer.cursor(), (0, 1));
}

#[test]
fn test_text_buffer_cursor_boundary() {
    let mut buffer = TextBuffer::from_content("ab\ncd");

    // Can't go up from first line
    buffer.cursor_up();
    assert_eq!(buffer.cursor(), (0, 0));

    // Go to last line
    buffer.cursor_down();
    buffer.cursor_down(); // Should stay at last line
    assert_eq!(buffer.cursor(), (1, 0));
}

#[test]
fn test_text_buffer_insert_char() {
    let mut buffer = TextBuffer::default();
    buffer.insert_char('a');
    buffer.insert_char('b');
    buffer.insert_char('c');
    assert_eq!(buffer.lines()[0], "abc");
    assert_eq!(buffer.cursor(), (0, 3));
}

#[test]
fn test_text_buffer_insert_newline() {
    let mut buffer = TextBuffer::from_content("abc");
    buffer.cursor_right();
    buffer.cursor_right(); // cursor at position 2
    buffer.insert_newline();
    assert_eq!(buffer.lines().len(), 2);
    assert_eq!(buffer.lines()[0], "ab");
    assert_eq!(buffer.lines()[1], "c");
    assert_eq!(buffer.cursor(), (1, 0));
}

#[test]
fn test_text_buffer_backspace() {
    let mut buffer = TextBuffer::from_content("abc");
    buffer.cursor_right();
    buffer.cursor_right();
    buffer.cursor_right();
    buffer.backspace();
    assert_eq!(buffer.lines()[0], "ab");
    assert_eq!(buffer.cursor(), (0, 2));
}

#[test]
fn test_text_buffer_backspace_merge_lines() {
    let mut buffer = TextBuffer::from_content("ab\ncd");
    buffer.cursor_down();
    buffer.backspace(); // Should merge lines
    assert_eq!(buffer.lines().len(), 1);
    assert_eq!(buffer.lines()[0], "abcd");
    assert_eq!(buffer.cursor(), (0, 2));
}

#[test]
fn test_text_buffer_delete() {
    let mut buffer = TextBuffer::from_content("abc");
    buffer.cursor_right();
    buffer.delete();
    assert_eq!(buffer.lines()[0], "ac");
}

// ═══════════════════════════════════════════════════════════════════════════
// YamlEditorPanel tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_yaml_editor_panel_new() {
    let view = YamlEditorPanel::new();
    assert_eq!(view.mode, EditorMode::Normal);
    assert!(!view.modified);
    assert!(view.path.is_none());
}

#[test]
fn test_yaml_editor_panel_mode_switch() {
    let mut view = YamlEditorPanel::new();
    assert_eq!(view.mode, EditorMode::Normal);

    view.mode = EditorMode::Insert;
    assert_eq!(view.mode, EditorMode::Insert);
}

#[test]
fn test_yaml_editor_panel_validation_valid_yaml_syntax() {
    let mut view = YamlEditorPanel::new();

    // Valid YAML syntax (but not a valid Nika workflow schema)
    view.buffer = TextBuffer::from_content("key: value");
    view.validate();
    assert!(view.validation.yaml_valid, "YAML syntax should be valid");
    // Note: schema validation will fail because this isn't a valid workflow
    // but yaml_valid should be true because the syntax is correct
    assert!(
        !view.validation.schema_valid,
        "Schema should be invalid for non-workflow YAML"
    );
}

#[test]
fn test_yaml_editor_panel_validation_invalid_yaml() {
    let mut view = YamlEditorPanel::new();

    // Invalid YAML
    view.buffer = TextBuffer::from_content("key: [unclosed");
    view.validate();
    assert!(!view.validation.yaml_valid);
    assert!(!view.validation.errors.is_empty());
}

#[test]
fn test_yaml_editor_panel_cursor_position() {
    let view = YamlEditorPanel::new();
    assert_eq!(view.current_line(), 1);
    assert_eq!(view.current_col(), 1);
}

#[test]
fn test_yaml_editor_panel_default_validation_result() {
    let result = ValidationResult::default();
    assert!(result.yaml_valid);
    assert!(result.schema_valid);
    assert!(result.warnings.is_empty());
    assert!(result.errors.is_empty());
}

#[test]
fn test_yaml_editor_panel_status_line_normal_mode() {
    let view = YamlEditorPanel::new();
    let state = TuiState::new("test.nika.yaml");
    let status = view.status_line(&state);
    assert!(status.contains("NORMAL"));
    assert!(status.contains("Ln 1"));
    assert!(status.contains("Col 1"));
}

#[test]
fn test_yaml_editor_panel_status_line_insert_mode() {
    let mut view = YamlEditorPanel::new();
    view.mode = EditorMode::Insert;
    let state = TuiState::new("test.nika.yaml");
    let status = view.status_line(&state);
    assert!(status.contains("INSERT"));
}

#[test]
fn test_yaml_editor_panel_status_line_modified() {
    let mut view = YamlEditorPanel::new();
    view.modified = true;
    let state = TuiState::new("test.nika.yaml");
    let status = view.status_line(&state);
    assert!(status.contains("●"));
}

#[test]
fn test_editor_mode_default() {
    let mode = EditorMode::default();
    assert_eq!(mode, EditorMode::Normal);
}

#[test]
fn test_yaml_editor_panel_handle_normal_mode_quit() {
    let mut view = YamlEditorPanel::new();
    let mut state = TuiState::new("test.nika.yaml");
    let key = KeyEvent::from(KeyCode::Char('q'));
    let action = view.handle_key(key, &mut state);
    match action {
        ViewAction::SwitchView(TuiView::Studio) => {}
        _ => panic!("Expected SwitchView(Studio)"),
    }
}

#[test]
fn test_yaml_editor_panel_handle_normal_mode_insert() {
    let mut view = YamlEditorPanel::new();
    let mut state = TuiState::new("test.nika.yaml");
    let key = KeyEvent::from(KeyCode::Char('i'));
    let _ = view.handle_key(key, &mut state);
    assert_eq!(view.mode, EditorMode::Insert);
}

#[test]
fn test_yaml_editor_panel_handle_insert_mode_escape() {
    let mut view = YamlEditorPanel::new();
    view.mode = EditorMode::Insert;
    let mut state = TuiState::new("test.nika.yaml");
    let key = KeyEvent::from(KeyCode::Esc);
    let _ = view.handle_key(key, &mut state);
    assert_eq!(view.mode, EditorMode::Normal);
}

#[test]
fn test_yaml_editor_panel_handle_insert_mode_typing() {
    let mut view = YamlEditorPanel::new();
    view.mode = EditorMode::Insert;
    let mut state = TuiState::new("test.nika.yaml");

    // Type some characters
    view.handle_key(KeyEvent::from(KeyCode::Char('a')), &mut state);
    view.handle_key(KeyEvent::from(KeyCode::Char('b')), &mut state);
    view.handle_key(KeyEvent::from(KeyCode::Char('c')), &mut state);

    assert_eq!(view.buffer.lines()[0], "abc");
    assert!(view.modified);
}

// ═══════════════════════════════════════════════════════════════════════════
// YAML syntax highlighting tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_yaml_highlight_comment() {
    let base = Style::default();
    let spans = YamlHighlight::highlight_line("# This is a comment", base);
    assert_eq!(spans.len(), 1);
    // Should have comment color (gray)
    assert_eq!(spans[0].style.fg, Some(YamlHighlight::COMMENT));
}

#[test]
fn test_yaml_highlight_key_value() {
    let base = Style::default();
    let spans = YamlHighlight::highlight_line("name: my-workflow", base);
    assert!(spans.len() >= 2, "Should have key and value spans");
    // First span should be the key with colon
    assert!(spans[0].content.contains("name:"));
}

#[test]
fn test_yaml_highlight_verb() {
    let base = Style::default();
    let spans = YamlHighlight::highlight_line("    infer: \"prompt\"", base);
    // Should highlight 'infer' as a Nika verb (cyan)
    assert!(spans[0].content.contains("infer:"));
    assert_eq!(spans[0].style.fg, Some(YamlHighlight::VERB));
}

#[test]
fn test_yaml_highlight_boolean() {
    let base = Style::default();
    let spans = YamlHighlight::highlight_line("enabled: true", base);
    assert!(spans.len() >= 2);
    // Value should have boolean color (purple)
    let value_span = &spans[1];
    assert_eq!(value_span.style.fg, Some(YamlHighlight::BOOL));
}

#[test]
fn test_yaml_highlight_number() {
    let base = Style::default();
    let spans = YamlHighlight::highlight_line("port: 8080", base);
    assert!(spans.len() >= 2);
    // Value should have number color (orange)
    let value_span = &spans[1];
    assert_eq!(value_span.style.fg, Some(YamlHighlight::NUMBER));
}

#[test]
fn test_yaml_highlight_string() {
    let base = Style::default();
    let spans = YamlHighlight::highlight_line("prompt: \"Hello world\"", base);
    assert!(spans.len() >= 2);
    // Value should have string color (green)
    let value_span = &spans[1];
    assert_eq!(value_span.style.fg, Some(YamlHighlight::STRING));
}

#[test]
fn test_yaml_highlight_list_item() {
    let base = Style::default();
    let spans = YamlHighlight::highlight_line("  - item", base);
    assert!(
        spans.len() >= 2,
        "Should have indent, dash, and value spans"
    );
    // Should contain dash
    assert!(spans.iter().any(|s| s.content.contains("-")));
}

#[test]
fn test_yaml_highlight_empty_line() {
    let base = Style::default();
    let spans = YamlHighlight::highlight_line("", base);
    assert_eq!(spans.len(), 1);
    assert!(spans[0].content.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// StudioView lifecycle tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_studio_view_on_enter_initializes_tree_state() {
    use crate::state::TuiState;
    use crate::views::View;

    // Create a StudioView with the current directory (which should have files)
    let mut studio = StudioView::new();
    let mut state = TuiState::new("test.yaml");

    // Before on_enter: tree_state should be empty
    assert!(
        studio.tree_state.visible_nodes().is_empty(),
        "visible_nodes should be empty before on_enter"
    );
    assert!(
        studio.tree_state.selected().is_none(),
        "selection should be None before on_enter"
    );

    // Call on_enter (the lifecycle hook)
    studio.on_enter(&mut state);

    // After on_enter: tree_state should be initialized
    assert!(
        !studio.tree_state.visible_nodes().is_empty(),
        "visible_nodes should NOT be empty after on_enter"
    );
    assert!(
        studio.tree_state.selected().is_some(),
        "selection should be Some after on_enter"
    );
    assert!(
        studio.cached_tree.is_some(),
        "cached_tree should be Some after on_enter"
    );
}

#[test]
fn test_studio_view_on_enter_expands_root() {
    use crate::state::TuiState;
    use crate::views::View;

    let mut studio = StudioView::new();
    let mut state = TuiState::new("test.yaml");

    studio.on_enter(&mut state);

    // Root directory should be expanded
    if let Some(ref tree) = studio.cached_tree {
        assert!(
            studio.tree_state.is_expanded(tree.id),
            "Root directory should be expanded after on_enter"
        );
    } else {
        panic!("cached_tree should be Some after on_enter");
    }
}

#[test]
fn test_studio_view_tab_cycles_panels_in_normal_mode() {
    use crate::state::TuiState;
    use crate::views::View;

    let mut studio = StudioView::new();
    let mut state = TuiState::new("test.yaml");

    // Start in Browser focus, editor in Normal mode
    studio.focus = StudioFocus::Browser;
    studio.editor.mode = EditorMode::Normal;

    // Press Tab - should cycle to Editor
    let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
    studio.handle_key(key, &mut state);

    assert_eq!(
        studio.focus,
        StudioFocus::Editor,
        "Tab should cycle from Browser to Editor"
    );

    // Press Tab again - should cycle to Dag (not insert indent)
    studio.handle_key(key, &mut state);
    assert_eq!(
        studio.focus,
        StudioFocus::Dag,
        "Tab should cycle from Editor to Dag when in Normal mode"
    );
}

#[test]
fn test_studio_view_tab_indents_in_insert_mode() {
    use crate::state::TuiState;
    use crate::views::View;

    let mut studio = StudioView::new();
    let mut state = TuiState::new("test.yaml");

    // Focus editor in Insert mode
    studio.focus = StudioFocus::Editor;
    studio.editor.mode = EditorMode::Insert;

    // Press Tab - should NOT cycle panels (handled by editor)
    let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
    studio.handle_key(key, &mut state);

    // Focus should still be Editor (Tab was sent to editor, not used for cycling)
    assert_eq!(
        studio.focus,
        StudioFocus::Editor,
        "Tab should NOT cycle panels when editor is in Insert mode"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Selection tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_text_buffer_selection_single_line() {
    let mut buffer = TextBuffer::from_content("hello world");

    // Select "world"
    buffer.start_selection();
    buffer.cursor_right();
    buffer.cursor_right();
    buffer.cursor_right();
    buffer.cursor_right();
    buffer.cursor_right();
    buffer.cursor_right(); // Now at position 6
    buffer.sync_selection_to_cursor();

    assert!(buffer.has_selection());
    assert_eq!(buffer.get_selected_text(), Some("hello ".to_string()));
}

#[test]
fn test_text_buffer_selection_clear() {
    let mut buffer = TextBuffer::from_content("hello");
    buffer.start_selection();
    buffer.cursor_right();
    buffer.cursor_right();
    buffer.sync_selection_to_cursor();

    assert!(buffer.has_selection());
    buffer.clear_selection();
    assert!(!buffer.has_selection());
}

#[test]
fn test_text_buffer_delete_selection() {
    let mut buffer = TextBuffer::from_content("hello world");

    // Select "hello "
    buffer.start_selection();
    for _ in 0..6 {
        buffer.cursor_right();
    }
    buffer.sync_selection_to_cursor();

    assert!(buffer.delete_selection());
    assert_eq!(buffer.content(), "world");
    assert!(!buffer.has_selection());
}

#[test]
fn test_text_buffer_select_all() {
    let mut buffer = TextBuffer::from_content("line1\nline2\nline3");
    buffer.select_all();

    assert!(buffer.has_selection());
    assert_eq!(
        buffer.get_selected_text(),
        Some("line1\nline2\nline3".to_string())
    );
}

#[test]
fn test_text_buffer_delete_multiline_selection() {
    let mut buffer = TextBuffer::from_content("aaa\nbbb\nccc");

    // Move to line 0, col 1 and start selection
    buffer.cursor_right();
    buffer.start_selection();

    // Move to line 2, col 1
    buffer.cursor_down();
    buffer.cursor_down();
    buffer.sync_selection_to_cursor();

    assert!(buffer.has_selection());
    buffer.delete_selection();

    // Should have "a" + "cc" = "acc"
    assert_eq!(buffer.content(), "acc");
}

#[test]
fn test_multi_cursor_add_cursor() {
    let mut buffer = TextBuffer::from_content("line1\nline2\nline3");

    assert_eq!(buffer.cursor_count(), 1);
    assert!(!buffer.is_multi_cursor());

    buffer.add_cursor(1, 0);
    assert_eq!(buffer.cursor_count(), 2);
    assert!(buffer.is_multi_cursor());

    buffer.add_cursor(2, 0);
    assert_eq!(buffer.cursor_count(), 3);
}

#[test]
fn test_multi_cursor_clear_additional() {
    let mut buffer = TextBuffer::from_content("hello\nworld");

    buffer.add_cursor(1, 0);
    buffer.add_cursor(1, 2);
    assert_eq!(buffer.cursor_count(), 3);

    buffer.clear_additional_cursors();
    assert_eq!(buffer.cursor_count(), 1);
    assert!(!buffer.is_multi_cursor());
}

#[test]
fn test_select_word_under_cursor() {
    let mut buffer = TextBuffer::from_content("hello world test");

    // Position cursor in "world"
    for _ in 0..7 {
        buffer.cursor_right();
    }

    // First Ctrl+D should select "world"
    let selected = buffer.select_next_occurrence();
    assert!(selected);
    assert!(buffer.has_selection());
    assert_eq!(buffer.get_selected_text(), Some("world".to_string()));
}

#[test]
fn test_select_next_occurrence() {
    let mut buffer = TextBuffer::from_content("foo bar foo baz foo");

    // Select first "foo" manually
    buffer.start_selection();
    for _ in 0..3 {
        buffer.cursor_right();
    }
    buffer.sync_selection_to_cursor();

    assert_eq!(buffer.get_selected_text(), Some("foo".to_string()));
    assert_eq!(buffer.cursor_count(), 1);

    // Ctrl+D should find next "foo" and add cursor
    let found = buffer.select_next_occurrence();
    assert!(found);
    assert_eq!(buffer.cursor_count(), 2);
}

#[test]
fn test_select_next_occurrence_wraps() {
    let mut buffer = TextBuffer::from_content("ab cd ab");

    // Select last "ab" (position 6-8)
    for _ in 0..6 {
        buffer.cursor_right();
    }
    buffer.start_selection();
    buffer.cursor_right();
    buffer.cursor_right();
    buffer.sync_selection_to_cursor();

    assert_eq!(buffer.get_selected_text(), Some("ab".to_string()));

    // Should wrap around and find "ab" at position 0
    let found = buffer.select_next_occurrence();
    assert!(found);
    assert_eq!(buffer.cursor_count(), 2);
}

#[test]
fn test_select_word_boundary() {
    let mut buffer = TextBuffer::from_content("hello_world test_case");

    // cursor at 'h'
    let selected = buffer.select_next_occurrence();
    assert!(selected);
    // Should select "hello_world" (underscore is part of word)
    assert_eq!(buffer.get_selected_text(), Some("hello_world".to_string()));
}
