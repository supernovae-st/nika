// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The trigger clause a request opens with, read from its own form: a distributive
//! quantifier ("for each critical row") distributes the work over the material; a
//! sequencing head ("once all three are done", "after the read") orders the work the
//! program already contains; a cadence ("every morning", "tous les matins", "at 9:00") or an
//! event ("when Stripe sends `payment_succeeded`", "dès qu'un ticket arrive") is a
//! REQUIREMENT the portable program bytes cannot carry: the outcome states it beside the
//! candidate as a typed [`TriggerRequirement`] (nika#1720), never bakes a cadence, a hook id
//! or a secret into the bytes, and never drops the clause in silence.

use super::shape;
use super::types::{TriggerKind, TriggerRequirement, TriggerStatus};

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
    let folded = shape::fold(phrase);
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
    if shape::led_by_quantifier(phrase) && !mentions_time {
        return TriggerForm::Distributive;
    }
    if mentions_time {
        return TriggerForm::Schedule;
    }
    TriggerForm::Distributive
}

/// The requirement a cadence or an event trigger states beside the candidate; a
/// distributive or sequencing trigger states none (the structure carries it).
pub(super) fn requirement(phrase: &str, payload_input: Option<&str>) -> Option<TriggerRequirement> {
    let kind = match classify(phrase) {
        TriggerForm::Distributive | TriggerForm::Sequence => return None,
        TriggerForm::Schedule => TriggerKind::Schedule,
        TriggerForm::Event => {
            if padded(phrase).contains(" webhook ") {
                TriggerKind::Webhook
            } else {
                TriggerKind::Event
            }
        }
    };
    let mut requirement = TriggerRequirement::new(kind, TriggerStatus::RequiresBinding);
    requirement.source_hint = Some(phrase.trim().to_owned());
    requirement.payload_input = payload_input.map(str::to_owned);
    Some(requirement)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trigger_is_read_from_its_form_in_six_languages() {
        for phrase in [
            "every morning",
            "Every weekday at 9:00",
            "tous les matins",
            "chaque lundi à 8h30",
            "cada dia",
            "ogni settimana",
            "jeden Morgen",
            "todas as manhas",
            "daily",
        ] {
            assert_eq!(classify(phrase), TriggerForm::Schedule, "{phrase}");
        }
        for phrase in [
            "When Stripe sends payment_succeeded",
            "whenever a ticket is opened",
            "quand un client ecrit",
            "dès qu'un ticket arrive",
            "cuando llega un pedido",
            "quando arriva un ticket",
            "wenn eine Mail eingeht",
            "sempre que um pedido chega",
        ] {
            assert_eq!(classify(phrase), TriggerForm::Event, "{phrase}");
        }
        for phrase in [
            "once all three are done",
            "after the read",
            "then",
            "une fois les trois terminés",
            "when all three are done",
            "quand tout est fini",
            "nach dem Lesen",
        ] {
            assert_eq!(classify(phrase), TriggerForm::Sequence, "{phrase}");
        }
        for phrase in [
            "for each critical row",
            "pour chaque ligne de niveau critique",
            "For each of the four files",
            "para cada cliente",
            "fur jede Datei",
            "every invoice",
        ] {
            assert_eq!(classify(phrase), TriggerForm::Distributive, "{phrase}");
        }
    }

    #[test]
    fn only_a_cadence_or_an_event_states_a_requirement() {
        let schedule = requirement("every morning", None).expect("a cadence");
        assert_eq!(schedule.kind, TriggerKind::Schedule);
        assert_eq!(schedule.status, TriggerStatus::RequiresBinding);
        assert_eq!(schedule.source_hint.as_deref(), Some("every morning"));
        assert!(schedule.payload_input.is_none());
        let event =
            requirement("When Stripe sends payment_succeeded", Some("item")).expect("an event");
        assert_eq!(event.kind, TriggerKind::Event);
        assert_eq!(event.payload_input.as_deref(), Some("item"));
        let hook = requirement("whenever the webhook fires", None).expect("a webhook");
        assert_eq!(hook.kind, TriggerKind::Webhook);
        assert!(requirement("for each critical row", None).is_none());
        assert!(requirement("once all three are done", None).is_none());
        assert_eq!(
            requirement("every morning", None).map(|r| r.to_json()),
            Some(serde_json::json!({
                "kind": "schedule", "source_hint": "every morning", "event_hint": null,
                "payload_input": null, "status": "requires_binding"
            }))
        );
    }

    #[test]
    fn a_clock_time_is_two_digits_with_a_colon_or_an_h() {
        for time in ["9:00", "09:30", "9h", "9h30", "14h"] {
            assert!(clock_time(time), "{time}");
        }
        for not in ["2026", "h", "9:0", "morning", "1234:00", "x9:00"] {
            assert!(!clock_time(not), "{not}");
        }
    }
}
