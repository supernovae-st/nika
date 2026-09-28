// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A period multiplier the cadence grammar cannot bind (R4 A5 · C1). Five cron fields hold a
//! day, a weekday, a named weekday at a time, or an hour or minute interval that divides its
//! day or hour; « every other monday », « every 2 weeks », « un lundi sur deux », « tous les
//! quinze jours », « biweekly », « twice a week » and « every 5 hours » state a period they
//! cannot hold. Such a phrase keeps no coarse label (never `weekly`): its cadence is asked,
//! never narrowed to the unit it names.

use nika_compile_reader::trigger_words::{DAILY, HOURLY, MINUTELY, MONTHLY, WEEKLY};

/// Ordinals that alternate their unit: « every other », « every second », « alternate »,
/// « semaines alternées », « jede zweite Woche », « cada otro lunes », « a settimane alterne ».
const ALTERNATION: &str = "other second alternate alternating alterne alternes alternee \
    alternees alterna alterni altro altra otro otra outro outra zweite zweiten zweiter zweites";

/// Number words from two up, with their value, in the trigger tables' languages.
const NUMBERS: &str = "two:2 three:3 four:4 five:5 six:6 seven:7 eight:8 nine:9 ten:10 \
    eleven:11 twelve:12 fourteen:14 fifteen:15 twenty:20 thirty:30 deux:2 trois:3 quatre:4 \
    cinq:5 sept:7 huit:8 neuf:9 dix:10 onze:11 douze:12 quinze:15 vingt:20 trente:30 dos:2 \
    tres:3 cuatro:4 cinco:5 seis:6 siete:7 ocho:8 nueve:9 diez:10 once:11 doce:12 catorce:14 \
    quince:15 veinte:20 treinta:30 zwei:2 drei:3 vier:4 funf:5 sechs:6 sieben:7 acht:8 neun:9 \
    zehn:10 elf:11 zwolf:12 vierzehn:14 zwanzig:20 dreissig:30 due:2 tre:3 quattro:4 cinque:5 \
    sei:6 sette:7 otto:8 nove:9 dieci:10 undici:11 dodici:12 quattordici:14 quindici:15 \
    venti:20 trenta:30 dois:2 duas:2 sete:7 oito:8 dez:10 doze:12 catorze:14 vinte:20 trinta:30";

/// One word that is already a multiple or a frequency: « biweekly », « quinzaine », « twice ».
const FUSED: &str = "biweekly fortnight fortnights fortnightly bimonthly semimonthly quinzaine \
    quinzaines bimensuel bimensuelle bihebdomadaire quincenal quinzenal quindicinale \
    bisettimanale zweiwochentlich vierzehntagig vierzehntaglich twice thrice zweimal";

/// Periods and counted occurrences the cadence tables do not spell (plurals among them).
const UNITS: &str = "tage wochen settimane semanas year years an ans annee annees ano anos \
    anno anni jahr jahre fois times veces volte vezes mal";

fn listed(list: &str, word: &str) -> bool {
    list.split_whitespace().any(|w| w == word)
}

/// A day-or-longer period (a day, a week or a named weekday, a month, a year) or a counted
/// occurrence (« fois », « times »), a plural `s` included.
fn long_unit(word: &str) -> bool {
    let named =
        |w: &str| [DAILY, WEEKLY, MONTHLY].iter().any(|t| t.contains(&w)) || listed(UNITS, w);
    !word.is_empty() && (named(word) || word.strip_suffix('s').is_some_and(named))
}

/// A count of two or more: digits, an ordinal (« 2nd », « 3e »), or a number word.
fn count(word: &str) -> Option<u32> {
    let digits = ["st", "nd", "rd", "th", "eme", "e"]
        .iter()
        .find_map(|suffix| word.strip_suffix(suffix))
        .unwrap_or(word);
    let spelled = || {
        NUMBERS.split_whitespace().find_map(|pair| {
            let (name, value) = pair.split_once(':')?;
            (name == word).then(|| value.parse().ok()).flatten()
        })
    };
    digits
        .parse::<u32>()
        .ok()
        .or_else(spelled)
        .filter(|n| *n >= 2)
}

/// A clock interval the hour or minute field cannot repeat evenly (« every 5 hours »).
fn uneven(n: u32, unit: &str) -> bool {
    let width = if HOURLY.contains(&unit) {
        24
    } else if MINUTELY.contains(&unit) {
        60
    } else {
        return false;
    };
    n > width || width % n != 0
}

/// Whether the trigger words state a period multiplier five cron fields cannot hold. A number
/// the time of day reads (« at 9 hours ») is the clock's, never an interval; a count before a
/// day-or-longer unit is a period even where a clock introducer precedes it (« alle 14 Tage »).
pub(super) fn unbindable(words: &[&str]) -> bool {
    let (_, clock) = super::time_of_day(words);
    let at = |i: usize| words.get(i).copied().unwrap_or_default();
    (0..words.len()).any(|i| {
        let (word, next, after) = (at(i), at(i + 1), at(i + 2));
        let post = listed(ALTERNATION, next)
            || (next == "sur" && count(after).is_some())
            || (next == "por" && after == "medio");
        listed(FUSED, word)
            || (matches!(word, "bi" | "semi") && long_unit(next))
            || (listed(ALTERNATION, word) && long_unit(next))
            || (long_unit(word) && post)
            || count(word)
                .is_some_and(|n| long_unit(next) || (!clock.contains(&i) && uneven(n, next)))
    })
}

#[cfg(test)]
mod tests {
    use super::super::{hot, phrase_words};
    use super::unbindable;

    fn reads(phrase: &str) -> bool {
        unbindable(&phrase_words(&hot::fold(phrase)))
    }

    #[test]
    fn a_period_five_cron_fields_cannot_hold_is_unbindable() {
        for phrase in [
            "every other monday at 09:00",
            "Every other Monday at 09:00",
            "every second week",
            "every 2nd Tuesday at 9",
            "alternate Mondays at 8",
            "every 2 weeks",
            "every two days at 9",
            "every 3 months",
            "every other day",
            "every 14 days",
            "biweekly on friday",
            "bi-weekly",
            "fortnightly at noon",
            "un lundi sur deux à 9h",
            "une semaine sur deux",
            "tous les deux jours",
            "toutes les deux semaines à 8h",
            "tous les quinze jours",
            "tous les 15 jours",
            "chaque quinzaine",
            "les semaines alternées",
            "deux fois par semaine",
            "twice a week",
            "3 times a week",
            "jede zweite Woche",
            "alle zwei Wochen",
            "alle 14 Tage",
            "cada dos semanas",
            "cada otro lunes",
            "lunes por medio",
            "a cada duas semanas",
            "ogni due settimane",
            "a settimane alterne",
            "the second Monday of each month",
            "every 5 hours",
            "every 90 minutes",
            "toutes les 7 heures",
        ] {
            assert!(reads(phrase), "{phrase}");
        }
    }

    #[test]
    fn a_period_the_cron_fields_hold_and_a_clock_are_never_a_multiple() {
        for phrase in [
            "every monday at 09:00",
            "every Monday at 9 am",
            "chaque lundi à 9h",
            "tous les lundis à 9h30",
            "every day at 8",
            "every weekday at 8:15",
            "every morning at 7",
            "every hour",
            "every 2 hours",
            "toutes les 2 heures",
            "toutes les deux heures",
            "every three hours",
            "every 15 minutes",
            "every 30 minutes",
            "on the 1st of each month",
            "le 2 de chaque mois à 10h",
            "every monday at 2 pm",
            "daily at 07:30",
            "each week, on monday",
            "every month at 9",
            "régulièrement",
            "from time to time",
            "when a ticket arrives",
            "every second",
        ] {
            assert!(!reads(phrase), "{phrase}");
        }
    }
}
