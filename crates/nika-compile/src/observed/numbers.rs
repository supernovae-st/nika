// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Every value a bound rule reads as a number is read under the reader's one number law (R4 A5
//! · C3/C4): a finite JSON number or a decimal text, anything else stopping the run with the field
//! and the value named, unless a policy the request grounds says otherwise. Where the host
//! observed a sampled value of such a field that is not a number (null, missing, a boolean, a
//! list, an object, other text), what a record like it does is asked before READY: skip it or
//! fail the run. The sample is bounded: an unseen value follows the stated policy, FAIL where none
//! was stated. The policy of every such field is stated on the bound rule and recorded
//! (`decision.numbers`); nothing is left to jq's total order or its lenient `tonumber`.
use super::grounding;
use crate::rules::{NumberPolicy, Rule};
use crate::{ChoiceOffer, CompileOutcome, CompileRequest, DiagnosticKind};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// The raw kinds that are not a number, in the order a question names them, with their words.
const OTHERS: [(&str, &str); 7] = [
    ("null", "null"),
    ("absent", "missing"),
    ("boolean", "true/false"),
    ("text", "text"),
    ("empty", "empty"),
    ("array", "list"),
    ("object", "object"),
];

/// The kinds the host observed in the source at `path` (`world.kinds`), when it did.
fn entry<'a>(world: Option<&'a Value>, path: &str) -> Option<&'a Value> {
    let bare = |p: &str| p.strip_prefix("./").unwrap_or(p).to_owned();
    let kinds = world?.get("kinds")?.as_object()?;
    kinds
        .iter()
        .find(|(p, _)| bare(p) == bare(path))
        .map(|(_, e)| e)
}

/// The sampled values of `field` that are not a number, by kind, with their count.
fn others(counts: &Value) -> Vec<(&'static str, u64)> {
    OTHERS
        .iter()
        .filter_map(|(kind, word)| {
            let n = counts.get(*kind).and_then(Value::as_u64)?;
            (n > 0).then_some((*word, n))
        })
        .collect()
}

/// A recorded answer is for another revision of the source when its observed row or its kinds
/// changed since the question was asked (a type-only change moves the kinds).
fn stale(request: &CompileRequest, path: &str) -> bool {
    let Some(record) = request.plan.as_ref() else {
        return false;
    };
    let asked = record.get("observed_world");
    let now = request.knowledge.as_ref().or(asked);
    grounding::stale(request, path) || entry(asked, path) != entry(now, path)
}

/// The policy an answer states, asked again when it is stale or states none.
fn answered(
    request: &CompileRequest,
    out: &mut CompileOutcome,
    key: &str,
    path: &str,
) -> Option<NumberPolicy> {
    request.answers.get(key)?;
    if stale(request, path) {
        // A plan recorded before kinds were observed (R4 A6): the file need not have changed.
        let recorded = request.plan.as_ref().and_then(|p| p.get("observed_world"));
        let predates = entry(recorded, path).is_none() && !grounding::stale(request, path);
        let message = if predates {
            format!(
                "The plan recorded for `{path}` predates the value kinds this question counts, so it is asked over them now. Answer again."
            )
        } else {
            format!(
                "`{path}` changed since this question was asked (its values or their kinds), so its answer is for another revision. Answer again."
            )
        };
        crate::finding(out, DiagnosticKind::Missed, key, message);
        return None;
    }
    let raw = request.answers.get(key).map(String::as_str);
    let word = crate::literal_answer(raw, key, out);
    let policy = [NumberPolicy::Skip, NumberPolicy::Fail]
        .into_iter()
        .find(|p| word.as_ref().and_then(Value::as_str) == Some(p.word()));
    if policy.is_none() {
        crate::finding(out, DiagnosticKind::Missed, key, "Answer skip or fail.");
    }
    policy
}

/// Ask what a record whose `field` is not a number does, naming what the sample showed.
fn ask(
    out: &mut CompileOutcome,
    key: &str,
    rule: &Rule,
    path: &str,
    field: &str,
    counts: &Value,
    sampled: u64,
) {
    let seen = others(counts);
    let n: u64 = seen.iter().map(|(_, n)| n).sum();
    let detail: Vec<String> = seen.iter().map(|(word, n)| format!("{word} {n}")).collect();
    let label = format!(
        "In `{path}`, `{field}` is not a number in {n} of {sampled} sampled records ({}). How should `{}` treat a record whose `{field}` is not a number?",
        detail.join(" · "),
        rule.text()
    );
    let options = vec![
        ChoiceOffer::new(
            "skip",
            "leave that record out: the comparison is false for it and a ranking or total leaves it out (a ranking or total left with no number stops the run)",
        ),
        ChoiceOffer::new(
            "fail",
            "stop the run and name the value; nothing after it is written",
        ),
    ];
    let why = "Counted from the sampled records only, never a whole-file proof: a value the sample did not show follows the same answer at run.";
    crate::choice_question(out, key, &label, why, options);
}

/// The number policy of every field `rule` reads as a number over the source at `path`, grounded
/// in the kinds the host observed: a field some sampled value of which is not a number is asked
/// (`const.rule_number_<n>`, skip or fail, bound to the source's revision); a plain sort over a
/// key observed as numbers only reads the law. Returns the rule, the fields a question governs
/// (a missing value is one of the kinds it names, so no second question asks about them) and
/// whether a question is open. The decision records every field (`decision.numbers`).
pub(crate) fn ground(
    mut rule: Rule,
    path: &str,
    request: &CompileRequest,
    out: &mut CompileOutcome,
    recognized: &mut BTreeSet<String>,
) -> (Rule, Vec<String>, bool) {
    let world = super::world(request);
    let seen = entry(world, path);
    let revision = grounding::revision(grounding::row(world, path));
    let sampled = seen.and_then(|e| e["sampled"].as_u64()).unwrap_or(0);
    let mut fields = rule.number_fields();
    if rule.ranking_without_count()
        && let Some(key) = rule.plain_sort()
    {
        fields.push(key.to_owned());
    }
    let (mut governed, mut open, mut records) = (Vec::new(), false, Vec::new());
    for (index, field) in fields.iter().enumerate() {
        let counts = seen.and_then(|e| e["keys"].get(field)).cloned();
        let observed = counts.as_ref().is_some_and(|c| !others(c).is_empty());
        let policy = if observed {
            governed.push(field.clone());
            let key = format!("const.rule_number_{}", index + 1);
            recognized.insert(key.clone());
            let policy = answered(request, out, &key, path);
            if policy.is_none() {
                ask(
                    out,
                    &key,
                    &rule,
                    path,
                    field,
                    counts.as_ref().unwrap_or(&Value::Null),
                    sampled,
                );
                open = true;
            }
            policy
        } else {
            Some(NumberPolicy::Fail)
        };
        if let Some(p) = policy
            && let Some(stated) = rule.with_number_policy(field, p)
        {
            rule = stated;
        }
        let bound_by = match (observed, policy, &counts) {
            (true, Some(_), _) => "answer",
            (true, None, _) => "pending",
            (false, _, Some(_)) => "observed numbers",
            (false, _, None) => "unobserved",
        };
        records.push(json!({
            "rule": rule.text(), "field": field, "source": path, "revision": revision,
            "sampled": sampled, "kinds": counts, "policy": policy.map(NumberPolicy::word),
            "bound_by": bound_by,
        }));
    }
    let numeric_only = |key: &str| {
        seen.and_then(|e| e["keys"].get(key))
            .is_some_and(|c| others(c).is_empty() && c.as_object().is_some_and(|o| !o.is_empty()))
    };
    if let Some(key) = rule.plain_sort().map(str::to_owned)
        && !fields.contains(&key)
        && numeric_only(&key)
        && let Some(stated) = rule.with_number_policy(&key, NumberPolicy::Fail)
    {
        rule = stated;
    }
    if !records.is_empty() {
        let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
        decision["numbers"] = Value::Array(records);
        out.provenance.decision = Some(decision);
    }
    (rule, governed, open)
}

/// The bound rule with a policy stated for every field it reads as a number: the one grounded
/// before, else FAIL. The decision records each (`decision.numbers`) unless grounding did.
pub(crate) fn numbered(mut rule: Rule, out: &mut CompileOutcome) -> Rule {
    let mut entries = Vec::new();
    for field in rule.number_fields() {
        let stated = rule.number_policy(&field);
        let policy = stated.unwrap_or(NumberPolicy::Fail);
        if stated.is_none()
            && let Some(bound) = rule.with_number_policy(&field, policy)
        {
            rule = bound;
        }
        entries.push(json!({
            "rule": rule.text(),
            "field": field,
            "policy": policy.word(),
            "bound_by": if stated.is_some() { "grounding" } else { "default" },
        }));
    }
    let grounded = out
        .provenance
        .decision
        .as_ref()
        .is_some_and(|d| d.get("numbers").is_some());
    if !entries.is_empty() && !grounded {
        let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
        decision["numbers"] = Value::Array(entries);
        out.provenance.decision = Some(decision);
    }
    rule
}
