// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The cadence head of a sentence — « Chaque lundi matin, … », « Jeden Montagmorgen schick mir
//! … » — cut off before the clause is read: where it ends, with or without a comma, and what
//! it records (the plan's trigger, or a read step after « à partir de »). A recurrence stated
//! without its cadence (« régulièrement », « from time to time ») is a trigger too, whether it
//! leads the sentence or sits inside a clause. Beside `lexicon.rs` at the 1,500-line file cap.
use super::cues::{
    CADENCE_HEAD_PREFIXES, CADENCE_WORDS, CLOCK_SUFFIXES, HEAD_FILLERS, START_DATE_WORDS,
    TRIGGER_PREFIXES,
};
use super::{Reading, normalize, number_word};
use crate::plan::{Op, Step};
use crate::{hot, words};

/// Where a cadence head ends and its clause begins: `(head_end, rest_start)` as byte offsets
/// of the lowercase sentence. The head runs from the prefix over the cadence words (a named
/// day, a part of the day, a period, a German day-part compound), the small words between
/// them and the clock tokens, and ends after its last cadence word or clock token — with or
/// without a comma: « Jeden Montagmorgen schick mir … », « Every Monday morning send me … »,
/// « Tous les jours à 18h, … ». A prefix no cadence word follows keeps the first comma as its
/// end (an event, a supplied document: « Dès qu'un ticket arrive, … », « Pour chaque fichier
/// de ./x, … »); none at all is no head.
fn head_bounds(body_lower: &str, prefix: &str) -> Option<(usize, usize)> {
    cadence_bounds(body_lower, prefix)
        .or_else(|| body_lower.find(',').map(|comma| (comma, comma + 1)))
}

/// The cadence part of a head (`head_bounds` without its comma fallback): `None` when no cadence
/// word or clock token follows the prefix. A start date the head anchors (« every other monday
/// at 9 from 2026-10-05 », « … à 9h à partir du 2026-10-05 ») ends it, even after the comma
/// that closes the rest (« every 2 weeks on monday at 9:00, starting 2026-10-05, … »).
fn cadence_bounds(body_lower: &str, prefix: &str) -> Option<(usize, usize)> {
    let tail = body_lower.get(prefix.len()..).unwrap_or_default();
    let mut at = prefix.len();
    let mut end = None;
    let mut anchored = false;
    let mut closed_on_head = false;
    for piece in tail.split_inclusive(' ') {
        let folded = hot::fold(piece);
        let word = folded.trim_matches(|c: char| !c.is_alphanumeric());
        let cadence =
            CADENCE_WORDS.lines().any(|c| c == word) || words::day_part_compound(word).is_some();
        let clock = end.is_some() && (clock_token(word) || CLOCK_SUFFIXES.contains(&word));
        let date = end.is_some() && anchored && iso_date(word);
        let filler =
            HEAD_FILLERS.contains(&word) || clock_token(word) || number_word(word).is_some();
        if !word.is_empty() && !cadence && !clock && !date && !filler {
            break;
        }
        anchored |= START_DATE_WORDS.contains(&word);
        let word_end = at + piece.trim_end_matches(|c: char| !c.is_alphanumeric()).len();
        at += piece.len();
        if cadence || clock || date {
            end = Some(word_end);
        }
        if piece.trim_end().ends_with(',') {
            closed_on_head = cadence || clock || date;
            break;
        }
    }
    let mut end = end?;
    if closed_on_head && let Some(date_end) = body_lower.get(at..).and_then(start_date_after_comma)
    {
        end = at + date_end;
    }
    let rest = body_lower.get(end..).unwrap_or_default();
    let skipped = rest.len() - rest.trim_start_matches([' ', ',']).len();
    Some((end, end + skipped))
}

/// A civil date in the ISO form `YYYY-MM-DD` (whether the calendar has it is the compiler's
/// to judge when it binds the cadence).
fn iso_date(word: &str) -> bool {
    word.len() == 10
        && word.bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
}

/// After the comma that closes a head, the start date that still belongs to it (« starting
/// 2026-10-05 », « à partir du 2026-10-05 »): the byte length up to the date's end, when only
/// start-date words and fillers come before it, one start-date word at least, and a comma, a
/// full stop or the end closes it. `None` for anything else (« , starting with the oldest
/// ticket, … », a range « , from 2026-10-05 to 2026-12-31, … »).
fn start_date_after_comma(text: &str) -> Option<usize> {
    let mut at = 0;
    let mut anchored = false;
    for piece in text.split_inclusive(' ') {
        let folded = hot::fold(piece);
        let word = folded.trim_matches(|c: char| !c.is_alphanumeric());
        if anchored && iso_date(word) {
            let after = text.get(at + piece.len()..).unwrap_or_default();
            let closed = piece.trim_end().ends_with([',', '.']) || after.trim().is_empty();
            return closed
                .then(|| at + piece.trim_end_matches(|c: char| !c.is_alphanumeric()).len());
        }
        if !word.is_empty() && !START_DATE_WORDS.contains(&word) && !HEAD_FILLERS.contains(&word) {
            return None;
        }
        anchored |= START_DATE_WORDS.contains(&word);
        at += piece.len();
        if piece.trim_end().ends_with(',') {
            return None;
        }
    }
    None
}

/// A clock token: `9`, `9:30`, `9h`, `9h30`, `18h`, `9am`, `9pm`.
fn clock_token(word: &str) -> bool {
    let word = word
        .strip_suffix("am")
        .or_else(|| word.strip_suffix("pm"))
        .unwrap_or(word);
    word.chars().any(|c| c.is_ascii_digit())
        && word
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, ':' | 'h'))
}

/// The sentence with its cadence or supplied-document head cut off: the head is recorded as
/// the plan's trigger (the first one) or, after « à partir de … », as a read step; the rest of
/// the sentence comes back in its original case and in lowercase. No head: the sentence as is.
pub(super) fn cut_head<'a>(
    sentence: &'a str,
    text: &str,
    reading: &mut Reading,
) -> (&'a str, String) {
    if let Some(prefix) = CADENCE_HEAD_PREFIXES.iter().find(|p| text.starts_with(*p))
        && let Some((head_end, rest_start)) = cadence_bounds(text, prefix)
    {
        if reading.plan.trigger.is_none() {
            reading.plan.trigger = text.get(..head_end).map(str::to_owned);
        }
        return rest(sentence, text, rest_start);
    }
    let Some(prefix) = TRIGGER_PREFIXES.iter().find(|p| text.starts_with(*p)) else {
        return recurrence_head(sentence, text, reading);
    };
    let Some((head_end, rest_start)) = head_bounds(text, prefix) else {
        // « Every so often summarize … »: the prefix closes no head, the recurrence does.
        return recurrence_head(sentence, text, reading);
    };
    let head = text.get(..head_end).unwrap_or_default().to_owned();
    if prefix.starts_with("à partir de")
        || prefix.starts_with("a partire da")
        || prefix.starts_with("a partir de")
        || prefix.starts_with("from the")
        || prefix.starts_with("starting from")
    {
        let detail = head
            .get(prefix.len()..)
            .unwrap_or_default()
            .trim()
            .to_owned();
        reading.plan.push_step(Step {
            op: Op::Read,
            evidence: sentence.to_owned(),
            detail,
            categories: Vec::new(),
        });
    } else if reading.plan.trigger.is_none() {
        reading.plan.trigger = Some(head);
    }
    rest(sentence, text, rest_start)
}

/// What a sentence says after its head, in its original case and in lowercase.
fn rest<'a>(sentence: &'a str, text: &str, rest_start: usize) -> (&'a str, String) {
    let rest_lower = text.get(rest_start..).unwrap_or_default().trim().to_owned();
    let body = match normalize(sentence).find(&rest_lower) {
        Some(pos) => sentence.get(pos..).map_or(sentence, str::trim),
        None => sentence,
    };
    (body, rest_lower)
}

/// A sentence led by a recurrence without its cadence (« Régulièrement, fais-moi … »,
/// « Regularly summarize … ») has that phrase for its head: recorded as the plan's trigger
/// (the first one), and the clause is read without it, as after a cadence head.
fn recurrence_head<'a>(sentence: &'a str, text: &str, reading: &mut Reading) -> (&'a str, String) {
    let Some((0, end)) = words::recurrence(text) else {
        return (sentence, text.to_owned());
    };
    if reading.plan.trigger.is_none() {
        reading.plan.trigger = text.get(..end).map(str::to_owned);
    }
    let after = text.get(end..).unwrap_or_default();
    let skipped = after.len() - after.trim_start_matches([' ', ',']).len();
    rest(sentence, text, end + skipped)
}

/// A recurrence stated inside a clause (« Fais-moi un rapport des trucs importants
/// régulièrement », « Résume ./notes.md de temps en temps dans … ») is the plan's trigger
/// once no head recorded one: the work repeats, and when it runs is the human's to say. The
/// clause keeps its words; its operations read as they did.
pub(super) fn record_recurrence(intent: &str, reading: &mut Reading) {
    if reading.plan.trigger.is_some() {
        return;
    }
    let lower = intent.to_lowercase();
    reading.plan.trigger = words::recurrence(&lower)
        .and_then(|(start, end)| lower.get(start..end))
        .map(str::to_owned);
}

/// The quantifiers that open a sentence-final cadence (FR · EN, lowercase): « … chaque
/// lundi », « … tous les matins », « … toutes les deux heures », « … every Monday », « …
/// each morning ».
const TAIL_QUANTIFIERS: &[&str] = &["chaque ", "tous les ", "toutes les ", "every ", "each "];

/// The words after which a quantified period is the complement of what precedes it, never a
/// schedule (folded): a grouping (« les ventes de chaque mois », « pour chaque mois », « sales
/// of every month », « for each day »), a negation or an exception (« mais pas chaque lundi »,
/// « not every Monday », « sauf chaque lundi »).
const NOT_A_SCHEDULE: &[&str] = &[
    "de", "du", "des", "par", "pour", "sur", "dans", "a", "pas", "plus", "jamais", "sauf", "sans",
    "ni", "of", "for", "per", "by", "in", "on", "from", "across", "within", "during", "not",
    "never", "except", "without",
];

/// A clause body with its sentence-final cadence cut off (« Résume ./notes.md dans
/// ./out/resume.md tous les matins », « … every Monday at 9 »): the cadence goes to `tails`
/// for [`settle_tails`], and the clause is read without it, as after a head — the program's
/// bytes carry no cadence. A body that ends otherwise comes back as is.
pub(super) fn cut_tail<'a>(body: &'a str, body_lower: &str, tails: &mut Vec<String>) -> &'a str {
    let Some((start, end)) = tail_bounds(body_lower) else {
        return body;
    };
    tails.push(body_lower.get(start..end).unwrap_or_default().to_owned());
    // The original body ends where its lowercase does: the same number of bytes goes.
    let body = body.trim_end();
    let cut = body.len().checked_sub(body_lower.len() - start);
    match cut.and_then(|cut| Some((body.get(..cut)?, body.get(cut..)?))) {
        Some((kept, tail)) if normalize(tail) == body_lower.get(start..).unwrap_or_default() => {
            kept.trim_end_matches([' ', ','])
        }
        _ => body,
    }
}

/// A clause's own final cadence (« Read ./tickets.json every weekday at 8, keep … »), cut from
/// its clause as [`cut_tail`] cuts a sentence's, under the same guards, and settled with the
/// tails. A clause that opens on a prohibition keeps it: the cadence is the ban's scope.
pub(super) fn cut_clause_tail<'a>(clause: &'a str, tails: &mut Vec<String>) -> &'a str {
    let lower = normalize(clause);
    if lower.len() != clause.len() || super::opens_negated(&lower) {
        return clause;
    }
    cut_tail(clause, &lower, tails)
}

/// Where a sentence-final cadence runs in a clause body, as `(start, end)` byte offsets of the
/// lowercase body. It opens on a quantifier that is not the body's first word (that one is a
/// head, cut before), runs as a head does (`head_bounds`: cadence words, the small words
/// between them, clock tokens), names a period, and nothing but closing punctuation follows
/// it. A quantifier after a grouping, negation or exception word, inside quotes (« Écris
/// "réunion chaque lundi" »), or after a colon that opens content (« Écris dans note.txt :
/// réunion chaque lundi »; a URL's `://` and a clock's `9:30` open none) opens no schedule.
fn tail_bounds(body_lower: &str) -> Option<(usize, usize)> {
    let mut starts: Vec<(usize, &str)> = TAIL_QUANTIFIERS
        .iter()
        .flat_map(|quantifier| {
            body_lower
                .match_indices(quantifier)
                .map(move |(at, _)| (at, *quantifier))
        })
        .filter(|(at, _)| {
            *at > 0
                && body_lower
                    .get(..*at)
                    .is_some_and(|before| before.ends_with(' '))
        })
        .collect();
    starts.sort_unstable();
    starts.into_iter().find_map(|(start, quantifier)| {
        let phrase = body_lower.get(start..)?;
        let (end, _) = head_bounds(phrase, quantifier)?;
        let closed = phrase
            .get(end..)?
            .trim_matches(|c: char| c.is_whitespace() || matches!(c, '.' | '!' | '?' | '…'))
            .is_empty();
        let before = body_lower.get(..start)?;
        let folded = hot::fold(before);
        let previous = folded
            .split(|c: char| !c.is_alphanumeric())
            .rfind(|word| !word.is_empty())
            .unwrap_or_default();
        let schedule = closed
            && names_a_period(phrase.get(..end)?)
            && !NOT_A_SCHEDULE.contains(&previous)
            && !quoted(before)
            && !before.contains(": ");
        schedule.then_some((start, start + end))
    })
}

/// Whether a phrase names a period the cadence tables know: a named day, a part of the day, a
/// period, a German day-part compound — never only clock tokens or small words.
fn names_a_period(phrase: &str) -> bool {
    phrase.split(' ').any(|piece| {
        let folded = hot::fold(piece);
        let word = folded.trim_matches(|c: char| !c.is_alphanumeric());
        CADENCE_WORDS.lines().any(|c| c == word) || words::day_part_compound(word).is_some()
    })
}

/// Whether text ends inside quotes: an odd count of straight double quotes or backticks, or
/// more opening than closing guillemets or curly double quotes; a mark after a backslash is
/// content (« "she said \"hi\"" »). The unnamed-destination law reads its connector through
/// the same guard.
pub(crate) fn quoted(before: &str) -> bool {
    quote_states(before, false)
        .last()
        .is_some_and(|(_, _, _, after)| after)
}

/// The text with its quoted content blanked, marks included, every character replaced by
/// spaces of its own byte length so each offset still names the same place: a law that reads
/// words by substring (a waiver, a bypass, an indecision) reads only what is stated outside
/// quotes.
pub(crate) fn unquoted(text: &str) -> String {
    quote_states(text, true)
        .flat_map(|(_, c, before, after)| {
            if before || (matches!(c, '"' | '\'' | '`' | '«' | '“') && after) {
                std::iter::repeat_n(' ', c.len_utf8())
            } else {
                std::iter::repeat_n(c, 1)
            }
        })
        .collect()
}

/// Each character's byte offset and quote state before/after it. All readers share this
/// transition law; only literal readers include apostrophes, and only the mask blanks marks.
fn quote_states(
    text: &str,
    single_literals: bool,
) -> impl Iterator<Item = (usize, char, bool, bool)> + '_ {
    // Track quote state once. Re-reading the entire prefix for every character
    // made a long conversational input quadratic before it could reach a model.
    let mut double = false;
    let mut tick = false;
    let mut angled: i64 = 0;
    let mut curved: i64 = 0;
    let mut single = false;
    let mut previous: Option<char> = None;
    let mut chars = text.char_indices().peekable();
    std::iter::from_fn(move || {
        let (at, c) = chars.next()?;
        let before = double || tick || angled > 0 || curved > 0 || single;
        if previous != Some('\\') {
            match c {
                '"' => double = !double,
                '`' => tick = !tick,
                '«' => angled += 1,
                '»' => angled -= 1,
                '“' => curved += 1,
                '”' => curved -= 1,
                _ => {}
            }
        }
        if single_literals && c == '\'' {
            let next = chars.peek().map(|(_, ch)| *ch);
            single = if single {
                !(previous.is_some_and(|p| !p.is_whitespace())
                    && next.is_none_or(|n| !n.is_alphanumeric()))
            } else {
                previous.is_none_or(|p| p.is_whitespace() || matches!(p, '(' | '[' | ':'))
                    && next.is_some_and(|n| !n.is_whitespace())
            };
        }
        let after = double || tick || angled > 0 || curved > 0 || single;
        previous = Some(c);
        Some((at, c, before, after))
    })
}

/// Whether a byte position of the text lies inside quoted content: the quotes [`quoted`]
/// counts, or a single-quoted literal (« write 'hello' »). A straight single quote opens at
/// the start or after a space or an opening bracket, before a non-space, and closes after a
/// non-space, before a space, a punctuation mark or the end; an apostrophe inside a word
/// (« don't », « n'écris », « l'envoie ») neither opens nor closes. What lies inside quotes
/// is what the workflow writes, reads or matches, never an instruction to it.
pub(crate) fn quoted_at(text: &str, pos: usize) -> bool {
    text.is_char_boundary(pos)
        && quote_states(text, true)
            .take_while(|(at, ..)| *at < pos)
            .last()
            .is_some_and(|(_, _, _, after)| after)
}

/// Settle the sentence-final cadences once every sentence is read. The widest becomes the
/// plan's trigger when no head recorded one, or when the head is a recurrence stated without
/// its cadence (« Régulièrement, … chaque lundi »: the cadence completes it); a head that
/// already says it (« Chaque lundi, … chaque lundi ») stays. Anything else — an event, a
/// distribution, a sequence or another cadence beside it, or two cadences that differ — is
/// two triggers one workflow cannot both start on: named as unknown work
/// ([`super::TWO_TRIGGERS`]), never one of them silently kept.
pub(super) fn settle_tails(tails: &[String], reading: &mut Reading) {
    let Some(widest) = tails.iter().max_by_key(|tail| tail.len()) else {
        return;
    };
    let says = |outer: &str, inner: &str| hot::fold(outer).contains(&hot::fold(inner));
    if let Some(other) = tails.iter().find(|tail| !says(widest, tail)) {
        reading.plan.unknowns.push(two_triggers(other, widest));
        return;
    }
    let head = reading.plan.trigger.clone();
    match head.as_deref() {
        None => reading.plan.trigger = Some(widest.clone()),
        Some(head) if words::recurrence(head) == Some((0, head.len())) => {
            reading.plan.trigger = Some(widest.clone());
        }
        Some(head) if says(head, widest) => {}
        Some(head) => reading.plan.unknowns.push(two_triggers(head, widest)),
    }
}

/// Two triggers as unknown work: both named, neither kept over the other.
fn two_triggers(first: &str, second: &str) -> String {
    format!(
        "{}: `{first}` and `{second}` — one workflow starts on one of them; say which, or ask for one workflow per trigger",
        super::TWO_TRIGGERS
    )
}

#[cfg(test)]
mod head_tests {
    use super::head_bounds;
    use crate::lexicon::read;

    #[test]
    fn a_schedule_no_comma_closes_is_the_head_in_german_portuguese_and_english() {
        for (intent, head, rest) in [
            (
                "Jeden Montagmorgen schick mir eine Zusammenfassung der offenen Tickets aus ./tickets.json",
                "jeden montagmorgen",
                "schick mir",
            ),
            (
                "Toda segunda-feira de manhã, envie-me um resumo dos tickets abertos de ./tickets.json",
                "toda segunda-feira de manhã",
                "envie-me",
            ),
            (
                "Every Monday morning at 9 pm send me the open tickets from ./tickets.json",
                "every monday morning at 9 pm",
                "send me",
            ),
            (
                "Tous les jours à 18h, lis ./tickets.json",
                "tous les jours à 18h",
                "lis",
            ),
            ("Every 2 hours, fetch ./x.json", "every 2 hours", "fetch"),
        ] {
            let reading = read(intent);
            assert_eq!(reading.plan.trigger.as_deref(), Some(head), "{intent}");
            let lower = intent.to_lowercase();
            let prefix = crate::lexicon::cues::TRIGGER_PREFIXES
                .iter()
                .find(|p| lower.starts_with(*p))
                .expect("a prefix");
            let (head_end, rest_start) = head_bounds(&lower, prefix).expect("a head");
            assert_eq!(&lower[..head_end], head, "{intent}");
            assert!(
                lower[rest_start..].starts_with(rest),
                "{intent}: {}",
                &lower[rest_start..]
            );
        }
    }

    #[test]
    fn a_start_date_or_a_month_end_stays_in_the_head() {
        for (intent, head) in [
            (
                "Every two weeks from 2026-10-05 at 09:00, read ./tickets.json",
                "every two weeks from 2026-10-05 at 09:00",
            ),
            (
                "Every 2 weeks on Monday at 9:00, starting 2026-10-05, read ./tickets.json",
                "every 2 weeks on monday at 9:00, starting 2026-10-05",
            ),
            (
                "Toutes les deux semaines le lundi à 9h à partir du 2026-10-05, lis ./tickets.json",
                "toutes les deux semaines le lundi à 9h à partir du 2026-10-05",
            ),
            (
                "Every month on the last day at 18:00, read ./tickets.json",
                "every month on the last day at 18:00",
            ),
            (
                "On the last day of every month at 18:00, read ./tickets.json",
                "on the last day of every month at 18:00",
            ),
            (
                "Le dernier jour de chaque mois à 18h, lis ./tickets.json",
                "le dernier jour de chaque mois à 18h",
            ),
        ] {
            let reading = read(intent);
            assert_eq!(reading.plan.trigger.as_deref(), Some(head), "{intent}");
            assert!(
                steps(intent).iter().all(|(_, detail, _)| {
                    !detail.contains("2026")
                        && !detail.contains("last")
                        && !detail.contains("dernier")
                }),
                "the clause reads without its head: {:?}",
                steps(intent)
            );
        }
        // No cadence word after the month-end words is no head; words after the comma that
        // are not a start date alone (a range, another clause) stay out of it.
        let sprint = "On the last day of the sprint, send ./summary.md to the team";
        assert_eq!(read(sprint).plan.trigger, None, "{sprint}");
        for intent in [
            "Every monday at 9, starting with the oldest ticket, summarize ./tickets.json",
            "Every monday at 9, from 2026-10-05 to 2026-12-31, summarize ./tickets.json",
        ] {
            assert_eq!(
                read(intent).plan.trigger.as_deref(),
                Some("every monday at 9"),
                "{intent}"
            );
        }
    }

    #[test]
    fn a_prefix_no_cadence_word_follows_keeps_the_comma_and_none_is_no_head() {
        let event = "Dès qu'un ticket arrive, rédige un accusé de réception dans ./out/accuse.md";
        assert_eq!(
            read(event).plan.trigger.as_deref(),
            Some("dès qu'un ticket arrive"),
            "the elided event head"
        );
        let plain = "Quand un ticket arrive, rédige un accusé de réception dans ./out/accuse.md";
        assert_eq!(
            read(plain).plan.trigger.as_deref(),
            Some("quand un ticket arrive")
        );
        let bare = "Jeden Montag";
        assert_eq!(head_bounds(&bare.to_lowercase(), "jeden "), Some((12, 12)));
        assert_eq!(
            head_bounds("wenn ein ticket ankommt schreib", "wenn "),
            None
        );
    }

    fn steps(intent: &str) -> Vec<(&'static str, String, String)> {
        read(intent)
            .plan
            .steps
            .iter()
            .map(|s| (s.op.word(), s.detail.clone(), s.evidence.clone()))
            .collect()
    }

    fn step(op: &'static str, detail: &str, evidence: &str) -> (&'static str, String, String) {
        (op, detail.to_owned(), evidence.to_owned())
    }

    #[test]
    fn a_recurrence_leading_its_sentence_is_the_head_and_the_clause_reads_without_it() {
        // The steps are the ones the sentence without its head reads (pinned binary, 0.120.3).
        for (intent, head, expected) in [
            (
                "Régulièrement, fais-moi un rapport des trucs importants",
                "régulièrement",
                vec![step(
                    "draft",
                    "un rapport des trucs importants",
                    "fais-moi un rapport des trucs importants",
                )],
            ),
            (
                "Regularly summarize ./notes.md into ./out/summary.md",
                "regularly",
                vec![
                    step(
                        "read",
                        "./notes.md",
                        "summarize ./notes.md into ./out/summary.md",
                    ),
                    step(
                        "draft",
                        "./notes.md into ./out/summary.md",
                        "summarize ./notes.md into ./out/summary.md",
                    ),
                ],
            ),
            (
                "De temps en temps, lis ./notes.md",
                "de temps en temps",
                vec![step("read", "./notes.md", "lis ./notes.md")],
            ),
        ] {
            let reading = read(intent);
            assert_eq!(reading.plan.trigger.as_deref(), Some(head), "{intent}");
            assert!(
                reading.unresolved.is_empty(),
                "{intent}: {:?}",
                reading.unresolved
            );
            assert_eq!(steps(intent), expected, "{intent}");
        }
    }

    #[test]
    fn a_recurrence_inside_a_clause_is_the_trigger_and_the_clause_keeps_its_words() {
        for (intent, phrase, expected) in [
            (
                "Fais-moi un rapport des trucs importants régulièrement",
                "régulièrement",
                vec![step(
                    "draft",
                    "un rapport des trucs importants régulièrement",
                    "Fais-moi un rapport des trucs importants régulièrement",
                )],
            ),
            (
                "Résume ./notes.md de temps en temps dans ./out/resume.md",
                "de temps en temps",
                vec![
                    step(
                        "read",
                        "./notes.md",
                        "Résume ./notes.md de temps en temps dans ./out/resume.md",
                    ),
                    step(
                        "draft",
                        "./notes.md de temps en temps dans ./out/resume.md",
                        "Résume ./notes.md de temps en temps dans ./out/resume.md",
                    ),
                ],
            ),
        ] {
            assert_eq!(
                read(intent).plan.trigger.as_deref(),
                Some(phrase),
                "{intent}"
            );
            assert_eq!(steps(intent), expected, "{intent}");
        }
        assert_eq!(
            read("Summarize ./notes.md into ./out/summary.md on a regular basis")
                .plan
                .trigger
                .as_deref(),
            Some("on a regular basis")
        );
    }

    #[test]
    fn a_stated_head_keeps_the_trigger_and_no_recurrence_word_is_no_trigger() {
        assert_eq!(
            read("Chaque lundi, résume ./notes.md régulièrement dans ./out/resume.md")
                .plan
                .trigger
                .as_deref(),
            Some("chaque lundi")
        );
        for intent in [
            "Résume ./notes.md dans ./out/resume.md",
            "Résume ./notes.md dans ./out/resume.md en vérifiant la régularité des dépenses",
            "Fais-moi un rapport régulier des ventes de ./ventes.csv",
            "Summarize ./notes.md into ./out/summary.md and flag irregular entries",
        ] {
            assert_eq!(read(intent).plan.trigger, None, "{intent}");
        }
        let text = "Lis-le À intervalles réguliers, ou Every So Often";
        let (start, end) = crate::words::recurrence(text).expect("a recurrence");
        assert_eq!(&text[start..end], "À intervalles réguliers");
        assert_eq!(crate::words::recurrence("régularité · irregularly"), None);
    }

    /// The unknowns of a reading that name two triggers.
    fn two_triggers(intent: &str) -> Vec<String> {
        read(intent)
            .plan
            .unknowns
            .into_iter()
            .filter(|unknown| unknown.starts_with(crate::lexicon::TWO_TRIGGERS))
            .collect()
    }

    #[test]
    fn a_sentence_final_cadence_is_the_trigger_and_the_clause_reads_as_without_it() {
        // Before this law each of these was READY once with no trigger (the e6bc576b witness).
        for (intent, cadence, plain) in [
            (
                "Fais-moi un rapport des trucs importants chaque lundi",
                "chaque lundi",
                "Fais-moi un rapport des trucs importants",
            ),
            (
                "Fais-moi un rapport des trucs importants, chaque lundi.",
                "chaque lundi",
                "Fais-moi un rapport des trucs importants",
            ),
            (
                "Résume ./notes.md dans ./out/resume.md tous les matins",
                "tous les matins",
                "Résume ./notes.md dans ./out/resume.md",
            ),
            (
                "Résume ./notes.md dans ./out/resume.md chaque lundi à 9h",
                "chaque lundi à 9h",
                "Résume ./notes.md dans ./out/resume.md",
            ),
            (
                "Résume ./notes.md dans ./out/resume.md tous les lundis",
                "tous les lundis",
                "Résume ./notes.md dans ./out/resume.md",
            ),
            (
                "Résume ./notes.md dans ./out/resume.md toutes les deux heures",
                "toutes les deux heures",
                "Résume ./notes.md dans ./out/resume.md",
            ),
            (
                "Summarize ./notes.md into ./out/summary.md every monday",
                "every monday",
                "Summarize ./notes.md into ./out/summary.md",
            ),
            (
                "Summarize ./notes.md into ./out/summary.md every day at 18:00",
                "every day at 18:00",
                "Summarize ./notes.md into ./out/summary.md",
            ),
            (
                "Summarize ./notes.md into ./out/summary.md each morning",
                "each morning",
                "Summarize ./notes.md into ./out/summary.md",
            ),
        ] {
            let reading = read(intent);
            assert_eq!(reading.plan.trigger.as_deref(), Some(cadence), "{intent}");
            assert!(
                reading.plan.unknowns.is_empty(),
                "{intent}: {:?}",
                reading.plan.unknowns
            );
            assert_eq!(steps(intent), steps(plain), "{intent}");
        }
    }

    #[test]
    fn a_grouping_a_quotation_a_negation_or_an_adjective_is_no_schedule() {
        for intent in [
            "Résume les ventes de chaque mois de ./ventes.csv dans ./out/resume.md",
            "Summarize the sales of every month from ./ventes.csv into ./out/summary.md",
            "Résume ./notes.md dans ./out/resume.md pour chaque mois",
            "Résume les tickets de ./tickets.json par jour dans ./out/resume.md",
            "Summarize the tickets of ./tickets.json per day into ./out/summary.md",
            "Écris \"réunion chaque lundi\" dans ./out/note.txt",
            "Écris dans ./out/note.txt le texte « réunion chaque lundi »",
            "Écris dans ./out/note.txt : réunion chaque lundi",
            "Ne résume pas ./notes.md chaque lundi",
            "Don't summarize ./notes.md every monday",
            "Résume ./notes.md dans ./out/resume.md, mais pas chaque lundi",
            "Fais-moi un rapport hebdomadaire des trucs importants",
            "Résume tous les fichiers de ./notes dans ./out/resume.md",
            "Résume ./notes.md dans ./out/resume.md chaque fois qu'un ticket arrive",
        ] {
            assert_eq!(read(intent).plan.trigger, None, "{intent}");
            assert!(two_triggers(intent).is_empty(), "{intent}");
        }
    }

    #[test]
    fn a_head_that_says_the_cadence_keeps_it_and_two_triggers_are_named_never_one_kept() {
        for (intent, trigger) in [
            (
                "Chaque lundi, résume ./notes.md dans ./out/resume.md chaque lundi",
                "chaque lundi",
            ),
            (
                "Chaque lundi à 9h, résume ./notes.md dans ./out/resume.md chaque lundi",
                "chaque lundi à 9h",
            ),
            // The stated cadence completes a recurrence that said none.
            (
                "Régulièrement, résume ./notes.md dans ./out/resume.md chaque lundi",
                "chaque lundi",
            ),
            (
                "Tous les lundis, résume ./notes.md dans ./out/resume.md",
                "tous les lundis",
            ),
        ] {
            let reading = read(intent);
            assert_eq!(reading.plan.trigger.as_deref(), Some(trigger), "{intent}");
            assert!(
                reading.plan.unknowns.is_empty(),
                "{intent}: {:?}",
                reading.plan.unknowns
            );
        }
        // Either order, a head beside a different cadence, two different cadences: both named.
        for (intent, head, both) in [
            (
                "Quand un ticket arrive, rédige un accusé de réception dans ./out/accuse.md chaque lundi",
                Some("quand un ticket arrive"),
                ["quand un ticket arrive", "chaque lundi"],
            ),
            (
                "Chaque lundi, résume ./notes.md dans ./out/resume.md tous les matins",
                Some("chaque lundi"),
                ["chaque lundi", "tous les matins"],
            ),
            (
                "Résume ./notes.md dans ./out/resume.md chaque lundi. Quand un ticket arrive, lis-le.",
                Some("quand un ticket arrive"),
                ["quand un ticket arrive", "chaque lundi"],
            ),
            (
                "Résume ./a.md dans ./out/a.md chaque lundi. Résume ./b.md dans ./out/b.md tous les matins.",
                None,
                ["chaque lundi", "tous les matins"],
            ),
        ] {
            assert_eq!(read(intent).plan.trigger.as_deref(), head, "{intent}");
            let named = two_triggers(intent);
            assert_eq!(named.len(), 1, "{intent}: {named:?}");
            for phrase in both {
                assert!(
                    named[0].contains(&format!("`{phrase}`")),
                    "{intent}: {named:?}"
                );
            }
        }
    }
}

#[cfg(test)]
mod continuous_quote_tests {
    use super::{quoted_at, unquoted};

    // Independent pre-refactor oracle; do not delegate to the new transition iterator.
    fn historical_quoted(before: &str) -> bool {
        let escaped = |at: usize| before.get(..at).is_some_and(|b| b.ends_with('\\'));
        let count = |c: char| {
            before
                .match_indices(c)
                .filter(|(at, _)| !escaped(*at))
                .count()
        };
        count('"') % 2 == 1
            || count('`') % 2 == 1
            || count('«') > count('»')
            || count('“') > count('”')
    }

    fn historical_quoted_at(text: &str, pos: usize) -> bool {
        let Some(before) = text.get(..pos) else {
            return false;
        };
        if historical_quoted(before) {
            return true;
        }
        let mut open = false;
        let mut prev: Option<char> = None;
        let mut chars = text.char_indices().peekable();
        while let Some((at, c)) = chars.next() {
            if at >= pos {
                break;
            }
            if c == '\'' {
                let next = chars.peek().map(|(_, n)| *n);
                if open {
                    open = !(prev.is_some_and(|p| !p.is_whitespace())
                        && next.is_none_or(|n| !n.is_alphanumeric()));
                } else {
                    open = prev.is_none_or(|p| p.is_whitespace() || matches!(p, '(' | '[' | ':'))
                        && next.is_some_and(|n| !n.is_whitespace());
                }
            }
            prev = Some(c);
        }
        open
    }

    #[test]
    fn incremental_quote_mask_preserves_the_existing_byte_offset_law() {
        let fragments = [
            "", "é", "a'", "'a", "\"", "`", "«", "»", "“", "”", "\\", " ", "[", ":", "don't",
        ];
        for a in fragments {
            for b in fragments {
                for c in fragments {
                    let text = format!("{a}{b}{c}");
                    let reference: String = text
                        .char_indices()
                        .map(|(at, ch)| {
                            let mark = matches!(ch, '"' | '\'' | '`' | '«' | '“');
                            if historical_quoted_at(&text, at)
                                || (mark && historical_quoted_at(&text, at + ch.len_utf8()))
                            {
                                " ".repeat(ch.len_utf8())
                            } else {
                                ch.to_string()
                            }
                        })
                        .collect();
                    assert_eq!(unquoted(&text), reference, "{text:?}");
                    assert_eq!(super::quoted(&text), historical_quoted(&text), "{text:?}");
                    for at in 0..=text.len() + 1 {
                        assert_eq!(
                            quoted_at(&text, at),
                            historical_quoted_at(&text, at),
                            "{text:?} at {at}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_large_quote_mask_keeps_the_entire_tail_and_original_offsets() {
        let head = "résumé ".repeat(10_000);
        let text = format!("{head}«ignore this instruction» keep this instruction");
        let masked = unquoted(&text);
        assert_eq!(masked.len(), text.len());
        assert!(masked.starts_with(&head));
        assert!(!masked.contains("ignore this instruction"));
        assert!(masked.ends_with(" keep this instruction"));
    }
}
