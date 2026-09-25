// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The money law as a decision the requester makes. An effect that moves money (a refund,
//! a payment, an order) is never constructed automatic: the request that states no human
//! approval before it gets ONE closed choice, `effect.<verb>.approval`, whose answer sets
//! the effect's policy at assembly (`human_first`: a person approves each one in a blocking
//! gate; `forbidden`: the workflow never runs it and the work before it still runs). The
//! answer is applied to the plan the assembler works on, never to the recorded plan, so an
//! answer round replays the same record and lands on the same question or its answer.
use super::plan::{EffectPolicy, Plan};
use super::types::{ChoiceOffer, DiagnosticKind};
use super::{CompileOutcome, CompileRequest};
use std::collections::BTreeSet;

const HUMAN_FIRST: &str = "human_first";
const FORBIDDEN: &str = "forbidden";

/// Decide every automatic money movement of the plan from its answer, or ask. Returns
/// whether a question is still open (the effect is then never bound).
pub(super) fn decide(
    plan: &mut Plan,
    request: &CompileRequest,
    out: &mut CompileOutcome,
    recognized: &mut BTreeSet<String>,
) -> bool {
    let mut pending = false;
    for effect in &mut plan.effects {
        if !(effect.verb.moves_money() && effect.policy == EffectPolicy::Automatic) {
            continue;
        }
        let verb = effect.verb.word();
        let key = format!("effect.{verb}.approval");
        recognized.insert(key.clone());
        let options = vec![
            ChoiceOffer::new(
                HUMAN_FIRST,
                format!("A person approves each `{verb}` in a blocking gate before it runs."),
            ),
            ChoiceOffer::new(
                FORBIDDEN,
                format!("The workflow never runs the `{verb}`; the work before it still runs."),
            ),
        ];
        let answered = request.answers.get(&key).map(String::as_str);
        match super::literal_answer(answered, &key, out) {
            Some(serde_json::Value::String(word)) if word.trim() == HUMAN_FIRST => {
                effect.policy = EffectPolicy::HumanFirst;
                super::finding(
                    out,
                    DiagnosticKind::Applied,
                    verb,
                    format!(
                        "`{verb}` gated by explicit answer: a person approves each one before it runs (the request stated no approval)."
                    ),
                );
            }
            Some(serde_json::Value::String(word)) if word.trim() == FORBIDDEN => {
                effect.policy = EffectPolicy::Forbidden;
                super::finding(
                    out,
                    DiagnosticKind::Applied,
                    verb,
                    format!(
                        "`{verb}` omitted by explicit answer: an automatic money movement is never constructed."
                    ),
                );
            }
            Some(_) => {
                super::finding(
                    out,
                    DiagnosticKind::Missed,
                    &key,
                    format!(
                        "Answer one of the offered keys as a JSON string: {HUMAN_FIRST} · {FORBIDDEN}."
                    ),
                );
                ask(out, &key, effect.target.trim(), verb, options);
                pending = true;
            }
            None => {
                ask(out, &key, effect.target.trim(), verb, options);
                pending = true;
            }
        }
    }
    pending
}

fn ask(out: &mut CompileOutcome, key: &str, target: &str, verb: &str, options: Vec<ChoiceOffer>) {
    super::choice_question(
        out,
        key,
        &format!(
            "`{target}` moves money and the request states no human approval before it. Choose `{HUMAN_FIRST}` (a person approves each `{verb}` in a blocking gate) or `{FORBIDDEN}` (the workflow never runs it)."
        ),
        "The compiler never constructs an automatic money movement; only a human-first version or its omission is constructible.",
        options,
    );
}
