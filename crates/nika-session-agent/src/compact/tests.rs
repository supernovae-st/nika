// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use nika_kernel::provider::{ContentBlock, StopReason, TokenUsage};
use nika_session_change::tools::ToolReply;
use serde_json::json;

use super::*;
use crate::run::Store;

struct Book {
    tree: Tree,
    lines: Vec<String>,
}

impl Book {
    fn new() -> Self {
        let mut lines = Vec::new();
        let tree = Tree::new("s", "p", 1, |l| Store::append(&mut lines, l)).unwrap();
        Self { tree, lines }
    }

    fn add(&mut self, kind: EntryKind) -> EntryId {
        let lines = &mut self.lines;
        self.tree
            .append(kind, 2, |l| Store::append(lines, l))
            .unwrap()
    }

    fn say(&mut self, words: &str, answers: Option<&str>) -> EntryId {
        let lines = &mut self.lines;
        let answers = answers.map(str::to_owned);
        let said = self
            .tree
            .append_user(words, answers, None, 2, |l| Store::append(lines, l));
        said.unwrap().0
    }

    fn reply(&mut self, words: &str, usage: Option<TokenUsage>) -> EntryId {
        self.add(EntryKind::Assistant {
            content: vec![ContentBlock::Text { text: words.into() }],
            stop: StopReason::EndTurn,
            usage: usage.map(Box::new),
            model: None,
        })
    }

    fn call(&mut self, id: &str, name: &str) -> EntryId {
        self.add(EntryKind::Assistant {
            content: vec![ContentBlock::ToolUse {
                id: id.into(),
                name: name.into(),
                input: json!({"path": "news.nika"}),
            }],
            stop: StopReason::ToolUse,
            usage: None,
            model: None,
        })
    }
}

#[test]
fn a_window_keeps_room_for_the_reply() {
    let window = Window::of(200_000);
    assert_eq!((window.reserve, window.keep), (16_384, 20_000));
    assert!(!window.exceeded_by(183_616));
    assert!(window.exceeded_by(183_617));
    let small = Window::of(8_000);
    assert_eq!((small.reserve, small.keep), (2_000, 2_000));
}

#[test]
fn the_size_is_the_last_report_and_what_came_after() {
    let mut book = Book::new();
    book.say("hello", None);
    assert_eq!(estimate(&book.tree), 8, "(5 + 24) / 4, rounded up");
    book.reply("hi", Some(TokenUsage::new(1_000, 200)));
    book.add(EntryKind::ToolResult {
        call: "c1".into(),
        name: "read".into(),
        reply: ToolReply::ok("x".repeat(400)),
    });
    assert_eq!(estimate(&book.tree), 1_300);
}

#[test]
fn after_a_compaction_a_report_from_before_it_no_longer_counts() {
    let mut book = Book::new();
    book.add(EntryKind::System {
        text: "x".repeat(40),
    });
    book.say("first", None);
    book.reply("a", Some(TokenUsage::new(90_000, 10_000)));
    let second = book.say("second", None);
    let lines = &mut book.lines;
    let summary = "y".repeat(80);
    let folded = book
        .tree
        .append_compaction(summary, second, 100_000, 3, |l| Store::append(lines, l));
    assert!(folded.is_ok());
    // system 10 + summary 20 + the kept line (6 + 24) / 4 = 8.
    assert_eq!(estimate(&book.tree), 38);
}

#[test]
fn the_cut_keeps_the_recent_tail_from_a_persons_line() {
    let mut book = Book::new();
    book.say(&"a".repeat(4_000), None);
    book.reply("first answer", None);
    let second = book.say(&"b".repeat(400), None);
    book.reply("second answer", None);
    let third = book.say("short", None);
    assert_eq!(cut(&book.tree, 50), Some(second.clone()));
    assert_eq!(cut(&book.tree, 0), Some(third));
    // A tail heavier than the whole branch still folds what the oldest line after the first
    // follows: a compaction is asked when room is needed.
    assert_eq!(cut(&book.tree, 1_000_000), Some(second));
}

#[test]
fn a_first_line_alone_is_never_folded() {
    let mut book = Book::new();
    book.add(EntryKind::System {
        text: "instructions".into(),
    });
    book.say(&"a".repeat(4_000), None);
    book.reply("an answer", None);
    assert_eq!(cut(&book.tree, 0), None);
}

#[test]
fn an_answer_to_a_waiting_call_is_never_where_a_cut_falls() {
    let mut book = Book::new();
    book.say("make a digest", None);
    book.call("c1", "ask");
    book.add(EntryKind::Parked {
        call: "c1".into(),
        name: "ask".into(),
        reply: ToolReply::ends_turn("asked q1"),
    });
    book.say("Le Monde", Some("c1"));
    assert_eq!(cut(&book.tree, 0), None);
}

#[test]
fn the_summary_reads_the_folded_part_as_a_transcript() {
    let mut book = Book::new();
    book.add(EntryKind::System {
        text: "instructions".into(),
    });
    book.say("Récupère les news de Hacker News", None);
    book.call("c1", "read");
    book.add(EntryKind::ToolResult {
        call: "c1".into(),
        name: "read".into(),
        reply: ToolReply::error("no such file"),
    });
    book.reply("I will write it.", None);
    let kept = book.say("Écris-le dans news/", None);
    book.reply("Done.", None);

    let request = request(&book.tree, &kept, Some("the sources"));
    assert!(request.tools.is_empty());
    assert_eq!(request.system.as_deref(), Some(SYSTEM));
    assert_eq!(request.messages.len(), 1);
    let ContentBlock::Text { text } = &request.messages[0].content[0] else {
        panic!("one text block");
    };
    for line in [
        "[u1] The person: Récupère les news de Hacker News",
        "Assistant called read (c1) with {\"path\":\"news.nika\"}",
        "read (c1) replied a failure: no such file",
        "Assistant: I will write it.",
        INSTRUCTION,
        "Give particular attention to: the sources",
    ] {
        assert!(text.contains(line), "{line}\n---\n{text}");
    }
    assert!(!text.contains("Écris-le"), "a kept line is not folded");
    assert!(
        !text.contains("instructions"),
        "the instructions are not conversation"
    );
}
