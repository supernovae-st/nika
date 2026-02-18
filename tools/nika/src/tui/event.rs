//! TUI Event Handling
//!
//! Manages keyboard and terminal events.

use std::time::Duration;

use crossterm::event::{Event as CrosstermEvent, KeyEvent, MouseEvent};

/// TUI events that the application handles
#[derive(Debug, Clone, Copy)]
pub enum Event {
    /// Terminal tick for periodic updates
    Tick,
    /// Keyboard input
    Key(KeyEvent),
    /// Mouse input
    Mouse(MouseEvent),
    /// Terminal resize
    Resize(u16, u16),
}

/// Event handler configuration
pub struct EventHandler {
    /// Tick rate for periodic updates
    tick_rate: Duration,
}

impl EventHandler {
    /// Create a new event handler with the given tick rate
    pub fn new(tick_rate: Duration) -> Self {
        Self { tick_rate }
    }

    /// Get the tick rate
    pub fn tick_rate(&self) -> Duration {
        self.tick_rate
    }
}

impl Default for EventHandler {
    fn default() -> Self {
        Self::new(Duration::from_millis(250))
    }
}
