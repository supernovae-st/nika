// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Bind one computation without dropping independent typed stages.
use super::{Bindings, Need, Source, Structured, joined_format, rules};
use crate::CompileRequest;
use crate::ledger::DutyKind;
use crate::plan::{Plan, Step};
use serde_json::{Value, json};

/// The rule a compute step states in words, when the corpus is what the rule can run over
/// and every part of the detail is in the grammar: one structured file for a filter, an
/// aggregate, a grouping, a sort, a top-N or a projection over its parsed records; one text
/// file for a removal of duplicate lines; several structured files of one format for a join.
pub(super) fn synthesized_rule(
    plan: &Plan,
    step: &Step,
    intent: &str,
    b: &Bindings,
    request: &CompileRequest,
) -> Option<rules::Rule> {
    let hint = source_columns(b, request, intent);
    // A validated rule stated for this very step first (the semantic frontend's typed
    // predicate, or a promoted constraint: meaning before syntax), then the closed grammar
    // over the whole detail. A detail the plan joined from several clauses (` ; `) must
    // parse whole: one recorded rule for one of its parts would silently drop the others.
    let detail = step.detail.trim();
    let whole = !detail.contains(" ; ");
    // A rule recorded for this very step stands for it when it is the only rule (the seat's
    // paraphrase beside the promoted constraint of the same rule); two recorded rules on a
    // joined detail are synthesized whole, so neither stands for the other. A rule stands
    // for a VERBATIM detail only when every sentence of the detail is its own: a second
    // sentence (« keep only the rows whose status is shipped … . Write the count of those
    // orders per country ») states more than the rule, and a rule over one part would
    // silently drop it. A seat's detail is its own paraphrase, not the request's sentences:
    // its length says nothing about a second computation (the request's coverage is the
    // accounting of its regions), so the rule anchored on its evidence stands.
    let verbatim = crate::text::exact_excerpt(intent, detail).is_some();
    let fold = |text: &str| {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let covers = |rule: &rules::Rule| {
        let text = fold(rule.text());
        !verbatim
            || detail
                .split(". ")
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .all(|sentence| {
                    let sentence = fold(sentence);
                    text.contains(&sentence) || sentence.contains(&text)
                })
    };
    let distinct = distinct_rules(plan);
    // This assembler has one compute binding. Independent rules cannot be represented
    // by selecting the first one, nor by reparsing their joined prose: the closed grammar
    // may recognize only a prefix and silently erase a second result. Leave the machine
    // binding unresolved so the existing native route can express the complete graph.
    // A typed rule for the whole detail already represents one computation; promoted
    // fragments beside it (for example each subtotal) do not force another model call.
    let fragmented = distinct.len() > 1 && !distinct.iter().any(|rule| rule.text() == detail);
    if fragmented
        && plan
            .effects
            .iter()
            .filter(|e| {
                e.policy != crate::plan::EffectPolicy::Forbidden && super::file_write(e).is_some()
            })
            .count()
            > 1
    {
        return None;
    }
    let stated = distinct
        .iter()
        .find(|rule| rule.text() == step.evidence || rule.text() == detail)
        .filter(|rule| (whole || distinct.len() == 1) && covers(rule))
        .map(|rule| (*rule).clone())
        .or_else(|| rules::synthesize(detail, &hint))
        // Keep a typed reading for the field question even when its noun is not a key.
        // ground_rule must admit it against the actual source before it can be bound.
        .or_else(|| rules::synthesize(detail, &crate::columns::columns_hint(intent)))
        .or_else(|| {
            (whole && distinct.len() == 1)
                .then(|| {
                    distinct
                        .first()
                        .filter(|rule| covers(rule))
                        .map(|rule| (*rule).clone())
                })
                .flatten()
        })?;
    let stated = holding(stated, plan, step, intent, &hint);
    // One output may use a composed pipeline (filter then sort, for example), but every
    // recorded typed stage must still be present. Recognizing a prose prefix is insufficient.
    if fragmented && !records_all_stages(&stated, &distinct) {
        return None;
    }
    match &b.read {
        Need::Bound(Source::File(path)) if Structured::of(path).is_some() => {
            (!stated.joins()).then_some(stated)
        }
        Need::Bound(Source::File(_)) => stated.over_lines(),
        Need::Bound(Source::Files(files)) if joined_format(files).is_some() => {
            stated.joins().then_some(stated)
        }
        _ => None,
    }
}

/// The rule that holds what the request's own words state (R4 A3): every operation the reader
/// reads in each anchored part, in request order, with its parameters, and, when every part is
/// read, no other operation shaping the rows (an extra filter around the stated ones changes
/// them; the summary stage computes beside them and is judged only when stated). A proposal
/// missing one, reordering them, adding one or stating another field, comparator, literal,
/// direction or count yields to a reading that holds them (the step's detail, its evidence, or
/// its parts' excerpts joined in request order); with none, it stays bound and the ledger leaves
/// what it misses, or adds, unresolved.
fn holding(
    stated: rules::Rule,
    plan: &Plan,
    step: &Step,
    intent: &str,
    hint: &[String],
) -> rules::Rule {
    let parts = stated_parts(plan, step, intent, hint);
    let expected = expected_operations(&parts);
    let exact = read_whole(&parts);
    let shaping = |ops: &[Operation]| ops.iter().filter(|op| op.shapes_rows()).count();
    let fits = |rule: &rules::Rule| {
        let ops = operations(rule);
        holds(&ops, &expected) && (!exact || shaping(&ops) == shaping(&expected))
    };
    if fits(&stated) {
        return stated;
    }
    let anchors: Vec<&str> = parts.iter().map(|part| part.anchor.as_str()).collect();
    let joined = anchors.join(" ; ");
    [step.detail.trim(), step.evidence.trim(), joined.as_str()]
        .into_iter()
        .filter_map(|text| rules::synthesize(text, hint))
        .find(|rule| fits(rule))
        .unwrap_or(stated)
}

/// Whether the parts account for every operation the computation may run: at least one part,
/// and every part read by the grammar.
pub(crate) fn read_whole(parts: &[Part]) -> bool {
    !parts.is_empty() && parts.iter().all(|part| part.reading.is_some())
}

/// What the rule bound for the compute step is judged against (R4 A3): the step's parts, read
/// over the columns the binding reads with, and that rule as chosen before grounding.
pub(super) fn stated_witness(
    plan: &Plan,
    step: &Step,
    intent: &str,
    b: &Bindings,
    request: &CompileRequest,
    chosen: &rules::Rule,
) -> Witness {
    let hint = source_columns(b, request, intent);
    Witness {
        parts: stated_parts(plan, step, intent, &hint),
        chosen: chosen.clone(),
    }
}

/// One typed operation of a computation, with the parameters a witness compares (R4 A3): a
/// filter's clause (field, comparator, value and its kind) or its « or » of clauses, a count's
/// grouping (its output name is the named outputs' law), a sort's key and direction, a cut's
/// size, the summary stage's count.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Operation {
    Filter(Value),
    Count(Value),
    Order(String, bool),
    Limit(u64),
    Summary,
}

impl Operation {
    /// Whether the operation shapes the rows the rule writes: every one but the summary stage,
    /// which computes its count and totals beside them, in its own task.
    pub(crate) fn shapes_rows(&self) -> bool {
        !matches!(self, Self::Summary)
    }
    /// The duty an operation states.
    pub(crate) const fn kind(&self) -> DutyKind {
        match self {
            Self::Filter(_) => DutyKind::Filter,
            Self::Count(_) | Self::Summary => DutyKind::Count,
            Self::Order(..) => DutyKind::Order,
            Self::Limit(_) => DutyKind::Limit,
        }
    }
    /// The source fields the operation reads.
    pub(crate) fn reads(&self) -> Vec<String> {
        let text = |v: &Value| v.as_str().map(str::to_owned);
        match self {
            Self::Filter(filter) => filter["clauses"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|clause| text(&clause["field"]))
                .collect(),
            Self::Count(group_by) => text(group_by).into_iter().collect(),
            Self::Order(key, _) => vec![key.clone()],
            Self::Limit(_) | Self::Summary => Vec::new(),
        }
    }
}

/// The typed operations of a rule in the order its lowering runs them: each step's filter
/// (an « and » of clauses is each clause in turn, in the order stated, which is also the order
/// a FAIL policy reads them in), then its counts (after any grouping), its sort and its cut,
/// and the summary stage last. A verified program has none a witness reads.
pub(crate) fn operations(rule: &rules::Rule) -> Vec<Operation> {
    let record = rule.to_json();
    if !record["program"].is_null() {
        return Vec::new();
    }
    let later = record["then"].as_array().cloned().unwrap_or_default();
    let mut ops = Vec::new();
    for step in std::iter::once(&record).chain(&later) {
        let clauses: Vec<Value> = step["clauses"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| json!({"field": c["field"], "comparator": c["comparator"], "value": c["value"], "value_kind": c["value_kind"]}))
            .collect();
        if clauses.len() > 1 && step["junction"] == "or" {
            ops.push(Operation::Filter(
                json!({"clauses": clauses, "junction": "or"}),
            ));
        } else {
            let each = clauses
                .into_iter()
                .map(|c| json!({"clauses": [c], "junction": null}));
            ops.extend(each.map(Operation::Filter));
        }
        let shape = &step["shape"];
        for aggregation in shape["aggregations"].as_array().into_iter().flatten() {
            if aggregation["op"] == "count" {
                ops.push(Operation::Count(shape["group_by"].clone()));
            }
        }
        if let Some(key) = shape["sort_by"].as_str() {
            ops.push(Operation::Order(
                key.to_owned(),
                shape["descending"] == true,
            ));
        }
        if let Some(n) = shape["limit"].as_u64() {
            ops.push(Operation::Limit(n));
        }
    }
    if record["summary"] == true {
        ops.push(Operation::Summary);
    }
    ops
}

/// Where each expected operation is found in `ops`, in order (R4 A3): the first equal
/// operation after the one found for the operation before it, `None` for one found nowhere
/// after it (so one missing operation never shifts the others).
pub(crate) fn found(ops: &[Operation], expected: &[Operation]) -> Vec<Option<usize>> {
    let mut from = 0;
    expected
        .iter()
        .map(|want| {
            let at = ops
                .iter()
                .skip(from)
                .position(|op| op == want)
                .map(|i| from + i);
            if let Some(at) = at {
                from = at + 1;
            }
            at
        })
        .collect()
}

fn holds(ops: &[Operation], expected: &[Operation]) -> bool {
    found(ops, expected).iter().all(Option::is_some)
}

/// One part the request states for the computation (R4 A3): the exact request excerpt it
/// anchors on, and the reader's own reading of those words when the grammar reads them whole.
/// The plan's typed content never stands in for the reading.
#[derive(Clone, Debug)]
pub(crate) struct Part {
    pub(crate) anchor: String,
    pub(crate) reading: Option<rules::Rule>,
}

/// What a bound computation is judged against (R4 A3): the parts in the order the request
/// states them, and the rule the binding chose before grounding rebinds a field or binds a
/// number policy (neither moves an operation or changes its other parameters).
#[derive(Clone, Debug)]
pub(crate) struct Witness {
    pub(crate) parts: Vec<Part>,
    pub(crate) chosen: rules::Rule,
}

#[cfg(test)]
impl Witness {
    /// A witness over `stated` excerpts, read with no column hint, judged against `chosen`.
    pub(crate) fn of(stated: &[&str], chosen: rules::Rule) -> Self {
        let read = |text: &&str| Part {
            anchor: (*text).to_owned(),
            reading: rules::synthesize(text, &[]),
        };
        Self {
            parts: stated.iter().map(read).collect(),
            chosen,
        }
    }
}

/// The parts of the compute step: each plan rule, the step's evidence and its detail whose text
/// is an exact excerpt of the request, ordered by where the request states them and
/// each re-read by the one grammar over the columns the binding reads with. Each word is read
/// once: the widest readable excerpt stands for the excerpts inside it (a seat's rule over a
/// whole clause beside the rules promoted from its parts), and an unreadable excerpt is a part
/// only where no readable one overlaps it.
fn stated_parts(plan: &Plan, step: &Step, intent: &str, hint: &[String]) -> Vec<Part> {
    let columns = crate::columns::columns_hint(intent);
    let place = |text: &str| {
        let excerpt = crate::text::exact_excerpt(intent, text)?;
        let start = intent.find(&excerpt)?;
        Some((start, start + excerpt.len(), excerpt))
    };
    // Every plan rule anchors its own words, a plain twin of a shaped rule included: the twin
    // is the reader's reading of a clause the proposal may have moved. The step's evidence and
    // its detail anchor theirs when they are request excerpts (a detail joined with ` ; ` is
    // none): a detail may state more than its evidence (« … and how many rows were kept »).
    let texts = plan.rules.iter().map(rules::Rule::text);
    let mut spans: Vec<(usize, usize, String)> = texts
        .chain([step.evidence.as_str(), step.detail.as_str()])
        .filter_map(place)
        .collect();
    spans.sort_by_key(|(start, end, _)| (*start, std::cmp::Reverse(*end)));
    spans.dedup();
    let read = |anchor: &str| {
        rules::synthesize(anchor, hint).or_else(|| rules::synthesize(anchor, &columns))
    };
    let parts: Vec<(usize, usize, Part)> = spans
        .into_iter()
        .map(|(start, end, anchor)| {
            (
                start,
                end,
                Part {
                    reading: read(&anchor),
                    anchor,
                },
            )
        })
        .collect();
    let mut kept: Vec<&(usize, usize, Part)> = Vec::new();
    for readable in [true, false] {
        for part in parts.iter().filter(|p| p.2.reading.is_some() == readable) {
            let (start, end) = (part.0, part.1);
            let inside = kept.iter().any(|k| k.0 <= start && end <= k.1);
            let overlaps = kept.iter().any(|k| start < k.1 && k.0 < end);
            if (readable && !inside) || (!readable && !overlaps) {
                kept.push(part);
            }
        }
    }
    kept.sort_by_key(|k| (k.0, k.1));
    kept.into_iter().map(|k| k.2.clone()).collect()
}

/// The operations the readable parts state, in request order.
fn expected_operations(parts: &[Part]) -> Vec<Operation> {
    parts
        .iter()
        .filter_map(|part| part.reading.as_ref())
        .flat_map(operations)
        .collect()
}

fn source_columns(b: &Bindings, request: &CompileRequest, intent: &str) -> Vec<String> {
    let observed = match &b.read {
        Need::Bound(Source::File(path)) => {
            crate::observed::columns(crate::observed::world(request), path)
        }
        _ => None,
    };
    observed.unwrap_or_else(|| crate::columns::columns_hint(intent))
}

/// The plan's rules with the plain twins removed: the promoted constraint of a clause
/// (« data igual a 2026-09-22 ») beside the seat's typed rule over the same clause with its
/// output columns is one rule, not two. A plain rule (a filter with no stage after it)
/// whose clauses and junction another rule states is that rule's twin.
fn distinct_rules(plan: &Plan) -> Vec<&rules::Rule> {
    let key = |rule: &rules::Rule| {
        let json = rule.to_json();
        (json["clauses"].clone(), json["junction"].clone())
    };
    let mut kept: Vec<&rules::Rule> = Vec::new();
    for rule in &plan.rules {
        let twin = plan
            .rules
            .iter()
            .any(|other| !std::ptr::eq(other, rule) && other.shaped() && key(other) == key(rule));
        if !rule.shaped() && twin {
            continue;
        }
        if !kept
            .iter()
            .any(|k| k.text() == rule.text() && k.jq() == rule.jq())
        {
            kept.push(rule);
        }
    }
    kept
}

/// A structural containment check over the existing typed rule contract, not a claim
/// about arbitrary jq. Empty fields carry no stage; populated fields must survive.
fn contains_stages(actual: &Value, required: &Value) -> bool {
    match required {
        Value::Null | Value::Bool(false) => true,
        Value::Array(items) => actual
            .as_array()
            .is_some_and(|found| items.iter().all(|item| found.contains(item))),
        Value::Object(fields) => fields
            .iter()
            .all(|(key, value)| contains_stages(&actual[key], value)),
        _ => actual == required,
    }
}

/// Every recorded part is a typed stage of the candidate, found in the candidate's steps in the
/// order the plan records the parts (R4 F5): a part never sits in a step before the step of
/// the part stated before it. An inventory that holds every stage in another order is refused.
fn records_all_stages(candidate: &rules::Rule, parts: &[&rules::Rule]) -> bool {
    let record = candidate.to_json();
    let later = record["then"].as_array().cloned().unwrap_or_default();
    let steps: Vec<Value> = std::iter::once(record).chain(later).collect();
    let mut at = 0;
    parts.iter().all(|rule| {
        let required = rule.to_json();
        let holds = |actual: &Value| {
            ["clauses", "shape", "program"]
                .iter()
                .all(|key| contains_stages(&actual[*key], &required[*key]))
                && (required["shape"]["sort_by"].is_null()
                    || actual["shape"]["descending"] == required["shape"]["descending"])
                && (required["clauses"]
                    .as_array()
                    .is_none_or(|clauses| clauses.len() < 2)
                    || actual["junction"] == required["junction"])
        };
        match steps.iter().skip(at).position(holds) {
            Some(found) => {
                at += found;
                true
            }
            None => false,
        }
    })
}
