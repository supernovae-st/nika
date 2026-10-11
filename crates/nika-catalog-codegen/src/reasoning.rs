// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The reasoning facts a capability rule records, `caps.reasoning_facts`: whether the model
//! reasons when the request names no reasoning control, whether that reasoning is spent inside
//! the request's output cap, and how a `.nika` file turns it off on this engine. A fact the rule
//! does not record stays unknown; a key or word outside this set fails the build.

use serde::Deserialize;

/// `reasoning_facts = { by_default = …, in_output_cap = …, off = "…" }`.
#[derive(Deserialize, Debug, Default, Clone, Copy)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReasoningFactsToml {
    #[serde(default)]
    by_default: Option<bool>,
    #[serde(default)]
    in_output_cap: Option<bool>,
    #[serde(default)]
    off: Option<ThinkingOffToml>,
}

/// The words of `off`, one per `ThinkingOff` variant of the same name.
#[derive(Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
enum ThinkingOffToml {
    ByDefault,
    BudgetZero,
    NotReached,
    Never,
}

/// The `ReasoningFacts` expression of a rule's facts; an unrecorded one is `None`.
pub(crate) fn emit_reasoning_facts(facts: ReasoningFactsToml) -> String {
    let off = facts.off.map_or_else(
        || "None".to_owned(),
        |off| format!("Some(crate::types::model::ThinkingOff::{off:?})"),
    );
    format!(
        "crate::types::model::ReasoningFacts::new({:?}, {:?}, {off})",
        facts.by_default, facts.in_output_cap
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> Result<ReasoningFactsToml, toml::de::Error> {
        toml::from_str(text)
    }

    #[test]
    fn recorded_facts_emit_typed_and_unrecorded_ones_stay_unknown() {
        let facts = parsed("by_default = true\nin_output_cap = false\noff = \"budget-zero\"")
            .expect("valid facts");
        assert_eq!(
            emit_reasoning_facts(facts),
            "crate::types::model::ReasoningFacts::new(Some(true), Some(false), \
             Some(crate::types::model::ThinkingOff::BudgetZero))"
        );
        assert_eq!(
            emit_reasoning_facts(ReasoningFactsToml::default()),
            "crate::types::model::ReasoningFacts::new(None, None, None)"
        );
        for (word, variant) in [
            ("by-default", "ByDefault"),
            ("not-reached", "NotReached"),
            ("never", "Never"),
        ] {
            let facts = parsed(&format!("off = \"{word}\"")).expect("a known word");
            let emitted = emit_reasoning_facts(facts);
            assert!(
                emitted.ends_with(&format!("ThinkingOff::{variant}))")),
                "{emitted}"
            );
        }
    }

    #[test]
    fn an_unknown_key_or_word_fails_the_build() {
        assert!(parsed("reasons_by_default = true").is_err());
        assert!(parsed("off = \"sometimes\"").is_err());
        assert!(parsed("by_default = \"yes\"").is_err());
    }
}
