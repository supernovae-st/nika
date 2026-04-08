// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Scroll and Panel Navigation for Chat View
//!
//! Contains panel focus management, scroll control, and smooth scrolling.

use super::{ChatPanel, ChatView, PanelScrollState};

// ═══════════════════════════════════════════════════════════════════════════════
// Panel Navigation
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Focus next panel (Tab key)
    pub fn focus_next_panel(&mut self) {
        self.focused_panel = self.focused_panel.next();
    }

    /// Focus previous panel (Shift+Tab)
    pub fn focus_prev_panel(&mut self) {
        self.focused_panel = self.focused_panel.prev();
    }

    /// Focus a specific panel (for mouse clicks)
    pub fn focus_panel(&mut self, panel: ChatPanel) {
        self.focused_panel = panel;
    }

    /// Get the scroll state for the currently focused panel (mutable)
    pub fn focused_scroll_mut(&mut self) -> Option<&mut PanelScrollState> {
        match self.focused_panel {
            ChatPanel::Conversation => Some(&mut self.conversation_scroll),
            ChatPanel::Activity => Some(&mut self.activity_scroll),
            ChatPanel::Input => None, // Input panel doesn't scroll
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scroll Control
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Scroll down by one item
    pub fn scroll_down(&mut self) {
        // Don't update total here - add_message() and render() handle it
        // This lets tests configure scroll state manually
        match self.focused_panel {
            ChatPanel::Input | ChatPanel::Conversation => {
                self.conversation_scroll.scroll_down();
                // Check if we reached the bottom
                self.user_at_bottom = self.is_at_bottom();
            }
            ChatPanel::Activity => {
                self.activity_scroll.scroll_down();
            }
        }
    }

    /// Scroll up by one item
    pub fn scroll_up(&mut self) {
        // Don't update total here - add_message() and render() handle it
        match self.focused_panel {
            ChatPanel::Input | ChatPanel::Conversation => {
                self.conversation_scroll.scroll_up();
                // User scrolled up = stop auto-following
                self.user_at_bottom = false;
            }
            ChatPanel::Activity => {
                self.activity_scroll.scroll_up();
            }
        }
    }

    /// Scroll to top
    pub fn scroll_to_top(&mut self) {
        match self.focused_panel {
            ChatPanel::Input | ChatPanel::Conversation => {
                self.conversation_scroll.scroll_to_top();
                // Went to top = stop auto-following
                self.user_at_bottom = false;
            }
            ChatPanel::Activity => {
                self.activity_scroll.scroll_to_top();
            }
        }
    }

    /// Scroll to bottom
    pub fn scroll_to_bottom(&mut self) {
        match self.focused_panel {
            ChatPanel::Input | ChatPanel::Conversation => {
                self.conversation_scroll.scroll_to_bottom();
                // Went to bottom = resume auto-following
                self.user_at_bottom = true;
            }
            ChatPanel::Activity => {
                self.activity_scroll.scroll_to_bottom();
            }
        }
    }

    /// Page down
    pub fn page_down(&mut self) {
        match self.focused_panel {
            ChatPanel::Input | ChatPanel::Conversation => {
                self.conversation_scroll.page_down();
                // Check if we reached the bottom
                self.user_at_bottom = self.is_at_bottom();
            }
            ChatPanel::Activity => {
                self.activity_scroll.page_down();
            }
        }
    }

    /// Page up
    pub fn page_up(&mut self) {
        match self.focused_panel {
            ChatPanel::Input | ChatPanel::Conversation => {
                self.conversation_scroll.page_up();
                // User scrolled up = stop auto-following
                self.user_at_bottom = false;
            }
            ChatPanel::Activity => {
                self.activity_scroll.page_up();
            }
        }
    }

    /// Check if conversation is scrolled to the bottom
    pub(super) fn is_at_bottom(&self) -> bool {
        let scroll = &self.conversation_scroll;
        if scroll.total == 0 || scroll.visible == 0 {
            return true; // Empty or not rendered yet = consider at bottom
        }
        // At bottom when offset + visible >= total
        scroll.offset + scroll.visible >= scroll.total
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Smooth Scrolling
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Friction coefficient for scroll deceleration (0.0 = instant stop, 1.0 = no friction)
    const SCROLL_FRICTION: f32 = 0.85;

    /// Minimum velocity before stopping animation (lines per tick)
    const SCROLL_MIN_VELOCITY: f32 = 0.1;

    /// Initial velocity multiplier for mouse wheel (lines per scroll event)
    const SCROLL_WHEEL_VELOCITY: f32 = 3.0;

    /// Apply smooth scroll with initial velocity (for mouse wheel)
    pub fn smooth_scroll(&mut self, direction: i8) {
        // Add velocity in the scroll direction (positive = down, negative = up)
        self.scroll_velocity += direction as f32 * Self::SCROLL_WHEEL_VELOCITY;
        self.scroll_animating = true;

        // Update user_at_bottom state based on scroll direction
        if direction < 0 {
            self.user_at_bottom = false; // Scrolling up = stop auto-following
        }
    }

    /// Update scroll animation (call this every frame/tick)
    /// Returns true if animation is still active
    pub fn update_scroll_animation(&mut self) -> bool {
        if !self.scroll_animating {
            return false;
        }

        // Apply velocity to accumulator (sub-line precision)
        self.scroll_accumulator += self.scroll_velocity;

        // Convert accumulated scroll to whole lines
        let lines_to_scroll = self.scroll_accumulator.trunc() as i32;
        if lines_to_scroll != 0 {
            self.scroll_accumulator -= lines_to_scroll as f32;

            // Apply scroll based on direction
            if lines_to_scroll > 0 {
                for _ in 0..lines_to_scroll {
                    self.conversation_scroll.scroll_down();
                }
            } else {
                for _ in 0..(-lines_to_scroll) {
                    self.conversation_scroll.scroll_up();
                }
            }
        }

        // Apply friction
        self.scroll_velocity *= Self::SCROLL_FRICTION;

        // Stop animation if velocity is negligible
        if self.scroll_velocity.abs() < Self::SCROLL_MIN_VELOCITY {
            self.scroll_velocity = 0.0;
            self.scroll_accumulator = 0.0;
            self.scroll_animating = false;

            // Check if we ended up at the bottom
            self.user_at_bottom = self.is_at_bottom();
            return false;
        }

        true
    }

    /// Stop any ongoing smooth scroll animation
    pub fn stop_smooth_scroll(&mut self) {
        self.scroll_velocity = 0.0;
        self.scroll_accumulator = 0.0;
        self.scroll_animating = false;
    }

    /// Check if smooth scroll animation is active
    pub fn is_scroll_animating(&self) -> bool {
        self.scroll_animating
    }
}
