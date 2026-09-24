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
    CompileRequest, DiagnosticKind, HotPolicy, NativeMode, QuestionType, Strategy,
    compose::{self, Candidate},
    decide::{ChoiceOption, ChoiceQuestion, DecisionSeat, NONE_OPTION},
    gates::backstop,
    lexicon::{self, Reading},
    plan::{Op, Plan, Step},
    types::{EditChange, Input},
};
use super::{
    admit_hot, intent_sha256, lexical_rest_is_explicit, plan_record, record_ledger,
    record_retrieval, record_route, replay, unresolved,
};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, Message, ProviderInferDyn, ResponseFormat, Role,
};
use serde_json::{Value, json};

mod instructions;
use instructions::INSTRUCTIONS;
mod backstops;
pub(super) mod knowledge;
mod native;
mod proposal;
mod sketch;
mod transform;
use proposal::{Proposal, decode, merge};
pub(super) use proposal::{ProposedRegion, nullable_default};

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

/// An EDIT under a seat: the constant door first (zero calls); when the change is more than a
/// constant and a native policy names a seat, the native door revises the base — the seat
/// reads the base candidate and the change in words beside the request the base answered, the
/// laws allow the base's own literals, and the outcome states the meaning delta. No seat, or a
/// change the constant door settles: the deterministic outcome as before.
async fn revise<P: ProviderInferDyn>(
    request: &CompileRequest,
    cognition: Cognition<'_, P>,
) -> Result<CompileOutcome, CompileError> {
    let deterministic = super::compile(request)?;
    let Input::Edit {
        change: EditChange::Text(_),
        ..
    } = &request.input
    else {
        return Ok(deterministic);
    };
    let unresolved = deterministic
        .diagnostics
        .iter()
        .any(|d| d.target == "change_request");
    let (Some(policy), Some(provider)) = (&request.authoring, cognition.provider) else {
        return Ok(deterministic);
    };
    if !unresolved || policy.native == NativeMode::Off {
        return Ok(deterministic);
    }
    let Some(folded) = super::revise_intent(request) else {
        return Ok(deterministic);
    };
    let mut out = super::initial();
    if !policy_bounded(policy, &folded) {
        super::finding(
            &mut out,
            DiagnosticKind::Missed,
            "authoring_policy",
            POLICY_BOUNDS,
        );
        return Ok(out);
    }
    let reading = lexicon::read(&folded);
    native::author(
        &folded,
        &reading,
        policy,
        provider,
        request,
        vec![
            "edit: the constant door could not settle the change; the seat revises the base"
                .to_owned(),
        ],
        out,
    )
    .await
}

/// Compile with explicit cognition: a decision seat (WARM) and/or a generative provider (COLD).
/// Exact skeletons and resolved constant edits keep the zero-call path. A text revision
/// may use native authoring under an explicit policy; CREATE follows the selected strategy,
/// including native/sketch modes that can precede the deterministic intent path.
///
/// # Errors
/// Returns the same representation/registry machinery failures as [`super::compile`].
#[allow(clippy::too_many_lines)] // the resolution ladder reads top to bottom
pub async fn compile_with_cognition<P: ProviderInferDyn>(
    request: &CompileRequest,
    cognition: Cognition<'_, P>,
) -> Result<CompileOutcome, CompileError> {
    let mut out = compile_inner(request, cognition).await?;
    nika_compile::surface::observed::record(request, &mut out);
    Ok(out)
}

#[allow(clippy::too_many_lines)] // the resolution ladder reads top to bottom
async fn compile_inner<P: ProviderInferDyn>(
    request: &CompileRequest,
    cognition: Cognition<'_, P>,
) -> Result<CompileOutcome, CompileError> {
    let Input::Create(intent) = &request.input else {
        return revise(request, cognition).await;
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
        if nika_compile::surface::pending_transform::present(record)
            && request.answers.contains_key("intent.clarification")
        {
            nika_compile::surface::pending_transform::invalid(
                &mut out,
                "A replacement request invalidates pending transform field choices; compile afresh.",
            );
            return Ok(out);
        }
        replay(&effective_intent, record, &assembly_request, &mut out)?;
        if record.get("pending_transform").is_some()
            && let (Some(policy), Some(provider)) = (&request.authoring, cognition.provider)
        {
            if !policy_bounded(policy, &effective_intent) {
                super::finding(
                    &mut out,
                    DiagnosticKind::Missed,
                    "authoring_policy",
                    POLICY_BOUNDS,
                );
                return Ok(out);
            }
            return transform::resume(&effective_intent, &assembly_request, policy, provider, out)
                .await;
        }
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
    if let Some(columns) = nika_compile::surface::observed::for_intent(
        assembly_request.knowledge.as_ref(),
        &effective_intent,
    ) {
        reading.columns = columns;
    }
    backstop(&effective_intent, &mut reading.plan);
    // The deterministic door judges the reading with its stated rules promoted: a rule
    // carries its own constraint, and the words inside it are its literals. The reading
    // itself keeps its constraints: they are the policy floor a seat's proposal inherits.
    // The ablation and the arena's treatment D: straight to the native candidate, before the
    // deterministic door and without the private plan, under the same bounds as COLD.
    if let (Some(policy), Some(provider)) = (&request.authoring, cognition.provider)
        && matches!(policy.native, NativeMode::Only | NativeMode::Sketch)
    {
        if !policy_bounded(policy, &effective_intent) {
            super::finding(
                &mut out,
                DiagnosticKind::Missed,
                "authoring_policy",
                POLICY_BOUNDS,
            );
            return Ok(out);
        }
        if policy.native == NativeMode::Sketch {
            route.push("native: sketch".to_owned());
            return sketch::author(
                &effective_intent,
                &reading,
                policy,
                provider,
                &assembly_request,
                route,
                out,
            )
            .await;
        }
        route.push("native: only".to_owned());
        return native::author(
            &effective_intent,
            &reading,
            policy,
            provider,
            &assembly_request,
            route,
            out,
        )
        .await;
    }
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
        Err(why) => route.push(format!("hot rejected: {}", why.reasons().join("; "))),
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
        if !policy_bounded(policy, &effective_intent) {
            super::finding(
                &mut out,
                DiagnosticKind::Missed,
                "authoring_policy",
                POLICY_BOUNDS,
            );
            return Ok(out);
        }
        route.push(format!("cold: {} sample(s)", policy.samples.clamp(1, 5)));
        let cold = sampled(
            &effective_intent,
            policy,
            provider,
            cognition.seat,
            &reading,
            &assembly_request,
            route.clone(),
            out,
        )
        .await?;
        // The private plan is not the language's ceiling: a cold round that ends without a
        // candidate, or hands the human a machine's problem, escalates to a native candidate.
        if cold
            .provenance
            .plan
            .as_ref()
            .is_some_and(|record| record.get("pending_transform").is_some())
        {
            return Ok(cold);
        }
        if policy.native == NativeMode::Escalate && native::escalates(&cold) {
            let mut route = route;
            route.push("native: escalated".to_owned());
            return native::author(
                &effective_intent,
                &reading,
                policy,
                provider,
                &assembly_request,
                route,
                cold,
            )
            .await;
        }
        return Ok(cold);
    }
    route.push("needs cognition".to_owned());
    record_route(&mut out, &route);
    unresolved(&reading, &mut out);
    Ok(out)
}

const POLICY_BOUNDS: &str = "Authoring requires an explicit model, 1..32768 output tokens, a timeout up to 600 seconds, and an intent no larger than 32768 bytes.";

/// The bounds every seat call honors: an explicit model, a bounded answer, a bounded wait,
/// a request the seat can hold.
fn policy_bounded(policy: &AuthoringPolicy, intent: &str) -> bool {
    !policy.model.trim().is_empty()
        && (1..=32_768).contains(&policy.max_tokens)
        && !policy.timeout.is_zero()
        && policy.timeout <= std::time::Duration::from_secs(600)
        && intent.len() <= 32_768
}

/// The first complete JSON object of a seat's text — the text itself when it is one, else
/// the balanced `{…}` it carries (a seat that wraps its answer in prose or a fence is not a
/// lost call). None when the text carries no balanced object.
pub(super) fn first_json_object(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Some(trimmed);
    }
    let start = text.find('{')?;
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escaped = false;
    for (i, ch) in text[start..].char_indices() {
        if in_string {
            match ch {
                '\\' if !escaped => {
                    escaped = true;
                    continue;
                }
                '"' if !escaped => in_string = false,
                _ => {}
            }
            escaped = false;
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..start + i + ch.len_utf8()]);
                }
            }
            _ => {}
        }
    }
    None
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
    let (proposal, text) = call(policy, provider, "plan", opening(intent), out).await?;
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
    match call(policy, provider, "repair", messages, out).await {
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
    role: &'static str,
    messages: Vec<Message>,
    out: &mut CompileOutcome,
) -> Option<(Proposal, String)> {
    let response = call_with_schema(policy, provider, role, messages, plan_schema(), out).await?;
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
    role: &'static str,
    messages: Vec<Message>,
    schema: Value,
    out: &mut CompileOutcome,
) -> Option<InferResponse> {
    out.provenance.cognition = AuthoringCognition::ExplicitProvider;
    let receipt = out
        .provenance
        .authoring
        .get_or_insert_with(|| AuthoringReceipt::new(policy.model.clone()));
    receipt.calls += 1;
    receipt
        .context
        .push(context_entry(role, &messages, &schema));
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

/// What one call received: its role, the sha256 of its instruction (the system message)
/// and of its answer schema, the bytes of its messages, and the references sent with it.
fn context_entry(role: &str, messages: &[Message], schema: &Value) -> Value {
    let sha = knowledge::sha256;
    let text_of = |m: &Message| -> String {
        m.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    };
    let instruction = messages
        .iter()
        .find(|m| matches!(m.role, Role::System))
        .map(text_of)
        .unwrap_or_default();
    let bytes: usize = messages.iter().map(|m| text_of(m).len()).sum();
    json!({
        "call": role,
        "instruction_sha256": sha(&instruction),
        "schema_sha256": sha(&schema.to_string()),
        "message_bytes": bytes,
        "references": [],
    })
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
    let mut schema: Value = serde_json::from_str(include_str!("../assets/plan_schema.json"))
        .unwrap_or_else(|_| json!({"type": "object"}));
    let step = &mut schema["properties"]["steps"]["items"]["properties"];
    step["op"]["enum"] = json!(Op::ALL.iter().map(|o| o.word()).collect::<Vec<_>>());
    step["computation"] = super::predicate::computation_schema();
    schema
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
    let mut context: Vec<Value> = Vec::new();
    for index in 0..policy.samples.clamp(1, 5) as usize {
        let mut scratch = super::initial();
        let proposal = propose(intent, policy, provider, &mut scratch).await;
        let plan = proposal.and_then(|p| merge(intent, p, reading, &mut scratch));
        if let Some(receipt) = &scratch.provenance.authoring {
            calls += receipt.calls;
            elapsed_ms += receipt.elapsed_ms;
            context.extend(receipt.context.iter().cloned());
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
    let mut receipt = AuthoringReceipt::new(policy.model.clone());
    receipt.calls = calls;
    receipt.input_tokens = input_tokens;
    receipt.output_tokens = output_tokens;
    receipt.elapsed_ms = elapsed_ms;
    receipt.context = context;
    out.provenance.authoring = Some(receipt);
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
        if let Some(mut pending) =
            transform::synthesize(intent, &mut plan, policy, provider, request, &mut out).await
        {
            pending.answer(request, &mut out);
            pending.suspend(&plan, &mut out);
            return Ok(out);
        }
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
