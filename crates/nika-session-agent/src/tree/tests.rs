// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use nika_kernel::provider::{ContentBlock, Role, StopReason};
use nika_session_change::tools::ToolReply;
use proptest::prelude::*;
use serde_json::json;

use super::*;
use crate::run::Store;

fn open() -> (Tree, Vec<String>) {
    let mut lines = Vec::new();
    let tree = Tree::new("session-1", "project-digest", 1_000, |l| {
        Store::append(&mut lines, l)
    })
    .unwrap();
    (tree, lines)
}

fn file(lines: &[String]) -> String {
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

fn add(tree: &mut Tree, lines: &mut Vec<String>, kind: EntryKind) -> EntryId {
    tree.append(kind, 2_000, |l| Store::append(lines, l))
        .unwrap()
}

fn say(tree: &mut Tree, lines: &mut Vec<String>, text: &str) -> (EntryId, String) {
    tree.append_user(text, None, None, 2_000, |l| Store::append(lines, l))
        .unwrap()
}

fn message(blocks: Vec<ContentBlock>) -> EntryKind {
    EntryKind::Assistant {
        content: blocks,
        stop: StopReason::ToolUse,
        usage: None,
        model: None,
    }
}

fn call(id: &str, name: &str) -> ContentBlock {
    ContentBlock::ToolUse {
        id: id.into(),
        name: name.into(),
        input: json!({"path": "news.nika"}),
    }
}

fn text(words: &str) -> ContentBlock {
    ContentBlock::Text { text: words.into() }
}

fn result(call: &str, name: &str, reply: ToolReply) -> EntryKind {
    EntryKind::ToolResult {
        call: call.into(),
        name: name.into(),
        reply,
    }
}

fn wire(entries: &[Entry]) -> Value {
    serde_json::to_value(entries).unwrap()
}

/// The text blocks and tool results of one message, as `(kind, text, call, is_error)`.
fn blocks(message: &Message) -> Vec<(&'static str, String, String, bool)> {
    (message.content.iter())
        .map(|block| match block {
            ContentBlock::Text { text } => ("text", text.clone(), String::new(), false),
            ContentBlock::ToolUse { id, .. } => ("use", String::new(), id.clone(), false),
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => ("result", content.clone(), tool_use_id.clone(), *is_error),
            _ => ("other", String::new(), String::new(), false),
        })
        .collect()
}

#[test]
fn a_tree_reads_back_as_it_was_written() {
    let (mut tree, mut lines) = open();
    add(
        &mut tree,
        &mut lines,
        EntryKind::System {
            text: "Author with the person.".into(),
        },
    );
    let (first, cite) = say(
        &mut tree,
        &mut lines,
        "Récupère les news de « Hacker News »",
    );
    assert_eq!(cite, "u1");
    let blocks = vec![text("Reading."), call("c1", "read"), call("c2", "ask")];
    add(&mut tree, &mut lines, message(blocks));
    add(
        &mut tree,
        &mut lines,
        result("c1", "read", ToolReply::ok("nika: news")),
    );
    let parked = EntryKind::Parked {
        call: "c2".into(),
        name: "ask".into(),
        reply: ToolReply::ends_turn("asked q1"),
    };
    add(&mut tree, &mut lines, parked);
    let fact = EntryKind::Fact {
        name: "candidate".into(),
        data: json!({"revision": 1}),
    };
    add(&mut tree, &mut lines, fact);

    let back = Tree::replay(&file(&lines)).unwrap();
    assert_eq!(wire(back.entries()), wire(tree.entries()));
    assert_eq!(back.header(), tree.header());
    assert_eq!(back.header().contract, TREE);
    assert_eq!(back.parked(), Some(("c2", "ask")));
    let (entry, words) = back.cited("u1").unwrap();
    assert_eq!(
        (&entry.id, words),
        (&first, "Récupère les news de « Hacker News »")
    );
    assert!(back.cited("u2").is_none());
    assert_eq!(back.leaf().map(|e| e.id.as_str()), Some("e6"));
    assert_eq!(back.branch().len(), 6);

    // The tree read back goes on where it stopped: the next line, the next citation.
    let (mut back, mut more) = (back, lines.clone());
    let answer = back.append_user("Le Monde aussi", Some("c2".into()), None, 3_000, |l| {
        Store::append(&mut more, l)
    });
    assert_eq!(answer.unwrap(), (EntryId("e7".into()), "u2".into()));
    assert_eq!(back.parked(), None);
    assert_eq!(Tree::replay(&file(&more)).unwrap().entries().len(), 7);
}

#[test]
fn a_cut_or_altered_file_is_refused_whole() {
    let (mut tree, mut lines) = open();
    say(&mut tree, &mut lines, "first line");
    say(&mut tree, &mut lines, "second line");
    let whole = file(&lines);
    assert!(Tree::replay(&whole).is_ok());

    assert!(matches!(
        Tree::replay(whole.trim_end()),
        Err(TreeError::Truncated)
    ));
    let edited = whole.replacen("second line", "second lime", 1);
    assert!(matches!(
        Tree::replay(&edited),
        Err(TreeError::Damaged { line: 3, .. })
    ));
    let swapped = file(&[lines[0].clone(), lines[2].clone(), lines[1].clone()]);
    assert!(matches!(
        Tree::replay(&swapped),
        Err(TreeError::Damaged { line: 2, .. })
    ));
    let dropped = file(&[lines[0].clone(), lines[2].clone()]);
    assert!(matches!(
        Tree::replay(&dropped),
        Err(TreeError::Damaged { line: 2, .. })
    ));
    let headless = file(&lines[1..]);
    assert!(matches!(
        Tree::replay(&headless),
        Err(TreeError::Damaged { line: 1, .. })
    ));
    let doubled = file(&[lines[0].clone(), lines[0].clone()]);
    assert!(matches!(
        Tree::replay(&doubled),
        Err(TreeError::Damaged { line: 2, .. })
    ));
}

#[test]
fn an_entry_whose_line_was_not_written_is_not_in_the_tree() {
    let (mut tree, mut lines) = open();
    say(&mut tree, &mut lines, "first line");
    let refused = tree.append_user("lost", None, None, 2_000, |_| {
        Err(io::Error::other("disk full"))
    });
    assert!(matches!(refused, Err(TreeError::Write(_))));
    assert_eq!(tree.entries().len(), 1);
    let (next, cite) = say(&mut tree, &mut lines, "second line");
    assert_eq!((next.as_str(), cite.as_str()), ("e2", "u2"));
    assert_eq!(Tree::replay(&file(&lines)).unwrap().entries().len(), 2);
}

#[test]
fn a_citation_is_never_reused() {
    let (mut tree, mut lines) = open();
    say(&mut tree, &mut lines, "first line");
    let second_u1 = EntryKind::User {
        cite: "u1".into(),
        text: "again".into(),
        answers: None,
        queued: None,
        nika: None,
    };
    let refused = tree.append(second_u1, 2_000, |l| Store::append(&mut lines, l));
    assert!(matches!(refused, Err(TreeError::Damaged { line: 3, .. })));
    assert_eq!(lines.len(), 2, "nothing was written");
}

#[test]
fn the_model_reads_every_call_followed_by_its_reply() {
    let (mut tree, mut lines) = open();
    add(
        &mut tree,
        &mut lines,
        EntryKind::System { text: "old".into() },
    );
    add(
        &mut tree,
        &mut lines,
        EntryKind::System {
            text: "instructions".into(),
        },
    );
    say(&mut tree, &mut lines, "hello");
    add(
        &mut tree,
        &mut lines,
        message(vec![
            text("Reading."),
            call("c1", "read"),
            call("c2", "check"),
        ]),
    );
    add(
        &mut tree,
        &mut lines,
        result("c1", "read", ToolReply::ok("nika: news")),
    );
    add(&mut tree, &mut lines, EntryKind::Stopped { queued: vec![] });
    say(&mut tree, &mut lines, "go on");

    let context = tree.context();
    assert_eq!(context.system.as_deref(), Some("instructions"));
    let roles: Vec<Role> = context.messages.iter().map(|m| m.role).collect();
    assert_eq!(roles, [Role::User, Role::Assistant, Role::User, Role::User]);
    let hello = blocks(&context.messages[0]);
    assert_eq!(hello[0].1, "hello");
    assert_eq!(hello[1].1, "(cited as u1)");
    let replies = blocks(&context.messages[2]);
    assert_eq!(
        replies[0],
        ("result", "nika: news".into(), "c1".into(), false)
    );
    assert_eq!(replies[1], ("result", NOT_RUN.into(), "c2".into(), true));
    assert_eq!(blocks(&context.messages[3])[1].1, "(cited as u2)");
}

#[test]
fn an_answer_is_the_reply_of_the_call_that_waited() {
    let (mut tree, mut lines) = open();
    say(&mut tree, &mut lines, "a digest of the news");
    add(&mut tree, &mut lines, message(vec![call("c1", "ask")]));
    let parked = EntryKind::Parked {
        call: "c1".into(),
        name: "ask".into(),
        reply: ToolReply::ends_turn("asked q1"),
    };
    add(&mut tree, &mut lines, parked);
    // While it waits, a summary reads the call as unanswered, never as failed.
    let waiting = blocks(&tree.context().messages[2]);
    assert_eq!(waiting[0], ("result", WAITING.into(), "c1".into(), false));

    let answer = tree.append_user("Le Monde aussi", Some("c1".into()), None, 3_000, |l| {
        Store::append(&mut lines, l)
    });
    assert_eq!(answer.unwrap().1, "u2");
    let context = tree.context();
    assert_eq!(
        context.messages.len(),
        3,
        "the answer is no separate message"
    );
    let reply = blocks(&context.messages[2]);
    let expected = "The person answered, cited as u2:\nLe Monde aussi";
    assert_eq!(reply[0], ("result", expected.into(), "c1".into(), false));
    assert_eq!(
        tree.cited("u2").map(|(_, words)| words),
        Some("Le Monde aussi")
    );
}

#[test]
fn a_summary_stands_for_the_entries_it_folded() {
    let (mut tree, mut lines) = open();
    add(
        &mut tree,
        &mut lines,
        EntryKind::System {
            text: "instructions".into(),
        },
    );
    say(&mut tree, &mut lines, "first");
    add(&mut tree, &mut lines, message(vec![text("a")]));
    let (second, _) = say(&mut tree, &mut lines, "second");
    add(&mut tree, &mut lines, message(vec![text("b")]));
    let folded = tree.append_compaction("S".into(), second, 100, 2_000, |l| {
        Store::append(&mut lines, l)
    });
    assert!(folded.is_ok());
    say(&mut tree, &mut lines, "third");

    let context = Tree::replay(&file(&lines)).unwrap().context();
    assert_eq!(context.system.as_deref(), Some("instructions"));
    let texts: Vec<String> = (context.messages.iter())
        .map(|m| blocks(m)[0].1.clone())
        .collect();
    // The person's lines it folded stay beside it, verbatim, each with its citation (live
    // gauntlet wave 6 r02: with the summary alone, none of their earlier words could be cited).
    let folded = serde_json::json!({"cite": "u1", "text": "first"});
    assert_eq!(
        texts,
        [
            format!("{SUMMARY_LEAD}\n\nS\n\n{FOLDED_LEAD}\n{folded}"),
            "second".into(),
            "b".into(),
            "third".into()
        ]
    );
    // The summary comes back as the model's evidence, never as the person's words.
    assert!(
        SUMMARY_LEAD.contains("not the person's words"),
        "{SUMMARY_LEAD}"
    );
    // Every entry stays in the tree: a folded line is still the person's to cite.
    assert_eq!(tree.cited("u1").map(|(_, words)| words), Some("first"));
}

/// One step of a conversation a property builds.
#[derive(Clone, Debug)]
enum Step {
    Say(String),
    Message(u8, bool, bool),
    Stop,
    Fact,
    Note,
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        "[a-zé« »]{0,12}".prop_map(Step::Say),
        (0u8..4, any::<bool>(), any::<bool>())
            .prop_map(|(calls, parks, proposes)| Step::Message(calls, parks, proposes)),
        Just(Step::Stop),
        Just(Step::Fact),
        Just(Step::Note),
    ]
}

/// Build a conversation from `steps` the way the loop records one: every call of a message is
/// answered, parked or left unrun by a Stop, and a line answers the call that waits.
fn build(steps: &[Step]) -> (Tree, Vec<String>) {
    let (mut tree, mut lines) = open();
    let mut next = 0;
    for step in steps {
        match step {
            Step::Say(words) => {
                let answers = tree.parked().map(|(call, _)| call.to_owned());
                let written = tree.append_user(words.as_str(), answers, None, 2_000, |l| {
                    Store::append(&mut lines, l)
                });
                written.unwrap();
            }
            Step::Message(count, parks, proposes) if tree.parked().is_none() => {
                let ids: Vec<String> = (0..*count)
                    .map(|k| format!("c{}", next + usize::from(k)))
                    .collect();
                next += usize::from(*count);
                let calls = ids.iter().map(|id| call(id, "read")).collect();
                add(&mut tree, &mut lines, message(calls));
                for (k, id) in ids.iter().enumerate() {
                    let kind = if *parks && k == 0 {
                        EntryKind::Parked {
                            call: id.clone(),
                            name: if *proposes { "propose" } else { "ask" }.into(),
                            reply: ToolReply::ends_turn("asked"),
                        }
                    } else {
                        result(id, "read", ToolReply::ok("ok"))
                    };
                    add(&mut tree, &mut lines, kind);
                }
            }
            Step::Stop if tree.parked().is_none() => {
                add(
                    &mut tree,
                    &mut lines,
                    message(vec![call(&format!("c{next}"), "read")]),
                );
                next += 1;
                add(&mut tree, &mut lines, EntryKind::Stopped { queued: vec![] });
            }
            Step::Fact => {
                add(
                    &mut tree,
                    &mut lines,
                    EntryKind::Fact {
                        name: "observation".into(),
                        data: json!({}),
                    },
                );
            }
            // Nika's note starts a run the model answers, as `Agent::note` does: never while a
            // question waits for the person.
            Step::Note if tree.parked().is_none_or(|(_, name)| name == "propose") => {
                let note = EntryKind::Note {
                    text: "the run failed".into(),
                };
                add(&mut tree, &mut lines, note);
                let answer = EntryKind::Assistant {
                    content: vec![text("it failed")],
                    stop: StopReason::EndTurn,
                    usage: None,
                    model: None,
                };
                add(&mut tree, &mut lines, answer);
            }
            _ => {}
        }
    }
    (tree, lines)
}

proptest! {
    /// Any conversation reads back as written, and the model reads every call of a message
    /// followed at once by one reply per call, in the order called.
    #[test]
    fn any_conversation_reads_back_and_pairs_every_call(steps in prop::collection::vec(step(), 0..30)) {
        let (tree, lines) = build(&steps);
        let back = Tree::replay(&file(&lines)).unwrap();
        prop_assert_eq!(wire(back.entries()), wire(tree.entries()));
        let messages = back.context().messages;
        // The person's line, last on the branch, is the last thing the model reads.
        if matches!(back.branch().last().map(|e| &e.kind), Some(EntryKind::User { .. })) {
            prop_assert_eq!(messages.last().map(|m| m.role), Some(Role::User));
        }
        for (k, message) in messages.iter().enumerate() {
            let used: Vec<String> = blocks(message).into_iter().filter(|b| b.0 == "use").map(|b| b.2).collect();
            if used.is_empty() {
                continue;
            }
            let replied: Vec<String> = messages.get(k + 1).map(blocks).unwrap_or_default()
                .into_iter().filter(|b| b.0 == "result").map(|b| b.2).collect();
            prop_assert_eq!(used, replied);
        }
    }

    /// Changing any one character of a tree's file is detected: the tree is refused, or read
    /// back exactly as written (an absent optional field respelled), never as something else.
    #[test]
    fn any_altered_character_is_refused(steps in prop::collection::vec(step(), 1..12), at in any::<prop::sample::Index>(), to in 0x20u8..0x7f) {
        let (tree, lines) = build(&steps);
        let whole = file(&lines);
        let mut bytes = whole.clone().into_bytes();
        let k = at.index(bytes.len() - 1);
        prop_assume!(bytes[k] != b'\n' && bytes[k] != to && bytes[k].is_ascii());
        bytes[k] = to;
        let altered = String::from_utf8(bytes).unwrap();
        if let Ok(read) = Tree::replay(&altered) {
            prop_assert_eq!(wire(read.entries()), wire(tree.entries()));
            prop_assert_eq!(read.header(), tree.header());
        }
    }
}

/// A host that keeps the file reads each line it just made durable: its place, and the
/// person's line it records, with the citation the tree minted and the call it answers.
#[test]
fn a_written_line_says_its_place_and_the_persons_words() {
    let (mut tree, mut lines) = open();
    assert_eq!(tree.next_cite(), "u1");
    assert_eq!(
        read_line(&lines[0]).unwrap(),
        LineFacts { n: 0, person: None }
    );
    say(&mut tree, &mut lines, "un digest des news");
    add(&mut tree, &mut lines, message(vec![call("c1", "ask")]));
    assert_eq!(tree.next_cite(), "u2");
    let answered = tree.append_user("oui", Some("c1".into()), None, 3_000, |l| {
        Store::append(&mut lines, l)
    });
    assert!(answered.is_ok());
    let first = read_line(&lines[1]).unwrap();
    assert_eq!(first.n, 1);
    let person = first.person.unwrap();
    assert_eq!(
        (person.cite.as_str(), person.text.as_str()),
        ("u1", "un digest des news")
    );
    assert_eq!(read_line(&lines[2]).unwrap().person, None);
    let reply = read_line(&lines[3]).unwrap().person.unwrap();
    assert_eq!(
        (reply.cite.as_str(), reply.answers.as_deref()),
        ("u2", Some("c1"))
    );
    assert!(read_line("not a tree line").is_none());
}

/// What the model said last is the text of its last message, its calls aside.
#[test]
fn the_last_words_are_the_text_of_the_last_message() {
    let (mut tree, mut lines) = open();
    assert_eq!(tree.last_said(), "");
    say(&mut tree, &mut lines, "un digest");
    add(
        &mut tree,
        &mut lines,
        message(vec![text("D'abord une question."), call("c1", "ask")]),
    );
    assert_eq!(tree.last_said(), "D'abord une question.");
    add(&mut tree, &mut lines, message(vec![call("c2", "ask")]));
    assert_eq!(tree.last_said(), "");
}

#[test]
fn a_host_reply_can_close_a_parked_call_without_a_human_answer() {
    let (mut tree, mut lines) = open();
    say(&mut tree, &mut lines, "the original request");
    add(
        &mut tree,
        &mut lines,
        message(vec![call("c1", "ask"), call("c2", "read")]),
    );
    add(
        &mut tree,
        &mut lines,
        result(
            "c2",
            "read",
            ToolReply::ok("The observed file is unchanged."),
        ),
    );
    add(
        &mut tree,
        &mut lines,
        EntryKind::Parked {
            call: "c1".into(),
            name: "ask".into(),
            reply: ToolReply::ends_turn("asked q1"),
        },
    );
    add(
        &mut tree,
        &mut lines,
        result("other", "ask", ToolReply::error("unrelated")),
    );
    assert_eq!(tree.parked(), Some(("c1", "ask")));
    add(
        &mut tree,
        &mut lines,
        result("c1", "ask", ToolReply::ends_turn("still waiting")),
    );
    assert_eq!(tree.parked(), Some(("c1", "ask")));
    let expired = "The question expired; no answer or authorization was supplied.";
    add(
        &mut tree,
        &mut lines,
        result("c1", "ask", ToolReply::error(expired)),
    );
    assert_eq!(tree.parked(), None);
    say(&mut tree, &mut lines, "continue my request");
    let replayed = Tree::replay(&file(&lines)).unwrap();
    assert_eq!(replayed.parked(), None);
    let context = replayed.context();
    assert_eq!(context.messages.len(), 4);
    assert_eq!(
        blocks(&context.messages[2]),
        vec![
            ("result", expired.into(), "c1".into(), true),
            (
                "result",
                "The observed file is unchanged.".into(),
                "c2".into(),
                false
            ),
        ]
    );
    assert_eq!(blocks(&context.messages[3])[0].1, "continue my request");
    assert_eq!(blocks(&context.messages[3])[1].1, "(cited as u2)");
}

/// A shown proposal the person's words acted on, then Nika's note (the run failed) and the
/// model's answer: nothing waits on the proposal any more. The person's next line is a line of
/// its own, last in the request, and the proposal's call reads as what its tool replied (live
/// gauntlet, 2026-10-16: the line was read as the call's answer, the request ended on the model's
/// message, and Claude Opus 5.5 refused it as an assistant prefill).
#[test]
fn a_line_after_a_note_the_model_answered_is_its_own_message() {
    let (mut tree, mut lines) = open();
    say(&mut tree, &mut lines, "set the ceiling at 0.001 and run it");
    add(&mut tree, &mut lines, message(vec![call("c1", "propose")]));
    let authorized = "the person's words authorize it: the Session saves it, then asks its run";
    add(
        &mut tree,
        &mut lines,
        EntryKind::Parked {
            call: "c1".into(),
            name: "propose".into(),
            reply: ToolReply::ends_turn(authorized),
        },
    );
    assert_eq!(tree.parked(), Some(("c1", "propose")));
    add(
        &mut tree,
        &mut lines,
        EntryKind::Note {
            text: "The run the person asked for failed; its facts follow.".into(),
        },
    );
    // The note's run is still under way: the proposal waits until the model answers.
    assert_eq!(tree.parked(), Some(("c1", "propose")));
    add(
        &mut tree,
        &mut lines,
        EntryKind::Assistant {
            content: vec![text("The ceiling stopped it, as you wanted.")],
            stop: StopReason::EndTurn,
            usage: None,
            model: None,
        },
    );
    assert_eq!(tree.parked(), None);
    reply(&mut tree, &mut lines, "ok, put it back at 0.10", None);
    let replayed = Tree::replay(&file(&lines)).unwrap();
    let messages = replayed.context().messages;
    let last = messages.last().unwrap();
    assert_eq!(last.role, Role::User);
    assert_eq!(blocks(last)[0].1, "ok, put it back at 0.10");
    assert_eq!(
        blocks(&messages[2]),
        vec![("result", authorized.into(), "c1".into(), false)]
    );
}

/// The person's `text`, naming the call it `answers`.
fn reply(tree: &mut Tree, lines: &mut Vec<String>, text: &str, answers: Option<&str>) {
    let answers = answers.map(str::to_owned);
    (tree.append_user(text, answers, None, 2_000, |l| Store::append(lines, l))).unwrap();
}

/// One `ask` call `c1` of two questions answered by `u2` while it waited, the author's words
/// ending that turn, then `u3` replying to the call's other question after it stopped waiting,
/// recorded under `late`, and a later step of the run.
fn grouped(late: Option<&str>) -> Tree {
    let (mut tree, mut lines) = open();
    say(&mut tree, &mut lines, "écris un court message");
    add(&mut tree, &mut lines, message(vec![call("c1", "ask")]));
    let asked = json!({"asked": [{"key": "a", "id": "q1"}, {"key": "b", "id": "q2"}]});
    let parked = EntryKind::Parked {
        call: "c1".to_owned(),
        name: "ask".to_owned(),
        reply: ToolReply::ends_turn(asked.to_string()),
    };
    add(&mut tree, &mut lines, parked);
    reply(&mut tree, &mut lines, "Oui", Some("c1"));
    let ended = EntryKind::Assistant {
        content: vec![text("C'est noté.")],
        stop: StopReason::EndTurn,
        usage: None,
        model: None,
    };
    add(&mut tree, &mut lines, ended);
    reply(&mut tree, &mut lines, "bonjour.nika", late);
    let wrote = message(vec![text("Voici."), call("c2", "candidate_write")]);
    add(&mut tree, &mut lines, wrote);
    let written = result("c2", "candidate_write", ToolReply::ok("written"));
    add(&mut tree, &mut lines, written);
    tree
}

/// A person's line naming a call an earlier line already answered (a reply to another question
/// of that call's group, given after it stopped waiting) keeps naming it, yet everything a model
/// reads is what an ordinary line gives: the conversation, an agent's transcript, a fold's
/// request and where a fold may cut. The first line naming the call stays its answer.
#[test]
fn a_late_reply_reads_as_an_ordinary_line() {
    let (late, plain) = (grouped(Some("c1")), grouped(None));
    let named: Vec<Option<String>> = (late.branch().iter())
        .filter_map(|entry| match &entry.kind {
            EntryKind::User { answers, .. } => Some(answers.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(named, [None, Some("c1".into()), Some("c1".into())]);
    assert_eq!(late.parked(), None);
    let context = late.context();
    assert_eq!(format!("{context:?}"), format!("{:?}", plain.context()));
    let answered: Vec<_> = (context.messages.iter())
        .flat_map(blocks)
        .filter(|(kind, _, call, _)| *kind == "result" && call == "c1")
        .collect();
    assert_eq!(answered.len(), 1, "{answered:?}");
    assert_eq!(answered[0].1, "The person answered, cited as u2:\nOui");
    assert_eq!(
        crate::run::transcript(&late),
        crate::run::transcript(&plain)
    );
    for keep in [0, 1, 40, 100_000] {
        let cut = crate::compact::cut(&late, keep);
        assert_eq!(cut, crate::compact::cut(&plain, keep), "{keep}");
    }
    let first_kept = EntryId("e7".into());
    let folded = |tree: &Tree| format!("{:?}", crate::compact::request(tree, &first_kept, None));
    assert_eq!(folded(&late), folded(&plain));
}
