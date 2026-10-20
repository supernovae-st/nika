// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Aggregate independently audited program obligations without turning preparation into code.
//! The same localization, extra-effect, model dataflow and trial laws still qualify the bytes.
//!
//! A trigger is deployment, never the candidate's bytes (#1720): an obligation whose every
//! evidence quote lies in the words of the schedule the proposal carries beside the candidate
//! (`presentation.schedule`, as the host accepted it) is carried there, settled with no call
//! and recorded on the verdict's engine facts. Evidence beyond those words keeps an obligation
//! asked; with no schedule beside the candidate every obligation is asked, and one only whole
//! schedule words evidence is told, when judged missing, where such words ride.
//!
//! A Run ceiling is the same (QA gauntlet q09): no workflow holds a dollar cap per run, the Run
//! does. An obligation whose every evidence quote lies in the words of the request that state the
//! ceiling the proposal binds beside the candidate (`presentation.money`) is carried there.

use serde_json::{Value, json};

use super::{Asked, Effects, OMITTED, Part, State, locate, model_parts, parsed_tasks, prefetch};
use crate::CompileOutcome;
use crate::cognition::conversation::interpretation;
use crate::cognition::verify::{Judge, Verdict};
use crate::decide::{ChoiceOption, ChoiceQuestion};
use crate::fidelity::resolution::anchored;
use nika_compile::trigger_of;
use nika_compile_clauses::parts::restricts;
use nika_kernel::ai::provider::ProviderInferDyn;

const OBLIGATION: &str = "Judge only the concrete `obligation` against candidate_nika's actual behavior. Full request, original_request, scoped answers and interpretation explain the obligation's provenance; preparation requests are not program operations. carried: the program implements this obligation with its exact conditions, values, order, units, data and effects. missing: it omits the obligation or implements it differently. Task names and comments are claims, not evidence. For work explicitly delegated to a model, a model step on the data path carries the requirement when its prompt and surrounding steps request it; this judgment cannot prove the model's future text. Use the complete normative language/tool reference. Choose none when the bytes and observations do not settle it. This question grants no Save, Run or authority.";
const SCOPE: &str = "Here the program claim is the complete independently audited runtime_obligations in request_interpretation.reading, together with the exact scoped human answers. The original request remains present as evidence. Preparation requests concern this Session, not operations to add to the workflow. No preparation act is certified as accomplished by this program judgment. An explicit per-execution interaction or persistent restriction remains a program obligation. presentation.pending_choices, if present, are actual alternatives offered by the host but not selected by the person: do not invent a human selection or reject a proposed default merely because its alternatives remain open. Judge program fidelity and effects, not permission to save or run the proposal.";

/// What a judged-missing obligation is told when only words stating a whole schedule evidence it.
const BESIDE_HINT: &str = "it says when the work runs, which a workflow never holds: carry the person's words for it as `propose.schedule` beside the candidate";

pub(super) fn protocol() -> String {
    format!("{OBLIGATION}\n{SCOPE}")
}

pub(super) fn framed(base: &Value, question: &str) -> String {
    if base.get("request_interpretation").is_some() {
        format!("{SCOPE}\n{question}")
    } else {
        question.to_owned()
    }
}

pub(super) fn parts(base: &Value) -> Option<Vec<Part>> {
    let reading = &base["request_interpretation"];
    if !interpretation::covered(reading) {
        return None;
    }
    let carried: Vec<&str> = [schedule(base), ceiling(base)]
        .into_iter()
        .flatten()
        .collect();
    let usd = base["presentation"]["money"]["usd"].as_f64();
    reading["reading"]["runtime_obligations"]
        .as_array()?
        .iter()
        .map(|obligation| {
            let text = obligation["requirement"].as_str()?.to_owned();
            let beside = |quote: &str| {
                carried.iter().any(|words| anchored(words, quote))
                    || usd.is_some_and(|usd| ceiling_talk(quote, usd))
            };
            // A requirement that states only the Run ceiling beside the candidate is carried
            // there, whatever words its quote spans (live gauntlet wave 7 r09, 2026-10-19: the
            // quote held the model clause too, and the first proposal was blocked twice).
            let ceiling_only = usd.is_some_and(|usd| ceiling_talk(&text, usd));
            let state = if ceiling_only || quoted(obligation, beside) {
                State::Beside
            } else {
                State::Unknown
            };
            Some(Part {
                restricting: restricts(&text),
                text,
                state,
            })
        })
        .collect()
}

/// The words of the schedule the proposal carries beside its candidate, as the host accepted
/// them (`presentation.schedule.excerpt`).
fn schedule(base: &Value) -> Option<&str> {
    (base["presentation"]["schedule"]["excerpt"].as_str()).filter(|words| !words.trim().is_empty())
}

/// The words of the request that state the Run ceiling the proposal binds beside its candidate
/// (`presentation.money.excerpt`).
fn ceiling(base: &Value) -> Option<&str> {
    (base["presentation"]["money"]["excerpt"].as_str()).filter(|words| !words.trim().is_empty())
}

/// The words a Run ceiling is said with, beside its amount and currency (French and English).
const CEILING_TALK: &[&str] = &[
    "le",
    "la",
    "les",
    "l",
    "un",
    "une",
    "de",
    "du",
    "des",
    "d",
    "au",
    "aux",
    "à",
    "a",
    "par",
    "pour",
    "chaque",
    "chacun",
    "chacune",
    "run",
    "runs",
    "exécution",
    "exécutions",
    "execution",
    "executions",
    "lancement",
    "lancer",
    "lance",
    "lancé",
    "s",
    "se",
    "applique",
    "appliquer",
    "appliqué",
    "appliquée",
    "plafond",
    "plafonds",
    "budget",
    "budgets",
    "maximum",
    "maximal",
    "maximale",
    "max",
    "ne",
    "pas",
    "plus",
    "dépasser",
    "dépasse",
    "doit",
    "doivent",
    "coûter",
    "coûte",
    "coût",
    "couter",
    "coute",
    "cout",
    "respecter",
    "respecte",
    "rester",
    "reste",
    "sous",
    "en",
    "limite",
    "limiter",
    "est",
    "sera",
    "soit",
    "qui",
    "que",
    "qu",
    "il",
    "elle",
    "ce",
    "cette",
    "cet",
    "fois",
    "the",
    "an",
    "of",
    "per",
    "each",
    "every",
    "launch",
    "at",
    "applies",
    "apply",
    "applied",
    "ceiling",
    "cap",
    "most",
    "must",
    "should",
    "cost",
    "costs",
    "stay",
    "stays",
    "within",
    "under",
    "not",
    "exceed",
    "exceeds",
    "limited",
    "is",
    "be",
    "it",
    "its",
    "to",
    "on",
    "by",
    "no",
    "more",
    "than",
    "dollar",
    "dollars",
    "usd",
    "us",
    "$",
];

/// Whether `quote` says only a Run ceiling of `usd` dollars: that amount, its currency and the
/// words a ceiling is said with, nothing of the program (live gauntlet wave 6 r09 B1, 2026-10-18:
/// « le plafond de 0,10 dollar s'applique au lancement du run », the person's later words for
/// the same ceiling, lay outside the words beside the candidate, and every revision was refused
/// for a ceiling no task can carry). Another amount says another fact.
fn ceiling_talk(quote: &str, usd: f64) -> bool {
    let lower = quote.replace(['\u{2019}', '\u{2018}'], "'").to_lowercase();
    let tokens: Vec<&str> = (lower.split(|c: char| c.is_whitespace() || c == '\''))
        .map(|token| token.trim_matches(|c: char| "()«»\"“”,;:!?.".contains(c)))
        .filter(|token| !token.is_empty())
        .collect();
    let mut said = false;
    for (k, token) in tokens.iter().enumerate() {
        let number = token.trim_matches('$').replace(',', ".");
        let Ok(value) = number.parse::<f64>() else {
            if CEILING_TALK.contains(token) || matches!(*token, "cents" | "centimes") {
                continue;
            }
            return false;
        };
        let cents = (tokens.get(k + 1)).is_some_and(|next| matches!(*next, "cents" | "centimes"));
        let value = if cents { value / 100.0 } else { value };
        if (value - usd).abs() > 1e-9 {
            return false;
        }
        said = true;
    }
    said
}

/// Whether `obligation` has evidence and every quote of it satisfies `holds`.
fn quoted(obligation: &Value, holds: impl Fn(&str) -> bool) -> bool {
    obligation["evidence"].as_array().is_some_and(|quotes| {
        !quotes.is_empty()
            && quotes
                .iter()
                .all(|cite| cite["quote"].as_str().is_some_and(&holds))
    })
}

/// Each obligation carried beside the candidate, as an engine fact: the question it never asks,
/// the obligation and the schedule that carries it.
fn beside(parts: &[Part], base: &Value, verdict: &mut Verdict) {
    for (k, part) in parts.iter().enumerate() {
        if matches!(part.state, State::Beside) {
            verdict
                .engine
                .push(json!({"question": format!("verify-part-{k}"),
                "settled": "beside_candidate", "by": "engine", "obligation": part.text,
                "schedule": base["presentation"]["schedule"],
                "money": base["presentation"]["money"]}));
        }
    }
}

/// The texts of the obligations carried beside the candidate (`beside`), or of the program's.
fn texts(parts: &[Part], beside: bool) -> Vec<&str> {
    (parts.iter())
        .filter(|part| matches!(part.state, State::Beside) == beside)
        .map(|part| part.text.as_str())
        .collect()
}

/// The note of each obligation judged missing because no task performs it, when every quote
/// evidencing it states a whole schedule (its cron fields): where such words ride instead.
fn hinted(base: &Value, verdict: &mut Verdict) {
    let obligations = base["request_interpretation"]["reading"]["runtime_obligations"].as_array();
    let timed = |text: &str| {
        (obligations.into_iter().flatten()).any(|obligation| {
            obligation["requirement"] == text
                && quoted(obligation, |quote| {
                    trigger_of(quote).is_some_and(|trigger| trigger.cron.is_some())
                })
        })
    };
    for (defect, note) in &mut verdict.notes {
        if let Some(over) = note.strip_suffix(OMITTED)
            && timed(defect)
        {
            *note = format!("{over}{BESIDE_HINT}");
        }
    }
}

pub(super) fn question(k: usize, base: &Value, reference: &str) -> Option<ChoiceQuestion> {
    let obligation = base["request_interpretation"]["reading"]["runtime_obligations"].get(k)?;
    let mut state = base.clone();
    state["obligation"] = obligation.clone();
    let options = vec![
        ChoiceOption::new(
            "carried",
            "The actual program carries this concrete obligation precisely.",
        ),
        ChoiceOption::new(
            "missing",
            "The actual program omits or violates this concrete obligation.",
        ),
    ];
    Some(ChoiceQuestion::new(
        format!("verify-part-{k}"),
        format!("{OBLIGATION}\n\n{reference}"),
        state,
        options,
    ))
}

pub(super) async fn whole<P: ProviderInferDyn>(
    intent: &str,
    (base, reference): (&Value, &str),
    judge: &Judge<'_, P>,
    observation: Option<&Value>,
    verdict: &mut Verdict,
    out: &mut CompileOutcome,
) {
    verdict.request = Some(intent.to_owned());
    let Some(parts) = parts(base).filter(|parts| !parts.is_empty()) else {
        verdict
            .unknown
            .push("no complete audited program obligations".to_owned());
        return;
    };
    beside(&parts, base, verdict);
    let candidate = base["candidate_nika"].as_str().unwrap_or_default();
    let paths = model_parts::walk(candidate);
    let sinks = model_parts::sink_questions(judge, paths.as_ref(), base, intent);
    if !sinks.is_empty() {
        prefetch(judge, "verify-model-parts", &sinks, verdict).await;
        model_parts::take(judge, &sinks, verdict, out).await;
        verdict.stopped |= sinks.iter().any(|question| {
            !verdict
                .records
                .iter()
                .any(|record| record["question"] == question.id && record["choice"].is_string())
        });
    }
    if verdict.stopped {
        verdict.unknown.push(intent.to_owned());
        model_parts::settle(intent, judge, paths.as_ref(), &sinks, verdict);
        return;
    }
    let (tasks, effects) = (parsed_tasks(candidate), Effects::of(candidate, intent));
    let asked = Asked {
        base,
        reference,
        judge,
        tasks: tasks.as_deref().unwrap_or_default(),
        intent,
        effects: effects.as_ref(),
    };
    if let Some(by) = Box::pin(locate(tasks.as_deref(), &asked, observation, verdict, out)).await {
        let by = if by == "verify-parts" {
            "verify-obligations"
        } else {
            by
        };
        verdict.settled_by = Some(by);
    }
    hinted(base, verdict);
    model_parts::settle(intent, judge, paths.as_ref(), &sinks, verdict);
    if verdict.settled_by.is_some() && verdict.settled() {
        let mut claim = json!({"claim":"audited program obligations",
            "program_obligations":texts(&parts, false),
            "preparation_fulfillment":false,"authority":false});
        let carried_beside = texts(&parts, true);
        if !carried_beside.is_empty() {
            claim["beside_candidate"] = json!(carried_beside);
        }
        verdict.engine.push(claim);
    }
}

#[cfg(test)]
mod ceiling_tests {
    use super::{State, ceiling_talk, parts};
    use serde_json::json;

    /// A requirement that states only the Run ceiling beside the candidate is carried there,
    /// though its quote spans a clause of the program too; the program's clause stays asked, and
    /// another amount than the one beside keeps it asked (live gauntlet wave 7 r09, 2026-10-19).
    #[test]
    fn a_requirement_that_states_only_the_run_ceiling_is_carried_beside() {
        let sentence = "L'étape IA doit utiliser Claude Haiku (le moins cher) et chaque run doit \
                        coûter au maximum 0,10 $";
        let base = |usd: f64| {
            json!({"request_interpretation": {"valid": true,
                "audit": {"status": "covered", "defects": []},
                "reading": {"runtime_obligations": [
                    {"requirement": "Each run must cost at most $0.10 (maximum budget $0.10 per run).",
                     "evidence": [{"quote": sentence}]},
                    {"requirement": "The AI step uses Claude Haiku.",
                     "evidence": [{"quote": sentence}]}
                ]}},
                "presentation": {"money": {"excerpt": "au maximum 0,10 $", "usd": usd}}})
        };
        let read = parts(&base(0.1)).expect("covered");
        assert!(matches!(read[0].state, State::Beside), "{:?}", read[0].text);
        assert!(
            !matches!(read[1].state, State::Beside),
            "{:?}",
            read[1].text
        );
        let other = parts(&base(0.25)).expect("covered");
        assert!(
            !matches!(other[0].state, State::Beside),
            "another amount stays asked"
        );
    }

    /// A quote that says only the Run ceiling of the amount beside the candidate is carried
    /// there; a word of the program, or another amount, keeps it asked.
    #[test]
    fn a_quote_that_says_only_the_run_ceiling_is_carried_beside() {
        for quote in [
            "le plafond de 0,10 dollar s'applique au lancement du run",
            "Budget max 0,10 dollar par run",
            "each run must cost at most $0.10",
            "10 cents par run",
        ] {
            assert!(ceiling_talk(quote, 0.1), "{quote}");
        }
        for quote in [
            "y compris avec Sonnet 5.5, un plafond de 0,10 dollar",
            "un plafond de 0,25 dollar par run",
            "écris le rapport dans rapport.md",
            "le plafond",
        ] {
            assert!(!ceiling_talk(quote, 0.1), "{quote}");
        }
    }
}
