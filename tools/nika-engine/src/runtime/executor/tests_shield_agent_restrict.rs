//! Sprint 2 Item 3a — agent tool restriction tests.
//!
//! Verifies the runner-side hot-path integration of `AgentToolPolicy`:
//! - tainted agents have dangerous tools removed
//! - elevated agents keep all tools (even with tainted inputs)
//! - clean (trusted) agents keep all tools
//! - the dispatcher's `agent_has_untrusted_inputs` correctly classifies
//!   bindings sourced from Task / Input / LoopVar

#![cfg(test)]

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

use crate::binding::ResolvedBindings;
use crate::event::EventLog;
use crate::runtime::boot::PolicyConfig;
use crate::runtime::TaskExecutor;
use crate::store::{RunContext, TaskResult};
use nika_core::trust::{InvocationSource, TrustLevel};

fn build_executor(policy: PolicyConfig) -> TaskExecutor {
    let event_log = EventLog::new();
    TaskExecutor::with_policy("mock", None, None, event_log, Some(policy), None, None)
        .expect("executor build")
}

fn insert_with_trust(ctx: &RunContext, task_id: &str, output: serde_json::Value, trust: TrustLevel) {
    let arc_id: Arc<str> = Arc::from(task_id);
    let mut tr = TaskResult::success(output, Duration::from_millis(1));
    tr.trust_level = trust;
    ctx.insert(arc_id, tr);
}

#[tokio::test]
async fn agent_has_untrusted_inputs_detects_tainted_task_source() {
    let executor = build_executor(PolicyConfig::default());
    let ctx = RunContext::new(InvocationSource::Cli);
    insert_with_trust(&ctx, "fetch_data", json!("payload"), TrustLevel::Untrusted);

    let mut bindings = ResolvedBindings::new();
    bindings.set_with_source("data", json!("payload"), "fetch_data");

    assert!(
        executor.agent_has_untrusted_inputs(&bindings, &ctx),
        "tainted task source must be detected"
    );
}

#[tokio::test]
async fn agent_has_untrusted_inputs_returns_false_for_trusted_task_source() {
    let executor = build_executor(PolicyConfig::default());
    let ctx = RunContext::new(InvocationSource::Cli);
    insert_with_trust(&ctx, "literal", json!("value"), TrustLevel::Trusted);

    let mut bindings = ResolvedBindings::new();
    bindings.set_with_source("data", json!("value"), "literal");

    assert!(!executor.agent_has_untrusted_inputs(&bindings, &ctx));
}

#[tokio::test]
async fn agent_has_untrusted_inputs_detects_serve_input_source() {
    use crate::binding::{WithEntry, WithSpec};
    use nika_core::binding::types::{BindingPath, BindingSource};
    use rustc_hash::FxHashMap;

    let executor = build_executor(PolicyConfig::default());
    let ctx = RunContext::new(InvocationSource::Serve);
    let mut inputs = FxHashMap::default();
    inputs.insert("topic".to_string(), json!("untrusted from HTTP"));
    ctx.set_inputs(inputs);

    let mut spec = WithSpec::default();
    spec.insert(
        "topic".to_string(),
        WithEntry::simple(BindingPath {
            source: BindingSource::Input("topic".into()),
            segments: vec![],
        }),
    );
    let (bindings, _evts) =
        ResolvedBindings::from_with_spec_traced(Some(&spec), &ctx, &Arc::from("test"))
            .expect("bindings");

    assert!(
        executor.agent_has_untrusted_inputs(&bindings, &ctx),
        "Serve-mode workflow inputs must be classified as untrusted"
    );
}

#[tokio::test]
async fn agent_has_untrusted_inputs_returns_false_for_cli_input_source() {
    use crate::binding::{WithEntry, WithSpec};
    use nika_core::binding::types::{BindingPath, BindingSource};
    use rustc_hash::FxHashMap;

    let executor = build_executor(PolicyConfig::default());
    let ctx = RunContext::new(InvocationSource::Cli);
    let mut inputs = FxHashMap::default();
    inputs.insert("topic".to_string(), json!("user typed this"));
    ctx.set_inputs(inputs);

    let mut spec = WithSpec::default();
    spec.insert(
        "topic".to_string(),
        WithEntry::simple(BindingPath {
            source: BindingSource::Input("topic".into()),
            segments: vec![],
        }),
    );
    let (bindings, _evts) =
        ResolvedBindings::from_with_spec_traced(Some(&spec), &ctx, &Arc::from("test"))
            .expect("bindings");

    assert!(
        !executor.agent_has_untrusted_inputs(&bindings, &ctx),
        "CLI inputs are Trusted — must not flag tools for restriction"
    );
}

#[test]
fn agent_tool_policy_strips_dangerous_tools_when_tainted_and_not_elevated() {
    use nika_core::capabilities::AgentToolPolicy;

    let dangerous: Arc<[String]> = Arc::from(vec![
        "nika:write".to_string(),
        "nika:exec".to_string(),
        "nika:run".to_string(),
        "nika:fetch".to_string(),
    ]);
    let policy = AgentToolPolicy::for_task(true, false, dangerous);
    let tools = vec![
        "novanet::search".to_string(),
        "nika:read".to_string(),
        "nika:write".to_string(),
        "nika:exec".to_string(),
    ];
    let (kept, removed) = policy.apply_to(tools);
    assert_eq!(kept.len(), 2);
    assert!(kept.contains(&"novanet::search".to_string()));
    assert!(kept.contains(&"nika:read".to_string()));
    assert_eq!(removed.len(), 2);
}

#[test]
fn agent_tool_policy_keeps_everything_when_elevated_even_with_taint() {
    use nika_core::capabilities::AgentToolPolicy;

    let dangerous: Arc<[String]> = Arc::from(vec!["nika:write".to_string()]);
    let policy = AgentToolPolicy::for_task(true, true, dangerous);
    let tools = vec![
        "nika:write".to_string(),
        "nika:read".to_string(),
        "novanet::search".to_string(),
    ];
    let (kept, removed) = policy.apply_to(tools);
    assert_eq!(kept.len(), 3);
    assert!(removed.is_empty());
}
