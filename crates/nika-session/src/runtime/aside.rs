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
    let mut text = format!(
        "This answer fills `{}` in the workflow Nika is preparing.",
        question.key
    );
    if !question.why.is_empty() {
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

/// The tasks that run after `gate` (their `after:` names it), one line
/// each: the id and what it does, from the parser, never from prose.
fn gated_tasks(workflow: &Path, gate: &str) -> Vec<String> {
    let Ok(source) = std::fs::read_to_string(workflow) else {
        return Vec::new();
    };
    let Some(wf) = crate::review::parse(&source) else {
        return Vec::new();
    };
    wf.tasks
        .iter()
        .filter(|t| t.value.after.iter().any(|(id, _)| id.value == gate))
        .map(|t| {
            format!(
                "{} · {}",
                t.value.id.value,
                crate::review::task_face(&t.value, wf.model.as_ref().map(|m| m.value.as_str()))
            )
        })
        .collect()
}

/// A request on one line.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
