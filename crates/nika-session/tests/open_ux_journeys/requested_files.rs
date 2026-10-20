// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A new workflow saved under the file the person's request names, whatever `nika:` its author
//! writes (`count-lines`): created there, replacing a file already there only on the ordinary
//! consent, a folder the request gives kept, a component it reads through its `const:` untouched.

use serde_json::{Value, json};

use super::trials::{COUNTED, SALES, count_write};
use super::{Act, COUNT, Scenario};
use crate::peer::{Script, call, call_saying, say};

/// The counting request, saved under the file it names.
const NAMED: &str = "compte les lignes de ./data/ventes.csv et ecris le total dans ./out/total.txt, puis enregistre le workflow dans comptage.nika";
/// The same request, saved under a folder it names.
const NESTED: &str = "compte les lignes de ./data/ventes.csv et ecris le total dans ./out/total.txt, puis enregistre le workflow dans workflows/comptage.nika";
/// A workflow already at the file the request names.
const BEFORE: &str = "nika: comptage\npermits:\n  tools: [\"nika:write\"]\n  fs:\n    write: [\"./out/old.txt\"]\ntasks:\n  old:\n    invoke: { tool: \"nika:write\", args: { path: \"./out/old.txt\", content: \"0\" } }\n";
/// The counting request over a component the project holds, naming no file for the workflow.
const COMPONENT: &str =
    "compte les lignes de ./components/base.nika et ecris le total dans ./out/total.txt";
/// The same request, saving the workflow under a file it names.
const COMPONENT_SAVED: &str = "compte les lignes de ./components/base.nika et ecris le total dans ./out/total.txt, puis enregistre le workflow dans comptage.nika";
/// The component the project holds: another workflow, with its own `nika:`.
pub(crate) const BASE: &str = "nika: base\ntasks: {}\n";
/// What the author says once the component's counting workflow is shown.
const COMPONENT_COUNTED: &str =
    "Voici le workflow : il compte les lignes de ./components/base.nika.";

/// The counting workflow over the component, which it reads through its own `const:`.
fn component_count() -> Value {
    let source = COUNT
        .replace(
            "permits:",
            "const:\n  component_path: ./components/base.nika\npermits:",
        )
        .replace("[\"./data/ventes.csv\"]", "[\"./components/base.nika\"]")
        .replace(
            "{ path: \"./data/ventes.csv\" }",
            "{ path: \"${{ const.component_path }}\" }",
        );
    json!({"source": source, "resolutions": [],
        "summary": "compte les lignes de ./components/base.nika"})
}

/// `write` proposed for `request` over the project `files`, the author saying `said`, then the
/// person's yes.
fn saved_under(
    request: &'static str,
    files: Vec<(&'static str, &'static str)>,
    (write, said): (Value, &'static str),
) -> Scenario {
    Scenario {
        files,
        history: false,
        preparing: false,
        acts: vec![Act::Turn(request), Act::Submit("oui")],
        script: Script {
            agent: vec![
                call("candidate_write", write),
                call_saying(said, "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// The requested-file journey `name` names (`scenarios::scenario` routes them here).
pub(super) fn scenario(name: &str) -> Scenario {
    match name {
        "requested_file_created" => requested_file_created(),
        "requested_file_occupied" => requested_file_occupied(),
        "requested_file_nested" => requested_file_nested(),
        "component_read" => component_read(),
        "component_read_saved" => component_read_saved(),
        "requested_file_answered_content" => requested_file_answered_content(),
        "requested_file_answered_named" => requested_file_answered_named(),
        "requested_file_unbound_content" => requested_file_unbound_content(),
        "requested_file_grouped_late_reply" => requested_file_grouped_late_reply(),
        "requested_file_grouped_late_named" => requested_file_grouped_late_named(),
        "requested_file_grouped_declined" => requested_file_grouped_declined(),
        "requested_file_grouped_declined_named" => requested_file_grouped_declined_named(),
        "requested_file_grouped_declined_first" => requested_file_grouped_declined_first(),
        "requested_file_grouped_declined_first_named" => {
            requested_file_grouped_declined_first_named()
        }
        other => panic!("unknown scenario {other}"),
    }
}

/// The request names `comptage.nika`, which the project does not hold.
pub(super) fn requested_file_created() -> Scenario {
    let files = vec![("data/ventes.csv", SALES)];
    saved_under(NAMED, files, (count_write(), COUNTED))
}

/// The request names `comptage.nika`, which another workflow already is.
pub(super) fn requested_file_occupied() -> Scenario {
    let files = vec![("data/ventes.csv", SALES), ("comptage.nika", BEFORE)];
    saved_under(NAMED, files, (count_write(), COUNTED))
}

/// The request names `workflows/comptage.nika`, in a folder the project does not hold yet.
pub(super) fn requested_file_nested() -> Scenario {
    let files = vec![("data/ventes.csv", SALES)];
    saved_under(NESTED, files, (count_write(), COUNTED))
}

/// The request reads the component through the workflow's `const:` and names no file for it.
pub(super) fn component_read() -> Scenario {
    let files = vec![("components/base.nika", BASE)];
    saved_under(COMPONENT, files, (component_count(), COMPONENT_COUNTED))
}

/// The request reads the component and names `comptage.nika` for the workflow.
pub(super) fn component_read_saved() -> Scenario {
    let files = vec![("components/base.nika", BASE)];
    saved_under(
        COMPONENT_SAVED,
        files,
        (component_count(), COMPONENT_COUNTED),
    )
}

/// A request for a message whose text the person gives in reply to the author's question.
const MESSAGE_REQUEST: &str = "écris un court message dans ./out/message.txt";
/// The same request, saving the workflow under a file it names.
const MESSAGE_NAMED: &str =
    "écris un court message dans ./out/message.txt, puis enregistre le workflow dans bonjour.nika";
/// The text the person gives: business content that looks like a workflow file.
const CONTENT: &str = "bonjour.nika";
/// The workflow writing that text, as its author names it.
const MESSAGE: &str = r#"nika: message
permits:
  tools: ["nika:write"]
  fs:
    write: ["./out/message.txt"]
tasks:
  message:
    invoke:
      tool: "nika:write"
      args:
        path: "./out/message.txt"
        content: "bonjour.nika"
        overwrite: true
        create_dirs: true
"#;

/// The author asks what text to write; the person replies `bonjour.nika`; the author binds it from
/// their line, writes the workflow and proposes; the person says yes.
fn answered_content(request: &'static str) -> Scenario {
    let asked = json!({"questions": [{"key": "message_text", "free_text": true, "options": [],
        "question": "Quel texte écrire dans ./out/message.txt ?"}]});
    let bound = json!({"answered": [{"key": "message_text", "value": CONTENT, "message": "u2",
        "excerpt": CONTENT}]});
    let write = json!({"source": MESSAGE, "resolutions": [], "summary": "écrit le message"});
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![Act::Turn(request), Act::Submit(CONTENT), Act::Submit("oui")],
        script: Script {
            agent: vec![
                call("ask", asked),
                call("ask", bound),
                call("candidate_write", write),
                call_saying("Voici le workflow.", "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// The request names no file for the workflow; the reply is the message's text.
pub(super) fn requested_file_answered_content() -> Scenario {
    answered_content(MESSAGE_REQUEST)
}

/// The request names `bonjour.nika` for the workflow; the reply is the same text.
pub(super) fn requested_file_answered_named() -> Scenario {
    answered_content(MESSAGE_NAMED)
}

/// The request names no file for the workflow; the author never binds the reply: it writes the
/// text and proposes.
pub(super) fn requested_file_unbound_content() -> Scenario {
    let mut scenario = answered_content(MESSAGE_REQUEST);
    scenario.script.agent.remove(1);
    scenario
}

/// The person's pick for the question that comes first in the group.
const REPLACE: &str = "Oui";
/// What the person asks once the Session reopens.
const EXCLAIM: &str = "ajoute un point d'exclamation au message";

/// The author's one `ask` of two questions: whether to replace the message, then its text.
fn grouped_ask() -> Value {
    json!({"questions": [
        {"key": "message_replace", "free_text": false,
         "question": "Remplacer le message s'il existe déjà ?",
         "options": [{"key": "oui", "label": REPLACE}, {"key": "non", "label": "Non"}]},
        {"key": "message_text", "free_text": true, "options": [],
         "question": "Quel texte écrire dans ./out/message.txt ?"}
    ]})
}

/// The message's workflow writing `content`, as its author writes it.
fn message_write(content: &str) -> Value {
    let quoted = format!("content: \"{content}\"");
    let source = MESSAGE.replace("content: \"bonjour.nika\"", &quoted);
    json!({"source": source, "resolutions": [], "summary": "écrit le message"})
}

/// The author asks both questions in one `ask`. The person picks the first by its identity and
/// the author ends that turn in words, asking nothing; that identity, consumed, is presented again
/// and answers nothing. The person gives the text (`bonjour.nika`) by the second question's
/// identity; the author never binds it: it writes the workflow and proposes. The Session reopens,
/// the person asks for one change, and the author writes it and proposes again.
fn grouped_late_reply(request: &'static str) -> Scenario {
    Scenario {
        files: Vec::new(),
        history: true,
        preparing: false,
        acts: vec![
            Act::Turn(request),
            Act::AnswerQuestionOf(0, REPLACE),
            Act::AnswerQuestionOf(0, CONTENT),
            Act::AnswerQuestionOf(1, CONTENT),
            Act::Reopen,
            Act::Turn(EXCLAIM),
        ],
        script: Script {
            agent: vec![
                call("ask", grouped_ask()),
                say("C'est noté : le message remplacera l'ancien."),
                call("candidate_write", message_write(CONTENT)),
                call_saying("Voici le workflow.", "propose", json!({})),
                call("candidate_write", message_write("bonjour.nika !")),
                call_saying("Voici le workflow corrigé.", "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: vec!["oui".to_owned()],
            verdicts: Vec::new(),
        },
    }
}

/// The author asks both questions in one `ask`. The person picks the first by its identity, and
/// the author writes a draft and proposes while the text's question still waits: the draft does
/// not show that question, so its identity answers it first. The person gives the text
/// (`bonjour.nika`) that way; the author never binds it: it writes the workflow and proposes. The
/// person declines that proposal (nothing is saved), and the question's identity, answered, takes
/// no answer again. The Session reopens, the person asks for one change, and the author writes it
/// and proposes again.
fn grouped_declined(request: &'static str) -> Scenario {
    Scenario {
        files: Vec::new(),
        history: true,
        preparing: false,
        acts: vec![
            Act::Turn(request),
            Act::AnswerQuestionOf(0, REPLACE),
            Act::AnswerQuestionOf(1, CONTENT),
            Act::Submit("non"),
            Act::AnswerQuestionOf(1, CONTENT),
            Act::Reopen,
            Act::Turn(EXCLAIM),
        ],
        script: Script {
            agent: vec![
                call("ask", grouped_ask()),
                call("candidate_write", message_write("brouillon")),
                call_saying("Voici un premier jet.", "propose", json!({})),
                call("candidate_write", message_write(CONTENT)),
                call_saying("Voici le workflow.", "propose", json!({})),
                call("candidate_write", message_write("bonjour.nika !")),
                call_saying("Voici le workflow corrigé.", "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: vec!["oui".to_owned()],
            verdicts: Vec::new(),
        },
    }
}

/// The author asks both questions in one `ask`. The person picks the first by its identity, and
/// the author writes a draft and proposes while the text's question still waits. The person
/// declines the draft at its consent door (nothing is saved) while that question still waits,
/// then gives the text (`bonjour.nika`) by the question's identity; the author never binds it: it
/// writes the workflow and proposes. The Session reopens, the person asks for one change, and the
/// author writes it and proposes again.
fn grouped_declined_first(request: &'static str) -> Scenario {
    Scenario {
        files: Vec::new(),
        history: true,
        preparing: false,
        acts: vec![
            Act::Turn(request),
            Act::AnswerQuestionOf(0, REPLACE),
            Act::Consent("non"),
            Act::AnswerQuestionOf(1, CONTENT),
            Act::Reopen,
            Act::Turn(EXCLAIM),
        ],
        script: Script {
            agent: vec![
                call("ask", grouped_ask()),
                call("candidate_write", message_write("brouillon")),
                call_saying("Voici un premier jet.", "propose", json!({})),
                call("candidate_write", message_write(CONTENT)),
                call_saying("Voici le workflow.", "propose", json!({})),
                call("candidate_write", message_write("bonjour.nika !")),
                call_saying("Voici le workflow corrigé.", "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: vec!["oui".to_owned()],
            verdicts: Vec::new(),
        },
    }
}

/// The request names no file for the workflow; the late reply is the message's text.
pub(super) fn requested_file_grouped_late_reply() -> Scenario {
    grouped_late_reply(MESSAGE_REQUEST)
}

/// The request names `bonjour.nika` for the workflow; the late reply is the same text.
pub(super) fn requested_file_grouped_late_named() -> Scenario {
    grouped_late_reply(MESSAGE_NAMED)
}

/// The request names no file for the workflow; the text comes while a draft waits.
pub(super) fn requested_file_grouped_declined() -> Scenario {
    grouped_declined(MESSAGE_REQUEST)
}

/// The request names `bonjour.nika` for the workflow; the text comes while a draft waits.
pub(super) fn requested_file_grouped_declined_named() -> Scenario {
    grouped_declined(MESSAGE_NAMED)
}

/// The request names no file for the workflow; the text comes once the draft is declined.
pub(super) fn requested_file_grouped_declined_first() -> Scenario {
    grouped_declined_first(MESSAGE_REQUEST)
}

/// The request names `bonjour.nika` for the workflow; the text comes once the draft is declined.
pub(super) fn requested_file_grouped_declined_first_named() -> Scenario {
    grouped_declined_first(MESSAGE_NAMED)
}
