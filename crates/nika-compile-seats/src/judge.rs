// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The untrusted state a judging seat reads (R4 A11): what every question of a verdict and its
//! repair show the judge, apart from the compiler-owned reference they also carry. The verifier
//! of `nika-compile-cognition` asks the questions and weighs the answers; this state moved here
//! at that crate's size cap (2026-10-08).

use nika_compile::surface::{EditChange, Input};
use nika_compile::{CompileOutcome, CompileRequest};
use serde_json::{Value, json};

/// The state every question and every repair carries: the request as compiled and as first
/// stated, its answers, the observed world and the candidate's bytes. The first statement is
/// the one the binding holds ([`Binding::of`](nika_compile::surface::Binding::of)): the
/// preserved original request, else the submitted text the compiled request was folded or
/// clarified from. A revision in words is judged on the request it resolves (`intent`); the
/// request of the base it revises, which the change partly supersedes, is never shown as its
/// first statement: it stays apart as history (`revision.base_request`) beside the change as
/// the human stated it (`revision.change`).
#[must_use]
pub fn state(intent: &str, request: &CompileRequest, candidate: &str) -> Value {
    let (submitted, change) = match &request.input {
        Input::Create(text) if text != intent => (Some(text.clone()), None),
        Input::Edit {
            change: EditChange::Text(words),
            ..
        } => (None, Some(words)),
        _ => (None, None),
    };
    let first = request.original_intent.clone().or(submitted);
    // A revision judged on the earlier request with the change appended (`… Change: …`) is
    // told so: its replaced clauses are still in the words, superseded by the change.
    let appended = request.original_intent.is_some()
        && nika_compile::revise_intent(request).is_some_and(|resolved| resolved == intent);
    let unknown = first.is_none();
    let (first, revision) = match change {
        Some(change) => {
            let mut revision = json!({"change": change, "base_request": first});
            if appended {
                revision["appended"] = json!(true);
            }
            // A base whose request is unknown is shown whole: the change is judged over it.
            if let (true, Input::Edit { source, .. }) = (unknown, &request.input) {
                revision["base_nika"] = json!(source);
            }
            (None, revision)
        }
        None => (first, Value::Null),
    };
    let mut state = json!({
        "request": intent,
        "original_request": first,
        "answers": request.answers,
        "observed": request.knowledge,
        "candidate_nika": candidate,
    });
    if !revision.is_null() {
        state["revision"] = revision;
    }
    state
}

/// A revision the compiler applied over the complete document (its record says so) is judged as
/// the base with exactly the change: the base shown whole, whatever request it answers.
pub fn over_document(base: &mut Value, request: &CompileRequest, out: &CompileOutcome) {
    let applied = (out.provenance.decision.as_ref())
        .is_some_and(|decision| decision.get("document_revision").is_some());
    if let (true, Some(revision), Input::Edit { source, .. }) =
        (applied, base.get_mut("revision"), &request.input)
    {
        revision["base_nika"] = json!(source);
        revision["over_document"] = json!(true);
    }
}
