// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The parts of a requested result a model must write (a summary, a classification, a
//! translation, a draft), settled against the candidate's data paths. The judge states the
//! parts; the engine's pure walk
//! ([`model_steps_on_paths`](nika_compile_fidelity::model_paths::model_steps_on_paths)) finds what
//! the bytes do with each:
//! - a provider judge lists them with its whole-request answer
//!   ([`AlsoProperty`](nika_compile_seats::decide::AlsoProperty)): the person's
//!   words (an item whose words the request does not hold carries nothing), the result they
//!   shape, and the sink it chose among the candidate's own;
//! - a seat that weighs its answers is asked, in the same request as the whole question, whether
//!   a model must write what each sink receives, once in each option order: the two
//!   distributions are averaged by key, which cancels a lean toward the first option, and a part
//!   is stated at an averaged probability of 0.6 or more, a margin of 0.1 over even odds.
//!
//! A part no model step writes (missing), or whose only model step feeds no result
//! (decorative), is a defect the repairs start from, and these bytes are declined; a part behind
//! a child workflow is stated, never held. A judge that states nothing, or nothing usable, a seat
//! taken at its word, or sinks both orders leave undecided leave the law unrun (`None`), never
//! passed, and the outcome says why (`model_parts_unrun`).

use nika_compile::{ModelFound, ModelPart, PartBasis};
use nika_compile_fidelity::model_paths::{ModelPaths, ModelSink, model_steps_on_paths};
use nika_kernel::ai::provider::ProviderInferDyn;
use serde_json::{Value, json};

use super::{Declined, Judge, Verdict, ask};
use crate::CompileOutcome;
use crate::decide::{AlsoProperty, ChoiceOption, ChoiceQuestion, system_one::Order};
use crate::lexicon;
use crate::paths::{self, PathShape};

/// The whole-request question: its answer carries the provider judge's statement.
const REQUEST: &str = "verify-request";
/// The property a provider judge returns with its whole-request answer.
const NAME: &str = "model_parts";
/// What the provider judge is told about the property.
const STATEMENT: &str = "Also list in `model_parts` each part of the requested result that a model must write (to summarize, classify, translate, draft…): `words` quotes the request verbatim, `result` says which result it shapes, and `sink` is where that result leaves the workflow, or `none`. An empty list means the request needs no model-written part.";
/// What a weighing seat is asked of each sink, once in each option order.
const SINK: &str = "Does the content this sink receives need a language model to write it, from what the request asks?";
/// A sink question's options: computed by a program, or written by a model.
const COMPUTED: (&str, &str) = (
    "computed",
    "a program fetches, copies or computes it; no model writes it",
);
const MODEL: (&str, &str) = (
    "model",
    "a model must write it: text, a summary, a judgment or a choice the request asks for",
);
/// The averaged probability that states a sink's model-written part, or that it has none.
const THRESHOLD: f64 = 0.6;
/// A sink question's id before the sink's index; the reversed order appends `-r`.
const SINK_ID: &str = "verify-sink-";
/// How the outcome says the law did not run with this judge.
const UNRUN: &str = "model-step check not run: ";
/// Why the law does not run with a seat taken at its word.
const UNWEIGHED: &str = "the judge seat does not weigh its answers";

/// The walk of the candidate's bytes: `None` when they do not parse, and no part is judged.
pub(super) fn walk(candidate: &str) -> Option<ModelPaths> {
    model_steps_on_paths(candidate)
}

/// What the whole question says of the walk's fact it carries ([`steps`]).
pub(super) const MODEL_STEPS: &str = "`model_steps` lists, for each result the workflow writes (`task`: the step that writes it) or returns (`output`: a value handed back to its caller, which writes nothing), the model steps on its data path, as the engine's walk finds them in these bytes: facts, not claims.";

/// The walk's fact the whole question carries: each sink in file order, named by the task that
/// writes it or the output key that returns it, with the model steps on its data path.
pub(super) fn steps(paths: &ModelPaths) -> Value {
    let on = |sink: &ModelSink| -> Vec<&str> {
        (paths.steps.iter())
            .filter(|(_, reached)| reached == sink)
            .map(|(task, _)| task.as_str())
            .collect()
    };
    (paths.sinks.iter())
        .map(|sink| {
            let named = match sink {
                ModelSink::Task(id) => json!({"task": id}),
                ModelSink::Output(key) => json!({"output": key}),
                _ => Value::Null,
            };
            json!({"sink": named, "model_steps": on(sink)})
        })
        .collect()
}

/// The whole-request question as `judge` is asked it: a provider judge returns the parts with
/// its answer, each sink chosen among the candidate's own; any other judge answers the choice
/// alone, its bytes unchanged.
pub(super) fn asked<P: ProviderInferDyn>(
    judge: &Judge<'_, P>,
    paths: Option<&ModelPaths>,
    question: ChoiceQuestion,
) -> ChoiceQuestion {
    match (judge, paths) {
        (Judge::Provider(..), Some(paths)) => {
            let schema = schema(&labels(paths));
            question.with_also(AlsoProperty::new(NAME, schema, STATEMENT))
        }
        _ => question,
    }
}

/// The question a seat that weighs its answers is asked of each sink, once in the options' own
/// order (`verify-sink-<k>`) and once reversed (`verify-sink-<k>-r`): the request state, the
/// sink (its task and the literal it writes, or its output key) and the request sentence that
/// names that literal. None for any other judge, or bytes with no sink.
pub(super) fn sink_questions<P: ProviderInferDyn>(
    judge: &Judge<'_, P>,
    paths: Option<&ModelPaths>,
    base: &Value,
    intent: &str,
) -> Vec<ChoiceQuestion> {
    let (Judge::Seat(seat), Some(paths)) = (judge, paths) else {
        return Vec::new();
    };
    if !seat.weighs() {
        return Vec::new();
    }
    let mut asked = Vec::new();
    for (k, sink) in paths.sinks.iter().enumerate() {
        let mut state = base.clone();
        state["sink"] = sink_entry(sink, paths);
        state["cited"] = json!(destination(paths, sink).and_then(|to| cited(intent, to)));
        let options = vec![
            ChoiceOption::new(COMPUTED.0, COMPUTED.1),
            ChoiceOption::new(MODEL.0, MODEL.1),
        ];
        let question = ChoiceQuestion::new(format!("{SINK_ID}{k}"), SINK, state, options);
        let mut reversed = question.clone().in_order(Order::Reversed);
        reversed.id = format!("{SINK_ID}{k}-r");
        asked.push(question.in_order(Order::Question));
        asked.push(reversed);
    }
    asked
}

/// Each sink question's answer, taken from what the seat answered with the whole question (no
/// call of its own), recorded under the role `judge_sink`.
pub(super) async fn take<P: ProviderInferDyn>(
    judge: &Judge<'_, P>,
    sinks: &[ChoiceQuestion],
    verdict: &mut Verdict,
    out: &mut CompileOutcome,
) {
    for question in sinks {
        ask(judge, question, "judge_sink", verdict, out).await;
    }
}

/// The parts the judge stated over these bytes, each with what the bytes do with it, into
/// `verdict`: a part no model step writes, or whose model step feeds no result, held as a
/// defect. When the law cannot run with this judge, why, never silent ([`recorded`]). Nothing
/// when the bytes do not parse: no judge reads them.
pub(super) fn settle<P: ProviderInferDyn>(
    intent: &str,
    judge: &Judge<'_, P>,
    paths: Option<&ModelPaths>,
    sinks: &[ChoiceQuestion],
    verdict: &mut Verdict,
) {
    let Some(paths) = paths else {
        return;
    };
    let parts = match judge {
        Judge::Provider(..) => stated(intent, paths, verdict),
        Judge::Seat(seat) if !seat.weighs() => Err(UNWEIGHED.to_owned()),
        Judge::Seat(_) => weighed(paths, sinks, &verdict.records),
    };
    match parts {
        Ok(parts) => {
            for part in &parts {
                hold(part, paths, verdict);
            }
            verdict.model_parts = Some(parts);
        }
        Err(why) => verdict.model_parts_unrun = Some(format!("{UNRUN}{why}")),
    }
}

/// What an attempt records of the law, and what the outcome says: the parts as judged, or why
/// the law did not run with this judge, so its absence is never silent. The outcome keeps the
/// latest attempt's.
pub(super) fn recorded(verdict: &Verdict, entry: &mut Value, out: &mut CompileOutcome) {
    if let Some(parts) = &verdict.model_parts {
        entry["model_parts"] = document(parts);
    }
    if let Some(why) = &verdict.model_parts_unrun {
        entry["model_parts_unrun"] = json!(why);
    }
    out.model_parts.clone_from(&verdict.model_parts);
    out.model_parts_unrun.clone_from(&verdict.model_parts_unrun);
}

/// The provider judge's statement, from its whole-request answer (or the earlier answer it reads
/// back), or why there is none: no answer, no statement, or a malformed one, whose reason the
/// answer's record also keeps.
fn stated(
    intent: &str,
    paths: &ModelPaths,
    verdict: &mut Verdict,
) -> Result<Vec<ModelPart>, String> {
    let at = (verdict.records.iter()).rposition(|record| record["question"] == REQUEST);
    let record = at
        .and_then(|at| verdict.records.get(at))
        .filter(|r| r["choice"].is_string());
    let Some((at, record)) = at.zip(record) else {
        return Err("the judge did not answer the whole request".to_owned());
    };
    let answer = if record["read_back"] == Value::Bool(true) {
        let earlier = (verdict.earlier.iter().rev()).find(|earlier| earlier["question"] == REQUEST);
        earlier
            .and_then(|earlier| earlier["also"].get(NAME))
            .cloned()
    } else {
        record["also"].get(NAME).cloned()
    };
    let Some(answer) = answer else {
        return Err("the judge stated no model parts".to_owned());
    };
    settled(intent, paths, &answer).map_err(|why| {
        if let Some(record) = verdict.records.get_mut(at) {
            record["model_parts_refused"] = json!(why);
        }
        format!("the judge's model parts were malformed ({why})")
    })
}

/// The parts a weighing seat's sink answers state: a sink whose averaged p(model) reaches the
/// threshold has one; a sink whose p(computed) does has none; any other leaves its sink open.
/// Why none is stated when the bytes have no sink, the seat answered no sink question, or no
/// part was found while a sink stayed open.
fn weighed(
    paths: &ModelPaths,
    sinks: &[ChoiceQuestion],
    records: &[Value],
) -> Result<Vec<ModelPart>, String> {
    if sinks.is_empty() {
        return Err("the candidate has no result sink to ask about".to_owned());
    }
    let (mut parts, mut open, mut answered) = (Vec::new(), Vec::new(), false);
    for (k, sink) in paths.sinks.iter().enumerate() {
        let cited = (sinks.get(2 * k)).and_then(|question| question.state["cited"].as_str());
        let (model, computed) = (
            averaged(records, k, MODEL.0),
            averaged(records, k, COMPUTED.0),
        );
        answered |= model.is_some();
        match (model, computed) {
            (Some(p), _) if p >= THRESHOLD => {
                let basis = PartBasis::Sink {
                    cited: cited.map(str::to_owned),
                    p,
                };
                parts.push(ModelPart::new(
                    basis,
                    Some(sink.clone()),
                    found(paths, Some(sink)),
                ));
            }
            (_, Some(p)) if p >= THRESHOLD => {}
            _ => open.push(shown(sink, paths)),
        }
    }
    if !answered {
        return Err("the seat answered no sink question in both option orders".to_owned());
    }
    if parts.is_empty() && !open.is_empty() {
        let open = open.join(", ");
        return Err(format!("both option orders left {open} undecided"));
    }
    Ok(parts)
}

/// The probability `key` gets over both orders of sink `k`'s question: the two admitted answers'
/// distributions averaged; `None` unless both answered.
fn averaged(records: &[Value], k: usize, key: &str) -> Option<f64> {
    let mut sum = 0.0;
    for id in [format!("{SINK_ID}{k}"), format!("{SINK_ID}{k}-r")] {
        let record = (records.iter().rev()).find(|record| record["question"] == id.as_str())?;
        sum += chance(record, key)?;
    }
    Some(sum / 2.0)
}

/// The probability an admitted answer gives `key`: its reported distribution, or its choice
/// taken at its word when it weighed nothing; `None` for a question with no admitted answer.
fn chance(record: &Value, key: &str) -> Option<f64> {
    let choice = record["choice"].as_str()?;
    let reported = record["probabilities"]
        .as_object()
        .filter(|p| !p.is_empty());
    Some(match reported {
        Some(probabilities) => probabilities
            .get(key)
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        None if choice == key => 1.0,
        None => 0.0,
    })
}

/// The labels a provider judge chooses a sink from: the candidate's sinks in file order, then
/// `none`.
fn labels(paths: &ModelPaths) -> Vec<String> {
    (paths.sinks.iter())
        .map(label)
        .chain(std::iter::once("none".to_owned()))
        .collect()
}

/// The label a sink is offered under: `task:<id>` or `output:<key>`.
fn label(sink: &ModelSink) -> String {
    match sink {
        ModelSink::Task(id) => format!("task:{id}"),
        ModelSink::Output(key) => format!("output:{key}"),
        _ => String::new(),
    }
}

/// The property's schema: a list of `{words, result, sink}`, the sink one of `labels`.
fn schema(labels: &[String]) -> Value {
    json!({
        "type": "array",
        "items": {
            "type": "object",
            "additionalProperties": false,
            "required": ["words", "result", "sink"],
            "properties": {
                "words": {"type": "string"},
                "result": {"type": "string"},
                "sink": {"type": "string", "enum": labels}
            }
        }
    })
}

/// The provider judge's `model_parts`, settled: the parts whose words the request holds, each
/// with what the candidate's data paths do with it.
///
/// # Errors
/// Why the statement is malformed: not a list, an item without a key, or a sink not offered.
fn settled(intent: &str, paths: &ModelPaths, answer: &Value) -> Result<Vec<ModelPart>, String> {
    let items = (answer.as_array()).ok_or("`model_parts` is not a list")?;
    let request = normalized(intent);
    let mut parts = Vec::new();
    for item in items {
        let text = |key: &str| {
            (item.get(key).and_then(Value::as_str))
                .ok_or_else(|| format!("a model part has no `{key}`"))
        };
        let (words, result, chosen) = (text("words")?, text("result")?, text("sink")?);
        let sink = if chosen == "none" {
            None
        } else {
            let offered = (paths.sinks.iter()).find(|sink| label(sink) == chosen);
            let offered = offered.ok_or_else(|| format!("the sink `{chosen}` was not offered"))?;
            Some(offered.clone())
        };
        let quoted = normalized(words);
        if quoted.is_empty() || !request.contains(&quoted) {
            continue;
        }
        let found = found(paths, sink.as_ref());
        let basis = PartBasis::Words {
            words: words.to_owned(),
            result: result.to_owned(),
        };
        parts.push(ModelPart::new(basis, sink, found));
    }
    Ok(parts)
}

/// What the data paths do with a part shaped at `sink`: a model step on its path meets it, a
/// child call on its path leaves it undecided, else a model step that reaches no sink is
/// decorative, else nothing writes it.
fn found(paths: &ModelPaths, sink: Option<&ModelSink>) -> ModelFound {
    if let Some(sink) = sink {
        if let Some((task, _)) = (paths.steps.iter()).find(|(_, on)| on == sink) {
            return ModelFound::Met { task: task.clone() };
        }
        if let Some((task, child, _)) = (paths.children.iter()).find(|(_, _, on)| on == sink) {
            return ModelFound::Undecided {
                task: task.clone(),
                child: child.clone(),
            };
        }
    }
    (paths.unreached.first()).map_or(ModelFound::Missing, |task| ModelFound::Decorative {
        task: task.clone(),
    })
}

/// The defect a part leaves when no model step writes it, or its only model step feeds no
/// result: the person's words, or the request sentence that names the sink (else the sink),
/// with the reason as its note. These bytes are declined: never asked of this judge again (R6).
fn hold(part: &ModelPart, paths: &ModelPaths, verdict: &mut Verdict) {
    let target = part.sink.as_ref().map(|sink| shown(sink, paths));
    let lack = match (&part.found, &part.basis, &target) {
        (ModelFound::Missing, PartBasis::Words { .. }, Some(target)) => {
            format!("no model step writes what {target} receives")
        }
        (ModelFound::Missing, ..) => "no model step writes it".to_owned(),
        (ModelFound::Decorative { task }, ..) => {
            format!("the model step `{task}` writes text that no result reads")
        }
        _ => return,
    };
    let (defect, note) = match &part.basis {
        PartBasis::Words { words, .. } => {
            (words.clone(), format!("a model must write it, and {lack}"))
        }
        PartBasis::Sink { cited, p } => {
            let target = target.unwrap_or_default();
            let note = format!(
                "{target} receives content a model must write (p {p:.2} over both option orders), and {lack}"
            );
            (cited.clone().unwrap_or(target), note)
        }
        _ => return,
    };
    if !verdict.defects.contains(&defect) {
        verdict.defects.push(defect.clone());
        verdict.notes.push((defect, note));
    }
    verdict.decline(Declined::Rejected);
}

/// How a sink is named to the person: the literal it writes with its task, its task alone, or
/// its output key.
fn shown(sink: &ModelSink, paths: &ModelPaths) -> String {
    match (sink, destination(paths, sink)) {
        (ModelSink::Task(id), Some(to)) => format!("`{to}` (task `{id}`)"),
        (ModelSink::Task(id), None) => format!("task `{id}`"),
        (ModelSink::Output(key), _) => format!("output `{key}`"),
        _ => "a result".to_owned(),
    }
}

/// What a sink question shows of its sink: its task and the literal it writes, or its key.
fn sink_entry(sink: &ModelSink, paths: &ModelPaths) -> Value {
    match (sink, destination(paths, sink)) {
        (ModelSink::Task(id), Some(to)) => json!({"task": id, "writes": to}),
        (ModelSink::Task(id), None) => json!({"task": id}),
        (ModelSink::Output(key), _) => json!({"output": key}),
        _ => Value::Null,
    }
}

/// The literal path or URL a sink writes or sends to, when its bytes state it whole.
fn destination<'p>(paths: &'p ModelPaths, sink: &ModelSink) -> Option<&'p str> {
    (paths.destinations.iter())
        .find(|(on, _)| on == sink)
        .map(|(_, to)| to.as_str())
}

/// The request sentence that names `to`: the first whose path literals name it (`./` aside, as
/// the reader locates paths), or that holds the URL whole.
fn cited(intent: &str, to: &str) -> Option<String> {
    let wanted = to.trim_start_matches("./");
    let names = |sentence: &str| {
        (to.contains("://") && sentence.contains(to))
            || (paths::literals(sentence).iter()).any(|shape| match shape {
                PathShape::File(text) | PathShape::Directory(text) => {
                    text.trim_start_matches("./") == wanted
                }
                _ => false,
            })
    };
    (lexicon::split_sentences(intent).into_iter())
        .find(|sentence| names(sentence))
        .map(|sentence| sentence.trim().to_owned())
}

/// The quote law's folding (the carriers' and the doubt's): apostrophes folded, ASCII case
/// ignored, whitespace collapsed.
fn normalized(text: &str) -> String {
    let folded = lexicon::fold_apostrophes(text).to_ascii_lowercase();
    folded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The parts as an attempt records them, and as an outcome document carries them: each with its
/// basis, its sink and what the bytes do with it.
pub(super) fn document(parts: &[ModelPart]) -> Value {
    Value::Array(parts.iter().map(part_document).collect())
}

/// One part's document.
fn part_document(part: &ModelPart) -> Value {
    let basis = match &part.basis {
        PartBasis::Words { words, result } => {
            json!({"kind": "words", "words": words, "result": result})
        }
        PartBasis::Sink { cited, p } => json!({"kind": "sink", "cited": cited, "p": p}),
        _ => Value::Null,
    };
    let sink = match &part.sink {
        Some(ModelSink::Task(id)) => json!({"task": id}),
        Some(ModelSink::Output(key)) => json!({"output": key}),
        _ => Value::Null,
    };
    let found = match &part.found {
        ModelFound::Met { task } => json!({"kind": "met", "task": task}),
        ModelFound::Missing => json!({"kind": "missing"}),
        ModelFound::Decorative { task } => json!({"kind": "decorative", "task": task}),
        ModelFound::Undecided { task, child } => {
            json!({"kind": "undecided", "task": task, "child": child})
        }
        _ => Value::Null,
    };
    json!({"basis": basis, "sink": sink, "found": found})
}

/// The parts a recorded attempt carries ([`document`]), read back with no call: `None` when it
/// carries none, or one this engine cannot read.
pub(super) fn read(recorded: &Value) -> Option<Vec<ModelPart>> {
    recorded.as_array()?.iter().map(read_part).collect()
}

/// One recorded part.
fn read_part(part: &Value) -> Option<ModelPart> {
    let text = |value: &Value, key: &str| value[key].as_str().map(str::to_owned);
    let basis = &part["basis"];
    let basis = match basis["kind"].as_str()? {
        "words" => PartBasis::Words {
            words: text(basis, "words")?,
            result: text(basis, "result")?,
        },
        "sink" => PartBasis::Sink {
            cited: text(basis, "cited"),
            p: basis["p"].as_f64()?,
        },
        _ => return None,
    };
    let sink = match (text(&part["sink"], "task"), text(&part["sink"], "output")) {
        (Some(id), _) => Some(ModelSink::Task(id)),
        (None, Some(key)) => Some(ModelSink::Output(key)),
        (None, None) => None,
    };
    let finding = &part["found"];
    let task = || text(finding, "task");
    let found = match finding["kind"].as_str()? {
        "met" => ModelFound::Met { task: task()? },
        "missing" => ModelFound::Missing,
        "decorative" => ModelFound::Decorative { task: task()? },
        "undecided" => ModelFound::Undecided {
            task: task()?,
            child: text(finding, "child")?,
        },
        _ => return None,
    };
    Some(ModelPart::new(basis, sink, found))
}

#[cfg(test)]
mod tests;
