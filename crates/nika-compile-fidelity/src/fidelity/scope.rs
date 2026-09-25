// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The scope of an effect the request states, bans or leaves unsettled. A ban beside a request
//! of the same family is targeted: it concerns only the tasks that carry one of its literals
//! (a URL, an address, a path), and its other words ride the plan to the seat and the review.
//! An effect the words leave unsettled (undecided, or both asked and banned) is never owed: the
//! seat reads the request whole and realizes what it means, and the realization is stated to
//! the review. A reading is not authority: the candidate's permits, Check, the approval floor
//! and the human's consent to the exact bytes decide what may run.

use super::{effect_and_gate_tasks, reads, strings, tool_of};
use crate::plan::{Effect, EffectPolicy, EffectVerb, Plan};
use nika_compile_reader::paths::{self, PathShape};
use serde_json::Value;

/// The kind of task an effect verb reaches: a write (`false`), an outbound effect (`true`).
fn outbound(verb: EffectVerb) -> Option<bool> {
    match verb {
        EffectVerb::Write => Some(false),
        EffectVerb::Send | EffectVerb::Notify | EffectVerb::Publish | EffectVerb::Other => {
            Some(true)
        }
        _ => None,
    }
}

/// Whether two effect verbs reach the same kind of task.
fn same_family(a: EffectVerb, b: EffectVerb) -> bool {
    outbound(a).is_some() && outbound(a) == outbound(b)
}

/// Whether a task of the candidate performs an effect of the verb's family.
fn in_family(doc: &Value, task: &str, verb: EffectVerb) -> bool {
    match outbound(verb) {
        Some(false) => tool_of(doc, task) == "nika:write",
        Some(true) => matches!(
            tool_of(doc, task),
            "nika:fetch" | "nika:notify" | "nika:emit"
        ),
        None => false,
    }
}

/// The literal tokens of an effect's words: URLs, addresses and paths, a leading `./` dropped.
fn literals(text: &str) -> Vec<String> {
    let mut found: Vec<String> = paths::literals(text)
        .into_iter()
        .filter_map(|shape| match shape {
            PathShape::File(path) | PathShape::Directory(path) | PathShape::Glob(path) => {
                Some(path.strip_prefix("./").unwrap_or(&path).to_owned())
            }
            _ => None,
        })
        .collect();
    found.extend(text.split_whitespace().filter_map(|token| {
        let token = token
            .trim_matches(|c: char| "«»\"'()[]`".contains(c))
            .trim_end_matches([',', ';', '.', ':', '!', '?']);
        (token.contains("://") || (token.contains('@') && token.contains('.')))
            .then(|| token.to_owned())
    }));
    found
}

/// Whether a task's arguments carry one of the literals.
fn carries(doc: &Value, task: &str, literals: &[String]) -> bool {
    let mut texts = Vec::new();
    strings(
        doc.pointer(&format!("/tasks/{task}/invoke/args"))
            .unwrap_or(&Value::Null),
        &mut texts,
    );
    // Resolve only constants actually referenced by these arguments. Unused constants
    // elsewhere in the document do not make this task perform their destination.
    let mut bound = Vec::new();
    for text in &texts {
        for (name, _) in reads(text, "const.") {
            strings(
                doc.get("const")
                    .and_then(|values| values.get(name))
                    .unwrap_or(&Value::Null),
                &mut bound,
            );
        }
    }
    texts.extend(bound);
    texts.iter().any(|text| {
        let text = text.trim_start_matches("./");
        literals
            .iter()
            .any(|literal| text.contains(literal.as_str()))
    })
}

fn requested(policy: EffectPolicy) -> bool {
    matches!(policy, EffectPolicy::Automatic | EffectPolicy::HumanFirst)
}

/// Whether `task` performs the effect `effect` names: an effect of its family, and — when the
/// plan also requests an effect of that family — one that carries a literal of `effect`'s own
/// words; a literal-free ban remains a semantic constraint for the seat and the review.
/// An unsettled effect without a literal concerns tasks outside the requested literal.
pub(super) fn concerns(plan: &Plan, effect: &Effect, doc: &Value, task: &str) -> bool {
    if !in_family(doc, task, effect.verb) {
        return false;
    }
    let beside: Vec<&Effect> = plan
        .effects
        .iter()
        .filter(|e| requested(e.policy) && same_family(e.verb, effect.verb))
        .collect();
    if beside.is_empty() {
        return true;
    }
    let own = literals(&effect.target);
    if !own.is_empty() {
        return carries(doc, task, &own);
    }
    effect.policy != EffectPolicy::Forbidden
        && !beside.iter().any(|e| {
            carries(
                doc,
                task,
                &literals(&format!("{} {}", e.target, e.evidence)),
            )
        })
}

fn unsettled(effect: &Effect) -> bool {
    matches!(
        effect.policy,
        EffectPolicy::Undecided | EffectPolicy::Conflict
    )
}

/// Whether an unsettled effect of the plan concerns `task` (Law 21 licenses its gate).
pub(super) fn unsettled_concerns(plan: &Plan, doc: &Value, task: &str) -> bool {
    plan.effects
        .iter()
        .filter(|e| unsettled(e))
        .any(|e| concerns(plan, e, doc, task))
}

/// The unsettled effects of the plan — undecided, or both asked and banned by the request's
/// own words — that the candidate performs, each as `verb · « words »`: the seat's reading of
/// the request, stated to the review; never a refusal and never a grant.
#[must_use]
pub fn unsettled_performed(plan: &Plan, doc: &Value) -> Vec<String> {
    let (effects, _) = effect_and_gate_tasks(doc);
    plan.effects
        .iter()
        .filter(|e| unsettled(e))
        .filter(|e| effects.iter().any(|task| concerns(plan, e, doc, task)))
        .map(|e| format!("`{}` · « {} »", e.verb.word(), e.evidence.trim()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn effect(verb: EffectVerb, words: &str, policy: EffectPolicy) -> Effect {
        Effect::new(verb, words, words, policy)
    }

    fn post(url: &str) -> Value {
        json!({"invoke": {"tool": "nika:fetch", "args": {"url": url, "method": "POST", "body": "${{ tasks.draft.output }}"}}})
    }

    #[test]
    fn a_ban_beside_a_request_concerns_only_its_own_literal() {
        let mut plan = Plan::default();
        plan.effects.push(effect(
            EffectVerb::Send,
            "Post the digest to https://hooks.example.test/x",
            EffectPolicy::Automatic,
        ));
        plan.effects.push(effect(
            EffectVerb::Send,
            "never post the raw CSV",
            EffectPolicy::Forbidden,
        ));
        plan.effects.push(effect(
            EffectVerb::Send,
            "never post to https://evil.example.test/y",
            EffectPolicy::Forbidden,
        ));
        let doc = json!({"tasks": {"digest": post("https://hooks.example.test/x"), "leak": post("https://evil.example.test/y")}});
        assert!(
            !concerns(&plan, &plan.effects[1], &doc, "digest"),
            "a ban without a literal never takes the request"
        );
        assert!(!concerns(&plan, &plan.effects[2], &doc, "digest"));
        assert!(
            concerns(&plan, &plan.effects[2], &doc, "leak"),
            "the ban's own literal is performed"
        );
        // Without a request beside it, a ban concerns its whole family.
        let mut alone = Plan::default();
        alone.effects.push(effect(
            EffectVerb::Send,
            "never post the raw CSV",
            EffectPolicy::Forbidden,
        ));
        assert!(concerns(&alone, &alone.effects[0], &doc, "digest"));
    }

    #[test]
    fn an_unsettled_effect_is_stated_when_performed_never_refused() {
        let mut plan = Plan::default();
        plan.effects.push(effect(
            EffectVerb::Send,
            "maybe send the report to ops@example.test",
            EffectPolicy::Undecided,
        ));
        let performed = json!({"tasks": {"mail": {"invoke": {"tool": "nika:notify", "args": {"channel": "email", "target": "ops@example.test", "message": "${{ tasks.draft.output }}"}}}}});
        assert_eq!(
            unsettled_performed(&plan, &performed),
            vec!["`send` · « maybe send the report to ops@example.test »".to_owned()]
        );
        assert!(
            unsettled_concerns(&plan, &performed, "mail"),
            "a gate the seat puts on it is no invented gate"
        );
        assert!(
            unsettled_performed(&plan, &json!({"tasks": {}})).is_empty(),
            "never owed"
        );
    }
    #[test]
    fn relative_paths_and_referenced_constants_keep_the_bans_scope() {
        let mut plan = Plan::default();
        plan.effects.push(effect(
            EffectVerb::Write,
            "write ./digest.csv",
            EffectPolicy::Automatic,
        ));
        for path in ["./raw.csv", "../raw.csv", "raw.csv"] {
            let ban = effect(
                EffectVerb::Write,
                &format!("never write {path}"),
                EffectPolicy::Forbidden,
            );
            let doc = json!({"const": {"destination": path, "unused": "./raw.csv"}, "tasks": {
                "bad": {"invoke": {"tool": "nika:write", "args": {"path": "${{ const.destination }}"}}},
                "good": {"invoke": {"tool": "nika:write", "args": {"path": "./digest.csv"}}}
            }});
            assert!(concerns(&plan, &ban, &doc, "bad"), "{path}");
            assert!(
                !concerns(&plan, &ban, &doc, "good"),
                "unused constants are not effects"
            );
        }
        let ban = effect(
            EffectVerb::Send,
            "never post to https://blocked.example.test/hook",
            EffectPolicy::Forbidden,
        );
        plan.effects.push(effect(
            EffectVerb::Send,
            "post to https://allowed.example.test/hook",
            EffectPolicy::Automatic,
        ));
        let doc = json!({"const": {"endpoint": "https://blocked.example.test/hook"}, "tasks": {
            "bad": post("${{ const.endpoint }}"), "good": post("https://allowed.example.test/hook")
        }});
        assert!(concerns(&plan, &ban, &doc, "bad"));
        assert!(!concerns(&plan, &ban, &doc, "good"));
    }
}
