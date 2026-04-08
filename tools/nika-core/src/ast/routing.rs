// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Routing configuration for provider fallback chains.
//!
//! ```yaml
//! routing:
//!   fallback: [anthropic, openai, groq]
//! ```

use serde::{Deserialize, Serialize};

/// Routing configuration for provider fallback.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RoutingConfig {
    /// Fallback provider chain (try in order).
    #[serde(default)]
    pub fallback: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_config_serde_roundtrip() {
        let config = RoutingConfig {
            fallback: vec!["anthropic".into(), "openai".into()],
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: RoutingConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn routing_config_default() {
        let config = RoutingConfig::default();
        assert!(config.fallback.is_empty());
    }
}
