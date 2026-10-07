// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The verdicts that rejected candidate bytes in one goal's compiles, handed to every later
//! compile of that goal (R6 across compiles): an answer round, the write-again after a located
//! defect, the stronger seat's retry, a continuation, a correction. The compiler carries one only
//! into a compile of the very request it judged, so a judge is never asked again on bytes it
//! rejected for that request, while a corrected request is judged again. Each is a
//! `semantic_verification` attempt as the compiler recorded it, the latest kept per bytes,
//! judge, request and context (other answers or another observed world are judged again);
//! an abstention is never kept, so a later round may still decide it. New work carries none.
//! Memory only: a reopened session carries no rejection from before it closed.

use nika_onboard::compile::{CompileOutcome, round};
use serde_json::Value;

/// The rejections kept for the goal they were found under.
#[derive(Debug, Default)]
pub(super) struct Declined {
    /// The goal they belong to: the session's goal when they were found, or the restatement
    /// that continued it.
    goal: Option<String>,
    /// One `semantic_verification` attempt per rejected bytes, judge, request and context, the
    /// latest found, in order of first appearance.
    attempts: Vec<Value>,
}

impl Declined {
    /// What a compile of `goal` carries: the rejections kept for that goal, none for another.
    pub(super) fn carried(&self, goal: Option<&String>) -> Vec<Value> {
        if self.goal.as_ref() == goal {
            self.attempts.clone()
        } else {
            Vec::new()
        }
    }

    /// Keep the rejections a compile of `goal` recorded (`round::rejections`): for another goal
    /// than the kept one, the kept ones go first (new work begins anew).
    pub(super) fn keep(&mut self, goal: Option<&String>, out: &CompileOutcome) {
        if self.goal.as_ref() != goal {
            self.goal = goal.cloned();
            self.attempts.clear();
        }
        round::keep_rejections(&mut self.attempts, round::rejections(out));
    }

    /// A restatement continues the goal `from` as `to` (a correction, the request read again
    /// with the human's words): the rejections kept for `from` follow it.
    pub(super) fn follow(&mut self, from: Option<&String>, to: Option<&String>) {
        if self.goal.as_ref() == from {
            self.goal = to.cloned();
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::Declined;
    use nika_onboard::compile::{CompileRequest, compile};
    use serde_json::{Value, json};

    /// An outcome whose verifications are `attempts`.
    fn judged(attempts: &[Value]) -> nika_onboard::compile::CompileOutcome {
        let mut out = compile(&CompileRequest::create("bounded-batch")).expect("compiles");
        out.provenance.decision = Some(json!({"semantic_verification": attempts}));
        out
    }

    /// One verification of the bytes `sha` by the judge `seat`, rejected or abstained.
    fn verdict(sha: &str, seat: &str, rejected: bool) -> Value {
        json!({"candidate_sha256": sha, "judge": {"seat": seat, "kind": "authoring_provider"},
            "declined": true, "rejected": rejected, "settled": false})
    }

    /// The rejections of a goal are carried into its later compiles only, once per bytes and
    /// judge, never an abstention; a restatement carries them on under its new words; another
    /// goal carries none, and keeping its own verdicts drops the earlier goal's.
    #[test]
    fn a_goals_rejections_follow_it_and_new_work_carries_none() {
        let (goal, corrected, other) = (
            Some("Read ./a.csv".to_owned()),
            Some("Original request:\nRead ./a.csv\nCorrection".to_owned()),
            Some("Write ./b.md".to_owned()),
        );
        let (b1, b2) = (verdict("b1", "m", true), verdict("b2", "m", true));
        let mut kept = Declined::default();
        assert_eq!(kept.carried(None), Vec::<Value>::new(), "nothing kept yet");
        kept.keep(
            goal.as_ref(),
            &judged(&[b1.clone(), verdict("b3", "m", false)]),
        );
        kept.keep(goal.as_ref(), &judged(&[b1.clone(), b2.clone()]));
        assert_eq!(kept.carried(goal.as_ref()), [b1.clone(), b2.clone()]);
        assert_eq!(kept.carried(other.as_ref()), Vec::<Value>::new());
        assert_eq!(kept.carried(None), Vec::<Value>::new());
        // A restatement of another goal moves nothing; one of this goal carries them on.
        kept.follow(other.as_ref(), corrected.as_ref());
        assert_eq!(kept.carried(goal.as_ref()), [b1.clone(), b2.clone()]);
        kept.follow(goal.as_ref(), corrected.as_ref());
        assert_eq!(kept.carried(goal.as_ref()), Vec::<Value>::new());
        assert_eq!(kept.carried(corrected.as_ref()), [b1, b2]);
        // New work: its compile keeps its own verdicts, the earlier goal's are gone.
        let b4 = verdict("b4", "m", true);
        kept.keep(other.as_ref(), &judged(std::slice::from_ref(&b4)));
        assert_eq!(kept.carried(other.as_ref()), [b4]);
        assert_eq!(kept.carried(corrected.as_ref()), Vec::<Value>::new());
        // A compile that rejected nothing under another goal still begins it anew.
        kept.keep(goal.as_ref(), &judged(&[]));
        assert_eq!(kept.carried(goal.as_ref()), Vec::<Value>::new());
        assert_eq!(kept.carried(other.as_ref()), Vec::<Value>::new());
    }
}
