// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The READY law over the assembled workflow: every duty the request states is carried by a
//! named element or the candidate is not emitted; one approval clause covering several
//! effects is one gate; an effect repeated per item is asked; two bounds that cannot both
//! hold are refused. The ledger rides in provenance either way, observational, never
//! authority.

use super::assemble::{Doc, Laws, emit};
use super::bindings::{Bindings, RuleBinding};
use super::ledger::{Duty, DutyKind, DutyState, Ledger};
use super::plan::{EffectPolicy, EffectVerb, Op, Plan};
use super::shape::Shape;
use super::{CompileError, CompileOutcome, DiagnosticKind, QuestionType};
use serde_json::json;

/// The realized topology and the READY law, then emission: every duty the request states is
/// carried by a named element, or the candidate is not emitted. The ledger rides in the
/// decision record either way.
pub(super) fn settle_candidate(
    plan: &Plan,
    b: &Bindings,
    d: Doc,
    laws: &Laws<'_>,
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
    realize(&mut ledger, plan, b, &d, out.requested_trigger.is_some());
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
    emit(d, laws, out)
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
            DutyKind::Transformation
            | DutyKind::Filter
            | DutyKind::Effect
            | DutyKind::Gate
            | DutyKind::Work
            | DutyKind::Context => {}
        }
    }
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
