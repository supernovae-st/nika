// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One compiler, one loop of actions, and a probabilistic frontend before a deterministic
//! backend (R1 · A1: [`agenda`] picks the next action from what the request still lacks).
//!
//! The historical names stay those of the mechanisms. HOT: the reader consumed every clause
//! under the strict admission contract; with a judge selected its plan is CHECKED before READY.
//! WARM: finite admissible readings settled by a bounded decision seat that may answer NONE,
//! then checked. COLD: explicitly authorized proposals of a private semantic plan, never source
//! or permits, each accountable for every region of the request; a computation the plan's typed
//! stages cannot state sends it to the sketch door next, with no program round to exhaust first.
//! Wherever a native door is permitted the author composes the complete document instead (the
//! document door): the whole language, admitted components composed in, judged as any candidate.
//! Deterministic policy facts (prohibitions, gates, indecision, contradictions,
//! bounds, approval bypasses) are the floor under every proposal, and every
//! strategy ends in the same deterministic assembler and the same Check.
//! A permitted seat is not an obligation; a consumed clause is not understanding.
use super::{
    AuthoringCognition, AuthoringPolicy, AuthoringReceipt, CompileError, CompileOutcome,
    CompileRequest, DiagnosticKind, HotPolicy, NativeMode, QuestionType, Strategy,
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
use nika_compile_seats::compose::{self, Candidate};
use nika_compile_seats::foundry::ComponentCatalog;
use nika_kernel::ai::provider::{InferRequest, InferResponse, Message, ProviderInferDyn, Role};
use serde_json::{Value, json};

mod admitted;
mod agenda;
use agenda::Action;
/// The document door of a fresh CREATE: the complete document, admitted components composed in.
mod document_create;
mod instructions;
use instructions::INSTRUCTIONS;
mod backstops;
mod forensic;
pub(super) mod knowledge;
mod native;
pub use native::judge_document;
mod proposal;
pub(crate) mod receipt;
mod rehearsal;
/// The record of the reasoning a call reported, owned with the decision seats that ask for it
/// too (ADR-146).
pub(crate) use nika_compile_seats::reasoning::reasoning_record;
use receipt::call_with_schema;
mod sketch;
mod transform;
mod verify;
#[cfg(test)]
use nika_compile_seats::objects::UNCLOSED_RETRIES;
/// The JSON objects of a seat's text, owned with the seats since the ADR-146 descent.
pub(super) use nika_compile_seats::objects::{
    Objects, answer_objects, answer_shaped, digests, first_json_object, record_objects,
    syntax_target,
};
use proposal::{Composition, Merged, Proposal, decode, merged};
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
    rehearsals: &mut rehearsal::Rehearsals<'_>,
) -> Result<CompileOutcome, CompileError> {
    let deterministic = super::compile(request)?;
    let Input::Edit {
        change: EditChange::Text(_),
        ..
    } = &request.input
    else {
        return Ok(deterministic);
    };
    // A native revision's answer round: the core replayed its record, and the whole request it
    // finishes is judged in this round by the round's judge, or stays pending (R4 A11, step 2).
    let native = (request.plan.as_ref()).filter(|record| {
        record.get("strategy").and_then(Value::as_str) == Some(Strategy::Native.word())
    });
    if let (Some(record), Some(folded)) = (native, super::revise_intent(request))
        && deterministic.provenance.strategy == Some(Strategy::Native)
    {
        let provider = (request.authoring.as_ref())
            .filter(|policy| policy_bounded(policy))
            .zip(cognition.provider);
        let judges = (cognition.seat, provider);
        let round = super::initial();
        let replayed = verify::replayed(&folded, record, request, judges, false, rehearsals, round);
        return Box::pin(replayed).await;
    }
    let unresolved = deterministic
        .diagnostics
        .iter()
        .any(|d| d.target == "change_request");
    // A new change to the bytes a source revision wrote: its record is no answer round of this
    // change; it binds those bytes (the core checks it) and states the words they answer.
    let next_turn = (request.plan.as_ref()).is_some_and(|record| {
        !record["source_revision"].is_null()
            && super::revise_intent(request).is_none_or(|folded| {
                record["intent_sha256"].as_str()
                    != Some(nika_compile::intent_sha256(&folded).as_str())
            })
    });
    let (Some(policy), Some(provider)) = (&request.authoring, cognition.provider) else {
        return Ok(deterministic);
    };
    if !(unresolved || next_turn) || policy.native == NativeMode::Off {
        return Ok(deterministic);
    }
    let Some(folded) = super::revise_intent(request) else {
        return Ok(deterministic);
    };
    let mut out = super::initial();
    if unbounded(policy, &mut out) {
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
    // A change in words to a base no semantic record binds (a historical, native or manual
    // source) is kept as it is, never revised from source (R4 F); a record-bound base is revised
    // at the entry ([`semantic_replayed`]). One destination it writes may still be replaced in
    // place (the source-anchored revision); anything else keeps the base with its limitation.
    Box::pin(sketch::revise::source(
        request,
        policy,
        (provider, cognition.seat),
        rehearsals,
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
pub async fn compile_with_cognition<P: ProviderInferDyn>(
    request: &CompileRequest,
    cognition: Cognition<'_, P>,
) -> Result<CompileOutcome, CompileError> {
    compile_with_cognition_rehearsed(request, cognition, None).await
}

/// Compile with a host that can rehearse final candidates on copies of the observed world.
/// A rehearsal grants no authority. Failed or missing results cannot be Ready; a safe
/// refusal to rehearse is recorded distinctly. The existing entry offers no host and
/// keeps its source-only behavior. Native repairs stay within the authoring policy.
///
/// # Errors
/// Returns the same representation/registry machinery failures as [`super::compile`].
pub async fn compile_with_cognition_rehearsed<P: ProviderInferDyn>(
    request: &CompileRequest,
    cognition: Cognition<'_, P>,
    host: Option<&dyn crate::rehearse::Rehearse>,
) -> Result<CompileOutcome, CompileError> {
    compile_with_cognition_composed(request, cognition, host, None).await
}

/// [`compile_with_cognition_rehearsed`] with the admitted release the host lends as executable
/// components (`nika_onboard::knowledge::Snapshot`, reopened from the session's pin): a pack is
/// qualified over the whole catalogue, and a revision of a base no semantic record binds may
/// compose an admitted component (expanded, bound and witnessed) or rebind one it composed. The
/// catalogue grants no authority: every candidate still passes the strict parser, Check and the
/// round's judge, and the receipt names what was consulted and what was really expanded.
///
/// # Errors
/// Returns the same representation/registry machinery failures as [`super::compile`].
pub async fn compile_with_cognition_composed<P: ProviderInferDyn>(
    request: &CompileRequest,
    cognition: Cognition<'_, P>,
    host: Option<&dyn crate::rehearse::Rehearse>,
    catalog: Option<&dyn ComponentCatalog>,
) -> Result<CompileOutcome, CompileError> {
    // The caller's own request, read before money or a clarification changes it (slice C).
    let caller = match nika_compile::surface::semantic::caller(request) {
        Ok(caller) => caller,
        Err(refused) => return Ok(*refused),
    };
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
    let mut rehearsals = rehearsal::Rehearsals::new(host)
        .serving(rehearsal::Serves {
            caller: caller.clone(),
            raw: request.clone(),
            reading: reading.clone(),
        })
        .lending(catalog);
    let mut out = if request
        .plan
        .as_ref()
        .is_some_and(|r| r.get("semantic_record").is_some())
    {
        Box::pin(semantic_replayed(request, &reading, seats, &mut rehearsals)).await?
    } else {
        compile_inner(&reading, seats, &mut rehearsals).await?
    };
    sketch::bind_caller(caller, request, &mut out);
    rehearsals.finish(&reading, &mut out).await;
    // A created document settled with nothing left to ask is bound to its final bytes.
    document_create::bind(&mut out);
    if let Some(money) = record {
        admitted::record(
            request,
            money,
            closed.as_deref().filter(|_| offered),
            &mut out,
        );
    }
    nika_compile::surface::observed::record(request, &mut out);
    forensic::record(request, offered, &mut out);
    Ok(out)
}

/// An answer round of a semantic record (slice C), before every shortcut: the request the
/// door read is derived from `reading` as `compile_inner` derives it (a clarification taken,
/// folded), only to bind this round's judge; the record replays from the raw request alone.
async fn semantic_replayed<P: ProviderInferDyn>(
    raw: &CompileRequest,
    reading: &CompileRequest,
    cognition: Cognition<'_, P>,
    rehearsals: &mut rehearsal::Rehearsals<'_>,
) -> Result<CompileOutcome, CompileError> {
    let Input::Create(words) = &reading.input else {
        // A revision of a base its record binds (R4 F): the core, then the semantic revision.
        let seat = (reading.authoring.as_ref())
            .filter(|p| super::revise_intent(reading).is_some() && policy_bounded(p))
            .zip(cognition.provider);
        return Box::pin(sketch::revise::edit(
            raw,
            reading,
            seat,
            cognition.seat,
            rehearsals,
        ))
        .await;
    };
    let mut assembly = reading.clone();
    let clarified = (assembly.answers.remove("intent.clarification"))
        .and_then(|raw| serde_json::from_str::<String>(&raw).ok());
    let intent = lexicon::fold_apostrophes(clarified.as_deref().unwrap_or(words));
    let provider = (reading.authoring.as_ref())
        .filter(|policy| policy_bounded(policy))
        .zip(cognition.provider);
    let judges = (cognition.seat, provider);
    verify::semantic((raw, &intent, &assembly), judges, rehearsals).await
}

async fn compile_inner<P: ProviderInferDyn>(
    request: &CompileRequest,
    cognition: Cognition<'_, P>,
    rehearsals: &mut rehearsal::Rehearsals<'_>,
) -> Result<CompileOutcome, CompileError> {
    let Input::Create(intent) = &request.input else {
        return revise(request, cognition, rehearsals).await;
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
        (cognition, rehearsals),
        out,
    ))
    .await
}

async fn resolve_create<P: ProviderInferDyn>(
    intent: &str,
    request: &CompileRequest,
    assembly_request: &CompileRequest,
    (cognition, rehearsals): (Cognition<'_, P>, &mut rehearsal::Rehearsals<'_>),
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
        if record.get("pending_transform").is_some() {
            replay(intent, record, assembly_request, &mut out)?;
            if let (Some(policy), Some(provider)) = (&request.authoring, cognition.provider) {
                if unbounded(policy, &mut out) {
                    return Ok(out);
                }
                let seats = (policy, provider, cognition.seat);
                return transform::resume(intent, assembly_request, seats, out).await;
            }
            return Ok(out);
        }
        // The remainder a record leaves unverified is judged in this round, or named (R4 A11).
        let provider = (request.authoring.as_ref())
            .filter(|policy| policy_bounded(policy))
            .zip(cognition.provider);
        let judges = (cognition.seat, provider);
        // Every plan but the reader's own HOT plan (a model's COLD or WARM plan, a record with no
        // strategy word or an unknown one) is READY only on a judgment of the whole request over
        // the bytes this round replays (R4 A11): no judgment a record carries counts, and an
        // answer changes the bytes. Fail closed.
        let strategy = record.get("strategy").and_then(Value::as_str);
        let pending = strategy != Some(Strategy::Hot.word());
        let request = assembly_request;
        return verify::replayed(intent, record, request, judges, pending, rehearsals, out).await;
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
    let seats = (cognition, rehearsals);
    route_create(intent, request, assembly_request, seats, reading, out).await
}

async fn route_create<P: ProviderInferDyn>(
    intent: &str,
    request: &CompileRequest,
    assembly_request: &CompileRequest,
    (cognition, rehearsals): (Cognition<'_, P>, &mut rehearsal::Rehearsals<'_>),
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
        let read_as = assembly_request.clone().with_replaced_input(intent);
        return super::compile(&read_as);
    }
    let mut route = Vec::new();
    // What the request still lacks after the reader decides the next action (R1 · A1). The
    // deterministic door judges the reading with its stated rules promoted; the reading itself
    // keeps its constraints, the policy floor a seat's proposal inherits. The sketch and only
    // policies name their composer (the sketch door, the document door) before the reader is
    // admitted.
    let selected = agenda::Selected::of(
        request,
        cognition.provider.is_some(),
        cognition.seat.is_some(),
    );
    let mut missing = agenda::Missing {
        untyped: reading.unresolved.len() + reading.soft_constraints.len(),
        ..agenda::Missing::default()
    };
    let mut admitted = reading.clone();
    super::shape::promote_stated_rules(&mut admitted.plan, intent);
    let explicit = matches!(selected.native, NativeMode::Sketch | NativeMode::Only);
    if !(selected.author && explicit) {
        match admit_hot(intent, &admitted, request.hot) {
            Ok(()) => {
                route.push("hot".to_owned());
                missing.composed = true;
            }
            Err(why) => route.push(format!("hot rejected: {}", why.reasons().join("; "))),
        }
        missing.choices = reading.unresolved.is_empty()
            && !reading.ambiguous.is_empty()
            && request.hot != HotPolicy::Off
            && lexical_rest_is_explicit(intent, &admitted);
    }
    let action = agenda::next(missing, selected);
    agenda::record(&mut out, missing, action);
    let seats = (cognition, rehearsals);
    match action {
        Action::Assemble => {
            record_route(&mut out, &route);
            settle(Strategy::Hot, &admitted.plan, intent, assembly_request, out)
        }
        Action::Check => {
            route.push(agenda::READER_CHECKED.to_owned());
            let plan = admitted.plan;
            let checked = check(
                intent,
                plan,
                assembly_request,
                seats.0,
                &reading,
                route,
                out,
            );
            Box::pin(checked).await
        }
        Action::Settle => {
            let state = (missing, selected);
            let settled =
                choose_create(intent, assembly_request, seats, reading, state, route, out);
            Box::pin(settled).await
        }
        _ => {
            let composed = compose(action, intent, assembly_request, seats, reading, route, out);
            Box::pin(composed).await
        }
    }
}

/// The Check action: a judge reads a whole plan before it can be READY (R1). The author, when
/// selected, repairs what the judge finds missing (the COLD verifier's loop, `strategy: cold`);
/// a decision seat alone judges and holds (WARM's verifier, `strategy: warm`).
async fn check<P: ProviderInferDyn>(
    intent: &str,
    plan: Plan,
    request: &CompileRequest,
    cognition: Cognition<'_, P>,
    reading: &Reading,
    route: Vec<String>,
    mut out: CompileOutcome,
) -> Result<CompileOutcome, CompileError> {
    record_route(&mut out, &route);
    if let (Some(policy), Some(provider)) = (&request.authoring, cognition.provider) {
        if unbounded(policy, &mut out) {
            return Ok(out);
        }
        out.provenance.cognition = AuthoringCognition::ExplicitProvider;
        return judged(
            intent,
            plan,
            policy,
            provider,
            cognition.seat,
            reading,
            request,
            out,
        )
        .await;
    }
    let Some(seat) = cognition.seat else {
        return settle(Strategy::Hot, &plan, intent, request, out);
    };
    out.provenance.cognition = AuthoringCognition::ExplicitDecision;
    Box::pin(verify::judged_warm(intent, &plan, seat, request, out)).await
}

/// A plan's computations through the transform seat, then the COLD verifier: the judge reads the
/// assembled candidate and the author repairs a concrete defect.
#[allow(clippy::too_many_arguments)] // the COLD verifier's own state, threaded once
async fn judged<P: ProviderInferDyn>(
    intent: &str,
    mut plan: Plan,
    policy: &AuthoringPolicy,
    provider: &P,
    seat: Option<&dyn DecisionSeat>,
    reading: &Reading,
    request: &CompileRequest,
    mut out: CompileOutcome,
) -> Result<CompileOutcome, CompileError> {
    // A computation the typed stages could not state asks the seat for a verified program
    // (treatment B), once, on the plan that will be assembled: the seat's own example is the
    // test, the runtime's jq the judge, the receipt counts the call.
    if let Some(mut pending) =
        transform::synthesize(intent, &mut plan, policy, provider, request, &[], &mut out).await
    {
        pending.answer(request, &mut out);
        pending.suspend(&plan, &mut out);
        return Ok(out);
    }
    Box::pin(verify::judged_cold(
        intent, plan, policy, provider, seat, reading, request, out,
    ))
    .await
}

/// The author composes (A1): the attached knowledge qualified by the decision seat first, then
/// the document door, the sketch door or the private plan as the action names. The qualification
/// record and what the candidate kept of the shown references ride the outcome, with the
/// expansions the document door's receipts name witnessed on the candidate.
async fn compose<P: ProviderInferDyn>(
    action: Action,
    intent: &str,
    request: &CompileRequest,
    (cognition, rehearsals): (Cognition<'_, P>, &mut rehearsal::Rehearsals<'_>),
    reading: Reading,
    mut route: Vec<String>,
    mut out: CompileOutcome,
) -> Result<CompileOutcome, CompileError> {
    let (Some(policy), Some(provider), false) = (
        &request.authoring,
        cognition.provider,
        action == Action::Ask,
    ) else {
        route.push("needs cognition".to_owned());
        record_route(&mut out, &route);
        unresolved(&reading, &mut out);
        return Ok(out);
    };
    if unbounded(policy, &mut out) {
        return Ok(out);
    }
    // The pack the request carries, qualified over the whole catalogue the host lent (none: the
    // pack alone, as before): every admitted entry the pack lacks is asked by its descriptor, and
    // so is every recalled entry the catalogue lists, read whole unless the seat discards it. The
    // host lends the catalogue of its request, so a held-out corpus stays out (measured: same
    // questions, about 69% fewer bytes to the seat on the r2 candidate).
    let qualified =
        knowledge::qualified_with(intent, request, cognition.seat, rehearsals.catalog()).await;
    let shown = qualified
        .as_ref()
        .map_or(request, |(qualified, _)| qualified);
    let seats = (provider, cognition.seat, rehearsals);
    // Boxed: the seat doors are rare and large; they must not grow every compile future.
    let mut done = match action {
        Action::Document => {
            route.push(document_create::ROUTE.to_owned());
            let door = document_create::author(intent, &reading, policy, seats, shown, route, out);
            Box::pin(door).await?
        }
        Action::Sketch => {
            route.push(forensic::NATIVE_SKETCH.to_owned());
            let door = sketch::author(intent, &reading, policy, seats, shown, route, out);
            Box::pin(door).await?
        }
        _ => {
            let door = author_create(intent, policy, seats, shown, reading, route, out);
            Box::pin(door).await?
        }
    };
    if let Some((qualified, record)) = qualified {
        if action == Action::Document {
            let receipts = document_create::receipts(&done);
            knowledge::reused(&qualified, record, &receipts, &mut done);
        } else {
            knowledge::traced(&qualified, record, &mut done);
        }
    }
    Ok(done)
}

/// The Settle action (WARM's mechanism): every clause is known, a few carry a finite set of
/// readings and the rest is strictly explicit; the decision seat settles each. A settled plan is
/// checked (the author, when selected, repairs it); readings the seat leaves open go back to the
/// loop, which hands them to a composer.
async fn choose_create<P: ProviderInferDyn>(
    intent: &str,
    request: &CompileRequest,
    (cognition, rehearsals): (Cognition<'_, P>, &mut rehearsal::Rehearsals<'_>),
    mut reading: Reading,
    (mut missing, selected): (agenda::Missing, agenda::Selected),
    mut route: Vec<String>,
    mut out: CompileOutcome,
) -> Result<CompileOutcome, CompileError> {
    if let Some(seat) = cognition.seat {
        out.provenance.cognition = AuthoringCognition::ExplicitDecision;
        let (records, settled_all, refused) =
            settle_readings(intent, seat, &mut reading, &mut out).await;
        let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
        decision["seat"] = json!(seat.name());
        decision["questions"] = json!(records);
        out.provenance.decision = Some(decision);
        if settled_all {
            route.push("warm".to_owned());
            if let (Some(policy), Some(provider)) = (&request.authoring, cognition.provider)
                && policy_bounded(policy)
            {
                route.push(agenda::SETTLED_CHECKED.to_owned());
                record_route(&mut out, &route);
                let plan = reading.plan.clone();
                let checked = judged(
                    intent,
                    plan,
                    policy,
                    provider,
                    Some(seat),
                    &reading,
                    request,
                    out,
                );
                return Box::pin(checked).await;
            }
            record_route(&mut out, &route);
            let judged = verify::judged_warm(intent, &reading.plan, seat, request, out);
            return Box::pin(judged).await;
        }
        route.push(
            if refused {
                "warm: refused"
            } else {
                "warm: none"
            }
            .to_owned(),
        );
        reading.ambiguous.clear();
    }
    missing.settled = true;
    missing.untyped = reading.unresolved.len() + reading.soft_constraints.len();
    let action = agenda::next(missing, selected);
    agenda::record(&mut out, missing, action);
    compose(
        action,
        intent,
        request,
        (cognition, rehearsals),
        reading,
        route,
        out,
    )
    .await
}

/// Each open reading put to the seat as one closed choice: the records, whether every reading
/// settled, and whether the compiler refused a settlement ([`refused_search`]).
async fn settle_readings(
    intent: &str,
    seat: &dyn DecisionSeat,
    reading: &mut Reading,
    out: &mut CompileOutcome,
) -> (Vec<Value>, bool, bool) {
    let (mut records, mut settled_all, mut refused) = (Vec::new(), true, false);
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
            Ok(answer) => super::decide::admit(&question, answer).map(|()| answer.choice.clone()),
            Err(error) => Err(error.clone()),
        };
        records.push(match (&answer, &admitted) {
            (Ok(answer), Ok(_)) => super::decide::record(&question, Ok(answer)),
            (_, Err(error)) | (Err(error), _) => super::decide::record(&question, Err(error)),
        });
        match admitted {
            Ok(choice) if choice != NONE_OPTION => match Op::parse(&choice) {
                Some(op) if refused_search(op, ambiguity, &mut reading.unresolved, out) => {
                    (settled_all, refused) = (false, true);
                }
                Some(op) => reading.plan.push_step(Step::new(
                    op,
                    ambiguity.clause.clone(),
                    ambiguity.detail.clone(),
                    Vec::new(),
                )),
                None => {}
            },
            Ok(_) => {
                settled_all = false;
                reading.unresolved.push(ambiguity.clause.clone());
            }
            Err(error) => {
                settled_all = false;
                reading.unresolved.push(ambiguity.clause.clone());
                super::finding(out, DiagnosticKind::Unknown, "decision_seat", error.0);
            }
        }
    }
    (records, settled_all, refused)
}

/// A seat's `search` cannot apply an identifier its clause names in one structured file: no
/// stated term binds the search and its grep matches substrings (`W-5` in `W-50`), while a
/// `lookup` selects that one record. The compiler, never the seat, refuses that settlement:
/// the seat's answer stays on record, the clause stays unresolved under the compiler's name.
fn refused_search(
    op: Op,
    ambiguity: &lexicon::Ambiguity,
    unresolved: &mut Vec<String>,
    out: &mut CompileOutcome,
) -> bool {
    let detail = &ambiguity.detail;
    let literals = crate::paths::literals(detail);
    let [crate::paths::PathShape::File(path)] = literals.as_slice() else {
        return false;
    };
    let Some(id) = crate::shape::identifier(detail).filter(|_| op == Op::Search) else {
        return false;
    };
    if crate::paths::Structured::of(path).is_none() {
        return false;
    }
    unresolved.push(ambiguity.clause.clone());
    super::finding(
        out,
        DiagnosticKind::Unknown,
        "retrieval_choice",
        format!(
            "The decision seat chose `search` for « {detail} », but a search cannot select the record `{id}` of `{path}` exactly; the clause stays unresolved."
        ),
    );
    true
}

/// The Plan action (COLD's mechanism): explicitly authorized generative proposals of a private
/// plan, constrained by the deterministic facts. The attached knowledge is context the plan reads
/// (`knowledge::plan_context`), never a route of its own.
async fn author_create<P: ProviderInferDyn>(
    intent: &str,
    policy: &AuthoringPolicy,
    (provider, seat, rehearsals): (
        &P,
        Option<&dyn DecisionSeat>,
        &mut rehearsal::Rehearsals<'_>,
    ),
    assembly_request: &CompileRequest,
    reading: Reading,
    mut route: Vec<String>,
    out: CompileOutcome,
) -> Result<CompileOutcome, CompileError> {
    route.push(format!("cold: {} sample(s)", policy.samples));
    let mut found: Option<Composition> = None;
    let cold = sampled(
        intent,
        policy,
        provider,
        seat,
        &reading,
        assembly_request,
        route.clone(),
        out,
        &mut found,
    )
    .await?;
    Box::pin(after_cold(
        intent,
        &reading,
        policy,
        (provider, seat, rehearsals),
        assembly_request,
        route,
        cold,
        found,
    ))
    .await
}

/// What follows a COLD round: a pending transform waits for its answer; branches the plan could not
/// keep apart go to the sketch door that represents them, with the same request, answers, floor
/// and receipt; under Escalate, a round that ends without a candidate or hands the human a
/// machine's problem escalates to the same sketch door (the private plan is not the language's
/// ceiling), never to source generation; otherwise the round's own outcome.
#[allow(clippy::too_many_arguments)] // the cold round's inputs, its outcome and its composition
async fn after_cold<P: ProviderInferDyn>(
    intent: &str,
    reading: &Reading,
    policy: &AuthoringPolicy,
    (provider, decision, rehearsals): (
        &P,
        Option<&dyn DecisionSeat>,
        &mut rehearsal::Rehearsals<'_>,
    ),
    assembly_request: &CompileRequest,
    route: Vec<String>,
    cold: CompileOutcome,
    found: Option<Composition>,
) -> Result<CompileOutcome, CompileError> {
    let pending = cold
        .provenance
        .plan
        .as_ref()
        .is_some_and(|record| record.get("pending_transform").is_some());
    if pending {
        return Ok(cold);
    }
    let why = match found {
        Some(composition) if cold.candidate.is_none() && cold.questions.is_empty() => {
            sketch::Escalation::Composition(composition)
        }
        _ if policy.native == NativeMode::Escalate && native::escalates(&cold) => {
            sketch::Escalation::Plan
        }
        _ => return Ok(cold),
    };
    let request = assembly_request;
    Box::pin(sketch::compose(
        intent,
        reading,
        policy,
        (provider, decision, rehearsals),
        request,
        route,
        cold,
        &why,
    ))
    .await
}

const POLICY_BOUNDS: &str = "Authoring requires an explicit model, a positive sample count, a positive output-token limit (an initial one within it) and a positive timeout.";

/// Whether `policy` fails the bounds every seat call honors ([`policy_bounded`]), said in `out`.
fn unbounded(policy: &AuthoringPolicy, out: &mut CompileOutcome) -> bool {
    let unbounded = !policy_bounded(policy);
    if unbounded {
        super::finding(
            out,
            DiagnosticKind::Missed,
            "authoring_policy",
            POLICY_BOUNDS,
        );
    }
    unbounded
}

/// The bounds every seat call honors: an explicit model, a positive answer limit and a positive
/// wait. What a route can hold (its output cap, its context, its deadline) is its own technical
/// limit, the host's and the provider's to answer, never a compiler ceiling on the request.
fn policy_bounded(policy: &AuthoringPolicy) -> bool {
    !policy.model.trim().is_empty()
        && policy.samples > 0
        && policy.max_tokens > 0
        && policy
            .initial_max_tokens
            .is_none_or(|initial| (1..=policy.max_tokens).contains(&initial))
        && !policy.timeout.is_zero()
}

fn settle(
    strategy: Strategy,
    plan: &Plan,
    intent: &str,
    request: &CompileRequest,
    out: CompileOutcome,
) -> Result<CompileOutcome, CompileError> {
    settle_judged(strategy, plan, intent, request, &[], out)
}

/// The deterministic assembly of a plan under the judgments a judge's seat made in this compile
/// (R4 A11): a plan a model shaped (WARM, COLD) is held to the whole request too, so no single
/// candidate of it is READY before its judgment.
fn settle_judged(
    strategy: Strategy,
    plan: &Plan,
    intent: &str,
    request: &CompileRequest,
    judgments: &[nika_compile::surface::Judgment],
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
    // The reader's own HOT plan assembles as it always did; a plan a model shaped waits for the
    // judgment of the whole request as well.
    if strategy == Strategy::Hot {
        super::assemble::assemble(&plan, intent, request, &mut out)?;
    } else {
        nika_compile::surface::assemble::assemble_judged(
            &plan, intent, request, judgments, true, &mut out,
        )?;
        proposal::told(intent, &plan, &mut out);
    }
    record_retrieval(&mut out, intent, Some(&plan));
    out.provenance.strategy = Some(strategy);
    out.provenance.plan = Some(plan_record(&plan, Some(strategy)));
    Ok(out)
}

/// The messages of the opening authoring call: the instructions followed by the context the
/// compiler was given ([`knowledge::plan_context`]), then the request alone as the user's words.
fn opening(intent: &str, context: &knowledge::PlanContext) -> Vec<Message> {
    vec![
        Message::text(Role::System, format!("{INSTRUCTIONS}\n\n{}", context.text)),
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
/// proposal to the merge, which refuses it as before; the failure stays recorded. Both calls
/// carry the same opening, its context and its stamp: the repair adds only the answer and the
/// counterexample.
async fn propose<P: ProviderInferDyn>(
    intent: &str,
    policy: &AuthoringPolicy,
    provider: &P,
    context: &knowledge::PlanContext,
    out: &mut CompileOutcome,
) -> Option<Proposal> {
    let opened = opening(intent, context);
    let before = receipt::journaled(out);
    let called = call(policy, provider, "plan", opened.clone(), out).await;
    knowledge::stamp_plan(out, before, context, &[]);
    let (proposal, text) = called?;
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
    let mut messages = opened;
    messages.push(Message::text(Role::Assistant, text));
    messages.push(Message::text(Role::User, counterexample(&defect)));
    let before = receipt::journaled(out);
    let called = call(policy, provider, "repair", messages, out).await;
    knowledge::stamp_plan(out, before, context, &[]);
    match called {
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
    let text = crate::decide::answer_text(&response)
        .unwrap_or_default()
        .to_owned();
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
    found: &mut Option<Composition>,
) -> Result<CompileOutcome, CompileError> {
    let mut accepted: Vec<(usize, Plan)> = Vec::new();
    let mut rejected: Vec<CompileOutcome> = Vec::new();
    let mut records = Vec::new();
    let mut calls = 0;
    let mut input_tokens: Option<u64> = None;
    let mut output_tokens: Option<u64> = None;
    let mut elapsed_ms = 0;
    let mut context: Vec<Value> = Vec::new();
    // One context for every sample: the same request, reading and attachments.
    let prepared = knowledge::plan_context(intent, reading, request);
    for index in 0..policy.samples as usize {
        let mut scratch = super::initial();
        let proposal = propose(intent, policy, provider, &prepared, &mut scratch).await;
        let merged = proposal.map(|p| merged(intent, p, reading, &mut scratch));
        let needs_sketch = matches!(merged, Some(Merged::NeedsSketch(_)));
        let plan = match merged {
            Some(Merged::NeedsSketch(composition)) => {
                found.get_or_insert(composition);
                None
            }
            other => other.and_then(Merged::into_plan),
        };
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
        let authority_spent = scratch
            .provenance
            .authoring
            .as_ref()
            .is_some_and(|receipt| {
                receipt
                    .context
                    .iter()
                    .any(|call| call["result"]["failure_kind"] == "admission_refused")
            });
        records.push(json!({
            "sample": index,
            "calls": scratch.provenance.authoring.as_ref().map_or(0, |r| r.calls),
            "accepted": plan.is_some(),
            "needs_sketch": needs_sketch,
            "signature": plan.as_ref().map(compose::signature),
            "findings": findings,
        }));
        match plan {
            Some(plan) => accepted.push((index, plan)),
            // A lawful composition is not a refusal: its findings are not reported as one.
            None if needs_sketch => {}
            None => rejected.push(scratch),
        }
        if authority_spent {
            break;
        }
    }
    // Every call beyond one per sample is a repair: the route says how many were bought.
    let repairs = calls.saturating_sub(u32::try_from(records.len()).unwrap_or(u32::MAX));
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
        "cap": null,
        "pattern_dimensions": composition.dimensions.iter().map(compose::Dimension::to_json).collect::<Vec<_>>(),
    });
    if let Some(record) = warm_record {
        decision["warm_after_cold"] = record;
    }
    decision["route"] = json!(route);
    out.provenance.decision = Some(decision);
    if let Some(candidate) = selected {
        let plan = candidate.plan.clone();
        // The plan's own observed limit (R5): a computation its typed stages cannot state. Where
        // the sketch door may serve, it composes the request next, the program one typed fill,
        // rather than a separate program round whose refusal would escalate anyway.
        let sketch = policy.native == NativeMode::Escalate && policy.repairs != Some(0);
        if sketch && transform::unstated(intent, &plan, request) {
            route.push(agenda::PLAN_LIMIT.to_owned());
            record_route(&mut out, &route);
            out.provenance.plan = Some(plan_record(&plan, Some(Strategy::Cold)));
            return Ok(out);
        }
        return judged(intent, plan, policy, provider, seat, reading, request, out).await;
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
mod retrieval_choice_tests;
#[cfg(test)]
mod tests;
