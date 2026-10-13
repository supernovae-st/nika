// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Aggregate independently audited program obligations without turning preparation into code.
//! The same localization, extra-effect, model dataflow and trial laws still qualify the bytes.

use serde_json::Value;

use super::{Asked, Effects, Part, State, locate, model_parts, parsed_tasks, prefetch};
use crate::CompileOutcome;
use crate::cognition::conversation::interpretation;
use crate::cognition::verify::{Judge, Verdict};
use crate::decide::{ChoiceOption, ChoiceQuestion};
use nika_compile_clauses::parts::restricts;
use nika_kernel::ai::provider::ProviderInferDyn;

const OBLIGATION: &str = "Judge only the concrete `obligation` against candidate_nika's actual behavior. Full request, original_request, scoped answers and interpretation explain the obligation's provenance; preparation requests are not program operations. carried: the program implements this obligation with its exact conditions, values, order, units, data and effects. missing: it omits the obligation or implements it differently. Task names and comments are claims, not evidence. For work explicitly delegated to a model, a model step on the data path carries the requirement when its prompt and surrounding steps request it; this judgment cannot prove the model's future text. Use the complete normative language/tool reference. Choose none when the bytes and observations do not settle it. This question grants no Save, Run or authority.";
const SCOPE: &str = "Here the program claim is the complete independently audited runtime_obligations in request_interpretation.reading, together with the exact scoped human answers. The original request remains present as evidence. Preparation requests concern this Session, not operations to add to the workflow. No preparation act is certified as accomplished by this program judgment. An explicit per-execution interaction or persistent restriction remains a program obligation. presentation.pending_choices, if present, are actual alternatives offered by the host but not selected by the person: do not invent a human selection or reject a proposed default merely because its alternatives remain open. Judge program fidelity and effects, not permission to save or run the proposal.";

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
    reading["reading"]["runtime_obligations"]
        .as_array()?
        .iter()
        .map(|obligation| {
            let text = obligation["requirement"].as_str()?.to_owned();
            Some(Part {
                restricting: restricts(&text),
                text,
                state: State::Unknown,
            })
        })
        .collect()
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
    model_parts::settle(intent, judge, paths.as_ref(), &sinks, verdict);
    if verdict.settled_by.is_some() && verdict.settled() {
        verdict
            .engine
            .push(serde_json::json!({"claim":"audited program obligations",
            "program_obligations":parts.iter().map(|part| &part.text).collect::<Vec<_>>(),
            "preparation_fulfillment":false,"authority":false}));
    }
}
