// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use nika_kernel::provider::{ContentBlock, StopReason};
use nika_session_agent::Store;
use nika_session_agent::conversation::Citations;
use nika_session_change::tools::{ToolCall, ToolReply};
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

/// Two outputs: where the event's name starts is open (`payload out/event.json`), the report is
/// another output, and the summary is named outright.
const EVENTS: &str = "Lis events.csv, écris payload out/event.json pour chaque ligne, puis un \
                      rapport out/report.md et un résumé dans notes/events.md.";

/// A choice of one output among `paths` (offers `o0`, `o1`, …), the last one its default.
fn output_choice(key: &str, paths: &[&str]) -> Value {
    let options: Vec<Value> = (paths.iter().enumerate())
        .map(|(k, path)| {
            json!({"key": format!("o{k}"), "label": path, "recommended": k + 1 == paths.len(),
                "values": [{"role": "output_path", "value": path}]})
        })
        .collect();
    json!({"key": key, "role": "output_path", "question": format!("{key} ?"), "options": options})
}

/// Where the open name `payload out/event.json` starts: a choice of its readings.
fn event_choice() -> Value {
    output_choice(
        "const.event_path",
        &["payload out/event.json", "out/event.json"],
    )
}

/// Shows `choices` beside the candidate as the call `call`; the keys and identities shown, or why
/// the Session does not show them.
fn show(
    conversation: &mut Conversation,
    citations: &Citations,
    choices: &Value,
    call: &str,
) -> Result<Vec<Value>, String> {
    let asker = Arc::new(nika_session_change::outcome::Incarnation);
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let args = json!({"open": choices});
    let open = conversation.open_choices(
        citations,
        &args,
        Some(call),
        (&mut mint, &mut |_: &str| None),
    )?;
    let shown = (open.iter())
        .map(|q| json!({"key": q.key, "id": q.id.as_str()}))
        .collect();
    let proposed = conversation.propose(citations, &json!({}), ("S".to_owned(), open));
    assert_eq!(proposed, Ok(None));
    Ok(shown)
}

/// A history whose tree holds the request `u1`, a proposal `c1` showing `choice` and the person's
/// line `u2` answering it, as a Session records them; and the conversation that wrote `source`
/// with `rows`, showed the choice and bound `u2`'s pick of the offer `offer`.
fn answered_history(
    (history, project): (&Path, &Path),
    (source, rows): (&str, &[Value]),
    (choice, offer): (&Value, &str),
) -> Conversation {
    let mut store = TreeFile::home(OwnedDir::open(history).unwrap());
    let mut tree = start_tree(&mut store, project, "deepseek/deepseek-flash").unwrap();
    (tree.append_user(EVENTS, None, None, 2, |line| {
        Store::append(&mut store, line)
    }))
    .unwrap();
    let citations = store.citations().lock().unwrap().clone();
    let mut conversation = Conversation::default();
    let texts = (source.to_owned(), "s".to_owned());
    let written = conversation.write(&citations, texts, (rows, &[]), &mut |_| Ok(()));
    assert!(!written.is_error, "{written:?}");
    let shown = show(&mut conversation, &citations, &json!([choice]), "c1").unwrap();
    let call = ContentBlock::ToolUse {
        id: "c1".into(),
        name: "propose".into(),
        input: json!({"open": [choice]}),
    };
    let assistant = EntryKind::Assistant {
        content: vec![call],
        stop: StopReason::ToolUse,
        usage: None,
        model: None,
    };
    (tree.append(assistant, 3, |line| Store::append(&mut store, line))).unwrap();
    let said = json!({"revision": 1, "proposal": "shown", "open": shown}).to_string();
    let reply = ToolReply::ends_turn(said);
    let (call, name) = ("c1".to_owned(), "propose".to_owned());
    let parked = EntryKind::Parked { call, name, reply };
    (tree.append(parked, 4, |line| Store::append(&mut store, line))).unwrap();
    let answers = Some("c1".to_owned());
    let (_, cite) = (tree.append_user("celui-là", answers, None, 5, |line| {
        Store::append(&mut store, line)
    }))
    .unwrap();
    let key = choice["key"].as_str().unwrap().to_owned();
    let read = BTreeMap::from([(key, Pick::Offer(offer.to_owned()))]);
    conversation.answered_by(&cite, "celui-là", &read);
    conversation
}

/// The outputs bound, sorted: each with its question, the question and offer of its provenance,
/// its kind and the person's line.
fn outputs(conversation: &Conversation) -> Vec<String> {
    let mut outputs: Vec<String> = (conversation.bindings().iter())
        .filter(|binding| binding.role == crate::work::ValueRole::OutputPath)
        .map(|binding| {
            let provenance = &binding.provenance;
            let key = binding.key.as_deref().unwrap_or("-");
            let question = provenance.question.as_deref().unwrap_or("-");
            let offer = provenance.option.as_deref().unwrap_or("-");
            let (value, kind) = (&binding.value, provenance.kind);
            format!(
                "{value} · {key} · {question}/{offer} · {kind:?} · {}",
                provenance.message
            )
        })
        .collect();
    outputs.sort();
    outputs
}

/// After the person answered where an open name starts and the Session reopened, another output's
/// choice is still shown, for the history's own record of that question shows the answer settles
/// that name alone; both answers stay bound, and a choice offering that name again stays settled.
#[test]
fn a_reopened_open_names_answer_settles_no_other_output() {
    let (history, project) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let source = "nika: events\n# writes out/event.json and out/report.md\n";
    let dirs = (history.path(), project.path());
    let conversation = answered_history(dirs, (source, &[]), (&event_choice(), "o1"));
    let driver = Driver::resumed(OwnedDir::open(history.path()), Some(&conversation.kept()));
    assert!(driver.damaged.is_none() && driver.tree.is_some());
    let citations = driver.store.citations().lock().unwrap().clone();
    let report = output_choice("const.report_path", &["out/report.json", "out/report.md"]);
    let again = output_choice("const.event_file", &["out/event.ndjson", "out/event.json"]);
    driver.toolbox.with(|c| {
        let shown = show(c, &citations, &json!([report]), "c2");
        assert!(
            shown.as_ref().is_ok_and(|shown| shown.len() == 1),
            "{shown:?}"
        );
        let read = BTreeMap::from([("const.report_path".to_owned(), Pick::Offer("o0".into()))]);
        c.answered_by("u3", "le JSON", &read);
        let both = [
            "out/event.json · const.event_path · const.event_path/o1 · Offered · u2",
            "out/report.json · const.report_path · const.report_path/o0 · Offered · u3",
        ];
        assert_eq!(outputs(c), both);
        let refused = show(c, &citations, &json!([again]), "c3").unwrap_err();
        assert!(
            refused.contains("already settled by the person"),
            "{refused}"
        );
    });
}

/// After the person answered where an open name starts and the Session reopened, a revision that
/// drops the output they named beside it is still reported: that answer replaced no other output.
#[test]
fn a_reopened_open_names_answer_replaces_no_other_output() {
    let (history, project) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let source = "nika: events\n# writes out/event.json and notes/events.md\n";
    let summary = json!({"value": "notes/events.md", "kind": "named", "role": "output_path",
        "message": "u1", "excerpt": "dans notes/events.md"});
    let dirs = (history.path(), project.path());
    let conversation = answered_history(dirs, (source, &[summary]), (&event_choice(), "o1"));
    let driver = Driver::resumed(OwnedDir::open(history.path()), Some(&conversation.kept()));
    assert!(driver.damaged.is_none() && driver.tree.is_some());
    let citations = driver.store.citations().lock().unwrap().clone();
    driver.toolbox.with(|c| {
        let texts = (
            "nika: events\n# writes out/event.json\n".to_owned(),
            "s".to_owned(),
        );
        let written = c.write(&citations, texts, (&[], &[]), &mut |_| Ok(()));
        assert!(!written.is_error, "{written:?}");
        let dropped = c.dropped();
        assert!(
            dropped.len() == 1 && dropped[0].contains("notes/events.md"),
            "{dropped:?}"
        );
    });
}

/// A message workflow writing the text `bonjour.nika`, the person's reply to the author's question.
const MESSAGE: &str = r#"nika: message
permits:
  tools: ["nika:write"]
  fs:
    write: ["./out/message.txt"]
tasks:
  message:
    invoke: { tool: "nika:write", args: { path: "./out/message.txt", content: "bonjour.nika" } }
"#;

/// A history whose tree holds the request `u1`, the author's `ask` call `c1` for the message's text
/// and the person's line `u2` replying `reply`, which the author never binds; and the conversation
/// that asked it and recorded that line as the question's answer.
/// A history whose author asked « Quel texte ? » after `request`, the question still waiting.
fn parked_history(dirs: (&Path, &Path), request: &str) -> Conversation {
    asked_in(dirs, request, None)
}

fn asked_history(dirs: (&Path, &Path), request: &str, reply: &str) -> Conversation {
    asked_in(dirs, request, Some(reply))
}

/// A history whose author asked « Quel texte ? » after `request`, answered by `reply` when one
/// is given.
fn asked_in(
    (history, project): (&Path, &Path),
    request: &str,
    reply: Option<&str>,
) -> Conversation {
    let mut store = TreeFile::home(OwnedDir::open(history).unwrap());
    let mut tree = start_tree(&mut store, project, "deepseek/deepseek-flash").unwrap();
    (tree.append_user(request, None, None, 2, |line| {
        Store::append(&mut store, line)
    }))
    .unwrap();
    let citations = store.citations().lock().unwrap().clone();
    let mut conversation = Conversation::default();
    let input = json!({"questions": [{"key": "message_text", "question": "Quel texte ?",
        "free_text": true, "options": []}]});
    let asker = Arc::new(nika_session_change::outcome::Incarnation);
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let posed = conversation.ask(
        &citations,
        &input,
        Some("c1"),
        (&mut mint, &mut |_: &str| None),
    );
    assert!(posed.ends_turn, "{posed:?}");
    let call = ContentBlock::ToolUse {
        id: "c1".into(),
        name: "ask".into(),
        input,
    };
    let assistant = EntryKind::Assistant {
        content: vec![call],
        stop: StopReason::ToolUse,
        usage: None,
        model: None,
    };
    (tree.append(assistant, 3, |line| Store::append(&mut store, line))).unwrap();
    let (call, name) = ("c1".to_owned(), "ask".to_owned());
    let parked = EntryKind::Parked {
        call,
        name,
        reply: posed,
    };
    (tree.append(parked, 4, |line| Store::append(&mut store, line))).unwrap();
    let Some(reply) = reply else {
        return conversation;
    };
    let answers = Some("c1".to_owned());
    let (_, cite) = (tree.append_user(reply, answers, None, 5, |line| {
        Store::append(&mut store, line)
    }))
    .unwrap();
    // The host records the line as the parked `ask`'s reply, then reads it as its answer.
    conversation.replied_by(&cite);
    conversation.answered_by(&cite, reply, &BTreeMap::new());
    conversation
}

/// The person replied in their own words to a question Nika asked, and no value was bound from
/// that reply: it never names the saved workflow, before the Session reopens or after, for the
/// history's own record shows the line replied to that question. A request naming the file still
/// names it.
#[test]
fn a_reopened_reply_never_names_the_saved_workflow() {
    let plain = "écris un court message dans ./out/message.txt";
    let named = "écris un court message dans ./out/message.txt, puis enregistre le workflow dans \
                 bonjour.nika";
    for (request, expected) in [(plain, None), (named, Some("bonjour.nika"))] {
        let (history, project) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dirs = (history.path(), project.path());
        let conversation = asked_history(dirs, request, "bonjour.nika");
        let mut lines = Citations::default();
        lines.record(2, Some(("u1".into(), request.into())));
        lines.record(5, Some(("u2".into(), "bonjour.nika".into())));
        let live = desk::requested(&conversation, &lines, MESSAGE);
        assert_eq!(live.as_deref(), expected, "before the reopen: {request}");
        let driver = Driver::resumed(OwnedDir::open(history.path()), Some(&conversation.kept()));
        assert!(driver.damaged.is_none() && driver.tree.is_some());
        let citations = driver.store.citations().lock().unwrap().clone();
        let reopened = driver
            .toolbox
            .with(|c| desk::requested(c, &citations, MESSAGE));
        assert_eq!(reopened.as_deref(), expected, "after the reopen: {request}");
    }
}

/// A question the author asked owns the person's next line: a schedule it reads (« tous les
/// soirs à 20h ») answers that question, never activates the workflow saved before it (live
/// gauntlet wave 7 r08, 2026-10-19: after a cancel, the journal's « quand ? » answered with
/// « tous les soirs à 20h » opened the activation of the welcome example). With no question
/// waiting, the same line still activates the saved workflow.
#[test]
fn a_question_the_author_asked_owns_the_schedule_that_answers_it() {
    let (history, project) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::fs::write(project.path().join("saved.nika"), "nika: saved\n").unwrap();
    let mut session = SessionRuntime::open(
        project.path(),
        crate::runtime::tests::ready(
            crate::intelligence::IntelligenceKind::None,
            crate::intelligence::DataLocus::None,
        ),
        Box::new(crate::reasoner::NoReasoner),
    );
    session.last_workflow = Some(PathBuf::from("saved.nika"));
    let line = "tous les soirs à 20h";
    assert!(
        session.schedules_the_saved(line).is_some(),
        "nothing asked: the saved workflow's schedule"
    );
    assert!(session.agent_yields(line));
    let request = "écris chaque soir la date du jour dans ./journal.txt";
    let conversation = parked_history((history.path(), project.path()), request);
    let driver = Driver::resumed(OwnedDir::open(history.path()), Some(&conversation.kept()));
    assert!(driver.tree.as_ref().and_then(Tree::parked).is_some());
    session.agent = Some(driver);
    assert!(session.author_asks());
    assert_eq!(
        session.schedules_the_saved(line),
        None,
        "the question's answer"
    );
    assert!(!session.agent_yields(line), "the author reads its answer");
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
    let none = BTreeMap::new();
    assert_eq!(
        desk::landing(dir.path(), changed, (Some(&saved), &none), None).unwrap(),
        "saved.nika"
    );
    std::fs::write(dir.path().join("saved.nika"), changed).unwrap();
    assert!(
        desk::landing(dir.path(), first, (Some(&saved), &none), None)
            .unwrap_err()
            .contains("changed since")
    );
    let observed = BTreeMap::from([("saved.nika".to_owned(), Witness::of(changed.as_bytes()))]);
    assert_eq!(
        desk::landing(dir.path(), first, (Some(&saved), &observed), None).unwrap(),
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
        desk::landing(dir.path(), revision, (Some(&saved), &observed), None).unwrap(),
        "saved.nika"
    );
    let ambiguous = BTreeMap::from([
        ("a-reference.nika".to_owned(), witness.clone()),
        ("saved.nika".to_owned(), witness),
    ]);
    assert_eq!(
        desk::landing(dir.path(), revision, (None, &ambiguous), None).unwrap_err(),
        "several observed workflows have this name; identify the workflow to revise before proposing"
    );
    for path in ["a-reference.nika", "saved.nika"] {
        assert_eq!(
            std::fs::read_to_string(dir.path().join(path)).unwrap(),
            first
        );
    }
}

/// A request naming its workflow file: the new workflow lands at that file whatever its `nika:`,
/// and the proposal's change set writes it there: absent, created; present, replaced over its
/// witness for the person's consent. With no file named, it lands where its `nika:` puts it.
#[test]
fn a_new_workflow_lands_at_the_file_the_request_names() {
    let dir = tempfile::tempdir().unwrap();
    let source = "nika: bilan-ventes\npermits:\n  tools: [\"nika:write\"]\n  fs:\n    write: [\"out/bilan.md\"]\ntasks:\n  total:\n    invoke: { tool: \"nika:write\", args: { path: \"out/bilan.md\", content: \"ok\" } }\n";
    let request =
        "Prépare un bilan des ventes, puis crée et sauvegarde le workflow dans bilan.nika.";
    let requested = nika_compile_fidelity::fidelity::requested_workflow(request, source);
    assert_eq!(requested.as_deref(), Some("bilan.nika"));
    let none = BTreeMap::new();
    let at = desk::landing(dir.path(), source, (None, &none), requested.as_deref()).unwrap();
    assert_eq!(at, "bilan.nika");
    let created = ProjectChangeSet::workflow_at(dir.path(), "bilan", &at, source.to_owned());
    let created = created.unwrap();
    assert!(
        matches!(&created.changes[..], [crate::change::ProjectChange::CreateWorkflow { path, .. }] if path == std::path::Path::new("bilan.nika")),
        "{:?}",
        created.changes
    );
    let unnamed = desk::landing(dir.path(), source, (None, &none), None).unwrap();
    assert_eq!(unnamed, "bilan-ventes.nika");
    std::fs::write(dir.path().join("bilan.nika"), "nika: bilan\ntasks: {}\n").unwrap();
    let at = desk::landing(dir.path(), source, (None, &none), requested.as_deref()).unwrap();
    let replaced = ProjectChangeSet::workflow_at(dir.path(), "bilan", &at, source.to_owned());
    let replaced = replaced.unwrap();
    assert!(
        matches!(&replaced.changes[..], [crate::change::ProjectChange::UpdateWorkflow { path, .. }] if path == std::path::Path::new("bilan.nika")),
        "{:?}",
        replaced.changes
    );
}

#[path = "tests/recovery.rs"]
mod recovery;

/// The welcome's demo typed as the welcome says (live gauntlet, 2026-10-15: under Claude Sonnet
/// the conversation's author asked for a data file the demo does not need) is the compiler's
/// exact skeleton, as is the first lesson, and so is a line that only asks to see, try or run it
/// by its name (the same gauntlet: « montre-moi cette démo aggregate-by-key … » was shown in the
/// chat and could not be saved). A line that asks for more than the demo stays the author's.
#[test]
fn a_demo_asked_for_by_its_name_is_the_compilers_door() {
    for (line, name) in [
        ("aggregate-by-key", "aggregate-by-key"),
        ("hello", "hello"),
        ("montre-moi la démo aggregate-by-key", "aggregate-by-key"),
        (
            "Ok, montre-moi cette démo aggregate-by-key dont tu parles sur l'écran d'accueil.",
            "aggregate-by-key",
        ),
        ("run aggregate-by-key", "aggregate-by-key"),
        ("Try the aggregate-by-key demo!", "aggregate-by-key"),
        // By where the welcome shows it, not its name (live gauntlet r08, 2026-10-16).
        (
            "montre-moi l'exemple de l'écran d'accueil",
            crate::WELCOME_DEMO,
        ),
        (
            "Show me the demo from the welcome screen",
            crate::WELCOME_DEMO,
        ),
    ] {
        assert_eq!(demo_named(line).as_deref(), Some(name), "{line}");
    }
    for line in [
        "montre-moi un exemple de workflow pour l'accueil des nouveaux clients",
        "l'exemple de l'écran d'accueil avec ./data/ventes.csv",
        "welcome! can you show me an example of a csv report",
    ] {
        assert_eq!(demo_named(line), None, "{line}");
    }
    for line in [
        "aggregate by key",
        "Aggregate-By-Key",
        "",
        "montre-moi la démo",
        "fais comme aggregate-by-key avec ./data/ventes.csv",
        "montre-moi la démo aggregate-by-key avec 3 régions",
        "aggregate-by-key et hello, montre-moi les deux démos",
    ] {
        assert_eq!(demo_named(line), None, "{line}");
    }
}

/// The gauntlet's sentence asking for the welcome's demo reaches its skeleton with no model: the
/// currency is asked, its answer proposes the demo, and nothing is written before a yes.
#[test]
fn a_sentence_asking_for_the_demo_proposes_its_skeleton() {
    let root = tempfile::tempdir().expect("root");
    let census =
        crate::intelligence::IntelligenceCensus::new(vec![], vec!["deepseek".into()], vec![]);
    let preference = crate::intelligence::UserIntelligencePreference::new(
        crate::intelligence::IntelligenceKind::Api {
            provider: "deepseek".into(),
        },
        None,
    );
    let mut s = SessionRuntime::open_with(
        root.path(),
        census,
        &preference,
        None,
        Box::new(|_| Box::new(crate::reasoner::NoReasoner)),
    );
    s.enable_continuous_preparation();
    let asked =
        s.turn("Ok, montre-moi cette démo aggregate-by-key dont tu parles sur l'écran d'accueil.");
    assert!(
        matches!(asked, TurnOutcome::Question { .. }),
        "the demo's one open value is asked: {asked:?}"
    );
    let _ = s.submit("EUR", &s.waiting());
    assert!(s.pending_proposal().is_some(), "the demo is proposed");
    assert!(!root.path().join("aggregate-by-key.nika").exists());
}

/// A conversation that starts after the person already spoke (a request the round driver or an
/// unusable seat kept) begins with their lines, cited as theirs: the request and their last
/// lines, never a consent word or a command. Live gauntlet, 2026-10-15: after two unusable
/// seats, the third intelligence answered « Je n'ai pas trace … aucun échange avant ce message ».
#[test]
fn a_starting_conversation_carries_what_the_person_said_before_it_led() {
    let root = tempfile::tempdir().expect("root");
    let none = crate::intelligence::ResolvedSessionIntelligence::new(
        crate::intelligence::IntelligenceKind::None,
        None,
        crate::intelligence::DataLocus::None,
        false,
        None,
    );
    let mut s = SessionRuntime::open(root.path(), none, Box::new(crate::reasoner::NoReasoner));
    let mut driver = Driver::with_store(TreeFile::memory());
    s.carry_into(&mut driver);
    assert!(
        driver.carried.is_empty(),
        "nothing was said: nothing is carried"
    );
    let request = "Traduis docs/note-produit.md en anglais et résume-la en 5 points.";
    let later = "Tu te souviens de ce que je veux faire ?";
    s.intent.goal = Some(request.to_owned());
    for line in [request, "/intelligence", "oui", later] {
        s.recent.push((line.to_owned(), "(noted)".to_owned()));
    }
    s.carry_into(&mut driver);
    assert_eq!(driver.carried, [request, later]);
    // The new tree begins with them as the person's lines, the first with Nika's note.
    let started = start_tree(
        &mut driver.store,
        root.path(),
        "anthropic/claude-sonnet-5-5",
    );
    driver.tree = Some(started.expect("tree"));
    append_carried(&mut driver).expect("carried");
    let context = driver.tree.as_ref().expect("tree").context();
    let said: Vec<String> = (context.messages.iter())
        .flat_map(|message| message.content.iter())
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(said.iter().any(|t| t == request), "{said:?}");
    assert!(said.iter().any(|t| t == later), "{said:?}");
    assert!(said.iter().any(|t| t == "(cited as u1)"), "{said:?}");
    assert_eq!(
        said.iter().filter(|t| t.contains(CARRIED_NOTE)).count(),
        1,
        "{said:?}"
    );
    // A conversation already under way carries nothing more.
    s.carry_into(&mut driver);
    assert!(driver.carried.is_empty());
}

/// A line that only asks to go on, its courtesy and punctuation aside, is read as continuing;
/// a line that also asks something, a consent and a change are not (live gauntlet r07 B5).
#[test]
fn only_a_line_asking_to_go_on_continues() {
    for line in [
        "On continue là où on en était",
        "continue",
        "on reprend stp",
        "Ok, on continue !",
        "go on please",
        "let\u{2019}s continue where we left off",
    ] {
        assert!(SessionRuntime::is_continue(line), "{line}");
    }
    for line in [
        "oui",
        "continue avec un plafond de 0,10 $",
        "on continue mais écris dans ./out/x.md",
        "continue to run it",
        "",
    ] {
        assert!(!SessionRuntime::is_continue(line), "{line}");
    }
}
