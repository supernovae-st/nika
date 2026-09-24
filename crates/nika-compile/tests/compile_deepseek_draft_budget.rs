// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The catalog's thinking declaration must reach the emitted draft, not only a model picker.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus, compile};
use serde_json::Value;

#[test]
fn current_deepseek_drafts_budget_for_reasoning_and_the_structured_answer() {
    for model in ["deepseek/deepseek-v4-pro", "deepseek/deepseek-flash"] {
        let request = CompileRequest::create("Résume le texte fourni.")
            .answer("model", serde_json::to_string(model).unwrap());
        let out = compile(&request).unwrap();
        assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
        let source: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
        assert_eq!(source["model"], model);
        assert!(
            source["tasks"]["draft"]["infer"]["max_tokens"]
                .as_u64()
                .unwrap()
                >= 16384
        );
        assert!(source["tasks"]["draft"]["infer"]["schema"].is_object());
        assert!(out.check_preview.as_ref().unwrap().report.is_clean());
    }
}
