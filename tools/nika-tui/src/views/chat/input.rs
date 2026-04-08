// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Input Rendering for Chat View
//!
//! Contains the render_input function for the command input bar.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::hints::{detect_verb_in_input, verb_placeholder};
use super::render::render_verb_gradient;
use super::{ChatMode, ChatPanel, ChatView};
use crate::theme::Theme;

// ═══════════════════════════════════════════════════════════════════════════════
// Input Rendering Methods
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    pub(super) fn render_input(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Show input with cursor (use tui-input's cursor position)
        let input_value = self.input.value();
        let cursor_pos = self.input.cursor();
        let before_cursor: String = input_value.chars().take(cursor_pos).collect();
        let cursor_char = input_value.chars().nth(cursor_pos).unwrap_or(' ');
        let after_cursor: String = input_value.chars().skip(cursor_pos + 1).collect();

        // UX: Check if input is focused for cursor animation
        let is_focused = self.focused_panel == ChatPanel::Input;

        // Build mode indicators for Claude Code-like UX
        let mut spans = vec![Span::raw(" ")];

        // Mode badge: [⚡ Infer] or [🐔 Agent]
        let mode_color = match self.chat_mode {
            ChatMode::Infer => theme.status_success, // Green for infer
            ChatMode::Agent => theme.status_running, // Amber for agent
        };
        spans.push(Span::styled(
            format!("[{} {}]", self.chat_mode.icon(), self.chat_mode.label()),
            Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
        ));

        // Deep thinking indicator: [🧠 Think] if enabled
        if self.deep_thinking {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                "[🧠 Think]",
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        // Provider indicator
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            &self.provider.name,
            Style::default().fg(theme.text_secondary),
        ));

        // Separator and prompt
        spans.push(Span::styled(" │ ", Style::default().fg(theme.text_muted)));
        spans.push(Span::raw("> "));

        // WOW: Blinking cursor effect (blinks every ~8 frames = ~500ms at 60fps)
        let cursor_visible = is_focused && (self.frame / 8) % 2 == 0;

        // Input text with cursor and placeholder
        if input_value.is_empty() {
            // WOW: Animated placeholder with typing dots when idle
            let dots = match (self.frame / 10) % 4 {
                0 => "   ",
                1 => ".  ",
                2 => ".. ",
                _ => "...",
            };

            // Show blinking cursor at start
            if cursor_visible {
                spans.push(Span::styled("█", Style::default().fg(theme.highlight)));
            } else {
                spans.push(Span::raw(" "));
            }

            // Animated placeholder hint
            spans.push(Span::styled(
                format!(" Type a message{}", if is_focused { dots } else { "..." }),
                Style::default()
                    .fg(theme.text_muted)
                    .add_modifier(Modifier::ITALIC),
            ));
            spans.push(Span::styled(
                " / for commands, @ for files",
                Style::default().fg(theme.text_muted),
            ));
        } else {
            // Detect verb at start of input for colorized rendering
            if let Some((verb_len, verb_color, is_complete, full_verb)) =
                detect_verb_in_input(input_value)
            {
                // Get base color for cursor
                let verb_fg_color = if is_complete {
                    verb_color.animated(self.frame)
                } else {
                    verb_color.muted()
                };

                // Split input into verb part and rest
                let verb_part = &input_value[..verb_len.min(input_value.len())];
                let rest_part = if input_value.len() > verb_len {
                    &input_value[verb_len..]
                } else {
                    ""
                };

                // Calculate remaining letters for autocomplete hint (Tab to complete)
                let typed_verb_len = verb_len.saturating_sub(1); // without /
                let remaining_hint: String = if !is_complete && typed_verb_len < full_verb.len() {
                    full_verb[typed_verb_len..].to_string()
                } else {
                    String::new()
                };

                // Handle cursor position relative to verb boundary
                if cursor_pos < verb_len {
                    // Cursor is within verb - use per-character gradient animation
                    let before_in_verb: String = verb_part.chars().take(cursor_pos).collect();
                    let after_in_verb: String = verb_part.chars().skip(cursor_pos + 1).collect();

                    // ASCII box with animated emoji for the verb

                    let emoji = verb_color.icon();
                    spans.extend(render_verb_gradient(
                        emoji,
                        &verb_color,
                        self.frame,
                        is_complete,
                    ));
                    spans.push(Span::raw(" ")); // Space after emoji

                    // Render before cursor with gradient
                    spans.extend(render_verb_gradient(
                        &before_in_verb,
                        &verb_color,
                        self.frame,
                        is_complete,
                    ));

                    // Cursor character with verb color background (dramatic)
                    if cursor_visible {
                        spans.push(Span::styled(
                            cursor_char.to_string(),
                            Style::default()
                                .bg(verb_color.glow())
                                .fg(Color::Black)
                                .add_modifier(Modifier::BOLD),
                        ));
                    } else {
                        spans.push(Span::styled(
                            cursor_char.to_string(),
                            Style::default()
                                .fg(verb_fg_color)
                                .add_modifier(Modifier::UNDERLINED | Modifier::BOLD),
                        ));
                    }

                    // Render after cursor with gradient
                    spans.extend(render_verb_gradient(
                        &after_in_verb,
                        &verb_color,
                        self.frame,
                        is_complete,
                    ));

                    // Show remaining letters hint for partial match (Tab to complete)
                    if !remaining_hint.is_empty() {
                        spans.push(Span::styled(
                            remaining_hint.clone(),
                            Style::default()
                                .fg(theme.text_muted)
                                .add_modifier(Modifier::ITALIC),
                        ));

                        spans.push(Span::styled(
                            " [Tab]",
                            Style::default().fg(theme.text_muted),
                        ));
                    } else if is_complete {
                    }
                    spans.push(Span::raw(rest_part));
                } else {
                    // Cursor is after verb - full gradient on verb
                    // ASCII box with animated emoji for the verb

                    let emoji = verb_color.icon();
                    spans.extend(render_verb_gradient(
                        emoji,
                        &verb_color,
                        self.frame,
                        is_complete,
                    ));
                    spans.push(Span::raw(" ")); // Space after emoji

                    spans.extend(render_verb_gradient(
                        verb_part,
                        &verb_color,
                        self.frame,
                        is_complete,
                    ));

                    let rest_cursor_pos = cursor_pos - verb_len;
                    let before_rest: String = rest_part.chars().take(rest_cursor_pos).collect();
                    let after_rest: String = rest_part.chars().skip(rest_cursor_pos + 1).collect();

                    spans.push(Span::raw(before_rest));
                    if cursor_visible {
                        spans.push(Span::styled(
                            cursor_char.to_string(),
                            Style::default().bg(theme.highlight).fg(Color::Black),
                        ));
                    } else {
                        spans.push(Span::styled(
                            cursor_char.to_string(),
                            Style::default()
                                .fg(theme.highlight)
                                .add_modifier(Modifier::UNDERLINED),
                        ));
                    }
                    spans.push(Span::raw(after_rest));

                    // Show contextual placeholder when verb is complete and no argument yet
                    if is_complete && rest_part.trim().is_empty() {
                        let placeholder = verb_placeholder(&verb_color, self.frame);
                        // Animated color pulse for placeholder
                        let pulse = ((self.frame as f32 / 30.0).sin() + 1.0) / 2.0; // 0.0-1.0
                        let (r, g, b) = verb_color.muted_tuple();
                        let fade = 0.4 + (pulse * 0.3); // 0.4-0.7 opacity
                        let pr = (r as f32 * fade).min(255.0) as u8;
                        let pg = (g as f32 * fade).min(255.0) as u8;
                        let pb = (b as f32 * fade).min(255.0) as u8;
                        spans.push(Span::styled(
                            format!(" {}", placeholder),
                            Style::default()
                                .fg(Color::Rgb(pr, pg, pb))
                                .add_modifier(Modifier::ITALIC),
                        ));
                        // Tab hint with verb color
                        spans.push(Span::styled(" ⭾", Style::default().fg(verb_color.muted())));
                    }
                }
            } else {
                // No verb detected, render normally
                spans.push(Span::raw(before_cursor));
                // WOW: Blinking block cursor
                if cursor_visible {
                    spans.push(Span::styled(
                        cursor_char.to_string(),
                        Style::default().bg(theme.highlight).fg(Color::Black),
                    ));
                } else {
                    spans.push(Span::styled(
                        cursor_char.to_string(),
                        Style::default()
                            .fg(theme.highlight)
                            .add_modifier(Modifier::UNDERLINED),
                    ));
                }
                spans.push(Span::raw(after_cursor));
            }
        }

        let line = Line::from(spans);

        // Build block with optional scroll indicator in title
        let total_lines = self.calculate_input_lines(area.width);
        let can_scroll_up = self.input_scroll_offset > 0;
        let can_scroll_down = total_lines > self.input_max_lines
            && self.input_scroll_offset < total_lines.saturating_sub(self.input_max_lines);

        // UX: Focus indicators for Input panel (is_focused defined above)
        let mut block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border_style(is_focused));

        // Add scroll indicator title if content exceeds max visible
        if total_lines > self.input_max_lines {
            let scroll_info = format!(
                " {}/{} {}{}",
                self.input_scroll_offset + 1,
                total_lines.saturating_sub(self.input_max_lines) + 1,
                if can_scroll_up { "↑" } else { " " },
                if can_scroll_down { "↓" } else { " " }
            );
            block = block.title_top(Line::from(Span::styled(
                scroll_info,
                Style::default().fg(theme.text_muted),
            )));
        }

        let paragraph = Paragraph::new(line)
            .block(block)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .scroll((self.input_scroll_offset as u16, 0));

        frame.render_widget(paragraph, area);
    }
}
