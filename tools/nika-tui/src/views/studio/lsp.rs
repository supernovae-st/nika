// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! In-process LSP integration for the Studio editor.
//!
//! Provides completion, hover tooltips, and code actions by calling
//! nika_lsp_core directly (same pattern as DiagnosticsEngine).

use nika_lsp_core::analysis::context::detect_context;
use nika_lsp_core::handler::LspHandler;

use super::{
    CodeActionDisplay, CodeActionState, CompletionEntry, HoverState, TextBuffer, YamlEditorPanel,
};

// ═══════════════════════════════════════════════════════════════════════════════
// In-process LSP: Completion + Hover (Phase 4)
// Same pattern as DiagnosticsEngine: call nika_lsp_core directly.
// ═══════════════════════════════════════════════════════════════════════════════

impl YamlEditorPanel {
    /// Trigger completion at the current cursor position.
    ///
    /// Calls `nika_lsp_core::DefaultHandler::completion()` in-process and
    /// populates the completion popup state.
    pub(super) fn trigger_completion(&mut self) {
        let content = self.buffer.content();
        let offset = self.buffer.cursor_position() as u32;
        let context = detect_context(&content, offset, None);
        let items = self.lsp_handler.completion(&content, offset, &context);

        self.completion.items = items
            .into_iter()
            .map(|item| CompletionEntry {
                label: item.label.clone(),
                kind: item.kind.map(|k| format!("{:?}", k)).unwrap_or_default(),
                detail: item.detail.clone(),
                insert_text: item.insert_text.unwrap_or(item.label),
            })
            .collect();
        self.completion.selected = 0;

        // Walk backward from cursor to find the word start, not the cursor
        // position after the trigger char was inserted (phantom completion fix).
        let (row, col) = self.buffer.cursor();
        let line = &self.buffer.lines()[row];
        let byte_col = TextBuffer::char_to_byte(line, col);
        let word_start = line[..byte_col]
            .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != ':')
            .map(|i| i + 1)
            .unwrap_or(0);
        self.completion.trigger_col = word_start;

        self.completion.visible = !self.completion.items.is_empty();
    }

    /// Accept the currently selected completion item.
    ///
    /// Deletes back to the trigger column, then inserts the completion text.
    pub(super) fn accept_completion(&mut self) {
        if let Some(item) = self.completion.items.get(self.completion.selected) {
            let insert = item.insert_text.clone();
            // Delete back to trigger column
            while self.buffer.cursor().1 > self.completion.trigger_col {
                self.buffer.backspace();
            }
            // Insert completion text
            for c in insert.chars() {
                self.buffer.insert_char(c);
            }
            self.mark_edited();
        }
        self.completion.visible = false;
    }

    /// Trigger hover tooltip at the current cursor position.
    pub(super) fn trigger_hover(&mut self) {
        let content = self.buffer.content();
        let offset = self.buffer.cursor_position() as u32;
        let context = detect_context(&content, offset, None);
        if let Some(result) = self.lsp_handler.hover(&content, offset, &context) {
            self.hover = HoverState {
                visible: true,
                content: result.contents,
            };
        }
    }

    /// Trigger code actions at the current cursor position (Ctrl+.).
    pub(super) fn trigger_code_actions(&mut self) {
        let content = self.buffer.content();
        let offset = self.buffer.cursor_position() as u32;
        let actions = self.lsp_handler.code_actions(&content, offset, offset + 1);

        self.code_actions = CodeActionState {
            visible: !actions.is_empty(),
            actions: actions
                .into_iter()
                .map(|a| CodeActionDisplay {
                    title: a.title,
                    edit: a.edit.map(|e| (e.offset, e.end_offset, e.new_text)),
                })
                .collect(),
            selected: 0,
        };
    }

    /// Accept the currently selected code action.
    pub(super) fn accept_code_action(&mut self) {
        if let Some(action) = self.code_actions.actions.get(self.code_actions.selected) {
            if let Some((offset, end_offset, ref new_text)) = action.edit {
                // Apply the text edit: replace the byte range with new_text.
                // Strategy: reconstruct buffer content with the edit applied,
                // then set the content and position the cursor at end of insertion.
                let content = self.buffer.content();
                let start = (offset as usize).min(content.len());
                let end = (end_offset as usize).min(content.len());
                // Ensure we slice at valid char boundaries to avoid panics
                // on multi-byte UTF-8 sequences.
                let start = snap_to_char_boundary(&content, start);
                let end = snap_to_char_boundary(&content, end).max(start);

                let mut new_content =
                    String::with_capacity(content.len() - (end - start) + new_text.len());
                new_content.push_str(&content[..start]);
                new_content.push_str(new_text);
                new_content.push_str(&content[end..]);

                self.buffer.set_content(&new_content);
                // Position cursor at end of inserted text
                self.buffer.set_cursor_position(start + new_text.len());
                self.mark_edited();
            }
        }
        self.code_actions.visible = false;
    }
}

/// Snap a byte index to the nearest valid char boundary at or before `idx`.
fn snap_to_char_boundary(s: &str, idx: usize) -> usize {
    let idx = idx.min(s.len());
    let mut i = idx;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}
