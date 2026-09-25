// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ONE machine projection of a [`CompileOutcome`]: generation 1 of the Compile wire.
//!
//! Every transport prints this document (`nika compile --json`, Serve
//! `POST /v1/compile`), so adapters cannot drift on status words, question shapes,
//! the requested boundary or the Check preview. A transport may add a fact only it
//! owns (the CLI adds `written`); it never re-projects the outcome. This renders
//! typed results; it is not a second IR and carries no authoring semantics.

use serde_json::{Value, json};

use super::types::{
    AuthoringCognition, CompileOutcome, CompileStatus, DiagnosticKind, PreviewScope, QuestionType,
    TriggerKind, TriggerRequirement, TriggerStatus,
};

/// Generation of the Compile machine document. Only a breaking change bumps it.
pub const COMPILE_WIRE_VERSION: u32 = 1;

impl CompileStatus {
    /// The stable machine word for this status.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Incomplete => "incomplete",
            Self::Refused => "refused",
        }
    }
}

impl AuthoringCognition {
    /// The stable machine word for this authoring cognition policy. A transport
    /// that lets a caller NAME a cognition compares against this word.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::DeterministicOnly => "deterministicOnly",
            Self::ExplicitProvider => "explicitProvider",
            Self::ExplicitDecision => "explicitDecision",
        }
    }
}

impl DiagnosticKind {
    /// The stable machine word for this disposition.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Missed => "missed",
            Self::Unknown => "unknown",
            Self::RequiresHuman => "requiresHuman",
            Self::Refused => "refused",
        }
    }
}

impl TriggerKind {
    /// The stable machine word for this trigger kind.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Schedule => "schedule",
            Self::Webhook => "webhook",
            Self::Event => "event",
        }
    }
}

impl TriggerStatus {
    /// The stable machine word for this trigger status.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Satisfied => "satisfied",
            Self::RequiresBinding => "requires_binding",
            Self::Unsupported => "unsupported",
        }
    }
}

/// The trigger requirement as the wire carries it: every field, nullable when unread.
fn trigger_document(trigger: &TriggerRequirement) -> Value {
    json!({
        "kind": trigger.kind.word(),
        "source_hint": trigger.source_hint,
        "event_hint": trigger.event_hint,
        "cadence": trigger.cadence,
        "cron": trigger.cron,
        "at": trigger.at,
        "payload_input": trigger.payload_input,
        "status": trigger.status.word(),
        "timezone": trigger.timezone,
        "missed": trigger.missed,
        "overlap": trigger.overlap,
        "ceiling": trigger.ceiling,
    })
}

/// Project a typed outcome onto the generation-1 machine document.
///
/// The preview is a REVIEW of source only: nothing here grants authority or
/// admits the candidate, and Run judges it again under its real environment.
#[must_use]
pub fn outcome_document(out: &CompileOutcome) -> Value {
    let questions: Vec<Value> = out
        .questions
        .iter()
        .map(|q| {
            let mut question = json!({
                "key": q.key,
                "label": q.label,
                "type": match q.answer_type {
                    QuestionType::Text => "text",
                    QuestionType::Literal => "literal",
                    QuestionType::Choice => "choice",
                },
                "why": q.why,
                "mandatory": q.mandatory,
            });
            if !q.options.is_empty() {
                question["options"] = json!(
                    q.options
                        .iter()
                        .map(|o| json!({"key": o.key, "label": o.label}))
                        .collect::<Vec<_>>()
                );
            }
            question
        })
        .collect();
    let diagnostics: Vec<Value> = out
        .diagnostics
        .iter()
        .map(|d| json!({"kind": d.kind.word(), "target": d.target, "message": d.message}))
        .collect();
    let preview = out.check_preview.as_ref().map(|p| {
        json!({
            "scope": match p.scope {
                PreviewScope::SourceOnly => "sourceOnly",
            },
            "report": p.report,
        })
    });
    let mut document = json!({
        "compile_version": if out.provenance.authoring.is_some() { 2 } else { COMPILE_WIRE_VERSION },
        "status": out.status.word(),
        "candidate": out.candidate,
        "questions": questions,
        "diagnostics": diagnostics,
        "requested_boundary": out.requested_boundary,
        "requested_trigger": out.requested_trigger.as_ref().map(trigger_document),
        "check_preview": preview,
        "provenance": {
            "compiler_version": out.provenance.compiler_version,
            "spec_pin": out.provenance.spec_pin,
            "skeleton": out.provenance.skeleton,
            "cognition": out.provenance.cognition.word(),
            "suggested_file": out.provenance.suggested_file,
        },
    });
    if let Some(strategy) = out.provenance.strategy {
        document["provenance"]["strategy"] = json!(strategy.word());
    }
    if let Some(plan) = &out.provenance.plan {
        document["provenance"]["plan"] = plan.clone();
    }
    if let Some(decision) = &out.provenance.decision {
        document["provenance"]["decision"] = decision.clone();
    }
    if let Some(receipt) = &out.provenance.authoring {
        document["provenance"]["authoring"] = json!({
            "model": receipt.model, "calls": receipt.calls,
            "input_tokens": receipt.input_tokens, "output_tokens": receipt.output_tokens,
            "elapsed_ms": receipt.elapsed_ms,
            "sampling": {"temperature": null, "seed": null, "effective": "providerDefaultUnknown"},
            "context": receipt.context,
            "backend": receipt.backend,
        });
    }
    document
}
