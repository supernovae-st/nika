//! Editor panel rendering for the Studio view.
//!
//! Contains `render_editor()` on `YamlEditorPanel` which draws the YAML editor
//! with syntax highlighting, line numbers, git gutter, diagnostic underlines,
//! scroll indicators, completion popup, hover tooltip, and code action overlay.

use std::collections::HashMap;
use std::fmt::Write;

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use super::syntax::YamlHighlight;
use super::types::EditorMode;
use super::YamlEditorPanel;
use crate::diagnostics::DiagnosticSeverity;
use crate::git::LineChange;
use crate::theme::Theme;
use crate::widgets::ScrollIndicator;

impl YamlEditorPanel {
    /// Render just the editor content (for use in StudioView 3-panel layout)
    pub(super) fn render_editor(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let mode_indicator = match self.mode {
            EditorMode::Normal => "",
            EditorMode::Insert => " [INSERT]",
        };

        // Breadcrumb with file path (VS Code-like)
        let title = if let Some(path) = &self.path {
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            let icon = if filename.ends_with(".nika.yaml") {
                "🦋"
            } else {
                "📄"
            };
            let modified = if self.buffer.is_modified() {
                " •"
            } else {
                ""
            };
            format!(" {} {}{}{} ", icon, filename, modified, mode_indicator)
        } else {
            format!(" EDITOR{} ", mode_indicator)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(theme.border_normal));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Split inner into content + status bar
        let content_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: inner.height.saturating_sub(1), // Leave 1 row for status bar
        };
        let status_area = Rect {
            x: inner.x,
            y: inner.y + inner.height.saturating_sub(1),
            width: inner.width,
            height: 1,
        };

        // Show placeholder when no file is loaded
        if self.path.is_none() {
            // Vertically center: total content is 7 lines, pad top accordingly
            let content_lines = 7u16;
            let pad_top = content_area.height.saturating_sub(content_lines) / 2;

            let mut placeholder = vec![Line::from(""); pad_top as usize];
            placeholder.extend(vec![
                Line::from(Span::styled(
                    "No file open",
                    Style::default()
                        .fg(theme.text_muted)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Select a .nika.yaml file",
                    Style::default().fg(theme.text_muted),
                )),
                Line::from(Span::styled(
                    "from the Browser panel",
                    Style::default().fg(theme.text_muted),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("[Enter]", Style::default().fg(theme.border_focused)),
                    Span::styled(" Open file", Style::default().fg(theme.text_muted)),
                ]),
                Line::from(vec![
                    Span::styled("[Tab]  ", Style::default().fg(theme.border_focused)),
                    Span::styled(" Switch panel", Style::default().fg(theme.text_muted)),
                ]),
            ]);

            let paragraph =
                Paragraph::new(placeholder).alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(paragraph, content_area);
            return;
        }

        // Calculate visible height (excluding borders and status bar) and sync to buffer
        let visible_height = content_area.height as usize;
        self.buffer.set_visible_height(visible_height);

        // Build lines with line numbers, syntax highlighting, and selection
        let selection = self.buffer.selection();

        // Get git line changes for gutter display
        // We clone the changes to avoid borrow issues in the closure
        let git_line_changes: Option<HashMap<usize, LineChange>> =
            self.path.as_ref().and_then(|p| {
                self.git_status
                    .as_mut()
                    .map(|gs| gs.line_changes(p).changes_map())
            });

        // PERF: Reusable buffer for line number formatting (~40 calls/frame)
        let mut line_buf = String::with_capacity(8);

        let lines: Vec<Line> = self
            .buffer
            .lines()
            .iter()
            .enumerate()
            .skip(self.buffer.scroll_offset())
            .take(visible_height)
            .map(|(i, line)| {
                let line_num = i + 1;
                let is_cursor_line = i == self.buffer.cursor().0;

                let base_style = if is_cursor_line {
                    Style::default().bg(theme.highlight)
                } else {
                    Style::default()
                };

                // Git gutter indicator
                let git_gutter = git_line_changes
                    .as_ref()
                    .and_then(|changes| changes.get(&i))
                    .map(|change| {
                        let (symbol, color) = match change {
                            LineChange::Added => ("+", theme.git_added),
                            LineChange::Modified => ("~", theme.git_modified),
                            LineChange::Deleted => ("-", theme.git_deleted),
                        };
                        Span::styled(symbol, Style::default().fg(color))
                    })
                    .unwrap_or_else(|| Span::raw(" "));

                // Diagnostic gutter icon (2 chars: icon + space)
                let diag_gutter = if let Some(diag) = self.diagnostics.most_severe_on_line(i) {
                    match diag.severity {
                        DiagnosticSeverity::Error => {
                            Span::styled("● ", Style::default().fg(theme.diagnostic_error))
                        }
                        DiagnosticSeverity::Warning => {
                            Span::styled("▲ ", Style::default().fg(theme.diagnostic_warning))
                        }
                        DiagnosticSeverity::Hint => {
                            Span::styled("◆ ", Style::default().fg(theme.diagnostic_hint))
                        }
                    }
                } else {
                    Span::raw("  ")
                };

                // Line number with git gutter + diagnostic gutter prefix
                // PERF: Reuse line_buf instead of format! allocation per line
                line_buf.clear();
                let _ = write!(line_buf, "{:4} ", line_num);
                let mut spans = vec![
                    git_gutter,
                    diag_gutter,
                    Span::styled(line_buf.clone(), Style::default().fg(theme.text_muted)),
                ];

                // Check for selection on this line
                let selection_range = selection.line_range(i);

                if let Some((sel_start, sel_end)) = selection_range {
                    // Line has selection - split into before/selected/after
                    let sel_end_col = sel_end.unwrap_or(line.len());
                    let sel_start = sel_start.min(line.len());
                    let sel_end_col = sel_end_col.min(line.len());

                    // Before selection
                    if sel_start > 0 {
                        let before = &line[..sel_start];
                        spans.extend(YamlHighlight::highlight_line(before, base_style));
                    }

                    // Selected portion
                    if sel_start < sel_end_col {
                        let selected = &line[sel_start..sel_end_col];
                        let selection_style = Style::default().bg(theme.selection).fg(theme.text);
                        spans.push(Span::styled(selected.to_string(), selection_style));
                    }

                    // After selection (only if selection ends on this line)
                    if sel_end.is_some() && sel_end_col < line.len() {
                        let after = &line[sel_end_col..];
                        spans.extend(YamlHighlight::highlight_line(after, base_style));
                    } else if sel_end.is_none() {
                        // Selection continues to next line - no after portion
                    }
                } else {
                    // No selection on this line - normal highlighting
                    spans.extend(YamlHighlight::highlight_line(line, base_style));
                }

                Line::from(spans)
            })
            .collect();

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, content_area);

        // Post-render: apply diagnostic underlines directly to frame buffer
        let scroll_offset = self.buffer.scroll_offset();
        let gutter_width = 8u16; // git(1) + diag(2) + linenum(5)
        for diag in self.diagnostics.diagnostics() {
            if diag.start_line < scroll_offset || diag.start_line >= scroll_offset + visible_height
            {
                continue; // Off-screen
            }
            let underline_color = match diag.severity {
                DiagnosticSeverity::Error => theme.diagnostic_error,
                DiagnosticSeverity::Warning => theme.diagnostic_warning,
                DiagnosticSeverity::Hint => theme.diagnostic_hint,
            };
            let y = content_area.y + (diag.start_line - scroll_offset) as u16;
            let start_x = content_area.x + gutter_width + diag.start_col as u16;
            let end_col = if diag.end_line == diag.start_line {
                diag.end_col
            } else {
                80
            };
            let end_x = content_area.x + gutter_width + end_col as u16;
            for x in start_x..end_x.min(content_area.right()) {
                if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
                    cell.set_style(
                        cell.style()
                            .fg(underline_color)
                            .add_modifier(Modifier::UNDERLINED),
                    );
                }
            }
        }

        // Render ScrollIndicator with dynamic arrows
        let total_lines = self.buffer.lines().len();
        if total_lines > visible_height {
            let scrollbar_area = Rect {
                x: content_area.x + content_area.width.saturating_sub(1),
                y: content_area.y,
                width: 1,
                height: content_area.height,
            };

            let scroll_indicator = ScrollIndicator::new()
                .position(self.buffer.scroll_offset(), total_lines, visible_height)
                .thumb_style(Style::default().fg(theme.scrollbar_thumb))
                .track_style(Style::default().fg(theme.scrollbar_track))
                .show_arrows(true);

            frame.render_widget(scroll_indicator, scrollbar_area);
        }

        // Render status bar (Ln:Col | Diagnostic | Mode | Schema)
        // Show cursor count when multi-cursor active
        let (cursor_row, cursor_col) = self.buffer.cursor();
        let mode_str = match self.mode {
            EditorMode::Normal => "NORMAL",
            EditorMode::Insert => "INSERT",
        };
        let cursor_count = self.buffer.cursor_count();
        let status_left = if cursor_count > 1 {
            format!(
                " Ln {}, Col {} ({} cursors) ",
                cursor_row + 1,
                cursor_col + 1,
                cursor_count
            )
        } else {
            format!(" Ln {}, Col {} ", cursor_row + 1, cursor_col + 1)
        };

        // Diagnostic message for cursor line (if any)
        let diag_msg = self
            .diagnostics
            .most_severe_on_line(self.buffer.cursor().0)
            .map(|d| {
                let icon = match d.severity {
                    DiagnosticSeverity::Error => "● ",
                    DiagnosticSeverity::Warning => "▲ ",
                    DiagnosticSeverity::Hint => "◆ ",
                };
                let color = match d.severity {
                    DiagnosticSeverity::Error => theme.diagnostic_error,
                    DiagnosticSeverity::Warning => theme.diagnostic_warning,
                    DiagnosticSeverity::Hint => theme.diagnostic_hint,
                };
                let text = format!("{}{}: {} ", icon, d.code, d.message);
                (text, color)
            });

        let status_right = format!(" {} | nika/workflow ", mode_str);

        // Calculate padding for right-alignment (account for diagnostic message width)
        let diag_width = diag_msg.as_ref().map_or(0, |(text, _)| text.len());
        let used = status_left.len() + diag_width + status_right.len();
        let padding = (status_area.width as usize).saturating_sub(used);
        let padding_str = " ".repeat(padding);

        let mut status_spans = vec![Span::styled(
            status_left,
            Style::default().fg(theme.text_muted),
        )];
        if let Some((text, color)) = diag_msg {
            status_spans.push(Span::styled(text, Style::default().fg(color)));
        }
        status_spans.push(Span::raw(padding_str));
        status_spans.push(Span::styled(
            status_right,
            Style::default().fg(theme.text_muted),
        ));

        let status_line = Line::from(status_spans);

        let status_paragraph = Paragraph::new(status_line)
            .style(Style::default().bg(theme.highlight).fg(theme.text_primary));
        frame.render_widget(status_paragraph, status_area);

        // ── Completion popup overlay ──────────────────────────────────
        // Rendered LAST so it draws on top of everything else.
        if self.completion.visible && !self.completion.items.is_empty() {
            self.render_completion_popup(frame, &content_area, visible_height, theme);
        }

        // ── Hover tooltip overlay ─────────────────────────────────────
        if self.hover.visible && !self.hover.content.is_empty() {
            self.render_hover_tooltip(frame, &content_area, theme);
        }

        // ── Code action popup overlay ───────────────────────────────────
        if self.code_actions.visible && !self.code_actions.actions.is_empty() {
            self.render_code_action_popup(frame, &content_area, visible_height, theme);
        }

        // ── Terminal cursor position (Insert mode) ──────────────────────
        // Show a blinking cursor in Insert mode so the user knows where
        // typing will appear.
        if self.mode == EditorMode::Insert {
            let gutter_cur = 8u16; // git(1) + diag(2) + linenum(5)
            let cursor_row = self.buffer.cursor().0;
            let cursor_col = self.buffer.cursor().1;
            let scroll_off = self.buffer.scroll_offset();

            if cursor_row >= scroll_off && cursor_row < scroll_off + visible_height {
                let screen_y = content_area.y + (cursor_row - scroll_off) as u16;
                let screen_x = content_area.x + gutter_cur + cursor_col as u16;
                if screen_x < content_area.right() && screen_y < content_area.bottom() {
                    frame.set_cursor_position(ratatui::layout::Position::new(screen_x, screen_y));
                }
            }
        }
    }

    /// Render the completion popup overlay
    fn render_completion_popup(
        &self,
        frame: &mut Frame,
        content_area: &Rect,
        _visible_height: usize,
        theme: &Theme,
    ) {
        let max_visible = 8.min(self.completion.items.len());
        let max_label_width = self
            .completion
            .items
            .iter()
            .take(max_visible)
            .map(|i| i.label.len())
            .max()
            .unwrap_or(10);
        let popup_width = (max_label_width + 4).min(40) as u16;

        // Position below cursor
        let gutter = 8u16; // git(1) + diag(2) + linenum(5)
        let cursor_viewport_row = (self
            .buffer
            .cursor()
            .0
            .saturating_sub(self.buffer.scroll_offset())) as u16;
        let popup_x = (content_area.x + gutter + self.buffer.cursor().1 as u16)
            .min(content_area.right().saturating_sub(popup_width + 2));
        let popup_y = (content_area.y + cursor_viewport_row + 1)
            .min(content_area.bottom().saturating_sub(max_visible as u16 + 2));

        let popup_area = Rect::new(popup_x, popup_y, popup_width + 2, max_visible as u16 + 2);

        // Clear area and draw popup
        frame.render_widget(Clear, popup_area);

        let popup_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.popup_border))
            .style(Style::default().bg(theme.popup_bg));

        let completion_items: Vec<ListItem> = self
            .completion
            .items
            .iter()
            .enumerate()
            .take(max_visible)
            .map(|(idx, item)| {
                let style = if idx == self.completion.selected {
                    Style::default()
                        .bg(theme.popup_selected)
                        .fg(theme.text_primary)
                } else {
                    Style::default().fg(theme.text_secondary)
                };
                ListItem::new(Span::styled(&*item.label, style))
            })
            .collect();

        frame.render_widget(List::new(completion_items).block(popup_block), popup_area);
    }

    /// Render the hover tooltip overlay
    fn render_hover_tooltip(&self, frame: &mut Frame, content_area: &Rect, theme: &Theme) {
        let lines: Vec<&str> = self.hover.content.lines().take(8).collect();
        let h = lines.len() as u16 + 2;
        let w = lines.iter().map(|l| l.len()).max().unwrap_or(20).min(60) as u16 + 4;

        let gutter = 8u16;
        let cursor_vp_row = (self
            .buffer
            .cursor()
            .0
            .saturating_sub(self.buffer.scroll_offset())) as u16;
        let popup_x = (content_area.x + gutter).min(content_area.right().saturating_sub(w));
        let popup_y = content_area.y + cursor_vp_row.saturating_sub(h);

        let popup_area = Rect::new(popup_x, popup_y, w, h);

        frame.render_widget(Clear, popup_area);

        let hover_block = Block::default()
            .title(" Documentation ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_focused))
            .style(Style::default().bg(theme.popup_bg));

        let text: Vec<Line> = lines
            .iter()
            .map(|l| {
                Line::from(Span::styled(
                    l.to_string(),
                    Style::default().fg(theme.text_primary),
                ))
            })
            .collect();

        frame.render_widget(Paragraph::new(text).block(hover_block), popup_area);
    }

    /// Render the code action popup overlay
    fn render_code_action_popup(
        &self,
        frame: &mut Frame,
        content_area: &Rect,
        _visible_height: usize,
        theme: &Theme,
    ) {
        let max_visible = 8.min(self.code_actions.actions.len());
        let max_title_width = self
            .code_actions
            .actions
            .iter()
            .take(max_visible)
            .map(|a| a.title.len())
            .max()
            .unwrap_or(20);
        let popup_width = (max_title_width + 4).min(50) as u16;

        // Position below cursor (same pattern as completion popup)
        let gutter_ca = 8u16;
        let cursor_vp_row_ca = (self
            .buffer
            .cursor()
            .0
            .saturating_sub(self.buffer.scroll_offset())) as u16;
        let popup_x_ca = (content_area.x + gutter_ca + self.buffer.cursor().1 as u16)
            .min(content_area.right().saturating_sub(popup_width + 2));
        let popup_y_ca = (content_area.y + cursor_vp_row_ca + 1)
            .min(content_area.bottom().saturating_sub(max_visible as u16 + 2));

        let popup_area_ca = Rect::new(
            popup_x_ca,
            popup_y_ca,
            popup_width + 2,
            max_visible as u16 + 2,
        );

        frame.render_widget(Clear, popup_area_ca);

        let ca_block = Block::default()
            .title(" Code Actions ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.diagnostic_warning))
            .style(Style::default().bg(theme.popup_bg));

        let ca_items: Vec<ListItem> = self
            .code_actions
            .actions
            .iter()
            .enumerate()
            .take(max_visible)
            .map(|(idx, action)| {
                let style = if idx == self.code_actions.selected {
                    Style::default()
                        .bg(theme.popup_selected)
                        .fg(theme.text_primary)
                } else {
                    Style::default().fg(theme.text_secondary)
                };
                // Prefix preferred actions with a lightbulb
                let icon = if action.edit.is_some() { "💡 " } else { "  " };
                ListItem::new(Span::styled(format!("{}{}", icon, action.title), style))
            })
            .collect();

        frame.render_widget(List::new(ca_items).block(ca_block), popup_area_ca);
    }
}
