// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Durable authoring work, never an executable rule or runtime authority.
use crate::plan::{Op, Plan, Step};
use crate::{CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, Strategy};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const SCHEMA: &str = "nika/pending-transform@1";
const MAX_ATTEMPTS: u8 = 2;

/// A stale or malformed continuation is an authoring refusal, never runtime authority.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PendingTransformError {
    #[error("no pending transform record")]
    MissingRecord,
    #[error("pending source has no path")]
    MissingSourcePath,
    #[error(
        "pending transform context changed or is malformed; compile the current request afresh"
    )]
    ChangedContext,
    #[error("verified transform no longer matches its field choices")]
    ChangedRule,
    #[error("the verified field choice changed; compile the request afresh")]
    ChangedField,
    #[error("a pending operation cannot already carry an executable rule")]
    PrematureRule,
    #[error(transparent)]
    Decode(#[from] serde_json::Error),
    /// The existing plan decoder's refusal, preserved at this typed continuation boundary.
    #[error("{0}")]
    InvalidPlan(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Field {
    name: String,
    answer: Option<String>,
}

/// A single unresolved compute operation in the existing private plan.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct PendingTransform {
    schema: String,
    intent: String,
    intent_sha256: String,
    plan_sha256: String,
    source: Value,
    evidence: String,
    detail: String,
    offered: Vec<String>,
    fields: Vec<Field>,
    attempts: u8,
    verified_rule: Option<String>,
}

fn plan_hash(plan: &Plan) -> String {
    crate::surface::sha256(&plan.to_json().to_string())
}
fn rule_hash(rule: &crate::rules::Rule) -> String {
    crate::surface::sha256(&rule.to_json().to_string())
}
fn same_path(a: &str, b: &str) -> bool {
    a.strip_prefix("./").unwrap_or(a) == b.strip_prefix("./").unwrap_or(b)
}
fn source(world: Option<&Value>, path: &str) -> Option<Value> {
    let mut rows = world?.get("observed")?.as_array()?.iter().filter(|row| {
        row.get("path")
            .and_then(Value::as_str)
            .is_some_and(|p| same_path(p, path))
    });
    let first = rows.next()?;
    rows.next().is_none().then(|| first.clone())
}

/// Whether a record carries this operation's pending or verified field context.
#[must_use]
pub fn present(record: &Value) -> bool {
    record.get("pending_transform").is_some() || record.get("verified_transform").is_some()
}

impl PendingTransform {
    /// Capture a mismatch only for the existing single-source, single-compute operation.
    #[must_use]
    pub fn new(
        intent: &str,
        plan: &Plan,
        step: &Step,
        names: &[String],
        request: &CompileRequest,
    ) -> Option<Self> {
        if names.is_empty() || plan.steps.iter().filter(|s| s.op == Op::Compute).count() != 1 {
            return None;
        }
        let path = crate::paths::single_file(&plan.step(Op::Read)?.detail)?;
        let source = source(crate::observed::world(request), &path)?;
        let offered = crate::observed::columns(crate::observed::world(request), &path)?;
        if offered.is_empty() {
            return None;
        }
        let mut fields = Vec::new();
        for name in names {
            if !fields.iter().any(|f: &Field| f.name == *name) {
                fields.push(Field {
                    name: name.clone(),
                    answer: None,
                });
            }
        }
        Some(Self {
            schema: SCHEMA.into(),
            intent: crate::lexicon::fold_apostrophes(intent),
            intent_sha256: crate::intent_sha256(intent),
            plan_sha256: plan_hash(plan),
            source,
            evidence: step.evidence.clone(),
            detail: step.detail.clone(),
            offered,
            fields,
            attempts: 0,
            verified_rule: None,
        })
    }

    /// Validate the record's operation, intent, source identity and closed choices.
    /// # Errors
    /// Reject malformed or stale state; an old answer must never authorize new source data.
    pub fn load(
        intent: &str,
        record: &Value,
        request: &CompileRequest,
    ) -> Result<(Self, Plan), PendingTransformError> {
        let raw = record
            .get("pending_transform")
            .or_else(|| record.get("verified_transform"))
            .ok_or(PendingTransformError::MissingRecord)?;
        let state: Self = serde_json::from_value(raw.clone())?;
        let plan = Plan::from_json(record).map_err(PendingTransformError::InvalidPlan)?;
        let path = state
            .source
            .get("path")
            .and_then(Value::as_str)
            .ok_or(PendingTransformError::MissingSourcePath)?;
        let current = match crate::observed::world(request) {
            Some(world) => source(Some(world), path),
            None => Some(state.source.clone()),
        };
        let observed = json!({"observed":[state.source.clone()]});
        let read_path = plan
            .step(Op::Read)
            .and_then(|s| crate::paths::single_file(&s.detail));
        let valid = state.schema == SCHEMA
            && state.intent_sha256 == crate::intent_sha256(intent)
            && state.intent_sha256 == crate::intent_sha256(&state.intent)
            && matches!(request.input, crate::types::Input::Create(_))
            && !request.answers.contains_key("intent.clarification")
            && state.plan_sha256 == plan_hash(&plan)
            && plan.anchored(intent)
            && plan.unknowns.is_empty()
            && read_path.as_deref().is_some_and(|p| same_path(p, path))
            && plan.steps.iter().filter(|s| s.op == Op::Compute).count() == 1
            && plan
                .step(Op::Compute)
                .is_some_and(|s| s.evidence == state.evidence && s.detail == state.detail)
            && current.as_ref() == Some(&state.source)
            && crate::observed::columns(Some(&observed), path).as_ref() == Some(&state.offered)
            && !state.offered.is_empty()
            && !state.fields.is_empty()
            && state.attempts <= MAX_ATTEMPTS
            && state.fields.iter().all(|f| {
                !f.name.is_empty() && f.answer.as_ref().is_none_or(|a| state.offered.contains(a))
            })
            && (record.get("pending_transform").is_some()
                != record.get("verified_transform").is_some())
            && (record.get("verified_transform").is_some() == state.verified_rule.is_some());
        if !valid {
            return Err(PendingTransformError::ChangedContext);
        }
        if let Some(hash) = &state.verified_rule {
            if !state.answered()
                || !plan
                    .rules
                    .iter()
                    .any(|r| r.text() == state.detail && rule_hash(r) == *hash)
            {
                return Err(PendingTransformError::ChangedRule);
            }
            for (index, field) in state.fields.iter().enumerate() {
                if let Some(raw) = request.answers.get(&Self::key(index)) {
                    let answer: Value = serde_json::from_str(raw)?;
                    if answer.as_str() != field.answer.as_deref() {
                        return Err(PendingTransformError::ChangedField);
                    }
                }
            }
        } else if plan
            .rules
            .iter()
            .any(|r| r.text() == state.detail || r.text() == state.evidence)
        {
            return Err(PendingTransformError::PrematureRule);
        }
        Ok((state, plan))
    }

    fn key(index: usize) -> String {
        format!("const.rule_field_{}", index + 1)
    }
    /// Bind only offered values and retain answers across deterministic rounds.
    pub fn answer(&mut self, request: &CompileRequest, out: &mut CompileOutcome) {
        for (index, field) in self.fields.iter_mut().enumerate() {
            let key = Self::key(index);
            if field.answer.is_some() && !request.answers.contains_key(&key) {
                continue;
            }
            let label = format!(
                "Which observed field does `{}` mean in `{}`?",
                field.name, self.evidence
            );
            field.answer = crate::observed::field_answer(request, out, &key, &label, &self.offered)
                .and_then(|v| v.as_str().map(str::to_owned));
        }
    }
    /// Whether all required field choices have an offered answer.
    #[must_use]
    pub fn answered(&self) -> bool {
        self.fields.iter().all(|f| f.answer.is_some())
    }
    /// Spend one of the durable regeneration attempts, never more than one call per round.
    pub fn begin_attempt(&mut self) -> bool {
        if !self.answered() || self.attempts >= MAX_ATTEMPTS {
            return false;
        }
        self.attempts += 1;
        true
    }
    /// Context for the existing transform provider, with mappings as data, never source edits.
    #[must_use]
    pub fn context(&self) -> Value {
        json!({"request":self.intent, "clause":self.evidence, "computation":self.detail,
            "columns":self.offered, "field_choices":self.fields, "source":self.source,
            "instruction":"Regenerate the computation using these explicit field choices; preserve the original literals, result shape and operation."})
    }
    /// Exact observed names available to the verifier.
    #[must_use]
    pub fn columns(&self) -> &[String] {
        &self.offered
    }
    /// Every chosen field must be declared as read by the regenerated program.
    #[must_use]
    pub fn uses_answers(&self, fields: &[String]) -> bool {
        self.fields
            .iter()
            .all(|f| f.answer.as_ref().is_some_and(|a| fields.contains(a)))
    }
    /// Original operation detail, not a rewritten request.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    /// Keep pending work visible without assembling it or asking for program text.
    pub fn suspend(&self, plan: &Plan, out: &mut CompileOutcome) {
        out.status = CompileStatus::Incomplete;
        out.candidate = None;
        out.check_preview = None;
        let message = if self.attempts >= MAX_ATTEMPTS {
            "The bounded transform regeneration attempts are exhausted; compile afresh to authorize new work."
        } else if self.answered() {
            "Field choices are recorded; verified program regeneration needs an explicitly authorized provider."
        } else {
            "Choose the observed fields before bounded program regeneration."
        };
        crate::finding(out, DiagnosticKind::Unknown, "authoring_transform", message);
        let mut record = crate::doors::plan_record(plan, Some(Strategy::Cold));
        record["pending_transform"] = json!(self);
        out.provenance.plan = Some(record);
        out.provenance.strategy = Some(Strategy::Cold);
    }

    /// Preserve the exact field-choice receipt beside the newly verified program.
    #[must_use]
    pub fn verified_record(mut self, plan: &Plan, rule: &crate::rules::Rule) -> Value {
        self.plan_sha256 = plan_hash(plan);
        self.verified_rule = Some(rule_hash(rule));
        let mut record = crate::doors::plan_record(plan, Some(Strategy::Cold));
        record["verified_transform"] = json!(self);
        record
    }
}

/// A stale record offers no stale choices or executable candidate.
pub fn invalid(out: &mut CompileOutcome, why: &str) {
    crate::finding(out, DiagnosticKind::Unknown, "pending_transform", why);
}

/// The deterministic replay owns questions and persistence, but cannot generate code.
pub(crate) fn replay(
    intent: &str,
    record: &Value,
    request: &CompileRequest,
    out: &mut CompileOutcome,
) -> bool {
    if !present(record) {
        return false;
    }
    match PendingTransform::load(intent, record, request) {
        Ok((mut state, plan)) if state.verified_rule.is_none() => {
            state.answer(request, out);
            state.suspend(&plan, out);
            true
        }
        Ok(_) => false,
        Err(why) => {
            invalid(out, &why.to_string());
            true
        }
    }
}

/// Field choices authorize only the exact verified rule and observed source they accompanied.
pub(crate) fn approves(
    request: &CompileRequest,
    rule: &crate::rules::Rule,
    path: &str,
    field: &str,
    recognized: &mut std::collections::BTreeSet<String>,
) -> bool {
    let crate::types::Input::Create(intent) = &request.input else {
        return false;
    };
    let Some(record) = &request.plan else {
        return false;
    };
    let Ok((state, _)) = PendingTransform::load(intent, record, request) else {
        return false;
    };
    let approved = state.verified_rule.as_deref() == Some(rule_hash(rule).as_str())
        && state
            .source
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|p| same_path(p, path))
        && state
            .fields
            .iter()
            .any(|f| f.answer.as_deref() == Some(field));
    if approved {
        recognized.extend((0..state.fields.len()).map(PendingTransform::key));
    }
    approved
}
