// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use nika_session_agent::Store;
use nika_session_change::tools::ToolCall;
use serde_json::json;

use super::store::TREE_FILE;
use super::*;

/// A conversation's tree kept beside the history is read back whole, its instructions and the
/// person's citations with it; a damaged one is reported, never reset or replaced.
#[test]
fn a_kept_tree_is_read_back_and_a_damaged_one_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let held = || OwnedDir::open(dir.path());
    let mut store = TreeFile::home(held().unwrap());
    let mut tree = start_tree(&mut store, dir.path(), "vllm/oux-author").unwrap();
    let said = tree.append_user("un digest des news", None, None, 2, |line| {
        Store::append(&mut store, line)
    });
    assert_eq!(said.unwrap().1, "u1");

    let driver = Driver::resumed(held(), None);
    assert!(driver.damaged.is_none());
    let resumed = driver.tree.as_ref().unwrap();
    assert_eq!(resumed.next_cite(), "u2");
    let system = resumed.context().system.unwrap();
    assert!(
        system.starts_with(SYSTEM) && system.contains("vllm/oux-author"),
        "{system}"
    );
    let citations = driver.store.citations();
    let cited = citations.lock().unwrap().get("u1").map(|c| c.text.clone());
    assert_eq!(cited.as_deref(), Some("un digest des news"));

    let kept = std::fs::read_to_string(dir.path().join(TREE_FILE)).unwrap();
    std::fs::write(dir.path().join(TREE_FILE), format!("{kept}not a line\n")).unwrap();
    let damaged = Driver::resumed(held(), None);
    assert!(damaged.tree.is_none() && damaged.damaged.is_some());
    assert!(
        std::fs::read_to_string(dir.path().join(TREE_FILE))
            .unwrap()
            .ends_with("not a line\n")
    );
}

/// What a history kept of a conversation resumes as evidence: its values and their provenance,
/// never a question; a value this engine does not read resumes nothing.
#[test]
fn kept_evidence_resumes_without_a_question() {
    let dir = tempfile::tempdir().unwrap();
    let binding = json!({"role": "read_source", "value": "https://news.ycombinator.com",
        "provenance": {"kind": "offered", "message": "u2", "question": "plan", "option": "recommended"}});
    let kept = json!({"version": 1, "request": 0, "since": 0, "candidate": null,
        "bindings": [binding], "accepted": [binding], "delegations": []});
    let driver = Driver::resumed(OwnedDir::open(dir.path()), Some(&kept));
    assert_eq!(driver.toolbox.with(|c| c.bindings().len()), 1);
    assert!(driver.open().is_empty() && driver.tree.is_none());
    let unread = Driver::resumed(OwnedDir::open(dir.path()), Some(&json!({"version": 7})));
    assert!(unread.toolbox.with(|c| c.bindings().is_empty()));
}

/// The tools live as long as the conversation, and between turns no call reaches a capability:
/// an ACP agent's call that arrives after its turn is refused, and nothing is asked or written.
#[test]
fn a_call_between_turns_reaches_no_capability() {
    let toolbox = Toolbox::new(Conversation::default(), Arc::default());
    let asked = json!({"questions": [{"key": "out", "question": "Which file?"}]});
    for call in [
        ToolCall::new("read", json!({"path": "a.md"})),
        ToolCall::new("ask", asked),
    ] {
        let reply = toolbox.call(call);
        assert!(
            reply.is_error && reply.text.starts_with("No turn is under way"),
            "{reply:?}"
        );
    }
    assert!(toolbox.with(|c| c.questions().is_empty()));
    assert_eq!(toolbox.end(), Decided::default());
}

/// A priced route lends the conversation its tariff's context window; a route no tariff prices,
/// or no route at all, lends none.
#[test]
fn a_priced_route_lends_the_conversation_its_context_window() {
    let window = super::route_window("deepseek/deepseek-v4-pro").expect("a priced route");
    let tariff = nika_catalog::admission::InferenceTariff::new(
        "deepseek",
        "deepseek-v4-pro",
        "https://api.deepseek.com/v1/chat/completions",
    );
    let tokens = tariff.map(|tariff| tariff.context_tokens);
    assert_eq!(Some(window.tokens), tokens);
    assert!(window.reserve > 0 && window.reserve < window.tokens);
    assert!(super::route_window("ollama/llama3.2").is_none());
    assert!(super::route_window("no-route").is_none());
}

/// The words of `resolutions` go out once per call, in `candidate_write`'s schema;
/// `candidate_edit` and `compose` carry its structure alone.
#[test]
fn the_resolution_words_are_sent_once() {
    let defs = tools::definitions();
    let sent: String = defs.iter().map(|t| t.input_schema.to_string()).collect();
    let kinds = sent
        .matches("named: a public source the person named")
        .count();
    assert_eq!(kinds, 1, "the words once");
    let edit = defs.iter().find(|t| t.name == "candidate_edit");
    let rows = edit.map(|t| t.input_schema["properties"]["resolutions"].clone());
    let required = rows.map(|rows| rows["items"]["required"].clone());
    assert_eq!(required, Some(json!(["value", "kind", "role", "message"])));
}

/// The author reads the language and an index up front, never the compiler's role or answer.
#[test]
fn the_instructions_carry_the_language_and_an_index() {
    let told = instructions("deepseek/deepseek-flash");
    assert!(told.contains("# The language in one page"), "the language");
    assert!(told.contains("# Output conventions"), "its conventions");
    assert!(
        !told.contains("# Your answer"),
        "never the compiler's answer"
    );
    let keys = told
        .lines()
        .find_map(|line| line.strip_prefix("- schema keys: "));
    let envelope = "const, inputs, model, nika, outputs, permits, run, secrets, tasks";
    assert_eq!(keys, Some(envelope), "the schema keys");
    assert!(index().len() < 3_000, "the index stays short");
}

/// The author never needs to ask the person how a workflow writes a file: its instructions show
/// the language's own write, with its arguments.
#[test]
fn the_instructions_show_how_a_workflow_writes_a_file() {
    let told = instructions("deepseek/deepseek-flash");
    assert!(
        told.contains("tool: \"nika:write\", args: { path:"),
        "the write"
    );
    assert!(
        told.contains("how to write something in a workflow"),
        "never asked"
    );
}

/// The doubt a ready verdict states, read from its outcome for the proposal to state.
#[test]
fn a_ready_verdict_keeps_its_stated_doubt() {
    let mut out = nika_compile::initial();
    let said = "The verifier doubted that this workflow carries the request as a whole";
    let applied = nika_compile::DiagnosticKind::Applied;
    nika_compile::finding(&mut out, applied, "verify_doubt", said);
    assert_eq!(desk::stated_doubt(&out).as_deref(), Some(said));
    assert_eq!(desk::stated_doubt(&nika_compile::initial()), None);
}
