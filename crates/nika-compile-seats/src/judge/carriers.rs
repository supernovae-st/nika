// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The properties a request states that a candidate must carry with an executable check: what the
//! reader's typed computations state an output holds (a count, a total, an order, a number of
//! rows), values the workflow computes without a model. Each is carried only by a check its runs
//! evaluate: a `nika:assert` with no `when:` whose condition reads a `with:` binding of a task's
//! output (the one place a task reads another's) and whose message quotes the words that state the
//! property; or the request's own typed rule, run as the reader lowers it. A property the bytes
//! leave uncarried is named before READY, with no model call: the author is told which words to
//! carry and how, and no judge is asked about it. A bound on produced text (its lines, bullets or
//! words) is no such property: what a model writes is never asserted, the room runs no model step,
//! and the bound stays the judge's.

use nika_compile::surface::literal_projection;
use nika_compile::{CompileOutcome, CompileStatus, DiagnosticKind};
use nika_compile_fidelity::fidelity::Diagnostic;
use nika_compile_reader::{lexicon, shape};
use serde_json::{Value, json};

/// How an author carries a property, told with each one left uncarried.
const REMEDY: &str = "Carry each property with a check every run evaluates: a nika:jq task computing it as a boolean over the value before it is written, then a nika:assert with no when: that binds that boolean through with: (with: { ok: \"${{ tasks.<check>.output }}\" }, condition: \"${{ with.ok }}\") and whose message quotes the property's evidence verbatim; the write waits for it with after: { <assert>: success }.";

/// One property the request states, and the task of the candidate that carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Property {
    /// `count`, `total`, `order` or `limit`.
    pub kind: &'static str,
    /// The request's words that state it, verbatim: its typed rule's text.
    pub evidence: String,
    /// The task that carries it, when one does.
    pub carrier: Option<String>,
}

/// The properties a request states and what one candidate carries of them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Witness {
    properties: Vec<Property>,
}

impl Witness {
    /// The properties `intent` states, none carried: the excerpts an author's checks quote.
    #[must_use]
    pub fn required(intent: &str) -> Vec<Property> {
        Self::of(intent, "").properties
    }

    /// The properties `intent` states, each with the task of `candidate` that carries it.
    #[must_use]
    pub fn of(intent: &str, candidate: &str) -> Self {
        let folded = lexicon::fold_apostrophes(intent);
        let mut reading = lexicon::read(&folded);
        shape::promote_stated_rules(&mut reading.plan, &folded);
        let document = literal_projection(candidate).unwrap_or(Value::Null);
        let tasks = document["tasks"].as_object().cloned().unwrap_or_default();
        let checks: Vec<(&String, &str)> = (tasks.iter())
            .filter_map(|(id, task)| check(task).map(|message| (id, message)))
            .collect();
        let quoted = |evidence: &str| {
            let wanted = normalized(evidence);
            (checks.iter())
                .find(|(_, message)| normalized(message).contains(&wanted))
                .map(|(id, _)| (*id).clone())
        };
        let mut properties: Vec<Property> = Vec::new();
        let mut add = |kind: &'static str, evidence: &str, carrier: Option<String>| {
            let property = Property {
                kind,
                evidence: evidence.trim().to_owned(),
                carrier,
            };
            if !properties.contains(&property) {
                properties.push(property);
            }
        };
        for rule in (reading.plan.rules.iter()).filter(|rule| rule.verified_program().is_none()) {
            let lowered = rule.jq();
            let typed = (tasks.iter())
                .find(|(_, task)| runs(task, &lowered))
                .map(|(id, _)| id.clone());
            for kind in kinds(&rule.to_json()) {
                add(
                    kind,
                    rule.text(),
                    typed.clone().or_else(|| quoted(rule.text())),
                );
            }
        }
        Self { properties }
    }

    /// Every stated property, in the request's order.
    #[must_use]
    pub fn properties(&self) -> &[Property] {
        &self.properties
    }

    /// The properties no check of the candidate carries.
    pub fn uncarried(&self) -> impl Iterator<Item = &Property> {
        self.properties.iter().filter(|p| p.carrier.is_none())
    }

    /// The provenance projection: each property, its evidence and its carrier.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let each =
            |p: &Property| json!({"kind": p.kind, "evidence": p.evidence, "carrier": p.carrier});
        Value::Array(self.properties.iter().map(each).collect())
    }

    /// What an author's repair starts from: the properties left uncarried and how to carry them
    /// (`{"kind":"unwitnessed","properties":[{"kind","evidence"}],"remedy"}`); none when every
    /// stated property is carried.
    #[must_use]
    pub fn defect(&self) -> Option<Diagnostic> {
        let open: Vec<Value> = (self.uncarried())
            .map(|p| json!({"kind": p.kind, "evidence": p.evidence}))
            .collect();
        let message = json!({"kind": "unwitnessed", "properties": open, "remedy": REMEDY});
        (!open.is_empty()).then(|| Diagnostic {
            kind: "witness",
            message: message.to_string(),
        })
    }

    /// Hold `out` while a stated property stays uncarried: never READY, the bytes kept as the
    /// preview, each property named with its remedy, and the record dropped so no round replays
    /// these bytes (the next one authors again). `true` when held; `out` is untouched otherwise.
    pub fn hold(&self, out: &mut CompileOutcome) -> bool {
        if self.uncarried().next().is_none() {
            return false;
        }
        out.status = CompileStatus::Incomplete;
        out.provenance.plan = None;
        for p in self.uncarried() {
            let (evidence, kind) = (&p.evidence, p.kind);
            let said = format!(
                "The request states « {evidence} » ({kind}) and no executable check of the candidate carries it; the core named it, no judge was asked. Nothing is READY on it. Next: a nika:jq check of it, asserted by a nika:assert with no when: whose message quotes « {evidence} »."
            );
            nika_compile::finding(out, DiagnosticKind::Unknown, "semantic_verification", said);
        }
        let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
        decision["witness"] = self.to_json();
        out.provenance.decision = Some(decision);
        true
    }
}

/// The kinds of property a typed rule's record states, step by step: a count or a total for each
/// aggregate, an order for a sort and a limit for a cut.
fn kinds(record: &Value) -> Vec<&'static str> {
    let later = record["then"].as_array().into_iter().flatten();
    let mut out = Vec::new();
    for step in std::iter::once(record).chain(later) {
        let shape = &step["shape"];
        for aggregation in shape["aggregations"].as_array().into_iter().flatten() {
            out.push(if aggregation["op"] == "count" {
                "count"
            } else {
                "total"
            });
        }
        if shape["sort_by"].is_string() {
            out.push("order");
        }
        if shape["limit"].is_u64() {
            out.push("limit");
        }
    }
    out
}

/// The message of `task` when it is a check its runs evaluate: a `nika:assert` with no `when:`
/// whose condition is one `${{ }}` reading a `with:` binding of a task's output, the one place a
/// task reads another's. A literal, or a condition over constants or inputs alone, checks nothing
/// a run made.
fn check(task: &Value) -> Option<&str> {
    let invoke = &task["invoke"];
    if invoke["tool"] != "nika:assert" || task.get("when").is_some() {
        return None;
    }
    let condition = invoke["args"]["condition"].as_str()?.trim();
    let expression = condition.strip_prefix("${{")?.strip_suffix("}}")?;
    let reads = |text: &str| text.contains("tasks.") && text.contains(".output");
    let bound = (task["with"].as_object().into_iter().flatten()).any(|(name, value)| {
        mentions(expression, &format!("with.{name}")) && value.as_str().is_some_and(reads)
    });
    bound.then(|| invoke["args"]["message"].as_str()).flatten()
}

/// Whether `text` names `name` as a whole reference (not a longer name it begins).
fn mentions(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(at, _)| {
        let next = text[at + name.len()..].chars().next();
        !next.is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

/// Whether `task` runs `lowered`, the reader's own program of a typed rule, byte for byte.
fn runs(task: &Value, lowered: &str) -> bool {
    let invoke = &task["invoke"];
    invoke["tool"] == "nika:jq" && invoke["args"]["expression"].as_str() == Some(lowered)
}

/// The words of `text` for a quotation: apostrophes folded, whitespace collapsed, ASCII case
/// ignored.
fn normalized(text: &str) -> String {
    let folded = lexicon::fold_apostrophes(text).to_ascii_lowercase();
    folded.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests;
