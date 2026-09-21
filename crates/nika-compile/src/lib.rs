// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The stateless Compile core: one intent in, one [`CompileOutcome`] out. The deterministic
//! reader, the typed semantic plan, the finite composer, the deterministic assembler and
//! the Check preview live here; `nika-onboard` re-exports this crate at its historical
//! `compile` path, so every caller keeps writing `nika_compile::…`.
//!
//! Descended from `nika-onboard` at the 15k prod-LOC wall (2026-09-21 · ADR-137): per
//! D-2026-07-09-N1 this is ONE architectural unit in TWO workspace members. The member
//! never depends back on the surface.
//!
//! Stateless authoring foundation: explicit request → ordinary source → pure Check preview.
//!
//! CREATE resolves exact embedded skeletons and a bounded support clause grammar,
//! then asks about their real unfilled values. EDIT consumes accepted source plus a textual or structured
//! constant change; both lower to one edit operation before emission/Check.
//! It never regenerates unrelated tasks. Source selection and CAS stay app-owned.
//! Unsupported intent stays incomplete, without guessed topology or hidden model calls.
//! Re-emission refuses source whose literal semantics cannot be proven stable.
//! Top-level answer objects with both `type` and `value` are refused in this slice.
//! An integer answer outside the canonical reader's exact `i64` range is refused
//! before decoding can round it, at any depth; fraction and exponent answers are
//! floats, and quoted digits stay text. This is not arbitrary precision.
//!
//! `compile()` does not materialize files, execute workflows, probe credentials,
//! resolve connections or grant permits. [`materialize_ready`] is the opt-in
//! `.nika` write adapter: no silent overwrite, run, or grant. Source-only
//! preview is not full host Check or admission: Run must judge the candidate
//! again under its actual environment.
//! [`compile_with_provider`] accepts explicit bounded authoring policy and an injected
//! kernel provider. Its closed private semantic plan rejoins deterministic motif
//! assembly before Check. Model output is never final YAML or runtime authority.
//! General natural-language qualification and Graph integration remain separate work.
//! Transports (the CLI, Serve) consume these typed outcomes and
//! print the one machine document of [`outcome_document`]; none re-projects an outcome.
//! Private pattern-facet derivation (#1666) walks the parsed AST and Check
//! facts; it is not a YAML key, a fifth verb, or an SDK noun, and it never
//! grants authority.
//!
//! ```
//! use nika_compile::{compile, CompileRequest, CompileStatus};
//! let request = CompileRequest::create("classify-and-route")
//!     .answer("const.request", r#""An outage affects our customers.""#);
//! let created = compile(&request)?;
//! assert_eq!(created.status, CompileStatus::Ready);
//! if let Some(source) = created.candidate {
//!     let edited = compile(&CompileRequest::edit(source,
//!         r#"Set const.request to "One customer cannot log in.""#))?;
//!     assert_eq!(edited.status, CompileStatus::Ready);
//! }
//! # Ok::<(), nika_compile::CompileError>(())
//! ```

//! A UI can submit an explicit operation against the source it owns:
//!
//! ```
//! use nika_compile::{compile, CompileRequest, CompileStatus};
//! let base = compile(&CompileRequest::create("classify-and-route")
//!     .answer("const.request", r#""Initial request""#))?;
//! if let Some(source) = base.candidate {
//!     let edited = compile(&CompileRequest::set_constant(
//!         source, "request", r#""https://example.invalid/a?q=é#résumé""#))?;
//!     assert_eq!(edited.status, CompileStatus::Ready);
//! }
//! # Ok::<(), nika_compile::CompileError>(())
//! ```

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod assemble;
mod bindings;
mod cognition;
mod compose;
pub mod decide;
mod edit;
mod edit_source;
mod laws;
mod materialize;
mod network;
pub(crate) mod pattern;
mod predicate;
mod retrieve;
mod shape;
mod support;
mod trigger;
mod types;
mod wire;

// The frozen reader and the typed plan live in the second member of this unit (the
// ADR-137 precedent); the composer, the assembler and the preview read them at their
// historical module paths.
use nika_compile_reader::{columns, hot, lexicon, objects, paths, plan, rule_tokens, rules};

use std::collections::BTreeSet;

use nika_schema::{FileId, ParseMode, raw::RawWorkflow};
use serde_json::Value;
use types::{EditChange, Input};

pub use cognition::{
    Cognition, NoProvider, compile_with_cognition, compile_with_provider, intent_sha256,
};
pub use materialize::{MaterializeError, materialize_ready};
pub use nika_compile_reader::text;
pub use retrieve::{Hit, HitKind, retrieve, retrieve_by_ops};
pub use types::{
    AuthoringCognition, AuthoringPolicy, AuthoringReceipt, CompileDiagnostic, CompileError,
    CompileOutcome, CompilePreview, CompileProvenance, CompileQuestion, CompileRequest,
    CompileStatus, DiagnosticKind, HotPolicy, PreviewScope, QuestionType, RepresentationError,
    Strategy, TriggerKind, TriggerRequirement, TriggerStatus,
};
pub use wire::{COMPILE_WIRE_VERSION, outcome_document};

/// Compile without effects or hidden state. Repeating a request produces the same candidate.
///
/// # Errors
/// Returns a machinery error only for a corrupt embedded skeleton or representation
/// failure. Missing values, invalid answers and unsupported user requests are outcomes.
#[must_use = "the candidate and its authoring questions must be reviewed"]
pub fn compile(request: &CompileRequest) -> Result<CompileOutcome, CompileError> {
    let mut outcome = initial();
    if request.workflow_id.is_some() && matches!(request.input, Input::Edit { .. }) {
        outcome.status = CompileStatus::Refused;
        finding(
            &mut outcome,
            DiagnosticKind::Refused,
            "nika",
            "An edit cannot rename its accepted base through CREATE options.",
        );
        if let Input::Edit { source, .. } = &request.input {
            outcome.candidate = Some(source.clone());
        }
        return Ok(outcome);
    }
    match &request.input {
        Input::Create(intent) => create(intent, request, &mut outcome)?,
        Input::Edit { source, change } => edit(source, change, request, &mut outcome)?,
    }
    Ok(outcome)
}

fn initial() -> CompileOutcome {
    CompileOutcome {
        status: CompileStatus::Incomplete,
        candidate: None,
        questions: Vec::new(),
        diagnostics: Vec::new(),
        requested_boundary: None,
        requested_trigger: None,
        check_preview: None,
        provenance: CompileProvenance {
            authoring: None,
            compiler_version: env!("CARGO_PKG_VERSION").to_owned(),
            spec_pin: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../SPEC_PIN"))
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty() && !line.starts_with('#'))
                .unwrap_or("")
                .to_owned(),
            skeleton: None,
            cognition: AuthoringCognition::DeterministicOnly,
            strategy: None,
            plan: None,
            decision: None,
        },
    }
}

fn finding(
    out: &mut CompileOutcome,
    kind: DiagnosticKind,
    target: &str,
    message: impl Into<String>,
) {
    out.diagnostics.push(CompileDiagnostic {
        kind,
        target: target.to_owned(),
        message: message.into(),
    });
}

fn question(out: &mut CompileOutcome, key: &str, label: &str, answer_type: QuestionType) {
    out.questions.push(CompileQuestion {
        key: key.to_owned(),
        label: label.to_owned(),
        answer_type,
        why: "The compiler cannot invent this authoring value.".to_owned(),
        mandatory: true,
    });
}

fn parse(source: &str) -> Result<RawWorkflow, nika_schema::SchemaError> {
    nika_schema::parse(source, FileId::new(0), ParseMode::Strict)
}

fn create(
    intent: &str,
    request: &CompileRequest,
    out: &mut CompileOutcome,
) -> Result<(), CompileError> {
    let slug = intent.trim();
    // Exact membership, not a fuzzy winner that can silently drop requested work.
    let hello = matches!(slug, "hello" | "01-hello");
    let source = if hello {
        nika_pack::example("01-hello")
    } else if nika_pack::template_names().iter().any(|name| name == slug) {
        nika_pack::template(slug)
    } else {
        // An answer round replays the plan its previous round produced (zero reading).
        if let Some(record) = &request.plan {
            return cognition::replay(intent, record, request, out);
        }
        // A partial support match is not a verdict: the general reader is a superset.
        if let Ok(Some(plan)) = support::resolve(intent) {
            support::assemble(&plan, request, out)?;
            out.provenance.strategy = Some(types::Strategy::Support);
            return Ok(());
        }
        if cognition::hot(intent, request, out)? {
            return Ok(());
        }
        finding(
            out,
            DiagnosticKind::Unknown,
            "intent",
            "The requested intent is outside the exact skeletons and bounded support clauses. Use an exact skeleton or explicitly opt in to provider authoring; no substitute workflow was selected.",
        );
        return Ok(());
    };
    let Some(source) = source else {
        return Err(CompileError::MissingSkeleton(slug.to_owned()));
    };
    out.provenance.skeleton = Some(slug.to_owned());
    out.provenance.strategy = Some(types::Strategy::Skeleton);
    let wf = parse(source).map_err(CompileError::Registry)?;
    let report = nika_check::check(&wf);
    let mut doc: Value = serde_yaml_bw::from_str(source).map_err(CompileError::representation)?;
    let before = doc.clone();
    let mut recognized = BTreeSet::new();
    let mut changed = false;
    if hello {
        // One embedded lesson, one assembler: the authoring hello is an offline
        // rehearsal even when the gallery lesson demonstrates a local provider.
        doc["model"] = Value::String("mock/echo".to_owned());
        changed = true;
    }
    if let Some(id) = &request.workflow_id {
        doc["nika"] = Value::String(id.clone());
        changed = true;
    }
    for slot in &report.slot_findings {
        recognized.insert(slot.path.as_str());
        let answer_type = if slot.path.starts_with("tasks.") || slot.path == "model" {
            QuestionType::Text
        } else {
            QuestionType::Literal
        };
        match literal_answer(
            request.answers.get(&slot.path).map(String::as_str),
            &slot.path,
            out,
        ) {
            Some(answer) if answer_type != QuestionType::Text || answer.is_string() => {
                if edit::literal_at(&mut doc, &slot.path)
                    .is_some_and(|node| edit::fill_slot(node, answer))
                {
                    changed = true;
                    finding(
                        out,
                        DiagnosticKind::Applied,
                        &slot.path,
                        "Filled the existing semantic hole from an explicit answer.",
                    );
                } else {
                    question(out, &slot.path, &slot.hint, answer_type);
                    finding(
                        out,
                        DiagnosticKind::Missed,
                        &slot.path,
                        "This hole requires literal text and must preserve its surrounding value.",
                    );
                }
            }
            Some(_) => {
                question(out, &slot.path, &slot.hint, answer_type);
                finding(
                    out,
                    DiagnosticKind::Missed,
                    &slot.path,
                    "This hole requires a JSON string.",
                );
            }
            None => question(out, &slot.path, &slot.hint, answer_type),
        }
    }
    unknown_answers(request, &recognized, out);
    if changed {
        finish_changed(source, &before, &doc, None, out)?;
    } else {
        finish(source.to_owned(), out);
    }
    Ok(())
}

fn edit(
    source: &str,
    change: &EditChange,
    request: &CompileRequest,
    out: &mut CompileOutcome,
) -> Result<(), CompileError> {
    let Ok(wf) = parse(source) else {
        finish(source.to_owned(), out);
        finding(
            out,
            DiagnosticKind::Missed,
            "base_workflow",
            "The accepted workflow must parse before any edit is applied.",
        );
        return Ok(());
    };
    if !nika_check::check(&wf).is_clean() {
        finish(source.to_owned(), out);
        finding(
            out,
            DiagnosticKind::Missed,
            "base_workflow",
            "The accepted workflow must pass pure Check before this localized edit.",
        );
        return Ok(());
    }
    let Some(edit::ConstantEdit {
        name,
        literal_json: inline,
    }) = edit::operation(change)
    else {
        finding(
            out,
            DiagnosticKind::Unknown,
            "change_request",
            "The entire change request remains unresolved. Use Set const.NAME to JSON_LITERAL or structured set_constant with a bare ASCII constant name. No workflow node was changed.",
        );
        finish(source.to_owned(), out);
        return Ok(());
    };
    let key = format!("const.{name}");
    let mut doc: Value = serde_yaml_bw::from_str(source).map_err(CompileError::representation)?;
    let before = doc.clone();
    let Some(node) = edit::literal_at(&mut doc, &key) else {
        finding(
            out,
            DiagnosticKind::Missed,
            &key,
            "Only an existing constant can be changed. No node was inserted.",
        );
        finish(source.to_owned(), out);
        return Ok(());
    };
    let recognized = BTreeSet::from([key.as_str()]);
    unknown_answers(request, &recognized, out);
    let answer = request.answers.get(&key).map(String::as_str);
    if inline.is_some() && answer.is_some() && inline != answer {
        finding(
            out,
            DiagnosticKind::Unknown,
            &key,
            "The change request and answer disagree; neither value was selected.",
        );
        question(
            out,
            &key,
            "Supply one unambiguous value in the change request or its answer.",
            QuestionType::Literal,
        );
        finish(source.to_owned(), out);
        return Ok(());
    }
    let Some(value) = literal_answer(inline.or(answer), &key, out) else {
        question(
            out,
            &key,
            "What literal value should this existing constant have?",
            QuestionType::Literal,
        );
        finish(source.to_owned(), out);
        return Ok(());
    };
    *node = value;
    finding(
        out,
        DiagnosticKind::Applied,
        &key,
        "Changed only the requested constant; task identities, other values and declared permits are preserved.",
    );
    finish_changed(source, &before, &doc, Some(name), out)
}

fn finish_changed(
    source: &str,
    before: &Value,
    after: &Value,
    constant_name: Option<&str>,
    out: &mut CompileOutcome,
) -> Result<(), CompileError> {
    let candidate = match constant_name {
        Some(name) => edit_source::emit(source, before, after, name),
        None => {
            edit::emit_preserving(source, before, after).map_err(CompileError::representation)?
        }
    };
    if let Some(candidate) = candidate {
        finish(candidate, out);
    } else {
        out.diagnostics
            .retain(|d| d.kind != DiagnosticKind::Applied);
        out.status = CompileStatus::Refused;
        finding(
            out,
            DiagnosticKind::Refused,
            "candidate",
            "Literal/source-preservation policy cannot prove a safe edit of this presentation; no changes were applied.",
        );
        finish(source.to_owned(), out);
    }
    Ok(())
}

fn literal_answer(raw: Option<&str>, key: &str, out: &mut CompileOutcome) -> Option<Value> {
    let raw = raw?;
    let value = match serde_json::from_str::<Value>(raw) {
        Ok(value) => value,
        Err(error) => {
            finding(
                out,
                DiagnosticKind::Missed,
                key,
                format!("Answer must be a JSON literal: {error}"),
            );
            return None;
        }
    };
    // Judge the answer's own text: `value` already holds the rounded f64, so
    // every later comparison would agree with the rounding.
    if let Some(token) = edit::inexact_integer(raw) {
        out.status = CompileStatus::Refused;
        finding(
            out,
            DiagnosticKind::Refused,
            key,
            format!(
                "Integer `{token}` is outside the exact integer range {} to {}; accepting it would silently round the answer. No value was applied.",
                i64::MIN,
                i64::MAX
            ),
        );
        return None;
    }
    if value.get("type").is_some() && value.get("value").is_some() {
        out.status = CompileStatus::Refused;
        finding(
            out,
            DiagnosticKind::Refused,
            key,
            "This literal-only slice does not accept answer objects with both type and value: they can be reinterpreted as constant declarations. No value was applied.",
        );
        return None;
    }
    if edit::has_expression(&value) {
        out.status = CompileStatus::Refused;
        finding(
            out,
            DiagnosticKind::Refused,
            key,
            "Literal-only authoring policy forbids introducing expression islands through values. No expression was applied.",
        );
        return None;
    }
    Some(value)
}

fn unknown_answers(
    request: &CompileRequest,
    recognized: &BTreeSet<&str>,
    out: &mut CompileOutcome,
) {
    for key in request
        .answers
        .keys()
        .filter(|key| !recognized.contains(key.as_str()))
    {
        finding(
            out,
            DiagnosticKind::Missed,
            key,
            "No current question owns this answer; it was not applied.",
        );
    }
}

fn finish(source: String, out: &mut CompileOutcome) {
    out.candidate = Some(source);
    let wf = match parse(out.candidate.as_deref().unwrap_or_default()) {
        Ok(wf) => wf,
        Err(error) => {
            finding(out, DiagnosticKind::Missed, "candidate", error.to_string());
            return;
        }
    };
    let report = nika_check::check(&wf);
    // Facets are observational in this slice: CREATE/EDIT status, candidate
    // and diagnostics stay unchanged. The compiler still grants no authority.
    pattern::observe(out.candidate.as_deref().unwrap_or_default(), &wf, &report);
    for slot in &report.slot_findings {
        if !out
            .questions
            .iter()
            .any(|question| question.key == slot.path)
        {
            let kind = if slot.path.starts_with("tasks.") || slot.path == "model" {
                QuestionType::Text
            } else {
                QuestionType::Literal
            };
            question(out, &slot.path, &slot.hint, kind);
        }
    }
    // These need a reader/registry: the source-only preview must never silently
    // graduate an unjudged dependency to a complete candidate.
    for task in &wf.tasks {
        use nika_schema::raw::{RawAction, RawInvokeTarget};
        let unjudged = match &task.value.action {
            RawAction::Invoke(invoke) => match &invoke.target {
                RawInvokeTarget::Workflow(_) => true,
                RawInvokeTarget::Tool(tool) => tool.value.starts_with("mcp:"),
            },
            RawAction::Agent(agent) => {
                !agent.skills.is_empty()
                    || agent
                        .tools
                        .iter()
                        .any(|tool| tool.value.starts_with("mcp:"))
            }
            _ => false,
        };
        if unjudged {
            finding(
                out,
                DiagnosticKind::Unknown,
                &task.value.id.value,
                "Source-only preview cannot resolve this child workflow, MCP registry entry or skill. No environment/admission claim is made.",
            );
        }
    }
    let unresolved = out
        .diagnostics
        .iter()
        .any(|d| d.kind != DiagnosticKind::Applied);
    if out.status != CompileStatus::Refused
        && out.questions.is_empty()
        && !unresolved
        && report.is_clean()
    {
        out.status = CompileStatus::Ready;
    } else if out.status != CompileStatus::Refused
        && out.questions.is_empty()
        && !unresolved
        && !report.is_clean()
    {
        // Nothing to ask and nothing else to report: the preview's own refusals are the
        // reason the candidate is not ready, and they must be visible without opening it.
        let refusals: Vec<String> = report
            .findings
            .iter()
            .take(3)
            .map(|f| {
                format!(
                    "Check refuses the candidate ({}): {}",
                    f.code.as_deref().unwrap_or(f.kind),
                    f.message
                )
            })
            .collect();
        for message in refusals {
            finding(out, DiagnosticKind::Unknown, "check_preview", message);
        }
    }
    out.requested_boundary = Some(report.permits.clone());
    out.check_preview = Some(CompilePreview {
        report,
        scope: PreviewScope::SourceOnly,
    });
}
