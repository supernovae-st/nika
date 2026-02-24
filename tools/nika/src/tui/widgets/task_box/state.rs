//! Task Box State Management
//!
//! Defines the lifecycle states for task boxes: Queued, Running, Success, Failed, Skipped.

use std::time::Instant;

use ratatui::style::Color;

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
            Self::Queued => "⚪",
            Self::Running { frame, .. } => {
                let chars = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
                chars[*frame % chars.len()]
            }
            Self::Success { .. } => "✅",
            Self::Failed { .. } => "❌",
            Self::Skipped { .. } => "⏭️",
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
    pub fn suffix(&self) -> String {
        match self {
            Self::Queued => "Waiting...".to_string(),
            Self::Running { start, .. } => {
                let elapsed = start.elapsed().as_secs_f64();
                format!("{:.1}s", elapsed)
            }
            Self::Success { duration_ms } => {
                format!("{:.1}s", *duration_ms as f64 / 1000.0)
            }
            Self::Failed { error, .. } => error.clone(),
            Self::Skipped { reason } => reason.clone(),
        }
    }

    /// Get the border color for this state
    pub fn border_color(&self, verb_color: Color) -> Color {
        match self {
            Self::Queued => Color::Rgb(100, 116, 139), // Slate 500
            Self::Running { .. } => verb_color,
            Self::Success { .. } => Color::Rgb(34, 197, 94), // Green 500
            Self::Failed { .. } => Color::Rgb(239, 68, 68),  // Red 500
            Self::Skipped { .. } => Color::Rgb(148, 163, 184), // Slate 400
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
mod tests {
    use super::*;

    #[test]
    fn test_default_state() {
        let state = BoxState::default();
        assert!(matches!(state, BoxState::Queued));
    }

    #[test]
    fn test_state_icons() {
        assert_eq!(BoxState::Queued.icon(), "⚪");
        assert_eq!(BoxState::success(1000).icon(), "✅");
        assert_eq!(BoxState::failed("error", 500).icon(), "❌");
        assert_eq!(BoxState::skipped("dep failed").icon(), "⏭️");
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
}
