// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Introspection builtin (1) — `nika:inspect` · 4 views (stdlib §inspect).
//! Reads live run state through the injected [`WorkflowIntrospect`] seam.

use crate::{Args, BuiltinFailure, BuiltinOutcome, WorkflowIntrospect, req_str};

/// `nika:inspect` — `view:`-discriminated workflow introspection.
pub(crate) fn inspect<W: WorkflowIntrospect>(workflow: &W, args: &Args) -> BuiltinOutcome {
    const C: &str = "NIKA-BUILTIN-INSPECT-001";
    match req_str(args, "view", C)? {
        "cost" => Ok(workflow.cost()),
        "records" => Ok(workflow.records()),
        "dag_info" => Ok(workflow.dag_info()),
        "threads" => Ok(workflow.threads()),
        other => Err(BuiltinFailure::new(
            C,
            format!("`view:` `{other}` is not cost|records|dag_info|threads"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NoWorkflow;

    fn args(v: serde_json::Value) -> Args {
        match v {
            serde_json::Value::Object(map) => map,
            other => panic!("test arg must be an object, got {other}"),
        }
    }

    #[test]
    fn each_view_returns_its_shape() {
        let cost = inspect(&NoWorkflow, &args(serde_json::json!({ "view": "cost" }))).expect("ok");
        assert!(cost.get("total_usd").is_some());
        let records =
            inspect(&NoWorkflow, &args(serde_json::json!({ "view": "records" }))).expect("ok");
        assert!(records.get("tasks").is_some());
        let dag = inspect(
            &NoWorkflow,
            &args(serde_json::json!({ "view": "dag_info" })),
        )
        .expect("ok");
        assert!(dag.get("waves").is_some());
        let threads =
            inspect(&NoWorkflow, &args(serde_json::json!({ "view": "threads" }))).expect("ok");
        assert!(threads.get("active").is_some());
    }

    #[test]
    fn unknown_view_is_a_finding() {
        let bad = inspect(&NoWorkflow, &args(serde_json::json!({ "view": "secrets" })));
        assert!(matches!(bad, Err(f) if f.code == "NIKA-BUILTIN-INSPECT-001"));
        let missing = inspect(&NoWorkflow, &args(serde_json::json!({})));
        assert!(missing.is_err());
    }
}
