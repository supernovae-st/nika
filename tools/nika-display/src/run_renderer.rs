//! Renderer factory functions — auto-detect or force a specific display backend.
//!
//! Replaces the old `RunRenderer` enum with `Box<dyn Renderer>` factory functions,
//! enabling open extension (e.g. TestRenderer) without modifying this module.

use std::io::IsTerminal;

use crate::{CliRenderer, DetailLevel, Renderer};

use super::live::LiveRenderer;

/// Auto-detect the best renderer for the current terminal.
///
/// Uses LiveRenderer when:
/// - stderr is a TTY (interactive terminal)
/// - Detail level is not JSON or Min
/// - `NIKA_NO_LIVE` env var is not set
///
/// Falls back to CliRenderer otherwise.
pub fn auto_renderer(detail: DetailLevel) -> Box<dyn Renderer + Send> {
    let is_tty = std::io::stderr().is_terminal();
    let no_live = std::env::var("NIKA_NO_LIVE").is_ok();

    if is_tty && !no_live && !detail.is_json() && detail != DetailLevel::Min {
        Box::new(LiveRenderer::new(detail))
    } else {
        Box::new(CliRenderer::new(detail))
    }
}

/// Force classic (append-only) renderer.
pub fn classic_renderer(detail: DetailLevel) -> Box<dyn Renderer + Send> {
    Box::new(CliRenderer::new(detail))
}

/// Force live (animated) renderer.
pub fn live_renderer(detail: DetailLevel) -> Box<dyn Renderer + Send> {
    Box::new(LiveRenderer::new(detail))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn test_classic_renderer() {
        let renderer = classic_renderer(DetailLevel::Max);
        assert_eq!(renderer.last_rendered_id(), None);
    }

    #[test]
    fn test_live_renderer() {
        let renderer = live_renderer(DetailLevel::Max);
        assert_eq!(renderer.last_rendered_id(), None);
    }

    #[test]
    fn test_json_forces_classic() {
        // JSON mode always uses classic (even if TTY is available)
        let _renderer = auto_renderer(DetailLevel::Json);
        // Can't easily test TTY detection in unit tests, but at minimum this doesn't panic
    }

    #[test]
    fn test_min_forces_classic() {
        let _renderer = auto_renderer(DetailLevel::Min);
    }

    #[test]
    fn test_init_tasks_on_classic_is_noop() {
        let mut renderer = classic_renderer(DetailLevel::Max);
        let task_ids = vec!["a".to_string(), "b".to_string()];
        let deps = HashMap::new();
        renderer.init_tasks(&task_ids, &deps); // no panic (default no-op)
    }
}
