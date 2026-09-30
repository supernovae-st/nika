// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The trigger a request names ("Every morning at 9, …", "chaque lundi à 8h30, …", "for
//! each incoming ticket, …") is deployment, not workflow: the candidate's bytes stay
//! trigger-agnostic and the outcome states the requirement beside them (nika#1720). The
//! reader keeps the cadence phrase verbatim on the plan; this module reads what the words
//! state, a cadence word and a time of day, and nothing they do not: a phrase with neither
//! is an event the operator binds, never a guessed cron. A recurrence stated without its
//! cadence (« régulièrement », « from time to time ») is a schedule whose cadence is asked.
//! The reading itself (the words, the cadence, the time of day, the multiples, the cron
//! fields, the form of a clause) lives in `nika-compile-trigger` (ADR-142); this module binds
//! what it reads.

use super::plan::Plan;
use super::{TriggerKind, TriggerRequirement, TriggerStatus, hot};
use nika_compile_reader::words::recurrence;
use nika_compile_trigger::words::{MANUAL, WEBHOOK};
pub(super) use nika_compile_trigger::{TriggerForm, arriving, classify};
use nika_compile_trigger::{multiple, phrase_words, schedule, stated_cadence};

/// The requirement the plan's trigger phrase states, when the plan carries one.
pub(super) fn requirement(plan: &Plan, item: bool) -> Option<TriggerRequirement> {
    let phrase = plan.trigger.as_deref()?.trim();
    if phrase.is_empty() {
        return None;
    }
    let folded = hot::fold(phrase);
    let words = phrase_words(&folded);
    let (cadence, at) = stated_cadence(&words);
    // A period five cron fields cannot hold (« every other monday ») keeps no coarse label: it
    // is a schedule whose cadence is asked (`bind_cadence`), never recorded as `weekly` (C1).
    let multiple = multiple::unbindable(&words);
    let cadence = cadence.filter(|_| !multiple);
    // A recurrence without its cadence (« régulièrement ») is a schedule; its cadence is asked
    // (`bind_cadence`). Under an event head (« quand … régulièrement ») it stays the event.
    let recurrent = recurrence(phrase).is_some() && classify(phrase) == TriggerForm::Schedule;
    let kind = if cadence.is_some() || at.is_some() || recurrent || multiple {
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
        cron: schedule::fields(phrase),
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
/// requirement; a wrong answer is a finding and the question stays. A schedule the request
/// states without its cadence first waits for it (`bind_cadence`, mandatory).
pub(super) fn bind_schedule(
    trigger: &mut TriggerRequirement,
    request: &super::CompileRequest,
    out: &mut super::CompileOutcome,
    recognized: &mut std::collections::BTreeSet<String>,
) {
    if trigger.kind != TriggerKind::Schedule {
        return;
    }
    bind_cadence(trigger, request, out, recognized);
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

/// `trigger.cadence`: the request wants its work repeated and never says when
/// (« régulièrement », « from time to time »), or states a period a schedule cannot bind
/// (« every other monday »: C1). A guessed or narrowed cadence would invent the deployment,
/// so the cadence is a mandatory question: the answer is read with the words a request states
/// a cadence with (« chaque lundi à 9h », « every day at 18:00 »), or `manual` says each run
/// starts by hand and nothing is bound. An answer that states neither, or another unbindable
/// period, is a finding and the question stays.
fn bind_cadence(
    trigger: &mut TriggerRequirement,
    request: &super::CompileRequest,
    out: &mut super::CompileOutcome,
    recognized: &mut std::collections::BTreeSet<String>,
) {
    use super::types::{DiagnosticKind, QuestionType};
    const KEY: &str = "trigger.cadence";
    let hint = trigger.source_hint.clone().unwrap_or_default();
    let unbound = multiple::unbindable(&phrase_words(&hot::fold(&hint)));
    if (trigger.cadence.is_some() || trigger.at.is_some()) && !unbound {
        return;
    }
    recognized.insert(KEY.to_owned());
    match answered(request, out, KEY) {
        Some(serde_json::Value::String(answer)) => {
            let folded = hot::fold(&answer);
            let words = phrase_words(&folded);
            let (cadence, at) = stated_cadence(&words);
            let said = answer.trim();
            if multiple::unbindable(&words) {
                super::finding(
                    out,
                    DiagnosticKind::Missed,
                    KEY,
                    format!(
                        "« {said} » is a period a schedule cannot bind either: answer a day, a weekday, a named weekday at a time or an hour or minute interval, or \"manual\"."
                    ),
                );
            } else if MANUAL.contains(&words.join(" ").as_str()) {
                trigger.kind = TriggerKind::Manual;
                trigger.status = TriggerStatus::Satisfied;
                super::finding(
                    out,
                    DiagnosticKind::Applied,
                    KEY,
                    format!("« {said} »: each run starts by hand, no cadence is bound."),
                );
                return;
            } else if cadence.is_some() || at.is_some() {
                trigger.cadence = cadence.map(str::to_owned);
                trigger.at = at;
                trigger.cron = schedule::fields(&answer);
                super::finding(
                    out,
                    DiagnosticKind::Applied,
                    KEY,
                    format!(
                        "« {said} » is the cadence: recorded on requested_trigger, never in the workflow bytes."
                    ),
                );
                return;
            } else {
                super::finding(
                    out,
                    DiagnosticKind::Missed,
                    KEY,
                    format!(
                        "« {said} » states no cadence: answer a period and, if wanted, a time (« chaque lundi à 9h », « every day at 18:00 »), or \"manual\"."
                    ),
                );
            }
        }
        Some(_) => super::finding(
            out,
            DiagnosticKind::Missed,
            KEY,
            "Answer the cadence as a JSON string (« chaque lundi à 9h », « every day at 18:00 », or \"manual\").",
        ),
        None => {}
    }
    let question = if unbound {
        format!(
            "The request says `{hint}`, a period a schedule cannot bind: it binds a day, a weekday, a named weekday at a time, or an hour or minute interval, never every other week or a count of days. How should it run? Answer a cadence it binds (« every Monday at 09:00 », « chaque lundi à 9h »), or \"manual\" to start each run by hand."
        )
    } else {
        format!(
            "The request says `{hint}` without saying when: how often should it run? A period and, if wanted, a time (« chaque lundi à 9h », « every day at 18:00 »), or \"manual\" to start each run by hand."
        )
    };
    super::question(out, KEY, &question, QuestionType::Text);
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
    if trigger.status == TriggerStatus::Satisfied {
        return format!(
            "`{}` is answered {}: each run starts when invoked and nothing is bound; the candidate's bytes carry no cadence, host or event.",
            trigger.source_hint.as_deref().unwrap_or_default(),
            trigger.kind.word()
        );
    }
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
            ("jeden montagmorgen", Some("weekly"), None),
            (
                "jeden freitagabend um 18 uhr",
                Some("weekly"),
                Some("18:00"),
            ),
            ("toda segunda-feira de manhã", Some("weekly"), None),
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

    #[test]
    fn a_recurrence_without_its_cadence_is_a_schedule_that_states_none() {
        for phrase in [
            "régulièrement",
            "de temps en temps",
            "regularly",
            "every so often",
            "on a regular basis",
        ] {
            assert_eq!(classify(phrase), TriggerForm::Schedule, "{phrase}");
            let trigger = read(phrase, false);
            assert_eq!(trigger.kind, TriggerKind::Schedule, "{phrase}");
            assert_eq!(trigger.cadence, None, "{phrase}");
            assert_eq!(trigger.at, None, "{phrase}");
            assert_eq!(trigger.status, TriggerStatus::RequiresBinding);
        }
        // Under an event head the recurrence describes the event, which stays the trigger.
        let event = read("quand un ticket arrive régulièrement", true);
        assert_eq!(event.kind, TriggerKind::Event);
        assert_eq!(
            classify("for each incoming ticket"),
            TriggerForm::Distributive
        );
        let words = phrase_words("chaque lundi a 9h");
        assert_eq!(
            stated_cadence(&words),
            (Some("weekly"), Some("09:00".to_owned()))
        );
        assert_eq!(stated_cadence(&phrase_words("bientot")), (None, None));
    }
}
