// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The word-level readers of the lexicon: the typed literals an intent carries as bindings
//! (every URL, address, local path and timezone token, copied verbatim), a numeric attempt
//! bound, a money literal beside a policy word, and the categories a classify object lists.

use super::super::plan::{Binding, Plan};
use super::cues::{ATTEMPT_NOUNS, BOUND_WORDS, CATEGORY_MARKERS, NUMBER_WORDS};
use super::normalize;

/// Every literal token of the intent, as a binding of its role, in order of appearance.
pub(super) fn collect_bindings(intent: &str, plan: &mut Plan) {
    for word in intent.split_whitespace() {
        let token = word.trim_end_matches(['.', ',', ';', ')', ']', ':']);
        if token.starts_with("http://") || token.starts_with("https://") {
            plan.bindings.push(Binding {
                role: "url",
                literal: token.to_owned(),
            });
        } else if token.contains('@') && token.contains('.') && !token.starts_with('@') {
            plan.bindings.push(Binding {
                role: "email",
                literal: token.to_owned(),
            });
        } else if (token.starts_with("./") || token.starts_with('/')) && token.len() > 2 {
            plan.bindings.push(Binding {
                role: "path",
                literal: token.to_owned(),
            });
        } else if token.starts_with("Europe/")
            || token.starts_with("America/")
            || token.starts_with("Asia/")
            || token.starts_with("Africa/")
        {
            plan.bindings.push(Binding {
                role: "timezone",
                literal: token.to_owned(),
            });
        }
    }
}

/// The attempt, cycle or iteration bound a clause states as a number.
pub(super) fn retry_bound(lower: &str) -> Option<u32> {
    if !BOUND_WORDS.iter().any(|w| lower.contains(w)) {
        return None;
    }
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .collect();
    for (index, word) in words.iter().enumerate() {
        let number = word.parse::<u32>().ok().or_else(|| {
            NUMBER_WORDS
                .iter()
                .find(|(w, _)| w == word)
                .map(|(_, n)| *n)
        });
        let Some(number) = number else { continue };
        let next = words.get(index + 1).copied().unwrap_or_default();
        let next2 = words.get(index + 2).copied().unwrap_or_default();
        let prev = index
            .checked_sub(2)
            .and_then(|i| words.get(i))
            .copied()
            .unwrap_or_default();
        let prev2 = index
            .checked_sub(3)
            .and_then(|i| words.get(i))
            .copied()
            .unwrap_or_default();
        if ATTEMPT_NOUNS.iter().any(|n| {
            next.starts_with(n)
                || next2.starts_with(n)
                || prev.starts_with(n)
                || prev2.starts_with(n)
        }) {
            return Some(number);
        }
    }
    None
}

/// An amount with a currency beside a policy word (a cap, an eligibility, a limit): the
/// literal policy data a money-moving effect carries.
pub(super) fn money_literal(sentence: &str) -> bool {
    let lower = normalize(sentence);
    let has_amount = lower
        .split(|c: char| !c.is_alphanumeric() && c != '€' && c != '$')
        .any(|w| w.chars().all(|c| c.is_ascii_digit()) && !w.is_empty())
        && ["€", "eur", "euro", "usd", "$", "dollar"]
            .iter()
            .any(|c| lower.contains(c));
    has_amount
        && [
            "éligible",
            "eligible",
            "limite",
            "maximum",
            "cap",
            "only",
            "uniquement",
            "seuls",
            "up to",
            "per ",
        ]
        .iter()
        .any(|c| lower.contains(c))
}

/// The categories a classify object lists after its marker (`as bug or feature`, `en A, B
/// ou C`): at least two short names, verbatim.
pub(super) fn categories_of(detail_lower: &str) -> Vec<String> {
    for marker in CATEGORY_MARKERS {
        if let Some(pos) = detail_lower.find(marker) {
            let tail = detail_lower.get(pos + marker.len()..).unwrap_or_default();
            let tail = tail.split([';', '.']).next().unwrap_or_default();
            let parts: Vec<String> = tail
                .replace(" ou ", ",")
                .replace(" or ", ",")
                .split(',')
                .map(|p| p.trim().trim_end_matches(['.', ';']).to_owned())
                .filter(|p| !p.is_empty() && p.split_whitespace().count() <= 3)
                .collect();
            if parts.len() >= 2 {
                return parts;
            }
        }
    }
    Vec::new()
}
