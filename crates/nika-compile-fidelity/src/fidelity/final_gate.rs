// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The approval guard of Law 3 and Law 3b, and Law 3b itself.
//!
//! The guard: an effect a stated approval holds back runs only on a human's typed yes, judged
//! with the analyzer's refusal substitution (the reading of Check's affirmative-consent lane,
//! NEP-0020). Law 3b: a final approval the request states (« demande-moi explicitement avant de
//! l'envoyer ») that the reader bound to no effect (« prépare son envoi à … ») leaves
//! [`GATE_WITHOUT_EFFECT`] and no human-first effect in the plan; the candidate's final
//! effects are held to the guard (measured 2026-09-23: every seat that gated the send was
//! refused « INVENTED GATE », and an ungated send met no law).

use super::{Diagnostic, depends_on, effect_and_gate_tasks, tool_of};
use crate::lexicon::GATE_WITHOUT_EFFECT;
use crate::plan::{EffectPolicy, Plan};
use nika_check_analyzer::gates::{affirmed_by, human_confirm};
use nika_schema::{FileId, ParseMode};
use serde_json::{Map, Value, json};

/// Whether the reader saw a final human gate it could bind to no effect: the request states
/// an approval, and no human-first effect of the plan carries it.
#[must_use]
pub fn unbound_final_gate(plan: &Plan) -> bool {
    plan.unknowns.iter().any(|u| u == GATE_WITHOUT_EFFECT)
        && !plan
            .effects
            .iter()
            .any(|e| e.policy == EffectPolicy::HumanFirst)
}

/// Law 3b over a candidate whose plan carries an unbound final gate.
pub(super) fn unbound_approval(doc: &Value, out: &mut Vec<Diagnostic>) {
    let (effects, gates) = effect_and_gate_tasks(doc);
    let finals = final_effects(doc, &effects);
    if finals.is_empty() && unreadable(doc).is_empty() {
        out.push(Diagnostic { kind: "gate", message: "UNGATED FINAL ACTION: the request asks a human before its final action, yet the candidate carries no effect (a `nika:fetch` beyond GET, a `nika:notify`, a `nika:emit`, a `nika:write`) for the approval to hold back. Realize the action the request states and gate it on a `nika:prompt`, or name it in `gaps`.".to_owned() });
        return;
    }
    if gates.is_empty() {
        out.push(Diagnostic { kind: "gate", message: "MISSING APPROVAL: the request asks a human before its final action; no `nika:prompt` task exists. Add a review task and gate the final effect on its output (`with: { approved: ${{ tasks.review.output }} }`, `when: \"${{ with.approved == true }}\"`).".to_owned() });
        return;
    }
    for task in finals.iter().filter(|task| !affirmed(doc, task)) {
        out.push(order(task));
    }
    unproven(doc, out);
}

/// The effect tasks an unbound final gate holds back: every outbound effect when the candidate
/// has one, else the effects no other effect waits for.
pub(super) fn final_effects(doc: &Value, effects: &[String]) -> Vec<String> {
    let outbound: Vec<String> = effects
        .iter()
        .filter(|t| matches!(tool_of(doc, t), "nika:fetch" | "nika:notify" | "nika:emit"))
        .cloned()
        .collect();
    if !outbound.is_empty() {
        return outbound;
    }
    effects
        .iter()
        .filter(|t| {
            !effects
                .iter()
                .any(|other| other != *t && depends_on(doc, other, std::slice::from_ref(*t)))
        })
        .cloned()
        .collect()
}

/// Whether `task` runs only on a human's typed yes: a blocking confirm gate of the candidate
/// ([`human_confirm`] and `blocking`) whose answer the task's own `when:` affirms
/// ([`affirmed_by`]). Both read the parsed AST of a view that keeps only what they judge (each
/// `nika:prompt`'s `invoke:`, `on_error:` and `for_each:`, the task's `with:` and `when:`), so a
/// field the parser refuses elsewhere (a hole a sketch has not filled yet) neither hides an
/// approval nor fakes one.
pub(super) fn affirmed(doc: &Value, task: &str) -> bool {
    let Some(node) = doc.get("tasks").and_then(|t| t.get(task)) else {
        return false;
    };
    let kept = |node: &Value, keys: &[&str]| -> Map<String, Value> {
        let field = |key: &&str| {
            node.get(*key)
                .map(|value| ((*key).to_owned(), value.clone()))
        };
        keys.iter().filter_map(field).collect()
    };
    let mut view: Map<String, Value> = effect_and_gate_tasks(doc)
        .1
        .into_iter()
        .map(|gate| {
            let fields = kept(&doc["tasks"][&gate], &["invoke", "on_error", "for_each"]);
            (gate, Value::Object(fields))
        })
        .collect();
    // The verb the parser requires; the guard never reads it.
    let mut effect = kept(node, &["with", "when"]);
    effect.insert("exec".to_owned(), json!({ "command": ["true"] }));
    view.insert(task.to_owned(), Value::Object(effect));
    let source = json!({ "nika": "approval-view", "tasks": view }).to_string();
    let Ok(wf) = nika_schema::parse(&source, FileId::new(0), ParseMode::Strict) else {
        return false;
    };
    let parsed: Vec<_> = wf.tasks.iter().map(|t| &t.value).collect();
    let raw = parsed.iter().find(|t| t.id.value == task);
    raw.is_some_and(|t| {
        parsed
            .iter()
            .any(|g| human_confirm(g) && blocking(doc, &g.id.value) && affirmed_by(t, &g.id.value))
    })
}

/// Whether the confirm gate `gate` blocks: its `nika:prompt` declares no `default:`, so an
/// unattended run pauses for the human (the trifecta's human gate, ADR-099's pause rider)
/// instead of answering itself. A `default: true` approves and a `default: false` refuses with
/// nobody there (AUTH-01/02/06, 2026-09-24: « Demande-moi explicitement avant de l'envoyer »
/// ran to exit 0 on `decision=deny source=policy`, never asking); neither is the human's
/// answer a stated approval waits for. Only this guard reads it: the analyzer's
/// [`human_confirm`] keeps its public meaning, and a prompt no stated approval needs keeps its
/// default.
fn blocking(doc: &Value, gate: &str) -> bool {
    doc["tasks"][gate]["invoke"]["args"]
        .get("default")
        .is_none()
}

/// The refusal of an approved effect that does not run only on the human's yes.
pub(super) fn order(task: &str) -> Diagnostic {
    Diagnostic {
        kind: "gate",
        message: format!(
            "APPROVAL ORDER: the effect task `{task}` does not run only on the human's yes. Bind a confirm `nika:prompt`'s output whole in its `with:` (`approved: ${{{{ tasks.review.output }}}}`) and guard it with `when: \"${{{{ with.approved == true }}}}\"`: an `after:` wait, a guard a « no » or a skipped prompt passes (`== false`, `!`, `||`, `!= false`), a derived value, a choice or input prompt, a `default:` on the prompt (`true` or `false`: it answers the gate unattended and never asks — omit it; an unattended « no » is the invocation's `--answer <id>=false`) and an `on_error:` on the prompt approve nothing."
        ),
    }
}

/// The honest limit of both approval laws: a task whose effect they cannot read runs on the yes
/// as well, or is refused by name, never assumed harmless. An `mcp:` tool has no exact effect
/// metadata here (the catalog's per-tool hints are advisory, and the server is the user's
/// alias); a child workflow, a process and an agent with effect tools act beyond the table.
pub(super) fn unproven(doc: &Value, out: &mut Vec<Diagnostic>) {
    for task in unreadable(doc).iter().filter(|task| !affirmed(doc, task)) {
        out.push(Diagnostic { kind: "gate", message: format!("APPROVAL UNPROVEN: the task `{task}` may act, and its effect is not one the compiler can read (an `mcp:` tool, a child workflow, a process, an agent with effect tools); the stated approval holds it only when it runs on the human's yes. Guard it like an effect (`with: {{ approved: ${{{{ tasks.review.output }}}} }}`, `when: \"${{{{ with.approved == true }}}}\"`), or leave it out and name it in `gaps`.") });
    }
}

/// The tasks [`unproven`] judges. An agent's tool reaches an effect through `mcp:`, a glob, or
/// a builtin of the effect table (`effect_and_gate_tasks`; an agent's fetch may POST).
fn unreadable(doc: &Value) -> Vec<String> {
    let acting = |tool: &str| {
        tool.starts_with("mcp:")
            || tool.contains('*')
            || matches!(
                tool,
                "nika:write" | "nika:notify" | "nika:emit" | "nika:edit" | "nika:fetch"
            )
    };
    doc.get("tasks")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(id, task)| {
            tool_of(doc, id).starts_with("mcp:")
                || task.pointer("/invoke/workflow").is_some()
                || task.get("exec").is_some()
                || task
                    .pointer("/agent/tools")
                    .and_then(Value::as_array)
                    .is_some_and(|tools| tools.iter().filter_map(Value::as_str).any(acting))
        })
        .map(|(id, _)| id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::{approvals, invented_gates};
    use super::*;
    use crate::plan::{Effect, EffectVerb};

    /// The plan the reader leaves for a final gate with no effect it knows.
    fn unbound() -> Plan {
        let mut plan = Plan::default();
        plan.unknowns.push(GATE_WITHOUT_EFFECT.to_owned());
        plan
    }

    /// The plan of a request that binds its approval to a write (Law 3).
    fn bound_write() -> Plan {
        let mut plan = Plan::default();
        plan.effects.push(Effect::new(
            EffectVerb::Write,
            "./final.txt",
            "demande-moi avant d'écrire ./final.txt",
            EffectPolicy::HumanFirst,
        ));
        plan
    }

    fn judge(plan: &Plan, doc: &Value) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        approvals(plan, doc, &mut out);
        invented_gates(plan, doc, &mut out);
        out
    }

    fn heads(out: &[Diagnostic]) -> Vec<&str> {
        out.iter()
            .map(|d| d.message.split(':').next().unwrap_or_default())
            .collect()
    }

    /// read → review → send, the send guarded by the review's answer; the review declares no
    /// `default:`, so unattended it pauses for the human.
    fn gated() -> Value {
        serde_json::json!({"tasks": {
            "read_note": {"invoke": {"tool": "nika:read", "args": {"path": "./note.txt"}}},
            "review": {"with": {"note": "${{ tasks.read_note.output }}"},
                       "invoke": {"tool": "nika:prompt", "args": {"message": "Envoyer ? ${{ with.note }}"}}},
            "send": {"with": {"approved": "${{ tasks.review.output }}", "note": "${{ tasks.read_note.output }}"},
                     "when": "${{ with.approved == true }}",
                     "invoke": {"tool": "nika:fetch", "args": {"url": "https://hooks.example.test/in", "method": "POST", "body": "${{ with.note }}"}}}
        }})
    }

    /// `gated()` whose send reads `when` (or waits with no `when:` at all).
    fn send_when(when: Option<&str>) -> Value {
        let mut doc = gated();
        let send = doc["tasks"]["send"].as_object_mut().unwrap();
        match when {
            Some(when) => send.insert("when".to_owned(), serde_json::json!(when)),
            None => send.remove("when"),
        };
        doc
    }

    #[test]
    fn the_unbound_final_gate_is_read_only_from_the_readers_unknown() {
        assert!(unbound_final_gate(&unbound()));
        assert!(!unbound_final_gate(&Plan::default()));
        // A human-first effect carries the gate already: Law 3 judges it, not Law 3b.
        let mut bound = unbound();
        bound.effects.push(Effect::new(
            EffectVerb::Send,
            "la note",
            "demande-moi avant de l'envoyer",
            EffectPolicy::HumanFirst,
        ));
        assert!(!unbound_final_gate(&bound));
    }

    #[test]
    fn a_stated_final_gate_over_the_final_send_is_accepted_not_invented() {
        // Red at 4a06aa3a: « INVENTED GATE » for `send`.
        let out = judge(&unbound(), &gated());
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn a_final_send_without_any_gate_is_refused() {
        // Red at 4a06aa3a: no law fires, the ungated send reaches READY.
        let mut doc = gated();
        doc["tasks"].as_object_mut().unwrap().remove("review");
        doc["tasks"]["send"] = serde_json::json!({"with": {"note": "${{ tasks.read_note.output }}"},
            "invoke": {"tool": "nika:fetch", "args": {"url": "https://hooks.example.test/in", "method": "POST", "body": "${{ with.note }}"}}});
        assert_eq!(heads(&judge(&unbound(), &doc)), ["MISSING APPROVAL"]);
    }

    /// A guard only counts when a « no » cannot pass it and a « yes » can: the analyzer's
    /// refusal substitution, never a text search for `with.approved`.
    #[test]
    fn every_guard_a_no_can_pass_is_the_wrong_order() {
        // Bypass attempts: an `after:` wait, a guard true on « no », a negation, an `||`, a
        // quoted name, another binding, a plain text with no template, a dead compare.
        let mut after_only = send_when(None);
        after_only["tasks"]["send"]["after"] = serde_json::json!({"review": "success"});
        let mut cases = vec![after_only];
        for when in [
            "${{ with.approved == false }}",
            "${{ !with.approved }}",
            "${{ with.approved || true }}",
            "${{ with.approved != false }}",
            "${{ 'with.approved' != '' }}",
            "${{ with.note != '' }}",
            "with.approved == true",
            "${{ with.approved == 'yes' }}",
        ] {
            cases.push(send_when(Some(when)));
        }
        for doc in cases {
            let out = judge(&unbound(), &doc);
            assert_eq!(heads(&out), ["APPROVAL ORDER"], "{doc}: {out:?}");
            assert!(out[0].message.contains("`send`"), "{out:?}");
        }
    }

    #[test]
    fn a_conjunction_that_needs_the_yes_is_affirmed() {
        let doc = send_when(Some("${{ with.approved == true && with.note != '' }}"));
        let out = judge(&unbound(), &doc);
        assert!(out.is_empty(), "{out:?}");
    }

    /// A choice or input prompt answers a string; a `default: true` answers yes and a
    /// `default: false` answers no with nobody there (AUTH-01/02/06: the run decided « no » by
    /// policy and never asked); a recovered error stands in for the answer: none is a typed
    /// human yes. The same gate without its default (`gated()`) is one.
    #[test]
    fn a_prompt_that_is_not_a_human_confirm_approves_nothing() {
        for args in [
            serde_json::json!({"message": "Envoyer ?", "mode": "choice", "choices": ["oui", "non"]}),
            serde_json::json!({"message": "Envoyer ?", "mode": "input"}),
            serde_json::json!({"message": "Envoyer ?", "default": true}),
            serde_json::json!({"message": "Envoyer ?", "default": false}),
        ] {
            let mut doc = gated();
            doc["tasks"]["review"]["invoke"]["args"] = args.clone();
            assert_eq!(
                heads(&judge(&unbound(), &doc)),
                ["APPROVAL ORDER"],
                "{args}"
            );
        }
        let mut doc = gated();
        doc["tasks"]["review"]["on_error"] = serde_json::json!({"recover": true});
        assert_eq!(heads(&judge(&unbound(), &doc)), ["APPROVAL ORDER"]);
    }

    /// Bypass attempt: a task waits for the review and returns a constant `true`; the send
    /// guards on that. A derived value is not the human's answer, and is not proven to be.
    #[test]
    fn a_decision_derived_from_the_review_is_not_proven_a_yes() {
        let mut doc = gated();
        doc["tasks"]["decide"] = serde_json::json!({"with": {"answer": "${{ tasks.review.output }}"},
            "invoke": {"tool": "nika:jq", "args": {"input": "${{ with.answer }}", "expression": "true"}}});
        doc["tasks"]["send"]["with"]["approved"] = serde_json::json!("${{ tasks.decide.output }}");
        assert_eq!(heads(&judge(&unbound(), &doc)), ["APPROVAL ORDER"]);
    }

    #[test]
    fn a_gate_held_over_an_earlier_effect_is_still_invented() {
        // Bypass attempt: the prompt holds back a draft write while the final send runs free.
        let doc = serde_json::json!({"tasks": {
            "read_note": {"invoke": {"tool": "nika:read", "args": {"path": "./note.txt"}}},
            "review": {"with": {"note": "${{ tasks.read_note.output }}"},
                       "invoke": {"tool": "nika:prompt", "args": {"message": "OK ?", "default": false}}},
            "keep_draft": {"with": {"approved": "${{ tasks.review.output }}", "note": "${{ tasks.read_note.output }}"},
                           "when": "${{ with.approved == true }}",
                           "invoke": {"tool": "nika:write", "args": {"path": "./draft.txt", "content": "${{ with.note }}"}}},
            "send": {"with": {"note": "${{ tasks.read_note.output }}"},
                     "invoke": {"tool": "nika:fetch", "args": {"url": "https://hooks.example.test/in", "method": "POST", "body": "${{ with.note }}"}}}
        }});
        let out = judge(&unbound(), &doc);
        let heads = heads(&out);
        assert!(heads.contains(&"APPROVAL ORDER"), "{out:?}");
        assert!(
            out.iter()
                .any(|d| d.message.starts_with("INVENTED GATE")
                    && d.message.contains("`keep_draft`")),
            "{out:?}"
        );
    }

    #[test]
    fn an_effect_that_follows_the_gated_final_send_is_not_an_invented_gate() {
        let mut doc = gated();
        doc["tasks"]["log_sent"] = serde_json::json!({"with": {"sent": "${{ tasks.send.output }}"},
            "invoke": {"tool": "nika:write", "args": {"path": "./sent.log", "content": "${{ with.sent }}"}}});
        let out = judge(&unbound(), &doc);
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn without_any_effect_the_stated_final_action_is_not_dropped_silently() {
        let doc = serde_json::json!({"tasks": {
            "read_note": {"invoke": {"tool": "nika:read", "args": {"path": "./note.txt"}}},
            "review": {"with": {"note": "${{ tasks.read_note.output }}"},
                       "invoke": {"tool": "nika:prompt", "args": {"message": "OK ?"}}}
        }});
        assert_eq!(heads(&judge(&unbound(), &doc)), ["UNGATED FINAL ACTION"]);
    }

    #[test]
    fn with_only_writes_the_last_write_is_the_final_effect() {
        let doc = serde_json::json!({"tasks": {
            "read_note": {"invoke": {"tool": "nika:read", "args": {"path": "./note.txt"}}},
            "stage": {"with": {"note": "${{ tasks.read_note.output }}"},
                      "invoke": {"tool": "nika:write", "args": {"path": "./stage.txt", "content": "${{ with.note }}"}}},
            "review": {"with": {"staged": "${{ tasks.stage.output }}"},
                       "invoke": {"tool": "nika:prompt", "args": {"message": "Publier ?"}}},
            "publish": {"with": {"approved": "${{ tasks.review.output }}", "staged": "${{ tasks.stage.output }}"},
                        "when": "${{ with.approved == true }}",
                        "invoke": {"tool": "nika:write", "args": {"path": "./final.txt", "content": "ok"}}}
        }});
        assert!(judge(&unbound(), &doc).is_empty());
        assert_eq!(
            final_effects(&doc, &effect_and_gate_tasks(&doc).0),
            ["publish"]
        );
    }

    #[test]
    fn without_a_stated_gate_every_gate_stays_invented() {
        // Control: no unknown, no human-first effect — Law 21 exactly as before.
        let out = judge(&Plan::default(), &gated());
        assert_eq!(heads(&out), ["INVENTED GATE"], "{out:?}");
    }

    /// Law 3, bound: the approved write runs only on the yes. Red at 4a06aa3a for every
    /// refusal below: any dependency on the prompt (an `after:` wait, a derived value, a
    /// choice prompt) passed.
    #[test]
    fn a_bound_approval_holds_every_approved_write_to_the_yes() {
        let doc = |write: Value, review_args: Value| {
            serde_json::json!({"tasks": {
                "read_draft": {"invoke": {"tool": "nika:read", "args": {"path": "./draft.txt"}}},
                "review": {"with": {"draft": "${{ tasks.read_draft.output }}"},
                           "invoke": {"tool": "nika:prompt", "args": review_args}},
                "decide": {"with": {"answer": "${{ tasks.review.output }}"},
                           "invoke": {"tool": "nika:jq", "args": {"input": "${{ with.answer }}", "expression": "true"}}},
                "write_final": write
            }})
        };
        let write = |guard: Value| {
            let mut node = serde_json::json!({"with": {"draft": "${{ tasks.read_draft.output }}"},
                "invoke": {"tool": "nika:write", "args": {"path": "./final.txt", "content": "${{ with.draft }}"}}});
            node.as_object_mut()
                .unwrap()
                .extend(guard.as_object().unwrap().clone());
            node
        };
        let confirm = serde_json::json!({"message": "Écrire ?"});
        let house = serde_json::json!({"with": {"approved": "${{ tasks.review.output }}", "draft": "${{ tasks.read_draft.output }}"},
            "when": "${{ with.approved == true }}"});
        assert!(judge(&bound_write(), &doc(write(house.clone()), confirm.clone())).is_empty());
        for (why, write, args) in [
            (
                "after-only",
                write(serde_json::json!({"after": {"review": "success"}})),
                confirm.clone(),
            ),
            (
                "derived",
                write(
                    serde_json::json!({"with": {"approved": "${{ tasks.decide.output }}", "draft": "${{ tasks.read_draft.output }}"},
                "when": "${{ with.approved == true }}"}),
                ),
                confirm.clone(),
            ),
            (
                "choice",
                write(house.clone()),
                serde_json::json!({"message": "Écrire ?", "mode": "choice", "choices": ["oui", "non"]}),
            ),
            (
                "default false",
                write(house.clone()),
                serde_json::json!({"message": "Écrire ?", "default": false}),
            ),
            (
                "default true",
                write(house.clone()),
                serde_json::json!({"message": "Écrire ?", "default": true}),
            ),
        ] {
            let out = judge(&bound_write(), &doc(write, args));
            assert_eq!(heads(&out), ["APPROVAL ORDER"], "{why}: {out:?}");
        }
    }

    /// Only the gate that holds a stated approval must block: another prompt of the candidate
    /// (an input asked with a default) keeps its default, and without a stated approval no
    /// law reads a prompt's default at all.
    #[test]
    fn a_default_disarms_only_the_gate_of_a_stated_approval() {
        let mut doc = gated();
        doc["tasks"]["subject"] = serde_json::json!({"invoke": {"tool": "nika:prompt",
            "args": {"message": "Objet ?", "mode": "input", "default": "Note"}}});
        assert!(judge(&unbound(), &doc).is_empty());
        let asked = serde_json::json!({"tasks": {
            "name": {"invoke": {"tool": "nika:prompt", "args": {"message": "Nom ?", "mode": "input", "default": "anonyme"}}},
            "greet": {"with": {"name": "${{ tasks.name.output }}"}, "infer": {"prompt": "Salue ${{ with.name }}"}}
        }});
        assert!(judge(&Plan::default(), &asked).is_empty());
    }

    /// An `mcp:` tool, a process, a child workflow or an agent with effect tools may be the
    /// approved action, and the compiler cannot read which: each is held to the yes, or refused
    /// by name — never counted as covered. An agent limited to `nika:done` is not held.
    #[test]
    fn an_effect_the_compiler_cannot_read_is_held_to_the_yes() {
        let with_task = |id: &str, node: Value| {
            let mut doc = gated();
            doc["tasks"].as_object_mut().unwrap().remove("send");
            doc["tasks"][id] = node;
            doc
        };
        let guard = serde_json::json!({"approved": "${{ tasks.review.output }}"});
        for (id, node) in [
            (
                "mail",
                serde_json::json!({"invoke": {"tool": "mcp:mail/send", "args": {"to": "x"}}}),
            ),
            (
                "post",
                serde_json::json!({"exec": {"command": ["curl", "-X", "POST", "https://hooks.example.test/in"]}}),
            ),
            (
                "child",
                serde_json::json!({"invoke": {"workflow": "./send.nika"}}),
            ),
            (
                "helper",
                serde_json::json!({"agent": {"prompt": "Envoie la note.", "tools": ["nika:write"]}}),
            ),
        ] {
            let doc = with_task(id, node.clone());
            let out = judge(&unbound(), &doc);
            assert_eq!(heads(&out), ["APPROVAL UNPROVEN"], "{id}: {out:?}");
            assert!(out[0].message.contains(&format!("`{id}`")), "{out:?}");
            let mut guarded = node;
            guarded["with"] = guard.clone();
            guarded["when"] = serde_json::json!("${{ with.approved == true }}");
            let out = judge(&unbound(), &with_task(id, guarded));
            assert!(out.is_empty(), "{id} guarded: {out:?}");
        }
        let explorer = serde_json::json!({"agent": {"prompt": "Explore.", "tools": ["nika:done"]}});
        let mut doc = gated();
        doc["tasks"]["explore"] = explorer;
        assert!(judge(&unbound(), &doc).is_empty());
        // Law 3, bound, holds the unreadable task the same way.
        let mut doc = gated();
        doc["tasks"]["mail"] =
            serde_json::json!({"invoke": {"tool": "mcp:mail/send", "args": {"to": "x"}}});
        let mut plan = Plan::default();
        plan.effects.push(Effect::new(
            EffectVerb::Send,
            "la note",
            "demande-moi avant d'envoyer",
            EffectPolicy::HumanFirst,
        ));
        assert_eq!(heads(&judge(&plan, &doc)), ["APPROVAL UNPROVEN"]);
    }

    /// The guard reads a view of the gates and the task: a field the strict parser would refuse
    /// elsewhere (an unfilled hole, an unknown key) cannot hide the approval.
    #[test]
    fn a_field_the_parser_refuses_elsewhere_does_not_hide_the_approval() {
        let mut doc = gated();
        doc["tasks"]["odd"] = serde_json::json!({"bogus": true, "infer": {"prompt": ""}});
        assert!(judge(&unbound(), &doc).is_empty());
    }
}
