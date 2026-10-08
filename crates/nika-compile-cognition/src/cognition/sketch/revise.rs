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
//! A READY revision the judge refuses is filled again from the part it names, its graph kept,
//! within the same round count (`filled`).
//! The base record is never rewritten: the revision's own semantic record binds the revised bytes
//! under the resolved words, the next revision's original; the delta is recorded apart
//! (`decision.revision`). A change to the graph's structure is not revised here, and a base no
//! semantic record binds is kept as it is ([`historical`]).
//!
//! The links are judged before any fill by every law that reads no program
//! (`sketch::revision::linked` and the replacement spans of `resolved`). Links that break one are
//! the seat's to state again, in the same talk, beside the same base, original request and clause
//! lists, within the door's one round count (`linked`); a change no fill carries, a repeated
//! refusal or a spent allowance is refused with every law named, and nothing is filled.
//!
//! [`historical`]: crate::cognition::sketch::revise::historical

use super::{
    Answer, Question, SKETCH, SketchAnswer, Talk, cold, conclude, fill, floor_refuses, graph,
    judge_defects, native, next_round, prelude, reopen, repair, semantic_record, system_message,
    withhold_record, within,
};
use crate::cognition::verify;
use crate::cognition::{AuthoringPolicy, CompileOutcome, CompileRequest, DiagnosticKind, Strategy};
use crate::decide::DecisionSeat;
use crate::fidelity::Diagnostic;
use crate::sketch::{Sketch, revision};
use crate::types::{EditChange, Input};
use crate::{CompileError, lexicon};
use nika_compile_seats::foundry::{ComponentCatalog, document};
use nika_kernel::ai::provider::{Message, ProviderInferDyn, Role};
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
    /// Operations over the complete base document (`crate::cognition::document`), for any change
    /// the destination links cannot state.
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    operations: Vec<Value>,
    /// The whole revised source, only when no operation can state the change.
    #[serde(default)]
    replace: Option<String>,
}

impl Links {
    /// Whether the seat stated the change over the document rather than as destination links.
    fn over_the_document(&self) -> bool {
        !self.operations.is_empty() || self.replacement().is_some()
    }

    /// The whole revised source, when one was given (an empty text is none).
    fn replacement(&self) -> Option<&str> {
        self.replace
            .as_deref()
            .filter(|source| !source.trim().is_empty())
    }
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

const REVISE_SOURCE: &str = "This is a REVISION of the base workflow below (`base_candidate`), which no semantic record binds; you never write the workflow from scratch. When the change replaces ONE destination the base writes, or adds ONE beside the existing ones, state it as links and the compiler edits it in place: in `supersedes`, name the clause of the original request that states the destination the change replaces (copied exactly from `original_clauses`) and the clause of the change that states its new destination (copied exactly from `change_clauses`) — or no link when the change adds a destination; in `adds`, every other clause of the change (copied exactly from `change_clauses`), each adding or restating a duty without replacing any. Every change clause is in exactly one of the two. When the change adds a destination, `like` names the destination of `base_destinations` whose written content the new one receives. Name only what the request and the change state; when they leave it open, omit it and the human is asked. With links, leave `operations` empty.";

/// The prompt of a revision with no destination link to state (the base writes none, or the
/// request it answers is unknown): operations over the document only.
const REVISE_DOCUMENT: &str = "This is a REVISION of the base workflow below, which no semantic record binds; you never write the workflow from scratch. There is no destination link to state here: leave `supersedes` and `adds` empty and state the change as `operations`.";

const REVISE: &str = "This is a REVISION of the base program below (`base_graph`, `base_fills`). Its graph stays exactly as it is: the change only changes what its tasks do through their typed holes. Call 1: in `supersedes`, name each clause of the original request the change replaces (copied exactly from `original_clauses`) and the clause of the change that replaces it (copied exactly from `change_clauses`); in `adds`, name each clause of the change (copied exactly from `change_clauses`) that adds a duty beside the original ones and replaces none. Every change clause is in exactly one of the two; every other original clause stays. Call 2: fill the base graph's holes for the revised request.";

/// The tail of a links repair: the base, the original request and both clause lists stay; only
/// the links are stated again, under the laws the diagnostics name.
const LINKS_AGAIN: &str = "\nState the links again: the base, the original request and both clause lists are unchanged. Answer the same {\"supersedes\", \"adds\", \"notes\"} object, every clause copied exactly from `original_clauses` or `change_clauses`. `supersedes` links only an original clause whose duty the change replaces; two links never name original clauses that overlap or nest (when one original clause contains another, link only the one whose duty the change replaces). A change clause that restates a duty the base already carries replaces nothing: it belongs in `adds`, never in a link. Every change clause is in exactly one of the two.";

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
    decision: Option<&dyn DecisionSeat>,
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
        Some((policy, provider)) => revise(reading, policy, (provider, decision)).await,
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

/// The seat's links judged before any fill, by the laws that read no program.
enum Judged {
    /// Every law holds: the request the revision consumes from here on.
    Holds(String),
    /// Links that break a law the seat repairs by stating them again.
    Links(Vec<String>),
    /// A change that adds what the program's structure carries: no restatement of its links can
    /// hold, so it is refused as it is.
    Change(Vec<String>),
}

/// The links `stated` judged before any fill. An addition the program's structure carries
/// (`revision::additions`) is the change's own, refused as it is; a link or accounting law
/// (`revision::linked`) or a replacement span (`revision::resolved`: a clause not stated once,
/// two that overlap or nest) is the seat's to repair. When every law holds, the contract the
/// revision consumes from here on: the original words with each linked clause replaced in place
/// and every addition beside (`revision::resolved`), never the original beside the whole change.
fn judged(
    (original, change): (&str, &str),
    (base, asked): (&Value, &Value),
    stated: &Value,
) -> Judged {
    if revision::accounted(asked, stated).is_ok()
        && let Err(structural) = revision::additions(asked, stated)
    {
        return Judged::Change(structural);
    }
    let links = &stated["supersedes"];
    let mut why =
        (revision::linked(original, base, change, asked, stated).err()).unwrap_or_default();
    // A span law reads no addition: an addition is appended, never replaced in place.
    for reason in (revision::resolved(original, links, &[]).err().into_iter()).flatten() {
        if !why.contains(&reason) {
            why.push(reason);
        }
    }
    if !why.is_empty() {
        return Judged::Links(why);
    }
    let read = revision::additions(asked, stated)
        .and_then(|added| revision::resolved(original, links, &added));
    match read {
        Ok(read) => Judged::Holds(lexicon::fold_apostrophes(&read)),
        Err(why) => Judged::Links(why),
    }
}

/// What the links round settled.
enum Linked {
    /// The links hold: what they state, the request they resolve, the round the fills start at.
    Held(Value, String, u32),
    /// A destination edit, proven by the source laws ([`delegated`]), and the round it was stated.
    Destination((Links, String), u32),
    /// No answer to read (a failed call, a malformed text): nothing is filled.
    Unanswered,
    /// Links that still break a law, or a change no fill carries: why. Nothing is filled.
    Refused(Vec<String>),
}

/// The seat's typed links, judged before any fill ([`judged`]) and repaired within the door's
/// one round count from round 0 (the last round leaves the fills their turn): a repair names the
/// laws the links break in the same talk, the base, the original request and both clause lists
/// unchanged. A destination edit leaves for the source laws as soon as it is stated; a change no
/// fill carries, a repeated refusal or a spent allowance is refused with why; a failed call or a
/// malformed text ends the talk with no fill.
async fn linked<P: ProviderInferDyn>(
    talk: &mut Talk,
    (request, original, change): (&CompileRequest, &str, &str),
    ledgers: (&Value, &Value),
    (policy, provider): (&AuthoringPolicy, &P),
    out: &mut CompileOutcome,
) -> Linked {
    let last = policy.repairs;
    talk.remember_under(policy);
    let mut round = 0;
    loop {
        let role = if round == 0 {
            "revision"
        } else {
            "revision-repair"
        };
        let schema = revision::links_schema();
        let called = super::call::<Links, P>(talk, round, role, schema, policy, provider, out);
        let Some((links, text)) = called.await else {
            return Linked::Unanswered;
        };
        let stated = links.stated();
        if destination_edit(request, original, ledgers, &stated) {
            return Linked::Destination((links, text), round);
        }
        // The seat's own notes are journaled by digest and shape only, never as text.
        let notes = crate::cognition::receipt::withheld(&links.notes, &[], "revision notes");
        let why = match judged((original, change), ledgers, &stated) {
            Judged::Holds(resolved) => {
                talk.rounds.push(json!({"round": round, "phase": "revision",
                    "supersedes": links.supersedes, "adds": links.adds, "notes": notes}));
                talk.messages.push(Message::text(Role::Assistant, text));
                return Linked::Held(stated, resolved, round + 1);
            }
            Judged::Change(why) => {
                talk.rounds
                    .push(refused_links(round, &stated, &notes, &why));
                return Linked::Refused(why);
            }
            Judged::Links(why) => why,
        };
        talk.rounds
            .push(refused_links(round, &stated, &notes, &why));
        let diagnostics: Vec<Diagnostic> = (why.iter())
            .map(|message| Diagnostic {
                kind: "revision",
                message: message.clone(),
            })
            .collect();
        if last.is_some_and(|last| round >= last) || !repair(talk, text, diagnostics, LINKS_AGAIN) {
            return Linked::Refused(why);
        }
        round += 1;
    }
}

/// A refused links round as the journal keeps it: the links by digest and shape (the laws
/// refused them), the notes withheld, each law named.
fn refused_links(round: u32, stated: &Value, notes: &Value, why: &[String]) -> Value {
    let keys = ["supersedes", "adds", "like"];
    let proposed =
        crate::cognition::receipt::withheld(&stated.to_string(), &keys, "refused revision links");
    let diagnostics: Vec<Value> = (why.iter())
        .map(|message| json!({"kind": "revision", "message": message}))
        .collect();
    json!({"round": round, "phase": "revision", "proposed_links": proposed, "notes": notes,
        "diagnostics": diagnostics})
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
/// sketch door: the links, judged and repaired before any fill ([`linked`]), then the fills of
/// the base graph against the resolved request, then the laws and the judge of that request.
async fn revise<P: ProviderInferDyn>(
    request: &CompileRequest,
    policy: &AuthoringPolicy,
    (provider, decision): (&P, Option<&dyn DecisionSeat>),
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
    let ledgers = (&base_ledger, &change_ledger);
    let (mut talk, sent, shown) = opened(base, &intent, &reading, &revising, ledgers);
    let at = (request, original.as_str(), change.as_str());
    let (stated, resolved, round) =
        match linked(&mut talk, at, ledgers, (policy, provider), &mut out).await {
            Linked::Held(stated, resolved, round) => (stated, resolved, round),
            Linked::Destination(pair, round) => {
                let at = (request, original.as_str(), base);
                let journal = Journal {
                    round,
                    talk,
                    sent,
                    shown,
                    cold,
                };
                return Ok(delegated(at, (policy, provider, decision), journal, pair, out).await);
            }
            Linked::Unanswered => {
                native::record(&mut out, &revising, &cold, &talk, &sent, None, shown);
                conclude(&intent, &reading, &revising, None, &talk, cold, &mut out);
                out.provenance.strategy = Some(Strategy::Native);
                return Ok(out);
            }
            Linked::Refused(why) => {
                native::record(&mut out, &revising, &cold, &talk, &sent, None, shown);
                refuse(&mut out, &why);
                out.provenance.strategy = Some(Strategy::Native);
                return Ok(out);
            }
        };
    let contract = Contract {
        original: &original,
        change,
        resolved: &resolved,
        base_ledger: &base_ledger,
        change_ledger: &change_ledger,
        stated: &stated,
    };
    let journal = Journal {
        round,
        talk,
        sent,
        shown,
        cold,
    };
    let connection = (policy, provider, decision);
    Ok(filled(
        (&revising, base, &pair),
        connection,
        journal,
        &contract,
        out,
    )
    .await)
}

/// The fills of the base graph against the resolved request (`contract.resolved`), from the
/// round the links left; the revised record bound and judged by the laws ([`bind`]), then a READY
/// result judged against that request. A part the judge finds missing reopens the fills from that
/// defect, the graph kept, within the door's one round count, as a creation's candidate reopens
/// its sketch; a defect already answered is no progress. Every reopening fills, binds and judges
/// new bytes, so no verdict or delta of a replaced candidate stands for them; what ends not READY
/// names the repairs made.
async fn filled<P: ProviderInferDyn>(
    (revising, base, pair): (&CompileRequest, &Value, &(Sketch, SketchAnswer)),
    (policy, provider, decision): (&AuthoringPolicy, &P, Option<&dyn DecisionSeat>),
    journal: Journal<'_>,
    contract: &Contract<'_>,
    mut out: CompileOutcome,
) -> CompileOutcome {
    let Journal {
        mut round,
        mut talk,
        sent,
        shown,
        cold,
    } = journal;
    let resolved = contract.resolved;
    let reading = lexicon::read(resolved);
    let (last, mut repairs) = (policy.repairs.map(|r| r.saturating_add(1)), 0);
    loop {
        let accepted = fill(
            &mut talk, resolved, &reading, policy, provider, &mut out, pair, round,
        )
        .await;
        let mut done = out.clone();
        let answer = accepted.as_ref().map(|(answer, _)| answer);
        native::record(&mut done, revising, &cold, &talk, &sent, answer, shown);
        conclude(
            resolved,
            &reading,
            revising,
            answer,
            &talk,
            cold.clone(),
            &mut done,
        );
        done.provenance.strategy = Some(Strategy::Native);
        if let Some((_, fills)) = &accepted {
            bind(&mut done, base, revising, &graph(&pair.1), fills, contract);
        }
        if done.status != crate::CompileStatus::Ready {
            return done;
        }
        let seats = (provider, decision);
        let judged = verify::native_verdict(
            resolved, &reading, policy, seats, revising, done, repairs, None,
        );
        let (mut judged, verdict) = match judged.await {
            Ok(judged) => return judged,
            Err(judged) => *judged,
        };
        // No delta of refused bytes survives them, whether the fills reopen or the revision ends.
        if let Some(record) = (judged.provenance.decision.as_mut()).and_then(Value::as_object_mut) {
            record.remove("revision");
        }
        // A defect reopens the fills while a round is left; an unsettled verdict never does, and
        // neither does a verdict repeated on bytes already judged (no progress, R6).
        let next = next_round(&talk);
        let open =
            !verdict.defects.is_empty() && verdict.same_bytes_as.is_none() && within(last, next);
        if !(open && reopen(&mut talk, judge_defects(&verdict), JUDGED_AGAIN)) {
            if open {
                verify::route(&mut judged, "native: no progress");
            }
            if verdict.defects.is_empty() && verdict.doubted() {
                return verify::held(judged, &verdict);
            }
            return verify::withdrawn(judged, &verdict, repairs);
        }
        repairs += 1;
        talk.route.push(format!("verify: repair {repairs}"));
        // The judge's calls and verdicts join the door's one journal.
        (out.provenance.authoring).clone_from(&judged.provenance.authoring);
        (out.provenance.decision).clone_from(&judged.provenance.decision);
        round = next;
    }
}

/// What a fill reopened from the judge's defect is asked, before its holes are listed again.
const JUDGED_AGAIN: &str = "\nThe workflow these fills made was refused by the judge above. The graph stays as accepted: fill its holes again so that the workflow does what the whole revised request asks.";

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
    (provider, decision): (&P, Option<&dyn DecisionSeat>),
    catalog: Option<&dyn ComponentCatalog>,
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
    // A base the strict parser does not read cannot be revised in place: kept, no seat asked.
    if nika_compile::parse(base).is_err() {
        return Ok(kept());
    }
    // Destination links need the request the base answers and a destination it writes; any
    // other change is stated over the complete document.
    let original = source_original(request);
    let destinations = original.is_some() && revisable(base);
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
    let ledgers = (
        ledger(original.as_deref().unwrap_or_default()),
        ledger(change),
    );
    let lent = (destinations, catalog);
    let (mut talk, sent, shown) = source_opened(base, &intent, &reading, request, &ledgers, lent);
    let linked = super::call::<Links, P>(
        &mut talk,
        0,
        "revision",
        revision_schema(),
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
        round: 0,
        talk,
        sent,
        shown,
        cold,
    };
    let reading = (intent.as_str(), &reading);
    let seated = (policy, provider, decision);
    if linked.0.over_the_document() || !destinations {
        let answer = (linked, catalog);
        return Ok(document_settled(request, reading, seated, journal, answer, out).await);
    }
    Ok(source_settled(request, reading, seated, journal, linked, out).await)
}

/// The schema of a record-less revision's one answer: the destination links, and the operations
/// or the whole replacement any other change is stated as. Every operation field is text (a
/// value travels as its JSON text) so a strict structured-output dialect can carry it.
fn revision_schema() -> Value {
    let mut schema = revision::links_schema();
    let (operations, replace) = document::answer_schema();
    schema["properties"]["operations"] = operations;
    schema["properties"]["replace"] = replace;
    schema
}

/// The change stated over the complete document: applied in order to the base
/// ([`document::apply`]), the result bound to its record and checked by the
/// strict parser and Check, then judged against the whole request by the round's judge — never
/// another seat call. Links stated where no destination edit applies are refused, never guessed
/// into operations. A refusal leaves no candidate and names every reason.
async fn document_settled<P: ProviderInferDyn>(
    request: &CompileRequest,
    (intent, reading): (&str, &lexicon::Reading),
    (policy, provider, decision): (&AuthoringPolicy, &P, Option<&dyn DecisionSeat>),
    mut journal: Journal<'_>,
    ((links, text), catalog): ((Links, String), Option<&dyn ComponentCatalog>),
    mut out: CompileOutcome,
) -> CompileOutcome {
    let Input::Edit { source: base, .. } = &request.input else {
        return out;
    };
    let carried = document::carried(request.plan.as_ref());
    let applied = if links.over_the_document() {
        document::apply(
            base,
            (&links.operations, links.replacement()),
            catalog,
            &carried,
        )
    } else {
        Err(vec![
            "the change was stated as destination links where no destination edit applies (the base writes none, or the request it answers is unknown); state it as operations".to_owned(),
        ])
    };
    let notes = crate::cognition::receipt::withheld(&links.notes, &[], "revision notes");
    let stated = json!({"operations": links.operations.len(),
        "replaced": links.replacement().is_some()});
    let refused: Vec<&String> = applied.as_ref().err().into_iter().flatten().collect();
    journal
        .talk
        .rounds
        .push(json!({"round": journal.round, "phase": "revision",
        "document": stated, "notes": notes, "refused": refused}));
    journal
        .talk
        .messages
        .push(Message::text(Role::Assistant, text));
    journal.talk.route.push(document::ROUTE.to_owned());
    let Journal {
        talk,
        sent,
        shown,
        cold,
        ..
    } = journal;
    let applied = match applied {
        Ok(applied) => applied,
        Err(why) => {
            native::record(&mut out, request, &cold, &talk, &sent, None, shown);
            crate::record_route(&mut out, &talk.route);
            refuse(&mut out, &why);
            out.provenance.strategy = Some(Strategy::Native);
            return out;
        }
    };
    let mut done = crate::initial();
    nika_compile::finish(applied.source.clone(), &mut done);
    done.provenance.strategy = Some(Strategy::Native);
    done.provenance.authoring = out.provenance.authoring.take();
    let resolved = source_original(request).map_or_else(
        || intent.to_owned(),
        |original| format!("{original}\n{}", revision_change(request)),
    );
    let sha = nika_compile::intent_sha256(intent);
    let record = document::record((base, &applied.source), &resolved, &sha, &applied);
    let mut decision_record = done.provenance.decision.take().unwrap_or_else(|| json!({}));
    decision_record["document_revision"] = record["document_revision"].clone();
    done.provenance.decision = Some(decision_record);
    done.provenance.plan = Some(record);
    let answer = Answer {
        candidate: applied.source.clone(),
        questions: Vec::new(),
        gaps: Vec::new(),
    };
    native::record(
        &mut done,
        request,
        &cold,
        &talk,
        &sent,
        Some(&answer),
        shown,
    );
    crate::record_route(&mut done, &talk.route);
    // What reuse the bytes really hold: each composed component witnessed on the candidate.
    let qualification = json!({"by": null, "why": "a revision: components are composed by operations, none is qualified here"});
    crate::cognition::knowledge::reused(request, qualification, &applied.receipts, &mut done);
    if done.status != crate::CompileStatus::Ready {
        return done;
    }
    super::super::verify::judged_native(
        intent,
        reading,
        policy,
        (provider, decision),
        request,
        done,
    )
    .await
}

/// The change words of an EDIT, as the revision read them.
fn revision_change(request: &CompileRequest) -> String {
    match &request.input {
        Input::Edit {
            change: EditChange::Text(words),
            ..
        } => format!("Change: {words}"),
        _ => String::new(),
    }
}

/// What a revision's round journals: the round its accepted answer was stated in (the links
/// round, or the next one: the fills' first), its talk, the references sent, the revision shown,
/// the cold report it opened over.
struct Journal<'a> {
    round: u32,
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
    (policy, provider, decision): (&AuthoringPolicy, &P, Option<&dyn DecisionSeat>),
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
        json!({"round": journal.round, "phase": "revision", "supersedes": links.supersedes,
        "adds": links.adds, "notes": notes, "diagnostics": refused}),
    );
    journal
        .talk
        .messages
        .push(Message::text(Role::Assistant, text));
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
        ..
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
    super::super::verify::judged_native(
        intent,
        reading,
        policy,
        (provider, decision),
        request,
        done,
    )
    .await
}

/// The talk a source-anchored revision opens: the base workflow, both clause lists and the
/// destinations the base writes, beside the request.
fn source_opened<'a>(
    base: &str,
    intent: &str,
    reading: &lexicon::Reading,
    request: &'a CompileRequest,
    ledgers: &(Value, Value),
    (destinations, catalog): (bool, Option<&dyn ComponentCatalog>),
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
    opening["base_source"] = json!(base);
    opening["base_document"] = document::nodes(base).unwrap_or_default();
    opening["components"] = document::components(catalog);
    opening["composed"] = json!(
        (document::carried(request.plan.as_ref()).iter())
            .map(|receipt| json!({"component": receipt["component"]["id"],
                "bindings": receipt["bindings"]}))
            .collect::<Vec<_>>()
    );
    let mut system = system_message(&references, &callables);
    system.push_str("\n\n");
    system.push_str(if destinations {
        REVISE_SOURCE
    } else {
        REVISE_DOCUMENT
    });
    system.push_str("\n\n");
    system.push_str(document::OPERATIONS);
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
    (policy, provider, decision): (&AuthoringPolicy, &P, Option<&dyn DecisionSeat>),
    journal: Journal<'_>,
    pair: (Links, String),
    out: CompileOutcome,
) -> CompileOutcome {
    let mut bound = request.clone().with_original_intent(original.to_owned());
    bound.plan = None;
    let intent = nika_compile::revise_intent(&bound).unwrap_or_default();
    let reading = lexicon::read(&intent);
    let seated = (policy, provider, decision);
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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod document_tests;
