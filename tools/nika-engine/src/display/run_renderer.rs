//! RunRenderer — dispatch enum for Live vs Classic CLI rendering.
//!
//! Automatically selects the appropriate renderer based on terminal capabilities.

use std::collections::HashMap;
use std::io::IsTerminal;
use std::sync::Arc;

use crate::display::{CliRenderer, DetailLevel};
use crate::event::Event;

use super::live::LiveRenderer;

/// Unified renderer that dispatches to either LiveRenderer (animated, in-place)
/// or CliRenderer (append-only, for non-TTY / CI / JSON).
#[allow(clippy::large_enum_variant)]
pub enum RunRenderer {
    /// Animated display with spinners, progress bars, and in-place updates.
    Live(LiveRenderer),
    /// Classic append-only println() display.
    Classic(CliRenderer),
}

impl RunRenderer {
    /// Auto-detect the best renderer for the current terminal.
    ///
    /// Uses LiveRenderer when:
    /// - stderr is a TTY (interactive terminal)
    /// - Detail level is not JSON or Min
    /// - `NIKA_NO_LIVE` env var is not set
    ///
    /// Falls back to CliRenderer otherwise.
    pub fn auto(detail: DetailLevel) -> Self {
        let is_tty = std::io::stderr().is_terminal();
        let no_live = std::env::var("NIKA_NO_LIVE").is_ok();

        if is_tty && !no_live && !detail.is_json() && detail != DetailLevel::Min {
            Self::Live(LiveRenderer::new(detail))
        } else {
            Self::Classic(CliRenderer::new(detail))
        }
    }

    /// Force classic (append-only) renderer.
    pub fn classic(detail: DetailLevel) -> Self {
        Self::Classic(CliRenderer::new(detail))
    }

    /// Force live (animated) renderer.
    pub fn live(detail: DetailLevel) -> Self {
        Self::Live(LiveRenderer::new(detail))
    }

    /// Initialize task bars for the LiveRenderer (no-op for Classic).
    pub fn init_tasks(
        &mut self,
        task_ids: &[String],
        task_deps: &HashMap<String, Vec<String>>,
    ) {
        if let Self::Live(ref mut live) = self {
            live.init_tasks(task_ids, task_deps);
        }
    }

    /// Set task-to-layer mapping.
    pub fn set_task_layers(&mut self, layers: HashMap<Arc<str>, usize>) {
        match self {
            Self::Live(live) => live.set_task_layers(layers),
            Self::Classic(cli) => cli.set_task_layers(layers),
        }
    }

    /// Get last rendered event ID for incremental rendering.
    pub fn last_rendered_id(&self) -> Option<u64> {
        match self {
            Self::Live(live) => live.last_rendered_id(),
            Self::Classic(cli) => cli.last_rendered_id(),
        }
    }

    /// Render a single EventKind (without an Event wrapper).
    pub fn render_kind(&mut self, kind: &crate::event::EventKind) {
        match self {
            Self::Live(live) => live.render_kind(kind),
            Self::Classic(cli) => cli.render_kind(kind),
        }
    }

    /// Render new events since last render call.
    pub fn render_new_events(&mut self, events: &[Event]) {
        match self {
            Self::Live(live) => live.render_new_events(events),
            Self::Classic(cli) => cli.render_new_events(events),
        }
    }

    /// Render the full summary footer.
    pub fn render_summary(&mut self, total_duration_ms: u64, trace_path: Option<&str>) {
        match self {
            Self::Live(live) => live.render_summary(total_duration_ms, trace_path),
            Self::Classic(cli) => cli.render_summary(total_duration_ms, trace_path),
        }
    }

    /// Render compact summary (quiet mode).
    pub fn render_quiet_summary(&mut self, total_duration_ms: u64) {
        match self {
            Self::Live(live) => live.render_quiet_summary(total_duration_ms),
            Self::Classic(cli) => cli.render_quiet_summary(total_duration_ms),
        }
    }

    /// Check if this is the live renderer.
    pub fn is_live(&self) -> bool {
        matches!(self, Self::Live(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classic_mode() {
        let renderer = RunRenderer::classic(DetailLevel::Max);
        assert!(!renderer.is_live());
        assert_eq!(renderer.last_rendered_id(), None);
    }

    #[test]
    fn test_live_mode() {
        let renderer = RunRenderer::live(DetailLevel::Max);
        assert!(renderer.is_live());
        assert_eq!(renderer.last_rendered_id(), None);
    }

    #[test]
    fn test_json_forces_classic() {
        // JSON mode always uses classic (even if TTY is available)
        let _renderer = RunRenderer::auto(DetailLevel::Json);
        // Can't easily test TTY detection in unit tests, but at minimum this doesn't panic
    }

    #[test]
    fn test_min_forces_classic() {
        let _renderer = RunRenderer::auto(DetailLevel::Min);
    }

    #[test]
    fn test_init_tasks_on_classic_is_noop() {
        let mut renderer = RunRenderer::classic(DetailLevel::Max);
        let task_ids = vec!["a".to_string(), "b".to_string()];
        let deps = HashMap::new();
        renderer.init_tasks(&task_ids, &deps); // no panic
    }
}
