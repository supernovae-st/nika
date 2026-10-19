// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The monetary directives of a work request (R4 A6 · B15): recognition only, never admission.
//! A directive is an explicit act about the work's own money, in one of three closed forms: a
//! whole sentence or comma segment made of money words only (« Budget: $0 », « budget=0 »,
//! « Le budget est de 2 dollars », « --max-cost-usd 0 »), the phrase that ends a segment and
//! attaches to the work — a connector, its articles and a limit anchor (« … ./out.csv with a
//! budget of $1 », « … avec un plafond de 3 dollars ») or the limit anchor alone (« hello budget
//! 2 USD »), then the amount and its currency and nothing else — or the clause that ends a
//! segment, attaches to the work and says what one run may cost, the run as its subject (« … et
//! chaque exécution doit coûter au maximum 0,10 $ », « each run must cost at most $0.10 »). A
//! phrase attaches to the work when its head, the word before it past the determiners, is a path
//! or a file, a pronoun, a greeting, or a conjunction no relative clause governs. A segment that
//! holds none as written is read again with its asides apart (QA gauntlet q09: « … au maximum
//! 0,10 $ (budget max 0,10 $ par run) »). Everything else money-shaped is business data, by its
//! role in the request and never by a word or an observed field: a predicate over records
//! (« rows whose budget is 1500 USD », « rows where cost is under 5 USD », « rows with a budget
//! of 1500 USD », « jobs dont chaque exécution coûte 5 $ »), a business amount (« refund the cost
//! of 50 USD »), a record's price (« chaque article coûte 10 $ »), `cost` wherever it stands,
//! quoted text, a path, a file name. Run and gate lines keep [`super::parse`]'s whole-line
//! reading.

use std::ops::Range;

use super::{
    CONFLICTING, INVALID, ParsedMoney, amount_like, anchor, compact_literal, currency, data,
    filler, outside_quotes, parse, per_run, skeleton, token, without_currency,
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
        .flat_map(|segment| held(&visible, segment))
    {
        let span = offsets[chars.start]..offsets[chars.end];
        let money = match stated(&request[span.clone()]) {
            Err(INVALID) if mentions_the_ceiling(&request[span.clone()], &request[span.end..]) => {
                continue;
            }
            stated => stated?,
        };
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

/// Whether a segment that names a ceiling with no amount speaks about it rather than states it:
/// it is a question (« le plafond ? », « quel est le budget »), or the clause after it says who
/// will give it (« Le plafond, je le donnerai au moment de lancer »: live gauntlet r09,
/// 2026-10-17, refused as an invalid amount). A bare anchor after the work (« …, budget ») is
/// still refused, and so is any malformed amount.
fn mentions_the_ceiling(segment: &str, after: &str) -> bool {
    const ASKS: &[&str] = &[
        "quel", "quelle", "combien", "what", "which", "how", "where", "où", "pourquoi", "why",
    ];
    const SUBJECTS: &[&str] = &[
        "je", "j'", "on", "nous", "vous", "tu", "il", "elle", "i", "we", "you", "they",
    ];
    let words: Vec<String> = (segment.split_whitespace())
        .map(|w| token(w).to_lowercase())
        .collect();
    // Anything written as an amount, attached (`budget=oops`, `budget:inf`) or not, is a
    // statement to validate, never a mention.
    if words.iter().any(|w| {
        amount_like(w)
            || compact_literal(w).is_some()
            || w.contains(|c: char| c.is_ascii_digit() || matches!(c, '=' | ':' | '$'))
    }) {
        return false;
    }
    let question =
        after.trim_start().starts_with('?') || words.iter().any(|w| ASKS.contains(&w.as_str()));
    let next = (after.trim_start_matches(|c: char| c == ',' || c.is_whitespace()))
        .split_whitespace()
        .next()
        .map(|w| w.replace('\u{2019}', "'").to_lowercase());
    let given_later = after.starts_with(',')
        && next.is_some_and(|w| {
            SUBJECTS
                .iter()
                .any(|s| w == *s || (s.ends_with('\'') && w.starts_with(s)))
        });
    question || given_later
}

/// What one directive's own words state, the French copula read as a link (« Le budget est de
/// 2 dollars »): the whole-line reading keeps no copula of its own, so it never swallows a
/// relative clause. No directive holds `cost` as an anchor, so there it is the verb of what a run
/// may cost (« each run must cost at most $0.10 »), never a word that awaits an amount.
///
/// # Errors
/// As [`super::parse`].
pub(crate) fn stated(words: &str) -> Result<ParsedMoney, &'static str> {
    let linked = words
        .split(' ')
        .map(|w| {
            let link = ["est", "cost", "costs"]
                .iter()
                .any(|l| w.eq_ignore_ascii_case(l));
            if link { "" } else { w }
        })
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

/// One directive a segment holds: its char range, whether it marks a currency, its first anchor.
type Held = (Range<usize>, bool, Option<String>);

/// The directives a segment holds: its own as written, or, when it holds none, those of its
/// words around its asides and of each aside read apart (QA gauntlet q09: « … et chaque
/// exécution doit coûter au maximum 0,10 $ (budget max 0,10 $ par run) »). An aside states money
/// only with a currency: « (budget 2026) » names a document, never a ceiling.
fn held(visible: &[char], segment: Range<usize>) -> Vec<Held> {
    if let Some(found) = directive(visible, segment.clone()) {
        return vec![found];
    }
    // A directive the next clause follows without a comma (« Mets le plafond à 0,001 $ par run
    // et relance », live gauntlet r09, 2026-10-16) is read before the conjunction that opens it.
    if let Some(found) = (clause_breaks(visible, &segment).into_iter().rev())
        .find_map(|at| directive(visible, segment.start..at))
    {
        return vec![found];
    }
    let asides = asides(visible, &segment);
    if asides.is_empty() {
        return Vec::new();
    }
    let mut around = visible.to_vec();
    for aside in &asides {
        around[aside.clone()].fill(' ');
    }
    let mut found: Vec<Held> = directive(&around, segment).into_iter().collect();
    found.extend(
        (asides.iter())
            .filter_map(|aside| directive(visible, aside.start + 1..aside.end - 1))
            .filter(|(_, currency, _)| *currency),
    );
    found.sort_by_key(|(span, ..)| span.start);
    found
}

/// Where a conjunction opens another clause inside a segment (« et », « puis », « and »,
/// « then »), as the char index of the blank before it.
fn clause_breaks(chars: &[char], segment: &Range<usize>) -> Vec<usize> {
    let text: String = chars[segment.clone()].iter().collect();
    let mut out = Vec::new();
    let mut at = segment.start;
    for piece in text.split_inclusive(' ') {
        let word = piece.trim().to_lowercase();
        if at > segment.start && matches!(word.as_str(), "et" | "puis" | "and" | "then") {
            out.push(at.saturating_sub(1));
        }
        at += piece.chars().count();
    }
    out
}

/// The outermost asides of a segment, brackets included: a `(` that opens a word, to the `)`
/// that closes it. An unmatched bracket, or one inside a word (« ./notes(1).md »), opens none.
fn asides(chars: &[char], segment: &Range<usize>) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let (mut open, mut depth) = (segment.start, 0_usize);
    for at in segment.clone() {
        match chars[at] {
            '(' if depth > 0 => depth += 1,
            '(' if at == segment.start || chars[at - 1].is_whitespace() => {
                (open, depth) = (at, 1);
            }
            ')' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    out.push(open..at + 1);
                }
            }
            _ => {}
        }
    }
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
/// directive: an optional connector and its articles, a limit anchor and its « max », an
/// optional link (of, de, a colon, an equals sign), then the amount and its currency, and at
/// most the run it bounds (« par run ») — nothing else, no copula, no comparator, no verb. A
/// compact anchor (`budget=0.5USD`) or the ceiling flag holds its own.
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
    while at < read.len() && matches!(lower(at).as_str(), "max" | "maximum") {
        at += 1;
    }
    let mut currency = anchor.currency;
    if !anchor.amount {
        if at < read.len()
            && matches!(
                lower(at).as_str(),
                "of" | "de" | "" | "=" | "à" | "a" | "to" | "at"
            )
        {
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
    // The run the amount bounds may close the phrase (« 0,10 $ par run »): the ceiling is one
    // run's, never a rate of the work's data.
    let rest: Vec<String> = (at..read.len()).map(lower).collect();
    let bound = (0..=rest.len())
        .rev()
        .find(|&end| per_run(&rest[end..]))
        .unwrap_or(rest.len());
    match read.get(at..at + bound) {
        Some([]) => currency,
        Some([Some(unit)]) => unit.currency && !unit.amount,
        _ => false,
    }
}

/// What a run may cost, said with the run as the subject: each closed verb phrase, and whether it
/// makes the amount a limit on its own (« ne doit pas dépasser ») or needs a limit word beside it
/// (« doit coûter au maximum »): « chaque exécution coûte 0,10 $ » states a price, no ceiling.
const COSTS: &[(&[&str], bool)] = &[
    (&["doit", "coûter"], false),
    (&["devra", "coûter"], false),
    (&["peut", "coûter"], false),
    (&["coûte"], false),
    (&["coûtera"], false),
    (&["ne", "doit", "pas", "coûter", "plus", "de"], true),
    (&["ne", "devra", "pas", "coûter", "plus", "de"], true),
    (&["ne", "doit", "pas", "dépasser"], true),
    (&["ne", "devra", "pas", "dépasser"], true),
    (&["ne", "dépasse", "pas"], true),
    (&["ne", "dépassera", "pas"], true),
    (&["must", "cost"], false),
    (&["should", "cost"], false),
    (&["may", "cost"], false),
    (&["can", "cost"], false),
    (&["will", "cost"], false),
    (&["costs"], false),
    (&["must", "not", "cost", "more", "than"], true),
    (&["should", "not", "cost", "more", "than"], true),
    (&["must", "not", "exceed"], true),
    (&["should", "not", "exceed"], true),
    (&["may", "not", "exceed"], true),
    (&["cannot", "exceed"], true),
];

/// Limit words between a run's cost phrase and its amount (« au maximum », « at most »).
const LIMITS: &[&[&str]] = &[
    &["au", "maximum"],
    &["au", "max"],
    &["au", "plus"],
    &["maximum"],
    &["max"],
    &["pas", "plus", "de"],
    &["moins", "de"],
    &["at", "most"],
    &["no", "more", "than"],
    &["not", "more", "than"],
    &["less", "than"],
    &["up", "to"],
    &["under"],
    &["below"],
];

/// The run itself named as a subject: never a record (« chaque article »).
fn run_noun(word: &str) -> bool {
    matches!(
        word,
        "run"
            | "runs"
            | "exécution"
            | "exécutions"
            | "execution"
            | "executions"
            | "lancement"
            | "lancements"
    )
}

/// Whether `words` open with `said`.
fn opens(words: &[String], said: &[&str]) -> bool {
    words.len() >= said.len() && words.iter().zip(said).all(|(word, said)| word == said)
}

/// Whether the words from `from` to the end of the segment say what one run may cost: the run as
/// their subject past its determiners (« chaque exécution », « each run »), a closed cost phrase
/// and its limit (« doit coûter au maximum », « must cost at most », « ne doit pas dépasser »),
/// then the amount and its currency, and at most a closing « max » or the run it bounds — no
/// other word (QA gauntlet q09).
fn run_cost(read: &[Option<Word>], text: &[String], from: usize) -> bool {
    let words: Vec<String> = (text[from..].iter())
        .map(|t| token(t).to_lowercase())
        .collect();
    let mut at = words.iter().take_while(|w| determiner(w)).count();
    if !words.get(at).is_some_and(|w| run_noun(w)) {
        return false;
    }
    at += 1;
    let Some((said, limiting)) = COSTS.iter().find(|(said, _)| opens(&words[at..], said)) else {
        return false;
    };
    at += said.len();
    let limit = (LIMITS.iter().find(|limit| opens(&words[at..], limit))).map_or(0, |l| l.len());
    at += limit;
    let read_at = |at: usize| read.get(from + at).copied().flatten();
    let Some(amount) = read_at(at).filter(|w| w.amount && !w.anchor) else {
        return false;
    };
    at += 1;
    let unit = read_at(at).filter(|w| w.currency && !w.amount);
    at += usize::from(unit.is_some());
    let rest = &words[at..];
    let closing = matches!(rest, [w] if w == "max" || w == "maximum");
    (amount.currency || unit.is_some())
        && (rest.is_empty() || closing || per_run(rest))
        && (*limiting || limit > 0 || closing)
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
        || sets_a_limit(&word)
}

/// A verb that sets, changes or asks for the work's own limit (« Mets le plafond à 0,001 $ »,
/// « Set the ceiling to $0.10 », « je veux un budget de 2 $ »): the anchor it governs is the
/// work's ceiling, never a record's (live gauntlet r09, 2026-10-16: « Mets le plafond à 0,001 $
/// par run et relance » kept the earlier 0,10 $).
fn sets_a_limit(word: &str) -> bool {
    matches!(
        word,
        "mets"
            | "mettez"
            | "mettre"
            | "remets"
            | "remettez"
            | "remettre"
            | "passe"
            | "passez"
            | "fixe"
            | "fixez"
            | "règle"
            | "réglez"
            | "change"
            | "changez"
            | "baisse"
            | "baissez"
            | "monte"
            | "montez"
            | "augmente"
            | "augmentez"
            | "réduis"
            | "réduisez"
            | "diminue"
            | "diminuez"
            | "ajuste"
            | "ajustez"
            | "définis"
            | "définissez"
            | "veux"
            | "voudrais"
            | "souhaite"
            | "set"
            | "reset"
            | "lower"
            | "raise"
            | "put"
            | "increase"
            | "reduce"
            | "decrease"
            | "adjust"
            | "want"
    )
}

/// The directive a segment holds (its char range, whether it marks a currency, its first
/// anchor), if it holds one.
fn directive(chars: &[char], segment: Range<usize>) -> Option<Held> {
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
        (0..text.len()).find(|&i| {
            (closed(&read, &text, i) || run_cost(&read, &text, i)) && attached(&text, i)
        })?
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
