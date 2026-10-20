// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used)]
//! The model parts a compile outcome carries: none until a judge states them, then each part with
//! the basis the judge gave, the sink it shapes and what the candidate's bytes do with it.

use nika_compile::{CompileRequest, ModelFound, ModelPart, ModelSink, PartBasis, compile};

#[test]
fn an_outcome_carries_no_model_parts_until_a_judge_states_them() {
    let out = compile(&CompileRequest::create(
        "Read ./in.txt and write it to ./out.txt.",
    ))
    .expect("compiles");
    assert_eq!(out.model_parts, None);
    assert_eq!(out.model_parts_unrun, None, "no judge ran, nothing to say");
}

#[test]
fn a_model_part_keeps_its_basis_sink_and_finding() {
    let quoted = ModelPart::new(
        PartBasis::Words {
            words: "un court résumé".to_owned(),
            result: "the summary written to out/summary.md".to_owned(),
        },
        Some(ModelSink::Task("save_summary".to_owned())),
        ModelFound::Missing,
    );
    assert_eq!(
        quoted.sink,
        Some(ModelSink::Task("save_summary".to_owned()))
    );
    assert_eq!(quoted.found, ModelFound::Missing);
    assert!(
        matches!(&quoted.basis, PartBasis::Words { words, .. } if words == "un court résumé"),
        "{quoted:?}"
    );
    let asked = ModelPart::new(
        PartBasis::Sink {
            cited: None,
            p: 0.83,
        },
        Some(ModelSink::Output("summary".to_owned())),
        ModelFound::Met {
            task: "summarize".to_owned(),
        },
    );
    assert!(
        matches!(asked.basis, PartBasis::Sink { cited: None, p } if (p - 0.83).abs() < 1e-9),
        "{asked:?}"
    );
    assert_eq!(
        asked.found,
        ModelFound::Met {
            task: "summarize".to_owned()
        }
    );
}
