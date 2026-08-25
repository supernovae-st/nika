// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Injected by `check-dataflow-parity.sh` into the pre- and post-split
//! `nika-runtime` crates. It deliberately uses only the historical private
//! module names, which exist on both sides of the descent.

use std::collections::BTreeMap;

use nika_error::traits::NikaErrorCode;
use serde_json::{Value, json};

use crate::RuntimeError;
use crate::expr::{self, Scope};
use crate::jq;
use crate::record::TaskRecord;

fn runtime_error<E>(err: E) -> RuntimeError
where
    RuntimeError: From<E>,
{
    RuntimeError::from(err)
}

fn error_kind(err: &RuntimeError) -> &'static str {
    match err {
        RuntimeError::UnresolvedTemplate { .. } => "unresolved",
        RuntimeError::WhenUnsupported { .. } => "when-unsupported",
        RuntimeError::CelEval { .. } => "cel-eval",
        RuntimeError::OutputBinding { .. } => "output-binding",
        _ => "other",
    }
}

fn emit_error(label: &str, err: RuntimeError) {
    println!(
        "PARITY|{label}|{}|{}|{}|{}|{}",
        error_kind(&err),
        err.spec_code(),
        err.nika_code(),
        err,
        err.wire_message()
    );
}

#[test]
fn pre_post_corpus() {
    let records = BTreeMap::<String, TaskRecord>::new();
    let inputs = BTreeMap::<String, Value>::from([
        ("count".into(), json!(3)),
        ("name".into(), json!("Nika")),
    ]);
    let scope = Scope::workflow(&records, &inputs);

    let rendered = expr::render("hello ${{ inputs.name }}", &scope)
        .unwrap_or_else(|err| panic!("render fixture failed: {}", runtime_error(err)));
    println!("PARITY|cel-render|{rendered}");

    let gate = expr::eval_when("inputs.count >= 3", &scope)
        .unwrap_or_else(|err| panic!("gate fixture failed: {}", runtime_error(err)));
    println!("PARITY|cel-gate|{gate}");

    emit_error(
        "cel-unresolved",
        runtime_error(
            expr::render("${{ inputs.missing }}", &scope)
                .expect_err("missing input must be unresolved"),
        ),
    );
    emit_error(
        "cel-static",
        runtime_error(
            expr::eval_when("a < b < c", &scope)
                .expect_err("chained comparison must stay outside the subset"),
        ),
    );
    emit_error(
        "cel-type",
        runtime_error(
            expr::eval_when("inputs.count < 'x'", &scope)
                .expect_err("cross-type comparison must fail"),
        ),
    );

    let jq_input = json!({"items": [{"n": 2}, {"n": 5}]});
    let jq_sum = jq::eval_binding("sum", ".items | map(.n) | add", &jq_input)
        .unwrap_or_else(|err| panic!("jq fixture failed: {}", runtime_error(err)));
    println!("PARITY|jq-single|{jq_sum}");

    emit_error(
        "jq-cardinality",
        runtime_error(
            jq::eval_binding("many", ".items[]", &jq_input)
                .expect_err("stream must violate the single-value law"),
        ),
    );
    emit_error(
        "jq-runtime",
        runtime_error(
            jq::eval_binding("boom", "error(\"boom\")", &jq_input)
                .expect_err("jq error() must fail"),
        ),
    );
}
