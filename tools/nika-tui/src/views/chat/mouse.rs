// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Mouse Handling for Chat View
//!
//! Contains mouse event handling and coordinate mapping.

use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::layout::Rect;

use super::{ChatMessageSelection, ChatSelectionPos, ChatView};
use crate::state::ChatPanel;
use crate::views::chat::layout::{compute_panel_areas, point_in_rect};

impl ChatView {
    /// Determine which panel is at the given screen position
    pub fn panel_at_position(&self, x: u16, y: u16, area: Rect) -> Option<ChatPanel> {
        let warning_height: u16 = if self.provider.is_none() { 1 } else { 0 };
        let input_height = self.calculate_input_height(area.width);
        let (_, conversation, activity, input, _) =
            compute_panel_areas(area, warning_height, input_height);

        // Check each panel (order matters for overlapping edges)
        if point_in_rect(x, y, conversation) {
            Some(ChatPanel::Conversation)
        } else if point_in_rect(x, y, activity) {
            Some(ChatPanel::Activity)
        } else if point_in_rect(x, y, input) {
            Some(ChatPanel::Input)
        } else {
            None
        }
    }

    /// Handle mouse event for Chat view
    /// Returns true if the event was handled
    pub fn handle_mouse(&mut self, kind: MouseEventKind, x: u16, y: u16, area: Rect) -> bool {
        match kind {
            // Left click - start text selection (no panel focus change)
            MouseEventKind::Down(MouseButton::Left) => {
                // Check if click is in conversation area (for text selection)
                if let Some(pos) = self.screen_to_selection_pos(x, y) {
                    // Start a new selection
                    self.text_selection = Some(ChatMessageSelection::new(pos));
                    self.is_selecting = true;
                    true
                } else {
                    // Clear any existing selection when clicking elsewhere
                    // No panel focus change on click - use Tab only
                    self.text_selection = None;
                    self.is_selecting = false;
                    self.panel_at_position(x, y, area).is_some() // Return true if within panels
                }
            }
            // Mouse drag - extend selection
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.is_selecting {
                    if let Some(pos) = self.screen_to_selection_pos(x, y) {
                        if let Some(ref mut selection) = self.text_selection {
                            selection.end = pos;
                        }
                    }
                    true
                } else {
                    false
                }
            }
            // Mouse release - finalize selection
            MouseEventKind::Up(MouseButton::Left) => {
                if self.is_selecting {
                    self.is_selecting = false;
                    // Keep selection visible (it will be cleared on next click)
                    // If selection is empty (same start/end), clear it
                    if let Some(ref selection) = self.text_selection {
                        if selection.start == selection.end {
                            self.text_selection = None;
                        }
                    }
                    true
                } else {
                    false
                }
            }
            // Scroll wheel up - smooth scroll focused panel (no panel switch)
            MouseEventKind::ScrollUp => {
                self.smooth_scroll(-1); // Negative = scroll up (content moves down)
                true
            }
            // Scroll wheel down - smooth scroll focused panel (no panel switch)
            MouseEventKind::ScrollDown => {
                self.smooth_scroll(1); // Positive = scroll down (content moves up)
                true
            }
            _ => false,
        }
    }

    /// Convert screen coordinates to selection position
    /// Returns None if not within a message text area
    pub(super) fn screen_to_selection_pos(&self, x: u16, y: u16) -> Option<ChatSelectionPos> {
        // Find which line is at this Y coordinate
        for line_pos in &self.line_positions {
            if line_pos.screen_y == y && x >= line_pos.start_x {
                // Calculate character offset within the line
                let x_offset = (x - line_pos.start_x) as usize;
                let char_offset = x_offset.min(line_pos.text.len());

                // Calculate total char offset in the message
                // This is simplified - we sum up characters from all previous lines in this message
                let mut total_offset = char_offset;
                for prev in &self.line_positions {
                    if prev.message_index == line_pos.message_index
                        && prev.line_in_message < line_pos.line_in_message
                    {
                        total_offset += prev.text.len();
                    }
                }

                return Some(ChatSelectionPos {
                    message_index: line_pos.message_index,
                    char_offset: total_offset,
                });
            }
        }
        None
    }
}
