// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A new workflow saved at the file the person's request names, whatever `nika:` its author
//! writes: created there, replacing a file already there only on the person's ordinary consent,
//! a folder the request gives kept, a component it reads through its `const:` untouched.

use serde_json::Value;

use crate::scenarios::{BASE, COUNT};
use crate::{is_workflow, open_questions, outcome_kind, reached_the_author, run, work};

/// What the author reads when the person replies to an earlier question while the proposal waits:
/// that proposal's call closes as no longer waiting as it is.
const ANSWERED_FIRST: &str = "before consenting, the person replies, by its identity";
/// What the author reads when the person replies to an earlier question once the consent door
/// settled the proposal.
const SETTLED: &str = "the Session's consent door settled it";

/// What a step's outcome says.
fn said(step: &Value) -> &str {
    step["outcome"]["text"].as_str().unwrap_or_default()
}

/// The person asked for `comptage.nika`; the author named the workflow `count-lines`. The
/// proposal creates `comptage.nika`, the yes saves it there, and no `count-lines.nika` appears.
#[test]
fn a_new_workflow_is_saved_under_the_file_its_request_names() {
    let journey = run("requested_file_created");
    let proposed = journey.step(0);
    assert_eq!(outcome_kind(proposed), "proposal", "{proposed:#}");
    assert!(
        said(proposed).contains("crée `comptage.nika`"),
        "{proposed:#}"
    );
    let yes = journey.step(1);
    assert_eq!(work(yes)["saved"]["workflow"], "comptage.nika", "{yes:#}");
    let files = journey.report["files"].as_array().unwrap();
    assert!(
        files.iter().any(|f| f.as_str() == Some("comptage.nika")),
        "{files:?}"
    );
    assert!(
        !files.iter().any(|f| f.as_str() == Some("count-lines.nika")),
        "{files:?}"
    );
}

/// A workflow already at the name the request gives is replaced only on the person's ordinary
/// consent: the proposal says it replaces that file whole, over its witness, and the yes writes
/// the proposed bytes there.
#[test]
fn a_named_file_already_there_is_replaced_only_on_the_ordinary_consent() {
    let journey = run("requested_file_occupied");
    let proposed = journey.step(0);
    assert_eq!(outcome_kind(proposed), "proposal", "{proposed:#}");
    assert!(
        said(proposed).contains("remplace `comptage.nika` en entier"),
        "{proposed:#}"
    );
    let yes = journey.step(1);
    assert_eq!(work(yes)["saved"]["workflow"], "comptage.nika", "{yes:#}");
    let saved = journey.dir.path().join("project/comptage.nika");
    assert_eq!(std::fs::read_to_string(saved).unwrap(), COUNT);
}

/// A folder the request gives is kept: the workflow is created and saved under it.
#[test]
fn a_folder_the_request_gives_is_kept() {
    let journey = run("requested_file_nested");
    let proposed = journey.step(0);
    assert!(
        said(proposed).contains("crée `workflows/comptage.nika`"),
        "{proposed:#}"
    );
    let yes = journey.step(1);
    assert_eq!(
        work(yes)["saved"]["workflow"],
        "workflows/comptage.nika",
        "{yes:#}"
    );
}

/// A component the workflow reads through its `const:` is never the file it is saved as: with no
/// file named for the workflow, the proposal creates `count-lines.nika`; naming `comptage.nika`,
/// it creates that file. Either way the yes saves it there and the component keeps its bytes.
#[test]
fn a_component_read_through_a_const_keeps_its_bytes() {
    for (name, target) in [
        ("component_read", "count-lines.nika"),
        ("component_read_saved", "comptage.nika"),
    ] {
        let journey = run(name);
        let proposed = journey.step(0);
        assert_eq!(outcome_kind(proposed), "proposal", "{name}: {proposed:#}");
        let creates = format!("crée `{target}`");
        assert!(said(proposed).contains(&creates), "{name}: {proposed:#}");
        let yes = journey.step(1);
        assert_eq!(work(yes)["saved"]["workflow"], target, "{name}: {yes:#}");
        let component = journey.dir.path().join("project/components/base.nika");
        assert_eq!(std::fs::read_to_string(component).unwrap(), BASE, "{name}");
    }
}

/// The person's reply to the author's question about the text is business content that looks like
/// a workflow file (`bonjour.nika`): it is that question's answer, never the request naming the
/// workflow's file, so the workflow is saved where its `nika:` puts it. A request that names the
/// same file for the workflow still saves it there.
#[test]
fn a_reply_to_a_question_never_names_the_saved_workflow() {
    let journey = run("requested_file_answered_content");
    assert_eq!(outcome_kind(journey.step(0)), "question");
    let proposed = journey.step(1);
    assert_eq!(outcome_kind(proposed), "proposal", "{proposed:#}");
    assert!(
        said(proposed).contains("crée `message.nika`"),
        "{proposed:#}"
    );
    let yes = journey.step(2);
    assert_eq!(work(yes)["saved"]["workflow"], "message.nika", "{yes:#}");
    let files = journey.report["files"].as_array().unwrap();
    assert!(
        !files.iter().any(|f| f.as_str() == Some("bonjour.nika")),
        "{files:?}"
    );
    let named = run("requested_file_answered_named");
    let yes = named.step(2);
    assert_eq!(work(yes)["saved"]["workflow"], "bonjour.nika", "{yes:#}");
}

/// The same reply, which the author never binds, is still that question's answer: the workflow is
/// saved where its `nika:` puts it, never under the reply.
#[test]
fn an_unbound_reply_never_names_the_saved_workflow() {
    let journey = run("requested_file_unbound_content");
    let proposed = journey.step(1);
    assert_eq!(outcome_kind(proposed), "proposal", "{proposed:#}");
    assert!(
        said(proposed).contains("crée `message.nika`"),
        "{proposed:#}"
    );
    let yes = journey.step(2);
    assert_eq!(work(yes)["saved"]["workflow"], "message.nika", "{yes:#}");
}

/// The author asks two questions in one `ask`. The person picks the first by its identity, and the
/// author ends that turn in words, asking nothing more: the second question still waits under its
/// identity, and the first, consumed, answers nothing. The person's reply to the second, business
/// content the author never binds (`bonjour.nika`), is still that question's reply, never the
/// request naming the workflow's file: the workflow is proposed where its `nika:` puts it, and
/// again after the Session reopens. A request that names the file still names it both times.
#[test]
fn a_late_reply_to_a_grouped_question_never_names_the_saved_workflow() {
    for (name, target) in [
        ("requested_file_grouped_late_reply", "message.nika"),
        ("requested_file_grouped_late_named", "bonjour.nika"),
    ] {
        let journey = run(name);
        let asked = journey.step(0);
        let open = open_questions(asked);
        let keys: Vec<&str> = open.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(
            keys,
            ["message_replace", "message_text"],
            "{name}: {asked:#}"
        );
        let picked = journey.step(1);
        assert_eq!(outcome_kind(picked), "reply", "{name}: {picked:#}");
        assert_eq!(open_questions(picked), open[1..], "{name}: {picked:#}");
        let consumed = journey.step(2);
        assert_eq!(outcome_kind(consumed), "refusal", "{name}: {consumed:#}");
        assert!(
            matches!(
                consumed["outcome"]["class"].as_str(),
                Some("already_consumed" | "stale_revision")
            ),
            "{name}: {consumed:#}"
        );
        let creates = format!("crée `{target}`");
        for at in [3, 5] {
            let proposed = journey.step(at);
            assert_eq!(
                outcome_kind(proposed),
                "proposal",
                "{name} {at}: {proposed:#}"
            );
            assert!(
                said(proposed).contains(&creates),
                "{name} {at}: {proposed:#}"
            );
        }
    }
}

/// The author asks two questions in one `ask`. The person picks the first by its identity, and the
/// author proposes a draft while the second still waits: the draft does not show it, so that
/// question waits first and its identity answers it (ADR-153). Their reply, business content the
/// author never binds (`bonjour.nika`), is that question's reply, never the request naming the
/// workflow's file: the workflow is proposed where its `nika:` puts it. The person declines it, and
/// nothing is saved; the question, answered, takes no answer again; the workflow is proposed there
/// again after the Session reopens. A request that names the file still names it both times.
#[test]
fn a_reply_after_a_declined_proposal_never_names_the_saved_workflow() {
    for (name, target) in [
        ("requested_file_grouped_declined", "message.nika"),
        ("requested_file_grouped_declined_named", "bonjour.nika"),
    ] {
        let journey = run(name);
        let asked = journey.step(0);
        let open = open_questions(asked);
        let keys: Vec<&str> = open.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(
            keys,
            ["message_replace", "message_text"],
            "{name}: {asked:#}"
        );
        let draft = journey.step(1);
        assert_eq!(outcome_kind(draft), "proposal", "{name}: {draft:#}");
        let replied = journey.step(2);
        assert!(open_questions(replied).is_empty(), "{name}: {replied:#}");
        assert_eq!(outcome_kind(replied), "proposal", "{name}: {replied:#}");
        assert_ne!(
            replied["outcome"]["id"], draft["outcome"]["id"],
            "{name}: a fresh proposal carries the reply: {replied:#}"
        );
        let declined = journey.step(3);
        assert_ne!(outcome_kind(declined), "proposal", "{name}: {declined:#}");
        let again = journey.step(4);
        assert_eq!(outcome_kind(again), "refusal", "{name}: {again:#}");
        assert_eq!(
            again["outcome"]["class"], "already_consumed",
            "{name}: {again:#}"
        );
        let creates = format!("crée `{target}`");
        for at in [2, 6] {
            let proposed = journey.step(at);
            assert_eq!(
                outcome_kind(proposed),
                "proposal",
                "{name} {at}: {proposed:#}"
            );
            assert!(
                said(proposed).contains(&creates),
                "{name} {at}: {proposed:#}"
            );
        }
        let files = journey.report["files"].as_array().unwrap();
        assert!(
            !files.iter().any(|f| f.as_str().is_some_and(is_workflow)),
            "{name}: nothing was saved: {files:?}"
        );
        assert!(
            reached_the_author(&journey, ANSWERED_FIRST),
            "{name}: the draft's call closes as no longer waiting as it is"
        );
        assert!(
            !reached_the_author(&journey, SETTLED),
            "{name}: no consent door settled the draft"
        );
    }
}

/// The same two questions, the draft declined at its consent door while the text's question still
/// waits: nothing is saved. The person's reply by that question's identity, `bonjour.nika`, is
/// still that question's reply, never the request naming the workflow's file: the workflow is
/// proposed where its `nika:` puts it, and again after the Session reopens. A request that names
/// the file still names it both times.
#[test]
fn a_reply_once_the_draft_is_declined_never_names_the_saved_workflow() {
    for (name, target) in [
        ("requested_file_grouped_declined_first", "message.nika"),
        (
            "requested_file_grouped_declined_first_named",
            "bonjour.nika",
        ),
    ] {
        let journey = run(name);
        let draft = journey.step(1);
        assert_eq!(outcome_kind(draft), "proposal", "{name}: {draft:#}");
        let declined = journey.step(2);
        assert_ne!(outcome_kind(declined), "proposal", "{name}: {declined:#}");
        let open = open_questions(declined);
        let keys: Vec<&str> = open.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(keys, ["message_text"], "{name}: {declined:#}");
        let creates = format!("crée `{target}`");
        for at in [3, 5] {
            let proposed = journey.step(at);
            assert_eq!(
                outcome_kind(proposed),
                "proposal",
                "{name} {at}: {proposed:#}"
            );
            assert!(
                said(proposed).contains(&creates),
                "{name} {at}: {proposed:#}"
            );
        }
        let files = journey.report["files"].as_array().unwrap();
        assert!(
            !files.iter().any(|f| f.as_str().is_some_and(is_workflow)),
            "{name}: nothing was saved: {files:?}"
        );
        assert!(
            reached_the_author(&journey, SETTLED),
            "{name}: the draft's call closes as settled by its consent door"
        );
        assert!(
            !reached_the_author(&journey, ANSWERED_FIRST),
            "{name}: no reply came while the draft waited"
        );
    }
}
