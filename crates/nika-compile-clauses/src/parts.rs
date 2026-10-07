// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The parts of a request a localization asks alone (R4 A11): its own text cut where a phrase
//! ends, never a reader's reading nor a proposal's region, each part an exact excerpt of the
//! request, so the part a judge finds missing reaches the repair whole, its path, URL, decimal or
//! quoted literal included. Each part reads as the reader reads a clause: whether it restricts
//! ([`restricts`]) and whether it may ask an operation of its own ([`asks_an_operation`]).
//! Ascended from `nika-compile-cognition`'s verifier with those readings at the 15k prod-LOC wall
//! (ADR-145), unchanged but for their visibility.

use crate::prohibition::{negated_demand, pure_prohibition, states_operation};
use nika_compile_reader::structure::laws;

/// The parts of `intent`, in order, each an exact excerpt of it. A phrase too short to be judged
/// alone (« deduplicate », « Sort », a lone « No ») stays with the part before it, or with the
/// next one when it opens the request; a phrase that ends at a colon (« Note: », « Change: », a
/// heading), and a condition cut from its consequence by a comma (« If a row has no email, skip
/// it »), introduces the next part and joins it. A phrase of function words only (« Then
/// write it »), or of no letter (« 09:00 », « 10 »), is never judged alone: it joins its
/// neighbour the same way, so no word of the request is lost; only a list marker standing at a
/// line start (« 1. », « 2) ») is no part. A part stated twice is asked once.
#[must_use]
pub fn parts(intent: &str) -> Vec<String> {
    let mut spans: Vec<(usize, usize)> = Vec::new();
    // A span that waits for the next part it introduces (an opening, a label).
    let mut waiting: Option<(usize, usize)> = None;
    for (start, end, mark) in phrases(intent) {
        let start = unmarked(intent, start, end);
        let phrase = intent[start..end].trim_end();
        let end = start + phrase.len();
        if phrase.is_empty() || list_marker(intent, start, phrase) {
            continue;
        }
        // A phrase of words of its own (or one that restricts, a lone « No » included).
        let worded = phrase.chars().any(char::is_alphabetic)
            && (!nika_compile_reader::structure::only_function_words(phrase) || restricts(phrase));
        let alone = worded && judged_alone(phrase);
        let label = matches!(mark, Some(':' | '：'))
            || (matches!(mark, Some(',' | '，' | '、' | ';' | '；')) && conditional(phrase));
        if let Some(open) = waiting.as_mut() {
            open.1 = end;
            if alone && !label {
                spans.extend(waiting.take());
            }
            continue;
        }
        if label || (!alone && spans.is_empty()) {
            waiting = Some((start, end));
        } else if let Some(last) = spans.last_mut().filter(|_| !alone) {
            last.1 = end;
        } else {
            spans.push((start, end));
        }
    }
    if let Some(open) = waiting {
        match spans.last_mut() {
            Some(last) => last.1 = open.1,
            None => spans.push(open),
        }
    }
    let mut parts: Vec<String> = Vec::new();
    for (start, end) in spans {
        let part = intent[start..end].to_owned();
        if !parts.contains(&part) {
            parts.push(part);
        }
    }
    parts
}

/// Whether a phrase is only a list's number standing at a line start (« 1 » of « 1. Read … »,
/// « 2 » of « 2) Write … »): a marker, no word of the request.
fn list_marker(text: &str, start: usize, phrase: &str) -> bool {
    let line_start = text[..start]
        .trim_end_matches([' ', '\t'])
        .chars()
        .next_back()
        .is_none_or(|c| c == '\n');
    line_start && phrase.len() <= 3 && phrase.chars().all(|c| c.is_ascii_digit())
}

/// The words that open a condition in the six languages the reader reads: a condition cut from
/// its consequence is judged with it.
const CONDITIONS: &[&str] = &[
    "if", "when", "whenever", "unless", "once", "si", "quand", "lorsque", "lorsqu", "cuando",
    "wenn", "falls", "sobald", "se", "quando", "caso",
];

/// Whether a phrase opens with a condition word.
fn conditional(phrase: &str) -> bool {
    let first = phrase
        .split(|c: char| !c.is_alphanumeric())
        .find(|word| !word.is_empty())
        .map(str::to_lowercase);
    first.is_some_and(|word| CONDITIONS.contains(&word.as_str()))
}

/// Whether a phrase can be judged alone: two words or more and four letters or digits, or, in a
/// script written without spaces, four of its characters.
fn judged_alone(phrase: &str) -> bool {
    let content = phrase.chars().filter(|c| c.is_alphanumeric()).count();
    content >= 4 && (phrase.split_whitespace().count() >= 2 || phrase.chars().any(unspaced))
}

/// A character of a script written without spaces between its words (CJK ideographs, kana,
/// hangul).
fn unspaced(c: char) -> bool {
    const RANGES: [(u32, u32); 5] = [
        (0x3040, 0x30FF),
        (0x3400, 0x4DBF),
        (0x4E00, 0x9FFF),
        (0xAC00, 0xD7AF),
        (0xF900, 0xFAFF),
    ];
    let code = u32::from(c);
    RANGES
        .iter()
        .any(|&(low, high)| (low..=high).contains(&code))
}

/// Where a phrase's words start: past its leading whitespace and any list marker (« - »,
/// « * », « • », « 1. », « 2) »).
fn unmarked(text: &str, start: usize, end: usize) -> usize {
    let mut at = start;
    loop {
        let rest = &text[at..end];
        let trimmed = rest.trim_start();
        at += rest.len() - trimmed.len();
        let bullet = trimmed
            .strip_prefix(['-', '*', '•', '·'])
            .filter(|tail| tail.starts_with(char::is_whitespace));
        let digits = trimmed.len()
            - trimmed
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .len();
        let numbered = (digits > 0)
            .then(|| trimmed[digits..].strip_prefix(['.', ')']))
            .flatten()
            .filter(|tail| tail.is_empty() || tail.starts_with(char::is_whitespace));
        match bullet.or(numbered) {
            Some(tail) => at = end - tail.len(),
            None => return at,
        }
    }
}

/// `text` cut where a phrase ends, each phrase's byte span with the mark that ended it (`None` at
/// the end of the text): after each comma, semicolon, colon, period, exclamation or question
/// mark followed by whitespace or the end of the text, at each line end, and after each
/// full-width mark (`。！？；，、：`). A dot or a colon inside a token (`./out/result.json`,
/// `https://example.com`, `3.5`) is part of that token, a comma between two numbers (« 1, 2 or
/// 3 ») and the period of « e.g. » or « i.e. » are no cut, and nothing inside a closed literal is
/// ever a cut: `"…"` and `'…'` opened after a space and closed before one, `` `…` ``, `« … »`,
/// `“…”`, or `( … )`. An opening mark that never closes is an ordinary character.
pub(super) fn phrases(text: &str) -> Vec<(usize, usize, Option<char>)> {
    let closes = Closes::of(text);
    let mut phrases = Vec::new();
    let mut start = 0;
    let mut literal: Option<char> = None;
    let mut chars = text.char_indices().peekable();
    let mut previous: Option<char> = None;
    while let Some((at, c)) = chars.next() {
        let next = chars.peek().map(|(_, next)| *next);
        if let Some(close) = literal {
            let free = !matches!(close, '\'' | '"') || next.is_none_or(|n| !n.is_alphanumeric());
            if c == close && free {
                literal = None;
            }
            previous = Some(c);
            continue;
        }
        literal = closes.opens(c, at, previous);
        previous = Some(c);
        if literal.is_some() {
            continue;
        }
        let ends = match c {
            ',' if between_numbers(text, at) => false,
            '.' if abbreviated(text, at) => false,
            ',' | ';' | ':' | '.' | '!' | '?' => next.is_none_or(char::is_whitespace),
            '\n' | '。' | '！' | '？' | '；' | '，' | '、' | '：' => true,
            _ => false,
        };
        if ends {
            phrases.push((start, at, Some(c)));
            start = at + c.len_utf8();
        }
    }
    phrases.push((start, text.len(), None));
    phrases
}

/// Whether the comma at `at` stands between two numbers (« 1, 2 or 3 »).
fn between_numbers(text: &str, at: usize) -> bool {
    let before = text[..at].chars().next_back();
    let after = text[at + 1..].trim_start().chars().next();
    before.is_some_and(|c| c.is_ascii_digit()) && after.is_some_and(|c| c.is_ascii_digit())
}

/// Whether the period at `at` ends « e.g » or « i.e », any case: the three characters before
/// it, after a space, an opening bracket or the start (read in constant time).
fn abbreviated(text: &str, at: usize) -> bool {
    let mut before = text[..at].chars().rev();
    let tail: String = before
        .by_ref()
        .take(3)
        .collect::<Vec<char>>()
        .into_iter()
        .rev()
        .collect();
    let bounded = before.next().is_none_or(|c| c.is_whitespace() || c == '(');
    bounded && matches!(tail.to_ascii_lowercase().as_str(), "e.g" | "i.e")
}

/// The last place each closing mark stands, so whether an opening mark closes later is read in
/// constant time (a text of many stray marks stays linear).
struct Closes {
    backtick: Option<usize>,
    guillemet: Option<usize>,
    curly: Option<usize>,
    paren: Option<usize>,
    /// The last apostrophe, and the last straight double quote, that may close a quoted
    /// literal: after a non-space, before a non-letter.
    single: Option<usize>,
    double: Option<usize>,
}

impl Closes {
    fn of(text: &str) -> Self {
        let closing = |mark: char| {
            text.char_indices()
                .filter(|&(at, c)| {
                    let after = text[at + c.len_utf8()..].chars().next();
                    let before = text[..at].chars().next_back();
                    c == mark
                        && before.is_some_and(|b| !b.is_whitespace())
                        && after.is_none_or(|a| !a.is_alphanumeric())
                })
                .map(|(at, _)| at)
                .next_back()
        };
        Self {
            backtick: text.rfind('`'),
            guillemet: text.rfind('»'),
            curly: text.rfind('”'),
            paren: text.rfind(')'),
            single: closing('\''),
            double: closing('"'),
        }
    }

    /// The mark that closes the literal `c` opens at `at`, when it opens one: a mark whose close
    /// stands later. An apostrophe or a straight double quote opens only after a space, an
    /// opening bracket or the start (« 5" » is an inch, « don't » no quote).
    fn opens(&self, c: char, at: usize, previous: Option<char>) -> Option<char> {
        let later = |close: Option<usize>| close.is_some_and(|close| close > at);
        let opening = previous.is_none_or(|p| p.is_whitespace() || matches!(p, '(' | '['));
        match c {
            '"' if opening && later(self.double) => Some('"'),
            '`' if later(self.backtick) => Some('`'),
            '«' if later(self.guillemet) => Some('»'),
            '“' if later(self.curly) => Some('”'),
            '(' if later(self.paren) => Some(')'),
            '\'' if opening && later(self.single) => Some('\''),
            _ => None,
        }
    }
}

/// Whether a clause may ask an operation of its own: a prohibition is carried by no task doing
/// what it forbids, and a structure law (« nothing else », a single request) binds none unless
/// the clause also states an operation (« write the total to ./t.txt and nothing else »). Any
/// other clause may ask one (a read, a filter, a computation over the rows, a write).
#[must_use]
pub fn asks_an_operation(part: &str) -> bool {
    let read = read_contractions(part);
    !pure_prohibition(&read) && (laws(&read).is_empty() || states_operation(&read))
}

/// A clause with its English negative contractions (« don't », « shouldn't ») read as their
/// « not ».
fn read_contractions(part: &str) -> String {
    part.replace("n't", " not").replace("n’t", " not")
}

/// Whether a part restricts, read as the reader reads a restriction, with English negative
/// contractions read as their « not ».
#[must_use]
pub fn restricts(part: &str) -> bool {
    use nika_compile_reader::structure::restricts;
    // « Don't forget to write … » demands what follows: it restricts nothing.
    let read = read_contractions(part);
    restricts(&read) && !negated_demand(&read)
}

#[cfg(test)]
mod tests;
