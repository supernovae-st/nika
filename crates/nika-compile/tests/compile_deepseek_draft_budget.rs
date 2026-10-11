// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The catalog's thinking declaration must reach the emitted draft, not only a model picker: the
//! reasoning room in its cap, and the documented low effort for a draft read whole.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, compile};
use nika_compile_fidelity::fidelity::budget::LOW_EFFORT_DECLARED;
use serde_json::Value;

/// The Ready candidate `intent` compiles to on `model`, with its outcome.
fn ready(intent: &str, model: &str) -> (Value, CompileOutcome) {
    let request =
        CompileRequest::create(intent).answer("model", serde_json::to_string(model).unwrap());
    let out = compile(&request).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
    let source = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
    (source, out)
}

/// What the proposal says of an effort the assembler declared.
fn said(out: &CompileOutcome) -> Vec<&str> {
    out.diagnostics
        .iter()
        .filter(|d| d.kind == DiagnosticKind::Applied && d.target == "run.reasoning.effort")
        .map(|d| d.message.as_str())
        .collect()
}

#[test]
fn current_deepseek_drafts_budget_for_reasoning_and_the_structured_answer() {
    for model in ["deepseek/deepseek-v4-pro", "deepseek/deepseek-flash"] {
        let (source, out) = ready("Résume le texte fourni.", model);
        assert_eq!(source["model"], model);
        assert!(
            source["tasks"]["draft"]["infer"]["max_tokens"]
                .as_u64()
                .unwrap()
                >= 16384
        );
        assert!(source["tasks"]["draft"]["infer"]["schema"].is_object());
        assert!(out.check_preview.as_ref().unwrap().report.is_clean());
        // The row documents `low`, so the draft declares it and the proposal says so.
        assert_eq!(source["run"]["reasoning"]["effort"], "low", "{source}");
        assert_eq!(said(&out), [LOW_EFFORT_DECLARED], "{model}");
    }
}

/// The low effort is declared for no work beyond drafts, no constraint beside the draft, and no
/// seat whose row documents no `low`: the route keeps its own effort.
#[test]
fn other_work_a_constraint_or_another_seat_keeps_the_route_s_effort() {
    let flash = "deepseek/deepseek-flash";
    for (intent, model) in [
        ("Extrais la date et le montant du texte fourni.", flash),
        ("Résume le texte fourni. Garde un ton formel.", flash),
        ("Résume le texte fourni.", "deepseek/deepseek-chat"),
        ("Résume le texte fourni.", "mock/echo"),
    ] {
        let (source, out) = ready(intent, model);
        assert!(source["run"].is_null(), "{intent} on {model}: {source}");
        assert!(said(&out).is_empty(), "{intent} on {model}");
    }
}
