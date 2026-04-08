// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Verbosity control for CLI output.

use std::fmt;
use std::str::FromStr;

/// Controls how much telemetry is displayed during `nika run`.
///
/// Levels (most -> least verbose):
/// - `max`: All events, previews, sparklines, full summary (DEFAULT)
/// - `default`: Task lifecycle + provider + MCP + errors
/// - `min`: Task checkmark/cross lines only + compact summary
/// - `json`: Raw NDJSON events to stdout
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DetailLevel {
    #[default]
    Max,
    Default,
    Min,
    Json,
}

impl DetailLevel {
    pub fn show_sub_events(&self) -> bool {
        matches!(self, Self::Max | Self::Default)
    }

    pub fn show_previews(&self) -> bool {
        matches!(self, Self::Max)
    }

    pub fn show_sparklines(&self) -> bool {
        matches!(self, Self::Max)
    }

    pub fn show_full_summary(&self) -> bool {
        matches!(self, Self::Max | Self::Default)
    }

    pub fn show_layer_separators(&self) -> bool {
        matches!(self, Self::Max)
    }

    pub fn is_json(&self) -> bool {
        matches!(self, Self::Json)
    }

    pub fn show_template_events(&self) -> bool {
        matches!(self, Self::Max)
    }
}

impl fmt::Display for DetailLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Max => write!(f, "max"),
            Self::Default => write!(f, "default"),
            Self::Min => write!(f, "min"),
            Self::Json => write!(f, "json"),
        }
    }
}

impl FromStr for DetailLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "max" => Ok(Self::Max),
            "default" => Ok(Self::Default),
            "min" => Ok(Self::Min),
            "json" => Ok(Self::Json),
            _ => Err(format!(
                "invalid detail level '{}': expected max, default, min, or json",
                s
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detail_level_from_str() {
        assert_eq!(DetailLevel::from_str("max").unwrap(), DetailLevel::Max);
        assert_eq!(
            DetailLevel::from_str("default").unwrap(),
            DetailLevel::Default
        );
        assert_eq!(DetailLevel::from_str("min").unwrap(), DetailLevel::Min);
        assert_eq!(DetailLevel::from_str("json").unwrap(), DetailLevel::Json);
        assert_eq!(DetailLevel::from_str("MAX").unwrap(), DetailLevel::Max);
        assert!(DetailLevel::from_str("invalid").is_err());
    }

    #[test]
    fn test_default_is_max() {
        assert_eq!(DetailLevel::default(), DetailLevel::Max);
    }

    #[test]
    fn test_visibility_max() {
        let d = DetailLevel::Max;
        assert!(d.show_sub_events());
        assert!(d.show_previews());
        assert!(d.show_sparklines());
        assert!(d.show_full_summary());
        assert!(d.show_layer_separators());
        assert!(d.show_template_events());
        assert!(!d.is_json());
    }

    #[test]
    fn test_visibility_default() {
        let d = DetailLevel::Default;
        assert!(d.show_sub_events());
        assert!(!d.show_previews());
        assert!(!d.show_sparklines());
        assert!(d.show_full_summary());
        assert!(!d.show_layer_separators());
        assert!(!d.show_template_events());
        assert!(!d.is_json());
    }

    #[test]
    fn test_visibility_min() {
        let d = DetailLevel::Min;
        assert!(!d.show_sub_events());
        assert!(!d.show_previews());
        assert!(!d.show_sparklines());
        assert!(!d.show_full_summary());
        assert!(!d.show_layer_separators());
        assert!(!d.show_template_events());
        assert!(!d.is_json());
    }

    #[test]
    fn test_visibility_json() {
        let d = DetailLevel::Json;
        assert!(!d.show_sub_events());
        assert!(d.is_json());
    }
}
