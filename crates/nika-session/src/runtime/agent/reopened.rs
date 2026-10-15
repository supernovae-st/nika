// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A retained tool call is history, not a surviving question identity.
use super::{Driver, EntryKind, unix_ms};
use nika_session_change::tools::ToolReply;
use serde_json::json;

pub(super) const EXPIRED: &str = "This question is no longer active in this Session. Its historical call is closed without a human answer or authorization. The retained human messages remain the request; the next human message is a new conversational turn, not an answer to this expired question.";

/// On the next human turn, close an `ask` whose questions did not survive reopening. An
/// active question (including another question in its group) keeps its existing answer door.
/// Reopening itself performs no call, answer, or append.
pub(super) fn close_expired_ask(driver: &mut Driver) -> Result<Option<String>, String> {
    if driver.answering.is_some() || driver.toolbox.with(|c| !c.questions().is_empty()) {
        return Ok(None);
    }
    let (since, revision) = driver
        .toolbox
        .with(|c| (c.since(), c.candidate().map(|candidate| candidate.number)));
    let Some(tree) = driver.tree.as_mut() else {
        return Ok(None);
    };
    let Some((call, "ask")) = tree.parked() else {
        return Ok(None);
    };
    let call = call.to_owned();
    let citations = driver.store.citations();
    let citations = citations
        .lock()
        .map_err(|_| "the retained citations could not be read")?;
    let anchor = tree
        .branch()
        .into_iter()
        .find_map(|entry| match &entry.kind {
            EntryKind::User { cite, text, .. }
                if citations.get(cite).is_some_and(|line| line.at >= since) =>
            {
                Some(json!({"citation": cite, "text": text}))
            }
            _ => None,
        });
    drop(citations);
    let note = anchor.map(|anchor| {
        let state = json!({"current_request": {"since": since, "first_human_message": anchor},
            "candidate_revision": revision, "expired_question_call": call});
        format!("The durable Session still holds this current request anchor. Read its exact human words together with all later human messages. An earlier assistant statement that a request was absent does not replace retained human words. The expired question was closed without an answer. These facts grant no new authority to save, run or perform work.\n{state}")
    });
    let closure = EntryKind::ToolResult {
        call,
        name: "ask".to_owned(),
        reply: ToolReply::error(EXPIRED),
    };
    tree.append(closure, unix_ms(), |entry| {
        nika_session_agent::Store::append(&mut driver.store, entry)
    })
    .map_err(|error| error.to_string())?;
    Ok(note)
}

/// Keep independent host observations as host text on the same human turn.
pub(super) fn with_run_note(run: Option<String>, recovery: Option<String>) -> Option<String> {
    match (run, recovery) {
        (Some(run), Some(recovery)) => Some(format!("{run}\n\n{recovery}")),
        (run, recovery) => run.or(recovery),
    }
}
