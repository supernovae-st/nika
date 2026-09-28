// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The monetary directives of a work request (R4 A6): recognition only, never admission. A
//! directive is a whole sentence or comma segment made of money words only that states an
//! amount beside an anchor or a currency (« Budget: $0 », « budget=0 », « budget 2 USD »,
//! « --max-cost-usd 0 »), or the trailing phrase of a segment that opens with a connector and
//! names an anchor, a currency and an amount (« … with a budget of $1 », « … avec un plafond de
//! 3 dollars »), or the trailing money words of a segment from their anchor on, currency
//! included (« hello budget 2 USD »). Everything else is data, whatever it looks like: a word
//! inside a business
//! clause (« rows whose budget is 15 », « price under $5 »), an anchor followed by a plain
//! word, quoted text, a path, a file name. Run, gate and consent lines keep [`super::parse`]'s
//! whole-line reading.

use std::ops::Range;

use super::{
    CONFLICTING, ParsedMoney, anchor, compact_literal, currency, data, filler, outside_quotes,
    parse, token, without_currency,
};

/// One monetary directive of a work request.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Directive {
    /// The directive's exact byte range in the request.
    pub span: Range<usize>,
    /// Whether it marks its amount as money with a currency or the ceiling flag.
    pub currency: bool,
    /// Its first monetary anchor word, lowercased (« budget »), when it names one.
    pub anchor: Option<String>,
}

/// The monetary directives of a work request, and what they state together.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct Directives {
    /// What the directives state together, read by [`super::parse`]; empty when there is none.
    pub money: ParsedMoney,
    /// Each directive, in the order the request states them.
    pub found: Vec<Directive>,
}

/// The monetary directives of a work request, and what they state together.
///
/// # Errors
/// As [`super::parse`] for a directive whose amount is malformed; a conflict when two
/// directives state different ceilings or defaults.
pub fn directives(request: &str) -> Result<Directives, &'static str> {
    let visible: Vec<char> = outside_quotes(request).chars().collect();
    let offsets: Vec<usize> = request
        .char_indices()
        .map(|(at, _)| at)
        .chain([request.len()])
        .collect();
    let mut all = Directives::default();
    for (chars, currency, anchor) in segments(&visible)
        .into_iter()
        .filter_map(|segment| directive(&visible, segment))
    {
        let span = offsets[chars.start]..offsets[chars.end];
        let money = parse(&request[span.clone()])?;
        let differ = |a: Option<f64>, b: Option<f64>| {
            a.zip(b).is_some_and(|(a, b)| a.to_bits() != b.to_bits())
        };
        if differ(all.money.amount, money.amount)
            || differ(all.money.replaced_default, money.replaced_default)
        {
            return Err(CONFLICTING);
        }
        all.money.amount = all.money.amount.or(money.amount);
        all.money.replaced_default = all.money.replaced_default.or(money.replaced_default);
        all.money.literal = all.money.literal.or(money.literal);
        all.found.push(Directive {
            span,
            currency,
            anchor,
        });
    }
    let covered = |at: usize| all.found.iter().any(|d| d.span.contains(&at));
    all.money.money_only = !all.found.is_empty()
        && visible.iter().zip(&offsets).all(|(c, at)| {
            covered(*at) || c.is_whitespace() || matches!(c, '.' | ',' | ';' | '!' | '?')
        });
    Ok(all)
}

/// Sentence and comma segments of the visible text, as char ranges: a period, a comma, `!` or
/// `?` ends one only before a blank or the end (a path, a decimal point or a decimal comma never
/// splits); a semicolon or a new line always does.
fn segments(chars: &[char]) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, &c) in chars.iter().enumerate() {
        let before_blank = chars.get(i + 1).is_none_or(|n| n.is_whitespace());
        if matches!(c, ';' | '\n') || (matches!(c, '.' | ',' | '!' | '?') && before_blank) {
            out.push(start..i);
            start = i + 1;
        }
    }
    out.push(start..chars.len());
    out
}

/// What one word of a candidate directive says; `None` for a word no directive holds.
#[derive(Clone, Copy, Default)]
struct Word {
    anchor: bool,
    currency: bool,
    amount: bool,
}

fn word(raw: &str) -> Option<Word> {
    let lower = raw.to_lowercase();
    let lower = lower.as_str();
    if let Some(flag) = lower.strip_prefix("--max-cost-usd") {
        let amount = flag.starts_with('=');
        return Some(Word {
            anchor: true,
            currency: true,
            amount,
        });
    }
    if let Some(amount) = compact_literal(raw) {
        let currency = amount.starts_with('$') || without_currency(raw) != raw;
        return Some(Word {
            anchor: raw.contains(['=', ':']),
            currency,
            amount: true,
        });
    }
    if anchor(lower) {
        return Some(Word {
            anchor: true,
            ..Word::default()
        });
    }
    let bare = without_currency(lower.strip_prefix('$').unwrap_or(lower));
    // An attached `$` commits the word to an amount: `$abc` is malformed money, never data.
    let committed = lower.len() > 1 && lower.starts_with('$') && !data(lower);
    if currency(lower) || committed || (bare.len() != lower.len() && amount_shaped(bare)) {
        let amount = !currency(lower);
        return Some(Word {
            currency: true,
            amount,
            ..Word::default()
        });
    }
    if amount_shaped(lower) {
        return Some(Word {
            amount: true,
            ..Word::default()
        });
    }
    (filler(lower) || article(lower) || money_link(lower)).then(Word::default)
}

/// Words that join money amounts or name the previous default inside a closed directive.
fn money_link(word: &str) -> bool {
    matches!(
        word,
        "and"
            | "et"
            | "replace"
            | "replaces"
            | "override"
            | "remplace"
            | "explicitly"
            | "explicitement"
            | "my"
            | "mon"
            | "default"
            | "défaut"
    )
}

/// An anchor and currency commit one unrecognized, non-path amount word to validation.
fn invalid_amount_word(read: &[Option<Word>], text: &[String]) -> bool {
    read.first().is_some_and(|w| w.is_some_and(|w| w.anchor))
        && read.last().is_some_and(|w| w.is_some_and(|w| w.currency))
        && read.iter().filter(|w| w.is_none()).count() == 1
        && read
            .iter()
            .zip(text)
            .all(|(w, t)| w.is_some() || !data(token(t)))
}

/// An amount as written, before validation: a digit, a sign or a point first, or a non-finite
/// spelling ([`super::parse`] refuses what is not a finite, nonnegative number).
fn amount_shaped(word: &str) -> bool {
    (word.starts_with(|c: char| c.is_ascii_digit() || matches!(c, '-' | '+' | '.')) && !data(word))
        || matches!(
            word,
            "nan" | "inf" | "infinity" | "-inf" | "+inf" | "-infinity"
        )
}

/// Connectors that open a trailing monetary phrase inside a clause.
fn connector(word: &str) -> bool {
    matches!(
        word,
        "with" | "within" | "under" | "for" | "avec" | "sous" | "pour"
    )
}

/// Articles a monetary phrase carries (« a budget », « un plafond »).
fn article(word: &str) -> bool {
    matches!(
        word,
        "a" | "an" | "the" | "un" | "une" | "le" | "la" | "max" | "maximum"
    )
}

/// The directive a segment holds (its char range, whether it marks a currency, its first
/// anchor), if it holds one.
fn directive(
    chars: &[char],
    segment: Range<usize>,
) -> Option<(Range<usize>, bool, Option<String>)> {
    let mut words: Vec<Range<usize>> = Vec::new();
    let mut at = segment.start;
    while at < segment.end {
        let begin = at;
        while at < segment.end && !chars[at].is_whitespace() {
            at += 1;
        }
        if at > begin {
            words.push(begin..at);
        }
        at += 1;
    }
    let text: Vec<String> = words
        .iter()
        .map(|w| chars[w.clone()].iter().collect())
        .collect();
    let read: Vec<Option<Word>> = text.iter().map(|t| word(token(t))).collect();
    let states = |from: usize, trailing: bool| {
        let tail = &read[from..];
        let all = || tail.iter().flatten();
        !tail.is_empty()
            && (tail.iter().all(Option::is_some) || invalid_amount_word(tail, &text[from..]))
            && all().any(|w| w.amount || w.anchor)
            && all().any(|w| w.anchor || w.currency)
            && (!trailing || (all().any(|w| w.anchor) && all().any(|w| w.currency)))
    };
    // The ceiling flag or an anchor opens a directive in place (« hello budget 2 USD »); a
    // connector opens a trailing phrase. Either way the phrase names an anchor and a currency.
    let opens = |i: usize| {
        let lower = token(&text[i]).to_lowercase();
        (read[i].is_some_and(|w| w.anchor) && states(i, true))
            || (connector(&lower) && states(i + 1, true))
    };
    let from = if states(0, false) {
        0
    } else {
        (0..text.len()).find(|&i| opens(i))?
    };
    let tail = &read[from..];
    let currency = tail.iter().flatten().any(|w| w.currency);
    let anchor = text[from..]
        .iter()
        .zip(tail)
        .find(|(_, w)| w.is_some_and(|w| w.anchor))
        .map(|(t, _)| {
            let t = token(t).to_lowercase();
            t.split(['=', ':']).next().unwrap_or_default().to_owned()
        });
    Some((
        words[from].start..words[words.len() - 1].end,
        currency,
        anchor,
    ))
}
