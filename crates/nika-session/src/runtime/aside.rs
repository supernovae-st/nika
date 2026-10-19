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

use super::Lang;
use crate::authoring::AuthoringRound;
use crate::change::PendingGate;

/// The aside beside an authoring question: what the value is for, which
/// hole it fills, what is already settled, how to go on.
#[must_use]
pub(super) fn explain_question(
    question: &CompileQuestion,
    round: &AuthoringRound,
    lang: Lang,
) -> String {
    let fr = lang == Lang::Fr;
    // A rule Nika writes as code is explained in words: what the human's
    // words do, never the code the compiler would have asked for.
    let mut text = if super::authoring::asks_for_syntax(question) {
        let clause = super::authoring::clause_of(&question.label).unwrap_or_default();
        if fr {
            format!(
                "Vos mots prennent la place de « {clause} » dans votre demande ; la règle est ensuite écrite en code à partir d'eux : on ne vous demande jamais de code."
            )
        } else {
            format!(
                "Your words take the place of « {clause} » in your request; Nika then writes the rule itself, as code, from them — it never asks you for code."
            )
        }
    } else if fr {
        format!(
            "Cette réponse remplit `{}` dans le workflow en préparation.",
            question.key
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
    let intent = one_line(&round.intent);
    let _ = if fr {
        write!(text, "\n  ce que vous avez demandé : « {intent} »")
    } else {
        write!(text, "\n  what you asked: « {intent} »")
    };
    if !round.answers.is_empty() {
        text.push_str(lang.pick("\n  already answered:", "\n  déjà répondu :"));
        for (key, value) in &round.answers {
            let _ = write!(text, "\n    · `{key}` = {value}");
        }
    }
    if round.questions.len() > 1 {
        let more = round.questions.len() - 1;
        let follow = if fr {
            format!("\n  encore {more} question(s) après celle-ci")
        } else {
            format!("\n  {more} more question(s) follow this one")
        };
        text.push_str(&follow);
    }
    text.push_str(lang.pick(
        "\n  the question still waits · reply on the next line · `cancel` drops the round",
        "\n  la question attend toujours · répondez sur la ligne suivante · « annuler » abandonne la préparation",
    ));
    text
}

/// The aside beside a declared input a run waits on.
#[must_use]
pub(super) fn explain_input(workflow: &Path, name: &str, remaining: usize, lang: Lang) -> String {
    let workflow = workflow.display();
    let mut text = if lang == Lang::Fr {
        format!(
            "`{name}` est une entrée que `{workflow}` déclare et demande avant de tourner : la valeur que vous donnez la remplit pour ce run seulement (`--var {name}=…` en ligne de commande fait de même)."
        )
    } else {
        format!(
            "`{name}` is an input `{workflow}` declares and needs before it runs: the value you give binds it for this run only (`--var {name}=…` on the command line does the same)."
        )
    };
    if remaining > 1 {
        let more = remaining - 1;
        let follow = if lang == Lang::Fr {
            format!("\n  encore {more} entrée(s) après celle-ci")
        } else {
            format!("\n  {more} more input(s) follow this one")
        };
        text.push_str(&follow);
    }
    text.push_str(lang.pick(
        "\n  the input still waits · reply on the next line · `cancel` drops the run",
        "\n  l'entrée attend toujours · répondez sur la ligne suivante · « annuler » abandonne le run",
    ));
    text
}

/// The aside beside a run's gate: the pause, what the answer lets happen
/// (the tasks that wait on this gate, from the workflow's own bytes),
/// what has not happened yet.
#[must_use]
pub(super) fn explain_gate(gate: &PendingGate, root: &Path, lang: Lang) -> String {
    let (workflow, task) = (gate.workflow.display(), &gate.task);
    // The card shows the gate's whole message: its first line names the question, and the rest
    // (a digest, a draft) is not said twice (live gauntlet r04 B6: the whole digest again).
    let asked = (gate.message.lines())
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    let mut text = match lang {
        Lang::Fr => format!(
            "Le run de `{workflow}` s'est mis en pause à `{task}` : le workflow demande là une validation humaine (« {asked} »), et rien de ce qu'elle retient ne se fait sans votre réponse."
        ),
        _ => format!(
            "The run of `{workflow}` paused at `{task}`: the workflow asks a person there (« {asked} »), and nothing it holds back happens without your answer."
        ),
    };
    let lets = lang.pick(
        "what your answer lets happen:",
        "ce que votre réponse permet :",
    );
    let gated = gated_tasks(&root.join(&gate.workflow), &gate.task);
    if gated.is_empty() {
        let tasks = lang.pick(
            "the tasks after this gate (the workflow's bytes name them)",
            "les tâches après cette validation (le workflow les nomme)",
        );
        let _ = write!(text, "\n  {lets} {tasks}");
    } else {
        let _ = write!(text, "\n  {lets}");
        for line in gated {
            let _ = write!(text, "\n    · {line}");
        }
    }
    let how = match gate.mode.as_str() {
        "confirm" => lang.pick("yes or no", "oui ou non"),
        "choice" => lang.pick(
            "one of the choices, as written",
            "l'un des choix, tel qu'écrit",
        ),
        _ => lang.pick("in words", "en mots"),
    };
    let trace = super::shown_trace(root, &gate.trace);
    let _ = match lang {
        Lang::Fr => write!(
            text,
            "\n  rien après la validation n'a encore eu lieu · la trace `{trace}` garde ce qui a tourné avant\n  la validation attend toujours · répondez {how} · rien ne répond à votre place"
        ),
        _ => write!(
            text,
            "\n  nothing after the gate has happened yet · the trace `{trace}` holds what ran before it\n  the gate still waits · answer {how} · nothing answers for you"
        ),
    };
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
    /// a task the gate does not hold back is not listed.
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

    /// « pourquoi ? » at a gate names its question once, never the card's whole message, and says
    /// that what it holds back waits for the answer (live gauntlet r04 B6, 2026-10-17).
    #[test]
    fn a_gate_is_explained_by_its_question_never_its_whole_message() {
        let dir = tempfile::tempdir().expect("tmp");
        let gate = PendingGate {
            workflow: std::path::PathBuf::from("digest.nika"),
            trace: std::path::PathBuf::from(".nika/traces/paused.ndjson"),
            task: "approve".to_owned(),
            message: "Poster ce digest sur https://hooks.example/team ?\n\n# Digest\n- une story\n"
                .to_owned(),
            mode: "confirm".to_owned(),
            values: None,
            withheld: None,
        };
        let said = explain_gate(&gate, dir.path(), Lang::Fr);
        assert!(
            said.starts_with("Le run de `digest.nika` s'est mis en pause à `approve` : le workflow demande là une validation humaine (« Poster ce digest sur https://hooks.example/team ? »)"),
            "{said}"
        );
        assert!(!said.contains("# Digest"), "{said}");
    }

    /// A run input is explained in the person's language, its lines under it too (live gauntlet
    /// wave 5, F list: « the input still waits · reply on the next line » under French answers).
    #[test]
    fn a_run_input_is_explained_in_the_persons_language() {
        let workflow = Path::new("digest.nika");
        let fr = explain_input(workflow, "ville", 2, Lang::Fr);
        assert!(
            fr.starts_with("`ville` est une entrée que `digest.nika` déclare")
                && fr.contains("encore 1 entrée(s) après celle-ci")
                && fr.ends_with("l'entrée attend toujours · répondez sur la ligne suivante · « annuler » abandonne le run"),
            "{fr}"
        );
        let en = explain_input(workflow, "city", 1, Lang::En);
        assert!(
            en.starts_with("`city` is an input `digest.nika` declares")
                && en.ends_with(
                    "the input still waits · reply on the next line · `cancel` drops the run"
                )
                && !en.contains("more input"),
            "{en}"
        );
    }
}
