// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Injected by `check-dataflow-parity.sh` into the pre- and post-split
//! `nika-runtime` crates. It deliberately uses only the historical private
//! module names, which exist on both sides of the descent.

#![allow(unexpected_cfgs)]

use std::collections::BTreeMap;

use nika_error::traits::NikaErrorCode;
use nika_types::timestamp::Timestamp;
use serde_json::{Value, json};

use crate::RuntimeError;
use crate::expr::{self, Scope};
use crate::jq;
use crate::record::{
    TaskErrorRecord, TaskRecord, TaskStatus, TerminalCause, failure_cause, legal, outcome_json,
    render_value,
};

fn scope_with_context<'a>(
    records: &'a BTreeMap<String, TaskRecord>,
    inputs: &'a BTreeMap<String, Value>,
    consts: &'a BTreeMap<String, Value>,
    secrets: &'a BTreeMap<String, Value>,
    with_ns: &'a BTreeMap<String, Value>,
    item: &'a Value,
) -> Scope<'a> {
    #[cfg(dataflow_post_split)]
    {
        Scope::workflow_with_value_authorities(records, inputs, consts, secrets)
            .with_task_context(Some(with_ns), Some(item), Some(4), None)
    }
    #[cfg(not(dataflow_post_split))]
    {
        Scope {
            records,
            inputs,
            consts,
            secrets,
            with_ns: Some(with_ns),
            item: Some(item),
            index: Some(4),
            permits: None,
        }
    }
}

fn runtime_error<E>(err: E) -> RuntimeError
where
    RuntimeError: From<E>,
{
    RuntimeError::from(err)
}

fn task_error(code: &str, message: &str, transient: bool) -> TaskErrorRecord {
    #[cfg(dataflow_post_split)]
    {
        TaskErrorRecord::new(code, message, transient)
    }
    #[cfg(not(dataflow_post_split))]
    {
        TaskErrorRecord {
            code: code.to_owned(),
            message: message.to_owned(),
            transient,
        }
    }
}

fn eval_jq(name: &str, program: &str, input: &Value) -> Result<Value, RuntimeError> {
    #[cfg(dataflow_post_split)]
    {
        jq::eval_binding(
            name,
            program,
            input,
            nika_cap::JqClock::at(Timestamp::from_unix_ns(1_700_000_000_125_000_000)),
        )
        .map_err(runtime_error)
    }
    #[cfg(not(dataflow_post_split))]
    {
        jq::eval_binding(name, program, input).map_err(runtime_error)
    }
}

fn outcome_record(status: TaskStatus, cause: TerminalCause) -> TaskRecord {
    let mut record = TaskRecord::unran(status, cause);
    match status {
        TaskStatus::Success => {
            record.output = json!({"ok": true});
            record.attempts = Some(2);
            if cause == TerminalCause::Recovered {
                record.recovered_from = Some(task_error("NIKA-X", "recovered", true));
            }
        }
        TaskStatus::Failure => {
            record.error = Some(task_error("NIKA-X", "failed", false));
            record.attempts = Some(3);
        }
        TaskStatus::Skipped if cause == TerminalCause::ErrorSkip => {
            record.error = Some(task_error("NIKA-X", "skipped", false));
        }
        TaskStatus::Skipped | TaskStatus::Cancelled => {}
        #[cfg(dataflow_post_split)]
        _ => panic!("parity corpus has no payload for a future canonical status"),
    }
    record
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
    let mut record = TaskRecord::unran(TaskStatus::Success, TerminalCause::Recovered);
    record.output = json!({"z": 2, "a": [3, 1]});
    record.attempts = Some(2);
    record.started_at = Some(Timestamp::from_unix_ms(1_700_000_000_123));
    record.ended_at = Some(Timestamp::from_unix_ms(1_700_000_000_456));
    record.duration_ms = Some(333);
    record.recovered_from = Some(task_error("NIKA-RECOVER-001", "repaired", true));
    record.named.insert("answer".into(), json!(42));
    let records = BTreeMap::from([("build".into(), record.clone())]);
    let inputs = BTreeMap::<String, Value>::from([
        ("count".into(), json!(3)),
        ("name".into(), json!("Nika")),
    ]);
    let consts = BTreeMap::from([("region".into(), json!("eu"))]);
    let secrets = BTreeMap::from([("token".into(), json!("sealed"))]);
    let with_ns = BTreeMap::from([("mode".into(), json!("strict"))]);
    let item = json!({"name": "butterfly"});
    let scope = scope_with_context(
        &records, &inputs, &consts, &secrets, &with_ns, &item,
    );

    let rendered = expr::render("hello ${{ inputs.name }}", &scope)
        .unwrap_or_else(|err| panic!("render fixture failed: {}", runtime_error(err)));
    println!("PARITY|cel-render|{rendered}");

    let gate = expr::eval_when("inputs.count >= 3", &scope)
        .unwrap_or_else(|err| panic!("gate fixture failed: {}", runtime_error(err)));
    println!("PARITY|cel-gate|{gate}");

    let namespaces = expr::render(
        "${{ tasks.build.output.a[0] }}|${{ tasks.build.answer }}|${{ const.region }}|${{ secrets.token }}|${{ with.mode }}|${{ item.name }}|${{ index }}",
        &scope,
    )
    .unwrap_or_else(|err| panic!("namespace fixture failed: {}", runtime_error(err)));
    println!("PARITY|cel-namespaces|{namespaces}");

    for field in [
        "output",
        "status",
        "cause",
        "started_at",
        "ended_at",
        "duration_ms",
        "answer",
        "error",
    ] {
        let value = record.field(field).expect("reserved/named field resolves");
        println!("PARITY|record-field-{field}|{}", render_value(&value));
    }
    println!("PARITY|record-member-status|{}", record.status.as_str());
    println!("PARITY|record-member-cause|{}", record.cause.as_str());
    println!("PARITY|record-member-output|{}", render_value(&record.output));
    println!("PARITY|record-member-integrity|{}", record.integrity.as_str());
    println!(
        "PARITY|record-member-error|{}",
        record.error.as_ref().map_or(Value::Null, TaskErrorRecord::to_value)
    );
    println!("PARITY|record-member-attempts|{:?}", record.attempts);
    println!(
        "PARITY|record-member-recovered-from|{}",
        record
            .recovered_from
            .as_ref()
            .map_or(Value::Null, TaskErrorRecord::to_value)
    );
    println!("PARITY|record-member-started-at|{:?}", record.started_at);
    println!("PARITY|record-member-ended-at|{:?}", record.ended_at);
    println!("PARITY|record-member-duration-ms|{:?}", record.duration_ms);
    println!(
        "PARITY|record-member-named|{}",
        render_value(&Value::Object(record.named.clone().into_iter().collect()))
    );
    println!("PARITY|record-outcome|{}", outcome_json(&record));
    println!(
        "PARITY|record-canonical|{}",
        render_value(&json!({"z": {"b": 2, "a": 1}, "a": [3, 2]}))
    );
    let statuses = [TaskStatus::Success, TaskStatus::Failure, TaskStatus::Skipped, TaskStatus::Cancelled];
    let causes = [
        TerminalCause::Normal, TerminalCause::Recovered, TerminalCause::VerbError,
        TerminalCause::Timeout, TerminalCause::RetryExhausted, TerminalCause::Gate,
        TerminalCause::ErrorSkip, TerminalCause::Upstream, TerminalCause::Operator,
        TerminalCause::Budget,
    ];
    for status in statuses {
        for cause in causes {
            println!(
                "PARITY|record-transition|{}|{}|{}",
                status.as_str(), cause.as_str(), legal(status, cause)
            );
            if legal(status, cause) {
                println!(
                    "PARITY|record-outcome-{}-{}|{}",
                    status.as_str(), cause.as_str(), outcome_json(&outcome_record(status, cause))
                );
            }
        }
    }
    for (code, attempts) in [("NIKA-TIMEOUT-001", 1), ("NIKA-X", 1), ("NIKA-X", 2)] {
        println!(
            "PARITY|failure-cause|{code}|{attempts}|{}",
            failure_cause(&task_error(code, "x", false), attempts).as_str()
        );
    }

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
    let jq_sum = eval_jq("sum", ".items | map(.n) | add", &jq_input)
        .unwrap_or_else(|err| panic!("jq fixture failed: {}", runtime_error(err)));
    println!("PARITY|jq-single|{jq_sum}");

    emit_error(
        "jq-cardinality",
        runtime_error(
            eval_jq("many", ".items[]", &jq_input)
                .expect_err("stream must violate the single-value law"),
        ),
    );
    emit_error(
        "jq-runtime",
        runtime_error(
            eval_jq("boom", "error(\"boom\")", &jq_input)
                .expect_err("jq error() must fail"),
        ),
    );

    let oversized = Value::String("x".repeat(16 * 1024 * 1024));
    emit_error(
        "jq-size-limit",
        runtime_error(
            eval_jq("oversized", ".", &oversized)
                .expect_err("the evaluator path must enforce the 16 MiB ceiling"),
        ),
    );
}
