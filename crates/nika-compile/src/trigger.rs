// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The trigger a request names ("Every morning at 9, …", "chaque lundi à 8h30, …", "for
//! each incoming ticket, …") is deployment, not workflow: the candidate's bytes stay
//! trigger-agnostic and the outcome states the requirement beside them (nika#1720). The
//! reader keeps the cadence phrase verbatim on the plan; this module reads what the words
//! state, a cadence word and a time of day, and nothing they do not: a phrase with neither
//! is an event the operator binds, never a guessed cron.

use super::plan::Plan;
use super::{TriggerKind, TriggerRequirement, TriggerStatus, hot};

/// Words that state a daily cadence (EN · FR · IT · ES · DE · PT, folded).
const DAILY: &[&str] = &[
    "morning",
    "mornings",
    "evening",
    "evenings",
    "night",
    "nights",
    "noon",
    "midday",
    "midnight",
    "day",
    "days",
    "daily",
    "matin",
    "matins",
    "soir",
    "soirs",
    "jour",
    "jours",
    "quotidien",
    "quotidienne",
    "quotidiennement",
    "midi",
    "minuit",
    "mattina",
    "sera",
    "giorno",
    "giorni",
    "quotidiano",
    "manana",
    "mananas",
    "tarde",
    "noche",
    "dia",
    "dias",
    "diario",
    "diaria",
    "diariamente",
    "morgen",
    "abend",
    "tag",
    "taglich",
    "manha",
    "manhas",
    "noite",
    "noites",
];
/// Words that state a weekday cadence: every working day.
const WEEKDAYS: &[&str] = &[
    "weekday",
    "weekdays",
    "workday",
    "workdays",
    "ouvre",
    "ouvres",
    "ouvrable",
    "ouvrables",
    "lavorativo",
    "lavorativi",
    "laborable",
    "laborables",
    "werktag",
    "werktags",
];
/// Words that state a weekly cadence: a week or a named day of the week.
const WEEKLY: &[&str] = &[
    "week",
    "weeks",
    "weekly",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
    "semaine",
    "semaines",
    "hebdomadaire",
    "hebdo",
    "lundi",
    "mardi",
    "mercredi",
    "jeudi",
    "vendredi",
    "samedi",
    "dimanche",
    "settimana",
    "settimanale",
    "lunedi",
    "martedi",
    "mercoledi",
    "giovedi",
    "venerdi",
    "sabato",
    "domenica",
    "semana",
    "semanal",
    "lunes",
    "martes",
    "miercoles",
    "jueves",
    "viernes",
    "sabado",
    "domingo",
    "woche",
    "wochentlich",
    "montag",
    "dienstag",
    "mittwoch",
    "donnerstag",
    "freitag",
    "samstag",
    "sonntag",
];
const MONTHLY: &[&str] = &[
    "month",
    "months",
    "monthly",
    "mois",
    "mensuel",
    "mensuelle",
    "mese",
    "mesi",
    "mensile",
    "mes",
    "meses",
    "mensual",
    "monat",
    "monate",
    "monatlich",
];
const HOURLY: &[&str] = &[
    "hour",
    "hours",
    "hourly",
    "heure",
    "heures",
    "horaire",
    "ora",
    "ore",
    "hora",
    "horas",
    "stunde",
    "stunden",
    "stundlich",
];
const MINUTELY: &[&str] = &["minute", "minutes", "minuto", "minuti", "minutos"];

/// Words that introduce a time of day ("at 9", "à 9h", "alle 7", "a las 8", "um 9 uhr",
/// "às 7h" — the Portuguese « às » folds to « as »).
const AT: &[&str] = &["at", "a", "as", "alle", "um", "vers", "around", "towards"];
/// Determiners that may sit between the introducer and the number ("a las 8").
const BETWEEN: &[&str] = &["las", "les", "le", "la", "the", "l"];
/// Units that follow a time number and belong to it, never to a cadence.
const TIME_UNITS: &[&str] = &[
    "h", "hours", "hour", "heures", "heure", "uhr", "horas", "ore",
];
/// Words that name a time of day outright.
const NAMED_TIMES: &[(&str, &str)] = &[
    ("noon", "12:00"),
    ("midday", "12:00"),
    ("midi", "12:00"),
    ("mezzogiorno", "12:00"),
    ("mediodia", "12:00"),
    ("mittag", "12:00"),
    ("midnight", "00:00"),
    ("minuit", "00:00"),
    ("mezzanotte", "00:00"),
    ("medianoche", "00:00"),
    ("mitternacht", "00:00"),
];
/// Words that name an inbound HTTP call.
const WEBHOOK: &[&str] = &["webhook", "webhooks", "hook", "hooks"];

/// The requirement the plan's trigger phrase states, when the plan carries one.
pub(super) fn requirement(plan: &Plan, item: bool) -> Option<TriggerRequirement> {
    let phrase = plan.trigger.as_deref()?.trim();
    if phrase.is_empty() {
        return None;
    }
    let folded = hot::fold(phrase);
    let words: Vec<&str> = folded
        .split(|c: char| !c.is_alphanumeric() && c != ':')
        .filter(|w| !w.is_empty())
        .collect();
    let (at, consumed) = time_of_day(&words);
    let cadence = cadence(&words, &consumed);
    let kind = if cadence.is_some() || at.is_some() {
        TriggerKind::Schedule
    } else if words.iter().any(|w| WEBHOOK.contains(w)) {
        TriggerKind::Webhook
    } else {
        TriggerKind::Event
    };
    Some(TriggerRequirement {
        kind,
        source_hint: Some(phrase.to_owned()),
        event_hint: None,
        cadence: cadence.map(str::to_owned),
        at,
        payload_input: item.then(|| "item".to_owned()),
        status: TriggerStatus::RequiresBinding,
        timezone: None,
        missed: None,
        overlap: None,
        ceiling: None,
    })
}

/// The overlap policies of the cadence grammar (`chevauchement:`), spelled as that grammar
/// spells them; mirrored here with its drift test (`nika-cadence` owns the enum).
pub(super) const OVERLAP_OPTIONS: &[(&str, &str)] = &[
    ("sauter", "skip the new run while one still runs"),
    ("file", "queue the new run behind the running one"),
    ("remplacer", "replace the running run with the new one"),
];

/// The missed-run policies of the project grammar (`manqué:`), from the grammar itself.
pub(super) fn missed_options() -> Vec<super::types::ChoiceOffer> {
    use nika_vocab::project::MissPolicy;
    [
        (
            MissPolicy::Rattraper,
            "fire every missed slot, oldest first",
        ),
        (
            MissPolicy::RattraperUneFois,
            "fire one catch-up for the whole silence",
        ),
        (
            MissPolicy::Sauter,
            "never catch up: a skip is an event, not a run",
        ),
    ]
    .into_iter()
    .map(|(policy, label)| super::types::ChoiceOffer::new(policy.as_str(), label))
    .collect()
}

const BINDING_WHY: &str = "A cadence is bound outside the program (the project's arm entry, `PUT /v1/schedules`); this value belongs to that binding, never to the workflow bytes. Optional here: the binding asks it if still missing.";

/// The values a schedule binding needs, asked beside the candidate without blocking it:
/// the timezone, the missed-run policy, the overlap policy and the per-run ceiling. An
/// answer is admitted against the owning grammar's own spellings and echoed on the
/// requirement; a wrong answer is a finding and the question stays.
pub(super) fn bind_schedule(
    trigger: &mut TriggerRequirement,
    request: &super::CompileRequest,
    out: &mut super::CompileOutcome,
    recognized: &mut std::collections::BTreeSet<String>,
) {
    if trigger.kind != TriggerKind::Schedule {
        return;
    }
    for key in [
        "trigger.timezone",
        "trigger.missed",
        "trigger.overlap",
        "trigger.ceiling",
    ] {
        recognized.insert(key.to_owned());
    }
    bind_timezone(trigger, request, out);
    bind_choices(trigger, request, out);
    bind_ceiling(trigger, request, out);
}

/// The answer a request carries for one key, decoded as a JSON literal (or nothing).
fn answered(
    request: &super::CompileRequest,
    out: &mut super::CompileOutcome,
    key: &str,
) -> Option<serde_json::Value> {
    let raw = request.answers.get(key).map(String::as_str)?;
    super::literal_answer(Some(raw), key, out)
}

/// `trigger.timezone`: a nonempty IANA name.
fn bind_timezone(
    trigger: &mut TriggerRequirement,
    request: &super::CompileRequest,
    out: &mut super::CompileOutcome,
) {
    use super::types::{DiagnosticKind, QuestionType};
    match answered(request, out, "trigger.timezone") {
        Some(serde_json::Value::String(zone)) if !zone.trim().is_empty() => {
            trigger.timezone = Some(zone.trim().to_owned());
        }
        Some(_) => super::finding(
            out,
            DiagnosticKind::Missed,
            "trigger.timezone",
            "Answer the timezone as a nonempty JSON string (an IANA name such as Europe/Paris).",
        ),
        None => {}
    }
    if trigger.timezone.is_none() {
        let hint = trigger.source_hint.clone().unwrap_or_default();
        super::optional_question(
            out,
            "trigger.timezone",
            &format!("Which timezone runs `{hint}`? An IANA name such as Europe/Paris."),
            QuestionType::Text,
            BINDING_WHY,
            Vec::new(),
        );
    }
}

/// `trigger.missed` · `trigger.overlap`: one of the owning grammar's spellings.
fn bind_choices(
    trigger: &mut TriggerRequirement,
    request: &super::CompileRequest,
    out: &mut super::CompileOutcome,
) {
    use super::types::{ChoiceOffer, DiagnosticKind, QuestionType};
    let choices: [(&str, Vec<ChoiceOffer>, &str); 2] = [
        (
            "trigger.missed",
            missed_options(),
            "If the machine was off when a run was due, what happens?",
        ),
        (
            "trigger.overlap",
            OVERLAP_OPTIONS
                .iter()
                .map(|(key, label)| ChoiceOffer::new(*key, *label))
                .collect(),
            "If a run is still running when the next one is due, what happens?",
        ),
    ];
    for (key, options, label) in choices {
        let chosen = match answered(request, out, key) {
            Some(serde_json::Value::String(word))
                if options.iter().any(|o| o.key == word.trim()) =>
            {
                Some(word.trim().to_owned())
            }
            Some(_) => {
                super::finding(
                    out,
                    DiagnosticKind::Missed,
                    key,
                    format!(
                        "Answer one of the offered keys as a JSON string: {}.",
                        options
                            .iter()
                            .map(|o| o.key.as_str())
                            .collect::<Vec<_>>()
                            .join(" · ")
                    ),
                );
                None
            }
            None => None,
        };
        match (key, chosen) {
            ("trigger.missed", Some(word)) => trigger.missed = Some(word),
            ("trigger.overlap", Some(word)) => trigger.overlap = Some(word),
            _ => super::optional_question(
                out,
                key,
                label,
                QuestionType::Choice,
                BINDING_WHY,
                options,
            ),
        }
    }
}

/// `trigger.ceiling`: a positive number, kept as its canonical text.
fn bind_ceiling(
    trigger: &mut TriggerRequirement,
    request: &super::CompileRequest,
    out: &mut super::CompileOutcome,
) {
    use super::types::{DiagnosticKind, QuestionType};
    match answered(request, out, "trigger.ceiling") {
        Some(serde_json::Value::Number(n)) if n.as_f64().is_some_and(|v| v > 0.0) => {
            trigger.ceiling = Some(n.to_string());
        }
        Some(_) => super::finding(
            out,
            DiagnosticKind::Missed,
            "trigger.ceiling",
            "Answer the per-run spend ceiling as a positive JSON number (USD).",
        ),
        None => {}
    }
    if trigger.ceiling.is_none() {
        super::optional_question(
            out,
            "trigger.ceiling",
            "What is the maximum spend per scheduled run, in USD?",
            QuestionType::Literal,
            BINDING_WHY,
            Vec::new(),
        );
    }
}

/// The note the compile records beside the candidate: what was read, where it went.
pub(super) fn note(trigger: &TriggerRequirement) -> String {
    let mut read = vec![trigger.kind.word().to_owned()];
    read.extend(trigger.cadence.iter().cloned());
    read.extend(trigger.at.iter().cloned());
    if let Some(input) = &trigger.payload_input {
        read.push(format!("each firing supplies `inputs.{input}`"));
    }
    format!(
        "`{}` is a trigger, not a task: recorded as requested_trigger ({}) for whoever binds it through a schedule or an ingress; the candidate's bytes carry no cadence, host or event and run once per invocation.",
        trigger.source_hint.as_deref().unwrap_or_default(),
        read.join(" · ")
    )
}

/// The coarsest cadence the words state, the named day or working day winning over the
/// day it also names ("every monday morning" is weekly).
fn cadence(words: &[&str], consumed: &[usize]) -> Option<&'static str> {
    let free: Vec<&str> = words
        .iter()
        .enumerate()
        .filter(|(i, _)| !consumed.contains(i))
        .map(|(_, w)| *w)
        .collect();
    let has = |table: &[&str]| free.iter().any(|w| table.contains(w));
    if has(WEEKDAYS) {
        Some("weekdays")
    } else if has(WEEKLY) {
        Some("weekly")
    } else if has(MONTHLY) {
        Some("monthly")
    } else if has(DAILY) {
        Some("daily")
    } else if has(HOURLY) {
        Some("hourly")
    } else if has(MINUTELY) {
        Some("minutely")
    } else {
        None
    }
}

/// The time of day the words state after an introducer ("at 9", "at 9:30 pm", "à 9h30",
/// "a las 8", "um 9 uhr") or by name ("noon"), as `HH:MM`, with the indices of the words
/// the time consumed (a unit after the number is the time's, never a cadence).
fn time_of_day(words: &[&str]) -> (Option<String>, Vec<usize>) {
    for (i, word) in words.iter().enumerate() {
        if let Some((_, time)) = NAMED_TIMES.iter().find(|(name, _)| name == word) {
            return (Some((*time).to_owned()), vec![i]);
        }
        if !AT.contains(word) {
            continue;
        }
        let mut k = i + 1;
        while words.get(k).is_some_and(|w| BETWEEN.contains(w)) {
            k += 1;
        }
        let Some(number) = words.get(k) else {
            continue;
        };
        let mut consumed = vec![i, k];
        let mut meridiem = None;
        let mut token = (*number).to_owned();
        for suffix in ["am", "pm"] {
            if let Some(stem) = token.strip_suffix(suffix) {
                meridiem = Some(suffix);
                token = stem.to_owned();
            }
        }
        if let Some(next) = words.get(k + 1) {
            if matches!(*next, "am" | "pm") {
                meridiem = Some(*next);
                consumed.push(k + 1);
            } else if TIME_UNITS.contains(next) {
                consumed.push(k + 1);
            }
        }
        let Some((hour, minute)) = clock(&token) else {
            continue;
        };
        let hour = match (meridiem, hour) {
            (Some("pm"), h) if h < 12 => h + 12,
            (Some("am"), 12) => 0,
            (_, h) => h,
        };
        if hour > 23 || minute > 59 {
            continue;
        }
        return (Some(format!("{hour:02}:{minute:02}")), consumed);
    }
    (None, Vec::new())
}

/// `9` · `09` · `9:30` · `9h` · `9h30` → (hour, minute); anything else is not a clock.
fn clock(token: &str) -> Option<(u32, u32)> {
    let (hour, minute) = match token.split_once([':', 'h']) {
        Some((hour, "")) => (hour, "0"),
        Some((hour, minute)) => (hour, minute),
        None => (token, "0"),
    };
    if hour.is_empty() || hour.len() > 2 || minute.len() > 2 {
        return None;
    }
    Some((hour.parse().ok()?, minute.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(phrase: &str, item: bool) -> TriggerRequirement {
        let mut plan = Plan::default();
        plan.trigger = Some(phrase.to_owned());
        requirement(&plan, item).expect("a trigger phrase")
    }

    #[test]
    fn a_cadence_and_a_time_of_day_are_read_in_five_languages_and_nothing_is_invented() {
        for (phrase, cadence, at) in [
            ("every morning at 9", Some("daily"), Some("09:00")),
            ("every weekday at 9:00", Some("weekdays"), Some("09:00")),
            ("every monday at 9 pm", Some("weekly"), Some("21:00")),
            ("every monday morning", Some("weekly"), None),
            ("every hour", Some("hourly"), None),
            ("every 2 hours", Some("hourly"), None),
            ("every month", Some("monthly"), None),
            ("every day at noon", Some("daily"), Some("12:00")),
            ("every night at 12 am", Some("daily"), Some("00:00")),
            ("tous les jours à 18h", Some("daily"), Some("18:00")),
            ("chaque lundi à 8h30", Some("weekly"), Some("08:30")),
            ("tous les matins", Some("daily"), None),
            ("ogni mattina alle 7", Some("daily"), Some("07:00")),
            ("cada lunes a las 8", Some("weekly"), Some("08:00")),
            ("jeden morgen um 9 uhr", Some("daily"), Some("09:00")),
            ("todas as manhãs às 7h", Some("daily"), Some("07:00")),
            ("every day at 25", Some("daily"), None),
        ] {
            let trigger = read(phrase, false);
            assert_eq!(trigger.kind, TriggerKind::Schedule, "{phrase}");
            assert_eq!(trigger.cadence.as_deref(), cadence, "{phrase}");
            assert_eq!(trigger.at.as_deref(), at, "{phrase}");
            assert_eq!(trigger.source_hint.as_deref(), Some(phrase));
            assert_eq!(trigger.status, TriggerStatus::RequiresBinding);
            assert_eq!(trigger.payload_input, None);
        }
    }

    #[test]
    fn a_phrase_with_no_cadence_is_an_event_that_supplies_the_item() {
        let trigger = read("for each incoming ticket", true);
        assert_eq!(trigger.kind, TriggerKind::Event);
        assert_eq!(trigger.cadence, None);
        assert_eq!(trigger.at, None);
        assert_eq!(trigger.payload_input.as_deref(), Some("item"));
        let trigger = read("when the stripe webhook fires", true);
        assert_eq!(trigger.kind, TriggerKind::Webhook);
        assert!(requirement(&Plan::default(), false).is_none());
        let note = note(&read("every morning at 9", false));
        assert!(
            note.contains("`every morning at 9`") && note.contains("schedule · daily · 09:00"),
            "{note}"
        );
    }
}

// ── the form of a trigger clause (season 2) ──────────────────────────────────────

/// What form a trigger clause takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TriggerForm {
    /// One unit of work per item of the material ("for each file", "pour chaque ligne").
    Distributive,
    /// An order between the program's own steps ("once all three are done", "after that").
    Sequence,
    /// A cadence the program runs on ("every morning", "tous les matins", "at 9:00").
    Schedule,
    /// An outside event the program runs on ("when Stripe sends …", "dès qu'un ticket arrive").
    Event,
}

/// Heads that order the program's own work (EN · FR · ES · IT · DE · PT, folded).
const SEQUENCE_HEADS: &[&str] = &[
    "once ",
    "after ",
    "then ",
    "apres ",
    "puis ",
    "ensuite ",
    "une fois ",
    "despues ",
    "luego ",
    "una vez ",
    "dopo ",
    "poi ",
    "nach ",
    "danach ",
    "sobald alle ",
    "depois ",
];

/// Heads that open an outside event (folded).
const EVENT_HEADS: &[&str] = &[
    "when ",
    "whenever ",
    "each time ",
    "every time ",
    "quand ",
    "lorsque ",
    "des que ",
    "des qu'",
    "chaque fois ",
    "a chaque fois ",
    "cuando ",
    "cada vez ",
    "quando ",
    "ogni volta ",
    "wenn ",
    "sobald ",
    "jedes mal ",
    "sempre que ",
    "toda vez ",
];

/// Words that say the program's own work is finished: a `when` over them is a sequence.
const COMPLETION_WORDS: &[&str] = &[
    "done",
    "finished",
    "complete",
    "completed",
    "termine",
    "terminee",
    "fini",
    "finie",
    "acheve",
    "terminado",
    "terminada",
    "finalizado",
    "completato",
    "completata",
    "finito",
    "fertig",
    "abgeschlossen",
    "concluido",
    "pronto",
];

/// Words that name a moment or a period of the calendar (folded): a quantifier over them
/// is a cadence, not a distribution over items.
const TIME_WORDS: &[&str] = &[
    "morning",
    "mornings",
    "noon",
    "afternoon",
    "evening",
    "evenings",
    "night",
    "nights",
    "midnight",
    "day",
    "days",
    "daily",
    "weekday",
    "weekdays",
    "week",
    "weeks",
    "weekly",
    "month",
    "months",
    "monthly",
    "quarter",
    "hour",
    "hours",
    "hourly",
    "minute",
    "minutes",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
    "matin",
    "matins",
    "midi",
    "soir",
    "soirs",
    "nuit",
    "jour",
    "jours",
    "quotidien",
    "semaine",
    "semaines",
    "hebdomadaire",
    "mois",
    "mensuel",
    "heure",
    "heures",
    "lundi",
    "mardi",
    "mercredi",
    "jeudi",
    "vendredi",
    "samedi",
    "dimanche",
    "manana",
    "mananas",
    "tarde",
    "noche",
    "dia",
    "dias",
    "diario",
    "semana",
    "semanas",
    "semanal",
    "mes",
    "meses",
    "mensual",
    "hora",
    "horas",
    "lunes",
    "martes",
    "miercoles",
    "jueves",
    "viernes",
    "sabado",
    "domingo",
    "mattina",
    "mattino",
    "sera",
    "notte",
    "giorno",
    "giorni",
    "giornaliero",
    "settimana",
    "settimane",
    "settimanale",
    "mese",
    "mesi",
    "mensile",
    "ora",
    "ore",
    "lunedi",
    "martedi",
    "mercoledi",
    "giovedi",
    "venerdi",
    "sabato",
    "domenica",
    "morgen",
    "morgens",
    "mittag",
    "abend",
    "abends",
    "nacht",
    "tag",
    "tage",
    "taglich",
    "woche",
    "wochen",
    "wochentlich",
    "monat",
    "monate",
    "monatlich",
    "stunde",
    "stunden",
    "stundlich",
    "montag",
    "dienstag",
    "mittwoch",
    "donnerstag",
    "freitag",
    "samstag",
    "sonntag",
    "manha",
    "manhas",
    "noite",
    "noites",
    "diariamente",
];

/// The folded phrase, one space between words, a leading space for whole-word heads.
fn padded(phrase: &str) -> String {
    let folded = hot::fold(phrase);
    let mut out = String::with_capacity(folded.len() + 2);
    out.push(' ');
    let mut space = false;
    for c in folded.chars() {
        if c.is_alphanumeric() || matches!(c, '\'' | ':' | '-') {
            out.push(c);
            space = false;
        } else if !space {
            out.push(' ');
            space = true;
        }
    }
    if !out.ends_with(' ') {
        out.push(' ');
    }
    out
}

fn words(padded: &str) -> impl Iterator<Item = &str> {
    padded.split(' ').filter(|w| !w.is_empty())
}

/// A clock time: `9:00`, `09:30`, `9h`, `9h30`, `14h`.
fn clock_time(word: &str) -> bool {
    let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != ':');
    let (hours, rest) = match word.find([':', 'h']) {
        Some(at) => (&word[..at], &word[at + 1..]),
        None => return false,
    };
    !hours.is_empty()
        && hours.len() <= 2
        && hours.chars().all(|c| c.is_ascii_digit())
        && (rest.is_empty() || (rest.len() == 2 && rest.chars().all(|c| c.is_ascii_digit())))
}

/// Words that mark an item as arriving with the invocation (an event stream, not a set that
/// exists somewhere): « each incoming brief », « chaque nouveau ticket », « cada correo
/// recibido », « ogni nuova richiesta », « jede eingehende Mail », « cada novo pedido ».
const ARRIVAL_WORDS: &[&str] = &[
    "incoming",
    "new",
    "arriving",
    "received",
    "submitted",
    "entrant",
    "entrante",
    "entrants",
    "entrantes",
    "nouveau",
    "nouvel",
    "nouvelle",
    "nouveaux",
    "nouvelles",
    "recu",
    "recue",
    "recus",
    "recues",
    "qui arrive",
    "nuevo",
    "nueva",
    "nuevos",
    "nuevas",
    "recibido",
    "recibida",
    "recibidos",
    "recibidas",
    "nuovo",
    "nuova",
    "nuovi",
    "nuove",
    "in arrivo",
    "ricevuto",
    "ricevuta",
    "ricevuti",
    "ricevute",
    "neu",
    "neue",
    "neuen",
    "neues",
    "neuer",
    "eingehend",
    "eingehende",
    "eingehenden",
    "eingegangen",
    "eingegangene",
    "novo",
    "nova",
    "novos",
    "novas",
    "recebido",
    "recebida",
    "recebidos",
    "recebidas",
    "a chegar",
];

/// Whether a distributive phrase quantifies over arriving items rather than a located set.
pub(super) fn arriving(phrase: &str) -> bool {
    let padded: String = format!(" {} ", super::hot::fold(phrase))
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '\'' {
                c
            } else {
                ' '
            }
        })
        .collect();
    ARRIVAL_WORDS
        .iter()
        .any(|w| padded.contains(&format!(" {w} ")))
}

/// Read the form of a trigger clause.
pub(super) fn classify(phrase: &str) -> TriggerForm {
    let padded = padded(phrase);
    let mentions_time = words(&padded).any(|w| TIME_WORDS.contains(&w) || clock_time(w));
    let completes = words(&padded).any(|w| COMPLETION_WORDS.contains(&w));
    if SEQUENCE_HEADS
        .iter()
        .any(|h| padded.starts_with(&format!(" {h}")))
    {
        return TriggerForm::Sequence;
    }
    if EVENT_HEADS
        .iter()
        .any(|h| padded.starts_with(&format!(" {h}")))
    {
        return if completes {
            TriggerForm::Sequence
        } else {
            TriggerForm::Event
        };
    }
    if super::shape::led_by_quantifier(phrase) && !mentions_time {
        return TriggerForm::Distributive;
    }
    if mentions_time {
        return TriggerForm::Schedule;
    }
    TriggerForm::Distributive
}
