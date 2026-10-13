// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use nika_session_agent::Store;
use nika_session_change::tools::ToolCall;
use serde_json::json;
use std::path::PathBuf;

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

/// Reopening keeps the saved target as context. It neither replays a run nor turns a request
/// to rerun unchanged bytes into a new proposal that asks for another Save.
#[test]
fn a_restored_agent_remembers_the_saved_target_without_replaying_it() {
    let project = tempfile::tempdir().unwrap();
    let history = tempfile::tempdir().unwrap();
    let source = "nika: saved\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  value:\n    invoke: { tool: \"nika:jq\", args: { input: 7, expression: \".\" } }\n";
    std::fs::write(project.path().join("saved.nika"), source).unwrap();
    let mut store = TreeFile::home(OwnedDir::open(history.path()).unwrap());
    let _tree = start_tree(&mut store, project.path(), "deepseek/deepseek-flash").unwrap();
    let mut conversation = Conversation::default();
    conversation.saved_at("saved.nika", source);
    let mut session = SessionRuntime::open(
        project.path(),
        crate::runtime::tests::ready(
            crate::intelligence::IntelligenceKind::None,
            crate::intelligence::DataLocus::None,
        ),
        Box::new(crate::reasoner::NoReasoner),
    );
    session.agent = Some(Driver::with_store(TreeFile::memory()));
    session.resume_agent(OwnedDir::open(history.path()), Some(&conversation.kept()));
    assert_eq!(session.last_workflow, Some(PathBuf::from("saved.nika")));
    assert_eq!(session.consented, Some(PathBuf::from("saved.nika")));
    assert_eq!(session.lifecycle().saved, crate::lifecycle::Stage::Done);
    assert_eq!(
        session.lifecycle().checked,
        crate::lifecycle::Stage::Pending
    );
    assert!(session.requested_run.is_none());
    assert!(session.pending.is_none());
    assert_eq!(session.last_check_clean, None);
    let outcome = session.run_turn("run it");
    assert!(
        matches!(outcome, Some(TurnOutcome::RunRequested { ref run, .. })
        if run.workflow == Path::new("saved.nika")),
        "{outcome:?}"
    );
    assert!(session.pending.is_none());
    assert_eq!(
        std::fs::read_to_string(project.path().join("saved.nika")).unwrap(),
        source
    );
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

/// The bytes a verification judged are kept once, redacted, by the digest its record carries.
#[test]
fn the_bytes_a_verification_judged_are_kept_by_their_digest() {
    let home = tempfile::tempdir().expect("a home");
    let dir = nika_fs::OwnedDir::create(home.path(), &["h"]).expect("the history");
    let source = "nika: news\n# key sk-live-ABCDEFGH12345678\n";
    desk::keep_judged(&dir, source);
    desk::keep_judged(&dir, source);
    let name = format!("{}.nika", nika_compile::surface::sha256(source));
    let verified = dir.open_below(&["verified"]).expect("kept under verified/");
    let kept = verified.read(&name).expect("kept by its digest");
    assert!(kept.starts_with("nika: news\n"), "the judged bytes");
    assert!(!kept.contains("ABCDEFGH12345678"), "redacted");
    assert_eq!(verified.names().expect("listed").len(), 1, "kept once");
}

/// The author is told to declare a documented `low` effort for writing tasks, and the `models`
/// tool's offers carry the efforts the catalog documents for each model.
#[test]
fn the_author_sees_the_efforts_a_run_model_documents() {
    let told = instructions("deepseek/deepseek-flash");
    assert!(
        told.contains("declare `run: { reasoning: { effort: low } }`"),
        "the rule"
    );
    let caps = desk::model_caps("deepseek/deepseek-v4-flash").expect("a catalogued model");
    assert!(
        caps["reasoning_efforts"].is_array(),
        "its efforts, as the catalog lists them"
    );
    assert!(
        desk::model_caps("mock/mock-default").is_none(),
        "a mock documents none"
    );
}

/// Repairs keep the saved destination, but a concurrently edited workflow must be read again.
#[test]
fn a_saved_revision_lands_in_place_and_protects_changed_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let first = "nika: saved\ntasks: {}\n";
    let changed = "nika: saved\nconst: {changed: true}\ntasks: {}\n";
    std::fs::write(dir.path().join("saved.nika"), first).unwrap();
    let saved = ("saved.nika".to_owned(), Witness::of(first.as_bytes()).0);
    assert_eq!(
        desk::landing(dir.path(), changed, Some(&saved), &BTreeMap::new()).unwrap(),
        "saved.nika"
    );
    std::fs::write(dir.path().join("saved.nika"), changed).unwrap();
    assert!(
        desk::landing(dir.path(), first, Some(&saved), &BTreeMap::new())
            .unwrap_err()
            .contains("changed since")
    );
    let observed = BTreeMap::from([("saved.nika".to_owned(), Witness::of(changed.as_bytes()))]);
    assert_eq!(
        desk::landing(dir.path(), first, Some(&saved), &observed).unwrap(),
        "saved.nika"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("saved.nika")).unwrap(),
        changed
    );
}

#[test]
fn a_saved_target_takes_precedence_over_an_observed_copy() {
    let dir = tempfile::tempdir().unwrap();
    let first = "nika: saved\ntasks: {}\n";
    let revision = "nika: saved\nconst: {changed: true}\ntasks: {}\n";
    for path in ["a-reference.nika", "saved.nika"] {
        std::fs::write(dir.path().join(path), first).unwrap();
    }
    let witness = Witness::of(first.as_bytes());
    let saved = ("saved.nika".to_owned(), witness.0.clone());
    let observed = BTreeMap::from([("a-reference.nika".to_owned(), witness.clone())]);
    assert_eq!(
        desk::landing(dir.path(), revision, Some(&saved), &observed).unwrap(),
        "saved.nika"
    );
    let ambiguous = BTreeMap::from([
        ("a-reference.nika".to_owned(), witness.clone()),
        ("saved.nika".to_owned(), witness),
    ]);
    assert_eq!(
        desk::landing(dir.path(), revision, None, &ambiguous).unwrap_err(),
        "several observed workflows have this name; identify the workflow to revise before proposing"
    );
    for path in ["a-reference.nika", "saved.nika"] {
        assert_eq!(
            std::fs::read_to_string(dir.path().join(path)).unwrap(),
            first
        );
    }
}

#[path = "tests/recovery.rs"]
mod recovery;
