// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The monetary directives of a work request (R4 A6 · B15): recognition only, never admission.
//! A directive is an explicit act about the work's own money, in one of two closed forms: a
//! whole sentence or comma segment made of money words only (« Budget: $0 », « budget=0 »,
//! « Le budget est de 2 dollars », « --max-cost-usd 0 »), or the phrase that ends a segment and
//! attaches to the work — a connector, its articles and a limit anchor (« … ./out.csv with a
//! budget of $1 », « … avec un plafond de 3 dollars ») or the limit anchor alone (« hello budget
//! 2 USD »), then the amount and its currency and nothing else. A phrase attaches to the work
//! when its head, the word before it past the determiners, is a path or a file, a pronoun, a
//! greeting, or a conjunction no relative clause governs. Everything else money-shaped is
//! business data, by its role in the request and never by a word or an observed field: a
//! predicate over records (« rows whose budget is 1500 USD », « rows where cost is under 5 USD »,
//! « rows with a budget of 1500 USD »), a business amount (« refund the cost of 50 USD »),
//! `cost` wherever it stands, quoted text, a path, a file name. Run and gate lines keep
//! [`super::parse`]'s whole-line reading.

use std::ops::Range;

use super::{
    CONFLICTING, ParsedMoney, anchor, compact_literal, currency, data, filler, outside_quotes,
    parse, skeleton, token, without_currency,
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
        let money = stated(&request[span.clone()])?;
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

/// What one directive's own words state, the French copula read as a link (« Le budget est de
/// 2 dollars »): the whole-line reading keeps no copula of its own, so it never swallows a
/// relative clause.
///
/// # Errors
/// As [`super::parse`].
pub(crate) fn stated(words: &str) -> Result<ParsedMoney, &'static str> {
    let linked = words
        .split(' ')
        .map(|w| if w.eq_ignore_ascii_case("est") { "" } else { w })
        .collect::<Vec<_>>()
        .join(" ");
    parse(&linked)
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
        // `cost` measures what a record holds (« cost=5 »): it never limits the work's money.
        if lower
            .split_once(['=', ':'])
            .is_some_and(|(name, _)| name == "cost")
        {
            return None;
        }
        let currency = amount.starts_with('$') || without_currency(raw) != raw;
        return Some(Word {
            anchor: raw.contains(['=', ':']),
            currency,
            amount: true,
        });
    }
    if anchor(lower) {
        return (lower != "cost").then_some(Word {
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
    // A skeleton's name is the work, never an amount (« 01-hello »).
    if amount_shaped(lower) && !skeleton(lower) {
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

/// An anchor and currency commit one unrecognized, non-path word to validation when it stands
/// where the amount would (« budget nope dollars »). Beside an amount of its own the word
/// relates that amount to something (« budget over 1000 USD »): business data, never money.
fn invalid_amount_word(read: &[Option<Word>], text: &[String]) -> bool {
    read.first().is_some_and(|w| w.is_some_and(|w| w.anchor))
        && read.last().is_some_and(|w| w.is_some_and(|w| w.currency))
        && read.iter().filter(|w| w.is_none()).count() == 1
        && !read.iter().flatten().any(|w| w.amount)
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

/// Determiners between a phrase and its head (« the rows », « chaque client »).
fn determiner(word: &str) -> bool {
    matches!(
        word,
        "a" | "an"
            | "the"
            | "my"
            | "our"
            | "your"
            | "their"
            | "its"
            | "each"
            | "every"
            | "un"
            | "une"
            | "le"
            | "la"
            | "les"
            | "des"
            | "du"
            | "mon"
            | "ma"
            | "mes"
            | "son"
            | "sa"
            | "ses"
            | "notre"
            | "nos"
            | "votre"
            | "vos"
            | "leur"
            | "leurs"
            | "ce"
            | "cet"
            | "cette"
            | "ces"
            | "chaque"
    )
}

/// Words that open a relative clause: what follows them predicates over records.
fn relative(word: &str) -> bool {
    matches!(
        word,
        "where"
            | "whose"
            | "which"
            | "that"
            | "who"
            | "whom"
            | "wherein"
            | "où"
            | "dont"
            | "qui"
            | "que"
            | "lequel"
            | "laquelle"
            | "lesquels"
            | "lesquelles"
            | "auquel"
            | "auxquels"
            | "auxquelles"
            | "duquel"
            | "desquels"
            | "desquelles"
    )
}

/// A whole segment of money words is an explicit directive. The French copula links its words
/// there (« Le budget est de 2 dollars »), and only there: never inside a relative clause.
fn standalone(read: &[Option<Word>], text: &[String]) -> bool {
    let linked: Vec<Option<Word>> = read
        .iter()
        .zip(text)
        .map(|(w, t)| w.or_else(|| (token(t).to_lowercase() == "est").then(Word::default)))
        .collect();
    let all = || linked.iter().flatten();
    !linked.is_empty()
        && (linked.iter().all(Option::is_some) || invalid_amount_word(&linked, text))
        && all().any(|w| w.amount || w.anchor)
        && all().any(|w| w.anchor || w.currency)
}

/// Whether the words from `from` to the end of the segment make the closed shape of a trailing
/// directive: an optional connector and its articles, a limit anchor, an optional link (of, de,
/// a colon, an equals sign), then the amount and its currency — nothing else, no copula, no
/// comparator, no verb. A compact anchor (`budget=0.5USD`) or the ceiling flag holds its own.
fn closed(read: &[Option<Word>], text: &[String], from: usize) -> bool {
    let lower = |i: usize| token(&text[i]).to_lowercase();
    let mut at = from;
    if connector(&lower(at)) {
        at += 1;
        while at < read.len() && article(&lower(at)) {
            at += 1;
        }
    }
    let Some(anchor) = read.get(at).copied().flatten().filter(|w| w.anchor) else {
        return false;
    };
    at += 1;
    let mut currency = anchor.currency;
    if !anchor.amount {
        if at < read.len() && matches!(lower(at).as_str(), "of" | "de" | "" | "=") {
            at += 1;
        }
        let Some(amount) = read
            .get(at)
            .copied()
            .flatten()
            .filter(|w| w.amount && !w.anchor)
        else {
            return false;
        };
        currency |= amount.currency;
        at += 1;
    }
    match read.get(at..) {
        Some([]) => currency,
        Some([Some(unit)]) => unit.currency && !unit.amount,
        _ => false,
    }
}

/// Whether a trailing phrase opening at `from` attaches to the work rather than to business
/// data: its head — the word before it, past the determiners — is none, a path or a file, a
/// pronoun, a greeting or a consent word, a skeleton's name opening the segment, or a
/// conjunction no relative clause governs. « … stars and budget=0.5USD », « yes but budget 0
/// dollars » and « chain budget 0 USD » are the work's; « … rows whose status is open and
/// budget=1500USD » continues the predicate; « … rows with a budget of 1500 USD » restricts the
/// rows.
fn attached(text: &[String], from: usize) -> bool {
    let lower = |i: usize| token(&text[i]).to_lowercase();
    let Some(head) = (0..from).rev().find(|&i| !determiner(&lower(i))) else {
        return true;
    };
    let word = lower(head);
    let governed = || {
        (0..head)
            .rev()
            .map(lower)
            .take_while(|w| !data(w))
            .any(|w| relative(&w))
    };
    data(&word)
        || matches!(
            word.as_str(),
            "it" | "them"
                | "ça"
                | "cela"
                | "hello"
                | "hi"
                | "hey"
                | "bonjour"
                | "salut"
                | "yes"
                | "oui"
                | "ok"
        )
        || (matches!(
            word.as_str(),
            "and" | "et" | "but" | "mais" | "then" | "puis" | "also" | "aussi"
        ) && !governed())
        || (skeleton(&word) && (0..head).all(|i| determiner(&lower(i))))
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
    let from = if standalone(&read, &text) {
        0
    } else {
        (0..text.len()).find(|&i| closed(&read, &text, i) && attached(&text, i))?
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
