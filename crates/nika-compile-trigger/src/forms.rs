// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The two cadence forms beyond plain cron fields that the cadence grammar holds
//! (`nika-cadence`): the last day of a month (`L` in the day-of-month field) and an interval
//! of weeks anchored on a start date (`every N weeks from DATE HH:MM`). The words are read
//! exactly, EN and FR like the other projections: a form the words do not state whole (no
//! clock, no start date, a start date on another weekday than the one named, a word left
//! over) is no proposal, and the cadence question stays. The start date carries which weeks
//! fire, so it is never guessed; a date is read only in the ISO form `YYYY-MM-DD`.

/// The named weekdays, Sunday `0`, as the cadence grammar numbers them (EN and FR, folded).
const NAMED_DAYS: &[(&str, u32)] = &[
    ("sunday", 0),
    ("monday", 1),
    ("tuesday", 2),
    ("wednesday", 3),
    ("thursday", 4),
    ("friday", 5),
    ("saturday", 6),
    ("dimanche", 0),
    ("lundi", 1),
    ("mardi", 2),
    ("mercredi", 3),
    ("jeudi", 4),
    ("vendredi", 5),
    ("samedi", 6),
];

/// Words that frame a month end without stating anything else (« on the », « de chaque »).
const MONTH_END_FILLER: &str = "on the of each every a le la du de des chaque tous toutes les";

/// Words that introduce a start date (« from », « starting on », « à partir du », « dès le »).
const ANCHOR_WORDS: &str = "from starting beginning since on a partir compter depuis des du le";

/// Words that frame an interval of weeks around its count, unit and weekday.
const WEEKS_FILLER: &str = "every each on the a le la les tous toutes chaque week weeks \
    semaine semaines sur other second alternate alternating";

/// The placeholder word an ISO start date is replaced by before the words are split.
const ANCHOR: &str = "anchordate";

fn listed(list: &str, word: &str) -> bool {
    list.split_whitespace().any(|w| w == word)
}

/// A proposal in one of the two forms, when the folded phrase states one whole.
pub(super) fn read(phrase: &str) -> Option<String> {
    let (text, date) = take_iso_date(phrase);
    let text = text.replace(',', " ");
    if text
        .chars()
        .any(|c| !c.is_alphanumeric() && !c.is_whitespace() && c != ':')
    {
        return None;
    }
    let words = crate::reading::phrase_words(&text);
    let (time, used) = crate::reading::time_of_day(&words);
    let time = time?;
    let rest: Vec<&str> = words
        .iter()
        .enumerate()
        .filter(|(i, _)| !used.contains(i))
        .map(|(_, w)| *w)
        .collect();
    match date {
        Some(date) => anchored_weeks(&rest, &time, &date),
        None => month_end(&rest, &time),
    }
}

/// `M H L * *` when the words say the last day of every month (a clock time read apart).
fn month_end(words: &[&str], time: &str) -> Option<String> {
    let stated: Vec<&str> = words
        .iter()
        .copied()
        .filter(|w| !listed(MONTH_END_FILLER, w))
        .collect();
    let exact = |set: [&str; 3]| stated.len() == 3 && set.iter().all(|w| stated.contains(w));
    let before = |a: &str, b: &str| {
        let at = |w: &str| stated.iter().position(|s| *s == w);
        at(a).zip(at(b)).is_some_and(|(x, y)| x < y)
    };
    let month_end = (exact(["last", "day", "month"]) && before("last", "day"))
        || (exact(["dernier", "jour", "mois"]) && before("dernier", "jour"));
    if !month_end {
        return None;
    }
    let (hour, minute) = time.split_once(':')?;
    Some(format!(
        "{} {} L * *",
        minute.parse::<u8>().ok()?,
        hour.parse::<u8>().ok()?
    ))
}

/// `every N weeks from DATE HH:MM` when the words say an interval of weeks (or a named
/// weekday) and a start date introduced by « from », « starting », « à partir du »…, whose
/// weekday is the one they name, if they name one.
fn anchored_weeks(words: &[&str], time: &str, date: &str) -> Option<String> {
    let anchor = words.iter().position(|w| *w == ANCHOR)?;
    if anchor == 0 || anchor + 1 != words.len() || !listed(ANCHOR_WORDS, words[anchor - 1]) {
        return None;
    }
    let mut period = words[..anchor].to_vec();
    while period.last().is_some_and(|w| listed(ANCHOR_WORDS, w)) {
        period.pop();
    }
    let weeks = week_count(&period)?;
    let named: Vec<u32> = period.iter().filter_map(|w| weekday(w)).collect();
    let day = date_weekday(date)?;
    match named.as_slice() {
        [] => {}
        [one] if *one == day => {}
        _ => return None,
    }
    let left_over = period
        .iter()
        .any(|w| weekday(w).is_none() && !listed(WEEKS_FILLER, w) && count(w).is_none());
    if left_over {
        return None;
    }
    let (hour, minute) = time.split_once(':')?;
    let unit = if weeks == 1 { "week" } else { "weeks" };
    Some(format!(
        "every {weeks} {unit} from {date} {:02}:{:02}",
        hour.parse::<u8>().ok()?,
        minute.parse::<u8>().ok()?
    ))
}

/// The first `YYYY-MM-DD` in the folded phrase, and the phrase with that date replaced by
/// [`ANCHOR`]; the phrase unchanged when it holds none.
fn take_iso_date(phrase: &str) -> (String, Option<String>) {
    let bytes = phrase.as_bytes();
    let shaped = |i: usize| {
        let body = bytes.get(i..i + 10).is_some_and(|s| {
            s.iter().enumerate().all(|(k, b)| {
                if k == 4 || k == 7 {
                    *b == b'-'
                } else {
                    b.is_ascii_digit()
                }
            })
        });
        body && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric())
            && bytes.get(i + 10).is_none_or(|b| !b.is_ascii_alphanumeric())
    };
    match (0..bytes.len()).find(|i| shaped(*i)) {
        Some(i) => {
            let date = phrase[i..i + 10].to_owned();
            let text = format!("{} {ANCHOR} {}", &phrase[..i], &phrase[i + 10..]);
            (text, Some(date))
        }
        None => (phrase.to_owned(), None),
    }
}

/// How many weeks the words put between two runs: « every other monday » and « une semaine
/// sur deux » are two, « every 3 weeks » three, « every week » or a named weekday alone one;
/// `None` when the words name neither a week nor a weekday, or two counts.
fn week_count(period: &[&str]) -> Option<u32> {
    let unit = period
        .iter()
        .any(|w| matches!(*w, "week" | "weeks" | "semaine" | "semaines"));
    let named = period.iter().any(|w| weekday(w).is_some());
    if !unit && !named {
        return None;
    }
    let alternate = period
        .iter()
        .any(|w| matches!(*w, "other" | "second" | "alternate" | "alternating"));
    // « une semaine sur deux », « un lundi sur deux »: the count after `sur` is the period.
    let counted: Vec<u32> = match period.iter().position(|w| *w == "sur") {
        Some(sur) => period
            .get(sur + 1)
            .and_then(|w| count(w))
            .into_iter()
            .collect(),
        None => period.iter().filter_map(|w| count(w)).collect(),
    };
    let weeks = match (alternate, counted.as_slice()) {
        (true, []) => 2,
        (false, [n]) => *n,
        (false, []) => 1,
        _ => return None,
    };
    (1..=52).contains(&weeks).then_some(weeks)
}

/// A count word: digits or a number word from one to twelve (EN and FR).
fn count(word: &str) -> Option<u32> {
    const WORDS: &str = "one:1 two:2 three:3 four:4 five:5 six:6 seven:7 eight:8 nine:9 ten:10 \
        eleven:11 twelve:12 un:1 une:1 deux:2 trois:3 quatre:4 cinq:5 sept:7 huit:8 neuf:9 \
        dix:10 onze:11 douze:12";
    word.parse::<u32>().ok().or_else(|| {
        WORDS.split_whitespace().find_map(|pair| {
            let (name, value) = pair.split_once(':')?;
            (name == word).then(|| value.parse().ok()).flatten()
        })
    })
}

/// The weekday a folded word names (a plural included), Sunday `0`.
fn weekday(word: &str) -> Option<u32> {
    let stem = word.strip_suffix('s').unwrap_or(word);
    NAMED_DAYS
        .iter()
        .find_map(|(name, day)| (*name == word || *name == stem).then_some(*day))
}

/// The weekday of a `YYYY-MM-DD` civil date that exists, Sunday `0` (Sakamoto's method);
/// `None` for a day the calendar does not have (the 30th of February).
fn date_weekday(date: &str) -> Option<u32> {
    const OFFSETS: [i64; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    const LENGTHS: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let (year, rest) = date.split_once('-')?;
    let (month, day) = rest.split_once('-')?;
    let (year, month, day) = (
        year.parse::<i64>().ok()?,
        month.parse::<usize>().ok()?,
        day.parse::<i64>().ok()?,
    );
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let length = LENGTHS.get(month.checked_sub(1)?)? + i64::from(leap && month == 2);
    if !(1..=length).contains(&day) {
        return None;
    }
    let offset = *OFFSETS.get(month.checked_sub(1)?)?;
    let year = if month < 3 { year - 1 } else { year };
    u32::try_from((year + year / 4 - year / 100 + year / 400 + offset + day).rem_euclid(7)).ok()
}

#[cfg(test)]
mod tests {
    use super::read;
    use nika_compile_reader::hot::fold;

    fn proposal(phrase: &str) -> Option<String> {
        read(fold(phrase).trim().trim_end_matches(['.', '!', '?']))
    }

    #[test]
    fn the_last_day_of_every_month_at_a_time_is_l() {
        for (phrase, expected) in [
            ("the last day of every month at 18:00", "0 18 L * *"),
            ("on the last day of each month at 9:30", "30 9 L * *"),
            ("every month on the last day at 7 am", "0 7 L * *"),
            ("le dernier jour de chaque mois à 18h", "0 18 L * *"),
            ("le dernier jour du mois à 8h15", "15 8 L * *"),
        ] {
            assert_eq!(proposal(phrase).as_deref(), Some(expected), "{phrase}");
            grammatical(expected);
        }
    }

    /// The proposal parses in the cadence grammar its binding validates with.
    fn grammatical(expression: &str) {
        nika_cadence::registry::Cadence::parse(&format!("TZ=Europe/Paris {expression}"))
            .expect("one canonical scheduler grammar");
    }

    #[test]
    fn an_interval_of_weeks_with_its_start_date_is_anchored() {
        for (phrase, expected) in [
            (
                "every other Monday at 09:00 from 2026-10-05",
                "every 2 weeks from 2026-10-05 09:00",
            ),
            (
                "every 2 weeks on monday at 9:00, starting 2026-10-05",
                "every 2 weeks from 2026-10-05 09:00",
            ),
            (
                "every 3 weeks at 18:30 from 2026-10-07",
                "every 3 weeks from 2026-10-07 18:30",
            ),
            (
                "toutes les deux semaines le lundi à 9h à partir du 2026-10-05",
                "every 2 weeks from 2026-10-05 09:00",
            ),
            (
                "une semaine sur deux, le lundi à 9h, à partir du 2026-10-05",
                "every 2 weeks from 2026-10-05 09:00",
            ),
            (
                "un lundi sur deux à 9h dès le 2026-10-05",
                "every 2 weeks from 2026-10-05 09:00",
            ),
            (
                "every monday at 9 from 2026-10-05",
                "every 1 week from 2026-10-05 09:00",
            ),
        ] {
            assert_eq!(proposal(phrase).as_deref(), Some(expected), "{phrase}");
            grammatical(expected);
        }
    }

    #[test]
    fn a_form_not_stated_whole_is_no_proposal() {
        for phrase in [
            // no start date: which weeks fire is never guessed
            "every other monday at 09:00",
            "toutes les deux semaines à 9h",
            // a start date on another weekday than the one named
            "every other monday at 09:00 from 2026-10-06",
            // no clock
            "the last day of every month",
            "every other monday from 2026-10-05",
            // a date not in the ISO form
            "every other monday at 9 from 5 October 2026",
            // an impossible date
            "every other monday at 9 from 2026-02-30",
            // a word left over, or a vaguer period
            "every other monday at 9 except holidays from 2026-10-05",
            "at the end of every month at 18:00",
            "the last business day of every month at 18:00",
            // more than 52 weeks
            "every 60 weeks at 9 from 2026-10-05",
        ] {
            assert_eq!(proposal(phrase), None, "{phrase}");
        }
    }
}
