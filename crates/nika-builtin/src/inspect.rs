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
    fn each_view_routes_to_its_seam_method() {
        // Every valid view dispatches to the matching WorkflowIntrospect
        // method (NOT an error · the seam supplies the answer). Under the
        // NoWorkflow stand-in each view honestly reports « not available »
        // (F3: an explicit `available: false`, never zeros that masquerade
        // as a real empty run · `{ nodes: [] }` was indistinguishable from
        // a genuine empty DAG).
        for view in ["cost", "records", "dag_info", "threads"] {
            let out = inspect(&NoWorkflow, &args(serde_json::json!({ "view": view })))
                .unwrap_or_else(|e| panic!("{view} routes: {e:?}"));
            assert_eq!(out["available"], false, "{view} is honestly unavailable");
            assert_eq!(out["view"], view, "{view} echoes its view");
            assert!(
                out.get("reason")
                    .and_then(serde_json::Value::as_str)
                    .is_some(),
                "{view} carries a human reason"
            );
        }
    }

    #[test]
    fn unknown_view_is_a_finding() {
        let bad = inspect(&NoWorkflow, &args(serde_json::json!({ "view": "secrets" })));
        assert!(matches!(bad, Err(f) if f.code == "NIKA-BUILTIN-INSPECT-001"));
        let missing = inspect(&NoWorkflow, &args(serde_json::json!({})));
        assert!(missing.is_err());
    }
}
