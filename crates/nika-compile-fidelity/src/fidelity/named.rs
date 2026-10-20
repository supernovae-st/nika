// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The workflow file a request names for the workflow it asks for.

use std::collections::BTreeMap;

use nika_check_analyzer::static_args::{ConstStrings, judgeable_arg};
use nika_compile_reader::hot;
use nika_schema::raw::{RawAction, RawWorkflow};
use nika_schema::{FileId, ParseMode};

/// The workflow file the request names for the workflow it asks for: the one file of Nika's own
/// format it states (`hot::stated_workflows`: `bilan.nika`) that the candidate neither calls as a
/// child nor names as a task's file (a bare `${{ const.<name> }}` read as its declared value), as
/// a path inside the project (no `..`, not rooted). A host saves a new workflow there, whatever
/// its `nika:`. `None` when the request names none, or two (one file written with and without its
/// leading `./` is one), or only the candidate's children and files, or a path outside the
/// project, or when the candidate does not parse.
#[must_use]
pub fn requested_workflow(intent: &str, candidate: &str) -> Option<String> {
    let workflow = nika_schema::parse(candidate, FileId::new(0), ParseMode::Strict).ok()?;
    let touched = touched(&workflow);
    let mut named: Vec<String> = Vec::new();
    for path in hot::stated_workflows(intent) {
        let path = path.trim_start_matches("./").to_owned();
        if !touched.contains(&path) && !named.contains(&path) {
            named.push(path);
        }
    }
    let [path] = named.as_slice() else {
        return None;
    };
    let inside = !path.starts_with(['/', '~']) && !path.split('/').any(|part| part == "..");
    inside.then(|| path.clone())
}

/// [`requested_workflow`], read from the request's words without the person's replies to a
/// Session's questions (`answers`, as [`super::laws_answered`] takes them): a reply located by its
/// citation that reads as typed is that question's answer, business content or a choice, never the
/// request naming the workflow's file. Every other line keeps its words, the request's own and a
/// later correction included.
#[must_use]
pub fn requested_workflow_answered(
    intent: &str,
    candidate: &str,
    answers: &BTreeMap<String, String>,
) -> Option<String> {
    requested_workflow(
        &super::answered::without_answer_replies(intent, answers),
        candidate,
    )
}

/// The files the candidate's own tasks use, without a leading `./`: each child workflow it calls
/// and each `path` a task's arguments give, a bare `${{ const.<name> }}` read as the value its
/// `const:` declares, as the Check reads it ([`ConstStrings`]). A const no task uses is no file.
fn touched(workflow: &RawWorkflow) -> Vec<String> {
    let consts = ConstStrings::of(workflow);
    let mut paths = Vec::new();
    for task in &workflow.tasks {
        let RawAction::Invoke(invoke) = &task.value.action else {
            continue;
        };
        let child = invoke
            .workflow()
            .and_then(|target| match target.value.as_str() {
                held if held.contains("${{") => consts.resolve(held).map(str::to_owned),
                literal => Some(literal.to_owned()),
            });
        let file = judgeable_arg(&consts, invoke, "path");
        let used = [child, file].into_iter().flatten();
        paths.extend(used.map(|path| path.trim_start_matches("./").to_owned()));
    }
    paths
}
