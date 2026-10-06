// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The word-level readers of the lexicon: the typed literals an intent carries as bindings
//! (every URL, address, local path and timezone token, copied verbatim), a numeric attempt
//! bound, a money literal beside a policy word, and the categories a classify object lists.

use super::super::plan::{Binding, Plan};
use super::cues::{ATTEMPT_NOUNS, BOUND_WORDS, CATEGORY_MARKERS, NUMBER_WORDS};
use super::normalize;

/// Every literal token of the intent, as a binding of its role, in order of appearance; a
/// token inside quoted content is what the workflow writes or matches, never a literal of it.
pub(super) fn collect_bindings(intent: &str, plan: &mut Plan) {
    let offset = |word: &str| word.as_ptr() as usize - intent.as_ptr() as usize;
    for word in intent.split_whitespace() {
        let token = word.trim_end_matches(['.', ',', ';', ')', ']', ':']);
        let role = if token.starts_with("http://") || token.starts_with("https://") {
            "url"
        } else if token.contains('@') && token.contains('.') && !token.starts_with('@') {
            "email"
        } else if (token.starts_with("./") || token.starts_with('/')) && token.len() > 2 {
            "path"
        } else if ["Europe/", "America/", "Asia/", "Africa/"]
            .iter()
            .any(|prefix| token.starts_with(prefix))
        {
            "timezone"
        } else {
            continue;
        };
        // Only literal candidates need the prefix quote scan. Ordinary prose must not
        // re-read the whole prefix for every word. Keep the original token and offset:
        // quoted_at asks whether its START was quoted, not whether it contains a quote.
        if !super::quoted_at(intent, offset(word)) {
            plan.bindings.push(Binding::new(role, token));
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
        let neighbors = [
            Some(index + 1),
            Some(index + 2),
            index.checked_sub(2),
            index.checked_sub(3),
        ];
        if neighbors
            .into_iter()
            .flatten()
            .filter_map(|i| words.get(i))
            .any(|word| ATTEMPT_NOUNS.iter().any(|noun| word.starts_with(noun)))
        {
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

#[cfg(test)]
mod binding_tests {
    use super::*;

    #[test]
    fn attempt_bound_keeps_the_same_four_neighbor_positions() {
        for clause in [
            "at most 3 attempts",
            "at most 3 total attempts",
            "attempts maximum 3",
            "attempts maximum of 3",
        ] {
            assert_eq!(retry_bound(clause), Some(3), "{clause}");
        }
        for clause in [
            "at most 3",
            "at most 3 total permitted attempts",
            "attempts maximum with only 3",
            "at most attempts 3",
        ] {
            assert_eq!(retry_bound(clause), None, "{clause}");
        }
    }

    fn bindings(intent: &str) -> Vec<Binding> {
        let mut plan = Plan::default();
        collect_bindings(intent, &mut plan);
        plan.bindings
    }

    #[test]
    fn literal_roles_order_punctuation_and_duplicates_are_unchanged() {
        assert_eq!(
            bindings(
                "https://user@example.test/data, user@example.test; ./notes.json) /tmp/result.json] Europe/Paris: ./notes.json."
            ),
            vec![
                Binding::new("url", "https://user@example.test/data"),
                Binding::new("email", "user@example.test"),
                Binding::new("path", "./notes.json"),
                Binding::new("path", "/tmp/result.json"),
                Binding::new("timezone", "Europe/Paris"),
                Binding::new("path", "./notes.json"),
            ],
        );
    }

    #[test]
    fn quoted_payload_literals_never_become_bindings() {
        for (open, close) in [('"', '"'), ('\'', '\''), ('`', '`'), ('«', '»'), ('“', '”')] {
            let intent = format!(
                "Write {open}content https://hidden.example user@hidden.test ./hidden.json Europe/Paris{close} to ./visible.json"
            );
            assert_eq!(
                bindings(&intent),
                vec![Binding::new("path", "./visible.json")],
                "{intent}"
            );
        }
    }

    #[test]
    fn quote_guard_still_uses_the_original_word_start_and_utf8_offset() {
        // These edge cases pin the historical lexical law, not a new quote parser.
        assert_eq!(
            bindings(r#""a@b.com""#),
            vec![Binding::new("email", r#""a@b.com""#)]
        );
        assert_eq!(
            bindings(r#"https://host/"part one""#),
            vec![Binding::new("url", r#"https://host/"part"#)]
        );
        assert_eq!(
            bindings("Résumé don't n’écris ./résultat.json"),
            vec![Binding::new("path", "./résultat.json")]
        );
        for text in [
            r#"say \" https://open.example ./open.txt"#,
            r#"say \\" https://open.example ./open.txt"#,
        ] {
            assert_eq!(
                bindings(text),
                vec![
                    Binding::new("url", "https://open.example"),
                    Binding::new("path", "./open.txt")
                ]
            );
        }
    }

    #[test]
    fn a_large_prose_prefix_keeps_the_final_literals_without_scanning_each_word() {
        let intent = "Ordinary context stays here ".repeat(20_000)
            + "https://example.test/data ./result.json";
        assert!(intent.len() > 512 * 1024);
        assert_eq!(
            bindings(&intent),
            vec![
                Binding::new("url", "https://example.test/data"),
                Binding::new("path", "./result.json"),
            ]
        );
    }
}
