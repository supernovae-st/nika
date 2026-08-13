// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `after:` — the CONTROL boundary (spec `03-dag.md` §after · W2).
//!
//! An `after:` entry is a map `{producer-task: predicate}`; each entry
//! is one CONTROL edge whose predicate names the producer states that
//! admit the consumer. The predicate set is CLOSED (`NIKA-DAG-005`
//! otherwise) and speaks the R5 outcome-class spellings (spec #118 ·
//! LAW-GRAMMAR-0231): `success` · `failure` · `skipped` · `terminal`,
//! where `terminal` includes `cancelled` (the resolved W2-Q2 witness —
//! « run once X is settled, whatever happened »). The pre-R5 participial
//! spellings (`succeeded` · `failed`) are DEAD FORMS: they refuse
//! TEACHING (the respelling + the `nika check --fix` repair), never a
//! bare unknown-predicate message — the C2 flag-day precedent
//! (`NIKA-VALUES-001/002`), and the teaching text lives here because the
//! message vocabulary's home is the vocabulary crate (the `dead_form.rs`
//! · `keys.rs` pattern — the 15k wall funds no message strings in
//! `nika-schema`).

use std::fmt;

/// The closed `after:` predicate set (spec `03-dag.md` §after).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterPredicate {
    /// Admits when the producer settles `success`.
    Success,
    /// Admits when the producer settles `failure`.
    Failure,
    /// Admits when the producer settles `skipped`.
    Skipped,
    /// Admits on ANY settled state — `success` · `failure` · `skipped`
    /// · `cancelled` (the always-pattern · cancelled IS terminal).
    Terminal,
    /// NOT a settle-state comparison — the `E_f` cleanup attachment
    /// (spec 03 §unwind). It fires on cancel and on timeout for a
    /// producer that STARTED, runs BEFORE the producer's failure
    /// settles outward, and its own failure never propagates. An
    /// `unwind` edge is never in `G_p`: it does not schedule, does not
    /// participate in cycle detection, and does not enter wave
    /// assignment. An engine that puts it in the precedence graph is
    /// wrong.
    Unwind,
}

impl AfterPredicate {
    /// Parse the wire spelling. `None` = outside the closed set
    /// (`NIKA-DAG-005`).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "success" => Some(Self::Success),
            "failure" => Some(Self::Failure),
            "skipped" => Some(Self::Skipped),
            "terminal" => Some(Self::Terminal),
            "unwind" => Some(Self::Unwind),
            _ => None,
        }
    }

    /// The wire spelling (spec `03-dag.md` §after · schema enum).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Skipped => "skipped",
            Self::Terminal => "terminal",
            Self::Unwind => "unwind",
        }
    }

    /// Every legal spelling, in spec order (teaching surfaces · LSP
    /// completion · the DAG-005 message).
    #[must_use]
    pub fn all() -> &'static [&'static str] {
        &["success", "failure", "skipped", "terminal", "unwind"]
    }
}

/// The R5 dead spellings — the participial forms the flag-day killed
/// (spec #118 · LAW-GRAMMAR-0231 · `succeeded`→`success` ·
/// `failed`→`failure` · `skipped`/`terminal` unchanged). `Some(respelling)`
/// when `s` is a dead spelling, `None` otherwise — the caller teaches
/// only what it can name.
#[must_use]
pub fn dead_spelling_respelling(s: &str) -> Option<&'static str> {
    match s {
        "succeeded" => Some("success"),
        "failed" => Some("failure"),
        _ => None,
    }
}

/// The `NIKA-DAG-005` refusal text for an out-of-set `after:` predicate.
/// A dead spelling refuses TEACHING — the respelling and the
/// `nika check --fix` repair (mode-independent, decided before the
/// generic check, the dead-form doctrine); any other out-of-set spelling
/// names the closed set.
#[must_use]
pub fn predicate_refusal(task: &str, target: &str, spelling: &str) -> String {
    match dead_spelling_respelling(spelling) {
        Some(to) => format!(
            "task `{task}` after.{target}: `{spelling}` is a dead predicate spelling \
             (R5 · spec #118 — the outcome-class rename) — respell as `{to}` \
             (`nika check --fix` applies it)"
        ),
        None => format!(
            "task `{task}` after.{target}: `{spelling}` is not a predicate — \
             the set is closed: success · failure · skipped · terminal · unwind"
        ),
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
        // The DAG-005 class — near-misses AND the R5 dead spellings
        // never coerce.
        for bad in ["passed", "succeeded", "failed", "SUCCESS", "done", ""] {
            assert!(AfterPredicate::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn dead_spellings_respell_one_to_one() {
        assert_eq!(dead_spelling_respelling("succeeded"), Some("success"));
        assert_eq!(dead_spelling_respelling("failed"), Some("failure"));
        // The unchanged pair is NOT a dead form — it parses today.
        assert_eq!(dead_spelling_respelling("skipped"), None);
        assert_eq!(dead_spelling_respelling("terminal"), None);
        assert_eq!(dead_spelling_respelling("passed"), None);
    }

    #[test]
    fn dead_spelling_refusal_teaches_the_respelling_and_the_repair() {
        let m = predicate_refusal("deploy", "tests", "succeeded");
        assert!(m.contains("task `deploy` after.tests"), "{m}");
        assert!(m.contains("dead predicate spelling"), "{m}");
        assert!(m.contains("respell as `success`"), "{m}");
        assert!(m.contains("nika check --fix"), "{m}");
        let m = predicate_refusal("deploy", "tests", "failed");
        assert!(m.contains("respell as `failure`"), "{m}");
    }

    #[test]
    fn unknown_spelling_refusal_names_the_closed_set() {
        let m = predicate_refusal("deploy", "tests", "passed");
        assert!(m.contains("is not a predicate"), "{m}");
        assert!(
            m.contains("success · failure · skipped · terminal · unwind"),
            "{m}"
        );
        assert!(!m.contains("dead predicate spelling"), "{m}");
    }
}
