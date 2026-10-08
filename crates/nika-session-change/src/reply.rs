// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The grammar of a human's reply to one compile question: whether it names an offered key
//! alone or carries one, whether it is a value as typed, the one bounded reading prompt for a
//! value or a choice, and the verbatim whole-token copy a reading may bind. Pure functions over
//! the question and the line; the Session decides when a reading runs and what it binds.

use nika_onboard::compile::CompileQuestion;

/// Whether `text` is one of the keys the question offers, exactly.
fn is_offered(question: &CompileQuestion, text: &str) -> bool {
    question.options.iter().any(|offer| offer.key == text)
}

/// The line is an offered key alone — as written, or as its JSON string (the shape the
/// compiler asks) — spacing aside: its own answer, no reading.
#[must_use]
pub fn names_an_offer_alone(question: &CompileQuestion, line: &str) -> bool {
    let line = line.trim();
    if let Ok(serde_json::Value::String(text)) = serde_json::from_str::<serde_json::Value>(line) {
        return is_offered(question, &text);
    }
    is_offered(question, line)
}

/// Whether the line carries at least one offered key as whole tokens: the only lines a
/// reading could bind from. A necessary condition, never a choice — which offered key the
/// line chooses (one of several, not the one it rejects) is the reading's, then the human's.
#[must_use]
pub fn carries_an_offer(question: &CompileQuestion, line: &str) -> bool {
    question
        .options
        .iter()
        .any(|offer| !offer.key.is_empty() && whole_part(line.trim(), &offer.key))
}

/// The keys offered now, in the compiler's order.
#[must_use]
pub fn offered_keys(question: &CompileQuestion) -> String {
    question
        .options
        .iter()
        .map(|offer| offer.key.as_str())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// One `provider/name` token: a model identity as the model question asks it, nothing else.
#[must_use]
pub fn names_a_model(line: &str) -> bool {
    let line = line.trim();
    !line.contains(char::is_whitespace)
        && line.split_once('/').is_some_and(|(provider, name)| {
            !name.is_empty()
                && !provider.is_empty()
                && provider
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
        })
}

/// The line is its own value without any reading: a JSON literal (`42` · `true` ·
/// `"a quoted value"` · `["a", "b"]`) or one token, nothing around it to leave out.
#[must_use]
pub fn as_typed(line: &str) -> bool {
    let line = line.trim();
    !line.contains(char::is_whitespace) || serde_json::from_str::<serde_json::Value>(line).is_ok()
}

/// The one bounded reading: the question in its own words, the reply verbatim, and one
/// instruction — copy the value, or say NONE.
#[must_use]
pub fn reading_prompt(question: &CompileQuestion, line: &str) -> String {
    let why = if question.why.is_empty() {
        String::new()
    } else {
        format!(" ({})", question.why)
    };
    format!(
        "Nika, an automation tool, asked a human for one value: «{label}»{why}.\nThe human replied: «{reply}».\nCopy that value exactly as it appears in the reply, character for character: only the value, without the words or the sentence punctuation around it; if the whole reply is the value, copy the whole reply. Never correct, translate, complete or add anything. If the reply gives no value, or more than one possible value, answer NONE.\nValue:",
        label = question.label,
        reply = line.trim(),
    )
}

/// The one bounded reading of a choice: the question in its own words, the exact keys it
/// offers now, the reply verbatim, and one instruction — copy the offered answer the reply
/// chooses, or say NONE.
#[must_use]
pub fn choice_prompt(question: &CompileQuestion, line: &str) -> String {
    let offers = question
        .options
        .iter()
        .map(|offer| format!("«{}»", offer.key))
        .collect::<Vec<_>>()
        .join(" · ");
    format!(
        "Nika, an automation tool, asked a human to choose one answer: «{label}».\nThe offered answers are exactly: {offers}.\nThe human replied: «{reply}».\nCopy the one offered answer the reply chooses, exactly as it appears in the reply, character for character: only that answer, without the words or the sentence punctuation around it. Never correct, translate, complete or add anything. If the reply chooses none of the offered answers, more than one, or only rejects one, answer NONE.\nValue:",
        label = question.label,
        reply = line.trim(),
    )
}

/// The value a reply points at: one line, its own `Value:` cue and one pair of quotes
/// around it removed (both are the model's, never part of the value), kept only when it
/// is made of whole tokens of the human's line (`whole_part`).
#[must_use]
pub fn verbatim_part(reply: &str, line: &str) -> Option<String> {
    let mut lines = reply.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.next()?;
    if lines.next().is_some() {
        return None;
    }
    let value = unquoted(first.strip_prefix("Value:").map_or(first, str::trim));
    (!value.is_empty() && value != "NONE" && whole_part(line.trim(), value))
        .then(|| value.to_owned())
}

/// The offered key a reply points at: the verbatim whole-token copy (`verbatim_part`), kept
/// only when it IS one of the keys the question offers now — a word the human typed that
/// is not offered, several keys, a key with anything around it bind nothing.
#[must_use]
pub fn offered_part(reply: &str, line: &str, question: &CompileQuestion) -> Option<String> {
    verbatim_part(reply, line).filter(|value| is_offered(question, value))
}

/// One pair of quotes around a copy, removed.
fn unquoted(text: &str) -> &str {
    for (open, close) in [
        ('"', '"'),
        ('\'', '\''),
        ('`', '`'),
        ('«', '»'),
        ('\u{201c}', '\u{201d}'),
    ] {
        if let Some(inner) = text.strip_prefix(open).and_then(|t| t.strip_suffix(close)) {
            return inner.trim();
        }
    }
    text
}

/// Quotes and brackets that may open a value in a line, those that may close it, and the
/// clause punctuation that may follow it: structure only, never a word.
const OPENING: &[char] = &[
    '"', '\'', '`', '«', '\u{201c}', '\u{2018}', '(', '[', '{', '<',
];
const CLOSING: &[char] = &[
    '"', '\'', '`', '»', '\u{201d}', '\u{2019}', ')', ']', '}', '>',
];
const CLAUSE_END: &[char] = &['.', ',', ';', ':', '!', '?', '\u{2026}'];

/// Whether `part` occurs in `line` as whole whitespace-separated tokens: before it only the
/// line's start or whitespace, past opening quotes or brackets; after it only the line's end
/// or whitespace, past closing quotes, brackets and clause punctuation. So « sortie.txt »
/// ends before the sentence's period, a quoted value binds without its quotes and
/// « dir/rapport final.txt » binds whole, while a piece cut out of a token never does:
/// « txt » of « sortie.txt », « rapport.txt » of « exports/rapport.txt », « file.txt » of
/// « out-file.txt », « b » of « `a_b` », « user » of « user@example.org ». A boundary rule,
/// not a proof of meaning: when several whole-token spans could be the value (« dir/rapport »
/// and « dir/rapport final.txt »), the model's reading chooses and the human reviews it.
fn whole_part(line: &str, part: &str) -> bool {
    let separated = |c: Option<char>| c.is_none_or(char::is_whitespace);
    line.match_indices(part).any(|(at, _)| {
        let before = line[..at].chars().rev().find(|c| !OPENING.contains(c));
        let after = line[at + part.len()..]
            .chars()
            .find(|c| !CLOSING.contains(c) && !CLAUSE_END.contains(c));
        separated(before) && separated(after)
    })
}
