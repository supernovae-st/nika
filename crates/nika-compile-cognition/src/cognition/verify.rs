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
//! Every question and the repair also carry one compiler-owned reference, apart from that state
//! ([`grounding`](fn@grounding), E36): the engine's output conventions, the language in one page
//! and the whole contract of each tool the candidate reaches by the checker's own capability
//! inference over the parsed workflow. The verdict records the digest of the reference text sent
//! and each piece's receipt, and every call that carried it journals the same receipts.
//!
//! The judge is a decision seat the caller permits, or the authoring provider itself. The
//! provider is asked through the journaled authoring call: its calls, usage and failures ride
//! the authoring receipt beside every other call, under the same physical ceiling, and each
//! attempt states its own usage apart.

use nika_compile::surface::{Binding, Disposition, Judgment};
use nika_kernel::ai::provider::{InferResponse, Message, ProviderInferDyn, Role};
use serde_json::{Value, json};

use super::knowledge;
use super::rehearsal::Rehearsals;
use crate::decide::{
    self, ChoiceAnswer, ChoiceOption, ChoiceQuestion, DecisionError, DecisionSeat,
};
use crate::lexicon::Reading;
use crate::plan::Plan;
use crate::types::{EditChange, Input};
use crate::{
    AuthoringPolicy, CompileError, CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind,
    Strategy,
};

/// The questions the historical estimates count for one whole-request judgment ([`whole`]): the
/// verdict, then, when it does not carry the request, one part of it asked alone. A part judged
/// missing adds the question of the task it points to (a located defect takes three), and every
/// other part, the extra-operation question and the questions over a trial run ([`faithful`])
/// add their own, each counted by the authority that sends it: the number depends on the
/// request, and no estimate bounds it.
pub(crate) const WHOLE_QUESTIONS: usize = 2;

mod faithful;
mod grounding;
mod held;
use faithful::{Pointed, whole};
use grounding::grounding;
use held::held_text;
pub(super) use held::{HELD_TARGET, held, preserve_unjudged, withdrawn};
use nika_compile_clauses::parts::{parts, restricts};

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
    /// The reference its questions carried ([`grounding`](fn@grounding)): the engine identity,
    /// the digest and size of the text sent, each piece's receipt and the tools the candidate
    /// reaches.
    pub(super) reference: Value,
    /// Duties the core named that no element of the plan carries, with their kind ([`silent`]):
    /// repaired from as the judge's defects are, but no judge was asked (B21 T3).
    pub(super) named: Vec<(String, String)>,
    /// The whole-request answers that did not carry the request (`unfaithful`, `none`).
    pub(super) doubt: Vec<String>,
    /// What the whole verdict and its localization disagree on with no defect located (R6):
    /// never READY, never repaired from, kept for an observation or a correction.
    pub(super) contested: Vec<String>,
    /// Why a disagreement stayed contested: no observation of these bytes, or one that could
    /// not decide.
    pub(super) unsettled: Vec<String>,
    /// The reason the judge gave for a defect: the task it points to, or no task performing it.
    pub(super) notes: Vec<(String, String)>,
    /// Whether the whole request was asked in this verdict, and the question that carried it.
    pub(super) whole_asked: bool,
    pub(super) settled_by: Option<&'static str>,
    /// The sha256 of the candidate bytes this verdict judged.
    pub(super) candidate_sha256: Option<String>,
    /// Whether the judge answered these bytes and did not accept them, and how: they are never
    /// asked of it again (R6).
    pub(super) declined: Declined,
    /// Whether a call got no answer (refused or failed): nothing more was asked after it.
    pub(super) stopped: bool,
    /// The whole request this verdict asked, when it asked it.
    pub(super) request: Option<String>,
    /// The earlier attempt of this compile on the same bytes and judge whose verdict this one
    /// repeats, with no call (R6).
    pub(super) same_bytes_as: Option<u64>,
    /// Whether this verdict repeats a rejection the host carried from an earlier round of the
    /// conversation (`CompileRequest::declined`), with no call (R6).
    pub(super) carried: bool,
    /// The digest of the context the judge read beside the bytes (the request as compiled and
    /// as first stated, the answers, the observed world): a carried rejection binds to it.
    pub(super) context_sha256: Option<String>,
    /// The questions of the attempt whose unfinished localization this verdict resumes, as
    /// asked and answered: each answer is read back with no call, never asked again (R6).
    pub(super) earlier: Vec<Value>,
    /// The answers read back from `earlier`.
    pub(super) read_back: u32,
    /// What a decision seat answered the questions of one step asked together (A1), by
    /// question id: each taken in its turn as the verdict asks that question.
    pub(super) prefetched: Vec<(String, Result<ChoiceAnswer, DecisionError>)>,
}

/// What the judge's admitted answers did to a candidate's bytes, by strength.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Declined {
    /// Every admitted answer accepted them, or none was admitted (a failed or refused call).
    #[default]
    No,
    /// An answer abstained (NONE) and none rejected them.
    Abstained,
    /// An answer rejected them (`unfaithful`, `missing`, a task named).
    Rejected,
}

impl Verdict {
    /// Note how an admitted answer declined these bytes; the strongest answer stands.
    pub(super) fn decline(&mut self, how: Declined) {
        self.declined = self.declined.max(how);
    }

    /// The answers this verdict holds: returned by a call, or read back from the attempt it
    /// resumes. A question that adds none got no answer: the localization stops there.
    pub(super) fn answers(&self) -> u32 {
        self.returned + self.read_back
    }

    /// Whether an admitted answer rejected these bytes (an abstention alone rejects nothing).
    pub(super) fn rejected(&self) -> bool {
        self.declined == Declined::Rejected
    }

    /// Whether nothing stands against READY: no defect, no unknown, nothing contested.
    pub(super) fn settled(&self) -> bool {
        self.defects.is_empty() && self.unknown.is_empty() && self.contested.is_empty()
    }

    /// Whether the judge answered these bytes and did not accept them, and nothing settled them
    /// since: they are never replayed to the same judge, so no later answer of it outvotes this
    /// one (R6). A judge that answered nothing (a failed or refused call) declined nothing.
    pub(super) fn doubted(&self) -> bool {
        self.declined != Declined::No && !self.settled()
    }

    /// The verdict an earlier attempt of this compile recorded (`semantic_verification`), read
    /// back with no call: what it found, never a judgment.
    fn recorded(attempt: &Value, index: u64) -> Self {
        let strings = |key: &str| -> Vec<String> {
            (attempt[key].as_array().into_iter().flatten())
                .filter_map(|entry| entry.as_str().map(str::to_owned))
                .collect()
        };
        let notes = (attempt["notes"].as_array().into_iter().flatten())
            .filter_map(|note| {
                let defect = note["defect"].as_str()?.to_owned();
                Some((defect, note["note"].as_str()?.to_owned()))
            })
            .collect();
        Self {
            defects: strings("defects"),
            unknown: strings("unknown"),
            doubt: strings("doubt"),
            contested: strings("contested"),
            unsettled: strings("unsettled"),
            notes,
            whole_asked: attempt["whole_asked"] == Value::Bool(true),
            candidate_sha256: attempt["candidate_sha256"].as_str().map(str::to_owned),
            declined: if attempt["rejected"] == Value::Bool(true) {
                Declined::Rejected
            } else {
                Declined::Abstained
            },
            stopped: attempt["stopped"] == Value::Bool(true),
            request: attempt["request"].as_str().map(str::to_owned),
            context_sha256: attempt["context_sha256"].as_str().map(str::to_owned),
            earlier: (attempt["questions"].as_array().cloned()).unwrap_or_default(),
            same_bytes_as: Some(index),
            reference: attempt["reference"].clone(),
            usage: json!({"calls": 0, "input_tokens": 0, "output_tokens": 0, "complete": true}),
            ..Self::default()
        }
    }

    /// Each defect as a repair reads it: the part, then the judge's reason when it gave one.
    fn noted_defects(&self) -> Vec<String> {
        (self.defects.iter())
            .map(
                |defect| match self.notes.iter().find(|(d, _)| d == defect) {
                    Some((_, note)) => format!("{defect} ({note})"),
                    None => defect.clone(),
                },
            )
            .collect()
    }

    /// Every part of the request a repair starts from: the judge's defects, then the duties the
    /// core named.
    fn parts(&self) -> Vec<String> {
        let named = self.named.iter().map(|(evidence, _)| evidence.clone());
        self.defects.iter().cloned().chain(named).collect()
    }
}

const CLAUSE: &str = "Judge ONE clause of the user's request against the candidate workflow's actual bytes (candidate_nika). Read the whole request, the answers, the observed world and the candidate. carried: the candidate's program does exactly what this clause asks; an equivalent program counts (same rows, order, counts, values, effects and conditions). missing: the candidate omits the clause or does it differently (another order, count, negation, number or unit, target or condition). Task names, comments, labels and the words a step restates are claims, never evidence.";
const NO_OPERATION: &str = "the clause asks nothing of the workflow (a courtesy, a sentence about the data) and restricts nothing";
const WHOLE: &str = "Compare the WHOLE user request with the candidate workflow's actual bytes (candidate_nika). faithful: the program does everything the request asks, each operation in the stated order with the stated counts, negations, numbers and units, targets and conditions, and nothing it does not ask. unfaithful: anything is missing, extra or different. Task names, comments and labels are claims, never evidence.";
/// What a whole-request question over a revision adds to its instructions ([`whole`]).
const REVISED: &str = "This candidate REVISES an earlier workflow. `request` is the whole revised request it must carry: the earlier request with each clause the change replaces replaced in place, then the change's additions; every other earlier clause is still asked. `revision.change` is the change as the human stated it; `revision.base_request` is the earlier request, history only: a clause the change replaced is no longer asked. A clause asking to create or modify the workflow file itself is carried by this candidate being that workflow; every other clause is judged on what its bytes do.";
/// What a whole-request question over a revision whose request is the earlier request followed
/// by the change (`… Change: …`) adds to its instructions ([`whole`]).
const REVISED_APPENDED: &str = "This candidate REVISES an earlier workflow. `request` is the earlier request followed by the change the human stated (« Change: … »): where they differ the change takes precedence, so a clause of the earlier request the change replaces is superseded and no longer asked; every other earlier clause is still asked. `revision.change` is the change as stated. A clause asking to create or modify the workflow file itself is carried by this candidate being that workflow; every other clause is judged on what its bytes do.";
/// What a whole-request question over any other candidate adds to its instructions ([`whole`]):
/// a request to author this workflow (« create report.nika that … ») asks for this program, not
/// for a step writing its own file. It attests no save, path or name: a stated name is judged on
/// the bytes, and every write the program itself does (another `.nika` too) stays judged.
const CREATED: &str = "This candidate is the workflow the request asks Nika to author. A clause asking to create this workflow asks for this program; it does not ask the program to write its own file. Saving that file is the host's step after review, outside these bytes: it is neither missing nor done here. A name the request gives this workflow is judged against the candidate's own `nika:` name. A workflow identity is not proof of a Save filename or path; do not infer a destination absent from the state. Every other clause is judged on what the bytes do, including every file the program itself writes (another `.nika` file among them).";

/// The state every question and every repair carries: the request as compiled and as first
/// stated, its answers, the observed world and the candidate's bytes. The first statement is
/// the one the binding holds ([`Binding::of`]): the preserved original request, else the
/// submitted text the compiled request was folded or clarified from. A revision in words is
/// judged on the request it resolves (`intent`); the request of the base it revises, which the
/// change partly supersedes, is never shown as its first statement: it stays apart as history
/// (`revision.base_request`) beside the change as the human stated it (`revision.change`).
fn state(intent: &str, request: &CompileRequest, candidate: &str) -> Value {
    let (submitted, change) = match &request.input {
        Input::Create(text) if text != intent => (Some(text.clone()), None),
        Input::Edit {
            change: EditChange::Text(words),
            ..
        } => (None, Some(words)),
        _ => (None, None),
    };
    let first = request.original_intent.clone().or(submitted);
    // A revision judged on the earlier request with the change appended (`… Change: …`) is
    // told so: its replaced clauses are still in the words, superseded by the change.
    let appended = request.original_intent.is_some()
        && nika_compile::revise_intent(request).is_some_and(|resolved| resolved == intent);
    let (first, revision) = match change {
        Some(change) => {
            let mut revision = json!({"change": change, "base_request": first});
            if appended {
                revision["appended"] = json!(true);
            }
            (None, revision)
        }
        None => (first, Value::Null),
    };
    let mut state = json!({
        "request": intent,
        "original_request": first,
        "answers": request.answers,
        "observed": request.knowledge,
        "candidate_nika": candidate,
    });
    if !revision.is_null() {
        state["revision"] = revision;
    }
    state
}

const REPAIR_REFERENCE: &str = "The compiler emits the workflow from your plan as the reference below states: how it writes and what each tool it calls does, so you can read the candidate's bytes in the STATE. Your answer stays the complete JSON plan.";

/// The decision key under which the spelling law keeps its notes on the programs it could not
/// judge (`transform::spelling`, B21 T1); the notes on the plan's programs ride the state of every
/// question ([`unjudged`]).
pub(super) const UNJUDGED_SPELLINGS: &str = "unjudged_spellings";

/// The spelling law's notes on the programs of `plan` it could not judge (R4 A11, B21 T1): those
/// of the decision's [`UNJUDGED_SPELLINGS`] whose program is one of the plan's rules, so a note on
/// a program a repair replaced is never shown. `None` when there is none.
fn unjudged(settled: &CompileOutcome, plan: &Plan) -> Option<Value> {
    let notes: Vec<Value> = settled
        .provenance
        .decision
        .as_ref()
        .and_then(|decision| decision[UNJUDGED_SPELLINGS].as_array())
        .into_iter()
        .flatten()
        .filter(|note| plan.rules.iter().any(|rule| note["program"] == rule.jq()))
        .cloned()
        .collect();
    (!notes.is_empty()).then(|| json!(notes))
}

/// A question's instructions after the verdict's reference: the reference first, so every
/// question of a verdict opens with the same bytes, then what this question asks.
fn grounded(reference: &str, instructions: &str) -> String {
    format!("{reference}\n\n{instructions}")
}

// The journal entry of the call just made, when that call was journaled after `before`
// entries, records the references its messages carried (R4 A11, E36), never an empty list.
use super::receipt::stamp_references as stamp;

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
    // An answer this judge already gave these bytes, in the attempt this verdict resumes, is
    // read back with no call: it is never asked again (R6).
    if let Some(choice) = read_back(verdict, question) {
        verdict.read_back += 1;
        let record = json!({"question": question.id, "options": question.keys(),
            "choice": choice, "read_back": true, "role": role});
        verdict.records.push(record);
        return Some(choice);
    }
    verdict.attempted += 1;
    let (returned, answer) = match judge {
        Judge::Seat(seat) => {
            let at = (verdict.prefetched.iter()).position(|(id, _)| *id == question.id);
            let answer = match at {
                Some(at) => verdict.prefetched.remove(at).1,
                None => seat.choose(question).await,
            };
            (answer.is_ok(), answer)
        }
        Judge::Provider(policy, provider) => {
            let (messages, schema) = decide::closed_choice(question);
            let before = journal(out).len();
            let response =
                super::call_with_schema(policy, *provider, role, messages, schema, out).await;
            stamp(out, before, &verdict.reference["references"]);
            match response {
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

/// The questions of one step, put to a decision seat together before their turn (A1): one
/// request for a provider seated for decisions, one per question in flight together for a
/// decision service. Each answer is taken in its turn as the verdict asks that question; one
/// read back from an earlier attempt is not asked again (R6). A provider judge asks each
/// question in its turn.
async fn prefetch<P: ProviderInferDyn>(
    judge: &Judge<'_, P>,
    step: &str,
    questions: &[ChoiceQuestion],
    verdict: &mut Verdict,
) {
    let Judge::Seat(seat) = judge else {
        return;
    };
    let open: Vec<ChoiceQuestion> = (questions.iter())
        .filter(|question| read_back(verdict, question).is_none())
        .cloned()
        .collect();
    if open.len() > 1 {
        let answers = seat
            .choose_each(&decide::ChoiceBatch::of(step, &open))
            .await;
        let ids = open.into_iter().map(|question| question.id);
        verdict.prefetched.extend(ids.zip(answers));
    }
}

/// The answers a seat gave questions the verdict never reached (it stopped first): sent, so each
/// is recorded and counted, never read.
fn unread(verdict: &mut Verdict) {
    for (id, answer) in std::mem::take(&mut verdict.prefetched) {
        verdict.attempted += 1;
        verdict.returned += u32::from(answer.is_ok());
        let record = json!({"question": id, "role": "unread", "answered": answer.is_ok()});
        verdict.records.push(record);
    }
}

/// The admitted answer the attempt a verdict resumes gave the same question over the same
/// observation (none for a question over the bytes alone), when it gave one.
fn read_back(verdict: &Verdict, question: &ChoiceQuestion) -> Option<String> {
    let observed = (question.state.get("observation"))
        .map(|observation| knowledge::sha256(&observation.to_string()));
    let earlier = (verdict.earlier.iter()).find(|record| {
        record["question"] == question.id.as_str()
            && record["observation"]["sha256"].as_str() == observed.as_deref()
    })?;
    let choice = earlier["choice"].as_str()?;
    (question.keys().iter())
        .any(|key| key == choice)
        .then(|| choice.to_owned())
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
    // The provider's physical requests are its journal entries (a cut answer widened once is
    // two); a seat's are its questions.
    let sent = |entry: &&Value| entry["result"]["failure_kind"] != "admission_refused";
    let calls = match judge {
        Judge::Provider(..) => u32::try_from(calls.iter().filter(sent).count()).unwrap_or(u32::MAX),
        Judge::Seat(_) => verdict.attempted,
    };
    json!({
        "calls": calls,
        "input_tokens": sum("input_tokens"),
        "output_tokens": sum("output_tokens"),
        "complete": complete,
    })
}

/// Judge every pending clause of the candidate the core named, the whole request among them,
/// under the binding of the plan the core assembled (its stated rules promoted). A provider's
/// calls are journaled into `out`. Each question's state also carries the spelling law's notes on
/// the plan's programs it could not judge ([`unjudged`], B21 T1).
async fn verdict_on<P: ProviderInferDyn>(
    (intent, request, plan): (&str, &CompileRequest, &Plan),
    settled: &CompileOutcome,
    judge: &Judge<'_, P>,
    observation: Option<&Value>,
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
    // One judgment of a clause at its statements settles every pending duty it holds (the core
    // admits it by clause and span), so a clause several duties hold is asked once (B21 T2), and
    // no-operation is offered only when none of them is claimed.
    let mut open: Vec<Open> = Vec::new();
    let pending = settled
        .provenance
        .decision
        .as_ref()
        .and_then(|d| d["pending"]["open"].as_array().cloned())
        .unwrap_or_default();
    for item in pending.iter().filter_map(Open::read) {
        match open
            .iter_mut()
            .find(|o| o.clause == item.clause && o.spans == item.spans)
        {
            Some(same) => same.unclaimed &= item.unclaimed,
            None => open.push(item),
        }
    }
    let sha = knowledge::sha256(candidate);
    // An observation binds only to the bytes it ran: the judge never sees another's.
    let run = observation.filter(|o| o["candidate_sha256"] == sha.as_str());
    if let Some(earlier) = judged_before(out, (request, intent), &sha, judge) {
        if !unfinished(&earlier, run) {
            route(out, if earlier.carried { CARRIED } else { SAME_BYTES });
            return earlier;
        }
        // Its answers stand and are read back with no call; what it never got is asked.
        route(out, RESUMED);
        verdict.earlier = earlier.earlier;
    }
    verdict.candidate_sha256 = Some(sha);
    verdict.context_sha256 = Some(context(intent, request));
    let mut base = state(intent, request, candidate);
    if let Some(notes) = unjudged(settled, plan) {
        base[UNJUDGED_SPELLINGS] = notes;
    }
    let grounding = grounding(Some(candidate));
    verdict.reference = grounding.record;
    for (k, open) in open.iter().enumerate() {
        // A call that got no answer stops the verdict: every clause after it stays unknown.
        if open.spans.is_empty() || verdict.stopped {
            verdict.unknown.push(open.clause.clone());
        } else if open.spans == [(0, intent.len())] {
            let asked = (&base, grounding.text.as_str());
            whole(intent, asked, judge, &binding, run, &mut verdict, out).await;
        } else {
            let asked = (k, &base, &binding, grounding.text.as_str());
            judge_clause(open, asked, judge, &mut verdict, out).await;
        }
    }
    tidy(&mut verdict);
    let calls = &journal(out)[journaled.min(journal(out).len())..];
    verdict.usage = usage(judge, &verdict, calls);
    verdict
}

/// The route a verdict repeated from an earlier attempt on the same bytes takes, and the one a
/// rejection carried from an earlier round takes.
const SAME_BYTES: &str = "verify: same bytes, earlier verdict stands";
const CARRIED: &str = "verify: same bytes, rejected in an earlier round";
/// The route of a repeated verdict whose localization had not finished: its whole-request answer
/// stands, and the questions it never got are asked.
const RESUMED: &str = "verify: same bytes, localization resumed";

/// The verdict an earlier attempt of this compile gave these very bytes under this judge, when
/// that judge answered them and did not accept them, or the rejection the host carries from an
/// earlier round of the conversation (`request.declined`): they are never asked of it again
/// (R6), and the attempt repeats that verdict with no call. A judge that answered nothing is
/// asked again; an abstention carried from another round is not repeated, so a new round may
/// still decide what that round left held; a carried rejection binds to the request it judged
/// (`intent`), so a corrected request asks again.
fn judged_before<P: ProviderInferDyn>(
    out: &CompileOutcome,
    (request, intent): (&CompileRequest, &str),
    sha: &str,
    judge: &Judge<'_, P>,
) -> Option<Verdict> {
    let same = |attempt: &Value| {
        attempt["candidate_sha256"] == sha
            && attempt["judge"]["seat"] == judge.name()
            && attempt["judge"]["kind"] == judge.kind()
            && attempt["declined"] == Value::Bool(true)
            && attempt["settled"] == Value::Bool(false)
    };
    let attempts = (out.provenance.decision.as_ref())
        .and_then(|decision| decision["semantic_verification"].as_array())
        .map_or(&[][..], Vec::as_slice);
    // The latest attempt of this compile that actually judged them: every repeat names it.
    let judged = (attempts.iter().enumerate().rev())
        .find(|(_, attempt)| same(attempt) && attempt["same_bytes_as"].is_null());
    if let Some((index, attempt)) = judged {
        let index = u64::try_from(index).unwrap_or(u64::MAX);
        return Some(Verdict::recorded(attempt, index));
    }
    // The rejection binds to the context it was judged in: a corrected request, other answers
    // or another observed world ask again.
    let context = context(intent, request);
    let carried = (request.declined.iter().rev()).find(|attempt| {
        same(attempt)
            && attempt["rejected"] == Value::Bool(true)
            && attempt["request"] == intent
            && attempt["context_sha256"] == context.as_str()
    })?;
    let mut verdict = Verdict::recorded(carried, 0);
    verdict.same_bytes_as = None;
    verdict.carried = true;
    Some(verdict)
}

/// The digest of the context a judge reads beside a candidate's bytes: the request as compiled
/// and as first stated, the answers and the observed world.
fn context(intent: &str, request: &CompileRequest) -> String {
    let read = json!({
        "request": intent,
        "original": request.original_intent,
        "answers": request.answers,
        "observed": request.knowledge,
    });
    knowledge::sha256(&read.to_string())
}

/// Whether a repeated verdict left its localization unfinished: a call got no answer, or it
/// waited for a whole trial run that this call now has. Its whole-request answer stands (R6);
/// what it never got is asked.
fn unfinished(earlier: &Verdict, observation: Option<&Value>) -> bool {
    let waited = (earlier.unsettled.iter()).any(|why| faithful::waited_for_a_run(why))
        && observation.is_some_and(faithful::trial_whole);
    earlier.stopped || waited
}

/// Each finding once, at its first place: a clause and the same part asked again name one
/// finding, and one note, never two.
fn tidy(verdict: &mut Verdict) {
    for list in [
        &mut verdict.defects,
        &mut verdict.unknown,
        &mut verdict.contested,
        &mut verdict.unsettled,
    ] {
        let mut seen = std::collections::BTreeSet::new();
        list.retain(|entry| seen.insert(entry.clone()));
    }
    let mut seen = std::collections::BTreeSet::new();
    verdict
        .notes
        .retain(|(defect, _)| seen.insert(defect.clone()));
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
/// not settled. `asked` is the clause's index, the base state, the candidate's binding and the
/// reference each question's instructions carry.
async fn judge_clause<P: ProviderInferDyn>(
    open: &Open,
    (k, base, binding, reference): (usize, &Value, &Binding, &str),
    judge: &Judge<'_, P>,
    verdict: &mut Verdict,
    out: &mut CompileOutcome,
) {
    let clause = &open.clause;
    let restricting = restricts(clause);
    let tasks = faithful::task_ids(base["candidate_nika"].as_str().unwrap_or_default());
    let instructions = if restricting {
        format!("{CLAUSE} {}", faithful::RESTRICTING)
    } else {
        CLAUSE.to_owned()
    };
    let told = faithful::told(base, reference, &instructions);
    for (n, &span) in open.spans.iter().enumerate() {
        let mut options = vec![
            ChoiceOption::new(
                "carried",
                "the candidate does exactly what this clause asks",
            ),
            ChoiceOption::new("missing", "the candidate omits it or does it differently"),
        ];
        // The core never admits « no operation » on a clause that restricts (R4 A11), as the
        // core reads a restriction (a « don't forget to … » among them).
        if open.unclaimed && !restricting && !nika_compile_reader::structure::restricts(clause) {
            options.push(ChoiceOption::new("no_operation", NO_OPERATION));
        }
        let mut asked = base.clone();
        asked["clause"] = json!({"text": clause, "span": [span.0, span.1]});
        let id = match n {
            0 => format!("verify-clause-{k}"),
            n => format!("verify-clause-{k}.{n}"),
        };
        let question = ChoiceQuestion::new(&id, &told, asked, options);
        let returned = verdict.answers();
        let answer = ask(judge, &question, "judge_clause", verdict, out).await;
        // A call that got no answer stops the verdict: nothing more is asked of this judge.
        if verdict.answers() == returned {
            verdict.stopped = true;
            verdict.unknown.push(clause.clone());
            return;
        }
        let disposition = match answer.as_deref() {
            Some("carried") => Some(Disposition::Carried),
            Some("no_operation") => Some(Disposition::NoOperation),
            Some("missing") => {
                verdict.consumed += 1;
                verdict.decline(Declined::Rejected);
                let point = format!("{id}-point");
                let asked = (base, reference);
                match faithful::point(&point, clause, &tasks, asked, judge, verdict, out).await {
                    Some(Pointed::Task(task)) => {
                        verdict.defects.push(clause.clone());
                        let note = faithful::pointed_to(&task);
                        verdict.notes.push((clause.clone(), note));
                    }
                    Some(Pointed::Omitted) => {
                        verdict.defects.push(clause.clone());
                        let note = faithful::OMITTED.to_owned();
                        verdict.notes.push((clause.clone(), note));
                    }
                    Some(Pointed::NoTask) => verdict.contested.push(clause.clone()),
                    Some(Pointed::Unsettled) => verdict.unknown.push(clause.clone()),
                    None => {
                        verdict.unknown.push(clause.clone());
                        return;
                    }
                }
                None
            }
            // NONE: an abstention declines these bytes and rejects nothing.
            Some(_) => {
                verdict.decline(Declined::Abstained);
                verdict.unknown.push(clause.clone());
                None
            }
            None => {
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

/// The verifier's record of one attempt in the decision provenance: the judge, its calls
/// attempted, returned and consumed, their usage, the reference they carried, each question,
/// the defects.
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
        "reference": verdict.reference,
        "questions": verdict.records,
        "defects": verdict.defects,
        "unknown": verdict.unknown,
        "doubt": verdict.doubt,
        "contested": verdict.contested,
        "unsettled": verdict.unsettled,
        "notes": (verdict.notes.iter())
            .map(|(defect, note)| json!({"defect": defect, "note": note}))
            .collect::<Vec<_>>(),
        "settled_by": verdict.settled_by,
        "candidate_sha256": verdict.candidate_sha256,
        "declined": verdict.declined != Declined::No,
        "rejected": verdict.rejected(),
        "settled": verdict.settled(),
        "stopped": verdict.stopped,
        "whole_asked": verdict.whole_asked,
        "request": verdict.request,
        "same_bytes_as": verdict.same_bytes_as,
        "carried": verdict.carried,
        "context_sha256": verdict.context_sha256,
        "read_back": verdict.read_back,
    });
    if let Some(attempts) = decision["semantic_verification"].as_array_mut() {
        attempts.push(entry);
    } else {
        decision["semantic_verification"] = json!([entry]);
    }
    out.provenance.decision = Some(decision);
}

/// One more step of the route the decision records.
pub(super) fn route(out: &mut CompileOutcome, step: &str) {
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    if let Some(route) = decision["route"].as_array_mut() {
        route.push(json!(step));
    } else {
        decision["route"] = json!([step]);
    }
    out.provenance.decision = Some(decision);
}

/// The duties the core named but no element of the seat's plan carries (it refused to emit):
/// concrete defects of the plan, the request's own words, each with its kind. The core named
/// them; no judge was asked (B21 T3).
fn silent(out: &CompileOutcome) -> Vec<(String, String)> {
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
        .filter_map(|duty| {
            let kind = duty["kind"].as_str().unwrap_or("duty").to_owned();
            Some((duty["evidence"].as_str()?.to_owned(), kind))
        })
        .collect()
}

/// The repair call: the verifier's concrete defects with the same state the judge read (the
/// request as compiled and as first stated, its answers, the observed world, the candidate's
/// bytes); the proposal it returns is merged as any other, never taken on its word. Its
/// instructions carry the reference the judge read over the same candidate
/// ([`grounding`](fn@grounding)), apart from that state: the seat cannot know by itself how the
/// compiler writes or what each tool it calls does (E36). The call's journal entry records the
/// reference's receipts. A part a judge found missing is said to be compared; a duty the core
/// named is said to be the compiler's, never compared (B21 T3).
#[allow(clippy::too_many_arguments)] // the COLD door's state the repair must carry whole
async fn repair<P: ProviderInferDyn>(
    intent: &str,
    policy: &AuthoringPolicy,
    provider: &P,
    reading: &Reading,
    request: &CompileRequest,
    verdict: &Verdict,
    candidate: Option<&str>,
    pre: &mut CompileOutcome,
) -> Option<Plan> {
    let listed = |parts: &[String]| {
        let lines: Vec<String> = parts.iter().map(|d| format!("- {d}")).collect();
        lines.join("\n")
    };
    let mut told = Vec::new();
    if !verdict.defects.is_empty() {
        told.push(format!(
            "VERIFIER: the workflow compiled from your plan was compared with the WHOLE request. It does not carry these parts of the request, or does them differently:\n{}",
            listed(&verdict.noted_defects())
        ));
    }
    if !verdict.named.is_empty() {
        let named: Vec<String> = verdict
            .named
            .iter()
            .map(|(evidence, kind)| format!("{evidence} ({kind})"))
            .collect();
        told.push(format!(
            "VERIFIER: the compiler named these parts of the request and no element of the workflow compiled from your plan carries them (no judge was asked):\n{}",
            listed(&named)
        ));
    }
    let judged = state(intent, request, candidate.unwrap_or("(none was emitted)"));
    let text = format!(
        "{}\nReturn the complete corrected JSON plan for the whole request: every part carried, with the stated order, counts, negations, numbers and units, targets and conditions, every evidence an exact excerpt of the request.\n\nSTATE (the request as compiled and as first stated, its answers, the observed world, the candidate's bytes):\n{}",
        told.join("\n"),
        serde_json::to_string_pretty(&judged).unwrap_or_default()
    );
    // The opening's messages: its instructions and the context the opening read, then the
    // reference over the candidate. The candidate's grounding and the state stay as they were;
    // the call's journal entry names the receipts of both, the grounding's first.
    let reference = grounding(candidate);
    let context = knowledge::plan_context(intent, reading, request);
    let system = format!(
        "{}\n\n{}\n\n{REPAIR_REFERENCE}\n\n{}",
        super::INSTRUCTIONS,
        context.text,
        reference.text
    );
    let messages = vec![
        Message::text(Role::System, system),
        Message::text(Role::User, intent),
        Message::text(Role::User, text),
    ];
    let before = journal(pre).len();
    let called = super::call(policy, provider, "repair", messages, pre).await;
    let grounded = reference.record["references"].as_array();
    knowledge::stamp_plan(pre, before, &context, grounded.map_or(&[], Vec::as_slice));
    let (proposal, _) = called?;
    let mut scratch = crate::initial();
    // A repair proposal is judged as a plan only; a composition is not repaired here.
    let plan = super::merged(intent, proposal, reading, &mut scratch).into_plan();
    for finding in scratch
        .diagnostics
        .into_iter()
        .filter(|d| d.kind != DiagnosticKind::Applied)
    {
        pre.diagnostics.push(finding);
    }
    plan
}

/// The findings a blocked verification leaves: each duty the core named, as the core's (no judge
/// was asked, B21 T3), each part the judge found missing (repaired from as the policy allowed)
/// and each it could not settle, with the next action. No question asks the human for
/// information the request already gives; a duty the core named keeps the clarification the
/// core asks, its only next action.
fn blocked(out: &mut CompileOutcome, verdict: &Verdict, repairs: usize) {
    for (evidence, kind) in &verdict.named {
        crate::finding(
            out,
            DiagnosticKind::Unknown,
            "semantic_verification",
            format!(
                "The core named `{evidence}` ({kind}) and no element of the compiled workflow carries it; no judge was asked. {repairs} repair(s) from it did not settle it; nothing is READY. Next: the clarification the compiler asks, or a restatement of that part."
            ),
        );
    }
    for defect in &verdict.noted_defects() {
        crate::finding(
            out,
            DiagnosticKind::Unknown,
            "semantic_verification",
            format!(
                "The judge compared the whole request with the candidate's bytes: it does not carry « {defect} ». {repairs} repair(s) from that defect did not settle it; nothing is READY. Next: a stronger authoring model, or a restatement of that part."
            ),
        );
    }
    for unknown in &verdict.unknown {
        let message = format!(
            "The judge could not settle `{unknown}` against the candidate (it abstained, answered outside its options, or its call failed); nothing is READY on it. Next: a judge that answers, or a restatement the deterministic reader reads."
        );
        crate::finding(
            out,
            DiagnosticKind::Unknown,
            "semantic_verification",
            message,
        );
    }
    for contested in &verdict.contested {
        // The whole request is named apart from a part: each contested entry is said once.
        let message = if verdict.request.as_deref() == Some(contested.as_str()) {
            format!(
                "The judge did not accept the request as carried ({}) and located no defect a repair could start from; the same judge asked again decides nothing ({}). Nothing is READY on it. Next: a correction of the request, or another verifier.",
                verdict.doubt.join(", "),
                verdict.unsettled.join("; ")
            )
        } else {
            format!(
                "The judge found « {contested} » missing but then named no task that fails it and no operation it lacks: nothing decided it, and nothing is READY on it."
            )
        };
        crate::finding(
            out,
            DiagnosticKind::Unknown,
            "semantic_verification",
            message,
        );
    }
    if verdict.stopped {
        crate::finding(
            out,
            DiagnosticKind::Unknown,
            "semantic_verification",
            STOPPED,
        );
    }
    if verdict.named.is_empty() {
        out.questions.retain(|q| q.key != "intent.clarification");
    }
}

/// Why a verification left part of its questions unasked.
const STOPPED: &str = "The verification stopped at a judge call that got no answer (refused by the call bound, or failed: the receipt says which); nothing after it was asked of that judge. Next: another round, or a larger call bound.";

/// A COLD candidate is judged before READY (R4 A11): a concrete defect (a part the judge finds
/// missing, a stated duty the plan leaves uncarried) is repaired from within the policy's
/// repairs, and the repaired plan's computations go through the transform seat again with the
/// judge's defects, so no program of the plan it replaced survives; an abstention, a failed
/// judge or exhausted repairs stay INCOMPLETE. Under no repair count, a repaired plan whose
/// judgment names the very parts the last one named made no progress: the repairs end there.
#[allow(clippy::too_many_arguments)] // the COLD door's own state, threaded once
pub(super) async fn judged_cold<P: ProviderInferDyn>(
    intent: &str,
    mut plan: Plan,
    policy: &AuthoringPolicy,
    provider: &P,
    seat: Option<&dyn DecisionSeat>,
    reading: &Reading,
    request: &CompileRequest,
    mut pre: CompileOutcome,
) -> Result<CompileOutcome, CompileError> {
    let judge = seat.map_or(Judge::Provider(policy, provider), Judge::Seat);
    let mut attempt = 0;
    let unbounded = policy.repairs.is_none();
    let mut seen_parts: Vec<Vec<String>> = Vec::new();
    // The clauses each judged candidate's verdict carried, by its digest: a verdict repeated on
    // the same bytes keeps them, so a clause the judge carried is never named pending again.
    let mut carried: Vec<(String, Vec<Judgment>)> = Vec::new();
    loop {
        let settled =
            super::settle_judged(Strategy::Cold, &plan, intent, request, &[], pre.clone())?;
        let omitted = silent(&settled);
        let verdict = if omitted.is_empty() {
            let on = (intent, request, &plan);
            let mut verdict = verdict_on(on, &settled, &judge, None, &mut pre).await;
            if let Some(sha) = verdict.candidate_sha256.clone() {
                match carried.iter().find(|(judged, _)| *judged == sha) {
                    Some((_, judgments)) if verdict.same_bytes_as.is_some() => {
                        verdict.judgments.clone_from(judgments);
                    }
                    _ => carried.push((sha, verdict.judgments.clone())),
                }
            }
            record(&mut pre, &judge, &verdict, attempt);
            verdict
        } else {
            Verdict {
                named: omitted,
                ..Verdict::default()
            }
        };
        if settled.candidate.is_some() && verdict.settled() {
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
        let parts = verdict.parts();
        if settled.candidate.is_none() && parts.is_empty() {
            // A genuine question (a field, a count the request withholds): asked as it was.
            return Ok(settled);
        }
        // The same bytes again (an earlier verdict repeated, no call) are no progress; under no
        // repair count, neither is a defect set that names no part never named before nor
        // narrows the last one (A, B, A ends; so does any reshuffle of parts already repaired
        // from), so the judge's variance never reopens the door without end.
        let mut key = parts.clone();
        key.sort();
        let stalled = verdict.same_bytes_as.is_some()
            || (unbounded && !seen_parts.is_empty() && !progressed(&seen_parts, &key));
        if stalled {
            route(&mut pre, "verify: no progress");
        }
        let allowed = (policy.repairs).is_none_or(|repairs| attempt < repairs as usize);
        if !parts.is_empty() && !stalled && allowed {
            attempt += 1;
            seen_parts.push(key);
            route(&mut pre, &format!("verify: repair {attempt}"));
            let candidate = settled.candidate.as_deref();
            let repaired = repair(
                intent, policy, provider, reading, request, &verdict, candidate, &mut pre,
            );
            if let Some(mut next) = repaired.await {
                let synthesized = super::transform::synthesize(
                    intent, &mut next, policy, provider, request, &parts, &mut pre,
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
        // The clauses the judge carried stay settled; what it did not carry keeps READY closed.
        let judgments = &verdict.judgments;
        let mut out = super::settle_judged(Strategy::Cold, &plan, intent, request, judgments, pre)?;
        blocked(&mut out, &verdict, attempt);
        unreplayed(&mut out, &verdict);
        return Ok(out);
    }
}

/// Whether a defect set is progress over the sets already repaired from: it names a part never
/// named before, or it narrows the last set (fewer parts, all among the last). Each new part
/// grows a finite set and each narrowing shrinks the last, so the repairs end.
fn progressed(seen: &[Vec<String>], key: &[String]) -> bool {
    let new = key
        .iter()
        .any(|part| !seen.iter().flatten().any(|named| named == part));
    let narrowed = seen
        .last()
        .is_some_and(|last| key.len() < last.len() && key.iter().all(|part| last.contains(part)));
    new || narrowed
}

/// A replayed record under this round's judge (R4 A11, Q2): deterministically closed duties
/// replay as they are, with no call; the remainder the core names (`pending.open`) is judged
/// by the round's judge (its seat, else its policy's provider through the journaled call) and
/// the record replays again under those active judgments. No judge, or a remainder the judge
/// finds missing or cannot settle, keeps the outcome INCOMPLETE naming it: an answer round
/// makes no proposal, so nothing is repaired here, and nothing a record carries is read as a
/// judgment. With `whole` (the first candidate of a model's plan) the whole request is part
/// of the remainder. A native record (the native and the sketch doors write one) is no plan: its
/// whole request is always the remainder, bound to the reader's plan of the request as the core
/// binds it (step 2). The route states the verdict after the replay it settles.
pub(super) async fn replayed<P: ProviderInferDyn>(
    intent: &str,
    saved: &Value,
    request: &CompileRequest,
    judges: (Option<&dyn DecisionSeat>, Option<(&AuthoringPolicy, &P)>),
    whole: bool,
    rehearsals: &mut Rehearsals<'_>,
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
    let native = saved.get("strategy").and_then(Value::as_str) == Some(Strategy::Native.word());
    let plan = if native {
        crate::lexicon::read(intent).plan
    } else if let Ok(plan) = Plan::from_json(saved) {
        plan
    } else {
        return Ok(out);
    };
    if out.candidate.is_none() || !open {
        return Ok(out);
    }
    let observation = Box::pin(rehearsals.trial(request, &out)).await;
    let mut pre = before;
    let on = (intent, request, &plan);
    let verdict = verdict_on(on, &out, &judge, observation.as_ref(), &mut pre).await;
    record(&mut pre, &judge, &verdict, 0);
    if verdict.settled() {
        crate::replay_judged(intent, saved, request, &verdict.judgments, whole, &mut pre)?;
        route(&mut pre, &format!("verify: judged ({})", judge.kind()));
        return Ok(pre);
    }
    // The clauses the judge carried stay settled; what it did not carry keeps READY closed.
    crate::replay_judged(intent, saved, request, &verdict.judgments, whole, &mut pre)?;
    route(&mut pre, "verify: not ready");
    blocked(&mut pre, &verdict, 0);
    unreplayed(&mut pre, &verdict);
    Ok(pre)
}

/// A whole request the judge answered and did not carry is never replayed to it: the record
/// that would ask it again on the same bytes is dropped, and the candidate stays as the core
/// left it. A judge that answered nothing keeps the record, so a later round asks it.
fn unreplayed(out: &mut CompileOutcome, verdict: &Verdict) {
    if verdict.doubted() {
        // Bytes the judge did not accept ask nothing more of the human: a later round authors
        // again and asks its own questions.
        out.provenance.plan = None;
        out.requested_boundary = None;
        out.questions.clear();
        route(out, "verify: doubted, not replayable");
        crate::finding(
            out,
            DiagnosticKind::Applied,
            HELD_TARGET,
            held_text(verdict),
        );
    }
}

/// The answer round of a semantic record (slice C): the core compiles the raw request with no
/// call (`compile_judged`); a graph the reach laws (read only here) refuse is named and nothing
/// is judged; a whole request still pending on a candidate is judged by the round's admitted
/// judge, a counted call bound to `intent`/`request` as this round derives them, and the raw
/// request is compiled again with those judgments, which settle only under the core's own
/// binding (a mismatch leaves it INCOMPLETE). The judge's journal joins that outcome; nothing
/// is read from the record as a judgment.
pub(super) async fn semantic<P: ProviderInferDyn>(
    (raw, intent, request): (&CompileRequest, &str, &CompileRequest),
    judges: (Option<&dyn DecisionSeat>, Option<(&AuthoringPolicy, &P)>),
    rehearsals: &mut Rehearsals<'_>,
) -> Result<CompileOutcome, CompileError> {
    let mut out = nika_compile::compile_judged(raw, &[])?;
    let rebuilt = out.provenance.plan.as_ref();
    let refused = rebuilt.map(super::sketch::replay_laws).unwrap_or_default();
    if !refused.is_empty() {
        crate::finding(
            &mut out,
            DiagnosticKind::Unknown,
            "recorded_plan",
            format!(
                "The recorded sketch no longer holds the reach laws ({} refusal(s), not repeated from the record): nothing is judged and nothing is READY; compile the intent again without the record.",
                refused.len()
            ),
        );
        out.candidate = None;
        out.check_preview = None;
        out.status = CompileStatus::Incomplete;
        return Ok(out);
    }
    let open = (out.provenance.decision.as_ref())
        .and_then(|d| d["pending"]["open"].as_array())
        .is_some_and(|open| !open.is_empty());
    let judge = match judges {
        (Some(seat), _) => Judge::Seat(seat),
        (None, Some((policy, provider))) => Judge::Provider(policy, provider),
        (None, None) => return Ok(out),
    };
    if out.candidate.is_none() || !open {
        return Ok(out);
    }
    let observation = Box::pin(rehearsals.trial(raw, &out)).await;
    let mut pre = crate::initial();
    let plan = crate::lexicon::read(intent).plan;
    let on = (intent, request, &plan);
    let verdict = verdict_on(on, &out, &judge, observation.as_ref(), &mut pre).await;
    record(&mut pre, &judge, &verdict, 0);
    let settled = verdict.settled();
    // The core's READY law weighs the judgments made; what the judge did not carry stays open.
    let mut done = nika_compile::compile_judged(raw, &verdict.judgments)?;
    done.provenance.authoring = pre.provenance.authoring.take();
    done.provenance.cognition = pre.provenance.cognition;
    if let (Some(decision), Some(verified)) =
        (done.provenance.decision.as_mut(), pre.provenance.decision)
    {
        decision["semantic_verification"] = verified["semantic_verification"].clone();
    }
    done.diagnostics.extend(pre.diagnostics);
    if settled {
        route(&mut done, &format!("verify: judged ({})", judge.kind()));
    } else {
        route(&mut done, "verify: not ready");
        blocked(&mut done, &verdict, 0);
        unreplayed(&mut done, &verdict);
    }
    Ok(done)
}

/// A candidate the native or the sketch door finishes READY is judged before READY (R4 A11, E39
/// C3): the seat wrote the workflow itself (the sketch door's seat its tasks and program holes),
/// so no law of the core reads its programs, and the parser, Check and the fidelity laws only
/// admit it. The whole request is judged against the candidate's actual final bytes as a COLD
/// candidate's is: the same state and reference, the selected decision seat when present,
/// otherwise the journaled authoring provider under its policy. A selected seat's abstention
/// or failure never falls back to the author. A refused candidate is withdrawn with
/// its questions, its requested boundary and its replayable record, so no answer round replays
/// it, and the request stays INCOMPLETE naming the part; no repair round follows here (the
/// sketch door and a semantic revision repair from [`native_verdict`]'s defects in their own
/// talk). An outcome that is not READY (a question open, a refusal) is returned as it is.
pub(super) async fn judged_native<P: ProviderInferDyn>(
    intent: &str,
    reading: &Reading,
    policy: &AuthoringPolicy,
    (provider, decision): (&P, Option<&dyn DecisionSeat>),
    request: &CompileRequest,
    out: CompileOutcome,
) -> CompileOutcome {
    let seats = (provider, decision);
    match native_verdict(intent, reading, policy, seats, request, out, 0, None).await {
        Ok(out) => out,
        Err(judged) => {
            let (out, verdict) = *judged;
            if verdict.defects.is_empty() && verdict.doubted() {
                held(out, &verdict)
            } else {
                withdrawn(out, &verdict, 0)
            }
        }
    }
}

/// The judgment of [`judged_native`] before its consequence: the outcome as judged, or the
/// outcome and the verdict that leave it not READY (its defects are what a repair starts from).
/// `attempt` is the verification attempt the record names: the repairs that preceded it.
#[allow(clippy::too_many_arguments)] // the native door's verdict state, threaded once
pub(super) async fn native_verdict<P: ProviderInferDyn>(
    intent: &str,
    reading: &Reading,
    policy: &AuthoringPolicy,
    (provider, decision): (&P, Option<&dyn DecisionSeat>),
    request: &CompileRequest,
    mut out: CompileOutcome,
    attempt: usize,
    observation: Option<&Value>,
) -> Result<CompileOutcome, Box<(CompileOutcome, Verdict)>> {
    let ready = out.status == CompileStatus::Ready;
    let Some(candidate) = out.candidate.clone().filter(|_| ready) else {
        return Ok(out);
    };
    let judge = decision.map_or(Judge::Provider(policy, provider), Judge::Seat);
    // Bytes this judge already answered and did not accept are never asked of it again (R6):
    // the attempt repeats that verdict with no call.
    let sha = knowledge::sha256(&candidate);
    // An observation binds only to the bytes it ran: the judge never sees another's.
    let observation = observation.filter(|o| o["candidate_sha256"] == sha.as_str());
    let mut verdict = Verdict::default();
    if let Some(earlier) = judged_before(&out, (request, intent), &sha, &judge) {
        if !unfinished(&earlier, observation) {
            route(&mut out, if earlier.carried { CARRIED } else { SAME_BYTES });
            record(&mut out, &judge, &earlier, attempt);
            return Err(Box::new((out, earlier)));
        }
        // Its answers stand and are read back with no call; what it never got is asked.
        route(&mut out, RESUMED);
        verdict.earlier = earlier.earlier;
    }
    let journaled = journal(&out).len();
    let mut assembled = reading.plan.clone();
    crate::shape::promote_stated_rules(&mut assembled, intent);
    let binding = Binding::of(intent, request, &assembled, &candidate);
    let grounding = grounding(Some(&candidate));
    verdict.reference = grounding.record;
    let base = state(intent, request, &candidate);
    let asked = (&base, grounding.text.as_str());
    verdict.candidate_sha256 = Some(sha);
    verdict.context_sha256 = Some(context(intent, request));
    whole(
        intent,
        asked,
        &judge,
        &binding,
        observation,
        &mut verdict,
        &mut out,
    )
    .await;
    tidy(&mut verdict);
    let calls = &journal(&out)[journaled.min(journal(&out).len())..];
    verdict.usage = usage(&judge, &verdict, calls);
    record(&mut out, &judge, &verdict, attempt);
    if verdict.settled() {
        route(&mut out, &format!("verify: judged ({})", judge.kind()));
        return Ok(out);
    }
    Err(Box::new((out, verdict)))
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
    let verdict = verdict_on((intent, request, plan), &settled, &judge, None, &mut pre).await;
    record(&mut pre, &judge, &verdict, 0);
    if verdict.settled() {
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
    let judgments = &verdict.judgments;
    let mut blocked_out =
        super::settle_judged(Strategy::Warm, plan, intent, request, judgments, pre)?;
    blocked(&mut blocked_out, &verdict, 0);
    unreplayed(&mut blocked_out, &verdict);
    Ok(blocked_out)
}

#[cfg(test)]
mod tests;
