// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A value a proposal the person saw bound, which a conversation's revision no longer carries:
//! the Session cites the person's words as removing it. That the words are the person's is
//! checked where they are cited; whether they remove that value is the judge's to say, asked
//! once per claim before the whole request is judged. Only `removed` lets the revision go on.

use serde_json::json;

use crate::decide::{ChoiceOption, ChoiceQuestion};

/// What the question asks.
const ASKED: &str = "A proposal the person saw bound the value `value`. The candidate no longer carries it, and the person's own words `words` are cited as removing it. removed: these words ask to remove or replace that value. kept: they do not; the value stays the person's.";

/// A value a proposal the person saw bound, and the person's words cited as removing it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Removal {
    /// The value the earlier proposal bound: a source, an output, a model.
    pub value: String,
    /// The person's own words cited as removing it.
    pub words: String,
}

impl Removal {
    /// The claim that the person's `words` remove `value`.
    #[must_use]
    pub fn new(value: impl Into<String>, words: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            words: words.into(),
        }
    }

    /// The closed question whether these words remove this value: the `k`-th removal claim of
    /// a revision of `stated`, the person's request (`verify-removed-<k>`).
    #[must_use]
    pub fn question(&self, k: usize, stated: &str) -> ChoiceQuestion {
        let state = json!({"value": self.value, "words": self.words, "request": stated});
        let options = vec![
            ChoiceOption::new("removed", "these words ask to remove or replace that value"),
            ChoiceOption::new("kept", "they do not: the value stays the person's"),
        ];
        ChoiceQuestion::new(format!("verify-removed-{k}"), ASKED, state, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One question per claim, numbered, its state the claim and the request, its options the
    /// two readings and NONE.
    #[test]
    fn a_removal_claim_is_one_closed_question_over_the_words() {
        let claim = Removal::new("https://techcrunch.com/feed", "enleve techcrunch");
        let question = claim.question(1, "x");
        assert_eq!(question.id, "verify-removed-1");
        assert_eq!(question.state["value"], "https://techcrunch.com/feed");
        assert_eq!(question.state["words"], "enleve techcrunch");
        assert_eq!(
            question.keys(),
            ["removed", "kept", crate::decide::NONE_OPTION]
        );
    }
}
