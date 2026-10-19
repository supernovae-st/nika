// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::BTreeMap;
use std::sync::Arc;

use nika_kernel::provider::{ContentBlock, StopReason};
use nika_session_change::outcome::{Incarnation, QuestionId};
use nika_session_change::tools::ToolReply;
use serde_json::{Value, json};

use super::super::{Citations, Conversation, Pick};
use crate::tree::{EntryKind, Tree};

const REQUEST: &str = "Lis events.csv et écris payload out/event.json pour chaque ligne.";

/// Where the open name `payload out/event.json` starts: its readings, the short one the default.
fn event_choice() -> Value {
    let offer = |key: &str, path: &str, recommended: bool| {
        json!({"key": key, "label": path, "recommended": recommended,
            "values": [{"role": "output_path", "value": path}]})
    };
    let options = [
        offer("o0", "payload out/event.json", false),
        offer("o1", "out/event.json", true),
    ];
    json!({"key": "const.event_path", "role": "output_path", "question": "Lequel ?",
        "options": options})
}

/// The person's lines: the request `u1`, then `u2`, `line`.
fn lines(line: &str) -> Citations {
    let mut citations = Citations::default();
    citations.record(1, Some(("u1".into(), REQUEST.into())));
    citations.record(4, Some(("u2".into(), line.into())));
    citations
}

/// A conversation whose candidate writes `source`, as the person's request asked.
fn written(source: &str) -> Conversation {
    let mut conversation = Conversation::default();
    let texts = (source.to_owned(), "s".to_owned());
    let reply = conversation.write(&lines("-"), texts, (&[], &[]), &mut |_| Ok(()));
    assert!(!reply.is_error, "{reply:?}");
    conversation
}

/// The conversation as a reopen reads it back: the choice shown as the call `c1`, the person's
/// `u2` picking `o1`, kept and restored.
fn reopened() -> Conversation {
    let mut conversation = written("nika: events\n# writes out/event.json\n");
    let asker = Arc::new(Incarnation);
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let args = json!({"open": [event_choice()]});
    let citations = lines("le court");
    let call = Some("c1");
    let open = conversation.open_choices(&citations, &args, call, (&mut mint, &mut |_: &str| None));
    let shown = ("S".to_owned(), open.unwrap());
    assert_eq!(
        conversation.propose(&citations, &json!({}), shown),
        Ok(None)
    );
    let read = BTreeMap::from([("const.event_path".to_owned(), Pick::Offer("o1".to_owned()))]);
    conversation.answered_by("u2", "le court", &read);
    Conversation::restored(&conversation.kept()).unwrap()
}

/// A tree recording the request, the call `c1` to `name` with `input`, its reply naming `shown`
/// (as `ask` or `propose` names what it asked or showed), and the person's `u2`, `line`,
/// answering `answers`.
fn record(
    name: &str,
    (input, shown): (&Value, &Value),
    (line, answers): (&str, Option<&str>),
) -> Tree {
    let mut tree = Tree::new("s", "p", 0, |_| Ok(())).unwrap();
    tree.append_user(REQUEST, None, None, 1, |_| Ok(()))
        .unwrap();
    let call = ContentBlock::ToolUse {
        id: "c1".into(),
        name: name.into(),
        input: input.clone(),
    };
    let message = EntryKind::Assistant {
        content: vec![call],
        stop: StopReason::ToolUse,
        usage: None,
        model: None,
    };
    tree.append(message, 2, |_| Ok(())).unwrap();
    let mut said = json!({"revision": 1});
    said[if name == "ask" { "asked" } else { "open" }] = shown.clone();
    let reply = ToolReply::ends_turn(said.to_string());
    let (call, name) = ("c1".to_owned(), name.to_owned());
    tree.append(EntryKind::Parked { call, name, reply }, 3, |_| Ok(()))
        .unwrap();
    let answers = answers.map(str::to_owned);
    tree.append_user(line, answers, None, 4, |_| Ok(()))
        .unwrap();
    tree
}

/// The keys recalled, each with the number of offers recalled for it.
fn recalled(conversation: &Conversation) -> Vec<(String, usize)> {
    (conversation.recalled.iter())
        .map(|(key, offers)| (key.clone(), offers.len()))
        .collect()
}

/// A kept output is recalled with its question's offers only from the exact record of the call
/// that showed it: the line it rests on answered that call, in the current request; the reply
/// names the question; the call's input holds it once; and the offer picked holds the value.
#[test]
fn an_output_recalls_its_question_only_from_its_exact_record() {
    let input = json!({"open": [event_choice()]});
    let shown = json!([{"key": "const.event_path", "id": "q1"}]);
    let exact = [("const.event_path".to_owned(), 2)];
    let mut conversation = reopened();
    let tree = record("propose", (&input, &shown), ("le court", Some("c1")));
    conversation.recall(&tree, &lines("le court"));
    assert_eq!(recalled(&conversation), exact);
    // A line no tree line places in the request recalls nothing.
    conversation.recall(&tree, &Citations::default());
    assert!(recalled(&conversation).is_empty());
    let mut twice = input.clone();
    twice["open"].as_array_mut().unwrap().push(event_choice());
    let mut moved = input.clone();
    moved["open"][0]["options"][1]["values"][0]["value"] = json!("out/other.json");
    let none = json!([]);
    let broken = [
        ("propose", &input, &shown, None),
        ("propose", &input, &shown, Some("c9")),
        ("propose", &input, &none, Some("c1")),
        ("propose", &twice, &shown, Some("c1")),
        ("propose", &moved, &shown, Some("c1")),
        ("read", &input, &shown, Some("c1")),
    ];
    for (name, input, shown, answers) in broken {
        let mut conversation = reopened();
        let tree = record(name, (input, shown), ("le court", answers));
        conversation.recall(&tree, &lines("le court"));
        let why = format!("{name} {input} {shown} {answers:?}");
        assert!(recalled(&conversation).is_empty(), "{why}");
    }
}

/// An answer the person typed for a question Nika asked is recalled from the same record, with no
/// offer picked: their line holds the value as typed, or nothing is recalled.
#[test]
fn a_typed_answer_recalls_its_question_only_when_its_line_holds_the_value() {
    let mut question = event_choice();
    question["options"][1]["recommended"] = json!(false);
    question["reopens"] = json!({"message": "u1", "excerpt": "payload out/event.json"});
    let asker = Arc::new(Incarnation);
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let mut conversation = written("nika: events\n# writes data/event.json\n");
    let typed = "dans data/event.json";
    let citations = lines(typed);
    let args = json!({"questions": [question]});
    let posed = conversation.ask(
        &citations,
        &args,
        Some("c1"),
        (&mut mint, &mut |_: &str| None),
    );
    assert!(posed.ends_turn, "{posed:?}");
    conversation.answered_by("u2", typed, &BTreeMap::new());
    let row = json!({"key": "const.event_path", "value": "data/event.json", "message": "u2",
        "excerpt": "data/event.json"});
    let args = json!({"answered": [row]});
    let bound = conversation.ask(
        &citations,
        &args,
        Some("c2"),
        (&mut mint, &mut |_: &str| None),
    );
    assert!(bound.text.contains("const.event_path"), "{bound:?}");
    let reopened = Conversation::restored(&conversation.kept()).unwrap();
    let shown = json!([{"key": "const.event_path", "id": "q1"}]);
    let input = json!({"questions": [question]});
    for (line, exact) in [(typed, true), ("dans data/autre.json", false)] {
        let mut conversation = Conversation::restored(&reopened.kept()).unwrap();
        let tree = record("ask", (&input, &shown), (line, Some("c1")));
        conversation.recall(&tree, &citations);
        assert_eq!(!recalled(&conversation).is_empty(), exact, "{line}");
    }
}

/// A tree recording the request, an `ask` call `c1` for the message's text answered as `name`
/// with the reply naming `asked`, and the person's `u2` replying `bonjour.nika` to `c1` with the
/// note `nika`.
fn asked_record(name: &str, asked: &Value, nika: Option<&str>) -> Tree {
    let mut tree = Tree::new("s", "p", 0, |_| Ok(())).unwrap();
    tree.append_user(REQUEST, None, None, 1, |_| Ok(()))
        .unwrap();
    let input = json!({"questions": [{"key": "message_text", "question": "Quel texte ?"}]});
    let call = ContentBlock::ToolUse {
        id: "c1".into(),
        name: name.into(),
        input,
    };
    let message = EntryKind::Assistant {
        content: vec![call],
        stop: StopReason::ToolUse,
        usage: None,
        model: None,
    };
    tree.append(message, 2, |_| Ok(())).unwrap();
    let reply = ToolReply::ends_turn(json!({"asked": asked}).to_string());
    let (call, name) = ("c1".to_owned(), name.to_owned());
    tree.append(EntryKind::Parked { call, name, reply }, 3, |_| Ok(()))
        .unwrap();
    let line = (Some("c1".to_owned()), None);
    let nika = nika.map(str::to_owned);
    tree.append_user_read("bonjour.nika", line, nika, 4, |_| Ok(()))
        .unwrap();
    tree
}

/// The lines `conversation` holds as replies once recalled from `tree`, by citation.
fn replied_lines(tree: &Tree, citations: &Citations) -> Vec<Value> {
    let mut conversation = Conversation::default();
    conversation.recall(tree, citations);
    (conversation.replies_in(citations).values())
        .map(|raw| serde_json::from_str::<Value>(raw).unwrap()["choice_context"].clone())
        .collect()
}

/// After a reopen, a line that replied to a question Nika asked is read back as that reply from
/// the tree, though no value was bound from it. A line the host recorded as a natural rerun, a
/// line answering a proposal, an `ask` naming no question and a line outside the request are not.
#[test]
fn a_reply_to_an_asked_question_is_recalled_as_a_reply() {
    let citations = lines("bonjour.nika");
    let asked = json!([{"key": "message_text", "id": "q1"}]);
    let located = json!({"human_citation": "u2", "human_lines": [1, 2],
        "human_reply": "bonjour.nika"});
    let exact = asked_record("ask", &asked, None);
    assert_eq!(replied_lines(&exact, &citations), [located]);
    let reading = asked_record("ask", &asked, Some("Nika read the answer."));
    assert_eq!(replied_lines(&reading, &citations).len(), 1);
    let none = json!([]);
    let broken = [
        asked_record("ask", &asked, Some(super::RERUN_NOTE)),
        asked_record("propose", &asked, None),
        asked_record("ask", &none, None),
    ];
    for tree in &broken {
        assert!(replied_lines(tree, &citations).is_empty());
    }
    assert!(replied_lines(&exact, &Citations::default()).is_empty());
}

/// A tree recording the request, one `ask` call `c1` of two questions (`q1`, `q2`), the person's
/// `u2` answering it while it waited, the author's words ending that turn, and the person's `u3`
/// replying to `q2` after `c1` stopped waiting, recorded under `late`.
fn grouped_record(late: Option<&str>) -> Tree {
    let mut tree = Tree::new("s", "p", 0, |_| Ok(())).unwrap();
    tree.append_user(REQUEST, None, None, 1, |_| Ok(()))
        .unwrap();
    let input = json!({"questions": [{"key": "message_replace", "question": "Remplacer ?"},
        {"key": "message_text", "question": "Quel texte ?"}]});
    let asked = json!([{"key": "message_replace", "id": "q1"},
        {"key": "message_text", "id": "q2"}]);
    ask(&mut tree, "c1", input, &asked);
    let line = (Some("c1".to_owned()), None);
    tree.append_user_read("Oui", line, None, 4, |_| Ok(()))
        .unwrap();
    let words = ContentBlock::Text {
        text: "C'est noté.".into(),
    };
    let ended = EntryKind::Assistant {
        content: vec![words],
        stop: StopReason::EndTurn,
        usage: None,
        model: None,
    };
    tree.append(ended, 5, |_| Ok(())).unwrap();
    let line = (late.map(str::to_owned), None);
    tree.append_user_read("bonjour.nika", line, None, 6, |_| Ok(()))
        .unwrap();
    tree
}

/// The `ask` call `id` with `input`, parked on its reply naming `asked`.
fn ask(tree: &mut Tree, id: &str, input: Value, asked: &Value) {
    let call = ContentBlock::ToolUse {
        id: id.into(),
        name: "ask".into(),
        input,
    };
    let message = EntryKind::Assistant {
        content: vec![call],
        stop: StopReason::ToolUse,
        usage: None,
        model: None,
    };
    tree.append(message, 2, |_| Ok(())).unwrap();
    let reply = ToolReply::ends_turn(json!({"asked": asked}).to_string());
    let (call, name) = (id.to_owned(), "ask".to_owned());
    tree.append(EntryKind::Parked { call, name, reply }, 3, |_| Ok(()))
        .unwrap();
}

/// A reply to another question of an `ask` group, given by its identity after the call stopped
/// waiting, names the call that asked it: `asking_call` finds that call by the question's
/// identity, and the line recorded under it is read back as a reply, as the first answer is. An
/// identity no call asked, or two calls asked, finds none, and a line named under no call is no
/// reply.
#[test]
fn a_late_reply_names_the_ask_that_asked_its_question() {
    let asker = Arc::new(Incarnation);
    let id = |witness: &str| QuestionId::new(witness.to_owned(), &asker);
    let tree = grouped_record(Some("c1"));
    assert_eq!(super::asking_call(&tree, &id("q2")).as_deref(), Some("c1"));
    assert_eq!(super::asking_call(&tree, &id("q1")).as_deref(), Some("c1"));
    assert_eq!(super::asking_call(&tree, &id("q9")), None);
    let mut citations = lines("Oui");
    citations.record(6, Some(("u3".into(), "bonjour.nika".into())));
    let cited = |tree: &Tree| -> Vec<String> {
        (replied_lines(tree, &citations).iter())
            .map(|context| context["human_citation"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(cited(&tree), ["u2", "u3"]);
    assert_eq!(cited(&grouped_record(None)), ["u2"]);
    let mut twice = grouped_record(Some("c1"));
    let input = json!({"questions": [{"key": "message_text", "question": "Quel texte ?"}]});
    ask(
        &mut twice,
        "c3",
        input,
        &json!([{"key": "message_text", "id": "q2"}]),
    );
    assert_eq!(super::asking_call(&twice, &id("q2")), None);
    assert_eq!(super::asking_call(&twice, &id("q1")).as_deref(), Some("c1"));
}
