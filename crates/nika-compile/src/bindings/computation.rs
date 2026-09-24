// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Bind one computation without dropping independent typed stages.
use super::{Bindings, Need, Source, Structured, joined_format, rules};
use crate::CompileRequest;
use crate::plan::{Plan, Step};
use serde_json::Value;

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
    let observed = match &b.read {
        Need::Bound(Source::File(path)) => {
            crate::observed::columns(crate::observed::world(request), path)
        }
        _ => None,
    };
    let hint = observed.unwrap_or_else(|| crate::columns::columns_hint(intent));
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

fn records_all_stages(candidate: &rules::Rule, parts: &[&rules::Rule]) -> bool {
    let actual = candidate.to_json();
    parts.iter().all(|rule| {
        let required = rule.to_json();
        ["clauses", "shape", "program"]
            .iter()
            .all(|key| contains_stages(&actual[*key], &required[*key]))
            && (required["shape"]["sort_by"].is_null()
                || actual["shape"]["descending"] == required["shape"]["descending"])
            && (required["clauses"]
                .as_array()
                .is_none_or(|clauses| clauses.len() < 2)
                || actual["junction"] == required["junction"])
    })
}
