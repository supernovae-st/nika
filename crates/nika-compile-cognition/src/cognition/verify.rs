// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Semantic verification of a candidate a model's plan produced (R4 A11). A bounded judge reads
//! the WHOLE request (as compiled and as first stated), its answers, the world the host observed
//! and the candidate's actual bytes, never the generator's summary alone. It settles each clause
//! the core could not witness from the bytes, then the request as a whole. Its answers become
//! judgments the core's READY law weighs under the binding it recomputes; a part the candidate
//! misses is a concrete defect the COLD door repairs from, within the policy's repairs; an
//! abstention or a failed judge keeps the request INCOMPLETE. Labels, task names, comments and
//! generator confidence are claims, never evidence; nothing here grants READY by itself.
//!
//! The judge is a decision seat the caller permits, or the authoring provider itself. The
//! provider is asked through the journaled authoring call: its calls, usage and failures ride
//! the authoring receipt beside every other call, under the same physical ceiling, and each
//! attempt states its own usage apart.

use nika_compile::surface::{Binding, Disposition, Judgment};
use nika_kernel::ai::provider::{InferResponse, Message, ProviderInferDyn, Role};
use serde_json::{Value, json};

use crate::decide::{
    self, ChoiceAnswer, ChoiceOption, ChoiceQuestion, DecisionError, DecisionSeat, NONE_OPTION,
};
use crate::lexicon::Reading;
use crate::plan::Plan;
use crate::{
    AuthoringPolicy, CompileError, CompileOutcome, CompileRequest, DiagnosticKind, Strategy,
};

/// Who judges a candidate: a decision seat the caller permits (its calls and usage are its own,
/// recorded here), or the authoring provider through the journaled authoring call.
enum Judge<'a, P: ProviderInferDyn> {
    Seat(&'a dyn DecisionSeat),
    Provider(&'a AuthoringPolicy, &'a P),
}

impl<P: ProviderInferDyn> Judge<'_, P> {
    fn name(&self) -> &str {
        match self {
            Self::Seat(seat) => seat.name(),
            Self::Provider(policy, _) => &policy.model,
        }
    }
    fn kind(&self) -> &'static str {
        match self {
            Self::Seat(_) => "decision_seat",
            Self::Provider(..) => "authoring_provider",
        }
    }
}

/// What the judge settled over one candidate.
#[derive(Default)]
pub(super) struct Verdict {
    /// The judgments the core's READY law weighs.
    pub(super) judgments: Vec<Judgment>,
    /// Parts of the request the candidate misses or does differently: repaired from.
    pub(super) defects: Vec<String>,
    /// What the judge could not settle (NONE, a failed or refused call).
    pub(super) unknown: Vec<String>,
    /// Each question as asked and answered (`decide::record`), with its role.
    pub(super) records: Vec<Value>,
    /// Calls attempted, answers returned, answers the door consumed.
    pub(super) attempted: u32,
    pub(super) returned: u32,
    pub(super) consumed: u32,
    /// What this attempt's judge calls cost ([`usage`]).
    pub(super) usage: Value,
}

const CLAUSE: &str = "Judge ONE clause of the user's request against the candidate workflow's actual bytes (candidate_nika). Read the whole request, the answers, the observed world and the candidate. carried: the candidate's program does exactly what this clause asks; an equivalent program counts (same rows, order, counts, values, effects and conditions). missing: the candidate omits the clause or does it differently (another order, count, negation, number or unit, target or condition). Task names, comments, labels and the words a step restates are claims, never evidence.";
const NO_OPERATION: &str = "the clause asks nothing of the workflow (a courtesy, a sentence about the data) and restricts nothing";
const WHOLE: &str = "Compare the WHOLE user request with the candidate workflow's actual bytes (candidate_nika). faithful: the program does everything the request asks, each operation in the stated order with the stated counts, negations, numbers and units, targets and conditions, and nothing it does not ask. unfaithful: anything is missing, extra or different. Task names, comments and labels are claims, never evidence.";
const LOCATE: &str = "The candidate does not carry the whole request. Choose the part of the request it misses or does differently.";

/// The state every question and every repair carries: the request as compiled and as first
/// stated, its answers, the observed world and the candidate's bytes. The first statement is
/// the one the binding holds ([`Binding::of`]): the preserved original request, else the
/// submitted text the compiled request was folded or clarified from.
fn state(intent: &str, request: &CompileRequest, candidate: &str) -> Value {
    let submitted = match &request.input {
        crate::types::Input::Create(text) if text != intent => Some(text.clone()),
        _ => None,
    };
    json!({
        "request": intent,
        "original_request": request.original_intent.clone().or(submitted),
        "answers": request.answers,
        "observed": request.knowledge,
        "candidate_nika": candidate,
    })
}

/// The parts of the request a localization offers: its own text split at punctuation, never a
/// reader's reading nor a proposal's region.
fn parts(intent: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    for part in intent.split([',', ';', '.', ':']) {
        let part = part.trim();
        let content = part.chars().filter(|c| c.is_alphanumeric()).count();
        if part.split_whitespace().count() >= 2 && content >= 4 && !parts.iter().any(|p| p == part)
        {
            parts.push(part.to_owned());
        }
    }
    parts.truncate(16);
    parts
}

/// The provider's answer as a seat's: its choice, the model asked, the usage it reported.
fn answered(policy: &AuthoringPolicy, response: &InferResponse, choice: String) -> ChoiceAnswer {
    let mut answer = ChoiceAnswer::new(choice, policy.model.clone());
    answer.input_tokens = response
        .usage_reported
        .then_some(response.usage.input_tokens);
    answer.output_tokens = response
        .usage_reported
        .then_some(response.usage.output_tokens);
    answer.reasoning = Some(crate::cognition::reasoning_record(
        policy.reasoning,
        Some(response),
    ));
    answer
}

/// Ask one question; every question and its answer (or its failure) is recorded, and a
/// provider's call is journaled in the authoring receipt under `role`.
async fn ask<P: ProviderInferDyn>(
    judge: &Judge<'_, P>,
    question: &ChoiceQuestion,
    role: &'static str,
    verdict: &mut Verdict,
    out: &mut CompileOutcome,
) -> Option<String> {
    verdict.attempted += 1;
    let (returned, answer) = match judge {
        Judge::Seat(seat) => {
            let answer = seat.choose(question).await;
            (answer.is_ok(), answer)
        }
        Judge::Provider(policy, provider) => {
            let (messages, schema) = decide::closed_choice(question);
            match super::call_with_schema(policy, *provider, role, messages, schema, out).await {
                Some(response) => (
                    true,
                    decide::decoded(question, &response)
                        .map(|choice| answered(policy, &response, choice)),
                ),
                None => (
                    false,
                    Err(DecisionError(
                        "the judge call got no answer; the receipt says why".to_owned(),
                    )),
                ),
            }
        }
    };
    let admitted = match &answer {
        Ok(answer) => decide::admit(question, answer).map(|()| answer.choice.clone()),
        Err(error) => Err(error.clone()),
    };
    let mut record = match (&answer, &admitted) {
        (Ok(answer), Ok(_)) => decide::record(question, Ok(answer)),
        (_, Err(error)) | (Err(error), _) => decide::record(question, Err(error)),
    };
    record["role"] = json!(role);
    if returned {
        verdict.returned += 1;
    }
    verdict.records.push(record);
    admitted.ok()
}

/// The authoring receipt's journal so far, in call order.
fn journal(out: &CompileOutcome) -> &[Value] {
    out.provenance
        .authoring
        .as_ref()
        .map_or(&[], |receipt| receipt.context.as_slice())
}

/// What one attempt's judge calls cost, apart from every other call (R4 A11). For the provider:
/// its own journal entries, their reported tokens and whether each answered with its usage or
/// was refused before any transport (`authority::usage_complete`). For a caller's seat: the
/// usage its answers reported. The tokens are a lower bound unless `complete`.
fn usage<P: ProviderInferDyn>(judge: &Judge<'_, P>, verdict: &Verdict, calls: &[Value]) -> Value {
    let (entries, complete): (Vec<&Value>, bool) = match judge {
        Judge::Provider(..) => (
            calls.iter().map(|entry| &entry["result"]).collect(),
            crate::authority::usage_complete(calls),
        ),
        Judge::Seat(_) => (
            verdict.records.iter().collect(),
            verdict
                .records
                .iter()
                .all(|record| record["input_tokens"].is_u64() && record["output_tokens"].is_u64()),
        ),
    };
    let sum = |key: &str| -> u64 { entries.iter().filter_map(|e| e[key].as_u64()).sum() };
    json!({
        "calls": verdict.attempted,
        "input_tokens": sum("input_tokens"),
        "output_tokens": sum("output_tokens"),
        "complete": complete,
    })
}

/// Judge every pending clause of the candidate the core named, the whole request among them,
/// under the binding of the plan the core assembled (its stated rules promoted). A provider's
/// calls are journaled into `out`.
async fn verdict_on<P: ProviderInferDyn>(
    intent: &str,
    request: &CompileRequest,
    plan: &Plan,
    settled: &CompileOutcome,
    judge: &Judge<'_, P>,
    out: &mut CompileOutcome,
) -> Verdict {
    let journaled = journal(out).len();
    let mut verdict = Verdict::default();
    let Some(candidate) = settled.candidate.as_deref() else {
        verdict.unknown.push(intent.to_owned());
        verdict.usage = usage(judge, &verdict, &[]);
        return verdict;
    };
    let mut assembled = plan.clone();
    crate::shape::promote_stated_rules(&mut assembled, intent);
    let binding = Binding::of(intent, request, &assembled, candidate);
    let open: Vec<Open> = settled
        .provenance
        .decision
        .as_ref()
        .and_then(|d| d["pending"]["open"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(Open::read)
        .collect();
    let base = state(intent, request, candidate);
    for (k, open) in open.iter().enumerate() {
        if open.spans.is_empty() {
            verdict.unknown.push(open.clause.clone());
        } else if open.spans == [(0, intent.len())] {
            whole(intent, &base, judge, &binding, &mut verdict, out).await;
        } else {
            judge_clause(open, (k, &base, &binding), judge, &mut verdict, out).await;
        }
    }
    verdict.defects.dedup();
    verdict.unknown.dedup();
    let calls = &journal(out)[journaled.min(journal(out).len())..];
    verdict.usage = usage(judge, &verdict, calls);
    verdict
}

/// A pending clause the core named: its text, whether no element claims it, and each statement
/// of it the core found in the request. A clause the request repeats has several statements; a
/// clause with none is asked nowhere and named as unsettled.
struct Open {
    clause: String,
    unclaimed: bool,
    spans: Vec<(usize, usize)>,
}

impl Open {
    fn read(item: &Value) -> Option<Self> {
        let at = |span: &Value, k: usize| usize::try_from(span[k].as_u64()?).ok();
        let spans = item["spans"]
            .as_array()
            .map(|spans| {
                spans
                    .iter()
                    .filter_map(|span| at(span, 0).zip(at(span, 1)))
                    .collect()
            })
            .unwrap_or_default();
        Some(Self {
            clause: item["clause"].as_str()?.to_owned(),
            unclaimed: item["witness"].is_null(),
            spans,
        })
    }
}

/// One pending clause judged at each of its statements (R4 A11): a judgment per statement
/// carried or asking for nothing, a defect when a statement misses it, an unknown when one is
/// not settled. `asked` is the clause's index, the base state and the candidate's binding.
async fn judge_clause<P: ProviderInferDyn>(
    open: &Open,
    (k, base, binding): (usize, &Value, &Binding),
    judge: &Judge<'_, P>,
    verdict: &mut Verdict,
    out: &mut CompileOutcome,
) {
    let clause = &open.clause;
    for (n, &span) in open.spans.iter().enumerate() {
        let mut options = vec![
            ChoiceOption::new(
                "carried",
                "the candidate does exactly what this clause asks",
            ),
            ChoiceOption::new("missing", "the candidate omits it or does it differently"),
        ];
        if open.unclaimed {
            options.push(ChoiceOption::new("no_operation", NO_OPERATION));
        }
        let mut asked = base.clone();
        asked["clause"] = json!({"text": clause, "span": [span.0, span.1]});
        let id = if n == 0 {
            format!("verify-clause-{k}")
        } else {
            format!("verify-clause-{k}.{n}")
        };
        let question = ChoiceQuestion::new(&id, CLAUSE, asked, options);
        let disposition = match ask(judge, &question, "judge_clause", verdict, out)
            .await
            .as_deref()
        {
            Some("carried") => Some(Disposition::Carried),
            Some("no_operation") => Some(Disposition::NoOperation),
            Some("missing") => {
                verdict.consumed += 1;
                verdict.defects.push(clause.clone());
                None
            }
            _ => {
                verdict.unknown.push(clause.clone());
                None
            }
        };
        if let Some(disposition) = disposition {
            verdict.consumed += 1;
            let judgment = Judgment::new(
                clause,
                span,
                disposition,
                judge.name(),
                &id,
                binding.clone(),
            );
            verdict.judgments.push(judgment);
        }
    }
}

/// The whole request against the candidate: faithful settles it; unfaithful names the part it
/// misses (a localization over the request's own text), a defect repaired from.
async fn whole<P: ProviderInferDyn>(
    intent: &str,
    base: &Value,
    judge: &Judge<'_, P>,
    binding: &Binding,
    verdict: &mut Verdict,
    out: &mut CompileOutcome,
) {
    let options = vec![
        ChoiceOption::new(
            "faithful",
            "the program does everything the request asks, nothing else",
        ),
        ChoiceOption::new(
            "unfaithful",
            "something the request asks is missing, extra or different",
        ),
    ];
    let question = ChoiceQuestion::new("verify-request", WHOLE, base.clone(), options);
    match ask(judge, &question, "judge_request", verdict, out)
        .await
        .as_deref()
    {
        Some("faithful") => {
            verdict.consumed += 1;
            verdict.judgments.push(Judgment::new(
                intent,
                (0, intent.len()),
                Disposition::Carried,
                judge.name(),
                "verify-request",
                binding.clone(),
            ));
        }
        Some("unfaithful") => {
            verdict.consumed += 1;
            let parts = parts(intent);
            let mut options: Vec<ChoiceOption> = parts
                .iter()
                .enumerate()
                .map(|(k, part)| ChoiceOption::new(format!("part-{k}"), part.clone()))
                .collect();
            options.push(ChoiceOption::new(
                "another_part",
                "a part not listed, or the request as a whole",
            ));
            let located = ChoiceQuestion::new("verify-locate", LOCATE, base.clone(), options);
            let part = match ask(judge, &located, "judge_locate", verdict, out).await {
                Some(key) if key != NONE_OPTION => key
                    .strip_prefix("part-")
                    .and_then(|k| k.parse::<usize>().ok())
                    .and_then(|k| parts.get(k).cloned()),
                _ => None,
            };
            verdict
                .defects
                .push(part.unwrap_or_else(|| intent.to_owned()));
        }
        _ => verdict.unknown.push(intent.to_owned()),
    }
}

/// The verifier's record of one attempt in the decision provenance: the judge, its calls
/// attempted, returned and consumed, their usage, each question, the defects.
fn record<P: ProviderInferDyn>(
    out: &mut CompileOutcome,
    judge: &Judge<'_, P>,
    verdict: &Verdict,
    attempt: usize,
) {
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    let entry = json!({
        "attempt": attempt,
        "judge": {"seat": judge.name(), "kind": judge.kind()},
        "attempted": verdict.attempted,
        "returned": verdict.returned,
        "consumed": verdict.consumed,
        "usage": verdict.usage,
        "questions": verdict.records,
        "defects": verdict.defects,
        "unknown": verdict.unknown,
    });
    if let Some(attempts) = decision["semantic_verification"].as_array_mut() {
        attempts.push(entry);
    } else {
        decision["semantic_verification"] = json!([entry]);
    }
    out.provenance.decision = Some(decision);
}

/// One more step of the route the decision records.
fn route(out: &mut CompileOutcome, step: &str) {
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    if let Some(route) = decision["route"].as_array_mut() {
        route.push(json!(step));
    } else {
        decision["route"] = json!([step]);
    }
    out.provenance.decision = Some(decision);
}

/// The duties the core named but no element of the seat's plan carries (it refused to emit):
/// concrete defects of the plan, the request's own words.
fn silent(out: &CompileOutcome) -> Vec<String> {
    let ledger = out
        .provenance
        .decision
        .as_ref()
        .and_then(|d| d["ledger"].as_array().cloned())
        .unwrap_or_default();
    let asks = out
        .questions
        .iter()
        .any(|q| q.mandatory && q.key != "intent.clarification");
    if asks || out.candidate.is_some() {
        return Vec::new();
    }
    ledger
        .iter()
        .filter(|duty| duty["state"] == "unresolved")
        .filter_map(|duty| duty["evidence"].as_str().map(str::to_owned))
        .collect()
}

/// The repair call: the verifier's concrete defects with the same state the judge read (the
/// request as compiled and as first stated, its answers, the observed world, the candidate's
/// bytes); the proposal it returns is merged as any other, never taken on its word.
#[allow(clippy::too_many_arguments)] // the COLD door's state the repair must carry whole
async fn repair<P: ProviderInferDyn>(
    intent: &str,
    policy: &AuthoringPolicy,
    provider: &P,
    reading: &Reading,
    request: &CompileRequest,
    defects: &[String],
    candidate: Option<&str>,
    pre: &mut CompileOutcome,
) -> Option<Plan> {
    let listed: Vec<String> = defects.iter().map(|d| format!("- {d}")).collect();
    let judged = state(intent, request, candidate.unwrap_or("(none was emitted)"));
    let text = format!(
        "VERIFIER: the workflow compiled from your plan was compared with the WHOLE request. It does not carry these parts of the request, or does them differently:\n{}\nReturn the complete corrected JSON plan for the whole request: every part carried, with the stated order, counts, negations, numbers and units, targets and conditions, every evidence an exact excerpt of the request.\n\nSTATE (the request as compiled and as first stated, its answers, the observed world, the candidate's bytes):\n{}",
        listed.join("\n"),
        serde_json::to_string_pretty(&judged).unwrap_or_default()
    );
    let mut messages = super::opening(intent);
    messages.push(Message::text(Role::User, text));
    let (proposal, _) = super::call(policy, provider, "repair", messages, pre).await?;
    let mut scratch = crate::initial();
    let plan = super::merge(intent, proposal, reading, &mut scratch);
    for finding in scratch
        .diagnostics
        .into_iter()
        .filter(|d| d.kind != DiagnosticKind::Applied)
    {
        pre.diagnostics.push(finding);
    }
    plan
}

/// The findings a blocked verification leaves: each part the judge found missing (repaired
/// from as the policy allowed) and each it could not settle, with the next action. No question
/// asks the human for information the request already gives.
fn blocked(out: &mut CompileOutcome, verdict: &Verdict, repairs: usize) {
    for defect in &verdict.defects {
        crate::finding(
            out,
            DiagnosticKind::Unknown,
            "semantic_verification",
            format!(
                "The judge compared the whole request with the candidate's bytes: it does not carry `{defect}`. {repairs} repair(s) from that defect did not settle it; nothing is READY. Next: a stronger authoring model, or a restatement of that part."
            ),
        );
    }
    for unknown in &verdict.unknown {
        crate::finding(
            out,
            DiagnosticKind::Unknown,
            "semantic_verification",
            format!(
                "The judge could not settle `{unknown}` against the candidate (it abstained or its call failed); nothing is READY on it. Next: a judge that answers, or a restatement the deterministic reader reads."
            ),
        );
    }
    out.questions.retain(|q| q.key != "intent.clarification");
}

/// A COLD candidate is judged before READY (R4 A11): a concrete defect (a part the judge finds
/// missing, a stated duty the plan leaves uncarried) is repaired from within the policy's
/// repairs, and the repaired plan's computations go through the transform seat again with the
/// judge's defects, so no program of the plan it replaced survives; an abstention, a failed
/// judge or exhausted repairs stay INCOMPLETE.
#[allow(clippy::too_many_arguments)] // the COLD door's own state, threaded once
pub(super) async fn judged_cold<P: ProviderInferDyn>(
    intent: &str,
    plan: Plan,
    policy: &AuthoringPolicy,
    provider: &P,
    seat: Option<&dyn DecisionSeat>,
    reading: &Reading,
    request: &CompileRequest,
    out: CompileOutcome,
) -> Result<CompileOutcome, CompileError> {
    let judge = match seat {
        Some(seat) => Judge::Seat(seat),
        None => Judge::Provider(policy, provider),
    };
    let mut pre = out;
    let mut plan = plan;
    let mut attempt = 0;
    loop {
        let settled =
            super::settle_judged(Strategy::Cold, &plan, intent, request, &[], pre.clone())?;
        let omitted = silent(&settled);
        let verdict = if omitted.is_empty() {
            let verdict = verdict_on(intent, request, &plan, &settled, &judge, &mut pre).await;
            record(&mut pre, &judge, &verdict, attempt);
            verdict
        } else {
            Verdict {
                defects: omitted,
                ..Verdict::default()
            }
        };
        if settled.candidate.is_some() && verdict.defects.is_empty() && verdict.unknown.is_empty() {
            route(&mut pre, &format!("verify: judged ({})", judge.kind()));
            return super::settle_judged(
                Strategy::Cold,
                &plan,
                intent,
                request,
                &verdict.judgments,
                pre,
            );
        }
        if settled.candidate.is_none() && verdict.defects.is_empty() {
            // A genuine question (a field, a count the request withholds): asked as it was.
            return Ok(settled);
        }
        if !verdict.defects.is_empty() && attempt < policy.repairs as usize {
            attempt += 1;
            route(&mut pre, &format!("verify: repair {attempt}"));
            let candidate = settled.candidate.as_deref();
            let defects = &verdict.defects;
            let repaired = repair(
                intent, policy, provider, reading, request, defects, candidate, &mut pre,
            );
            if let Some(mut next) = repaired.await {
                let synthesized = super::transform::synthesize(
                    intent, &mut next, policy, provider, request, defects, &mut pre,
                );
                if let Some(mut pending) = synthesized.await {
                    pending.answer(request, &mut pre);
                    pending.suspend(&next, &mut pre);
                    return Ok(pre);
                }
                plan = next;
                continue;
            }
        }
        route(&mut pre, "verify: not ready");
        let mut out = super::settle_judged(Strategy::Cold, &plan, intent, request, &[], pre)?;
        blocked(&mut out, &verdict, attempt);
        return Ok(out);
    }
}

/// A replayed record under this round's judge (R4 A11, Q2): deterministically closed duties
/// replay as they are, with no call; the remainder the core names (`pending.open`) is judged
/// by the round's judge (its seat, else its policy's provider through the journaled call) and
/// the record replays again under those active judgments. No judge, or a remainder the judge
/// finds missing or cannot settle, keeps the outcome INCOMPLETE naming it: an answer round
/// makes no proposal, so nothing is repaired here, and nothing a record carries is read as a
/// judgment. With `whole` (the first candidate of a model's plan) the whole request is part
/// of the remainder.
pub(super) async fn replayed<P: ProviderInferDyn>(
    intent: &str,
    saved: &Value,
    request: &CompileRequest,
    judges: (Option<&dyn DecisionSeat>, Option<(&AuthoringPolicy, &P)>),
    whole: bool,
    out: CompileOutcome,
) -> Result<CompileOutcome, CompileError> {
    let folded = crate::lexicon::fold_apostrophes(intent);
    let intent = folded.as_str();
    let before = out.clone();
    let mut out = out;
    crate::replay_judged(intent, saved, request, &[], whole, &mut out)?;
    let open = out
        .provenance
        .decision
        .as_ref()
        .and_then(|d| d["pending"]["open"].as_array())
        .is_some_and(|open| !open.is_empty());
    let judge = match judges {
        (Some(seat), _) => Judge::Seat(seat),
        (None, Some((policy, provider))) => Judge::Provider(policy, provider),
        (None, None) => return Ok(out),
    };
    let Ok(plan) = Plan::from_json(saved) else {
        return Ok(out);
    };
    if out.candidate.is_none() || !open {
        return Ok(out);
    }
    let mut pre = before;
    let verdict = verdict_on(intent, request, &plan, &out, &judge, &mut pre).await;
    record(&mut pre, &judge, &verdict, 0);
    if verdict.defects.is_empty() && verdict.unknown.is_empty() {
        route(&mut pre, &format!("verify: judged ({})", judge.kind()));
        crate::replay_judged(intent, saved, request, &verdict.judgments, whole, &mut pre)?;
        return Ok(pre);
    }
    route(&mut pre, "verify: not ready");
    crate::replay_judged(intent, saved, request, &[], whole, &mut pre)?;
    blocked(&mut pre, &verdict, 0);
    Ok(pre)
}

/// A WARM candidate is judged by the seat that settled its readings (R4 A11); a part it finds
/// missing, or one it cannot settle, keeps the request INCOMPLETE (WARM makes no proposal).
pub(super) async fn judged_warm(
    intent: &str,
    plan: &Plan,
    seat: &dyn DecisionSeat,
    request: &CompileRequest,
    out: CompileOutcome,
) -> Result<CompileOutcome, CompileError> {
    let settled = super::settle_judged(Strategy::Warm, plan, intent, request, &[], out.clone())?;
    if settled.candidate.is_none() {
        return Ok(settled);
    }
    let judge = Judge::<super::NoProvider>::Seat(seat);
    let mut pre = out;
    let verdict = verdict_on(intent, request, plan, &settled, &judge, &mut pre).await;
    record(&mut pre, &judge, &verdict, 0);
    if verdict.defects.is_empty() && verdict.unknown.is_empty() {
        route(&mut pre, "verify: judged (decision_seat)");
        return super::settle_judged(
            Strategy::Warm,
            plan,
            intent,
            request,
            &verdict.judgments,
            pre,
        );
    }
    route(&mut pre, "verify: not ready");
    let mut blocked_out = super::settle_judged(Strategy::Warm, plan, intent, request, &[], pre)?;
    blocked(&mut blocked_out, &verdict, 0);
    Ok(blocked_out)
}
