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
    ContentBlock, InferRequest, InferResponse, Message, ProviderInferDyn, Role,
};
use serde_json::{Value, json};

mod admitted;
mod instructions;
use instructions::INSTRUCTIONS;
mod backstops;
pub(super) mod knowledge;
mod native;
mod proposal;
mod receipt;
use receipt::call_with_schema;
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
    // A change whose words both ask for an effect and prohibit it stays the human's (R4 S0): no
    // seat revises the base to choose a side.
    if nika_compile::surface::assemble::refuse_contradiction(&reading.plan, &mut out) {
        record_route(
            &mut out,
            &["edit: the change contradicts itself".to_owned()],
        );
        return Ok(out);
    }
    Box::pin(native::author(
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
    ))
    .await
}

/// Compile with explicit cognition: a decision seat (WARM) and/or a generative provider (COLD).
/// Exact skeletons and resolved constant edits keep the zero-call path. A text revision
/// may use native authoring under an explicit policy; CREATE follows the selected strategy,
/// including native/sketch modes that can precede the deterministic intent path. The money
/// the host admitted or its operator stated is read first, for every strategy (R4 B15).
///
/// # Errors
/// Returns the same representation/registry machinery failures as [`super::compile`].
#[allow(clippy::too_many_lines)] // the resolution ladder reads top to bottom
pub async fn compile_with_cognition<P: ProviderInferDyn>(
    request: &CompileRequest,
    cognition: Cognition<'_, P>,
) -> Result<CompileOutcome, CompileError> {
    let admitted::Money {
        reading,
        record,
        closed,
    } = match admitted::read(request) {
        Ok(money) => money,
        Err(refused) => return Ok(*refused),
    };
    let offered = cognition.provider.is_some() || cognition.seat.is_some();
    let seats = if closed.is_some() {
        Cognition::default()
    } else {
        cognition
    };
    let mut out = compile_inner(&reading, seats).await?;
    if let Some(money) = record {
        admitted::record(
            request,
            money,
            closed.as_deref().filter(|_| offered),
            &mut out,
        );
    }
    nika_compile::surface::observed::record(request, &mut out);
    Ok(out)
}

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
    Box::pin(resolve_create(
        &effective_intent,
        request,
        &assembly_request,
        cognition,
        out,
    ))
    .await
}

async fn resolve_create<P: ProviderInferDyn>(
    intent: &str,
    request: &CompileRequest,
    assembly_request: &CompileRequest,
    cognition: Cognition<'_, P>,
    mut out: CompileOutcome,
) -> Result<CompileOutcome, CompileError> {
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
        replay(intent, record, assembly_request, &mut out)?;
        if record.get("pending_transform").is_some()
            && let (Some(policy), Some(provider)) = (&request.authoring, cognition.provider)
        {
            if !policy_bounded(policy, intent) {
                super::finding(
                    &mut out,
                    DiagnosticKind::Missed,
                    "authoring_policy",
                    POLICY_BOUNDS,
                );
                return Ok(out);
            }
            return transform::resume(intent, assembly_request, policy, provider, out).await;
        }
        return Ok(out);
    }
    // The exact grammar keeps its zero-call, fail-closed path when a provider is permitted.
    if let Ok(Some(plan)) = super::support::resolve(intent) {
        super::support::assemble(&plan, assembly_request, &mut out)?;
        out.provenance.strategy = Some(Strategy::Support);
        return Ok(out);
    }
    record_retrieval(&mut out, intent, None);
    let mut reading = lexicon::read(intent);
    if let Some(columns) =
        nika_compile::surface::observed::for_intent(assembly_request.knowledge.as_ref(), intent)
    {
        reading.columns = columns;
    }
    backstop(intent, &mut reading.plan);
    route_create(intent, request, assembly_request, cognition, reading, out).await
}

async fn route_create<P: ProviderInferDyn>(
    intent: &str,
    request: &CompileRequest,
    assembly_request: &CompileRequest,
    cognition: Cognition<'_, P>,
    reading: Reading,
    mut out: CompileOutcome,
) -> Result<CompileOutcome, CompileError> {
    // An effect the request's own words both ask for and prohibit stays the human's (R4 S0):
    // no seat reads it to choose a side, whatever the strategy. The outcome is the deterministic
    // door's own refusal, the one every door states, of the request as read: a clarification
    // that replaced the original is the request (the door never falls back to the original).
    if reading
        .plan
        .effects
        .iter()
        .any(|e| e.policy == crate::plan::EffectPolicy::Conflict)
    {
        let mut read_as = assembly_request.clone();
        read_as.input = Input::Create(intent.to_owned());
        return super::compile(&read_as);
    }
    let mut route = Vec::new();
    // The deterministic door judges the reading with its stated rules promoted: a rule
    // carries its own constraint, and the words inside it are its literals. The reading
    // itself keeps its constraints: they are the policy floor a seat's proposal inherits.
    // The ablation and the arena's treatment D: straight to the native candidate, before the
    // deterministic door and without the private plan, under the same bounds as COLD.
    if let (Some(policy), Some(provider)) = (&request.authoring, cognition.provider)
        && matches!(policy.native, NativeMode::Only | NativeMode::Sketch)
    {
        if !policy_bounded(policy, intent) {
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
            // Boxed: the seat doors are rare and large; they must not grow every compile future.
            return Box::pin(sketch::author(
                intent,
                &reading,
                policy,
                provider,
                assembly_request,
                route,
                out,
            ))
            .await;
        }
        route.push("native: only".to_owned());
        return Box::pin(native::author(
            intent,
            &reading,
            policy,
            provider,
            assembly_request,
            route,
            out,
        ))
        .await;
    }
    let mut admitted = reading.clone();
    super::shape::promote_stated_rules(&mut admitted.plan, intent);
    match admit_hot(intent, &admitted, request.hot) {
        Ok(()) => {
            route.push("hot".to_owned());
            record_route(&mut out, &route);
            return settle(Strategy::Hot, &admitted.plan, intent, assembly_request, out);
        }
        Err(why) => route.push(format!("hot rejected: {}", why.reasons().join("; "))),
    }
    choose_create(
        intent,
        request,
        assembly_request,
        cognition,
        reading,
        route,
        out,
    )
    .await
}

async fn choose_create<P: ProviderInferDyn>(
    intent: &str,
    request: &CompileRequest,
    assembly_request: &CompileRequest,
    cognition: Cognition<'_, P>,
    mut reading: Reading,
    mut route: Vec<String>,
    mut out: CompileOutcome,
) -> Result<CompileOutcome, CompileError> {
    // WARM on lexical ambiguity: every clause is known; a few carry a finite set of readings
    // and the rest of the reading is strictly explicit.
    if reading.unresolved.is_empty()
        && !reading.ambiguous.is_empty()
        && request.hot != HotPolicy::Off
        && let Some(seat) = cognition.seat
        && lexical_rest_is_explicit(intent, &{
            let mut admitted = reading.clone();
            super::shape::promote_stated_rules(&mut admitted.plan, intent);
            admitted
        })
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
                json!({"request": intent, "clause": ambiguity.clause, "object": ambiguity.detail}),
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
            return settle(Strategy::Warm, &reading.plan, intent, assembly_request, out);
        }
        route.push("warm: none".to_owned());
        reading.ambiguous.clear();
    }
    author_create(
        intent,
        request,
        assembly_request,
        cognition,
        reading,
        route,
        out,
    )
    .await
}

async fn author_create<P: ProviderInferDyn>(
    intent: &str,
    request: &CompileRequest,
    assembly_request: &CompileRequest,
    cognition: Cognition<'_, P>,
    reading: Reading,
    mut route: Vec<String>,
    mut out: CompileOutcome,
) -> Result<CompileOutcome, CompileError> {
    // COLD: explicitly authorized generative proposals, constrained by the deterministic facts.
    if let (Some(policy), Some(provider)) = (&request.authoring, cognition.provider) {
        if !policy_bounded(policy, intent) {
            super::finding(
                &mut out,
                DiagnosticKind::Missed,
                "authoring_policy",
                POLICY_BOUNDS,
            );
            return Ok(out);
        }
        // HOT and finite WARM judgments keep their place. Open generation starts with
        // the attached knowledge instead of first paying for a plan that cannot read it.
        if policy.native == NativeMode::Escalate
            && request
                .authoring_knowledge
                .as_ref()
                .is_some_and(|pack| !pack.references.is_empty())
        {
            route.push("native: informed generation".to_owned());
            return Box::pin(native::author(
                intent,
                &reading,
                policy,
                provider,
                assembly_request,
                route,
                out,
            ))
            .await;
        }
        route.push(format!("cold: {} sample(s)", policy.samples.clamp(1, 5)));
        let cold = sampled(
            intent,
            policy,
            provider,
            cognition.seat,
            &reading,
            assembly_request,
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
            return Box::pin(native::author(
                intent,
                &reading,
                policy,
                provider,
                assembly_request,
                route,
                cold,
            ))
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
        && policy
            .initial_max_tokens
            .is_none_or(|initial| (1..=policy.max_tokens).contains(&initial))
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
    balanced_object(text, 0)?.ok().map(|range| &text[range])
}

/// The unclosed braces a scan retries after, before it stops judging.
const UNCLOSED_RETRIES: usize = 64;

/// What the complete JSON objects of a seat's text come to, judged by the answer's own shape.
pub(super) enum Objects<'a> {
    /// No complete, non-empty JSON object: the text keeps its syntax path.
    None,
    /// The one answer to read (the first object when none is an answer), and the objects
    /// beside it that cannot be answers: kept by digest, never read.
    One {
        answer: &'a str,
        unread: Vec<&'a str>,
    },
    /// Two or more different answers, which the journal keeps by digest: none is read.
    Two(Vec<&'a str>),
    /// An object that never closes beside a complete one, or unclosed braces past the retry
    /// bound: undecided, never read as one answer. The complete objects, by digest.
    Undecided(Vec<&'a str>),
}

/// Every complete JSON object of a seat's text, an identical repetition once; prose, template
/// braces and empty objects are skipped. `is_answer` is the answer's own shape: two answers are
/// never resolved by reading the first, and an example that cannot be one never kills it.
pub(super) fn answer_objects(text: &str, is_answer: impl Fn(&str) -> bool) -> Objects<'_> {
    let mut objects: Vec<&str> = Vec::new();
    let (mut from, mut retries, mut undecided) = (0, 0, false);
    while let Some(group) = balanced_object(text, from) {
        match group {
            Ok(range) => {
                let object = &text[range.clone()];
                let empty = object[1..object.len() - 1].trim().is_empty();
                if !empty
                    && !objects.contains(&object)
                    && serde_json::from_str::<serde::de::IgnoredAny>(object).is_ok()
                {
                    objects.push(object);
                }
                from = range.end;
            }
            Err(start) => {
                // A brace that opens like an object (`{` then `"`) and never closes may be a
                // cut answer.
                undecided |= text[start + 1..].trim_start().starts_with('"');
                retries += 1;
                if retries > UNCLOSED_RETRIES {
                    undecided = true;
                    break;
                }
                from = start + 1;
            }
        }
    }
    let Some(&first) = objects.first() else {
        return Objects::None;
    };
    if undecided {
        return Objects::Undecided(objects);
    }
    let answers: Vec<&str> = objects
        .iter()
        .copied()
        .filter(|object| is_answer(object))
        .collect();
    let answer = match answers.as_slice() {
        [] => first,
        [answer] => answer,
        _ => return Objects::Two(answers),
    };
    let unread = objects
        .into_iter()
        .filter(|object| *object != answer)
        .collect();
    Objects::One { answer, unread }
}

/// Whether an object is shaped like an answer of type `T`: it decodes as one, or it carries one
/// of the `keys` only such an answer carries. A defect (an unknown key, a null field) never
/// turns a competing answer into an example.
pub(super) fn answer_shaped<T: serde::de::DeserializeOwned>(object: &str, keys: &[&str]) -> bool {
    serde_json::from_str::<T>(object).is_ok()
        || serde_json::from_str::<serde_json::Map<String, Value>>(object)
            .is_ok_and(|map| keys.iter().any(|key| map.contains_key(*key)))
}

/// Objects by digest and length, as the journals keep them.
pub(super) fn digests(objects: &[&str]) -> Value {
    objects
        .iter()
        .map(|object| json!({"sha256": knowledge::sha256(object), "bytes": object.len()}))
        .collect()
}

/// Objects of a seat's text recorded on the call that returned them, under `field`: the unread
/// ones beside an answer, the competitors of a refused text. Never read, never silent.
pub(super) fn record_objects(out: &mut CompileOutcome, field: &str, objects: &[&str]) {
    if objects.is_empty() {
        return;
    }
    if let Some(call) = out
        .provenance
        .authoring
        .as_mut()
        .and_then(|receipt| receipt.context.last_mut())
    {
        call[field] = digests(objects);
    }
}

/// The group the syntax path judges when no complete object is JSON: the first closed brace
/// group that opens like a JSON object (`{` then `"`), so template or prose braces beside a
/// broken answer are never the target of its diagnostic. None when no group opens so.
pub(super) fn syntax_target(text: &str) -> Option<&str> {
    let (mut from, mut retries) = (0, 0);
    while let Some(group) = balanced_object(text, from) {
        match group {
            Ok(range) if text[range.start + 1..].trim_start().starts_with('"') => {
                return Some(&text[range]);
            }
            Ok(range) => from = range.end,
            Err(start) if retries < UNCLOSED_RETRIES => {
                retries += 1;
                from = start + 1;
            }
            Err(_) => return None,
        }
    }
    None
}

/// The balanced `{…}` opening at the first `{` at or after byte `from` (braces inside JSON
/// strings ignored), as a byte range; `Err(start)` when that brace never closes, None when no
/// brace opens.
fn balanced_object(text: &str, from: usize) -> Option<Result<std::ops::Range<usize>, usize>> {
    let start = from + text.get(from..)?.find('{')?;
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
                    return Some(Ok(start..start + i + ch.len_utf8()));
                }
            }
            _ => {}
        }
    }
    Some(Err(start))
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

#[cfg(test)]
mod tests {
    use super::{Objects, UNCLOSED_RETRIES, answer_objects, first_json_object};
    use nika_kernel::ai::provider::{ContentBlock, InferResponse, StopReason, TokenUsage};

    const A: &str = r#"{"steps": [], "note": "a {brace} and a \" quote in a string"}"#;
    const B: &str = r#"{"steps": [{"op": "read"}]}"#;
    const EXAMPLE: &str = r#"{"status": "paid"}"#;

    /// The answer's own type, for these tests: an object that carries `steps`.
    fn is_plan(object: &str) -> bool {
        serde_json::from_str::<serde_json::Value>(object).is_ok_and(|v| v.get("steps").is_some())
    }

    fn read(text: &str) -> Option<&str> {
        match answer_objects(text, is_plan) {
            Objects::One { answer, .. } => Some(answer),
            _ => None,
        }
    }

    #[test]
    fn one_answer_is_read_through_prose_repetitions_templates_and_examples() {
        for text in [
            A.to_owned(),
            format!("Sure!\n```json\n{A}\n```"),
            // The same answer twice, bare or in prose, is one answer.
            format!("{A}\n{A}"),
            format!("Draft {A} final {A}"),
            // Template braces, an empty object and a closing prose brace are not answers.
            format!("{A}\nIt reads ${{{{ with.content }}}}, keeps permits: {{}}, ends {{name}}"),
            format!("It uses ${{{{ with.content }}}} before the answer:\n{A}"),
            // An example that cannot be an answer never kills the one answer.
            format!("For example {EXAMPLE}, then {A}"),
            format!("{A} then {{ an unclosed prose brace"),
        ] {
            assert_eq!(read(&text), Some(A), "{text}");
        }
        let beside_example = format!("E.g. {EXAMPLE}: {A}");
        let Objects::One { unread, .. } = answer_objects(&beside_example, is_plan) else {
            panic!("one answer beside an example");
        };
        assert_eq!(unread, [EXAMPLE], "kept by digest, never read");
        assert_eq!(
            first_json_object(&format!("Sure!\n```json\n{A}\n```")),
            Some(A)
        );
    }

    #[test]
    fn two_answers_or_one_beside_a_cut_object_are_never_resolved_by_reading_the_first() {
        for text in [
            format!("Draft {A} final {B}"),
            format!("{A}\n{B}"),
            format!("```json\n{B}\n```\n```json\n{A}\n```"),
            format!("{A} then {{ an unclosed brace, then {B}"),
        ] {
            assert!(
                matches!(answer_objects(&text, is_plan), Objects::Two(ref objects) if objects.len() == 2),
                "{text}"
            );
        }
        for text in [
            // A competitor that opens like an object and never closes, after or before.
            format!("Draft:\n{A}\nFinal:\n{{\"steps\": [{{\"op\": \"write\""),
            format!("Draft:\n{{ \"steps\": [\nFinal:\n{A}"),
            // Past the retry bound, the rest of the text is not judged: never one answer.
            format!("{A}{}", " {".repeat(UNCLOSED_RETRIES + 1)),
        ] {
            assert!(
                matches!(answer_objects(&text, is_plan), Objects::Undecided(ref objects) if objects == &[A]),
                "{text}"
            );
        }
        assert_eq!(
            read(&format!("{A}{}", " {".repeat(UNCLOSED_RETRIES))),
            Some(A)
        );
        // Without a complete object, the text keeps its syntax path.
        assert!(matches!(
            answer_objects("{\"steps\": !}", is_plan),
            Objects::None
        ));
        assert!(matches!(
            answer_objects("no object", is_plan),
            Objects::None
        ));
    }

    #[test]
    fn a_cold_plan_is_read_beside_an_example_and_never_beside_another_plan() {
        let plan = r#"{"steps":[],"effects":[],"obligations":[],"constraints":[],"unknowns":[]}"#;
        let other = r#"{"steps":[{"op":"draft","detail":"x","evidence":"x"}],"effects":[],"obligations":[],"constraints":[],"unknowns":[]}"#;
        let response = |text: String| {
            InferResponse::new(
                vec![ContentBlock::Text { text }],
                TokenUsage::new(1, 1),
                StopReason::EndTurn,
            )
        };
        let mut out = crate::initial();
        let two = response(format!("Plan A:\n{plan}\nPlan B:\n{other}"));
        assert!(super::proposal::decode(&two, &mut out).is_none());
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.message.contains("two plans")),
            "{out:#?}"
        );
        let mut out = crate::initial();
        let one = response(format!("Plan:\n{plan}\nFor example {EXAMPLE}."));
        assert!(super::proposal::decode(&one, &mut out).is_some());
    }

    /// An outcome whose receipt holds one call, as `call_with_schema` leaves it.
    fn called() -> crate::CompileOutcome {
        let mut out = crate::initial();
        let mut receipt = crate::AuthoringReceipt::new("mock/authoring".to_owned());
        receipt.context.push(serde_json::json!({"call": "plan"}));
        out.provenance.authoring = Some(receipt);
        out
    }

    #[test]
    fn a_competitor_with_a_defect_is_still_a_competitor_and_its_digest_is_kept() {
        use super::proposal::Proposal;
        let plan = r#"{"steps":[],"effects":[],"obligations":[],"constraints":[],"unknowns":[]}"#;
        // An unknown key or a null field keeps it from decoding, never from competing.
        for rival in [
            r#"{"steps":[{"op":"draft","detail":"x","evidence":"x"}],"confidence":0.9}"#,
            r#"{"steps":null,"effects":[]}"#,
        ] {
            assert!(
                super::answer_shaped::<Proposal>(rival, &["steps"]),
                "{rival}"
            );
            let text = format!("Draft:\n{plan}\nFinal:\n{rival}");
            assert!(
                matches!(answer_objects(&text, |o| super::answer_shaped::<Proposal>(o, &["steps"])), Objects::Two(ref o) if o == &[plan, rival]),
                "{text}"
            );
            let mut out = called();
            let response = InferResponse::new(
                vec![ContentBlock::Text { text }],
                TokenUsage::new(1, 1),
                StopReason::EndTurn,
            );
            assert!(super::proposal::decode(&response, &mut out).is_none());
            let call = &out.provenance.authoring.as_ref().unwrap().context[0];
            assert_eq!(call["competing_objects"], super::digests(&[plan, rival]));
        }
        // An object that carries none of a plan's keys and does not decode is an example.
        assert!(!super::answer_shaped::<Proposal>(EXAMPLE, &["steps"]));
    }

    #[test]
    fn the_syntax_path_judges_the_broken_answer_never_a_template() {
        let broken = r#"{"steps": !}"#;
        for text in [
            format!("It uses ${{{{ with.content }}}}, then {broken}"),
            format!("{{name}} {broken} {{{{ x }}}}"),
            broken.to_owned(),
        ] {
            assert_eq!(super::syntax_target(&text), Some(broken), "{text}");
        }
        for text in ["{{ a }} {name}", "{\"steps\": [", "no object"] {
            assert_eq!(super::syntax_target(text), None, "{text}");
        }
    }
}
