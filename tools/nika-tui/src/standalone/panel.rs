// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Panel navigation for standalone TUI mode.

/// Panel in standalone mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StandalonePanel {
    #[default]
    Browser,
    History,
    Preview,
}

impl StandalonePanel {
    pub fn next(&self) -> Self {
        match self {
            StandalonePanel::Browser => StandalonePanel::History,
            StandalonePanel::History => StandalonePanel::Preview,
            StandalonePanel::Preview => StandalonePanel::Browser,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            StandalonePanel::Browser => StandalonePanel::Preview,
            StandalonePanel::History => StandalonePanel::Browser,
            StandalonePanel::Preview => StandalonePanel::History,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            StandalonePanel::Browser => "WORKFLOW BROWSER",
            StandalonePanel::History => "HISTORY",
            StandalonePanel::Preview => "PREVIEW",
        }
    }

    pub fn number(&self) -> u8 {
        match self {
            StandalonePanel::Browser => 1,
            StandalonePanel::History => 2,
            StandalonePanel::Preview => 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standalone_panel_cycle() {
        let panel = StandalonePanel::Browser;
        assert_eq!(panel.next(), StandalonePanel::History);
        assert_eq!(panel.next().next(), StandalonePanel::Preview);
        assert_eq!(panel.next().next().next(), StandalonePanel::Browser);
    }

    #[test]
    fn test_standalone_panel_prev() {
        let panel = StandalonePanel::Browser;
        assert_eq!(panel.prev(), StandalonePanel::Preview);
        assert_eq!(panel.prev().prev(), StandalonePanel::History);
        assert_eq!(panel.prev().prev().prev(), StandalonePanel::Browser);
    }

    #[test]
    fn test_standalone_panel_title() {
        assert_eq!(StandalonePanel::Browser.title(), "WORKFLOW BROWSER");
        assert_eq!(StandalonePanel::History.title(), "HISTORY");
        assert_eq!(StandalonePanel::Preview.title(), "PREVIEW");
    }

    #[test]
    fn test_standalone_panel_number() {
        assert_eq!(StandalonePanel::Browser.number(), 1);
        assert_eq!(StandalonePanel::History.number(), 2);
        assert_eq!(StandalonePanel::Preview.number(), 3);
    }

    #[test]
    fn test_standalone_panel_default() {
        let panel = StandalonePanel::default();
        assert_eq!(panel, StandalonePanel::Browser);
    }

    #[test]
    fn test_standalone_panel_equality() {
        let browser1 = StandalonePanel::Browser;
        let browser2 = StandalonePanel::Browser;
        let history = StandalonePanel::History;
        assert_eq!(browser1, browser2);
        assert_ne!(browser1, history);
    }
}
