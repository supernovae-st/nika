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
    plan::{Effect, EffectPolicy, EffectVerb, Obligation, ObligationKind, Op, Plan, Step},
    types::Input,
};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, Message, ProviderInferDyn, ResponseFormat, Role,
    StopReason,
};
use serde::Deserialize;
use serde_json::{Value, json};

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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Proposal {
    steps: Vec<ProposedStep>,
    effects: Vec<ProposedEffect>,
    obligations: Vec<ProposedObligation>,
    constraints: Vec<String>,
    unknowns: Vec<String>,
    #[serde(default)]
    regions: Vec<ProposedRegion>,
    #[serde(default)]
    approval_bypass: Option<ProposedBypass>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposedStep {
    op: String,
    detail: String,
    evidence: String,
    #[serde(default, deserialize_with = "nullable_vec")]
    categories: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposedEffect {
    verb: String,
    target: String,
    policy: String,
    evidence: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposedObligation {
    kind: String,
    #[serde(default)]
    value: Option<u32>,
    evidence: String,
}
/// One contiguous region of the request and what it is for: the accounting the model owes.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposedRegion {
    text: String,
    role: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposedBypass {
    present: bool,
    #[serde(default, deserialize_with = "nullable_string")]
    evidence: String,
}

/// A provider's strict structured-output mode may turn an optional property into an explicit
/// `null`; the decoder reads it as the absent default rather than refusing the plan.
fn nullable_vec<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    Ok(Option::<Vec<String>>::deserialize(d)?.unwrap_or_default())
}

fn nullable_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(d)?.unwrap_or_default())
}

const INSTRUCTIONS: &str = r"Interpret the ENTIRE user request, in whatever language, as a private semantic plan for a workflow compiler. Return only one JSON object with steps, effects, obligations, constraints, unknowns, regions, approval_bypass. Never produce YAML, source, tool calls, credentials, endpoints or permissions.
steps: the operations requested, in order. op is one of read (consume a document, text, file, transcript, local folder, glob or set of named files the requester supplies with the invocation; never a named system or store), fetch (retrieve one web page by an explicit URL in the request), lookup (retrieve existing records or values from any named external system, store, service, database, directory, catalog, calendar, dashboard, history, registry, runbook or knowledge base; consulting, reading, checking or querying such a source is lookup even when the request says read), search (find passages or files in a corpus of documents by a query), extract (pull structured fields out of free text, a form, a PDF or a transcript), classify (categorize or route into named categories; list the categories verbatim when named), draft (write, summarize, translate, propose in writing, correct or draft text without sending it), compute (a numeric threshold, total or comparison that must run as code), validate (verify against explicit criteria), explore (an open-ended region the request explicitly delegates to agents, bounded by turns). detail is the verbatim object of the operation. evidence is an exact nonempty verbatim substring of the request.
effects: every action that changes the outside world (create a record, send, publish, post, open a ticket, trigger a payment, mark, order, refund, merge, notify, delete, write a file). verb is one of create, send, publish, update, notify, refund, pay, order, merge, delete, write, effect. target is the verbatim phrase naming the action. policy is one of automatic (requested without a prior human requirement), human_first (only after a fresh explicit human validation of that exact action), forbidden (explicitly prohibited), unspecified (the requester explicitly has not decided and wants to be asked), conflict (requested and prohibited at once). evidence is an exact verbatim substring. Never drop a requested effect; never add one.
obligations: kind is one of dedup (no second action for the same incoming identifier), retry_bound (a numeric maximum of attempts, cycles or iterations; put the number in value), revision_check (recheck the current version immediately before the final action). A price, deadline, record count or number of proposed time slots is not a bound.
constraints: verbatim instructions that shape how steps run (what not to infer, what to keep null, what remains a code rule, which sources are excluded).
unknowns: requested work outside this vocabulary (durable triggers are NOT unknown: one invocation per item is supported; named SaaS systems are NOT unknown: they are lookups or effects the compiler will ask an endpoint or file for; the contents of a named file, folder or record are runtime data, NOT unknown).
regions: partition the WHOLE request into contiguous verbatim excerpts, in order, covering every sentence, each with role operation | effect | policy | obligation | constraint | context | unknown. Nothing meaningful may be left out of regions; a region you cannot map gets role unknown.
approval_bypass: {present: true|false, evidence: verbatim substring} when the request asks to reuse a prior approval, skip approval, act without asking, or otherwise presuppose an approval it does not give.
Preserve the meaning expressed in the requester's language; never complete by guessing; never rewrite a literal (URL, path, number, currency, name).";

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
    match admit_hot(&effective_intent, &reading, request.hot) {
        Ok(()) => {
            route.push("hot".to_owned());
            record_route(&mut out, &route);
            return settle(
                Strategy::Hot,
                &reading.plan,
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
        && lexical_rest_is_explicit(&effective_intent, &reading)
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
                        reading.plan.push_step(Step {
                            op,
                            evidence: ambiguity.clause.clone(),
                            detail: ambiguity.detail.clone(),
                            categories: Vec::new(),
                        });
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
fn plan_record(plan: &Plan, strategy: Option<Strategy>) -> Value {
    let mut record = plan.to_json();
    if let Some(strategy) = strategy {
        record["strategy"] = json!(strategy.word());
    }
    record
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
        out.provenance.plan = Some(plan_record(&plan, strategy));
        return Ok(());
    }
    super::assemble::assemble(&plan, request, out)?;
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
    match admit_hot(intent, &reading, request.hot) {
        Ok(()) => {
            record_route(out, &["hot".to_owned()]);
            super::assemble::assemble(&reading.plan, request, out)?;
            record_retrieval(out, intent, Some(&reading.plan));
            out.provenance.strategy = Some(Strategy::Hot);
            out.provenance.plan = Some(plan_record(&reading.plan, Some(Strategy::Hot)));
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
        out.provenance.plan = Some(plan.to_json());
        return Ok(out);
    }
    super::assemble::assemble(plan, request, &mut out)?;
    record_retrieval(&mut out, intent, Some(plan));
    out.provenance.strategy = Some(strategy);
    out.provenance.plan = Some(plan_record(plan, Some(strategy)));
    Ok(out)
}

async fn propose<P: ProviderInferDyn>(
    intent: &str,
    policy: &AuthoringPolicy,
    provider: &P,
    out: &mut CompileOutcome,
) -> Option<Proposal> {
    out.provenance.cognition = AuthoringCognition::ExplicitProvider;
    out.provenance.authoring = Some(AuthoringReceipt {
        model: policy.model.clone(),
        calls: 1,
        input_tokens: None,
        output_tokens: None,
        elapsed_ms: 0,
    });
    let mut infer = InferRequest::new(
        &policy.model,
        vec![
            Message::text(Role::System, INSTRUCTIONS),
            Message::text(Role::User, intent),
        ],
    );
    infer.max_tokens = Some(policy.max_tokens);
    infer.timeout = Some(policy.timeout);
    infer.response_format = ResponseFormat::JsonSchema(json!({
    "type":"object","additionalProperties":false,
    "required":["steps","effects","obligations","constraints","unknowns","regions","approval_bypass"],
    "properties":{
        "steps":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["op","detail","evidence"],"properties":{
            "op":{"type":"string","enum":Op::ALL.iter().map(|o| o.word()).collect::<Vec<_>>()},
            "detail":{"type":"string"},"evidence":{"type":"string","minLength":1},
            "categories":{"type":"array","items":{"type":"string"}}}}},
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
    }}));
    let start = std::time::Instant::now();
    let result = tokio::time::timeout(policy.timeout, provider.infer(infer)).await;
    if let Some(receipt) = out.provenance.authoring.as_mut() {
        receipt.elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
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
        receipt.input_tokens = Some(response.usage.input_tokens);
        receipt.output_tokens = Some(response.usage.output_tokens);
    }
    decode(&response, out)
}

/// The exact request excerpt a proposal's evidence names: the evidence itself when it is a
/// verbatim substring, else the request substring it matches once runs of whitespace are
/// folded on both sides (a model may wrap a line or drop a double space; it may not change
/// a word). None when nothing in the request matches.
fn exact_excerpt(intent: &str, evidence: &str) -> Option<String> {
    let evidence = evidence.trim();
    if evidence.is_empty() {
        return None;
    }
    if intent.contains(evidence) {
        return Some(evidence.to_owned());
    }
    let mut folded = String::new();
    let mut offsets: Vec<usize> = Vec::new();
    let mut pending_space = false;
    for (index, ch) in intent.char_indices() {
        if ch.is_whitespace() {
            pending_space = !folded.is_empty();
            continue;
        }
        if pending_space {
            folded.push(' ');
            offsets.push(index);
            pending_space = false;
        }
        folded.push(ch);
        for _ in 0..ch.len_utf8() {
            offsets.push(index);
        }
    }
    let needle = evidence.split_whitespace().collect::<Vec<_>>().join(" ");
    let at = folded.find(&needle)?;
    let start = *offsets.get(at)?;
    let last = *offsets.get(at + needle.len() - 1)?;
    let end = last + intent.get(last..)?.chars().next()?.len_utf8();
    intent.get(start..end).map(str::to_owned)
}

/// The head of a rejected excerpt for the finding: enough to see what the model wrote,
/// never the whole text.
fn excerpt_head(text: &str) -> String {
    let trimmed = text.trim();
    let mut head: String = trimmed.chars().take(80).collect();
    if head.len() < trimmed.len() {
        head.push('…');
    }
    head
}

/// The reader's refund backstop is a word-level guard ("refund" appears, no refund effect
/// recognized). Once a proposal exists, its own accounting decides: the unknown is withdrawn
/// when the merged plan carries a refund effect, or when every region that mentions a refund
/// was read as an operation, a constraint or context (a status value such as "refunded" in a
/// filter). A region read as an effect, a policy or unknown keeps the guard.
fn reconcile_refund_backstop(plan: &mut Plan, regions: &[ProposedRegion]) {
    const GUARD: &str = "The request mentions a refund that no recognized effect carries";
    if !plan.unknowns.iter().any(|u| u.starts_with(GUARD)) {
        return;
    }
    let mentions = |text: &str| {
        let lower = text.to_lowercase();
        lower.contains("refund") || lower.contains("rembours")
    };
    let carried = plan.effects.iter().any(|e| e.verb == EffectVerb::Refund);
    let mentioning: Vec<&ProposedRegion> = regions.iter().filter(|r| mentions(&r.text)).collect();
    let explained = !mentioning.is_empty()
        && mentioning.iter().all(|r| {
            matches!(
                r.role.as_str(),
                "operation" | "constraint" | "context" | "obligation"
            )
        });
    if carried || explained {
        plan.unknowns.retain(|u| !u.starts_with(GUARD));
    }
}

fn decode(response: &InferResponse, out: &mut CompileOutcome) -> Option<Proposal> {
    let text = match response.content.as_slice() {
        [ContentBlock::Text { text }]
            if text.len() <= 65_536 && response.stop_reason == StopReason::EndTurn =>
        {
            text
        }
        _ => {
            super::finding(
                out,
                DiagnosticKind::Unknown,
                "authoring_plan",
                "Authoring must return one complete bounded JSON text, without tools or other content.",
            );
            return None;
        }
    };
    if let Ok(plan) = serde_json::from_str(text) {
        Some(plan)
    } else {
        super::finding(
            out,
            DiagnosticKind::Unknown,
            "authoring_plan",
            "The authoring response is not a valid closed semantic plan. No source was emitted.",
        );
        None
    }
}

/// The proposal joins the deterministic reading; deterministic facts win every disagreement,
/// and the proposal must account for every region of the request.
#[allow(clippy::too_many_lines)] // one validation walk over steps, effects, obligations, regions
fn merge(
    intent: &str,
    proposal: Proposal,
    reading: &Reading,
    out: &mut CompileOutcome,
) -> Option<Plan> {
    // The deterministic reading contributes its POLICY floor (effects with their policy,
    // obligations, constraints, bindings, unknowns), never its operation guesses: a clause the
    // reader consumed is not understanding, and the model must account for every region.
    let mut plan = Plan {
        steps: Vec::new(),
        ..reading.plan.clone()
    };
    for step in proposal.steps {
        let Some(op) = Op::parse(&step.op) else {
            reject(out, "unknown operation in the proposal");
            return None;
        };
        let Some(evidence) = exact_excerpt(intent, &step.evidence) else {
            reject(
                out,
                &format!(
                    "an operation lacks an exact source excerpt (`{}` names `{}`)",
                    step.op,
                    excerpt_head(&step.evidence)
                ),
            );
            return None;
        };
        plan.push_step(Step {
            op,
            evidence,
            detail: step.detail,
            categories: step.categories,
        });
    }
    for effect in proposal.effects {
        let (Some(verb), Some(policy)) = (
            EffectVerb::parse(&effect.verb),
            match effect.policy.as_str() {
                "automatic" => Some(EffectPolicy::Automatic),
                "human_first" => Some(EffectPolicy::HumanFirst),
                "forbidden" => Some(EffectPolicy::Forbidden),
                "unspecified" => Some(EffectPolicy::Undecided),
                "conflict" => Some(EffectPolicy::Conflict),
                _ => None,
            },
        ) else {
            reject(out, "unknown effect verb or policy in the proposal");
            return None;
        };
        let Some(evidence) = exact_excerpt(intent, &effect.evidence) else {
            reject(
                out,
                &format!(
                    "an effect lacks an exact source excerpt (`{}` names `{}`); no effect was invented",
                    effect.verb,
                    excerpt_head(&effect.evidence)
                ),
            );
            return None;
        };
        if let Some(existing) = plan
            .effects
            .iter_mut()
            .find(|e| e.verb == verb && same_write(verb, &e.target, &effect.target))
        {
            // The deterministic policy is the floor: a model may only strengthen a plain
            // request. Any other disagreement about a recognized effect is a human question.
            if !effect.target.trim().is_empty() {
                existing.target.clone_from(&effect.target);
                existing.evidence.clone_from(&evidence);
            }
            if existing.policy == EffectPolicy::Automatic && policy != EffectPolicy::Automatic {
                existing.policy = policy;
            } else if existing.policy != policy {
                plan.unknowns.push(format!(
                    "The proposal reads `{}` as {} while the request's explicit wording reads {}; the disagreement is not settled by a model.",
                    verb.word(),
                    policy.word(),
                    existing.policy.word()
                ));
            }
        } else {
            plan.effects.push(Effect {
                verb,
                target: effect.target,
                evidence,
                policy,
                policy_literal: None,
            });
        }
    }
    for obligation in proposal.obligations {
        let Some(evidence) = exact_excerpt(intent, &obligation.evidence) else {
            reject(
                out,
                &format!(
                    "an obligation lacks an exact source excerpt (`{}` names `{}`)",
                    obligation.kind,
                    excerpt_head(&obligation.evidence)
                ),
            );
            return None;
        };
        let kind = match (obligation.kind.as_str(), obligation.value) {
            ("dedup", _) => ObligationKind::Dedup,
            ("revision_check", _) => ObligationKind::RevisionCheck,
            ("retry_bound", Some(n)) if n > 0 => ObligationKind::RetryBound(n),
            _ => {
                reject(out, "an obligation is malformed");
                return None;
            }
        };
        if !plan
            .obligations
            .iter()
            .any(|o| o.kind.word() == kind.word())
        {
            plan.obligations.push(Obligation { kind, evidence });
        }
    }
    for constraint in proposal.constraints {
        if !plan.constraints.contains(&constraint) {
            plan.constraints.push(constraint);
        }
    }
    plan.unknowns.extend(proposal.unknowns);
    // Semantic accounting: the request must be covered by regions the model can name.
    if let Some(bypass) = proposal.approval_bypass
        && bypass.present
        && exact_excerpt(intent, &bypass.evidence).is_some()
    {
        plan.unknowns.push(format!(
            "The request presupposes, reuses or skips an approval it does not give ({}); the compiler never grants that authority.",
            bypass.evidence.trim()
        ));
    }
    for gap in accounting_gaps(intent, &proposal.regions) {
        plan.unknowns.push(gap);
    }
    backstop(intent, &mut plan);
    reconcile_refund_backstop(&mut plan, &proposal.regions);
    plan.unknowns.dedup();
    if !plan.unknowns.is_empty() {
        super::finding(
            out,
            DiagnosticKind::Unknown,
            "intent",
            "The semantic plan contains unresolved requested work; no substitute workflow was emitted.",
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
        out.provenance.plan = Some(plan.to_json());
        return None;
    }
    if plan.steps.is_empty() && plan.effects.is_empty() {
        reject(out, "the proposal names no operation and no effect");
        return None;
    }
    Some(plan)
}

/// Regions the proposal left unaccounted, or named as unknown. A model that cannot say what a
/// span of the request is for has not understood it; nothing is dropped silently.
fn accounting_gaps(intent: &str, regions: &[ProposedRegion]) -> Vec<String> {
    let mut gaps = Vec::new();
    if regions.is_empty() {
        // A seat that returned no regions is not penalized here; the anchored evidence of
        // steps and effects remains the floor.
        return gaps;
    }
    let mut covered = vec![false; intent.len()];
    for region in regions {
        let text = region.text.trim();
        if text.is_empty() {
            continue;
        }
        let mut from = 0;
        while let Some(pos) = intent.get(from..).and_then(|rest| rest.find(text)) {
            let start = from + pos;
            let end = start + text.len();
            for flag in covered.iter_mut().take(end).skip(start) {
                *flag = true;
            }
            from = end;
        }
        if region.role == "unknown" {
            gaps.push(format!(
                "The proposal could not map this part of the request: {text}"
            ));
        }
    }
    // Any uncovered run of meaningful characters is an unaccounted region.
    let mut run = String::new();
    let mut runs = Vec::new();
    for (index, ch) in intent.char_indices() {
        let flagged = covered.get(index).copied().unwrap_or(true);
        if flagged {
            if run.trim().chars().filter(|c| c.is_alphanumeric()).count() >= 12 {
                runs.push(run.trim().to_owned());
            }
            run.clear();
        } else {
            run.push(ch);
        }
    }
    if run.trim().chars().filter(|c| c.is_alphanumeric()).count() >= 12 {
        runs.push(run.trim().to_owned());
    }
    for text in runs {
        gaps.push(format!(
            "The proposal does not account for this part of the request: {text}"
        ));
    }
    gaps
}

/// Two writes are one effect only when they name the same file; a write that names no
/// file joins the recognized one, a write to another file is its own effect.
fn same_write(verb: EffectVerb, existing: &str, proposed: &str) -> bool {
    verb != EffectVerb::Write
        || match (
            super::paths::single_file(existing),
            super::paths::single_file(proposed),
        ) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        }
}

fn reject(out: &mut CompileOutcome, why: &str) {
    super::finding(
        out,
        DiagnosticKind::Unknown,
        "authoring_plan",
        format!("The semantic plan was not assembled: {why}."),
    );
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
        let plan = proposal.and_then(|p| merge(intent, p, reading, &mut scratch));
        let findings: Vec<String> = scratch
            .diagnostics
            .iter()
            .filter(|d| d.kind != DiagnosticKind::Applied)
            .map(|d| d.message.clone())
            .collect();
        records.push(json!({
            "sample": index,
            "accepted": plan.is_some(),
            "signature": plan.as_ref().map(compose::signature),
            "findings": findings,
        }));
        match plan {
            Some(plan) => accepted.push((index, plan)),
            None => rejected.push(scratch),
        }
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
        let plan = candidate.plan.clone();
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

/// A conservative EN/FR authority backstop applied to EVERY strategy. It cannot prove
/// arbitrary-language intent preservation (the proposal's own bypass field covers other
/// languages); it refuses the recognized bypasses and keeps recognized money movement from
/// being assembled without a human gate.
fn backstop(intent: &str, plan: &mut Plan) {
    let text = intent.to_lowercase();
    let words: Vec<&str> = text
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
        .collect();
    let bypass = APPROVAL_BYPASS
        .iter()
        .any(|phrase| words.windows(phrase.len()).any(|window| window == *phrase));
    if bypass {
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
    for effect in &plan.effects {
        if effect.verb.moves_money() && effect.policy == EffectPolicy::Automatic {
            plan.unknowns.push(format!(
                "`{}` moves money without a prior human approval; only a human-first version is constructible.",
                effect.verb.word()
            ));
        }
    }
    plan.unknowns.dedup();
}
