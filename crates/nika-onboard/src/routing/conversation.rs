// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Closed conversational acts shared by hosts, separate from workflow intent routing.

/// The few words that abandon an authoring round (a `no` is an ANSWER —
/// « should each filename be a heading? » — never an abandonment).
#[must_use]
pub fn is_cancel(line: &str) -> bool {
    matches!(
        line.trim().to_lowercase().as_str(),
        "cancel"
            | "/cancel"
            | "stop"
            | "drop"
            | "discard"
            | "abandon"
            | "annule"
            | "annuler"
            | "laisse tomber"
            | "forget it"
            | "never mind"
    )
}

/// A closed line's words whatever the typography: spaces as one (a no-break
/// space, or the narrow one French sets before `?`), the closing marks set
/// aside, a typographic apostrophe as `'`, lower case. It adds no word.
fn closed_words(line: &str, closing: &[char]) -> String {
    line.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(|c: char| c == ' ' || closing.contains(&c))
        .replace(['\u{2018}', '\u{2019}'], "'")
        .to_lowercase()
}

/// The few words that ask WHY beside what waits (a question, a gate) —
/// answered from the machine's state, consuming nothing. A closed set of
/// whole lines, punctuation aside.
#[must_use]
pub fn is_why(line: &str) -> bool {
    let word = closed_words(line, &['?', '!', '.']);
    matches!(
        word.as_str(),
        "why"
            | "/why"
            | "why this"
            | "why this question"
            | "why do you ask"
            | "explain"
            | "explain this"
            | "pourquoi"
            | "pourquoi cette question"
            | "pourquoi ça"
            | "explique"
            | "c'est quoi"
            | "c'est pour quoi"
    )
}

/// The few words that ask what Nika understood of the request — the
/// Meaning view from the compiler's ledger; beside a proposal it holds it.
#[must_use]
pub fn is_meaning(line: &str) -> bool {
    let word = closed_words(line, &['?', '!', '.']);
    matches!(
        word.as_str(),
        "/meaning"
            | "meaning"
            | "what did you understand"
            | "what did you keep"
            | "did you keep everything"
            | "qu'as-tu compris"
            | "qu'as-tu retenu"
            | "tu as tout gardé"
    )
}

/// The few words that ask what just went wrong — answered by the last
/// recovery card, from memory, never by another call.
#[must_use]
pub fn is_what_happened(line: &str) -> bool {
    let word = closed_words(line, &['?', '!', '.']);
    matches!(
        word.as_str(),
        "what happened"
            | "what just happened"
            | "what went wrong"
            | "what was that"
            | "/last"
            | "de quoi"
            | "quoi"
            | "hein"
            | "comment ça"
            | "qu'est-ce qui s'est passé"
            | "qu'est-ce qui se passe"
    )
}

/// A bare greeting or thanks — the conversation's, never the compiler's
/// (whose exact-skeleton door would read a lone `hello` as the `hello`
/// lesson). A closed set of whole lines, punctuation aside.
#[must_use]
pub fn is_greeting(input: &str) -> bool {
    let word = closed_words(input, &['!', '.', '?', ',']);
    matches!(
        word.as_str(),
        "hello"
            | "hi"
            | "hey"
            | "yo"
            | "bonjour"
            | "salut"
            | "coucou"
            | "thanks"
            | "thank you"
            | "merci"
            | "bye"
            | "au revoir"
    )
}
