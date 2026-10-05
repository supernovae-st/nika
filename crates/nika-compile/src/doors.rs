// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The deterministic doors of the Compile core (ADR-140): the HOT admission over the reader's
//! own vocabulary, the record replay — an answer round replays the plan or the native
//! candidate its previous round produced, zero calls — and the records every door writes into
//! the provenance (the route, the retrieval, the plan, the obligation ledger, the intent
//! digest). The seats' doors (a model proposes, writes or sketches; the compiler judges) live
//! above, in `nika-compile-cognition`, and read these through [`crate::surface`].

use std::collections::BTreeSet;

use super::{
    CompileError, CompileOutcome, CompileRequest, DiagnosticKind, HotPolicy, QuestionType,
    Strategy,
    gates::backstop,
    lexicon::{self, Reading},
    plan::Plan,
};
use nika_compile_fidelity::{literal::answered::grant_host, sketch::same_caller};
use serde_json::{Value, json};

mod answered_paths;
pub use answered_paths::{AnsweredPaths, native_answered_paths};
/// The source-anchored revision of a base no semantic record binds: one destination in place.
pub(crate) mod import;

/// The strict admission contract, the legacy one, or none.
pub(crate) fn admit_hot(
    intent: &str,
    reading: &Reading,
    hot: HotPolicy,
) -> Result<(), Vec<String>> {
    match hot {
        HotPolicy::Off => Err(vec!["hot policy off".to_owned()]),
        HotPolicy::Legacy => {
            if reading.complete() {
                Ok(())
            } else {
                Err(vec!["reading incomplete".to_owned()])
            }
        }
        HotPolicy::Strict => {
            let mut why = reading.hot_rejections();
            why.extend(super::hot::rejections(intent, reading));
            if why.is_empty() { Ok(()) } else { Err(why) }
        }
    }
}

/// Under the strict contract, a lexical WARM may only settle an otherwise explicit reading.
#[must_use]
pub fn lexical_rest_is_explicit(intent: &str, reading: &Reading) -> bool {
    let mut why = reading.hot_rejections();
    why.extend(super::hot::rejections(intent, reading));
    why.iter().all(|why| why.contains("ambiguous clause"))
}

/// Recall only: what the embedded candidate index returns for the request text and, once a
/// plan exists, for its operation words. Recorded so recall can be measured against labeled
/// cases; nothing here selects a candidate, ranks a verdict or widens authority.
pub fn record_retrieval(out: &mut CompileOutcome, intent: &str, plan: Option<&Plan>) {
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    if decision.get("intent_sha256").is_none() {
        // The key a transport files a recorded plan under; the same fold as the reader.
        decision["intent_sha256"] = json!(intent_sha256(intent));
    }
    if decision.get("retrieval").is_none() {
        decision["retrieval"] = json!({});
    }
    let project = |hits: Vec<super::retrieve::Hit>| -> serde_json::Value {
        json!(
            hits.iter()
                .map(|hit| {
                    json!({
                        "id": hit.id,
                        "kind": if hit.kind == super::retrieve::HitKind::Skeleton { "skeleton" } else { "family" },
                        "score": (hit.score * 1000.0).round() / 1000.0,
                    })
                })
                .collect::<Vec<_>>()
        )
    };
    if decision["retrieval"].get("by_intent").is_none() {
        decision["retrieval"]["by_intent"] = project(super::retrieve::retrieve(intent, 10));
    }
    if let Some(plan) = plan {
        let mut words: Vec<&str> = plan.steps.iter().map(|step| step.op.word()).collect();
        words.extend(plan.effects.iter().map(|effect| effect.verb.word()));
        words.extend(
            plan.obligations
                .iter()
                .map(|obligation| obligation.kind.word()),
        );
        decision["retrieval"]["by_ops"] = project(super::retrieve::retrieve_by_ops(&words, 10));
    }
    out.provenance.decision = Some(decision);
}

pub fn record_route(out: &mut CompileOutcome, route: &[String]) {
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    decision["route"] = json!(route);
    out.provenance.decision = Some(decision);
}

/// The sha256 (lowercase hex) of an intent as the compiler reads it: typographic
/// apostrophes folded, nothing else changed. A transport keys a recorded plan by this
/// value so an answer round can find the plan its previous round produced; the compiler
/// records it in `provenance.decision.intent_sha256` on every general-path outcome.
#[must_use]
pub fn intent_sha256(intent: &str) -> String {
    super::surface::sha256(&lexicon::fold_apostrophes(intent))
}

/// The provenance projection of a settled plan: the plan itself plus the strategy that
/// settled it, so the record replays under the same name.
/// The plan projection with its strategy word and the obligation ledger the plan states
/// (every duty typed with its state), for provenance and for the answer-round replay.
#[must_use]
pub fn plan_record(plan: &Plan, strategy: Option<Strategy>) -> Value {
    let mut record = plan.to_json();
    if let Some(strategy) = strategy {
        record["strategy"] = json!(strategy.word());
    }
    record
}

/// Record the obligation ledger a plan states in the decision record (the assembler
/// overwrites it with the realized one when it emits): the plan record itself stays the
/// replayable identity of the plan, byte-identical across answer rounds.
pub fn record_ledger(out: &mut CompileOutcome, ledger: &super::ledger::Ledger) {
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    decision["ledger"] = ledger.to_json();
    out.provenance.decision = Some(decision);
}

/// Replay a recorded plan for the same intent: straight to the deterministic assembler,
/// with zero reading, zero seat calls and zero provider calls. The record's own `strategy`
/// word is kept as the outcome's strategy; the route says `replayed plan`. A record that
/// does not parse, is not anchored in this intent or still carries unknown work is a
/// finding on `recorded_plan`, never a candidate. A native record's candidate stays pending on
/// its whole request (no law reads the seat's program): its READY takes a judgment made in the
/// round ([`replay_judged`]).
///
/// # Errors
/// Returns representation failures while replaying an admitted record through assembly.
pub fn replay(
    intent: &str,
    record: &Value,
    request: &CompileRequest,
    out: &mut CompileOutcome,
) -> Result<(), CompileError> {
    replay_judged(intent, record, request, &[], false, out)
}

/// The same replay under the judgments a judge made in THIS round (R4 A11): each settles its
/// clause only under the binding the core recomputes from the request, the recorded plan and
/// the bytes it emits, so a judgment of another clause, request or candidate settles nothing.
/// A judged field the record carries is never read. With `whole`, the whole request waits for
/// its own judgment too (the first candidate of a model's plan); a native record's whole
/// request always does (`native_replay`). Every gate of [`replay`] (anchoring, binding,
/// unknown work, unfed plan) runs before, unchanged.
///
/// # Errors
/// Returns representation failures while replaying an admitted record through assembly.
pub fn replay_judged(
    intent: &str,
    record: &Value,
    request: &CompileRequest,
    judgments: &[super::ledger::Judgment],
    whole: bool,
    out: &mut CompileOutcome,
) -> Result<(), CompileError> {
    let folded = lexicon::fold_apostrophes(intent);
    let intent = folded.as_str();
    if seat_record(intent, record, request, judgments, out) {
        return Ok(());
    }
    record_route(out, &["replayed plan".to_owned()]);
    record_retrieval(out, intent, None);
    let plan = match Plan::from_json(record) {
        Ok(plan) => plan,
        Err(why) => {
            super::finding(
                out,
                DiagnosticKind::Unknown,
                "recorded_plan",
                format!(
                    "The recorded plan cannot be replayed ({why}). Compile the intent again without it."
                ),
            );
            return Ok(());
        }
    };
    let strategy = record
        .get("strategy")
        .and_then(Value::as_str)
        .and_then(Strategy::parse);
    if !plan.anchored(intent) {
        super::finding(
            out,
            DiagnosticKind::Unknown,
            "recorded_plan",
            "The recorded plan is not anchored in this request: an operation, effect or obligation names an excerpt the request does not contain. Compile the intent again without it.",
        );
        return Ok(());
    }
    // Every recorded rule is re-derived from its words by the law that created it.
    let observed = super::observed::for_intent(super::observed::world(request), intent);
    if let Some(why) = super::binding::unbound(&plan, intent, observed) {
        super::finding(
            out,
            DiagnosticKind::Unknown,
            "recorded_plan",
            format!(
                "The recorded plan cannot be replayed: {why}. Compile the intent again without it."
            ),
        );
        return Ok(());
    }
    if !plan.unknowns.is_empty() {
        super::finding(
            out,
            DiagnosticKind::Unknown,
            "recorded_plan",
            "The recorded plan still carries unresolved requested work; no substitute workflow was emitted.",
        );
        for unknown in &plan.unknowns {
            super::finding(out, DiagnosticKind::Unknown, "intent", unknown.clone());
        }
        super::question(
            out,
            "intent.clarification",
            "Supply a complete replacement request including all work still wanted. It explicitly replaces the earlier intent.",
            QuestionType::Text,
        );
        record_ledger(out, &super::ledger::Ledger::extract(&plan));
        out.provenance.plan = Some(plan_record(&plan, strategy));
        return Ok(());
    }
    // A record from an earlier engine may still carry a numeric rule as guidance.
    let mut plan = plan;
    super::shape::promote_stated_rules(&mut plan, intent);
    // Or it may lack the write a destination the request leaves unnamed asks for (« … dans un
    // fichier »): the reader's floor restores it, so an answer round never makes READY a draft
    // that writes nothing. The floor is idempotent on a current record.
    if strategy == Some(Strategy::Hot) {
        super::hot::unnamed_destination_floor(&mut plan);
    }
    // A seat's plan (or a record with no strategy word) that works on nothing is asked,
    // never assembled; the reader's own HOT plan was already judged explicit.
    if strategy != Some(Strategy::Hot) && super::assemble::unfed(&plan, intent, out) {
        out.provenance.plan = Some(plan_record(&plan, strategy));
        return Ok(());
    }
    super::assemble::assemble_judged(&plan, intent, request, judgments, whole, out)?;
    record_retrieval(out, intent, Some(&plan));
    out.provenance.strategy = strategy;
    out.provenance.plan = Some(plan_record(&plan, strategy));
    Ok(())
}

/// The deterministic-only door: HOT under the request's contract, or an honest report.
pub(crate) fn hot(
    intent: &str,
    request: &CompileRequest,
    out: &mut CompileOutcome,
) -> Result<bool, CompileError> {
    let folded = lexicon::fold_apostrophes(intent);
    let intent = folded.as_str();
    let mut reading = lexicon::read(intent);
    backstop(intent, &mut reading.plan);
    // The deterministic door judges the reading with its stated rules promoted: a rule
    // carries its own constraint, and the words inside it are its literals.
    let mut admitted = reading.clone();
    super::shape::promote_stated_rules(&mut admitted.plan, intent);
    match admit_hot(intent, &admitted, request.hot) {
        Ok(()) => {
            record_route(out, &["hot".to_owned()]);
            super::assemble::assemble(&admitted.plan, intent, request, out)?;
            record_retrieval(out, intent, Some(&admitted.plan));
            out.provenance.strategy = Some(Strategy::Hot);
            out.provenance.plan = Some(plan_record(&admitted.plan, Some(Strategy::Hot)));
            Ok(true)
        }
        Err(why) => {
            if reading.plan.steps.is_empty()
                && reading.plan.effects.is_empty()
                && reading.ambiguous.is_empty()
                && reading.unresolved.len() <= 1
                && reading.clauses <= 1
            {
                // Nothing recognizable: keep the historical message of the exact-skeleton door.
                return Ok(false);
            }
            record_route(
                out,
                &[
                    format!("hot rejected: {}", why.join("; ")),
                    "needs cognition".to_owned(),
                ],
            );
            record_retrieval(out, intent, None);
            // A contradiction is the human's whatever else the reading could not settle (R4
            // S0): stated with both clauses beside every unresolved one, never left to a model.
            let contradicted = super::assemble::refuse_contradiction(&reading.plan, out);
            unresolved(&reading, out);
            if !contradicted && reading.unresolved.is_empty() && reading.ambiguous.is_empty() {
                super::finding(
                    out,
                    DiagnosticKind::Unknown,
                    "intent",
                    format!(
                        "The deterministic reader cannot admit this request on its own ({}). Permit an authoring model (`--authoring-model`) or a decision seat, or rephrase with explicit operations and literals.",
                        why.join("; ")
                    ),
                );
            }
            Ok(true)
        }
    }
}

pub fn unresolved(reading: &Reading, out: &mut CompileOutcome) {
    for clause in &reading.unresolved {
        super::finding(
            out,
            DiagnosticKind::Unknown,
            "intent",
            format!(
                "Unresolved clause: {clause}. No requested operation was dropped; no substitute workflow was selected."
            ),
        );
    }
    for ambiguity in &reading.ambiguous {
        super::finding(
            out,
            DiagnosticKind::Unknown,
            "intent",
            format!(
                "Ambiguous clause: {} (could be {}). A bounded decision seat or an explicit rephrase settles it; no substitute workflow was selected.",
                ambiguity.clause,
                ambiguity
                    .options
                    .iter()
                    .map(|op| op.word())
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
        );
    }
    for unknown in &reading.plan.unknowns {
        super::finding(out, DiagnosticKind::Unknown, "intent", unknown.clone());
    }
    if !out
        .questions
        .iter()
        .any(|q| q.key == "intent.clarification")
    {
        super::question(
            out,
            "intent.clarification",
            "Supply a complete replacement request including all work still wanted. It explicitly replaces the earlier intent.",
            QuestionType::Text,
        );
    }
    // The reading's own ledger: the plan's duties plus every clause the reader could not
    // settle, so the unresolved work is typed beside the plan.
    record_ledger(out, &super::ledger::Ledger::extract_reading(reading));
    out.provenance.plan = Some(reading.plan.to_json());
}

/// The trigger the request states is recorded beside the candidate, never in it: the same
/// reading the assembler makes (a cadence, a time of day, an event), bound to the schedule
/// answers when the human gives them.
fn record_trigger(record: &Value, request: &CompileRequest, out: &mut CompileOutcome) {
    two_triggers(record, request, out);
    let Some(phrase) = record["trigger"].as_str().filter(|p| !p.trim().is_empty()) else {
        return;
    };
    if matches!(
        crate::trigger::classify(phrase),
        crate::trigger::TriggerForm::Sequence
    ) {
        return;
    }
    let mut plan = crate::plan::Plan::default();
    plan.trigger = Some(phrase.to_owned());
    if let Some(mut trigger) = crate::trigger::requirement(&plan, false) {
        let mut recognized = BTreeSet::new();
        crate::trigger::bind_schedule(&mut trigger, request, out, &mut recognized);
        super::finding(
            out,
            DiagnosticKind::Applied,
            "trigger",
            crate::trigger::note(&trigger),
        );
        out.requested_trigger = Some(trigger);
    }
}

/// Two triggers the request states that one workflow cannot both start on (an event or another
/// head beside a sentence-final cadence, two different cadences): a seat's record keeps one
/// trigger, so the reading of the request the record was authored from names both as unknown
/// work and asks the clarification the assembler's refusal asks — never one of them kept in
/// silence. A record authored from a replacement request (its digest is another one) is left to
/// that request's reading.
fn two_triggers(record: &Value, request: &CompileRequest, out: &mut CompileOutcome) {
    let intent = if let super::Input::Create(intent) = &request.input {
        lexicon::fold_apostrophes(intent)
    } else if let Some(intent) = super::revise_intent(request) {
        intent
    } else {
        return;
    };
    if record["intent_sha256"].as_str() != Some(intent_sha256(&intent).as_str()) {
        return;
    }
    let reading = lexicon::read(&intent);
    let Some(conflict) = reading
        .plan
        .unknowns
        .iter()
        .find(|unknown| unknown.starts_with(lexicon::TWO_TRIGGERS))
    else {
        return;
    };
    super::finding(out, DiagnosticKind::Unknown, "intent", conflict.clone());
    if !out
        .questions
        .iter()
        .any(|q| q.key == "intent.clarification")
    {
        super::question(
            out,
            "intent.clarification",
            "Supply a complete replacement request including all work still wanted. It explicitly replaces the earlier intent.",
            QuestionType::Text,
        );
    }
}

/// A record no plan replay reads: a semantic record (first, whatever else it carries), a pending
/// transform, a native record. `true` when one of them answered the round.
fn seat_record(
    intent: &str,
    record: &Value,
    request: &CompileRequest,
    judgments: &[super::ledger::Judgment],
    out: &mut CompileOutcome,
) -> bool {
    // A revision's bounded question: its answer round decides on the very base it was asked on.
    if record.get("source_question").is_some() {
        import::answered(intent, record, request, judgments, out);
        return true;
    }
    if record.get("semantic_record").is_some() {
        semantic_refusal(
            out,
            "it replays only through a compile of the caller's raw request",
        );
    } else if !super::pending_transform::replay(intent, record, request, out) {
        let native =
            record.get("strategy").and_then(Value::as_str) == Some(Strategy::Native.word());
        if native {
            native_replay(intent, record, request, judgments, out);
        }
        return native;
    }
    true
}

/// Replay a native record: the same candidate with this round's answers, zero calls. The seat
/// wrote its program, so the whole request stays pending on the bytes this round finishes until a
/// judgment made in this compile settles it ([`native_pending`], R4 A11 step 2).
pub(crate) fn native_replay(
    intent: &str,
    record: &Value,
    request: &CompileRequest,
    judgments: &[super::ledger::Judgment],
    out: &mut CompileOutcome,
) {
    record_route(out, &["replayed native candidate".to_owned()]);
    if record["intent_sha256"].as_str() != Some(intent_sha256(intent).as_str()) {
        super::finding(
            out,
            DiagnosticKind::Unknown,
            "recorded_plan",
            "The recorded native candidate was written for another request; compile the intent again without it.",
        );
        return;
    }
    // A source revision replays only on the base it revised, its substitution made again.
    if let Err(why) = import::rebound(record, request) {
        super::finding(
            out,
            DiagnosticKind::Unknown,
            "recorded_plan",
            format!(
                "The recorded revision does not bind this base: {why}. Revise the workflow again without it."
            ),
        );
        return;
    }
    out.provenance.strategy = Some(Strategy::Native);
    out.provenance.plan = Some(record.clone());
    native_apply(record, request, out);
    native_pending(intent, request, judgments, out);
}

/// The whole request a replayed native candidate carries (R4 A11, step 2): no law reads the
/// seat's program, so the request stays pending on the bytes this round finishes, as a model's
/// first candidate does (the whole-request duty). A judgment settles it only when made in this
/// compile, `Carried` over the whole request at its whole span, under the binding recomputed here:
/// the reader's plan of the request with its stated rules promoted (the plan the verifier binds a
/// native candidate to, never a record's) and these very bytes. Otherwise nothing is READY: the
/// candidate stays the preview and the finding names the judge a round can permit. Only a READY
/// finish is held: an outcome the laws already keep from READY (a question open, a check
/// refusal) is returned as it is, its own findings saying why, and no judge is asked of it, as
/// the authoring door judges only a READY conclusion.
fn native_pending(
    intent: &str,
    request: &CompileRequest,
    judgments: &[super::ledger::Judgment],
    out: &mut CompileOutcome,
) {
    let ready = out.status == super::CompileStatus::Ready;
    let Some(candidate) = out.candidate.as_deref().filter(|_| ready) else {
        return;
    };
    let mut stated = lexicon::read(intent).plan;
    super::shape::promote_stated_rules(&mut stated, intent);
    let bound = super::ledger::Binding::of(intent, request, &stated, candidate);
    let whole = (0, intent.len());
    let judged = judgments.iter().any(|j| {
        j.binding == bound
            && j.clause == intent
            && j.span == whole
            && j.disposition == super::ledger::Disposition::Carried
    });
    let open = if judged {
        json!([])
    } else {
        json!([{"clause": intent, "witness": null, "spans": [[whole.0, whole.1]]}])
    };
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    decision["pending"] =
        json!({"candidate_sha256": bound.candidate, "plan_sha256": bound.plan, "open": open});
    out.provenance.decision = Some(decision);
    if judged {
        return;
    }
    super::finding(
        out,
        DiagnosticKind::Unknown,
        "semantic_verification",
        format!(
            "The seat wrote candidate {} itself: no law reads its programs, and no judgment made in this compile settles the whole request against it. Nothing is READY on a native candidate its round has not judged: a bounded judge permitted in this answer round (the authoring model or a decision seat) judges it against the whole request, or it stays INCOMPLETE.",
            &bound.candidate[..12]
        ),
    );
    out.status = super::CompileStatus::Incomplete;
}

/// Bake the answers into the recorded source and finish it: every unanswered question stays
/// mandatory and no candidate is emitted until all are answered; the `model` placeholder takes
/// the human's model.
/// Apply a native record to the request: the answers are baked into the recorded source,
/// the gaps disposed of, the model seated, and the candidate finished — or its questions
/// stay open. The seats' doors call it once a candidate is accepted; the replay calls it on
/// every answer round.
pub fn native_apply(record: &Value, request: &CompileRequest, out: &mut CompileOutcome) {
    apply_native(record, request, out);
}

fn apply_native(
    record: &Value,
    request: &CompileRequest,
    out: &mut CompileOutcome,
) -> AnsweredPaths {
    let mut paths = AnsweredPaths::empty();
    let mut source = record["source"].as_str().unwrap_or_default().to_owned();
    let mut open = false;
    record_trigger(record, request, out);
    for question in record["questions"].as_array().into_iter().flatten() {
        let key = question["key"].as_str().unwrap_or_default();
        if let Some(literal) = request.answers.get(key) {
            if !bake(&mut source, question, literal, out) {
                open = true;
            }
        } else {
            open = true;
            ask(question, out);
        }
    }
    if !open {
        paths = grant_answered_paths(
            record["source"].as_str().unwrap_or_default(),
            &mut source,
            out,
        );
    }
    // An unused envelope model is not a runtime requirement. Ask only when Check
    // proves language work remains, including parametric fan-out calls.
    let language_work = crate::parse(&source)
        .is_ok_and(|wf| !nika_check::check(&wf).certificate.llm_calls.is_zero());
    if language_work
        && source.contains("model: mock/echo")
        && !seat_model(&mut source, request, out)
    {
        open = true;
    }
    dispose_gaps(record, request, out);
    if open {
        // Questions stay; the candidate waits for them (the same contract as the assembler).
        return paths;
    }
    // A stated cap may be the human's and nothing proves otherwise: it is admitted against the
    // catalog's judges, never rewritten. A task that states none gets the compiler's default,
    // sized now that the workflow's model is seated.
    if let Some(filled) = crate::seat_cap::fill_defaults(&source) {
        source = filled;
    }
    crate::seat_cap::admit(&source, out);
    super::finish(source, out);
    paths
}

/// The clauses the seat could not realize never vanish: each is a `Missed` diagnostic on every
/// transport (the clause verbatim, the candidate does not carry it) and an optional question
/// `gap.<n>` the human answers — « drop » or how it should be done — recorded as the human's
/// disposition in the decision record, never baked into the candidate (MP §3.1: nothing
/// important silently disappears into READY).
fn dispose_gaps(record: &Value, request: &CompileRequest, out: &mut CompileOutcome) {
    let gaps: Vec<&str> = record["gaps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if gaps.is_empty() {
        return;
    }
    let mut dispositions = Vec::new();
    for (n, clause) in gaps.iter().enumerate() {
        let key = format!("gap.{}", n + 1);
        if let Some(answer) = request.answers.get(&key) {
            dispositions.push(json!({"clause": clause, "disposition": answer}));
            continue;
        }
        super::finding(
            out,
            DiagnosticKind::Missed,
            "gap",
            format!(
                "the seat could not realize « {clause} »; the candidate does not carry it — answer `{key}` with \"drop\" to accept that, or say in words how it should be done"
            ),
        );
        out.questions.push(super::CompileQuestion {
            key,
            label: format!(
                "« {clause} » is not in the candidate: drop it, or say how it should be done"
            ),
            answer_type: QuestionType::Text,
            why: "A clause the request states never disappears silently; the human disposes of it."
                .to_owned(),
            mandatory: false,
            options: Vec::new(),
        });
    }
    if !dispositions.is_empty() {
        let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
        decision["gap_dispositions"] = json!(dispositions);
        out.provenance.decision = Some(decision);
    }
}

/// Bake one answered question into the source's `const:`; false when it could not be.
fn bake(source: &mut String, question: &Value, literal: &str, out: &mut CompileOutcome) -> bool {
    let key = question["key"].as_str().unwrap_or_default();
    let slug = key.trim_start_matches("const.");
    let Some(value) = super::literal_answer(Some(literal), key, out) else {
        return false;
    };
    // A choice takes one of its offered keys (a column the request left open, among the
    // observed ones); any other answer is a finding and the question stays.
    let offered = offers(question);
    if !offered.is_empty()
        && !offered
            .iter()
            .any(|o| value.as_str() == Some(o.key.as_str()))
    {
        super::finding(
            out,
            DiagnosticKind::Missed,
            key,
            format!(
                "Answer one of the offered keys as a JSON string: {}.",
                offered
                    .iter()
                    .map(|o| o.key.as_str())
                    .collect::<Vec<_>>()
                    .join(" · ")
            ),
        );
        ask(question, out);
        return false;
    }
    let value = if question["answer_type"] == "literal" {
        value
    } else {
        match value {
            Value::String(s) => Value::String(s),
            other => Value::String(other.to_string()),
        }
    };
    let baked = crate::edit::literal_projection(source.as_str()).and_then(|before| {
        let mut after = before.clone();
        after["const"][slug] = value.clone();
        let edited = crate::edit_source::emit(source.as_str(), &before, &after, slug)?;
        // An answered endpoint also grants its host: the boundary is the compiler's to
        // complete from the answer, never the seat's to guess.
        let before = crate::edit::literal_projection(&edited)?;
        if !grant_host(&mut after, slug, &value) {
            return Some(edited);
        }
        // A stated list is rewritten in place; an absent one has no slot, so the document is
        // re-emitted whole under the literal-projection proof (its presentation may normalize).
        if before.pointer("/permits/net/http").is_some() {
            crate::edit_source::emit_at(&edited, &before, &after, &["permits", "net", "http"])
        } else {
            crate::edit::emit_preserving(&edited, &before, &after).ok()?
        }
    });
    if let Some(edited) = baked {
        *source = edited;
        super::finding(
            out,
            DiagnosticKind::Applied,
            key,
            "Answer applied to the candidate's const.",
        );
        true
    } else {
        super::finding(
            out,
            DiagnosticKind::Missed,
            key,
            "The answer could not be baked into the candidate's const; it was not applied.",
        );
        false
    }
}

/// Complete the read or write boundary a seat left as its empty placeholder (`[""]`, the
/// one narrow shape the judge admits while a path is still asked) with the exact paths the
/// answers introduced, as `grant_host` completes an answered endpoint. The paths are the
/// capability inference's own (`nika check --infer-permits`), taken over the seat's source
/// and over the answered one: only their difference, in the direction the tool uses, bound
/// to a bare `${{ const.<slug> }}` (the inference resolves nothing else). A path that
/// escapes the workspace is never inferred; a glob, or a direction the seat declared with
/// any other entry, is never touched — the check then refuses the candidate, as before.
fn grant_answered_paths(
    seat_source: &str,
    source: &mut String,
    out: &mut CompileOutcome,
) -> AnsweredPaths {
    let paths = AnsweredPaths::introduced(seat_source, source);
    for (direction, introduced) in [("read", paths.reads()), ("write", paths.writes())] {
        if introduced.is_empty() {
            continue;
        }
        let Some(before) = crate::edit::literal_projection(source) else {
            break;
        };
        let placeholder = before
            .pointer(&format!("/permits/fs/{direction}"))
            .and_then(Value::as_array)
            .is_some_and(|list| matches!(list.as_slice(), [only] if only.as_str() == Some("")));
        if !placeholder {
            continue;
        }
        let mut after = before.clone();
        after["permits"]["fs"][direction] = json!(introduced);
        if let Some(edited) =
            crate::edit_source::emit_at(source, &before, &after, &["permits", "fs", direction])
        {
            *source = edited;
            super::finding(
                out,
                DiagnosticKind::Applied,
                &format!("permits.fs.{direction}"),
                format!(
                    "The answered path completes the empty placeholder the candidate declared: {}.",
                    introduced.join(" · "),
                ),
            );
        }
    }
    paths
}

/// Ask one recorded business question, mandatory.
fn ask(question: &Value, out: &mut CompileOutcome) {
    let options = offers(question);
    let why = question["why"]
        .as_str()
        .filter(|w| !w.is_empty())
        .unwrap_or("The compiler cannot invent this business value.");
    // A transport that shows no options still shows what a choice takes.
    let why = if options.is_empty() {
        why.to_owned()
    } else {
        let keys: Vec<&str> = options.iter().map(|o| o.key.as_str()).collect();
        format!("{why} Answer one of: {}.", keys.join(" · "))
    };
    out.questions.push(super::CompileQuestion {
        key: question["key"].as_str().unwrap_or_default().to_owned(),
        label: question["label"].as_str().unwrap_or_default().to_owned(),
        answer_type: if question["answer_type"] == "literal" {
            QuestionType::Literal
        } else if options.is_empty() {
            QuestionType::Text
        } else {
            QuestionType::Choice
        },
        why,
        mandatory: true,
        options,
    });
}

/// The closed offers a recorded choice carries (the observed alternatives the judge admitted),
/// or none for any other question.
fn offers(question: &Value) -> Vec<super::types::ChoiceOffer> {
    if question["answer_type"] != "choice" {
        return Vec::new();
    }
    question["options"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| {
            Some(super::types::ChoiceOffer::new(
                o["key"].as_str()?,
                o["label"].as_str()?,
            ))
        })
        .collect()
}

/// The model placeholder: the human's `model` answer replaces it; else the question is asked.
fn seat_model(source: &mut String, request: &CompileRequest, out: &mut CompileOutcome) -> bool {
    let Some(literal) = request.answers.get("model") else {
        super::question(
            out,
            "model",
            "Which model runs the language work of this workflow? Answer a `provider/name` string.",
            QuestionType::Text,
        );
        return false;
    };
    match super::literal_answer(Some(literal), "model", out) {
        Some(Value::String(model)) if model.contains('/') => {
            *source = source.replacen("model: mock/echo", &format!("model: {model}"), 1);
            true
        }
        _ => {
            super::finding(
                out,
                DiagnosticKind::Missed,
                "model",
                "Answer must be a `provider/name` string.",
            );
            false
        }
    }
}

/// The basis a door reads, before any proposal: its effective words and initial answers, the
/// world's identity, every clause occurrence in order (repeats kept), the reader's floor, the
/// ledger and the partial behavior contract (unsupported portion named). No candidate in it.
#[must_use]
pub fn request_basis(intent: &str, request: &CompileRequest) -> Value {
    let mut basis = nika_compile_fidelity::sketch::read_basis(intent, &request.answers);
    basis["world_sha256"] = json!(nika_compile_fidelity::observed::basis::of_request(
        request.knowledge.as_ref(),
        intent
    ));
    basis["ledger"] = super::ledger::Ledger::extract_reading(&lexicon::read(intent)).to_json();
    basis
}

/// The caller's own request, before money is blanked or a clarification taken.
fn caller_basis(request: &CompileRequest) -> Value {
    let words = match &request.input {
        super::Input::Create(text) => Some(text.as_str()),
        super::Input::Edit { .. } => None,
    };
    let money: Vec<_> = request.money.iter().map(|r| [r.start, r.end]).collect();
    json!({"input": words, "original_intent": request.original_intent, "answers": request.answers,
           "money": money, "stated_money": request.stated_money})
}

/// Read at a compile's entry on the raw request: its caller basis; or the refused outcome when it
/// replays a semantic record of another caller (words, original, money, an initial answer, or a
/// clarification the record never had: a replacement is a new basis). No call, nothing emitted.
///
/// # Errors
/// The refused outcome of a semantic record this caller cannot replay.
pub fn caller(request: &CompileRequest) -> Result<Value, Box<CompileOutcome>> {
    let now = caller_basis(request);
    let Some(record) = (request.plan.as_ref()).filter(|r| r.get("semantic_record").is_some())
    else {
        return Ok(now);
    };
    // A revision binds its record by the base the record emitted, never by restated words (F).
    if let super::Input::Edit { source, .. } = &request.input {
        return match base_view(record, source, request) {
            Ok(_) => Ok(now),
            Err(why) => {
                let mut out = super::initial();
                stale_base(&mut out, why);
                Err(Box::new(out))
            }
        };
    }
    let stored = &record["basis"]["caller"];
    let same = same_caller(stored, &now, &request.answers);
    if same {
        return Ok(now);
    }
    let mut out = super::initial();
    semantic_refusal(
        &mut out,
        "its caller's words, money or initial answers are not this request's",
    );
    Err(Box::new(out))
}

/// The answer round of a semantic record, reached only from a compile whose raw caller was
/// read: the request the door read is derived as the door derived it (a clarification taken as
/// the effective words, folded), then [`semantic_replay`]. `false` when the request carries no
/// semantic record.
pub(crate) fn semantic_round(
    request: &CompileRequest,
    judgments: &[super::ledger::Judgment],
    out: &mut CompileOutcome,
) -> bool {
    let (Some(record), super::Input::Create(words)) = (request.plan.as_ref(), &request.input)
    else {
        return false;
    };
    if record.get("semantic_record").is_none() {
        return false;
    }
    let mut assembly = request.clone();
    let clarified = (assembly.answers.remove("intent.clarification"))
        .and_then(|raw| serde_json::from_str::<String>(&raw).ok());
    let intent = lexicon::fold_apostrophes(clarified.as_deref().unwrap_or(words));
    semantic_replay(&intent, record, &assembly, judgments, out);
    true
}

/// A revision of a base its semantic record binds ([`caller`] checked the pair), in the core
/// (R4 F): the zero-call constant door over the base. When it changed a constant the record's
/// final answers hold, the record is bound anew to the edited bytes, kept only if it replays
/// to them exactly; any other result keeps no record. A change in words the door does not
/// settle stays for the semantic revision, the base untouched.
pub(crate) fn semantic_edit(
    request: &CompileRequest,
    out: &mut CompileOutcome,
) -> Result<(), super::CompileError> {
    let (super::Input::Edit { source, change }, Some(record)) =
        (&request.input, request.plan.as_ref())
    else {
        return Ok(());
    };
    let mut plain = request.clone();
    plain.plan = None;
    super::edit(source, change, &plain, out)?;
    let (Some(edited), Some(constant)) = (out.candidate.clone(), super::edit::operation(change))
    else {
        return Ok(());
    };
    let key = format!("const.{}", constant.name);
    let literal =
        (constant.literal_json.map(str::to_owned)).or_else(|| request.answers.get(&key).cloned());
    let mut rebound = record.clone();
    let (Some(literal), true) = (literal, rebound["final"]["answers"].get(&key).is_some()) else {
        return Ok(());
    };
    if edited == *source {
        return Ok(());
    }
    rebound["final"]["answers"][&key] = json!(literal);
    rebound["final"]["candidate_sha256"] = json!(super::surface::sha256(&edited));
    if base_view(&rebound, &edited, request).is_ok() {
        out.provenance.plan = Some(rebound);
    }
    Ok(())
}

/// A revision whose base and semantic record are not a pair: static, the base kept as it is.
fn stale_base(out: &mut CompileOutcome, why: &str) {
    out.status = super::CompileStatus::Refused;
    super::finding(
        out,
        DiagnosticKind::Refused,
        "recorded_plan",
        format!(
            "The semantic record does not bind this base: {why}. The base is kept as it is, nothing was emitted and no model was asked; revise a base with the record it was saved with."
        ),
    );
}

/// A semantic record's refusal: static, never a record value or key name repeated.
fn semantic_refusal(out: &mut CompileOutcome, why: &str) {
    record_route(out, &["replayed semantic record".to_owned()]);
    super::finding(
        out,
        DiagnosticKind::Unknown,
        "recorded_plan",
        format!(
            "The recorded sketch cannot be replayed: {why}. Nothing was emitted, its stored source is never used and no model was asked; compile the intent again without the record."
        ),
    );
}

/// Replay a semantic record: its graph and fills are emitted again under THIS request; its bound
/// answers must still emit its final candidate; a new answer must answer a question that
/// candidate asks, and binds anew; a gap stays an open duty; the stored source is never read and
/// the whole request stays pending ([`native_pending`]). Any mismatch is a refusal.
fn semantic_replay(
    intent: &str,
    record: &Value,
    request: &CompileRequest,
    judgments: &[super::ledger::Judgment],
    out: &mut CompileOutcome,
) {
    out.provenance.strategy = Some(Strategy::Native);
    let (view, bound) = match rebuilt(intent, record, request) {
        Ok(rebuilt) => rebuilt,
        Err(why) => return semantic_refusal(out, why),
    };
    let mut earlier = request.clone();
    earlier.answers.clone_from(&bound);
    let mut asked = super::initial();
    apply_native(&view, &earlier, &mut asked);
    let sha = |out: &CompileOutcome| json!(out.candidate.as_deref().map(super::surface::sha256));
    let fresh = (request.answers.keys()).filter(|key| !bound.contains_key(*key));
    if sha(&asked) != record["final"]["candidate_sha256"] {
        return semantic_refusal(
            out,
            "its answers no longer emit the final candidate it binds",
        );
    }
    if fresh
        .into_iter()
        .any(|k| !asked.questions.iter().any(|q| &q.key == k))
    {
        return semantic_refusal(out, "an answer of this round answers no question it asks");
    }
    // Origin only: judgments a host supplied, never their authenticity or their acceptance.
    let mut route = vec!["replayed semantic record".to_owned()];
    if !judgments.is_empty() {
        route.push("judgments supplied by the host".to_owned());
    }
    record_route(out, &route);
    native_apply(&view, request, out);
    if !view["gaps"].as_array().is_none_or(Vec::is_empty)
        && out.status == super::CompileStatus::Ready
    {
        out.status = super::CompileStatus::Incomplete;
        super::finding(
            out,
            DiagnosticKind::Missed,
            "recorded_plan",
            "A clause the seat could not realize stays an open duty: a `gap` answer records a disposition, never its realization; nothing is READY.",
        );
    }
    let mut kept = record.clone();
    kept["final"] = json!({"answers": request.answers, "candidate_sha256": sha(out)});
    out.provenance.plan = Some(kept);
    native_pending(intent, request, judgments, out);
}

/// The record emitted again under this request, its answers held to A0 ⊆ Ak ⊆ Ac (the initial
/// answers within the bound ones, the bound ones this round's), its basis recomputed from the
/// request with only A0, its graph and fills completed by the sketch laws and lowered to bytes
/// that must be the assembly it names: the view a native settlement reads, with the bound
/// answers; or why the record does not replay.
pub(crate) fn rebuilt(
    intent: &str,
    record: &Value,
    request: &CompileRequest,
) -> Result<(Value, std::collections::BTreeMap<String, String>), &'static str> {
    let read = &record["basis"]["read"];
    let (initial, bound) = nika_compile_fidelity::sketch::bound_answers(record, &request.answers)?;
    let mut basis = request.clone();
    basis.answers.retain(|key, _| initial.contains_key(key));
    if request_basis(intent, &basis) != *read {
        return Err("it was recorded for another request, answers, world or reading");
    }
    let allowed = nika_compile_fidelity::fidelity::allowed_values(&basis.answers);
    let world = request.knowledge.as_ref();
    let mut view =
        nika_compile_fidelity::sketch::replayed_observed(record, intent, &allowed, world)?;
    let source = serde_yaml_bw::to_string(&view["document"])
        .map_err(|_| "its document is not representable")?;
    if record["assembly_sha256"] != json!(super::surface::sha256(&source)) {
        return Err("its graph and fills do not emit the candidate it names");
    }
    view["document"] = json!(source);
    Ok((
        json!({"source": view["document"], "questions": view["questions"], "gaps": view["gaps"],
               "trigger": view["trigger"]}),
        bound,
    ))
}

/// The view a semantic record emits for the EDIT base it is paired with (R4 F): the record
/// replays under its own effective words and final answers over this request's observation, and
/// its emitted bytes must be exactly the base's and the record's final candidate. The words a
/// caller restates are never trusted for the original request: the record's own are. Otherwise
/// why the pairing is stale or tampered. No call, nothing emitted.
pub(crate) fn base_view(
    record: &Value,
    base: &str,
    request: &CompileRequest,
) -> Result<(Value, std::collections::BTreeMap<String, String>), &'static str> {
    let sha = |text: &str| json!(super::surface::sha256(text));
    if record["final"]["candidate_sha256"] != sha(base) {
        return Err("the base is not the candidate its record emitted");
    }
    let closed = "it is not a closed record of this format";
    let intent = record["basis"]["read"]["effective"]
        .as_str()
        .ok_or(closed)?;
    let answers: Option<std::collections::BTreeMap<String, String>> =
        (record["final"]["answers"].as_object().into_iter().flatten())
            .map(|(key, value)| Some((key.clone(), value.as_str()?.to_owned())))
            .collect();
    let mut words = CompileRequest::create(intent);
    words.answers = answers.ok_or(closed)?;
    words.knowledge.clone_from(&request.knowledge);
    let (view, bound) = rebuilt(intent, record, &words)?;
    words.answers.clone_from(&bound);
    let mut emitted = super::initial();
    apply_native(&view, &words, &mut emitted);
    (emitted.candidate.as_deref().map(sha) == Some(sha(base)))
        .then_some((view, bound))
        .ok_or("its graph, fills and answers do not emit this base")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bake applies the answer and grants its host into a stated list (rewritten in place) or
    /// into an absent one (the document re-emitted under the literal-projection proof), block or
    /// flow; an answer no net argument reads grants nothing.
    #[test]
    fn the_bake_grants_an_answered_host_into_a_stated_or_absent_list() {
        let question = json!({"key": "const.endpoint", "label": "Where?", "answer_type": "text"});
        let answer = "\"https://hooks.example.invalid/recap\"";
        let send = |target: &str| {
            format!(
                "tasks:\n  send:\n    invoke:\n      tool: nika:notify\n      args: {{target: \"{target}\", message: hi}}\n"
            )
        };
        let head = "nika: x\nconst:\n  endpoint: ''\n";
        let whole = send("${{ const.endpoint }}");
        let partial = send("${{ const.endpoint }}/x");
        for (permits, tasks, granted) in [
            (
                "permits:\n  tools:\n  - nika:notify\n  net:\n    http: []\n",
                &whole,
                true,
            ),
            (
                "permits: {tools: [\"nika:notify\"], net: {http: []}}\n",
                &whole,
                true,
            ),
            ("permits:\n  tools:\n  - nika:notify\n", &whole, true),
            ("permits: {tools: [\"nika:notify\"]}\n", &whole, true),
            ("permits:\n  tools:\n  - nika:notify\n", &partial, false),
        ] {
            let mut source = format!("{head}{permits}{tasks}");
            let mut out = crate::surface::initial();
            assert!(bake(&mut source, &question, answer, &mut out), "{out:#?}");
            let doc: Value = serde_yaml_bw::from_str(&source).unwrap();
            assert_eq!(
                doc["const"]["endpoint"],
                "https://hooks.example.invalid/recap"
            );
            assert_eq!(doc["tasks"]["send"]["invoke"]["args"]["message"], "hi");
            let expected = if granted {
                json!(["hooks.example.invalid"])
            } else {
                Value::Null
            };
            assert!(
                doc["permits"]["net"]["http"] == expected,
                "the answered host must match the expected HTTP grant"
            );
            assert!(
                doc["permits"]["tools"] == json!(["nika:notify"]),
                "answering the host must preserve the tool grant"
            );
        }
    }
}
