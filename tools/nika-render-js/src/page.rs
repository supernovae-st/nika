// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Single-page render configuration + (Batch B) the render flow.
//!
//! Batch A ships [`RenderOptions`] only — a pure, dependency-free config
//! struct. Batch B adds `render_page()` which wraps every chromiumoxide op
//! in a bounded [`tokio::time::timeout`] and closes the tab on EVERY exit
//! path (chromiumoxide 0.9.1 does NOT auto-close tabs on `Page` drop).

use std::time::Duration;

/// Per-render tunables. Defaults target SPA hydration (React/Vue/Next/Nuxt).
#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// Hard cap on `Page::goto` navigation.
    pub nav_timeout: Duration,
    /// Cap on the post-load network-settle wait (`Page::wait_for_navigation`).
    pub networkidle_timeout: Duration,
    /// Optional `User-Agent` override applied before navigation.
    pub user_agent: Option<String>,
    /// Optional `Accept-Language` value (informational in v0).
    pub accept_language: Option<String>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            nav_timeout: Duration::from_secs(30),
            networkidle_timeout: Duration::from_secs(10),
            user_agent: None,
            accept_language: None,
        }
    }
}

impl RenderOptions {
    /// The configured navigation timeout in whole milliseconds, saturating.
    ///
    /// Used to populate [`crate::RenderError::NavTimeout`] when a navigation
    /// exceeds [`RenderOptions::nav_timeout`].
    #[must_use]
    pub fn nav_timeout_ms(&self) -> u64 {
        u64::try_from(self.nav_timeout.as_millis()).unwrap_or(u64::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_targets_spa_timeouts() {
        let o = RenderOptions::default();
        assert_eq!(o.nav_timeout, Duration::from_secs(30));
        assert_eq!(o.networkidle_timeout, Duration::from_secs(10));
        assert!(o.user_agent.is_none());
        assert!(o.accept_language.is_none());
    }

    #[test]
    fn timeout_config_is_independent() {
        let o = RenderOptions {
            nav_timeout: Duration::from_secs(15),
            networkidle_timeout: Duration::from_secs(3),
            user_agent: Some("nika-render/0".to_string()),
            accept_language: Some("en-US".to_string()),
        };
        assert_eq!(o.nav_timeout, Duration::from_secs(15));
        assert_eq!(o.networkidle_timeout, Duration::from_secs(3));
        assert_eq!(o.user_agent.as_deref(), Some("nika-render/0"));
        assert_eq!(o.accept_language.as_deref(), Some("en-US"));
    }

    #[test]
    fn nav_timeout_ms_converts() {
        let o = RenderOptions::default();
        assert_eq!(o.nav_timeout_ms(), 30_000);
    }
}
