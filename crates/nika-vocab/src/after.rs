// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `after:` — the CONTROL boundary (spec `03-dag.md` §after · W2).
//!
//! An `after:` entry is a map `{producer-task: predicate}`; each entry
//! is one CONTROL edge whose predicate names the producer states that
//! admit the consumer. The predicate set is CLOSED (`NIKA-DAG-005`
//! otherwise) and `terminal` includes `cancelled` (the resolved W2-Q2
//! witness — « run once X is settled, whatever happened »).

use std::fmt;

/// The closed `after:` predicate set (spec `03-dag.md` §after).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterPredicate {
    /// Admits when the producer settles `success`.
    Succeeded,
    /// Admits when the producer settles `failure`.
    Failed,
    /// Admits when the producer settles `skipped`.
    Skipped,
    /// Admits on ANY settled state — `success` · `failure` · `skipped`
    /// · `cancelled` (the always-pattern · cancelled IS terminal).
    Terminal,
}

impl AfterPredicate {
    /// Parse the wire spelling. `None` = outside the closed set
    /// (`NIKA-DAG-005`).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "skipped" => Some(Self::Skipped),
            "terminal" => Some(Self::Terminal),
            _ => None,
        }
    }

    /// The wire spelling (spec `03-dag.md` §after · schema enum).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::Terminal => "terminal",
        }
    }

    /// Every legal spelling, in spec order (teaching surfaces · LSP
    /// completion · the DAG-005 message).
    #[must_use]
    pub fn all() -> &'static [&'static str] {
        &["succeeded", "failed", "skipped", "terminal"]
    }
}

impl fmt::Display for AfterPredicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_round_trips_the_closed_set() {
        for s in AfterPredicate::all() {
            let p = AfterPredicate::parse(s).expect("closed-set spelling");
            assert_eq!(p.as_str(), *s);
        }
    }

    #[test]
    fn parse_refuses_outside_the_set() {
        // The DAG-005 class — near-misses never coerce.
        for bad in ["passed", "success", "SUCCEEDED", "done", ""] {
            assert!(AfterPredicate::parse(bad).is_none(), "{bad}");
        }
    }
}
