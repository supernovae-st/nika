// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Schedule configuration — the `schedule:` section at workflow root.
//!
//! Controls when and how a workflow should be triggered automatically
//! by the daemon (cron expressions, intervals, webhooks).

use std::fmt;

use serde::{Deserialize, Serialize};

/// Schedule configuration for automated workflow execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ScheduleConfig {
    /// Cron expression (5-field or 6-field).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cron: Option<String>,
    /// Fixed interval in seconds between runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_secs: Option<u64>,
    /// Whether the schedule is currently enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Timezone for cron interpretation (IANA, e.g. "Europe/Paris").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    /// What to do if the previous run is still active.
    #[serde(default)]
    pub overlap: OverlapPolicy,
}

fn default_true() -> bool {
    true
}

impl ScheduleConfig {
    /// Create a schedule with a cron expression.
    #[must_use]
    pub fn cron(expr: impl Into<String>) -> Self {
        Self {
            cron: Some(expr.into()),
            interval_secs: None,
            enabled: true,
            timezone: None,
            overlap: OverlapPolicy::default(),
        }
    }

    /// Create a schedule with a fixed interval.
    #[must_use]
    pub fn interval(secs: u64) -> Self {
        Self {
            cron: None,
            interval_secs: Some(secs),
            enabled: true,
            timezone: None,
            overlap: OverlapPolicy::default(),
        }
    }

    /// Create a new empty (disabled) schedule.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cron: None,
            interval_secs: None,
            enabled: true,
            timezone: None,
            overlap: OverlapPolicy::default(),
        }
    }
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Policy for handling overlapping scheduled runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum OverlapPolicy {
    /// Skip this trigger if previous run is still active.
    #[default]
    Skip,
    /// Queue this run to start after the previous finishes.
    Queue,
    /// Cancel the previous run and start a new one.
    Replace,
}

impl fmt::Display for OverlapPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Skip => write!(f, "skip"),
            Self::Queue => write!(f, "queue"),
            Self::Replace => write!(f, "replace"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cron_constructor() {
        let s = ScheduleConfig::cron("0 */5 * * *");
        assert_eq!(s.cron.as_deref(), Some("0 */5 * * *"));
        assert!(s.interval_secs.is_none());
        assert!(s.enabled);
    }

    #[test]
    fn interval_constructor() {
        let s = ScheduleConfig::interval(300);
        assert!(s.cron.is_none());
        assert_eq!(s.interval_secs, Some(300));
    }

    #[test]
    fn overlap_policy_default_is_skip() {
        assert_eq!(OverlapPolicy::default(), OverlapPolicy::Skip);
    }

    #[test]
    fn serde_roundtrip() {
        let s = ScheduleConfig {
            cron: Some("0 0 * * 1".into()),
            timezone: Some("Europe/Paris".into()),
            overlap: OverlapPolicy::Replace,
            ..ScheduleConfig::new()
        };
        let json = serde_json::to_string(&s).expect("serialize");
        let back: ScheduleConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.cron.as_deref(), Some("0 0 * * 1"));
        assert_eq!(back.timezone.as_deref(), Some("Europe/Paris"));
        assert_eq!(back.overlap, OverlapPolicy::Replace);
    }

    #[test]
    fn overlap_display() {
        assert_eq!(OverlapPolicy::Queue.to_string(), "queue");
    }
}
