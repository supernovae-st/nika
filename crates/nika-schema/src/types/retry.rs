// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Retry policy — the task-level `retry:` block.
//!
//! Per spec `05-errors.md` §retry · « Retries apply to **transient** errors
//! only » · fields ·
//!
//! | Field | Required | Notes |
//! |---|---|---|
//! | `max_attempts` | yes · integer ≥ 1 | total attempts (including first try) |
//! | `backoff_ms` | no | initial backoff · default 1000 |
//! | `backoff_strategy` | no | `fixed` · `linear` · `exponential` (default) |
//! | `backoff_max_ms` | no | cap · default 60000 |
//! | `jitter` | no | default **true** (anti-thundering-herd) |
//! | `on_codes` | no | whitelist of `NIKA-<NS>-<NNN>` codes |

use std::fmt;

use serde::{Deserialize, Serialize};

/// Backoff strategy between retry attempts (spec `05-errors.md`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "lowercase")]
pub enum BackoffStrategy {
    /// `backoff_ms` between every attempt.
    Fixed,
    /// `backoff_ms * attempt` between attempts (1s · 2s · 3s · …).
    Linear,
    /// `backoff_ms * 2^(attempt-1)` · capped at `backoff_max_ms` (default).
    #[default]
    Exponential,
}

impl BackoffStrategy {
    /// Parse the YAML `backoff_strategy:` scalar (closed enum).
    #[must_use]
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "fixed" => Some(Self::Fixed),
            "linear" => Some(Self::Linear),
            "exponential" => Some(Self::Exponential),
            _ => None,
        }
    }
}

impl fmt::Display for BackoffStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fixed => write!(f, "fixed"),
            Self::Linear => write!(f, "linear"),
            Self::Exponential => write!(f, "exponential"),
        }
    }
}

/// A task's retry policy. Defaults per spec `05-errors.md` §retry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RetryConfig {
    /// Total attempts including the first try. REQUIRED · ≥ 1.
    pub max_attempts: u32,
    /// Initial backoff in milliseconds (default 1000).
    pub backoff_ms: u64,
    /// Backoff growth strategy (default `exponential`).
    pub backoff_strategy: BackoffStrategy,
    /// Backoff cap in milliseconds (default 60000 · 1 min).
    pub backoff_max_ms: u64,
    /// Randomize the computed backoff (default **true**).
    pub jitter: bool,
    /// If non-empty · only retry on these canonical `NIKA-<NS>-<NNN>`
    /// codes; else retry all transient errors.
    pub on_codes: Vec<String>,
}

impl RetryConfig {
    /// Create a retry policy with the spec defaults for every optional
    /// field (`backoff_ms` 1000 · `exponential` · `backoff_max_ms` 60000 ·
    /// `jitter` true · no code whitelist).
    #[must_use]
    pub fn new(max_attempts: u32) -> Self {
        Self {
            max_attempts,
            backoff_ms: 1_000,
            backoff_strategy: BackoffStrategy::default(),
            backoff_max_ms: 60_000,
            jitter: true,
            on_codes: Vec::new(),
        }
    }
}

/// Validate a `retry.on_codes` entry against the canonical error-code
/// regex (spec `05-errors.md` §namespaces) ·
/// `^NIKA-[A-Z]{2,9}(-[A-Z]{2,9})?-[0-9]{3}$`.
///
/// Hand-rolled (no regex dep in L0) · segments split on `-` ·
/// `NIKA` · 1-2 uppercase namespace segments of 2-9 letters · 3 digits.
#[must_use]
pub fn is_valid_error_code(code: &str) -> bool {
    let segments: Vec<&str> = code.split('-').collect();
    let (ns_segments, digits) = match segments.as_slice() {
        ["NIKA", ns, digits] => (vec![*ns], *digits),
        ["NIKA", ns1, ns2, digits] => (vec![*ns1, *ns2], *digits),
        _ => return false,
    };
    for ns in ns_segments {
        let len = ns.len();
        if !(2..=9).contains(&len) || !ns.chars().all(|c| c.is_ascii_uppercase()) {
            return false;
        }
    }
    digits.len() == 3 && digits.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec() {
        let r = RetryConfig::new(3);
        assert_eq!(r.max_attempts, 3);
        assert_eq!(r.backoff_ms, 1_000);
        assert_eq!(r.backoff_strategy, BackoffStrategy::Exponential);
        assert_eq!(r.backoff_max_ms, 60_000);
        assert!(r.jitter, "jitter defaults true · anti-thundering-herd");
        assert!(r.on_codes.is_empty());
    }

    #[test]
    fn backoff_strategy_closed_enum() {
        assert_eq!(
            BackoffStrategy::from_str_opt("fixed"),
            Some(BackoffStrategy::Fixed)
        );
        assert_eq!(
            BackoffStrategy::from_str_opt("linear"),
            Some(BackoffStrategy::Linear)
        );
        assert_eq!(
            BackoffStrategy::from_str_opt("exponential"),
            Some(BackoffStrategy::Exponential)
        );
        assert_eq!(BackoffStrategy::from_str_opt("cubic"), None);
    }

    #[test]
    fn error_code_regex_valid() {
        assert!(is_valid_error_code("NIKA-PROVIDER-001"));
        assert!(is_valid_error_code("NIKA-DAG-001"));
        assert!(is_valid_error_code("NIKA-BUILTIN-FETCH-001"));
        assert!(is_valid_error_code("NIKA-PARSE-WHEN-001"));
    }

    #[test]
    fn error_code_regex_invalid() {
        assert!(!is_valid_error_code("NIKA-001")); // no namespace
        assert!(!is_valid_error_code("NIKA-X-001")); // ns too short
        assert!(!is_valid_error_code("NIKA-TOOLONGNAME-001")); // ns 11 > 9
        assert!(!is_valid_error_code("NIKA-dag-001")); // lowercase
        assert!(!is_valid_error_code("NIKA-DAG-1")); // 1 digit
        assert!(!is_valid_error_code("NIKA-DAG-0001")); // 4 digits
        assert!(!is_valid_error_code("OLY-DAG-001")); // wrong prefix
        assert!(!is_valid_error_code("NIKA-A-B-C-001")); // 3 sub-namespaces
        assert!(!is_valid_error_code("503")); // HTTP status · not a code
    }
}
