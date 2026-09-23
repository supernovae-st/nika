// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a candidate document does, read from its literal projection as the plan the
//! deterministic reader states for an intent: the operations its tasks perform (a model
//! extraction, a classification, a draft, a fetch), the effects it reaches with their policy
//! (behind a `nika:prompt` gate or automatic), the obligations it carries (a bounded retry or
//! agent loop, a deduplication), the literals it binds. The native door records it beside
//! the candidate so provenance says what was BUILT — the reader's plan says what was READ —
//! and a judge reads the native strategy through the same record as the others.
//! Observational, never authority: a projection of the bytes, not a reading of the intent.
use serde_json::Value;

use super::plan::{
    Binding, Effect, EffectPolicy, EffectVerb, Obligation, ObligationKind, Op, Plan, Step,
};

/// The plan a candidate document states by its structure; an empty plan for a document
/// without tasks.
#[must_use]
pub fn plan_of_document(doc: &Value) -> Plan {
    let mut plan = Plan::default();
    let Some(tasks) = doc.get("tasks").and_then(Value::as_object) else {
        return plan;
    };
    let gates: Vec<&str> = tasks
        .iter()
        .filter(|(_, task)| tool_of(task) == Some("nika:prompt"))
        .map(|(id, _)| id.as_str())
        .collect();
    for (id, task) in tasks {
        let evidence = format!("tasks.{id}");
        if let Some(infer) = task.get("infer") {
            let op = if infer.get("schema").is_some() {
                if names_categories(infer) {
                    Op::Classify
                } else {
                    Op::Extract
                }
            } else {
                Op::Draft
            };
            plan.push_step(step(op, &evidence, prompt_head(infer)));
        }
        if let Some(agent) = task.get("agent") {
            plan.push_step(step(Op::Draft, &evidence, prompt_head(agent)));
            if let Some(turns) = agent.get("max_turns").and_then(Value::as_u64) {
                plan.obligations.push(Obligation::new(
                    ObligationKind::RetryBound(bounded(turns)),
                    evidence.clone(),
                ));
            }
        }
        if let Some(attempts) = task
            .get("retry")
            .and_then(|r| r.get("max_attempts"))
            .and_then(Value::as_u64)
        {
            plan.obligations.push(Obligation::new(
                ObligationKind::RetryBound(bounded(attempts)),
                evidence.clone(),
            ));
        }
        let policy = if refers_to_a_gate(task, &gates) {
            EffectPolicy::HumanFirst
        } else {
            EffectPolicy::Automatic
        };
        let args = task.get("invoke").and_then(|i| i.get("args"));
        match tool_of(task) {
            Some("nika:fetch") => {
                let url = arg(args, "url");
                if is_post(args) {
                    plan.effects
                        .push(effect(EffectVerb::Send, &url, &evidence, policy));
                } else {
                    plan.push_step(step(Op::Fetch, &evidence, url.clone()));
                }
                bind(&mut plan, "url", url);
            }
            Some("nika:read" | "nika:grep") => bind(&mut plan, "path", arg(args, "path")),
            Some("nika:glob") => bind(&mut plan, "path", arg(args, "pattern")),
            Some("nika:write" | "nika:edit") => {
                let path = arg(args, "path");
                plan.effects
                    .push(effect(EffectVerb::Write, &path, &evidence, policy));
                bind(&mut plan, "path", path);
            }
            Some("nika:notify") => {
                let target = arg(args, "target");
                plan.effects
                    .push(effect(EffectVerb::Notify, &target, &evidence, policy));
                bind(&mut plan, "url", target);
            }
            Some("nika:emit") => {
                plan.effects.push(effect(
                    EffectVerb::Send,
                    &arg(args, "event"),
                    &evidence,
                    policy,
                ));
            }
            Some("nika:jq") if is_dedup(args) => {
                plan.obligations
                    .push(Obligation::new(ObligationKind::Dedup, evidence.clone()));
            }
            Some(tool) if tool.starts_with("mcp:") => {
                plan.push_step(step(Op::Lookup, &evidence, tool.to_owned()));
            }
            _ => {}
        }
    }
    plan
}

/// What a revision changed between two documents: the tasks added, removed and changed (by
/// id, a task compared whole), the effects and operations added and removed (by their words).
#[must_use]
pub fn delta(base: &Value, revised: &Value) -> Value {
    let empty = serde_json::Map::new();
    let before = base
        .get("tasks")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let after = revised
        .get("tasks")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let added: Vec<&String> = after.keys().filter(|k| !before.contains_key(*k)).collect();
    let removed: Vec<&String> = before.keys().filter(|k| !after.contains_key(*k)).collect();
    let changed: Vec<&String> = after
        .iter()
        .filter(|(k, v)| before.get(*k).is_some_and(|b| b != *v))
        .map(|(k, _)| k)
        .collect();
    let words = |plan: &Plan| -> (Vec<String>, Vec<String>) {
        (
            plan.steps.iter().map(|s| s.op.word().to_owned()).collect(),
            plan.effects
                .iter()
                .map(|e| format!("{} {} ({})", e.verb.word(), e.target, e.policy.word()))
                .collect(),
        )
    };
    let (ops_before, effects_before) = words(&plan_of_document(base));
    let (ops_after, effects_after) = words(&plan_of_document(revised));
    let diff = |a: &[String], b: &[String]| -> Vec<String> {
        a.iter().filter(|x| !b.contains(x)).cloned().collect()
    };
    serde_json::json!({
        "tasks_added": added,
        "tasks_removed": removed,
        "tasks_changed": changed,
        "operations_added": diff(&ops_after, &ops_before),
        "operations_removed": diff(&ops_before, &ops_after),
        "effects_added": diff(&effects_after, &effects_before),
        "effects_removed": diff(&effects_before, &effects_after),
        "unchanged_tasks": after.keys().filter(|k| before.get(*k) == after.get(*k)).count(),
    })
}

fn step(op: Op, evidence: &str, detail: String) -> Step {
    Step {
        op,
        evidence: evidence.to_owned(),
        detail,
        categories: Vec::new(),
    }
}

fn effect(verb: EffectVerb, target: &str, evidence: &str, policy: EffectPolicy) -> Effect {
    Effect {
        verb,
        target: target.to_owned(),
        evidence: evidence.to_owned(),
        policy,
        policy_literal: None,
    }
}

fn bind(plan: &mut Plan, role: &'static str, literal: String) {
    if !literal.is_empty() && !literal.contains("${{") {
        plan.bindings.push(Binding::new(role, literal));
    }
}

fn tool_of(task: &Value) -> Option<&str> {
    task.get("invoke")?.get("tool")?.as_str()
}

fn arg(args: Option<&Value>, name: &str) -> String {
    args.and_then(|a| a.get(name))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn is_post(args: Option<&Value>) -> bool {
    args.and_then(|a| a.get("method"))
        .and_then(Value::as_str)
        .is_some_and(|m| m.eq_ignore_ascii_case("post"))
}

fn is_dedup(args: Option<&Value>) -> bool {
    args.and_then(|a| a.get("expression").or_else(|| a.get("filter")))
        .and_then(Value::as_str)
        .is_some_and(|e| e.contains("unique"))
}

/// A schema whose values are an enumeration names categories: the model classifies.
fn names_categories(infer: &Value) -> bool {
    infer
        .get("schema")
        .map(Value::to_string)
        .is_some_and(|s| s.contains("\"enum\""))
}

/// The first line of the prompt, bounded, as the operation's object.
fn prompt_head(verb: &Value) -> String {
    verb.get("prompt")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(120)
        .collect()
}

/// An effect task that binds, follows or is conditioned on a `nika:prompt` task is gated
/// by it: `with: { approved: "${{ tasks.<gate>.output }}" }`, `after: { <gate>: success }`,
/// `when: "${{ … }}"` naming the gate.
fn refers_to_a_gate(task: &Value, gates: &[&str]) -> bool {
    if gates.is_empty() {
        return false;
    }
    let mentions = |value: Option<&Value>| {
        value
            .map(Value::to_string)
            .is_some_and(|text| gates.iter().any(|g| text.contains(&format!("tasks.{g}."))))
    };
    let after_gate = task
        .get("after")
        .and_then(Value::as_object)
        .is_some_and(|after| after.keys().any(|k| gates.contains(&k.as_str())));
    after_gate || mentions(task.get("with")) || mentions(task.get("when"))
}

fn bounded(n: u64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::plan_of_document;
    use crate::plan::{EffectPolicy, EffectVerb, ObligationKind, Op};

    #[test]
    fn a_gated_recap_states_its_operations_effects_and_bindings() {
        let doc = json!({
            "nika": "recap",
            "tasks": {
                "read_tickets": {"invoke": {"tool": "nika:read", "args": {"path": "./tickets.json"}}},
                "open_only": {"with": {"doc": "${{ tasks.read_tickets.output }}"},
                    "invoke": {"tool": "nika:jq", "args": {"input": "${{ with.doc }}", "expression": "fromjson | map(select(.status == \"open\")) | unique_by(.id)"}}},
                "summarize": {"with": {"t": "${{ tasks.open_only.output }}"},
                    "infer": {"prompt": "Summarize these tickets: ${{ with.t }}", "max_tokens": 400}},
                "review": {"with": {"s": "${{ tasks.summarize.output }}"},
                    "invoke": {"tool": "nika:prompt", "args": {"message": "Send this?"}}},
                "send": {"with": {"approved": "${{ tasks.review.output }}", "s": "${{ tasks.summarize.output }}"},
                    "when": "${{ with.approved == true }}",
                    "invoke": {"tool": "nika:notify", "args": {"channel": "webhook", "target": "http://127.0.0.1:8793/hook", "message": "${{ with.s }}"}}},
                "archive": {"with": {"s": "${{ tasks.summarize.output }}"},
                    "invoke": {"tool": "nika:write", "args": {"path": "./out/recap.md", "content": "${{ with.s }}"}}}
            }
        });
        let plan = plan_of_document(&doc);
        assert_eq!(
            plan.steps.iter().map(|s| s.op.word()).collect::<Vec<_>>(),
            vec![Op::Draft.word()],
            "{plan:#?}"
        );
        let send = plan
            .effects
            .iter()
            .find(|e| e.verb == EffectVerb::Notify)
            .expect("the notify");
        assert_eq!(send.policy, EffectPolicy::HumanFirst, "gated by the review");
        assert_eq!(send.target, "http://127.0.0.1:8793/hook");
        let write = plan
            .effects
            .iter()
            .find(|e| e.verb == EffectVerb::Write)
            .expect("the write");
        assert_eq!(
            write.policy,
            EffectPolicy::Automatic,
            "not bound to the review"
        );
        assert!(
            plan.obligations
                .iter()
                .any(|o| o.kind == ObligationKind::Dedup),
            "{plan:#?}"
        );
        // The document's tasks are read in key order (the projection keeps keys sorted).
        let mut literals: Vec<&str> = plan.bindings.iter().map(|b| b.literal.as_str()).collect();
        literals.sort_unstable();
        assert_eq!(
            literals,
            [
                "./out/recap.md",
                "./tickets.json",
                "http://127.0.0.1:8793/hook"
            ]
        );
    }

    #[test]
    fn a_delta_names_the_tasks_and_effects_a_revision_touched() {
        let base = json!({"tasks": {
            "read": {"invoke": {"tool": "nika:read", "args": {"path": "./tickets.json"}}},
            "send": {"invoke": {"tool": "nika:notify", "args": {"channel": "webhook", "target": "http://127.0.0.1:8793/hook", "message": "x"}}}
        }});
        let revised = json!({"tasks": {
            "read": {"invoke": {"tool": "nika:read", "args": {"path": "./tickets.json"}}},
            "archive": {"invoke": {"tool": "nika:write", "args": {"path": "./out/recap.md", "content": "x"}}}
        }});
        let d = super::delta(&base, &revised);
        assert_eq!(d["tasks_added"], json!(["archive"]), "{d:#}");
        assert_eq!(d["tasks_removed"], json!(["send"]), "{d:#}");
        assert_eq!(d["tasks_changed"], json!([]), "{d:#}");
        assert_eq!(d["unchanged_tasks"], 1);
        assert_eq!(
            d["effects_added"],
            json!(["write ./out/recap.md (automatic)"]),
            "{d:#}"
        );
        assert_eq!(
            d["effects_removed"],
            json!(["notify http://127.0.0.1:8793/hook (automatic)"]),
            "{d:#}"
        );
    }

    #[test]
    fn a_schema_extracts_an_enum_classifies_a_bound_loop_is_an_obligation() {
        let doc = json!({
            "tasks": {
                "fields": {"infer": {"prompt": "Read the invoice", "schema": {"type": "object", "properties": {"total": {"type": "number"}}}}},
                "route": {"infer": {"prompt": "Which topic?", "schema": {"type": "object", "properties": {"topic": {"type": "string", "enum": ["billing", "auth"]}}}}},
                "fetch": {"retry": {"max_attempts": 3}, "invoke": {"tool": "nika:fetch", "args": {"url": "https://api.example.com/items", "mode": "raw"}}},
                "post": {"invoke": {"tool": "nika:fetch", "args": {"url": "https://api.example.com/items", "method": "POST", "body": "{}"}}},
                "loop": {"agent": {"prompt": "Improve until it passes", "max_turns": 4}},
                "lookup": {"invoke": {"tool": "mcp:crm/find_contact", "args": {"email": "x@y.z"}}}
            }
        });
        let plan = plan_of_document(&doc);
        let mut ops: Vec<&str> = plan.steps.iter().map(|s| s.op.word()).collect();
        ops.sort_unstable();
        let mut wanted = vec![
            Op::Extract.word(),
            Op::Classify.word(),
            Op::Fetch.word(),
            Op::Draft.word(),
            Op::Lookup.word(),
        ];
        wanted.sort_unstable();
        assert_eq!(ops, wanted, "{plan:#?}");
        assert_eq!(plan.effects.len(), 1, "the POST is the effect: {plan:#?}");
        assert_eq!(plan.effects[0].verb, EffectVerb::Send);
        let bounds: Vec<u32> = plan
            .obligations
            .iter()
            .filter_map(|o| match o.kind {
                ObligationKind::RetryBound(n) => Some(n),
                _ => None,
            })
            .collect();
        assert_eq!(bounds, [3, 4]);
        assert!(plan_of_document(&json!({"nika": "x"})).steps.is_empty());
    }
}
