// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One compiler, three internal resolution strategies: HOT, WARM, COLD.
//!
//! HOT: the deterministic reader consumed every clause; zero authoring calls.
//! WARM: every clause is known but a few carry a small finite set of readings;
//! an explicit bounded decision seat picks one (or NONE) per clause.
//! COLD: a clause is unknown to the reader; one explicitly authorized generative
//! call proposes a private semantic plan, never source or permits.
//! Deterministic policy facts (prohibitions, gates, indecision, contradictions,
//! bounds) always win over a proposal, and every strategy ends in the same
//! deterministic assembler and the same Check. The least cognition that can
//! settle the intent is the one used; a permitted seat is not an obligation.
use super::{
    AuthoringCognition, AuthoringPolicy, AuthoringReceipt, CompileError, CompileOutcome,
    CompileRequest, DiagnosticKind, QuestionType, Strategy,
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
use serde_json::json;

/// The explicit cognition a caller permits for one request. Absent seats are not consent.
#[derive(Clone, Copy)]
pub struct Cognition<'a, P: ProviderInferDyn = NoProvider> {
    /// One bounded generative call for COLD, under the request's authoring policy.
    pub provider: Option<&'a P>,
    /// Bounded closed choices for WARM.
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
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposedStep {
    op: String,
    detail: String,
    evidence: String,
    #[serde(default)]
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

const INSTRUCTIONS: &str = r"Interpret the ENTIRE user request as a private semantic plan for a workflow compiler. Return only one JSON object with steps, effects, obligations, constraints, unknowns. Never produce YAML, source, tool calls, credentials, endpoints or permissions.
steps: the operations requested, in order. op is one of read (consume the document supplied with each invocation; not an external retrieval), fetch (retrieve one web page by an explicit URL in the request), lookup (retrieve existing records from an external source, database, directory, catalog, calendar, history, registry, runbook or knowledge base), search (find passages or files in a corpus of documents by a query), extract (pull structured fields out of free text, a form, a PDF or a transcript), classify (categorize or route into named categories; list the categories verbatim when named), draft (write, summarize, translate, propose in writing or draft text without sending it), compute (a numeric threshold, total or comparison that must run as code), validate (verify against explicit criteria). detail is the verbatim object of the operation. evidence is an exact nonempty verbatim substring of the request.
effects: every action that changes the outside world (create a record, send, publish, post, open a ticket, trigger a payment, mark, order, refund, merge, notify, delete). verb is one of create, send, publish, update, notify, refund, pay, order, merge, delete, effect. target is the verbatim phrase naming the action. policy is one of automatic (requested without a prior human requirement), human_first (only after a fresh explicit human validation of that exact action), forbidden (explicitly prohibited), unspecified (the requester explicitly has not decided and wants to be asked), conflict (requested and prohibited at once). evidence is an exact verbatim substring. Never drop a requested effect; never add one.
obligations: kind is one of dedup (no second action for the same incoming identifier), retry_bound (a numeric maximum of attempts, cycles or iterations; put the number in value), revision_check (recheck the current version immediately before the final action). A price, deadline, record count or number of proposed time slots is not a bound.
constraints: verbatim instructions that shape how steps run (what not to infer, what to keep null, what remains a code rule, which sources are excluded).
unknowns: requested work outside this vocabulary (durable triggers are NOT unknown: one invocation per item is supported; named SaaS systems are NOT unknown: they are lookups or effects the compiler will ask an endpoint or file for). Preserve the meaning expressed in the requester's language; never complete by guessing.";

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
/// Exact skeletons, EDIT, bounded support clauses and fully readable intents keep the
/// deterministic path and never call either seat.
///
/// # Errors
/// Returns the same representation/registry machinery failures as [`super::compile`].
#[allow(clippy::too_many_lines)] // the three strategies read top to bottom as one ladder
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
    // The exact grammar keeps its zero-call, fail-closed path when a provider is permitted.
    if let Ok(Some(plan)) = super::support::resolve(&effective_intent) {
        super::support::assemble(&plan, &assembly_request, &mut out)?;
        out.provenance.strategy = Some(Strategy::Support);
        return Ok(out);
    }
    let mut reading = lexicon::read(&effective_intent);
    backstop(&effective_intent, &mut reading.plan);
    if reading.complete() {
        return settle(
            Strategy::Hot,
            &reading.plan,
            &effective_intent,
            &assembly_request,
            out,
        );
    }
    // WARM: every clause is known; a few carry a small finite set of readings.
    if reading.unresolved.is_empty()
        && !reading.ambiguous.is_empty()
        && let Some(seat) = cognition.seat
    {
        out.provenance.cognition = AuthoringCognition::ExplicitDecision;
        let mut records = Vec::new();
        let mut settled_all = true;
        for (index, ambiguity) in reading.ambiguous.iter().enumerate() {
            let options = ambiguity
                .options
                .iter()
                .map(|op| ChoiceOption {
                    key: op.word().to_owned(),
                    description: op.definition().to_owned(),
                })
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
            return settle(
                Strategy::Warm,
                &reading.plan,
                &effective_intent,
                &assembly_request,
                out,
            );
        }
        reading.ambiguous.clear();
    }
    // COLD: one explicitly authorized generative proposal, constrained by the deterministic facts.
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
        if let Some(proposal) = propose(&effective_intent, policy, provider, &mut out).await
            && let Some(plan) = merge(&effective_intent, proposal, &reading, &mut out)
        {
            return settle(
                Strategy::Cold,
                &plan,
                &effective_intent,
                &assembly_request,
                out,
            );
        }
        return Ok(out);
    }
    unresolved(&reading, &mut out);
    Ok(out)
}

/// The deterministic-only door: HOT or an honest unresolved report. Never a seat call.
pub(super) fn hot(
    intent: &str,
    request: &CompileRequest,
    out: &mut CompileOutcome,
) -> Result<bool, CompileError> {
    let folded = lexicon::fold_apostrophes(intent);
    let intent = folded.as_str();
    let mut reading = lexicon::read(intent);
    backstop(intent, &mut reading.plan);
    if reading.complete() {
        super::assemble::assemble(&reading.plan, request, out)?;
        out.provenance.strategy = Some(Strategy::Hot);
        out.provenance.plan = Some(reading.plan.to_json());
        return Ok(true);
    }
    if reading.plan.steps.is_empty()
        && reading.plan.effects.is_empty()
        && reading.ambiguous.is_empty()
        && reading.unresolved.len() <= 1
        && reading.clauses <= 1
    {
        // Nothing recognizable: keep the historical message of the exact-skeleton door.
        return Ok(false);
    }
    unresolved(&reading, out);
    Ok(true)
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
    super::question(
        out,
        "intent.clarification",
        "Supply a complete replacement request including all work still wanted. It explicitly replaces the earlier intent.",
        QuestionType::Text,
    );
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
    out.provenance.strategy = Some(strategy);
    out.provenance.plan = Some(plan.to_json());
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
    "required":["steps","effects","obligations","constraints","unknowns"],
    "properties":{
        "steps":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["op","detail","evidence"],"properties":{
            "op":{"type":"string","enum":Op::ALL.iter().map(|o| o.word()).collect::<Vec<_>>()},
            "detail":{"type":"string"},"evidence":{"type":"string","minLength":1},
            "categories":{"type":"array","items":{"type":"string"}}}}},
        "effects":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["verb","target","policy","evidence"],"properties":{
            "verb":{"type":"string","enum":["create","send","publish","update","notify","refund","pay","order","merge","delete","effect"]},
            "target":{"type":"string"},
            "policy":{"type":"string","enum":["automatic","human_first","forbidden","unspecified","conflict"]},
            "evidence":{"type":"string","minLength":1}}}},
        "obligations":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["kind","evidence"],"properties":{
            "kind":{"type":"string","enum":["dedup","retry_bound","revision_check"]},
            "value":{"type":["integer","null"]},"evidence":{"type":"string","minLength":1}}}},
        "constraints":{"type":"array","items":{"type":"string"}},
        "unknowns":{"type":"array","items":{"type":"string"}}
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

/// The proposal joins the deterministic reading; deterministic facts win every disagreement.
#[allow(clippy::too_many_lines)] // one validation walk over steps, effects and obligations
fn merge(
    intent: &str,
    proposal: Proposal,
    reading: &Reading,
    out: &mut CompileOutcome,
) -> Option<Plan> {
    let mut plan = reading.plan.clone();
    let anchored = |evidence: &str| !evidence.trim().is_empty() && intent.contains(evidence);
    for step in proposal.steps {
        let Some(op) = Op::parse(&step.op) else {
            reject(out, "unknown operation in the proposal");
            return None;
        };
        if !anchored(&step.evidence) {
            reject(out, "an operation lacks an exact source excerpt");
            return None;
        }
        plan.push_step(Step {
            op,
            evidence: step.evidence,
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
        if !anchored(&effect.evidence) {
            reject(
                out,
                "an effect lacks an exact source excerpt; no effect was invented",
            );
            return None;
        }
        if let Some(existing) = plan.effects.iter_mut().find(|e| e.verb == verb) {
            // The deterministic policy is the floor: a model may only strengthen a plain
            // request. Any other disagreement about a recognized effect is a human question,
            // never a model verdict.
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
                evidence: effect.evidence,
                policy,
                policy_literal: None,
            });
        }
    }
    for obligation in proposal.obligations {
        if !anchored(&obligation.evidence) {
            reject(out, "an obligation lacks an exact source excerpt");
            return None;
        }
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
            plan.obligations.push(Obligation {
                kind,
                evidence: obligation.evidence,
            });
        }
    }
    for constraint in proposal.constraints {
        if !plan.constraints.contains(&constraint) {
            plan.constraints.push(constraint);
        }
    }
    plan.unknowns.extend(proposal.unknowns);
    backstop(intent, &mut plan);
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

fn reject(out: &mut CompileOutcome, why: &str) {
    super::finding(
        out,
        DiagnosticKind::Unknown,
        "authoring_plan",
        format!("The semantic plan was not assembled: {why}."),
    );
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
/// arbitrary-language intent preservation; it refuses the recognized bypasses and
/// keeps recognized money movement from being assembled without a human gate.
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
