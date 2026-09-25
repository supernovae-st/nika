// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A bounded exact projection beside the coarse cadence label. This reads the entire
//! trigger phrase, never just the first period. Five cron fields are a proposal in the
//! existing cadence grammar; the binding supplies the zone and validates with its owner.
//! No default hour/day, elapsed-duration scheduler, calendar arithmetic or authority.

use super::{hot, phrase_words, time_of_day};

/// FR/EN daily/weekday/named weekday with a stated clock, or a whole clock interval.
/// Unsupported, conflicting and incomplete phrases all keep their words but no cron.
pub(super) fn fields(phrase: &str) -> Option<String> {
    let folded = hot::fold(phrase);
    let phrase = folded.trim().trim_end_matches([',', '.', '!', '?']);
    if phrase
        .chars()
        .any(|c| !c.is_alphanumeric() && !c.is_whitespace() && c != ':')
    {
        return None;
    }
    let words = phrase_words(phrase);
    let cut = words.iter().position(|w| matches!(*w, "at" | "a"));
    let (period, time) = match cut {
        Some(cut) => {
            let tail = &words[cut..];
            let (time, used) = time_of_day(tail);
            // Named clocks (noon/midi) consume only the name, not its introducer.
            if tail
                .iter()
                .enumerate()
                .any(|(i, _)| i != 0 && !used.contains(&i))
            {
                return None;
            }
            (&words[..cut], Some(time?))
        }
        None => (words.as_slice(), None),
    };
    let period = match period {
        ["every" | "each" | "chaque", rest @ ..] | ["tous" | "toutes", "les", rest @ ..] => rest,
        ["daily" | "hourly" | "minutely" | "quotidien" | "quotidienne" | "quotidiennement"] => {
            period
        }
        _ => return None,
    };
    if let Some(cron) = interval(period) {
        return time.is_none().then_some(cron);
    }
    let [day] = period else { return None };
    let days = match *day {
        "day" | "days" | "daily" | "morning" | "mornings" | "evening" | "evenings" | "night"
        | "nights" | "jour" | "jours" | "quotidien" | "quotidienne" | "quotidiennement"
        | "matin" | "matins" | "soir" | "soirs" => "*",
        "weekday" | "weekdays" | "workday" | "workdays" => "1-5",
        "monday" | "lundi" | "lundis" => "1",
        "tuesday" | "mardi" | "mardis" => "2",
        "wednesday" | "mercredi" | "mercredis" => "3",
        "thursday" | "jeudi" | "jeudis" => "4",
        "friday" | "vendredi" | "vendredis" => "5",
        "saturday" | "samedi" | "samedis" => "6",
        "sunday" | "dimanche" | "dimanches" => "0",
        _ => return None,
    };
    let time = time?;
    let (hour, minute) = time.split_once(':')?;
    Some(format!(
        "{} {} * * {days}",
        minute.parse::<u8>().ok()?,
        hour.parse::<u8>().ok()?
    ))
}

/// Cron steps reset at the field boundary. Accept only divisors so the local-clock
/// interval remains constant across the boundary (5 hours would silently become 4).
/// The zero phase, local clock and DST semantics are shown for activation consent.
fn interval(words: &[&str]) -> Option<String> {
    let (step, unit) = match words {
        [unit] => (1, *unit),
        [number, unit] => (
            match *number {
                "one" | "un" | "une" => 1,
                "two" | "deux" => 2,
                _ => number.parse::<u8>().ok()?,
            },
            *unit,
        ),
        _ => return None,
    };
    let (width, hourly) = match unit {
        "hour" | "hours" | "hourly" | "heure" | "heures" => (24, true),
        "minute" | "minutes" | "minutely" => (60, false),
        _ => return None,
    };
    if step == 0 || step > width || width % step != 0 {
        return None;
    }
    let field = if step == 1 {
        "*".to_owned()
    } else {
        format!("*/{step}")
    };
    Some(if hourly {
        format!("0 {field} * * *")
    } else {
        format!("{field} * * * *")
    })
}

#[cfg(test)]
mod tests {
    use super::fields;

    #[test]
    fn exact_fields_are_in_the_existing_grammar_and_keep_day_and_interval() {
        for (phrase, expected) in [
            ("every Tuesday at 09:15", "15 9 * * 2"),
            ("chaque vendredi à 18h30", "30 18 * * 5"),
            ("tous les mardis à 9h", "0 9 * * 2"),
            ("every day at noon", "0 12 * * *"),
            ("chaque matin à 8h", "0 8 * * *"),
            ("every hour", "0 * * * *"),
            ("every 2 hours", "0 */2 * * *"),
            ("toutes les 2 heures", "0 */2 * * *"),
            ("toutes les deux heures", "0 */2 * * *"),
            ("every 15 minutes", "*/15 * * * *"),
        ] {
            assert_eq!(fields(phrase).as_deref(), Some(expected), "{phrase}");
            nika_cadence::registry::Cadence::parse(&format!("TZ=Europe/Paris {expected}"))
                .expect("one canonical scheduler grammar");
        }
    }

    #[test]
    fn no_partial_period_or_clock_and_no_invented_hour_or_weekday() {
        for phrase in [
            "every Tuesday",
            "chaque matin",
            "weekly at 9",
            "every 2 days at 9",
            "every 5 hours",
            "every 0 hours",
            "every 2 hours at 9",
            "every 2.5 hours",
            "every -2 hours",
            "Tuesday at 9",
            "every day and every hour at 9",
            "chaque mardi et vendredi à 9h",
            "every Tuesday at 9 and 10",
            "every Tuesday at 25",
            "every other Friday at 9",
            "every month at 9",
            "toutes les deux semaines",
            "every day at 9 except holidays",
        ] {
            assert_eq!(fields(phrase), None, "{phrase}");
        }
    }
}
