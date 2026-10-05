// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The semantic revision (R4 F): a change in words to a base its semantic record binds (the
//! core checked the pair) goes through the sketch door, never through source authoring. A
//! revision keeps the base graph as its record holds it (`sketch::revision::preserved`); the seat
//! reads that graph and its fills as data beside the change and the original request read from
//! the record itself (never restated words), states which original clauses the change
//! supersedes, each linked to a clause of the change. From then on the revision consumes the
//! resolved request (`sketch::revision::resolved`: the original words, each linked clause
//! replaced in place, then the change's additions): the fills, the laws, the record's basis and
//! the judge read it, so a superseded clause is no obligation any more, an added one is, and
//! every other one is read as before; an added effect or gate is refused before any fill. The
//! obligation delta must hold (`sketch::revision::delta`), or nothing is READY and why is named.
//! The base record is never rewritten: the revision's own semantic record binds the revised bytes
//! under the resolved words, the next revision's original; the delta is recorded apart
//! (`decision.revision`). A change to the graph's structure is not revised here, and a base no
//! semantic record binds is kept as it is ([`self::historical`]).

use super::{
    Answer, Question, SKETCH, SketchAnswer, Talk, cold, conclude, fill, floor_refuses, graph,
    native, prelude, semantic_record, system_message, withhold_record,
};
use crate::cognition::{AuthoringPolicy, CompileOutcome, CompileRequest, DiagnosticKind, Strategy};
use crate::sketch::{Sketch, revision};
use crate::types::{EditChange, Input};
use crate::{CompileError, lexicon};
use nika_kernel::ai::provider::ProviderInferDyn;
use serde_json::{Value, json};

/// The seat's first revision answer: the original clauses the change supersedes, each
/// `{"replaces": <original clause>, "by": <change clause>}`, and the change clauses it adds.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Links {
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    supersedes: Vec<Value>,
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    adds: Vec<String>,
    /// For a destination the change adds: the destination the base writes whose write it copies.
    #[serde(default)]
    like: Option<String>,
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    notes: String,
}

impl Links {
    /// The typed delta the seat stated: `{"supersedes", "adds", "like"}`.
    fn stated(&self) -> Value {
        json!({"supersedes": self.supersedes, "adds": self.adds, "like": self.like})
    }
}

impl native::Shaped for Links {
    const KEYS: &'static [&'static str] = &["supersedes", "adds"];
}

/// The route a semantic revision records.
pub(in crate::cognition) const ROUTE: &str = "edit: semantic revision through the sketch door";

/// The route a source-anchored revision records (one destination replaced in place).
pub(in crate::cognition) const SOURCE_ROUTE: &str =
    "edit: source-anchored revision of one destination";

const REVISE_SOURCE: &str = "This is a REVISION of the base workflow below (`base_candidate`), which no semantic record binds. The compiler replaces ONE destination it writes, or adds ONE beside the existing ones, in place, and changes nothing else; you never write the workflow. Your only call: in `supersedes`, name the clause of the original request that states the destination the change replaces (copied exactly from `original_clauses`) and the clause of the change that states its new destination (copied exactly from `change_clauses`) — or no link when the change adds a destination; in `adds`, every other clause of the change (copied exactly from `change_clauses`), each adding or restating a duty without replacing any. Every change clause is in exactly one of the two. When the change adds a destination, `like` names the destination of `base_destinations` whose written content the new one receives. Name only what the request and the change state; when they leave it open, omit it and the human is asked.";

const REVISE: &str = "This is a REVISION of the base program below (`base_graph`, `base_fills`). Its graph stays exactly as it is: the change only changes what its tasks do through their typed holes. Call 1: in `supersedes`, name each clause of the original request the change replaces (copied exactly from `original_clauses`) and the clause of the change that replaces it (copied exactly from `change_clauses`); in `adds`, name each clause of the change (copied exactly from `change_clauses`) that adds a duty beside the original ones and replaces none. Every change clause is in exactly one of the two; every other original clause stays. Call 2: fill the base graph's holes for the revised request.";

/// An EDIT of a base its semantic record binds, at the entry: the core first (the pair checked,
/// the zero-call constant door, the record bound anew), then, only for a change in words the
/// core leaves unresolved and a seat the policy admits, the semantic revision. `raw` is the
/// caller's request, `reading` the one the seats read (money blanked).
///
/// # Errors
/// The core's machinery failures.
pub(in crate::cognition) async fn edit<P: ProviderInferDyn>(
    raw: &CompileRequest,
    reading: &CompileRequest,
    seat: Option<(&AuthoringPolicy, &P)>,
) -> Result<CompileOutcome, CompileError> {
    let core = nika_compile::compile(raw)?;
    let unresolved = core
        .diagnostics
        .iter()
        .any(|d| d.target == "change_request");
    let open = |(policy, _): &(&AuthoringPolicy, &P)| {
        unresolved && policy.native != crate::cognition::NativeMode::Off
    };
    match seat.filter(open) {
        Some((policy, provider)) => revise(reading, policy, provider).await,
        None => Ok(core),
    }
}

/// The base pair as the fills phase reads it: the record's graph and its settled questions,
/// gaps and notes; `None` when the record's graph does not decode.
fn base_pair(base: &Value) -> Option<(Sketch, SketchAnswer)> {
    let sketch = Sketch::from_json(&base["sketch"]).ok()?;
    let questions: Vec<Question> =
        serde_json::from_value(base["settlement"]["questions"].clone()).ok()?;
    let answer = SketchAnswer {
        name: base["sketch"]["name"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        tasks: base["sketch"]["tasks"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
        outputs: base["sketch"].get("outputs").cloned(),
        questions,
        gaps: serde_json::from_value(base["settlement"]["gaps"].clone()).unwrap_or_default(),
        notes: String::new(),
    };
    Some((sketch, answer))
}

/// The request the links resolve, read; then, when the seat answered the links, the fills of the
/// base graph against it. `Err` why a link does not resolve: no fill call is spent then.
async fn resolve_and_fill<P: ProviderInferDyn>(
    talk: &mut Talk,
    out: &mut CompileOutcome,
    linked: Option<&(Links, String)>,
    (original, intent, change_ledger): (&str, &str, &Value),
    stated: &Value,
    (policy, provider): (&AuthoringPolicy, &P),
    pair: &(Sketch, SketchAnswer),
) -> Result<(String, lexicon::Reading, Option<(Answer, Vec<Value>)>), Vec<String>> {
    // The contract the revision consumes from here on: the original words with each linked
    // clause replaced in place and every addition beside (`revision::resolved`), never the
    // original beside the whole change.
    let resolved = match linked {
        Some(_) => {
            let added = revision::additions(change_ledger, stated)?;
            let read = revision::resolved(original, &stated["supersedes"], &added)?;
            lexicon::fold_apostrophes(&read)
        }
        None => intent.to_owned(),
    };
    let reading = lexicon::read(&resolved);
    let Some((links, text)) = linked else {
        return Ok((resolved, reading, None));
    };
    // The seat's own notes are journaled by digest and shape only, never as text.
    let notes = crate::cognition::receipt::withheld(&links.notes, &[], "revision notes");
    talk.rounds.push(json!({"round": 0, "phase": "revision",
        "supersedes": links.supersedes, "adds": links.adds, "notes": notes}));
    let said = nika_kernel::ai::provider::Message::text(
        nika_kernel::ai::provider::Role::Assistant,
        text.clone(),
    );
    talk.messages.push(said);
    let accepted = fill(talk, &resolved, &reading, policy, provider, out, pair, 1).await;
    Ok((resolved, reading, accepted))
}

/// What a revision consumes and is judged against: the original words, the change, the resolved
/// request, both ledgers and the stated links.
struct Contract<'a> {
    original: &'a str,
    change: &'a str,
    resolved: &'a str,
    base_ledger: &'a Value,
    change_ledger: &'a Value,
    stated: &'a Value,
}

/// The talk a revision opens: the base graph, fills and both clause lists beside the request.
fn opened<'a>(
    base: &Value,
    intent: &str,
    reading: &lexicon::Reading,
    revising: &'a CompileRequest,
    ledgers: (&Value, &Value),
) -> (Talk, Vec<Value>, Option<(&'a str, &'a str)>) {
    let native::Prelude {
        references,
        callables,
        sent,
        revision: shown,
        mut opening,
        allowed,
    } = prelude(intent, reading, revising);
    opening["base_graph"] = base["sketch"].clone();
    opening["base_fills"] = base["fills"].clone();
    opening["original_clauses"] = json!(revision::clauses(ledgers.0));
    opening["change_clauses"] = json!(revision::clauses(ledgers.1));
    let mut system = system_message(&references, &callables);
    for part in ["\n\n", SKETCH, "\n\n", REVISE] {
        system.push_str(part);
    }
    // The opening is the machine-readable facts alone (hosts read it); what to answer is the
    // system's to say.
    let first = opening.to_string();
    let mut talk = Talk::open(system, first, vec![ROUTE.to_owned()], allowed, revising);
    talk.presented = json!(sent);
    (talk, sent, shown)
}

/// The revised record bound to the resolved words and judged by the preservation and delta laws:
/// the delta recorded (`decision.revision`), or the revision refused.
fn bind(
    out: &mut CompileOutcome,
    base: &Value,
    revising: &CompileRequest,
    graph_of: &Value,
    fills: &[Value],
    contract: &Contract<'_>,
) {
    let Some(settled) = out.provenance.plan.take() else {
        return;
    };
    let basis = nika_compile::surface::semantic::request_basis(contract.resolved, revising);
    out.provenance.plan = semantic_record(
        basis,
        graph_of,
        fills,
        &settled,
        out,
        revising.knowledge.as_ref(),
    );
    let Some(next) = out.provenance.plan.as_mut() else {
        withhold_record(out);
        return;
    };
    // A revision's record binds its resolved words, never replayed as the caller's creation.
    let words = CompileRequest::create(contract.resolved);
    next["basis"]["caller"] = nika_compile::surface::semantic::caller(&words).unwrap_or_default();
    let laws = revision::preserved(base, next).and_then(|changed| {
        let c = contract;
        let (original, change) = (c.original, c.change);
        revision::delta(
            original,
            c.base_ledger,
            change,
            c.change_ledger,
            c.resolved,
            c.stated,
            &changed,
        )
    });
    match laws {
        Ok(mut delta) => {
            delta["base_candidate_sha256"] = base["final"]["candidate_sha256"].clone();
            out.provenance.decision.get_or_insert_with(|| json!({}))["revision"] = delta;
        }
        Err(why) => refuse(out, &why),
    }
}

/// Revise `request` (an EDIT in words whose `plan` is the base's semantic record) through the
/// sketch door: the links, then the fills of the base graph against the resolved request, then
/// the laws and the judge of that request.
async fn revise<P: ProviderInferDyn>(
    request: &CompileRequest,
    policy: &AuthoringPolicy,
    provider: &P,
) -> Result<CompileOutcome, CompileError> {
    let mut out = crate::initial();
    let (
        Input::Edit {
            change: EditChange::Text(change),
            ..
        },
        Some(base),
    ) = (&request.input, request.plan.as_ref())
    else {
        return Ok(out);
    };
    let original = base["basis"]["read"]["effective"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let revising = revising_of(request, base, &original);
    let intent = nika_compile::revise_intent(&revising).unwrap_or_default();
    let reading = lexicon::read(&intent);
    let cold = cold(&mut out);
    let Some(pair) = base_pair(base).filter(|_| !floor_refuses(&reading, &mut out)) else {
        out.provenance.strategy = Some(Strategy::Native);
        return Ok(out);
    };
    let words_only = CompileRequest::create(change.as_str());
    let change_ledger =
        nika_compile::surface::semantic::request_basis(change, &words_only)["ledger"].clone();
    let base_ledger = base["basis"]["read"]["ledger"].clone();
    let (mut talk, sent, shown) = opened(
        base,
        &intent,
        &reading,
        &revising,
        (&base_ledger, &change_ledger),
    );
    let linked = super::call::<Links, P>(
        &mut talk,
        0,
        "revision",
        revision::links_schema(),
        policy,
        provider,
        &mut out,
    )
    .await;
    let stated = json!(linked.as_ref().map(|(links, _)| links.stated()));
    let ledgers = (&base_ledger, &change_ledger);
    let linked = match linked {
        Some(pair) if destination_edit(request, &original, ledgers, &stated) => {
            let at = (request, original.as_str(), base);
            let talked = (talk, sent, shown, cold);
            return Ok(delegated(at, (policy, provider), talked, pair, out).await);
        }
        other => other,
    };
    let filled = resolve_and_fill(
        &mut talk,
        &mut out,
        linked.as_ref(),
        (&original, &intent, &change_ledger),
        &stated,
        (policy, provider),
        &pair,
    );
    let (resolved, reading, accepted) = match filled.await {
        Ok(filled) => filled,
        Err(why) => {
            native::record(&mut out, &revising, &cold, &talk, &sent, None, shown);
            refuse(&mut out, &why);
            out.provenance.strategy = Some(Strategy::Native);
            return Ok(out);
        }
    };
    let answer = accepted.as_ref().map(|(answer, _)| answer);
    native::record(&mut out, &revising, &cold, &talk, &sent, answer, shown);
    conclude(
        &resolved, &reading, &revising, answer, &talk, cold, &mut out,
    );
    out.provenance.strategy = Some(Strategy::Native);
    if let Some((_, fills)) = &accepted {
        let contract = Contract {
            original: &original,
            change,
            resolved: &resolved,
            base_ledger: &base_ledger,
            change_ledger: &change_ledger,
            stated: &stated,
        };
        bind(&mut out, base, &revising, &graph(&pair.1), fills, &contract);
    }
    if out.status != crate::CompileStatus::Ready {
        return Ok(out);
    }
    let judge = super::super::verify::judged_native;
    Ok(judge(&resolved, &reading, policy, provider, &revising, out).await)
}

/// A revision the preservation or delta laws refuse: nothing READY, no record, each law named.
fn refuse(out: &mut CompileOutcome, why: &[String]) {
    out.provenance.plan = None;
    out.candidate = None;
    out.status = crate::CompileStatus::Incomplete;
    for reason in why {
        let message = format!(
            "The revision is not kept: {reason}. The base is kept as it is; state the change again."
        );
        crate::finding(out, DiagnosticKind::Missed, "revision", message);
    }
}

/// A revision in words of a base no semantic record binds (a historical, native or manually
/// written source) that is no replacement of one destination: kept as it is, never rewritten
/// from source, with the limitation stated. No candidate is returned: the host keeps its base,
/// and no bytes of a revision that was not made could be saved in its place.
pub(in crate::cognition) fn historical() -> CompileOutcome {
    let mut out = crate::initial();
    crate::finding(
        &mut out,
        DiagnosticKind::Missed,
        "change_request",
        "This base carries no semantic record, so a change in words cannot be revised semantically: the base is kept as it is and no model was asked. Without a record, only the replacement of one destination it writes, stated with its new path beside the request the base answers, is revised in place. Revise a workflow saved with its semantic record, change a constant (`Set const.NAME to JSON_LITERAL`), or create the workflow again from its request.",
    );
    out
}

/// The words the base answers for a source-anchored revision: the resolved words its source
/// revision record states (the record binds the base in the core), else the request's own.
fn source_original(request: &CompileRequest) -> Option<String> {
    let recorded = (request.plan.as_ref()).and_then(|r| r["source_revision"]["resolved"].as_str());
    recorded
        .map(str::to_owned)
        .or_else(|| request.original_intent.clone())
}

/// Whether a source-anchored revision can be asked at all, zero calls: the base parses and
/// writes a destination. Whether the change touches one — even without naming a path — is the
/// seat's typed reading to state, never a word list.
fn revisable(base: &str) -> bool {
    crate::edit::literal_projection(base)
        .and_then(|document| crate::sketch::import::written(&document).ok())
        .is_some_and(|written| !written.is_empty())
}

/// The revision in words of a base no semantic record binds (slice F, bounded): one destination
/// it writes replaced in place. The seat answers the typed links and additions only (one call);
/// the core proves and writes the substitution (`nika_compile::source_revision`) and records it
/// in the native seam; the round's judge reads the result against the whole request. A link to
/// a written destination with no new path asks the path (a bounded question). A base that cannot
/// be read or writes nothing is kept with its limitation, no call.
pub(in crate::cognition) async fn source<P: ProviderInferDyn>(
    request: &CompileRequest,
    policy: &AuthoringPolicy,
    provider: &P,
) -> Result<CompileOutcome, CompileError> {
    let kept = || {
        let mut out = historical();
        crate::record_route(&mut out, &[super::super::forensic::EDIT_KEPT.to_owned()]);
        out
    };
    let Input::Edit {
        source: base,
        change: EditChange::Text(change),
    } = &request.input
    else {
        return Ok(kept());
    };
    let Some(original) = source_original(request).filter(|_| revisable(base)) else {
        return Ok(kept());
    };
    let mut out = crate::initial();
    let intent = nika_compile::revise_intent(request).unwrap_or_default();
    let reading = lexicon::read(&intent);
    let cold = cold(&mut out);
    if floor_refuses(&reading, &mut out) {
        out.provenance.strategy = Some(Strategy::Native);
        return Ok(out);
    }
    let ledger = |words: &str| {
        nika_compile::surface::semantic::request_basis(words, &CompileRequest::create(words))
            ["ledger"]
            .clone()
    };
    let ledgers = (ledger(&original), ledger(change));
    let (mut talk, sent, shown) = source_opened(base, &intent, &reading, request, &ledgers);
    let linked = super::call::<Links, P>(
        &mut talk,
        0,
        "revision",
        revision::links_schema(),
        policy,
        provider,
        &mut out,
    )
    .await;
    let Some(linked) = linked else {
        native::record(&mut out, request, &cold, &talk, &sent, None, shown);
        crate::record_route(&mut out, &talk.route);
        out.provenance.strategy = Some(Strategy::Native);
        return Ok(out);
    };
    let journal = Journal {
        talk,
        sent,
        shown,
        cold,
    };
    let reading = (intent.as_str(), &reading);
    Ok(source_settled(request, reading, (policy, provider), journal, linked, out).await)
}

/// What a revision's round journals: its talk, the references sent, the revision shown, the cold
/// report it opened over.
struct Journal<'a> {
    talk: Talk,
    sent: Vec<Value>,
    shown: Option<(&'a str, &'a str)>,
    cold: native::Cold,
}

/// The source edit a typed revision states, proven and recorded by the core
/// (`nika_compile::source_revision`), its links journaled in the round, a READY result judged by
/// the round's judge — never another seat call. `out` holds the call's receipt.
async fn source_settled<P: ProviderInferDyn>(
    request: &CompileRequest,
    (intent, reading): (&str, &lexicon::Reading),
    (policy, provider): (&AuthoringPolicy, &P),
    mut journal: Journal<'_>,
    (links, text): (Links, String),
    mut out: CompileOutcome,
) -> CompileOutcome {
    let mut done = nika_compile::source_revision(request, &links.stated());
    let refused: Vec<Value> = (done.diagnostics.iter())
        .filter(|d| d.kind == DiagnosticKind::Missed && d.target == "revision")
        .map(|d| json!({"kind": "revision", "message": d.message}))
        .collect();
    let notes = crate::cognition::receipt::withheld(&links.notes, &[], "revision notes");
    journal.talk.rounds.push(
        json!({"round": 0, "phase": "revision", "supersedes": links.supersedes,
        "adds": links.adds, "notes": notes, "diagnostics": refused}),
    );
    journal
        .talk
        .messages
        .push(nika_kernel::ai::provider::Message::text(
            nika_kernel::ai::provider::Role::Assistant,
            text,
        ));
    let accepted = (done.provenance.plan.as_ref())
        .and_then(|record| record["source"].as_str())
        .map(|candidate| Answer {
            candidate: candidate.to_owned(),
            questions: Vec::new(),
            gaps: Vec::new(),
        });
    done.provenance.authoring = out.provenance.authoring.take();
    let Journal {
        talk,
        sent,
        shown,
        cold,
    } = journal;
    native::record(
        &mut done,
        request,
        &cold,
        &talk,
        &sent,
        accepted.as_ref(),
        shown,
    );
    crate::record_route(&mut done, &talk.route);
    if done.status != crate::CompileStatus::Ready {
        return done;
    }
    super::super::verify::judged_native(intent, reading, policy, provider, request, done).await
}

/// The talk a source-anchored revision opens: the base workflow, both clause lists and the
/// destinations the base writes, beside the request.
fn source_opened<'a>(
    base: &str,
    intent: &str,
    reading: &lexicon::Reading,
    request: &'a CompileRequest,
    ledgers: &(Value, Value),
) -> (Talk, Vec<Value>, Option<(&'a str, &'a str)>) {
    let native::Prelude {
        references,
        callables,
        sent,
        revision: shown,
        mut opening,
        allowed,
    } = prelude(intent, reading, request);
    opening["original_clauses"] = json!(revision::clauses(&ledgers.0));
    opening["change_clauses"] = json!(revision::clauses(&ledgers.1));
    opening["base_destinations"] = json!(
        crate::edit::literal_projection(base)
            .and_then(|doc| crate::sketch::import::written(&doc).ok())
            .unwrap_or_default()
    );
    let mut system = system_message(&references, &callables);
    system.push_str("\n\n");
    system.push_str(REVISE_SOURCE);
    // The opening is the machine-readable facts alone (hosts read it); what to answer is the
    // system's to say.
    let first = opening.to_string();
    let mut talk = Talk::open(
        system,
        first,
        vec![SOURCE_ROUTE.to_owned()],
        allowed,
        request,
    );
    talk.presented = json!(sent);
    (talk, sent, shown)
}

/// The base bytes of an edit request.
fn base_source(request: &CompileRequest) -> Option<&str> {
    match &request.input {
        Input::Edit { source, .. } => Some(source.as_str()),
        _ => None,
    }
}

/// Whether the typed `stated` answer edits a destination of the request's base (a written
/// destination replaced, or one added): a structural edit the base graph's laws keep out of a
/// fill, proven instead on the very bytes the record binds ([`delegated`]) from the same typed
/// answer — or left to a bounded question.
fn destination_edit(
    request: &CompileRequest,
    original: &str,
    ledgers: (&Value, &Value),
    stated: &Value,
) -> bool {
    let Some(document) = base_source(request).and_then(crate::edit::literal_projection) else {
        return false;
    };
    crate::sketch::import::decide(&document, original, ledgers, stated, &|_| None).is_ok()
}

/// A destination edit of a record-bound base, proven by the source laws on the very bytes the
/// record binds, under the record's own words (`original`), from the same typed answer; the
/// revised record keeps the digest of the base's semantic record.
async fn delegated<P: ProviderInferDyn>(
    (request, original, base): (&CompileRequest, &str, &Value),
    (policy, provider): (&AuthoringPolicy, &P),
    (talk, sent, shown, cold): (Talk, Vec<Value>, Option<(&str, &str)>, native::Cold),
    pair: (Links, String),
    out: CompileOutcome,
) -> CompileOutcome {
    let journal = Journal {
        talk,
        sent,
        shown,
        cold,
    };
    let mut bound = request.clone().with_original_intent(original.to_owned());
    bound.plan = None;
    let intent = nika_compile::revise_intent(&bound).unwrap_or_default();
    let reading = lexicon::read(&intent);
    let seated = (policy, provider);
    let mut done = source_settled(&bound, (&intent, &reading), seated, journal, pair, out).await;
    if let Some(record) = done.provenance.plan.as_mut().filter(|r| r.is_object()) {
        record["semantic_base_sha256"] = json!(nika_compile::surface::sha256(&base.to_string()));
    }
    done
}

/// The request a semantic revision reads: the record's own original words and answers, never
/// restated ones, and no record (the revision binds its own).
fn revising_of(request: &CompileRequest, base: &Value, original: &str) -> CompileRequest {
    let mut revising = request.clone().with_original_intent(original.to_owned());
    revising.plan = None;
    revising.answers = (base["final"]["answers"].as_object().into_iter().flatten())
        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
        .collect();
    revising
}
