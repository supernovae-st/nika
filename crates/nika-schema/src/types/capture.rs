// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Exec capture mode — the `exec.capture:` field.
//!
//! Per spec `02-verbs.md` §exec · « `capture` · `stdout` (default) ·
//! `stderr` · `combined` · `structured` (= `{ stdout, stderr,
//! exit_code }`) ».

use std::fmt;

use serde::{Deserialize, Serialize};

/// What an `exec:` task's `.output` captures.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "lowercase")]
pub enum CaptureMode {
    /// The command's stdout (the default).
    #[default]
    Stdout,
    /// The command's stderr.
    Stderr,
    /// Stdout + stderr interleaved.
    Combined,
    /// `{ stdout, stderr, exit_code }` — the structured record.
    Structured,
}

impl CaptureMode {
    /// Parse the YAML `capture:` scalar (closed enum).
    #[must_use]
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "stdout" => Some(Self::Stdout),
            "stderr" => Some(Self::Stderr),
            "combined" => Some(Self::Combined),
            "structured" => Some(Self::Structured),
            _ => None,
        }
    }
}

impl fmt::Display for CaptureMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stdout => write!(f, "stdout"),
            Self::Stderr => write!(f, "stderr"),
            Self::Combined => write!(f, "combined"),
            Self::Structured => write!(f, "structured"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_stdout() {
        assert_eq!(CaptureMode::default(), CaptureMode::Stdout);
    }

    #[test]
    fn closed_enum_parse() {
        assert_eq!(
            CaptureMode::from_str_opt("stdout"),
            Some(CaptureMode::Stdout)
        );
        assert_eq!(
            CaptureMode::from_str_opt("stderr"),
            Some(CaptureMode::Stderr)
        );
        assert_eq!(
            CaptureMode::from_str_opt("combined"),
            Some(CaptureMode::Combined)
        );
        assert_eq!(
            CaptureMode::from_str_opt("structured"),
            Some(CaptureMode::Structured)
        );
        assert_eq!(CaptureMode::from_str_opt("both"), None);
    }

    #[test]
    fn display_round_trips() {
        for m in [
            CaptureMode::Stdout,
            CaptureMode::Stderr,
            CaptureMode::Combined,
            CaptureMode::Structured,
        ] {
            assert_eq!(CaptureMode::from_str_opt(&m.to_string()), Some(m));
        }
    }
}
