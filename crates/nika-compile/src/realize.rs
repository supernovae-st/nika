// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The READY law over the assembled workflow: every duty the request states is carried by a
//! named element or the candidate is not emitted; one approval clause covering several
//! effects is one gate; an effect repeated per item is asked; two bounds that cannot both
//! hold are refused. The ledger rides in provenance either way, observational, never
//! authority.

use super::assemble::{Doc, Laws, emit};
use super::bindings::{Bindings, Operation, RuleBinding, Witness, found, operations, read_whole};
use super::ledger::{
    Binding, Disposition, Duty, DutyKind, DutyState, Judgment, Ledger, WitnessKind,
};
use super::plan::{EffectPolicy, EffectVerb, Op, Plan};
use super::shape::Shape;
use super::{
    CompileError, CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, QuestionType,
};
use serde_json::{Value, json};

/// The realized topology and the READY law, then emission: every duty the request states is
/// carried by a named element, or the candidate is not emitted. The ledger rides in the
/// decision record either way.
pub(super) fn settle_candidate(
    plan: &Plan,
    b: &Bindings,
    d: Doc,
    laws: &Laws<'_>,
    judged: &Judged<'_>,
    out: &mut CompileOutcome,
) -> Result<(), CompileError> {
    // The realized topology, recorded beside the route: observational, never authority.
    let shape = Shape {
        fan_out: b.fan_out(),
        per_item: !b.per_item.is_empty(),
        outputs: b.writes.len() + b.wired.len(),
        gated: b.gated(),
    };
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    decision["shape"] = shape.to_json();
    if let Some(RuleBinding::Synthesized(rule)) = b.rule.bound() {
        decision["rule"] = rule.to_json();
    }
    // The READY law: every duty the request states is carried by a named element, or the
    // candidate is not emitted. The ledger rides in provenance either way.
    let mut ledger = Ledger::extract(plan);
    type_computation(&mut ledger, plan, b, &d);
    realize(&mut ledger, plan, b, &d, out.requested_trigger.is_some());
    ledger.cover(laws.intent, plan);
    let silent: Vec<(DutyKind, String, Option<String>)> = ledger
        .silent()
        .map(|duty| (duty.kind, duty.evidence.clone(), duty.note.clone()))
        .collect();
    decision["ledger"] = ledger.to_json();
    out.provenance.decision = Some(decision);
    if !silent.is_empty() {
        for (kind, evidence, note) in &silent {
            let message = match note {
                Some(note) => format!(
                    "The request states `{evidence}` ({}) and the compiled workflow breaks it: {note}; nothing is READY against a stated law.",
                    kind.word()
                ),
                None => format!(
                    "The request states `{evidence}` ({}) and no element of the compiled workflow carries it; nothing is READY with a silent obligation.",
                    kind.word()
                ),
            };
            super::finding(out, DiagnosticKind::Unknown, kind.word(), message);
        }
        super::question(
            out,
            "intent.clarification",
            "Supply a complete replacement request that names the operation carrying each stated instruction. It explicitly replaces the earlier intent.",
            QuestionType::Text,
        );
        return Ok(());
    }
    out.provenance.suggested_file = Some(suggested_file(plan, b));
    emit(d, laws, out)?;
    settle_pending(&mut ledger, laws.intent, judged, out);
    record_ledger(out, &ledger);
    Ok(())
}

/// What the READY law holds a candidate's pending duties to (R4 A11): the request, the plan as
/// the caller stated it (before the requester's decisions), the judgments a judge's seat made
/// in this very compile, and whether a model's plan produced the candidate: then the whole
/// request is pending too, and no single candidate is READY without its judgment.
pub(super) struct Judged<'a> {
    pub(super) request: &'a CompileRequest,
    pub(super) stated: &'a Plan,
    pub(super) judgments: &'a [Judgment],
    pub(super) whole: bool,
}

/// The pending duties of an emitted candidate (R4 A11). A judgment settles the duty whose
/// excerpt and span it names, only under the binding the core recomputes from the request, the
/// stated plan and these very bytes (context and bytes, not a round nonce); a clause the request
/// states several times is settled only when each statement is judged. Asking for no operation
/// is never admitted on a clause an element claims, nor on one that restricts or conditions the
/// material. A record's serialized judgment is data, never one: whatever stays pending keeps
/// READY closed, named in a finding with its next action.
fn settle_pending(
    ledger: &mut Ledger,
    intent: &str,
    judged: &Judged<'_>,
    out: &mut CompileOutcome,
) {
    let Some(candidate) = out.candidate.as_deref() else {
        return;
    };
    if judged.whole {
        ledger.duties.push(Duty::whole(intent));
    }
    if ledger.pending().next().is_none() {
        return;
    }
    let bound = Binding::of(intent, judged.request, judged.stated, candidate);
    for duty in ledger
        .duties
        .iter_mut()
        .filter(|d| d.state == DutyState::Pending)
    {
        let admitted = |span: (usize, usize), j: &Judgment| {
            j.binding == bound
                && j.clause == duty.evidence
                && j.span == span
                && match j.disposition {
                    Disposition::Carried => true,
                    Disposition::NoOperation => {
                        duty.witness.is_none()
                            && span != (0, intent.len())
                            && !super::structure::restricts(&duty.evidence)
                    }
                }
        };
        // Each statement of the excerpt is judged: one of them settles no other (R4 A11).
        let found: Option<Vec<&Judgment>> = statements(intent, &duty.evidence)
            .into_iter()
            .map(|span| judged.judgments.iter().find(|j| admitted(span, j)))
            .collect();
        if let Some(j) = found.as_ref().and_then(|found| found.first().copied()) {
            let what = match j.disposition {
                Disposition::Carried => "carried by the candidate",
                Disposition::NoOperation => "asking for no operation",
            };
            let questions: Vec<&str> = found
                .iter()
                .flatten()
                .map(|j| j.question.as_str())
                .collect();
            let note = format!("judged {what} by {} ({})", j.seat, questions.join(", "));
            duty.judge(&j.seat, note);
        }
    }
    let open: Vec<Value> = ledger
        .pending()
        .map(|d| {
            let spans: Vec<[usize; 2]> = statements(intent, &d.evidence)
                .into_iter()
                .map(|(start, end)| [start, end])
                .collect();
            let witness = d.witness.map(WitnessKind::word);
            json!({"clause": d.evidence, "witness": witness, "spans": spans})
        })
        .collect();
    for duty in ledger.pending() {
        let why = match duty.witness {
            Some(WitnessKind::Label) => "only the words of a step restate it",
            Some(WitnessKind::Unverified) => "a task carries words no law reads",
            _ => "no element of the plan names it",
        };
        super::finding(
            out,
            DiagnosticKind::Unknown,
            "semantic_verification",
            format!(
                "The request states `{}` and {why}: no law reads from candidate {} that it carries it, and no judgment made in this compile settles it. Nothing is READY on a pending clause: a bounded judge's seat judges it against the whole request, or it stays INCOMPLETE.",
                duty.evidence,
                &bound.candidate[..12]
            ),
        );
    }
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    decision["pending"] =
        json!({"candidate_sha256": bound.candidate, "plan_sha256": bound.plan, "open": open});
    out.provenance.decision = Some(decision);
    if !open.is_empty() && out.status == CompileStatus::Ready {
        out.status = CompileStatus::Incomplete;
    }
}

/// Every statement of `excerpt` in the request, in order, as byte spans: a pending duty is
/// settled only when each is judged (R4 A11). An empty excerpt states nothing.
fn statements(intent: &str, excerpt: &str) -> Vec<(usize, usize)> {
    if excerpt.is_empty() {
        return Vec::new();
    }
    intent
        .match_indices(excerpt)
        .map(|(at, _)| (at, at + excerpt.len()))
        .collect()
}

/// A kebab-case file name for the candidate: the first written file's stem (`open-sorted`),
/// else the first outbound effect's verb, else the first operation and its object (`draft-
/// summary`), else `compiled-workflow`; at most 40 characters, always `.nika`.
fn suggested_file(plan: &Plan, b: &Bindings) -> String {
    let stem = b
        .writes
        .first()
        .map(|w| w.stem.clone())
        .or_else(|| b.wired.first().map(|w| w.slug.clone()))
        .or_else(|| {
            plan.steps.first().map(|step| {
                let object = super::lexicon::slug(&step.detail);
                if object.is_empty() {
                    step.op.word().to_owned()
                } else {
                    format!("{}-{object}", step.op.word())
                }
            })
        })
        .unwrap_or_else(|| "compiled-workflow".to_owned());
    let mut kebab = String::new();
    for c in stem.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            kebab.push(c);
        } else if !kebab.ends_with('-') {
            kebab.push('-');
        }
    }
    let mut kebab = kebab.trim_matches('-').chars().take(40).collect::<String>();
    kebab = kebab.trim_matches('-').to_owned();
    if kebab.is_empty() {
        "compiled-workflow".clone_into(&mut kebab);
    }
    format!("{kebab}.nika")
}

/// The effect family a gate covers: writing a file, moving money, or reaching out.
fn effect_family(verb: EffectVerb) -> u8 {
    if verb == EffectVerb::Write {
        0
    } else if verb.moves_money() {
        1
    } else {
        2
    }
}

/// Whether the request states ONE approval for its several gated effects: a single approval
/// phrase in the whole request, or one approval phrase whose own sentence names at least two
/// of the gated effects ("only after I say yes: do the POST, then write the receipt"). Two
/// approval phrases each naming one effect are two gates.
pub(super) fn shared_approval(intent: &str, plan: &Plan) -> bool {
    let lower = intent.to_lowercase();
    if super::gates::gate_phrases(&lower) == 1 {
        return true;
    }
    let gated: Vec<u8> = plan
        .effects
        .iter()
        .filter(|e| e.policy == EffectPolicy::HumanFirst)
        .map(|e| effect_family(e.verb))
        .collect();
    super::lexicon::split_sentences(&lower)
        .into_iter()
        .filter(|sentence| {
            super::gates::final_gate(sentence).is_some()
                || super::gates::named_gate(sentence).is_some()
        })
        .any(|sentence| {
            let named = super::lexicon::effect_words(sentence, &[]);
            gated
                .iter()
                .filter(|family| named.iter().any(|w| effect_family(*w) == **family))
                .count()
                >= 2
        })
}

/// Record a ledger in the decision record (observational, never authority).
pub(super) fn record_ledger(out: &mut CompileOutcome, ledger: &Ledger) {
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    decision["ledger"] = ledger.to_json();
    out.provenance.decision = Some(decision);
}

/// An outbound effect the request repeats per item of its own corpus is not yet compiled
/// (one workflow performs one outbound effect): it is asked, never performed once in
/// silence and never given a phantom `inputs.item`.
pub(super) fn repeated_effect_asked(
    plan: &Plan,
    intent: &str,
    b: &Bindings,
    out: &mut CompileOutcome,
) -> bool {
    let Some(effect) = b.repeated_effect(plan, intent) else {
        return false;
    };
    let trigger = plan.trigger.as_deref().unwrap_or_default().trim();
    super::finding(
        out,
        DiagnosticKind::Unknown,
        effect.verb.word(),
        format!(
            "The request repeats `{}` once per item (`{trigger}`, {}), but a compiled workflow performs an outbound effect once over the whole result; a per-item effect is not built yet. Name the one effect over the selected items, or one request per item.",
            effect.verb.word(),
            effect.evidence.trim()
        ),
    );
    super::question(
        out,
        "intent.clarification",
        "Supply a complete replacement request that performs the effect once over the selected items, or one request per item. It explicitly replaces the earlier intent.",
        QuestionType::Text,
    );
    true
}

/// Give every stated duty the element that carries it, or leave it unresolved. A
/// transformation, an effect and its gate are carried by the task the emitter recorded; a
/// format, a cardinality or an identity by the structure that consumed it or by every
/// language task's prompt guidance (a bound stated to a prompt is carried, not verified:
/// the note says so); a distributive trigger by the per-item fan-out, the incoming item,
/// the fan-out read or the row computation; a safeguard by the task that enforces it.
fn realize(ledger: &mut Ledger, plan: &Plan, b: &Bindings, d: &Doc, trigger_stated: bool) {
    let has_task = |id: &str| d.root["tasks"].get(id).is_some();
    let retried = d.root["tasks"]
        .as_object()
        .into_iter()
        .flat_map(|tasks| tasks.iter())
        .find(|(_, node)| node.get("retry").is_some())
        .map(|(id, _)| id.clone());
    let trigger_carrier = if b.draft_per_item() {
        Some("draft")
    } else if b.item {
        Some("inputs.item")
    } else if b.fan_out() {
        Some("read_source")
    } else if plan.has(Op::Compute) && has_task("compute") {
        Some("compute")
    } else {
        None
    };
    for duty in ledger
        .duties
        .iter_mut()
        .filter(|duty| duty.state == DutyState::Unresolved)
    {
        if let Some((_, _, task)) = d
            .carriers
            .iter()
            .find(|(kind, evidence, _)| *kind == duty.kind && *evidence == duty.evidence)
        {
            duty.realize(task, None);
            continue;
        }
        match duty.kind {
            DutyKind::Format | DutyKind::Cardinality | DutyKind::Identity => {
                realize_format(duty, plan, b, d, trigger_carrier);
            }
            DutyKind::Safeguard => {
                let obligation = plan
                    .obligations
                    .iter()
                    .find(|o| o.evidence.trim() == duty.evidence);
                let carrier = match obligation.map(|o| o.kind.word()) {
                    Some("dedup") if has_task("dedup_admit") => Some("dedup_admit".to_owned()),
                    Some("revision_check") if has_task("revision_admit") => {
                        Some("revision_admit".to_owned())
                    }
                    Some("retry_bound") => retried.clone(),
                    _ => None,
                };
                if let Some(carrier) = carrier {
                    duty.realize(&carrier, None);
                }
            }
            DutyKind::Trigger => {
                if trigger_stated {
                    duty.realize(
                        "requested_trigger",
                        Some("requires binding outside the program bytes"),
                    );
                }
            }
            DutyKind::Structure => realize_structure(duty, b, d),
            DutyKind::Work => {
                let answered = matches!(b.rule.bound(), Some(RuleBinding::Answered(_)));
                realize_selection(duty, plan, has_task("compute"), answered);
            }
            DutyKind::Transformation
            | DutyKind::Filter
            | DutyKind::Count
            | DutyKind::Order
            | DutyKind::Limit
            | DutyKind::Effect
            | DutyKind::Gate
            | DutyKind::Context => {}
        }
    }
}

/// A selection of rows stated as a constraint (R4 A10) is claimed by the compute task when the
/// compute step states each of its clauses (in its detail, its evidence or one of its rules):
/// the constraint restates the computation. Words restating it are a label, not what the
/// program does: the claim waits for a judgment of the candidate (R4 A11). A program the human
/// answered for that step is the human's own and closes it as every answered duty is closed.
/// Anywhere else it stays unresolved work.
fn realize_selection(duty: &mut Duty, plan: &Plan, computes: bool, answered: bool) {
    let Some(step) = plan.step(Op::Compute) else {
        return;
    };
    if !computes || !super::structure::selection_demand(&duty.evidence) {
        return;
    }
    let stated = |clause: &str| {
        let clause = clause.trim();
        !clause.is_empty()
            && (step.detail.contains(clause)
                || step.evidence.contains(clause)
                || plan.rules.iter().any(|rule| rule.text().contains(clause)))
    };
    if !duty.evidence.split([',', ';']).all(stated) {
        return;
    }
    if answered {
        duty.realize(
            "compute",
            Some("the human's answered program runs the computation that restates it"),
        );
        duty.witness = Some(WitnessKind::Answered);
    } else {
        duty.claim(
            "compute",
            "restates the computation the compute step runs",
            WitnessKind::Label,
        );
    }
}

/// The computation's duties typed by what the request's own words state (R4 A3). The compute
/// step's generic filter duty gives way to one duty per operation its anchored parts read
/// (filter, count, order, limit), in request order, each realized only when the task running
/// the bound rule emits that rule's lowering byte for byte and the rule holds the operation,
/// with its parameters, after the operation before it: never by a task id, a record flag or a
/// plan annotation. A part the reader cannot read, or a program answered as written, keeps its
/// excerpt in the ledger, carried by the task and said to be unverified.
fn type_computation(ledger: &mut Ledger, plan: &Plan, b: &Bindings, d: &Doc) {
    let Some(step) = plan.step(Op::Compute) else {
        return;
    };
    let evidence = super::ledger::step_evidence(step).trim().to_owned();
    let generic = |duty: &Duty| duty.kind == DutyKind::Filter && duty.evidence == evidence;
    let stated = ledger.duties.iter().position(generic);
    let mut typed = match (b.rule.bound(), &b.witness) {
        (Some(RuleBinding::Synthesized(rule)), Some(witness)) => {
            typed_duties(rule, witness, d, &evidence)
        }
        (Some(RuleBinding::Answered(_)), _) if stated.is_some() => {
            vec![Duty::answered(DutyKind::Filter, &evidence, "compute")]
        }
        // Nothing bound: the stated duty stays as the plan states it.
        _ => return,
    };
    let carried = |duty: &Duty| duty.kind == DutyKind::Transformation && duty.evidence == evidence;
    // The step's own words unread: its transformation duty says so, never a second duty.
    if let Some(unread) = typed.iter().position(carried)
        && let Some(own) = ledger.duties.iter_mut().find(|duty| carried(duty))
    {
        *own = typed.remove(unread);
    }
    let at = stated
        .or_else(|| ledger.duties.iter().position(carried).map(|at| at + 1))
        .unwrap_or(ledger.duties.len());
    ledger.duties.retain(|duty| !generic(duty));
    ledger.duties.splice(at..at, typed);
}

/// The typed duties of a bound computation (R4 A3), realized against the emitted bytes. When
/// every part is read, each operation shaping the rows that the bound rule runs beyond them is
/// an unresolved duty on the step's `evidence`: an unstated operation is never harmless.
fn typed_duties(
    rule: &super::rules::Rule,
    witness: &Witness,
    d: &Doc,
    evidence: &str,
) -> Vec<Duty> {
    let expression = |task: &str| d.root["tasks"][task]["invoke"]["args"]["expression"].as_str();
    let lowered = super::laws::with_decimal(&rule.jq());
    let runs = expression("compute") == Some(lowered.as_str());
    let summarizes = expression("compute_summary") == Some(super::laws::SUMMARY);
    let mut expected: Vec<(&str, Operation)> = Vec::new();
    let mut unread = Vec::new();
    // An identity the reader's conversion law states from these very words (the law a replay
    // binds it by) is read, with no operation to hold (R4 A11).
    let converts = !rule.filters()
        && !rule.shaped()
        && !rule.summary()
        && !rule.lines()
        && super::lexicon::read(rule.text()).plan.rules.contains(rule);
    for part in &witness.parts {
        match &part.reading {
            Some(reading) => {
                let anchor = part.anchor.as_str();
                expected.extend(operations(reading).into_iter().map(|op| (anchor, op)));
            }
            None if converts => unread.push(Duty::conversion(&part.anchor, "compute", runs)),
            None => unread.push(Duty::unverified(
                DutyKind::Transformation,
                &part.anchor,
                "compute",
                "no deterministic reading of these words; the task carries them unchecked",
            )),
        }
    }
    let wanted: Vec<Operation> = expected.iter().map(|(_, op)| op.clone()).collect();
    let chosen = operations(&witness.chosen);
    let places = found(&chosen, &wanted);
    let mut duties = Vec::new();
    let mut unstated = Vec::new();
    if read_whole(&witness.parts) {
        let matched: Vec<usize> = places.iter().flatten().copied().collect();
        let extra = chosen
            .iter()
            .enumerate()
            .filter(|(at, op)| op.shapes_rows() && !matched.contains(at));
        unstated.extend(extra.map(|(_, op)| Duty::unstated(op.kind(), evidence, op.reads())));
    }
    for (position, ((anchor, op), place)) in expected.into_iter().zip(places).enumerate() {
        let mut duty = Duty::typed(op.kind(), anchor, position, op.reads());
        let (task, emitted) = if op == Operation::Summary {
            ("compute_summary", summarizes)
        } else {
            ("compute", runs)
        };
        match (place, emitted) {
            (Some(_), true) => {
                duty.realize(task, None);
                duty.witness = Some(WitnessKind::Typed);
            }
            (Some(_), false) => {
                duty.note = Some(format!(
                    "`{task}` does not run the lowering of the bound rule"
                ));
            }
            (None, _) => {
                duty.note = Some(
                    "the emitted computation does not hold this operation, with its parameters, at its place in the stated order".to_owned(),
                );
            }
        }
        duties.push(duty);
    }
    duties.extend(unstated);
    duties.extend(unread);
    duties
}

/// A format, a cardinality or an identity is realized by what verifies it at run, by the
/// trigger's carrier, by the structure that consumed it, by the compute task that keeps the
/// source columns, or as prompt guidance of the first language step.
fn realize_format(
    duty: &mut Duty,
    plan: &Plan,
    b: &Bindings,
    d: &Doc,
    trigger_carrier: Option<&str>,
) {
    let has_task = |id: &str| d.root["tasks"].get(id).is_some();
    if let Some((_, task)) = d
        .verified_bounds
        .iter()
        .find(|(constraint, _)| *constraint == duty.evidence)
    {
        duty.realize(task, Some("verified at run"));
    } else if plan.trigger.as_deref().map(str::trim) == Some(duty.evidence.as_str()) {
        if let Some(carrier) = trigger_carrier {
            duty.realize(carrier, Some("once per item of the material"));
        }
    } else if b.consumed.contains(&duty.evidence) {
        // A concurrency bound lives on the fan-out; order and headings on the fold.
        let carrier = if super::cardinality::parallel_bound(&duty.evidence).is_some() {
            "for_each"
        } else {
            "draft_fold"
        };
        duty.realize(carrier, Some("realized by the structure"));
    } else if matches!(duty.kind, DutyKind::Format | DutyKind::Identity)
        && has_task("compute")
        && (keeps_columns(&duty.evidence)
            || super::rules::keeps_order(&duty.evidence)
            || names_computed_column(&duty.evidence, d)
            || stated_by_rule(&duty.evidence, plan))
    {
        duty.realize(
            "compute",
            Some("the computed rows carry the columns the request names"),
        );
    } else if let Some(task) = d.infer_tasks.first() {
        let note = match duty.kind {
            DutyKind::Cardinality => "prompt guidance; not verified at run",
            _ => "prompt guidance",
        };
        duty.realize(task, Some(note));
    }
}

/// A format that names a column the computation outputs (« exactly two columns,
/// `roast_level` and `total_kg` », « one row per roast level ») is carried by the compute
/// task: the projection, the grouping or the aggregation produced that very column.
fn names_computed_column(constraint: &str, d: &Doc) -> bool {
    let folded = super::shape::fold(constraint);
    d.computed_columns.iter().flatten().any(|column| {
        let column = super::shape::fold(column);
        let spaced = column.replace('_', " ");
        !column.is_empty() && (folded.contains(&column) || folded.contains(&spaced))
    })
}

/// A format the rule itself states: the rule's own words (« los tres corredores más
/// rápidos (menor `tiempo_seg`) » promoted from the clause the rule was read from), or an
/// ordering phrase (« del más rápido al más lento », « highest first », « croissant ») when
/// the rule sorts. The compute task carries both.
fn stated_by_rule(constraint: &str, plan: &Plan) -> bool {
    let fold = |text: &str| {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let wanted = fold(constraint);
    if wanted.is_empty() {
        return false;
    }
    let inside_a_rule = plan.rules.iter().any(|rule| {
        let text = fold(rule.text());
        !text.is_empty() && (text.contains(&wanted) || wanted.contains(&text))
    });
    if inside_a_rule {
        return true;
    }
    let sorted = plan
        .rules
        .iter()
        .any(|rule| !rule.to_json()["shape"]["sort_by"].is_null());
    sorted && ordering_phrase(&super::shape::fold(constraint))
}

/// An ordering phrase in six languages, folded.
fn ordering_phrase(folded: &str) -> bool {
    [
        "ascending",
        "descending",
        "highest first",
        "lowest first",
        "largest first",
        "smallest first",
        "croissant",
        "decroissant",
        "du plus",
        "de la plus",
        "del mas",
        "de mayor a menor",
        "de menor a mayor",
        "dal piu",
        "dalla piu",
        "aufsteigend",
        "absteigend",
        "vom hochsten",
        "vom niedrigsten",
        "do maior",
        "do menor",
        "crescente",
        "decrescente",
    ]
    .iter()
    .any(|cue| folded.contains(cue))
}

/// A format the computed rows honour by construction: « avec les mêmes colonnes et dans le
/// même ordre », « mismas columnas », « stesse colonne », « denselben Spalten », « same
/// columns », « mesma ordem ». A filter keeps every column of every row it keeps, in the
/// order they came; a projection keeps the columns it names, in the order it names them.
fn keeps_columns(constraint: &str) -> bool {
    let folded = super::shape::fold(constraint);
    [
        "same column",
        "same order",
        "same row order",
        "columns unchanged",
        "memes colonnes",
        "meme ordre",
        "colonnes identiques",
        "mismas columnas",
        "mismo orden",
        "stesse colonne",
        "stesso ordine",
        "denselben spalten",
        "dieselben spalten",
        "gleichen spalten",
        "gleiche spalten",
        "selben reihenfolge",
        "gleicher reihenfolge",
        "gleiche reihenfolge",
        "mesmas colunas",
        "mesma ordem",
    ]
    .iter()
    .any(|phrase| folded.contains(phrase))
}

/// A structure law is realized by the emitted shape or left unresolved with the reason: a
/// closure and « no other file » hold by construction (the compiler emits only the stated
/// steps and destinations); « no language model » holds when no step infers; « a single
/// request » holds when exactly one outbound effect is sent once.
fn realize_structure(duty: &mut Duty, b: &Bindings, d: &Doc) {
    use super::structure::Law;
    let mut broken: Option<&str> = None;
    for law in super::structure::laws(&duty.evidence) {
        let breaks = match law {
            Law::NoModel => !d.infer_tasks.is_empty(),
            Law::SingleRequest => b.wired.len() != 1 || !b.per_item.is_empty(),
            Law::NothingElse | Law::NoOtherFile => false,
        };
        if breaks {
            broken = Some(if law == Law::NoModel {
                "the request forbids a language model, yet a step produces content only a model drafts"
            } else {
                "the request wants one outbound request, yet the workflow sends none or one per item"
            });
        }
    }
    match broken {
        None => duty.realize(
            "the emitted shape",
            Some("by construction: only the stated steps, destinations and calls are emitted"),
        ),
        Some(note) => duty.note = Some(note.to_owned()),
    }
}

/// Two bounds the request states on one unit of its content that cannot both hold ("exactly
/// 5 lines" and "at least 12 lines") are a contradiction: the request is refused as stated,
/// never run on a prompt that silently obeys one of them.
pub(super) fn refused_contradiction(ledger: &Ledger, out: &mut CompileOutcome) -> bool {
    let contradicted: Vec<&Duty> = ledger
        .contradicted()
        .filter(|d| d.kind == DutyKind::Cardinality)
        .collect();
    let [first, second, ..] = contradicted.as_slice() else {
        return false;
    };
    out.status = super::CompileStatus::Refused;
    super::finding(
        out,
        DiagnosticKind::RequiresHuman,
        "intent",
        format!(
            "Contradictory bounds on the produced content: `{}` and `{}` cannot both hold. The contradiction stays visible; no workflow resolves it.",
            first.evidence, second.evidence
        ),
    );
    super::question(
        out,
        "intent.clarification",
        "Supply a complete replacement request whose bounds on the produced content can all hold at once. It explicitly replaces the earlier intent.",
        QuestionType::Text,
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::synthesize;
    use DutyKind::{Count, Filter, Limit, Order, Transformation};
    use DutyState::{Pending, Realized, Unresolved};

    const COUNT: &str = "count the rows where status is paid";
    const PAID: &str = "keep the rows where status is paid";
    const TOP: &str = "keep the 2 rows with the highest amount_usd";

    /// The typed duties `stated` puts on a bound rule read from `bound`, whose lowering the
    /// compute task emits unless `emitted` stands in its place.
    fn witnessed(stated: &[&str], bound: &str, emitted: Option<&str>) -> Vec<Duty> {
        let bound = synthesize(bound, &[]).expect("the bound rule reads");
        let lowered = crate::laws::with_decimal(&bound.jq());
        let mut d = Doc::new("witness", false);
        let expression = emitted.unwrap_or(&lowered);
        d.root["tasks"]["compute"] = json!({"invoke": {"args": {"expression": expression}}});
        typed_duties(&bound, &Witness::of(stated, bound.clone()), &d, "the step")
    }

    fn states(duties: &[Duty]) -> Vec<(DutyKind, DutyState, Option<usize>)> {
        duties
            .iter()
            .map(|d| (d.kind, d.state, d.position))
            .collect()
    }

    /// A witness compares the parameters of each stated operation, never its kind alone (R4 A3):
    /// with no reading to fall back on, each changed parameter leaves its own duty unresolved,
    /// the changed operation is named as one the request does not state, and every other duty is
    /// realized (a missing operation never shifts the others).
    #[test]
    fn a_witness_compares_the_parameters_of_each_stated_operation() {
        let both = [(Filter, Realized, Some(0)), (Count, Realized, Some(1))];
        assert_eq!(states(&witnessed(&[COUNT], COUNT, None)), both);
        // A count-free rule compiled faithfully.
        let count_free = [(Filter, Realized, Some(0)), (Count, Unresolved, Some(1))];
        assert_eq!(states(&witnessed(&[COUNT], PAID, None)), count_free);
        // Another field, comparator or literal.
        let filter = [
            (Filter, Unresolved, Some(0)),
            (Count, Realized, Some(1)),
            (Filter, Unresolved, None),
        ];
        for other in [
            "count the rows where state is paid",
            "count the rows where status is not paid",
            "count the rows where status is open",
        ] {
            assert_eq!(states(&witnessed(&[COUNT], other, None)), filter, "{other}");
        }
        // A flipped direction, a cut of 3.
        let lowest = "keep the 2 rows with the lowest amount_usd";
        let order = [
            (Order, Unresolved, Some(0)),
            (Limit, Realized, Some(1)),
            (Order, Unresolved, None),
        ];
        assert_eq!(states(&witnessed(&[TOP], lowest, None)), order);
        let three = "keep the 3 rows with the highest amount_usd";
        let limit = [
            (Order, Realized, Some(0)),
            (Limit, Unresolved, Some(1)),
            (Limit, Unresolved, None),
        ];
        assert_eq!(states(&witnessed(&[TOP], three, None)), limit);
        // A later step omitted, or moved before the cut; kept in the stated order, all hold.
        let later = [
            (Order, Realized, Some(0)),
            (Limit, Realized, Some(1)),
            (Filter, Unresolved, Some(2)),
        ];
        assert_eq!(states(&witnessed(&[TOP, PAID], TOP, None)), later);
        let moved = format!("{PAID} ; {TOP}");
        let first = states(&witnessed(&[TOP, PAID], &moved, None));
        assert_eq!(first[..3], later);
        assert_eq!(first[3..], [(Filter, Unresolved, None)]);
        let kept = format!("{TOP} ; {PAID}");
        let all = [
            (Order, Realized, Some(0)),
            (Limit, Realized, Some(1)),
            (Filter, Realized, Some(2)),
        ];
        assert_eq!(states(&witnessed(&[TOP, PAID], &kept, None)), all);
    }

    /// A label realizes nothing (R4 A3): a task named `compute` that runs another program leaves
    /// every duty unresolved. Words the grammar cannot read state no typed duty; they stay in the
    /// ledger, claimed by the task, said to be unverified, pending a judgment (R4 A11).
    #[test]
    fn only_the_emitted_computation_realizes_a_typed_duty() {
        let other = witnessed(
            &[COUNT],
            COUNT,
            Some("[.records[] | select(.status == \"paid\")]"),
        );
        let nothing = [(Filter, Unresolved, Some(0)), (Count, Unresolved, Some(1))];
        assert_eq!(states(&other), nothing);
        let why = "`compute` does not run the lowering of the bound rule";
        assert!(
            other.iter().all(|d| d.note.as_deref() == Some(why)),
            "{other:?}"
        );
        let unread = witnessed(&["tally whatever looks settled"], COUNT, None);
        assert_eq!(states(&unread), [(Transformation, Pending, None)]);
        assert_eq!(unread[0].realized_by.as_deref(), Some("compute"));
        let note = unread[0].note.as_deref().unwrap_or_default();
        assert!(note.starts_with("unverified: "), "{note}");
    }

    /// An operation the request does not state is never harmless (R4 A3): with every part read,
    /// each operation the bound rule runs beyond them is an unresolved duty, even around a stated
    /// order that still holds. With a part unread, its words may state it, and it is not judged.
    #[test]
    fn an_unstated_operation_is_an_unresolved_duty() {
        let around = witnessed(&[TOP, PAID], &format!("{PAID} ; {TOP} ; {PAID}"), None);
        let held = [
            (Order, Realized, Some(0)),
            (Limit, Realized, Some(1)),
            (Filter, Realized, Some(2)),
        ];
        assert_eq!(states(&around)[..3], held);
        assert_eq!(states(&around)[3..], [(Filter, Unresolved, None)]);
        let why = around[3].note.as_deref().unwrap_or_default();
        assert!(why.contains("the request does not state"), "{why}");
        let over = "keep the rows where amount_usd is over 10";
        let added = witnessed(&[TOP, PAID], &format!("{TOP} ; {PAID} ; {over}"), None);
        assert_eq!(states(&added)[3..], [(Filter, Unresolved, None)]);
        assert_eq!(added[3].reads, ["amount_usd"]);
        let unread = ["tally whatever looks settled", TOP];
        let open = witnessed(&unread, &format!("{PAID} ; {TOP}"), None);
        let judged = [
            (Order, Realized, Some(0)),
            (Limit, Realized, Some(1)),
            (Transformation, Pending, None),
        ];
        assert_eq!(states(&open), judged);
    }
}
