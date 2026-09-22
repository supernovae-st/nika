// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One compiler, three internal resolutions, and a probabilistic frontend before a
//! deterministic backend.
//!
//! HOT: the deterministic reader consumed every clause AND the strict admission
//! contract holds (explicit objects, no coordinated residue, nothing unknown); zero
//! seat calls. WARM: a finite set of admissible readings or proposals, settled by an
//! explicit bounded decision seat that may answer NONE. COLD: one or several
//! explicitly authorized generative proposals of a private semantic plan, never
//! source or permits, each accountable for every region of the request.
//! Deterministic policy facts (prohibitions, gates, indecision, contradictions,
//! bounds, approval bypasses) are the floor under every proposal, and every
//! strategy ends in the same deterministic assembler and the same Check.
//! A permitted seat is not an obligation; a consumed clause is not understanding.
use super::{
    AuthoringCognition, AuthoringPolicy, AuthoringReceipt, CompileError, CompileOutcome,
    CompileRequest, DiagnosticKind, HotPolicy, QuestionType, Strategy,
    compose::{self, Candidate},
    decide::{ChoiceOption, ChoiceQuestion, DecisionSeat, NONE_OPTION},
    lexicon::{self, Reading},
    plan::{EffectVerb, Op, Plan, Step},
    types::Input,
};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, Message, ProviderInferDyn, ResponseFormat, Role,
};
use serde_json::{Value, json};

mod instructions;
use instructions::INSTRUCTIONS;
mod anchor;
mod backstops;
pub(super) use backstops::starts_with_prohibition;
mod proposal;
mod transform;
use proposal::{Proposal, decode, merge};
pub(super) use proposal::{ProposedRegion, exact_excerpt, nullable_string, nullable_vec};

/// The explicit cognition a caller permits for one request. Absent seats are not consent.
#[derive(Clone, Copy)]
pub struct Cognition<'a, P: ProviderInferDyn = NoProvider> {
    /// One or several bounded generative calls for COLD, under the request's authoring policy.
    pub provider: Option<&'a P>,
    /// Bounded closed choices for WARM, before or after COLD.
    pub seat: Option<&'a dyn DecisionSeat>,
}

impl<P: ProviderInferDyn> Default for Cognition<'_, P> {
    fn default() -> Self {
        Self {
            provider: None,
            seat: None,
        }
    }
}

/// The absent generative seat: a caller that only permits decisions names this type.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoProvider;

impl ProviderInferDyn for NoProvider {
    async fn infer(
        &self,
        _: InferRequest,
    ) -> Result<InferResponse, nika_kernel::ai::provider::ProviderError> {
        Err(nika_kernel::ai::provider::ProviderError::Other {
            reason: "no generative seat was permitted for this request".to_owned(),
        })
    }
}

/// Compile with one explicitly authorized generative provider (COLD only).
///
/// # Errors
/// Returns the same representation/registry machinery failures as [`super::compile`].
pub async fn compile_with_provider<P: ProviderInferDyn>(
    request: &CompileRequest,
    provider: &P,
) -> Result<CompileOutcome, CompileError> {
    compile_with_cognition(
        request,
        Cognition {
            provider: Some(provider),
            seat: None,
        },
    )
    .await
}

/// Compile with explicit cognition: a decision seat (WARM) and/or a generative provider (COLD).
/// Exact skeletons, EDIT, bounded support clauses and strictly explicit intents keep the
/// deterministic path and never call either seat.
///
/// # Errors
/// Returns the same representation/registry machinery failures as [`super::compile`].
#[allow(clippy::too_many_lines)] // the resolution ladder reads top to bottom
pub async fn compile_with_cognition<P: ProviderInferDyn>(
    request: &CompileRequest,
    cognition: Cognition<'_, P>,
) -> Result<CompileOutcome, CompileError> {
    let Input::Create(intent) = &request.input else {
        return super::compile(request);
    };
    if matches!(intent.trim(), "hello" | "01-hello")
        || nika_pack::template_names()
            .iter()
            .any(|name| name == intent.trim())
    {
        return super::compile(request);
    }
    if request.authoring.is_none() && cognition.seat.is_none() {
        return super::compile(request);
    }
    let mut out = super::initial();
    let mut assembly_request = request.clone();
    let clarification = if let Some(raw) = request.answers.get("intent.clarification") {
        match super::literal_answer(Some(raw), "intent.clarification", &mut out) {
            Some(serde_json::Value::String(text)) if !text.trim().is_empty() => Some(text),
            _ => {
                super::question(
                    &mut out,
                    "intent.clarification",
                    "Supply a complete replacement request as a nonempty JSON string, including every operation still requested. This explicitly replaces the earlier intent.",
                    QuestionType::Text,
                );
                return Ok(out);
            }
        }
    } else {
        None
    };
    assembly_request.answers.remove("intent.clarification");
    // The question explicitly asks for a complete replacement, never an implicit edit.
    // Apostrophes fold once here so reading, anchoring and the proposal see one text.
    let effective_intent =
        lexicon::fold_apostrophes(&clarification.unwrap_or_else(|| intent.clone()));
    // An answer round replays the plan its previous round produced: no reading, no seat,
    // no proposal, the same candidate.
    if let Some(record) = &request.plan {
        replay(&effective_intent, record, &assembly_request, &mut out)?;
        return Ok(out);
    }
    // The exact grammar keeps its zero-call, fail-closed path when a provider is permitted.
    if let Ok(Some(plan)) = super::support::resolve(&effective_intent) {
        super::support::assemble(&plan, &assembly_request, &mut out)?;
        out.provenance.strategy = Some(Strategy::Support);
        return Ok(out);
    }
    let mut route: Vec<String> = Vec::new();
    record_retrieval(&mut out, &effective_intent, None);
    let mut reading = lexicon::read(&effective_intent);
    backstop(&effective_intent, &mut reading.plan);
    // The deterministic door judges the reading with its stated rules promoted: a rule
    // carries its own constraint, and the words inside it are its literals. The reading
    // itself keeps its constraints: they are the policy floor a seat's proposal inherits.
    let mut admitted = reading.clone();
    super::shape::promote_stated_rules(&mut admitted.plan, &effective_intent);
    match admit_hot(&effective_intent, &admitted, request.hot) {
        Ok(()) => {
            route.push("hot".to_owned());
            record_route(&mut out, &route);
            return settle(
                Strategy::Hot,
                &admitted.plan,
                &effective_intent,
                &assembly_request,
                out,
            );
        }
        Err(why) => route.push(format!("hot rejected: {}", why.join("; "))),
    }
    // WARM on lexical ambiguity: every clause is known; a few carry a finite set of readings
    // and the rest of the reading is strictly explicit.
    if reading.unresolved.is_empty()
        && !reading.ambiguous.is_empty()
        && request.hot != HotPolicy::Off
        && let Some(seat) = cognition.seat
        && lexical_rest_is_explicit(&effective_intent, &admitted)
    {
        out.provenance.cognition = AuthoringCognition::ExplicitDecision;
        let mut records = Vec::new();
        let mut settled_all = true;
        for (index, ambiguity) in reading.ambiguous.iter().enumerate() {
            let options = ambiguity
                .options
                .iter()
                .map(|op| ChoiceOption::new(op.word(), op.definition()))
                .collect();
            let question = ChoiceQuestion::new(
                format!("clause-{index}"),
                "Which operation does this clause of the request ask for? Judge the clause in the context of the whole request; an option you cannot support from the text is not a fit.",
                json!({"request": effective_intent, "clause": ambiguity.clause, "object": ambiguity.detail}),
                options,
            );
            let answer = seat.choose(&question).await;
            let admitted = match &answer {
                Ok(answer) => {
                    super::decide::admit(&question, answer).map(|()| answer.choice.clone())
                }
                Err(error) => Err(error.clone()),
            };
            records.push(match (&answer, &admitted) {
                (Ok(answer), Ok(_)) => super::decide::record(&question, Ok(answer)),
                (_, Err(error)) | (Err(error), _) => super::decide::record(&question, Err(error)),
            });
            match admitted {
                Ok(choice) if choice != NONE_OPTION => {
                    if let Some(op) = Op::parse(&choice) {
                        reading.plan.push_step(Step::new(
                            op,
                            ambiguity.clause.clone(),
                            ambiguity.detail.clone(),
                            Vec::new(),
                        ));
                    }
                }
                Ok(_) => {
                    settled_all = false;
                    reading.unresolved.push(ambiguity.clause.clone());
                }
                Err(error) => {
                    settled_all = false;
                    reading.unresolved.push(ambiguity.clause.clone());
                    super::finding(&mut out, DiagnosticKind::Unknown, "decision_seat", error.0);
                }
            }
        }
        out.provenance.decision = Some(json!({"seat": seat.name(), "questions": records}));
        if settled_all {
            route.push("warm".to_owned());
            record_route(&mut out, &route);
            return settle(
                Strategy::Warm,
                &reading.plan,
                &effective_intent,
                &assembly_request,
                out,
            );
        }
        route.push("warm: none".to_owned());
        reading.ambiguous.clear();
    }
    // COLD: explicitly authorized generative proposals, constrained by the deterministic facts.
    if let (Some(policy), Some(provider)) = (&request.authoring, cognition.provider) {
        if policy.model.trim().is_empty()
            || !(1..=8192).contains(&policy.max_tokens)
            || policy.timeout.is_zero()
            || policy.timeout > std::time::Duration::from_secs(120)
            || effective_intent.len() > 32_768
        {
            super::finding(
                &mut out,
                DiagnosticKind::Missed,
                "authoring_policy",
                "Authoring requires an explicit model, 1..8192 output tokens, a timeout up to 120 seconds, and an intent no larger than 32768 bytes.",
            );
            return Ok(out);
        }
        route.push(format!("cold: {} sample(s)", policy.samples.clamp(1, 5)));
        return sampled(
            &effective_intent,
            policy,
            provider,
            cognition.seat,
            &reading,
            &assembly_request,
            route,
            out,
        )
        .await;
    }
    route.push("needs cognition".to_owned());
    record_route(&mut out, &route);
    unresolved(&reading, &mut out);
    Ok(out)
}

/// The strict admission contract, the legacy one, or none.
fn admit_hot(intent: &str, reading: &Reading, hot: HotPolicy) -> Result<(), Vec<String>> {
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
fn lexical_rest_is_explicit(intent: &str, reading: &Reading) -> bool {
    let mut why = reading.hot_rejections();
    why.extend(super::hot::rejections(intent, reading));
    why.iter().all(|why| why.contains("ambiguous clause"))
}

/// Recall only: what the embedded candidate index returns for the request text and, once a
/// plan exists, for its operation words. Recorded so recall can be measured against labeled
/// cases; nothing here selects a candidate, ranks a verdict or widens authority.
fn record_retrieval(out: &mut CompileOutcome, intent: &str, plan: Option<&Plan>) {
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

fn record_route(out: &mut CompileOutcome, route: &[String]) {
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
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(lexicon::fold_apostrophes(intent).as_bytes());
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// The provenance projection of a settled plan: the plan itself plus the strategy that
/// settled it, so the record replays under the same name.
/// The plan projection with its strategy word and the obligation ledger the plan states
/// (every duty typed with its state), for provenance and for the answer-round replay.
fn plan_record(plan: &Plan, strategy: Option<Strategy>) -> Value {
    let mut record = plan.to_json();
    if let Some(strategy) = strategy {
        record["strategy"] = json!(strategy.word());
    }
    record
}

/// Record the obligation ledger a plan states in the decision record (the assembler
/// overwrites it with the realized one when it emits): the plan record itself stays the
/// replayable identity of the plan, byte-identical across answer rounds.
fn record_ledger(out: &mut CompileOutcome, ledger: &super::ledger::Ledger) {
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    decision["ledger"] = ledger.to_json();
    out.provenance.decision = Some(decision);
}

/// Replay a recorded plan for the same intent: straight to the deterministic assembler,
/// with zero reading, zero seat calls and zero provider calls. The record's own `strategy`
/// word is kept as the outcome's strategy; the route says `replayed plan`. A record that
/// does not parse, is not anchored in this intent or still carries unknown work is a
/// finding on `recorded_plan`, never a candidate.
pub(super) fn replay(
    intent: &str,
    record: &Value,
    request: &CompileRequest,
    out: &mut CompileOutcome,
) -> Result<(), CompileError> {
    let folded = lexicon::fold_apostrophes(intent);
    let intent = folded.as_str();
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
    // A seat's plan (or a record with no strategy word) that works on nothing is asked,
    // never assembled; the reader's own HOT plan was already judged explicit.
    if strategy != Some(Strategy::Hot) && super::assemble::unfed(&plan, intent, out) {
        out.provenance.plan = Some(plan_record(&plan, strategy));
        return Ok(());
    }
    super::assemble::assemble(&plan, intent, request, out)?;
    record_retrieval(out, intent, Some(&plan));
    out.provenance.strategy = strategy;
    out.provenance.plan = Some(plan_record(&plan, strategy));
    Ok(())
}

/// The deterministic-only door: HOT under the request's contract, or an honest report.
pub(super) fn hot(
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
            unresolved(&reading, out);
            if reading.unresolved.is_empty() && reading.ambiguous.is_empty() {
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

fn unresolved(reading: &Reading, out: &mut CompileOutcome) {
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

fn settle(
    strategy: Strategy,
    plan: &Plan,
    intent: &str,
    request: &CompileRequest,
    mut out: CompileOutcome,
) -> Result<CompileOutcome, CompileError> {
    if !plan.anchored(intent) {
        super::finding(
            &mut out,
            DiagnosticKind::Unknown,
            "authoring_plan",
            "Every operation, effect and obligation needs an exact nonempty source excerpt. Nothing invented is assembled.",
        );
        record_ledger(&mut out, &super::ledger::Ledger::extract(plan));
        out.provenance.plan = Some(plan_record(plan, None));
        return Ok(out);
    }
    let mut plan = plan.clone();
    super::shape::promote_stated_rules(&mut plan, intent);
    // A seat's plan that works on nothing is asked, never assembled; the reader's own
    // HOT plan was already judged explicit.
    if strategy != Strategy::Hot && super::assemble::unfed(&plan, intent, &mut out) {
        out.provenance.strategy = Some(strategy);
        out.provenance.plan = Some(plan_record(&plan, Some(strategy)));
        return Ok(out);
    }
    super::assemble::assemble(&plan, intent, request, &mut out)?;
    record_retrieval(&mut out, intent, Some(&plan));
    out.provenance.strategy = Some(strategy);
    out.provenance.plan = Some(plan_record(&plan, Some(strategy)));
    Ok(out)
}

/// The messages of the opening authoring call: the instructions, then the request.
fn opening(intent: &str) -> Vec<Message> {
    vec![
        Message::text(Role::System, INSTRUCTIONS),
        Message::text(Role::User, intent),
    ]
}

/// The verifier's counterexample for the one repair call: which evidence failed and the
/// law it failed, never a word about meaning.
fn counterexample(defect: &proposal::Unanchored) -> String {
    format!(
        "VERIFIER: your {} `{}` cites this evidence:\n{}\nThat text is not an exact excerpt of the request: the verifier could not find it verbatim. Only wrapped lines, doubled spaces and typographic quotes are tolerated; a changed, added or missing letter or word is not. Return the complete corrected JSON, identical to your answer except that every evidence string is copied character for character from the request (an ellipsis ... may abbreviate the middle of a long clause). Do not change any op, verb, kind, detail, target, policy or region.",
        defect.role, defect.label, defect.evidence
    )
}

/// One proposal for the request: the opening call, then at most ONE bounded repair call
/// when the proposal's judged defect is an evidence the request never wrote. The seat's
/// own answer and the verifier's counterexample go back as the conversation, and the
/// repaired proposal is judged by the same merge as any other: a repair changes letters,
/// never what the seat may propose. A repair the provider fails leaves the original
/// proposal to the merge, which refuses it as before; the failure stays recorded.
async fn propose<P: ProviderInferDyn>(
    intent: &str,
    policy: &AuthoringPolicy,
    provider: &P,
    out: &mut CompileOutcome,
) -> Option<Proposal> {
    let (proposal, text) = call(policy, provider, opening(intent), out).await?;
    let Some(defect) = proposal::unanchored(intent, &proposal) else {
        return Some(proposal);
    };
    super::finding(
        out,
        DiagnosticKind::Applied,
        "authoring_plan",
        format!(
            "The seat's {} `{}` cited `{}`, which the request never wrote; one bounded repair call sent the verifier's counterexample back with the seat's own answer.",
            defect.role,
            defect.label,
            proposal::excerpt_head(&defect.evidence)
        ),
    );
    let mut messages = opening(intent);
    messages.push(Message::text(Role::Assistant, text));
    messages.push(Message::text(Role::User, counterexample(&defect)));
    match call(policy, provider, messages, out).await {
        Some((repaired, _)) => Some(repaired),
        None => Some(proposal),
    }
}

/// One authoring call, accounted in the outcome's receipt (calls, tokens, wall time):
/// the decoded proposal with the text it was decoded from, or None with the finding
/// recorded. Every call is bounded by the policy's output cap and timeout; none retries.
async fn call<P: ProviderInferDyn>(
    policy: &AuthoringPolicy,
    provider: &P,
    messages: Vec<Message>,
    out: &mut CompileOutcome,
) -> Option<(Proposal, String)> {
    let response = call_with_schema(policy, provider, messages, plan_schema(), out).await?;
    let text = match response.content.as_slice() {
        [ContentBlock::Text { text }] => text.clone(),
        _ => String::new(),
    };
    decode(&response, out).map(|proposal| (proposal, text))
}

/// One bounded call under any answer schema (the plan's, the transform's), accounted in the
/// outcome's receipt: the raw response, or None with the finding recorded. Never retries.
async fn call_with_schema<P: ProviderInferDyn>(
    policy: &AuthoringPolicy,
    provider: &P,
    messages: Vec<Message>,
    schema: Value,
    out: &mut CompileOutcome,
) -> Option<InferResponse> {
    out.provenance.cognition = AuthoringCognition::ExplicitProvider;
    let receipt = out
        .provenance
        .authoring
        .get_or_insert_with(|| AuthoringReceipt {
            model: policy.model.clone(),
            calls: 0,
            input_tokens: None,
            output_tokens: None,
            elapsed_ms: 0,
        });
    receipt.calls += 1;
    let start = std::time::Instant::now();
    let result = tokio::time::timeout(
        policy.timeout,
        provider.infer(authoring_request(policy, messages, schema)),
    )
    .await;
    if let Some(receipt) = out.provenance.authoring.as_mut() {
        receipt.elapsed_ms = receipt
            .elapsed_ms
            .saturating_add(u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX));
    }
    let response = match result {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => {
            super::finding(
                out,
                DiagnosticKind::Unknown,
                "authoring_provider",
                error.to_string(),
            );
            return None;
        }
        Err(_) => {
            super::finding(
                out,
                DiagnosticKind::Unknown,
                "authoring_provider",
                "The single authorized authoring call timed out. No retry occurred.",
            );
            return None;
        }
    };
    if let Some(receipt) = out.provenance.authoring.as_mut()
        && response.usage_reported
    {
        receipt.input_tokens =
            Some(receipt.input_tokens.unwrap_or(0) + response.usage.input_tokens);
        receipt.output_tokens =
            Some(receipt.output_tokens.unwrap_or(0) + response.usage.output_tokens);
    }
    Some(response)
}

/// The bounded JSON-schema request every authoring call makes, whatever its messages.
fn authoring_request(
    policy: &AuthoringPolicy,
    messages: Vec<Message>,
    schema: Value,
) -> InferRequest {
    let mut infer = InferRequest::new(&policy.model, messages);
    infer.max_tokens = Some(policy.max_tokens);
    infer.timeout = Some(policy.timeout);
    infer.response_format = ResponseFormat::JsonSchema(schema);
    infer
}

/// The closed shape of a proposed plan.
fn plan_schema() -> Value {
    json!({
    "type":"object","additionalProperties":false,
    "required":["steps","effects","obligations","constraints","unknowns","regions","approval_bypass"],
    "properties":{
        "steps":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["op","detail","evidence"],"properties":{
            "op":{"type":"string","enum":Op::ALL.iter().map(|o| o.word()).collect::<Vec<_>>()},
            "detail":{"type":"string"},"evidence":{"type":"string","minLength":1},
            "categories":{"type":"array","items":{"type":"string"}},
            "computation": super::predicate::computation_schema()}}},
        "effects":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["verb","target","policy","evidence"],"properties":{
            "verb":{"type":"string","enum":["create","send","publish","update","notify","refund","pay","order","merge","delete","write","effect"]},
            "target":{"type":"string"},
            "policy":{"type":"string","enum":["automatic","human_first","forbidden","unspecified","conflict"]},
            "evidence":{"type":"string","minLength":1}}}},
        "obligations":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["kind","evidence"],"properties":{
            "kind":{"type":"string","enum":["dedup","retry_bound","revision_check"]},
            "value":{"type":["integer","null"]},"evidence":{"type":"string","minLength":1}}}},
        "constraints":{"type":"array","items":{"type":"string"}},
        "unknowns":{"type":"array","items":{"type":"string"}},
        "regions":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["text","role"],"properties":{
            "text":{"type":"string","minLength":1},
            "role":{"type":"string","enum":["operation","effect","policy","obligation","constraint","context","unknown"]}}}},
        "approval_bypass":{"type":"object","additionalProperties":false,"required":["present"],"properties":{
            "present":{"type":"boolean"},"evidence":{"type":"string"}}}
    }})
}

/// COLD with N proposals, then the composer: the distinct admissible plans become a finite
/// candidate set judged by the deterministic feasibility filter. Exactly one feasible
/// candidate is used without any seat call; several and a seat is ONE closed choice over
/// the feasible candidates or NONE; several and no seat is the documented deterministic
/// rank; none is a human question. A plan that fails the deterministic facts never enters
/// the pool, and an infeasible candidate is recorded but never offered.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)] // one experiment path, fully traced
async fn sampled<P: ProviderInferDyn>(
    intent: &str,
    policy: &AuthoringPolicy,
    provider: &P,
    seat: Option<&dyn DecisionSeat>,
    reading: &Reading,
    request: &CompileRequest,
    mut route: Vec<String>,
    mut out: CompileOutcome,
) -> Result<CompileOutcome, CompileError> {
    let mut accepted: Vec<(usize, Plan)> = Vec::new();
    let mut rejected: Vec<CompileOutcome> = Vec::new();
    let mut records = Vec::new();
    let mut calls = 0;
    let mut input_tokens: Option<u64> = None;
    let mut output_tokens: Option<u64> = None;
    let mut elapsed_ms = 0;
    for index in 0..policy.samples.clamp(1, 5) as usize {
        let mut scratch = super::initial();
        let proposal = propose(intent, policy, provider, &mut scratch).await;
        let plan = proposal.and_then(|p| merge(intent, p, reading, &mut scratch));
        if let Some(receipt) = &scratch.provenance.authoring {
            calls += receipt.calls;
            elapsed_ms += receipt.elapsed_ms;
            if let Some(n) = receipt.input_tokens {
                input_tokens = Some(input_tokens.unwrap_or(0) + n);
            }
            if let Some(n) = receipt.output_tokens {
                output_tokens = Some(output_tokens.unwrap_or(0) + n);
            }
        }
        let findings: Vec<String> = scratch
            .diagnostics
            .iter()
            .filter(|d| d.kind != DiagnosticKind::Applied)
            .map(|d| d.message.clone())
            .collect();
        records.push(json!({
            "sample": index,
            "calls": scratch.provenance.authoring.as_ref().map_or(0, |r| r.calls),
            "accepted": plan.is_some(),
            "signature": plan.as_ref().map(compose::signature),
            "findings": findings,
        }));
        match plan {
            Some(plan) => accepted.push((index, plan)),
            None => rejected.push(scratch),
        }
    }
    // Every call beyond one per sample is a repair: the route says how many were bought.
    let repairs = calls.saturating_sub(policy.samples.clamp(1, 5));
    if repairs > 0 {
        route.push(format!("cold: repair {repairs}"));
    }
    out.provenance.cognition = AuthoringCognition::ExplicitProvider;
    out.provenance.authoring = Some(AuthoringReceipt {
        model: policy.model.clone(),
        calls,
        input_tokens,
        output_tokens,
        elapsed_ms,
    });
    // Distinct admissible signatures, for the sample record.
    let mut distinct: Vec<Vec<String>> = Vec::new();
    for (_, plan) in &accepted {
        let sig = compose::signature(plan);
        if !distinct.contains(&sig) {
            distinct.push(sig);
        }
    }
    // The composer: the finite candidate set and its deterministic feasibility verdicts.
    // Recall informs the pattern dimensions only; it never selects.
    let hits = super::retrieve::retrieve(intent, 5);
    let composition = compose::compose(&accepted, reading, &hits, intent);
    let candidates = composition.candidates;
    let feasible: Vec<usize> = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate.feasible())
        .map(|(k, _)| k)
        .collect();
    let mut chosen: Option<usize> = None;
    let mut warm_record = None;
    let mut seat_declined = false;
    match (feasible.len(), seat) {
        (0, _) => route.push("compose: none feasible".to_owned()),
        (1, _) => {
            chosen = feasible.first().copied();
            route.push("compose: single".to_owned());
        }
        (_, Some(seat)) => {
            // ONE closed choice over the feasible candidates, or NONE.
            let options = feasible
                .iter()
                .enumerate()
                .map(|(k, index)| {
                    ChoiceOption::new(
                        format!("plan-{k}"),
                        compose::describe(&candidates, &feasible, *index),
                    )
                })
                .collect();
            let question = ChoiceQuestion::new(
                "cold-plans",
                "Several readings of the request survived validation. Choose the one that preserves every requested operation, effect, policy and obligation without adding any; choose none if no reading is faithful.",
                json!({
                    "request": intent,
                    "plans": feasible.iter().filter_map(|k| candidates.get(*k)).map(|c| &c.signature).collect::<Vec<_>>(),
                }),
                options,
            );
            let answer = seat.choose(&question).await;
            let admitted = match &answer {
                Ok(answer) => {
                    super::decide::admit(&question, answer).map(|()| answer.choice.clone())
                }
                Err(error) => Err(error.clone()),
            };
            warm_record = Some(match (&answer, &admitted) {
                (Ok(answer), Ok(_)) => super::decide::record(&question, Ok(answer)),
                (_, Err(error)) | (Err(error), _) => super::decide::record(&question, Err(error)),
            });
            match admitted {
                Ok(choice) if choice != NONE_OPTION => {
                    let k: usize = choice
                        .trim_start_matches("plan-")
                        .parse()
                        .unwrap_or(usize::MAX);
                    chosen = feasible.get(k).copied();
                    route.push("compose: seat".to_owned());
                }
                Ok(_) => {
                    seat_declined = true;
                    route.push("compose: seat none".to_owned());
                }
                Err(_) => {
                    route.push("compose: seat failed; deterministic rank".to_owned());
                    chosen = compose::rank(&candidates, &feasible, &accepted);
                }
            }
        }
        (_, None) => {
            chosen = compose::rank(&candidates, &feasible, &accepted);
            route.push("compose: deterministic rank".to_owned());
        }
    }
    let disagreement = compose::classify_disagreement(&distinct);
    let selected = chosen.and_then(|k| candidates.get(k));
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    decision["cold_samples"] = json!({
        "requested": policy.samples,
        "accepted": accepted.len(),
        "distinct": distinct.len(),
        "disagreement": disagreement,
        "selected": selected.map(Candidate::sample),
        "samples": records,
    });
    decision["candidates"] = json!(
        candidates
            .iter()
            .enumerate()
            .map(|(k, candidate)| candidate.to_json(k))
            .collect::<Vec<_>>()
    );
    decision["feasible_count"] = json!(feasible.len());
    decision["selected_candidate"] = json!(chosen);
    decision["compose"] = json!({
        "cap": compose::CAP,
        "pattern_dimensions": composition.dimensions.iter().map(compose::Dimension::to_json).collect::<Vec<_>>(),
    });
    if let Some(record) = warm_record {
        decision["warm_after_cold"] = record;
    }
    decision["route"] = json!(route);
    out.provenance.decision = Some(decision);
    if let Some(candidate) = selected {
        let mut plan = candidate.plan.clone();
        // A computation the typed stages could not state asks the seat for a verified
        // program (treatment B), once, on the plan that will be assembled: the seat's own
        // example is the test, the runtime's jq the judge, the receipt counts the call.
        let answered = request.answers.contains_key("const.rule_expression");
        transform::synthesize(intent, &mut plan, policy, provider, answered, &mut out).await;
        return settle(Strategy::Cold, &plan, intent, request, out);
    }
    if seat_declined {
        // The seat said none of the readings is faithful: a human settles the disagreement.
        super::finding(
            &mut out,
            DiagnosticKind::RequiresHuman,
            "intent",
            format!(
                "The proposals disagree ({}) and the decision seat found none faithful; no candidate was assembled.",
                disagreement.join(", ")
            ),
        );
        super::question(
            &mut out,
            "intent.clarification",
            "Supply a complete replacement request that states each operation and effect explicitly. It explicitly replaces the earlier intent.",
            QuestionType::Text,
        );
        return Ok(out);
    }
    if let Some(first) = candidates.first() {
        // Every admissible proposal failed the deterministic feasibility filter: the
        // reasons are the findings, and a human settles what the proposals dropped.
        for (k, candidate) in candidates.iter().enumerate() {
            for reason in candidate.feasibility.as_ref().err().into_iter().flatten() {
                super::finding(
                    &mut out,
                    DiagnosticKind::Unknown,
                    "authoring_plan",
                    format!("Candidate {k} is not feasible: {reason}."),
                );
            }
        }
        super::finding(
            &mut out,
            DiagnosticKind::RequiresHuman,
            "intent",
            format!(
                "No proposal preserved every recognized fact of the request ({} candidate(s) judged infeasible); no candidate was assembled.",
                candidates.len()
            ),
        );
        super::question(
            &mut out,
            "intent.clarification",
            "Supply a complete replacement request that states each operation, effect and literal explicitly. It explicitly replaces the earlier intent.",
            QuestionType::Text,
        );
        out.provenance.plan = Some(first.plan.to_json());
        return Ok(out);
    }
    // Every sample was refused: report the first refusal's findings and question.
    if let Some(first) = rejected.into_iter().next() {
        out.diagnostics.extend(first.diagnostics);
        out.questions.extend(first.questions);
        if out.provenance.plan.is_none() {
            out.provenance.plan = first.provenance.plan;
        }
    }
    Ok(out)
}

/// Recognized EN/FR approval-bypass phrases, matched as whole-word sequences.
const APPROVAL_BYPASS: &[&[&str]] = &[
    &["without", "approval"],
    &["without", "asking"],
    &["do", "not", "ask"],
    &["sans", "mon", "accord"],
    &["sans", "accord"],
    &["ne", "pas", "demander"],
    &["yesterday"],
    &["hier"],
    &["prior", "approval"],
    &["previous", "approval"],
];

/// Negations and prohibitions in six languages: before a bypass phrase in the same
/// sentence, they turn it into a gate.
const NEGATIONS: &[&str] = &[
    "not",
    "never",
    "nothing",
    "no",
    "rien",
    "jamais",
    "ne",
    "aucun",
    "aucune",
    "interdit",
    "interdite",
    "nada",
    "nunca",
    "prohibido",
    "prohibida",
    "niente",
    "mai",
    "non",
    "vietato",
    "nichts",
    "nie",
    "niemals",
    "nicht",
    "verboten",
    "nao",
    "não",
    "proibido",
    "proibida",
];

/// Whether a recognized bypass phrase is stated as a bypass. The same words inside a
/// prohibition state a gate: « rien ne doit partir sans mon accord », « never send without
/// asking » forbid the effect until the approval, they do not skip it. The negation must
/// precede the phrase in its own sentence; « envoie-le sans mon accord, ne me demande rien »
/// stays a bypass.
fn bypass_stated(lower: &str) -> bool {
    lexicon::split_sentences(lower).into_iter().any(|sentence| {
        let words: Vec<&str> = sentence
            .split(|c: char| !c.is_alphabetic())
            .filter(|w| !w.is_empty())
            .collect();
        APPROVAL_BYPASS.iter().any(|phrase| {
            words.windows(phrase.len()).enumerate().any(|(at, window)| {
                window == *phrase && !words[..at].iter().any(|w| NEGATIONS.contains(w))
            })
        })
    })
}

/// A conservative EN/FR authority backstop applied to EVERY strategy. It cannot prove
/// arbitrary-language intent preservation (the proposal's own bypass field covers other
/// languages); it refuses the recognized bypasses and keeps recognized money movement from
/// being assembled without a human gate.
fn backstop(intent: &str, plan: &mut Plan) {
    let text = intent.to_lowercase();
    if bypass_stated(&text) {
        plan.unknowns.push(
            "The request reuses, skips or presupposes an approval (recognized approval-bypass wording); the compiler never grants that authority."
                .to_owned(),
        );
    }
    let refund_words = text.contains("refund") || text.contains("rembours");
    if refund_words && !plan.effects.iter().any(|e| e.verb == EffectVerb::Refund) {
        plan.unknowns.push(
            "The request mentions a refund that no recognized effect carries; a refund is never dropped silently."
                .to_owned(),
        );
    }
    // An automatic money movement is not unknown work: the assembler asks its approval
    // as one closed choice (`effect.<verb>.approval`).
    plan.unknowns.dedup();
}
