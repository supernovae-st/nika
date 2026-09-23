// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! An answered rule is emitted as the literal program of its task, where the Check ladder's
//! static jq compile-check (NIKA-VAR-005) judges it. The morning audit of 2026-09-22 (§4.3):
//! a jq expression asked to a human was accepted without validation and failed at run; the
//! preview said clean because the assembler bound the answer as `${{ const.rule_expression }}`,
//! a templated program the checker never reads.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus, compile_with_provider};
use serde_json::{Value, json};

mod common;
use common::{Provider, policy};

const INTENT: &str =
    "Read ./data/orders.csv. Harmonise the totals per country and write them to ./out/totals.json.";

fn plan() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./data/orders.csv","evidence":"Read ./data/orders.csv"},
        {"op":"compute","detail":"the totals per country","evidence":"Harmonise the totals per country"}],
      "effects":[{"verb":"write","target":"./out/totals.json","policy":"automatic","evidence":"write them to ./out/totals.json"}],
      "obligations":[],"constraints":[],"unknowns":[]})
}

async fn answered(rule: &str) -> nika_compile::CompileOutcome {
    let provider = Provider::new(plan());
    let req = CompileRequest::create(INTENT)
        .with_authoring_policy(policy())
        .answer("const.rule_expression", format!("{rule:?}"));
    compile_with_provider(&req, &provider).await.unwrap()
}

#[tokio::test]
async fn a_rule_that_does_not_compile_is_refused_by_the_preview_not_at_run() {
    let out = answered("[.records[] | select(").await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.target == "check_preview" && d.message.contains("NIKA-VAR-005")),
        "{out:#?}"
    );
    assert!(
        !out.check_preview
            .as_ref()
            .expect("a preview")
            .report
            .is_clean()
    );
}

#[tokio::test]
async fn a_rule_that_compiles_is_the_literal_program_of_its_task() {
    let out = answered(".records").await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
    assert_eq!(
        doc["tasks"]["compute"]["invoke"]["args"]["expression"], ".records",
        "{doc:#}"
    );
    assert!(doc["const"].get("rule_expression").is_none(), "{doc:#}");
}
