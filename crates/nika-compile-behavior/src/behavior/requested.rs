// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The contract a request states. Its facts are the reader's typed plan of the request (each
//! operation, effect and unknown with its evidence, each effect's policy and whether it writes
//! one value alone, each rule's typed fields), the paths the request names and the answers the
//! human gave. A rule is read by its typed fields only (clauses, junction, shape, the steps
//! after it, the number policies); the jq the reader lowers it to is an observation and is
//! never read here, and no candidate (its program, tasks, intermediate formats or wrappers)
//! takes part. Where the facts do not pair a computation with its source and its file, the
//! obligation stays unsupported rather than guessed.

use std::collections::BTreeMap;

use nika_compile_reader::aggregate::AggOp;
use nika_compile_reader::plan::{Effect, EffectPolicy, EffectVerb, Op, Plan};
use nika_compile_reader::rules::{Comparator, Junction, NumberPolicy, Rule, keeps_order};
use nika_compile_reader::{hot, paths, shape, structure};
use serde_json::Value;

use super::composed::{Composed, composable, composed, neutral};
use super::formats::{Format, json_document};
use super::numbers::{Decimal, Law};
use super::pipeline::{Aggregate, Filter, OnEmpty, Operand, Pipeline, Sort, Stages, Step, Test};
use super::values::{Datum, reading};
use super::{Contract, Form, Obligation, Presence, Requirement, Target, same_path};

type Answers = BTreeMap<String, String>;
type Policies = BTreeMap<String, NumberPolicy>;

/// The contract the reader's plan of a request states, with the answers the human gave
/// (`const.<slug>` keys, JSON literals). Pure and deterministic.
///
/// `plan` must be the plan of the request itself, never one stated by a candidate's
/// structure: the contract exists before, and independently of, any candidate.
#[must_use]
pub fn contract_of(plan: &Plan, intent: &str, answers: &BTreeMap<String, String>) -> Contract {
    let mut obligations = Vec::new();
    let composed = composed(plan, answers);
    unverified_work(plan, composed.as_ref(), &mut obligations);
    let writes: Vec<&Effect> = plan
        .effects
        .iter()
        .filter(|e| e.verb == EffectVerb::Write)
        .collect();
    let computation = computation(plan, intent, answers, &writes, composed.as_ref());
    for effect in &writes {
        obligations.push(write_obligation(effect, computation.as_ref()));
        if composed.is_some() {
            beyond_the_write(effect, &mut obligations);
        }
    }
    if writes.is_empty() {
        for rule in &plan.rules {
            obligations.push(Obligation::new(
                "result",
                None,
                Presence::Unproven,
                Requirement::Unsupported("the request names no file for this result".to_owned()),
                rule.text(),
            ));
        }
    }
    for rule in &plan.rules {
        if !rule.summary() {
            continue;
        }
        obligations.push(Obligation::new(
            "summary",
            None,
            Presence::Unproven,
            Requirement::Unsupported(
                "the count or total of the kept rows the request also asks for".to_owned(),
            ),
            rule.text(),
        ));
    }
    // Every path the request names, apart from the files it writes, is a source: a fixture
    // without one is no instance of the request.
    let written: Vec<String> = writes
        .iter()
        .filter_map(|effect| paths::single_file(&effect.target))
        .collect();
    let sources: Vec<String> = hot::stated_sources(intent)
        .into_iter()
        .filter(|source| !written.iter().any(|path| same_path(source, path)))
        .collect();
    kept_sources(plan, &sources, &mut obligations);
    Contract::new(obligations).with_sources(sources)
}

/// A composed computation reads its write's words up to its file: the words after it (another
/// result, a rule of the write) stay an explicit unsupported obligation with the whole sentence,
/// never absorbed into the computed file.
fn beyond_the_write(effect: &Effect, out: &mut Vec<Obligation>) {
    let rest = effect
        .evidence
        .find(effect.target.as_str())
        .and_then(|at| effect.evidence.get(at + effect.target.len()..));
    let rest =
        rest.map(|rest| rest.trim_matches(|c: char| c.is_whitespace() || ",;:.".contains(c)));
    if rest.is_some_and(|rest| rest.is_empty() || structure::only_function_words(rest)) {
        return;
    }
    out.push(Obligation::new(
        format!("beyond write {}", effect.target),
        None,
        Presence::Unproven,
        Requirement::Unsupported(
            "words of this sentence beyond the file it writes, which this component does not \
             verify"
                .to_owned(),
        ),
        effect.evidence.clone(),
    ));
}

/// A prohibition naming a source the request reads (the path as one of its words): that file
/// is never written, judged as any forbidden write (the rehearsal reads it back).
fn kept_sources(plan: &Plan, sources: &[String], out: &mut Vec<Obligation>) {
    for constraint in plan.constraints.iter().filter(|c| shape::prohibits(c)) {
        let words: Vec<&str> = constraint
            .split_whitespace()
            .map(|word| word.trim_matches(|c: char| ",;:!?»«\"'`()".contains(c)))
            .map(|word| word.strip_suffix('.').unwrap_or(word))
            .collect();
        for source in sources {
            if !words.iter().any(|word| same_path(word, source)) {
                continue;
            }
            let format = Format::of_path(source).unwrap_or(Format::Text);
            out.push(Obligation::new(
                format!("keep {source}"),
                Some(Target::new(source.clone(), format)),
                Presence::Forbidden,
                Requirement::PresenceOnly,
                constraint.clone(),
            ));
        }
    }
}

/// The requested work this component never verifies, each as an explicit unsupported
/// obligation: a retrieval, a model's or a person's work, an unknown, an obligation over
/// effects across runs, an effect beyond the files a rehearsal observes.
fn unverified_work(plan: &Plan, composed: Option<&Composed>, out: &mut Vec<Obligation>) {
    for (at, step) in plan.steps.iter().enumerate() {
        if let Some(composed) = composed.filter(|_| composable(step.op)) {
            for (_, heading) in composed.headings.iter().filter(|(of, _)| *of == at) {
                out.push(Obligation::new(
                    format!("operation {}", step.op.word()),
                    None,
                    Presence::Unproven,
                    Requirement::Unsupported(
                        "the heading of a computation, which states no rule of it".to_owned(),
                    ),
                    heading.clone(),
                ));
            }
            continue;
        }
        let why = match step.op {
            Op::Read => continue,
            Op::Compute if !plan.rules.is_empty() => continue,
            Op::Compute => "a computation the plan states without a typed rule",
            Op::Fetch | Op::Lookup | Op::Search => {
                "a retrieval from the world, which a rehearsal never performs"
            }
            _ => "work a model or a person performs, which records cannot verify",
        };
        out.push(Obligation::new(
            format!("operation {}", step.op.word()),
            None,
            Presence::Unproven,
            Requirement::Unsupported(why.to_owned()),
            step.evidence.clone(),
        ));
    }
    for (at, unknown) in plan.unknowns.iter().enumerate() {
        out.push(Obligation::new(
            format!("unknown {}", at + 1),
            None,
            Presence::Unproven,
            Requirement::Unsupported("requested work the reader could not construct".to_owned()),
            unknown.clone(),
        ));
    }
    for obligation in &plan.obligations {
        out.push(Obligation::new(
            format!("obligation {}", obligation.kind.word()),
            None,
            Presence::Unproven,
            Requirement::Unsupported(
                "an obligation over effects across runs, which one rehearsal does not observe"
                    .to_owned(),
            ),
            obligation.evidence.clone(),
        ));
    }
    for effect in plan.effects.iter().filter(|e| e.verb != EffectVerb::Write) {
        out.push(Obligation::new(
            format!("{} {}", effect.verb.word(), effect.target),
            None,
            presence_of(effect),
            Requirement::Unsupported("an effect outside the files a rehearsal observes".to_owned()),
            effect.evidence.clone(),
        ));
    }
}

/// The computation the request states and the source it reads.
struct Computed {
    source: String,
    format: Format,
    pipeline: Pipeline,
}

/// What the plan states about an effect's presence. An automatic authorization does not prove
/// that no condition governs the effect (the plan has no field for one), and policy words the
/// plan does not type leave it undecided.
fn presence_of(effect: &Effect) -> Presence {
    match effect.policy {
        EffectPolicy::Automatic if effect.policy_literal.is_none() => Presence::Unproven,
        EffectPolicy::HumanFirst => Presence::Approval,
        EffectPolicy::Forbidden => Presence::Forbidden,
        _ => Presence::Undecided,
    }
}

/// The one rule, its one source and its one write, when the facts pair them.
fn computation(
    plan: &Plan,
    intent: &str,
    answers: &Answers,
    writes: &[&Effect],
    composed: Option<&Composed>,
) -> Option<Result<Computed, String>> {
    let (pipeline, stated) = match (plan.rules.as_slice(), composed) {
        (_, Some(composed)) => (Ok(composed.pipeline.clone()), composed.text.as_str()),
        ([], None) => return None,
        ([_], None) if plan.constraints.iter().any(|c| !neutral(c)) => {
            return Some(Err(
                "a constraint beside the rule may shape its rows and is not read".to_owned(),
            ));
        }
        ([rule], None) => (pipeline_of(rule, answers), rule.text()),
        _ => {
            return Some(Err(
                "several rules: which result lands in which file is not a stated fact".to_owned(),
            ));
        }
    };
    let [write] = writes else {
        return Some(Err(
            "several writes: which one holds the result is not a stated fact".to_owned(),
        ));
    };
    Some(computed_from(plan, pipeline, stated, intent, write))
}

/// Whether a path names several files: a pattern or a directory.
fn several_files(path: &str) -> bool {
    path.contains(['*', '?', '[', '{']) || path.ends_with('/')
}

fn computed_from(
    plan: &Plan,
    pipeline: Result<Pipeline, String>,
    stated: &str,
    intent: &str,
    write: &Effect,
) -> Result<Computed, String> {
    let destination = paths::single_file(&write.target);
    let sources: Vec<String> = hot::stated_sources(intent)
        .into_iter()
        .filter(|source| {
            destination
                .as_deref()
                .is_none_or(|destination| !same_path(source, destination))
        })
        .collect();
    let source = match sources.as_slice() {
        [source] => source.clone(),
        [] => return Err("the request names no source file apart from the result".to_owned()),
        _ => return Err("the request names several source files".to_owned()),
    };
    if several_files(&source) {
        return Err(format!("{source} names several files"));
    }
    let format = Format::of_path(&source)
        .filter(|format| *format != Format::Text)
        .ok_or_else(|| format!("{source} has a format this component does not read"))?;
    let mut pipeline = pipeline?;
    pipeline.keep_order = keeps_order(stated)
        || plan
            .constraints
            .iter()
            .any(|constraint| keeps_order(constraint.as_str()));
    Ok(Computed {
        source,
        format,
        pipeline,
    })
}

fn write_obligation(effect: &Effect, computation: Option<&Result<Computed, String>>) -> Obligation {
    let presence = presence_of(effect);
    let Some(path) = paths::single_file(&effect.target) else {
        return Obligation::new(
            format!("write {}", effect.target),
            None,
            presence,
            Requirement::Unsupported("the write names no single file".to_owned()),
            effect.evidence.clone(),
        );
    };
    let id = format!("write {path}");
    let Some(format) = Format::of_path(&path) else {
        let why = format!("{path} has a format this component does not read");
        return Obligation::new(
            id,
            None,
            presence,
            Requirement::Unsupported(why),
            effect.evidence.clone(),
        );
    };
    let requirement = if presence == Presence::Forbidden {
        Requirement::PresenceOnly
    } else {
        match computation {
            None => Requirement::Unsupported(
                "the request states no computation this component verifies for this file"
                    .to_owned(),
            ),
            Some(Err(why)) => Requirement::Unsupported(why.clone()),
            Some(Ok(computed)) => requirement_of(computed, effect.alone, format),
        }
    };
    Obligation::new(
        id,
        Some(Target::new(path, format)),
        presence,
        requirement,
        effect.evidence.clone(),
    )
}

/// The form of the written result: one value alone only where the plan states it (a bare JSON
/// value, or the whole text of the file), totals as the one object naming them, else rows.
fn requirement_of(computed: &Computed, alone: bool, format: Format) -> Requirement {
    let form = if alone {
        let one_total = computed
            .pipeline
            .steps
            .last()
            .is_some_and(|step| step.stages.totals() && step.stages.aggregates.len() == 1);
        if !one_total {
            return Requirement::Unsupported("a value alone needs exactly one total".to_owned());
        }
        Form::Alone
    } else if computed.pipeline.totals() {
        Form::Totals
    } else {
        Form::Rows
    };
    let readable = match form {
        Form::Rows => matches!(format, Format::Json | Format::JsonLines | Format::Csv),
        Form::Totals => format == Format::Json,
        Form::Alone => matches!(format, Format::Json | Format::Text),
    };
    if !readable {
        return Requirement::Unsupported(format!(
            "this result written as {} is not read",
            format.word()
        ));
    }
    Requirement::Computed {
        source: computed.source.clone(),
        source_format: computed.format,
        pipeline: computed.pipeline.clone(),
        form,
    }
}

/// The relation a rule states, its answered values resolved from `answers` (`const.<slug>`
/// keys, JSON literals). The rule is read by its typed fields, never by the jq it lowers to.
///
/// # Errors
///
/// The reason, in words, when the rule states what this component does not evaluate (a
/// seat-written program, the lines of a text source, a join, rows per group with no named
/// aggregate, outputs defined as arithmetic over the aggregates) or a value the human has not
/// answered.
pub fn pipeline_of(rule: &Rule, answers: &BTreeMap<String, String>) -> Result<Pipeline, String> {
    if rule.verified_program().is_some() {
        return Err("a program a seat wrote, whose meaning is only its code".to_owned());
    }
    if rule.lines() {
        return Err("the lines of a text source".to_owned());
    }
    if rule.joins() {
        return Err("a join of several sources".to_owned());
    }
    pipeline_of_record(&rule.to_json(), answers)
}

/// The relation of a rule's record, read by its typed fields: `clauses`, `junction`, `shape`,
/// `then` and `numbers`. Its `jq`, `fields` and every other key are ignored.
pub(crate) fn pipeline_of_record(record: &Value, answers: &Answers) -> Result<Pipeline, String> {
    if record
        .get("program")
        .is_some_and(|program| !program.is_null())
    {
        return Err("a program a seat wrote, whose meaning is only its code".to_owned());
    }
    if record.get("lines").and_then(Value::as_bool) == Some(true) {
        return Err("the lines of a text source".to_owned());
    }
    let policies = policies_of(record)?;
    let mut steps = vec![step_of(record, answers, &policies)?];
    if let Some(then) = record.get("then") {
        let then = then
            .as_array()
            .ok_or("the steps after the first are not a list")?;
        for step in then {
            steps.push(step_of(step, answers, &policies)?);
        }
    }
    let mut pipeline = Pipeline::new(steps);
    pipeline.policies = policies;
    Ok(pipeline)
}

fn policies_of(record: &Value) -> Result<Policies, String> {
    let mut policies = Policies::new();
    let Some(stated) = record.get("numbers") else {
        return Ok(policies);
    };
    let stated = stated
        .as_object()
        .ok_or("number policies that are not a map")?;
    for (field, word) in stated {
        let policy = match word.as_str() {
            Some("fail") => NumberPolicy::Fail,
            Some("skip") => NumberPolicy::Skip,
            _ => return Err(format!("the number policy of {field}")),
        };
        policies.insert(field.clone(), policy);
    }
    Ok(policies)
}

fn text_at<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("a record without its {key}"))
}

fn optional_text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn texts(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map_or_else(Vec::new, |items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
}

fn step_of(value: &Value, answers: &Answers, policies: &Policies) -> Result<Step, String> {
    let clauses = value
        .get("clauses")
        .and_then(Value::as_array)
        .ok_or("a step without its clauses")?;
    let tests = clauses
        .iter()
        .map(|clause| test_of(clause, answers))
        .collect::<Result<Vec<Test>, String>>()?;
    let junction = match value.get("junction").and_then(Value::as_str) {
        Some("or") => Junction::Or,
        Some("and") => Junction::And,
        None if tests.len() <= 1 => Junction::And,
        Some(other) => return Err(format!("the junction {other}")),
        None => return Err("clauses without their junction".to_owned()),
    };
    let stages = stages_of(value.get("shape"), policies)?;
    Ok(Step::new(Filter::new(tests, junction), stages))
}

/// Whether a comparator reads the value as text (starts with, contains, ends with, negated).
fn textual(comparator: Comparator) -> bool {
    matches!(
        comparator,
        Comparator::StartsWith
            | Comparator::NotStartsWith
            | Comparator::Contains
            | Comparator::NotContains
            | Comparator::EndsWith
            | Comparator::NotEndsWith
    )
}

/// Whether a comparator orders values (greater, less, or equal to either).
fn orders(comparator: Comparator) -> bool {
    matches!(
        comparator,
        Comparator::Gt | Comparator::Ge | Comparator::Lt | Comparator::Le
    )
}

fn test_of(clause: &Value, answers: &Answers) -> Result<Test, String> {
    let field = text_at(clause, "field")?.trim();
    if field.is_empty() || field == "." {
        return Err("a comparison on the whole record".to_owned());
    }
    let symbol = text_at(clause, "comparator")?;
    let comparator =
        Comparator::from_word(symbol).ok_or_else(|| format!("the comparison {symbol}"))?;
    let literal = text_at(clause, "value")?;
    let kind = clause.get("value_kind").and_then(Value::as_str);
    // A text comparison reads its literal as the text the request wrote, whatever its kind.
    let operand = match kind {
        Some("column") => Operand::Column(literal.to_owned()),
        Some("slot") => answered(literal, answers, comparator)?,
        Some("bool") if !textual(comparator) => Operand::Bool(literal == "true"),
        Some("number") if !textual(comparator) => Operand::Number(
            Decimal::from_law(literal).ok_or_else(|| format!("the number {literal}"))?,
        ),
        _ => Operand::Text(literal.to_owned()),
    };
    let mut test = Test::new(field, comparator, operand);
    test.spellings = texts(clause, "spellings");
    Ok(test)
}

/// A value the request alluded to, as the human answered it (`const.<slug>`), read the way the
/// reader lowers the comparison: a number for an order, text for a text function, and the
/// value's text for an equality.
fn answered(slug: &str, answers: &Answers, comparator: Comparator) -> Result<Operand, String> {
    let raw = answers
        .get(&format!("const.{slug}"))
        .ok_or_else(|| format!("the value {slug} is not answered"))?;
    let datum = json_document(raw).unwrap_or_else(|_| Datum::Text(raw.clone()));
    if orders(comparator) {
        let Law::Number(number) = reading(&datum) else {
            return Err(format!("the answer for {slug} is no number"));
        };
        return Ok(Operand::Number(number));
    }
    let text = match datum {
        Datum::Text(text) => text,
        Datum::Bool(truth) => truth.to_string(),
        Datum::Number(_) => {
            return Err(format!(
                "the answer for {slug} is a number whose text the runtime writes"
            ));
        }
        Datum::Null | Datum::List(_) | Datum::Record(_) => {
            return Err(format!("the answer for {slug} is no single value"));
        }
    };
    Ok(if textual(comparator) {
        Operand::Text(text)
    } else {
        Operand::AsText(text)
    })
}

fn aggregate_of(item: &Value, policies: &Policies) -> Result<Aggregate, String> {
    let word = text_at(item, "op")?;
    let op = AggOp::from_word(word).ok_or_else(|| format!("the aggregate {word}"))?;
    let name = text_at(item, "name")?.trim();
    if name.is_empty() {
        return Err("an aggregate without its name".to_owned());
    }
    let field = optional_text(item, "field");
    let round = item
        .get("round")
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_u64()
                .and_then(|places| u32::try_from(places).ok())
                .ok_or("a rounding that is no count of decimals")
        })
        .transpose()?;
    let stated = field.as_ref().is_some_and(|f| policies.contains_key(f));
    let on_empty = match op {
        AggOp::Sum | AggOp::Count => OnEmpty::Zero,
        _ if stated => OnEmpty::Stops,
        _ => OnEmpty::Unstated,
    };
    Ok(Aggregate::new(op, field, name, round, on_empty))
}

fn stages_of(shape: Option<&Value>, policies: &Policies) -> Result<Stages, String> {
    let Some(shape) = shape else {
        return Ok(Stages::default());
    };
    if shape.get("join_on").is_some_and(|join| !join.is_null()) {
        return Err("a join of several sources".to_owned());
    }
    let derived = shape.get("derived").and_then(Value::as_array);
    if derived.is_some_and(|items| !items.is_empty()) {
        return Err("outputs defined as arithmetic over the aggregates".to_owned());
    }
    let mut aggregates = Vec::new();
    if let Some(items) = shape.get("aggregations").and_then(Value::as_array) {
        for item in items {
            aggregates.push(aggregate_of(item, policies)?);
        }
    }
    let group_by = optional_text(shape, "group_by");
    if group_by.is_some() && aggregates.is_empty() {
        return Err("rows per group with a count whose name the request does not state".to_owned());
    }
    let sort = optional_text(shape, "sort_by").map(|field| {
        let descending = shape.get("descending").and_then(Value::as_bool) == Some(true);
        let stable = shape.get("ties").and_then(Value::as_str) == Some("first_in_file");
        Sort::new(field, descending, stable)
    });
    let mut renames = Vec::new();
    if let Some(items) = shape.get("renames").and_then(Value::as_array) {
        for item in items {
            let from = text_at(item, "from")?;
            let to = text_at(item, "to")?;
            renames.push((from.to_owned(), to.to_owned()));
        }
    }
    Ok(Stages {
        derived: Vec::new(),
        distinct_by: texts(shape, "distinct_by"),
        group_by,
        aggregates,
        sort,
        limit: shape
            .get("limit")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n > 0),
        columns: texts(shape, "columns"),
        number_columns: texts(shape, "numbers"),
        renames,
        distinct: shape.get("distinct").and_then(Value::as_bool) == Some(true),
    })
}
