// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A schedule in the person's words: a declared cadence (`TZ=Europe/Paris 0 8 * * *`), its zone
//! by its place, the policy for a missed run and the keys an activation asks, in English or
//! French, and the person's answers to those questions read back (a delegation, a missed-run
//! policy, a ceiling). The session says its schedule questions, declarations and fired runs
//! through them.

use super::Lang;

/// The three values a declared beat needs that a sentence rarely states, by the keys an
/// activation asks them under.
pub const TIMEZONE: &str = "project.timezone";
/// The policy for a run missed while the machine slept.
pub const MISSED: &str = "project.missed";
/// The ceiling of each run.
pub const CEILING: &str = "project.ceiling";

/// A declared cadence (`TZ=Europe/Paris 0 8 * * *`) in the person's words: « tous les jours à 8h
/// (heure de Paris) », « every Monday at 09:00 (Paris time) ». A shape it does not say in words
/// keeps its cron, still with the zone in words.
#[must_use]
pub fn cadence_words(cadence: &str, lang: Lang) -> String {
    let (zone, fields) = match cadence
        .strip_prefix("TZ=")
        .and_then(|rest| rest.split_once(' '))
    {
        Some((zone, fields)) => (Some(zone), fields),
        None => (None, cadence),
    };
    let said = cron_words(fields, lang).unwrap_or_else(|| format!("`{fields}`"));
    match zone {
        Some(zone) => format!("{said} ({})", zone_words(zone, lang)),
        None => said,
    }
}

/// The five cron fields in words, for the shapes a schedule is usually declared with: every day,
/// some weekdays, a day of the month, every hour or every few hours. `None` for another shape.
fn cron_words(fields: &str, lang: Lang) -> Option<String> {
    let parts: Vec<&str> = fields.split_whitespace().collect();
    let [minute, hour, dom, month, dow] = parts.as_slice() else {
        return None;
    };
    if *month != "*" {
        return None;
    }
    let minute: u32 = minute.parse().ok()?;
    if let Some(step) = hour.strip_prefix("*/") {
        let step: u32 = step.parse().ok()?;
        return (*dom == "*" && *dow == "*").then(|| match lang {
            Lang::Fr => format!("toutes les {step} heures"),
            _ => format!("every {step} hours"),
        });
    }
    if *hour == "*" {
        return (*dom == "*" && *dow == "*").then(|| match (lang, minute) {
            (Lang::Fr, 0) => "toutes les heures".to_owned(),
            (Lang::Fr, m) => format!("toutes les heures à la minute {m:02}"),
            (_, 0) => "every hour".to_owned(),
            (_, m) => format!("every hour at :{m:02}"),
        });
    }
    let hour: u32 = hour.parse().ok()?;
    let at = match lang {
        Lang::Fr if minute == 0 => format!("{hour}h"),
        Lang::Fr => format!("{hour}h{minute:02}"),
        _ => format!("{hour:02}:{minute:02}"),
    };
    match (*dom, *dow) {
        ("*", "*") => Some(match lang {
            Lang::Fr => format!("tous les jours à {at}"),
            _ => format!("every day at {at}"),
        }),
        ("*", "1-5") => Some(match lang {
            Lang::Fr => format!("du lundi au vendredi à {at}"),
            _ => format!("every weekday at {at}"),
        }),
        ("*", days) => {
            let names: Option<Vec<&str>> = (days.split(','))
                .map(|day| day.parse::<usize>().ok().and_then(|d| weekday(d, lang)))
                .collect();
            let names = names?;
            let joined = match (names.split_last(), lang) {
                (Some((last, [])), _) => (*last).to_owned(),
                (Some((last, rest)), Lang::Fr) => format!("{} et {last}", rest.join(", ")),
                (Some((last, rest)), _) => format!("{} and {last}", rest.join(", ")),
                (None, _) => return None,
            };
            Some(match lang {
                Lang::Fr => format!("chaque {joined} à {at}"),
                _ => format!("every {joined} at {at}"),
            })
        }
        (day, "*") => {
            let day: u32 = day.parse().ok()?;
            Some(match (lang, day) {
                (Lang::Fr, 1) => format!("le 1er de chaque mois à {at}"),
                (Lang::Fr, d) => format!("le {d} de chaque mois à {at}"),
                (_, 1) => format!("on the 1st of every month at {at}"),
                (_, 2) => format!("on the 2nd of every month at {at}"),
                (_, 3) => format!("on the 3rd of every month at {at}"),
                (_, d) => format!("on the {d}th of every month at {at}"),
            })
        }
        _ => None,
    }
}

/// A cron weekday (`0` and `7` are Sunday) by its name.
fn weekday(day: usize, lang: Lang) -> Option<&'static str> {
    let fr = [
        "dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi",
    ];
    let en = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];
    let names = if lang == Lang::Fr { fr } else { en };
    names.get(day % 7).copied()
}

/// A zone as people say it: « heure de Maurice », « Paris time ». A host that shows a zone says
/// it with these words, so the Session and its host name it the same way.
#[must_use]
pub fn zone_words(zone: &str, lang: Lang) -> String {
    let place = zone_place(zone, lang);
    match lang {
        Lang::Fr => format!("heure de {place}"),
        _ => format!("{place} time"),
    }
}

/// The place a zone names, as people say it: « Maurice », « Mauritius », « New York ». For a
/// narrow row that has no room for [`zone_words`].
#[must_use]
pub fn zone_place(zone: &str, lang: Lang) -> String {
    const FRENCH: &[(&str, &str)] = &[
        ("Indian/Mauritius", "Maurice"),
        ("Indian/Reunion", "La Réunion"),
        ("Europe/London", "Londres"),
        ("Europe/Brussels", "Bruxelles"),
        ("Europe/Zurich", "Genève"),
        ("America/Montreal", "Montréal"),
        ("America/New_York", "New York"),
    ];
    let city = zone.rsplit('/').next().unwrap_or(zone).replace('_', " ");
    match lang {
        Lang::Fr => (FRENCH.iter())
            .find(|(name, _)| *name == zone)
            .map_or(city, |(_, place)| (*place).to_owned()),
        _ => city,
    }
}

/// A missed-run policy as declared (`rattraper-une-fois`) in the person's words.
#[must_use]
pub fn missed_words(policy: &str, lang: Lang) -> &str {
    match (policy, lang) {
        ("rattraper-une-fois", Lang::Fr) => "le rattraper une fois",
        ("rattraper-une-fois", _) => "catch up once",
        ("sauter", Lang::Fr) => "ne pas le rattraper",
        ("sauter", _) => "skip it",
        ("rattraper", Lang::Fr) => "tout rattraper",
        ("rattraper", _) => "catch up every missed run",
        (other, _) => other,
    }
}

/// The value an activation waits for, in words.
#[must_use]
pub fn key_words(key: &str, lang: Lang) -> &'static str {
    match (key, lang) {
        (TIMEZONE, Lang::Fr) => "le fuseau horaire",
        (TIMEZONE, _) => "the time zone",
        (MISSED, Lang::Fr) => "ce qu'il faut faire d'un run manqué",
        (MISSED, _) => "what to do with a missed run",
        (CEILING, Lang::Fr) => "le plafond par run",
        (CEILING, _) => "the ceiling per run",
        (_, Lang::Fr) => "une valeur",
        _ => "a value",
    }
}

/// Whether an activation answer leaves the choice to Nika: it opens with a delegation
/// (« choisis pour moi », « fais au mieux », « you decide »), a yes or a courtesy before it.
#[must_use]
pub fn delegates(answer: &str) -> bool {
    const DELEGATIONS: &[&str] = &[
        "choisis pour moi",
        "choisis pour toi",
        "choisis toi-même",
        "choisis toi-meme",
        "choisis",
        "choisissez",
        "fais au mieux",
        "faites au mieux",
        "comme tu veux",
        "comme vous voulez",
        "je te laisse choisir",
        "je vous laisse choisir",
        "à toi de voir",
        "a toi de voir",
        "peu importe",
        "carte blanche",
        // The person may name who chooses, or take the option marked recommended (live gauntlet
        // wave 6 r08 B5/B6, 2026-10-18).
        "vous choisissez",
        "tu choisis",
        "à vous de voir",
        "a vous de voir",
        "à vous de choisir",
        "a vous de choisir",
        "à toi de choisir",
        "a toi de choisir",
        "c'est vous qui voyez",
        "c'est toi qui vois",
        "le recommandé",
        "le recommande",
        "la recommandée",
        "la recommandee",
        "celui recommandé",
        "celui recommande",
        "celle recommandée",
        "ce que vous recommandez",
        "ce que tu recommandes",
        "the recommended one",
        "the recommended",
        "recommended",
        "let nika choose",
        "you choose",
        "you decide",
        "choose for me",
        "up to you",
        "whatever",
    ];
    let lower = answer.replace('\u{2019}', "'").to_lowercase();
    // Any clause may leave the choice (live gauntlet r08, 2026-10-16: « je ne comprends pas,
    // choisissez pour moi » was refused as no amount).
    lower.split([',', '.', ';', '!', '?']).any(|clause| {
        let mut rest = clause.trim();
        for lead in [
            "oui", "ok", "bon", "alors", "yes", "et", "and", "mais", "but",
        ] {
            if let Some(after) = rest.strip_prefix(lead)
                && after.starts_with([' ', ','])
            {
                rest = after.trim_start_matches([' ', ',']);
            }
        }
        DELEGATIONS.iter().any(|phrase| {
            rest.strip_prefix(phrase).is_some_and(|after| {
                after.is_empty() || after.starts_with([' ', ',', '.', '!', '?'])
            })
        })
    })
}

/// What « missed » a free answer means, in the person's words (live gauntlet r08: « si
/// l'ordinateur est éteint, le faire une fois quand il se rallume » was refused): run once when
/// back, skip, or replay every missed run. `None` when the words say none of them, or two.
#[must_use]
pub fn missed_meaning(answer: &str) -> Option<&'static str> {
    let lower = answer.replace('\u{2019}', "'").to_lowercase();
    let says = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    let once = says(&[
        "une fois",
        "une seule",
        "once",
        "quand il se rallume",
        "quand elle se rallume",
        "au redémarrage",
        "au retour",
        "when it comes back",
        "when it's back",
        "when it is back",
        "rattraper une fois",
    ]);
    let every = says(&[
        "tous les",
        "toutes les",
        "chacun",
        "chaque run",
        "tout rattraper",
        "rattrape tout",
        "every",
        "all of them",
        "all missed",
        "each missed",
    ]);
    let skip = says(&[
        "saute",
        "sauter",
        "ignore",
        "rien faire",
        "ne rien",
        "laisse tomber",
        "pas la peine",
        "skip",
        "nothing",
        "don't",
        "do not",
    ]);
    match (once, every, skip) {
        (true, false, false) => Some("rattraper-une-fois"),
        (false, true, false) => Some("rattraper"),
        (false, false, true) => Some("sauter"),
        _ => None,
    }
}

/// The amount a ceiling answer states with its currency (« 0,10 $ », « $0.10 », « 0.10 USD »,
/// « 10 cents »), as the money reader reads one; a bare number as it is.
#[must_use]
pub fn ceiling_amount(answer: &str) -> Option<f64> {
    let lower = answer.trim().to_lowercase().replace(',', ".");
    let cents = ["cents", "cent", "centimes", "centime"];
    let words: Vec<&str> = lower.split_whitespace().collect();
    if let [number, unit] = words.as_slice()
        && cents.contains(unit)
    {
        return number.parse::<f64>().ok().map(|n| n / 100.0);
    }
    // One amount said with its unit inside a sentence (« celui à 10 cents », « mets 0.10 dollar
    // par run »): two amounts say none.
    let dollars = ["dollar", "dollars", "usd", "$"];
    let said: Vec<f64> = (words.windows(2))
        .filter_map(|pair| {
            let number = pair[0].trim_start_matches('$').parse::<f64>().ok()?;
            let unit = pair[1].trim_end_matches(['.', '!', '?', ';']);
            if cents.contains(&unit) {
                Some(number / 100.0)
            } else {
                (dollars.contains(&unit) || pair[0].starts_with('$')).then_some(number)
            }
        })
        .collect();
    if words.len() > 2
        && let [amount] = said.as_slice()
    {
        return Some(*amount);
    }
    let bare = (lower.trim_start_matches('$').trim())
        .trim_end_matches(|c: char| c == '$' || c.is_whitespace())
        .trim_end_matches("usd")
        .trim_end_matches("dollars")
        .trim_end_matches("dollar")
        .trim();
    bare.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::{Lang, cadence_words, key_words, missed_words, zone_place, zone_words};

    /// A zone reads as its place, the way people say it, in each language.
    #[test]
    fn a_zone_reads_as_its_place() {
        assert_eq!(zone_place("Indian/Mauritius", Lang::Fr), "Maurice");
        assert_eq!(zone_place("Indian/Mauritius", Lang::En), "Mauritius");
        assert_eq!(zone_place("America/New_York", Lang::En), "New York");
        assert_eq!(zone_place("Europe/Paris", Lang::Fr), "Paris");
        assert_eq!(zone_words("Indian/Mauritius", Lang::Fr), "heure de Maurice");
        assert_eq!(zone_words("Europe/Paris", Lang::En), "Paris time");
    }

    /// A declared cadence reads in the person's words, its zone as people say it; a shape it
    /// does not say keeps its cron (supernovae-1c's list, 2026-10-16: « TZ=Indian/Mauritius 0 20
    /// * * * » was shown as is).
    #[test]
    fn a_declared_cadence_reads_in_words() {
        for (cadence, fr, en) in [
            (
                "TZ=Indian/Mauritius 0 20 * * *",
                "tous les jours à 20h (heure de Maurice)",
                "every day at 20:00 (Mauritius time)",
            ),
            (
                "TZ=Europe/Paris 15 9 * * 2",
                "chaque mardi à 9h15 (heure de Paris)",
                "every Tuesday at 09:15 (Paris time)",
            ),
            (
                "TZ=Europe/Paris 0 8 * * 1-5",
                "du lundi au vendredi à 8h (heure de Paris)",
                "every weekday at 08:00 (Paris time)",
            ),
            (
                "TZ=Europe/Paris 0 9 * * 1,4",
                "chaque lundi et jeudi à 9h (heure de Paris)",
                "every Monday and Thursday at 09:00 (Paris time)",
            ),
            (
                "TZ=Europe/Paris 0 8 1 * *",
                "le 1er de chaque mois à 8h (heure de Paris)",
                "on the 1st of every month at 08:00 (Paris time)",
            ),
            (
                "TZ=America/New_York 0 */2 * * *",
                "toutes les 2 heures (heure de New York)",
                "every 2 hours (New York time)",
            ),
            (
                "TZ=Europe/Paris 0 8 1 1 *",
                "`0 8 1 1 *` (heure de Paris)",
                "`0 8 1 1 *` (Paris time)",
            ),
        ] {
            assert_eq!(cadence_words(cadence, Lang::Fr), fr, "{cadence}");
            assert_eq!(cadence_words(cadence, Lang::En), en, "{cadence}");
        }
        assert_eq!(
            missed_words("rattraper-une-fois", Lang::Fr),
            "le rattraper une fois"
        );
        assert_eq!(missed_words("sauter", Lang::En), "skip it");
        assert_eq!(key_words("project.timezone", Lang::Fr), "le fuseau horaire");
        assert_eq!(
            key_words("project.ceiling", Lang::En),
            "the ceiling per run"
        );
    }
}
