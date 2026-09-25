// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Asides: a question about what waits, answered from the machine's own
//! state, while what waits keeps waiting. « why? » beside an authoring
//! question says what the value is for and which hole it fills; beside a
//! run's gate it says what the answer would let happen. EXPLAIN is never
//! EDIT: an aside consumes no question, decides no gate, applies nothing.

use std::fmt::Write as _;
use std::path::Path;

use nika_onboard::compile::CompileQuestion;

use crate::authoring::AuthoringRound;
use crate::change::PendingGate;

/// The aside beside an authoring question: what the value is for, which
/// hole it fills, what is already settled, how to go on.
#[must_use]
pub(super) fn explain_question(question: &CompileQuestion, round: &AuthoringRound) -> String {
    // A rule Nika writes as code is explained in words: what the human's
    // words do, never the code the compiler would have asked for.
    let mut text = if super::authoring::asks_for_syntax(question) {
        let clause = super::authoring::clause_of(&question.label).unwrap_or_default();
        format!(
            "Your words take the place of « {clause} » in your request; Nika then writes the rule itself, as code, from them — it never asks you for code."
        )
    } else {
        format!(
            "This answer fills `{}` in the workflow Nika is preparing.",
            question.key
        )
    };
    if !question.why.is_empty() && !super::authoring::asks_for_syntax(question) {
        let _ = write!(text, "\n  {}", question.why);
    }
    let _ = write!(text, "\n  what you asked: « {} »", one_line(&round.intent));
    if !round.answers.is_empty() {
        text.push_str("\n  already answered:");
        for (key, value) in &round.answers {
            let _ = write!(text, "\n    · `{key}` = {value}");
        }
    }
    if round.questions.len() > 1 {
        let _ = write!(
            text,
            "\n  {} more question(s) follow this one",
            round.questions.len() - 1
        );
    }
    text.push_str(
        "\n  the question still waits · reply on the next line · `cancel` drops the round",
    );
    text
}

/// The aside beside a declared input a run waits on.
#[must_use]
pub(super) fn explain_input(workflow: &Path, name: &str, remaining: usize) -> String {
    let mut text = format!(
        "`{name}` is an input `{}` declares and needs before it runs: the value you give binds it for this run only (`--var {name}=…` on the command line does the same).",
        workflow.display()
    );
    if remaining > 1 {
        let _ = write!(text, "\n  {} more input(s) follow this one", remaining - 1);
    }
    text.push_str("\n  the input still waits · reply on the next line · `cancel` drops the run");
    text
}

/// The aside beside a run's gate: the pause, what the answer lets happen
/// (the tasks that wait on this gate, from the workflow's own bytes),
/// what has not happened yet.
#[must_use]
pub(super) fn explain_gate(gate: &PendingGate, root: &Path) -> String {
    let mut text = format!(
        "The run of `{}` paused at `{}`, a human gate, and asks: {}",
        gate.workflow.display(),
        gate.task,
        gate.message
    );
    let gated = gated_tasks(&root.join(&gate.workflow), &gate.task);
    if gated.is_empty() {
        text.push_str("\n  what your answer lets happen: the tasks after this gate (the workflow's bytes name them)");
    } else {
        text.push_str("\n  what your answer lets happen:");
        for line in gated {
            let _ = write!(text, "\n    · {line}");
        }
    }
    let how = match gate.mode.as_str() {
        "confirm" => "yes or no",
        "choice" => "one of the choices, as written",
        _ => "in words",
    };
    let _ = write!(
        text,
        "\n  nothing after the gate has happened yet · the trace `{}` holds what ran before it\n  the gate still waits · answer {how} · nothing answers for you",
        gate.trace.display()
    );
    text
}

/// The tasks the gate holds back, at any depth — their `after:` names it
/// (the control edge) or a `with:` binding reads its output (the data edge
/// the compiler writes: `with: { approved: tasks.<gate>.output }` + a
/// `when:`), and every task that follows one of those in turn: two effects
/// behind one gate are both shown, whatever their depth. One line each,
/// in the workflow's order: the id and what it does, from the parser,
/// never prose.
pub(super) fn gated_tasks(workflow: &Path, gate: &str) -> Vec<String> {
    let Ok(source) = std::fs::read_to_string(workflow) else {
        return Vec::new();
    };
    let Some(wf) = crate::review::parse(&source) else {
        return Vec::new();
    };
    let mut held: Vec<String> = vec![gate.to_owned()];
    let mut frontier: Vec<String> = vec![gate.to_owned()];
    while let Some(upstream) = frontier.pop() {
        for t in &wf.tasks {
            let id = &t.value.id.value;
            if held.contains(id) {
                continue;
            }
            let follows = t.value.after.iter().any(|(a, _)| a.value == upstream)
                || t.value
                    .with
                    .iter()
                    .any(|(_, v)| names_task(&v.value.to_string(), &upstream));
            if follows {
                held.push(id.clone());
                frontier.push(id.clone());
            }
        }
    }
    wf.tasks
        .iter()
        .filter(|t| t.value.id.value != gate && held.contains(&t.value.id.value))
        .map(|t| {
            format!(
                "{} · {}",
                t.value.id.value,
                crate::review::task_face(&t.value, wf.model.as_ref().map(|m| m.value.as_str()))
            )
        })
        .collect()
}

/// Does a binding's text read `tasks.<id>` — the data edge — and not a
/// task whose id merely starts with it?
fn names_task(text: &str, id: &str) -> bool {
    let needle = format!("tasks.{id}");
    text.match_indices(&needle).any(|(at, _)| {
        text[at + needle.len()..]
            .chars()
            .next()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
    })
}

/// A request on one line.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Two effects behind one gate are both listed, whatever their depth:
    /// the task after the gate and the task that reads that task's output;
    /// a task the gate does not hold back is not listed (A10 · P-153).
    #[test]
    fn a_gate_lists_every_task_it_holds_back_at_any_depth() {
        let dir = tempfile::tempdir().expect("tmp");
        let workflow = dir.path().join("gated.nika");
        std::fs::write(
            &workflow,
            "nika: gated\npermits: { fs: { write: [\"./out/a.md\", \"./out/b.md\"] }, tools: [\"nika:prompt\", \"nika:write\"] }\ntasks:\n  approve:\n    invoke: { tool: \"nika:prompt\", args: { mode: confirm, message: \"Write both?\" } }\n  first:\n    after: { approve: success }\n    invoke: { tool: \"nika:write\", args: { path: \"./out/a.md\", content: \"a\" } }\n  second:\n    with: { done: \"${{ tasks.first.output }}\" }\n    invoke: { tool: \"nika:write\", args: { path: \"./out/b.md\", content: \"${{ with.done }}\" } }\n  aside:\n    invoke: { tool: \"nika:write\", args: { path: \"./out/a.md\", content: \"free\" } }\n",
        )
        .expect("workflow");
        let ids: Vec<String> = gated_tasks(&workflow, "approve")
            .iter()
            .map(|line| line.split(" · ").next().unwrap_or("").to_owned())
            .collect();
        assert_eq!(ids, ["first", "second"]);
    }
}
