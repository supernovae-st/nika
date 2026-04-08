// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Task Box State Management
//!
//! Defines the lifecycle states for task boxes: Queued, Running, Success, Failed, Skipped.

use std::borrow::Cow;
use std::time::Instant;

use ratatui::style::Color;

use crate::icons;
use crate::theme::Theme;

/// Braille spinner frames for running state animation
pub const BRAILLE_SPINNER: &[char] = &['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'];

/// State of a task box
#[derive(Debug, Clone, Default)]
pub enum BoxState {
    /// Task is waiting to execute
    #[default]
    Queued,
    /// Task is currently executing
    Running {
        /// When execution started
        start: Instant,
        /// Animation frame (0-7 for spinner)
        frame: usize,
    },
    /// Task completed successfully
    Success {
        /// Execution duration in milliseconds
        duration_ms: u64,
    },
    /// Task failed with error
    Failed {
        /// Error message
        error: String,
        /// Execution duration in milliseconds
        duration_ms: u64,
    },
    /// Task was skipped (dependency failed)
    Skipped {
        /// Reason for skipping
        reason: String,
    },
}

impl BoxState {
    /// Create a new Running state
    pub fn running() -> Self {
        Self::Running {
            start: Instant::now(),
            frame: 0,
        }
    }

    /// Create a new Success state
    pub fn success(duration_ms: u64) -> Self {
        Self::Success { duration_ms }
    }

    /// Create a new Failed state
    pub fn failed(error: impl Into<String>, duration_ms: u64) -> Self {
        Self::Failed {
            error: error.into(),
            duration_ms,
        }
    }

    /// Create a new Skipped state
    pub fn skipped(reason: impl Into<String>) -> Self {
        Self::Skipped {
            reason: reason.into(),
        }
    }

    /// Get the status icon for this state
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Queued => icons::status::PENDING,
            Self::Running { frame, .. } => {
                // Static str array for icon display (BRAILLE_SPINNER is char array)
                const SPINNER_STR: &[&str] = &["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
                SPINNER_STR[*frame % SPINNER_STR.len()]
            }
            Self::Success { .. } => icons::status::SUCCESS,
            Self::Failed { .. } => icons::status::FAILED,
            Self::Skipped { .. } => icons::status::SKIPPED,
        }
    }

    /// Get the spinner character for running state
    pub fn spinner_char(&self) -> Option<char> {
        if let Self::Running { frame, .. } = self {
            Some(BRAILLE_SPINNER[*frame % BRAILLE_SPINNER.len()])
        } else {
            None
        }
    }

    /// Get the status suffix text (duration or message)
    ///
    /// Returns `Cow<'_, str>` to avoid allocation when possible:
    /// - `Queued` → borrows static string (zero allocation)
    /// - `Failed/Skipped` → borrows owned error/reason string (zero allocation)
    /// - `Running/Success` → allocates formatted duration string
    pub fn suffix(&self) -> Cow<'_, str> {
        match self {
            Self::Queued => Cow::Borrowed("Waiting..."),
            Self::Running { start, .. } => {
                let elapsed = start.elapsed().as_secs_f64();
                Cow::Owned(format!("{:.1}s", elapsed))
            }
            Self::Success { duration_ms } => {
                Cow::Owned(format!("{:.1}s", *duration_ms as f64 / 1000.0))
            }
            // PERF: Borrow owned strings instead of cloning
            Self::Failed { error, .. } => Cow::Borrowed(error.as_str()),
            Self::Skipped { reason } => Cow::Borrowed(reason.as_str()),
        }
    }

    /// Get the border color for this state (hardcoded fallback).
    ///
    /// **Deprecated**: Use `border_color_themed()` for theme-aware colors.
    #[deprecated(note = "Use border_color_themed() for theme-aware colors")]
    pub fn border_color(&self, verb_color: Color) -> Color {
        match self {
            Self::Queued => Color::Rgb(100, 116, 139), // Slate 500
            Self::Running { .. } => verb_color,
            Self::Success { .. } => Color::Rgb(34, 197, 94), // Green 500
            Self::Failed { .. } => Color::Rgb(239, 68, 68),  // Red 500
            Self::Skipped { .. } => Color::Rgb(148, 163, 184), // Slate 400
        }
    }

    /// Get the border color for this state using the theme.
    ///
    /// Status colors come from the theme's semantic palette.
    pub fn border_color_themed(&self, verb_color: Color, theme: &Theme) -> Color {
        match self {
            Self::Queued => theme.text_muted,
            Self::Running { .. } => verb_color,
            Self::Success { .. } => theme.status_success,
            Self::Failed { .. } => theme.status_failed,
            Self::Skipped { .. } => theme.text_muted,
        }
    }

    /// Get border color with pulse effect applied
    ///
    /// Only applies pulse to Running state; other states return normal border_color.
    /// Pulse brightens the color by interpolating toward white.
    pub fn border_color_with_pulse(&self, verb_color: Color, pulse_intensity: f32) -> Color {
        #[allow(deprecated)]
        let base = self.border_color(verb_color);

        // Only pulse when running
        if !self.is_running() {
            return base;
        }

        // Brighten color based on pulse_intensity (0.0-1.0)
        if let Color::Rgb(r, g, b) = base {
            let factor = 1.0 + (pulse_intensity * 0.3); // Max 30% brighter
            let brighten = |c: u8| -> u8 { ((c as f32 * factor).min(255.0)) as u8 };
            Color::Rgb(brighten(r), brighten(g), brighten(b))
        } else {
            base
        }
    }

    /// Get border color with pulse effect applied (theme-aware).
    pub fn border_color_with_pulse_themed(
        &self,
        verb_color: Color,
        pulse_intensity: f32,
        theme: &Theme,
    ) -> Color {
        let base = self.border_color_themed(verb_color, theme);
        if !self.is_running() {
            return base;
        }
        if let Color::Rgb(r, g, b) = base {
            let factor = 1.0 + (pulse_intensity * 0.3);
            let brighten = |c: u8| -> u8 { ((c as f32 * factor).min(255.0)) as u8 };
            Color::Rgb(brighten(r), brighten(g), brighten(b))
        } else {
            base
        }
    }

    /// Check if state is terminal (no more updates expected)
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Success { .. } | Self::Failed { .. } | Self::Skipped { .. }
        )
    }

    /// Check if state is running
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    /// Advance the animation frame (for running state)
    pub fn tick(&mut self) {
        if let Self::Running { frame, .. } = self {
            *frame = (*frame + 1) % BRAILLE_SPINNER.len();
        }
    }

    /// Get duration if available
    pub fn duration_ms(&self) -> Option<u64> {
        match self {
            Self::Running { start, .. } => Some(start.elapsed().as_millis() as u64),
            Self::Success { duration_ms } | Self::Failed { duration_ms, .. } => Some(*duration_ms),
            _ => None,
        }
    }
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state() {
        let state = BoxState::default();
        assert!(matches!(state, BoxState::Queued));
    }

    #[test]
    fn test_state_icons() {
        assert_eq!(BoxState::Queued.icon(), icons::status::PENDING);
        assert_eq!(BoxState::success(1000).icon(), icons::status::SUCCESS);
        assert_eq!(BoxState::failed("error", 500).icon(), icons::status::FAILED);
        assert_eq!(
            BoxState::skipped("dep failed").icon(),
            icons::status::SKIPPED
        );
    }

    #[test]
    fn test_running_spinner() {
        let mut state = BoxState::running();
        assert!(state.is_running());

        // Test spinner changes on tick
        let char1 = state.spinner_char();
        state.tick();
        let char2 = state.spinner_char();

        assert!(char1.is_some());
        assert!(char2.is_some());
        // After tick, character changes
        assert_ne!(char1, char2);
    }

    #[test]
    fn test_state_suffix() {
        assert_eq!(BoxState::Queued.suffix(), "Waiting...");
        assert_eq!(BoxState::success(1500).suffix(), "1.5s");
        assert_eq!(
            BoxState::failed("Connection refused", 200).suffix(),
            "Connection refused"
        );
    }

    #[test]
    fn test_is_terminal() {
        assert!(!BoxState::Queued.is_terminal());
        assert!(!BoxState::running().is_terminal());
        assert!(BoxState::success(100).is_terminal());
        assert!(BoxState::failed("err", 100).is_terminal());
        assert!(BoxState::skipped("dep").is_terminal());
    }

    #[test]
    fn test_border_color() {
        let verb_color = Color::Rgb(139, 92, 246); // Violet

        // Running uses verb color
        let running = BoxState::running();
        assert_eq!(running.border_color(verb_color), verb_color);

        // Success uses green
        let success = BoxState::success(100);
        assert_eq!(success.border_color(verb_color), Color::Rgb(34, 197, 94));

        // Failed uses red
        let failed = BoxState::failed("err", 100);
        assert_eq!(failed.border_color(verb_color), Color::Rgb(239, 68, 68));
    }

    #[test]
    fn test_duration_ms() {
        assert!(BoxState::Queued.duration_ms().is_none());
        assert_eq!(BoxState::success(1234).duration_ms(), Some(1234));
        assert_eq!(BoxState::failed("err", 567).duration_ms(), Some(567));

        // Running returns elapsed time
        let running = BoxState::running();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let duration = running.duration_ms();
        assert!(duration.is_some());
        assert!(duration.unwrap() >= 10);
    }

    #[test]
    fn test_tick_only_affects_running() {
        let mut queued = BoxState::Queued;
        queued.tick(); // Should not panic

        let mut success = BoxState::success(100);
        success.tick(); // Should not panic

        let mut running = BoxState::running();
        if let BoxState::Running { frame, .. } = &running {
            assert_eq!(*frame, 0);
        }
        running.tick();
        if let BoxState::Running { frame, .. } = &running {
            assert_eq!(*frame, 1);
        }
    }

    #[test]
    fn test_border_color_with_pulse() {
        let verb_color = Color::Rgb(139, 92, 246); // Violet

        // Running state with pulse_intensity 0.0 → base color
        let running = BoxState::running();
        let base_color = running.border_color(verb_color);
        assert_eq!(base_color, verb_color);

        // With pulse_intensity 0.5 → brightened color
        let pulsed = running.border_color_with_pulse(verb_color, 0.5);
        if let Color::Rgb(r, g, b) = pulsed {
            // Should be brighter than base (all components >= original)
            assert!(r >= 139 && g >= 92 && b >= 246);
        }

        // With pulse_intensity 1.0 → maximum brightness
        let max_pulsed = running.border_color_with_pulse(verb_color, 1.0);
        if let Color::Rgb(r, g, b) = max_pulsed {
            // Should be significantly brighter (at least one component > original)
            assert!(r > 139 || g > 92 || b > 246);
        }
    }

    #[test]
    fn test_border_color_pulse_non_running_unchanged() {
        let verb_color = Color::Rgb(139, 92, 246);

        // Queued state ignores pulse
        let queued = BoxState::Queued;
        let color = queued.border_color_with_pulse(verb_color, 1.0);
        assert_eq!(color, queued.border_color(verb_color)); // Unchanged

        // Success state ignores pulse
        let success = BoxState::success(1000);
        let color = success.border_color_with_pulse(verb_color, 1.0);
        assert_eq!(color, success.border_color(verb_color)); // Unchanged
    }

    // ═══ THEMED BORDER COLOR TESTS ═══

    #[test]
    fn test_border_color_themed_uses_theme_values() {
        let theme = Theme::dark();
        let verb_color = Color::Rgb(139, 92, 246);
        assert_eq!(
            BoxState::Queued.border_color_themed(verb_color, &theme),
            theme.text_muted
        );
        assert_eq!(
            BoxState::running().border_color_themed(verb_color, &theme),
            verb_color
        );
        assert_eq!(
            BoxState::success(100).border_color_themed(verb_color, &theme),
            theme.status_success
        );
        assert_eq!(
            BoxState::failed("err", 100).border_color_themed(verb_color, &theme),
            theme.status_failed
        );
        assert_eq!(
            BoxState::skipped("dep").border_color_themed(verb_color, &theme),
            theme.text_muted
        );
    }

    #[test]
    fn test_border_color_themed_differs_between_themes() {
        let dark = Theme::dark();
        let light = Theme::light();
        let verb_color = Color::Rgb(139, 92, 246);
        let success = BoxState::success(100);
        assert_ne!(
            success.border_color_themed(verb_color, &dark),
            success.border_color_themed(verb_color, &light),
        );
    }

    #[test]
    fn test_border_color_with_pulse_themed() {
        let theme = Theme::dark();
        let verb_color = Color::Rgb(139, 92, 246);
        let running = BoxState::running();
        let base = running.border_color_themed(verb_color, &theme);
        let pulsed = running.border_color_with_pulse_themed(verb_color, 1.0, &theme);
        assert_ne!(base, pulsed);
        let success = BoxState::success(100);
        let color = success.border_color_with_pulse_themed(verb_color, 1.0, &theme);
        assert_eq!(color, success.border_color_themed(verb_color, &theme));
    }
}
