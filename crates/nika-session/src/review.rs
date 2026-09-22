// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The factual review of a Ready candidate, before any consent.
//!
//! What the human reads comes from three authorities only: the candidate's
//! own bytes (parsed by the engine's parser — the tasks in order, their
//! verbs and tools), the compiler's requested boundary (what the workflow
//! reaches), and the same check facade `nika check` uses (the set's own
//! preview rows). No model describes a workflow here. The candidate the
//! human accepts is the exact bytes the consent lands ([`crate::change`]):
//! a fresh file at a destination this module chooses, never a replacement
//! of a file the human did not name.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use nika_check::EffectivePermits;
use nika_onboard::compile::CompileOutcome;
use nika_schema::raw::{RawAction, RawInvokeTarget, RawWorkflow};
use nika_schema::{FileId, ParseMode};

use crate::change::{ChangeError, ProjectChangeSet};

/// The directory a project keeps its workflows under, when it keeps one.
pub const WORKFLOWS_DIR: &str = "workflows";
/// The id a candidate carries when the compiler named none it could read.
const FALLBACK_ID: &str = "workflow";
/// How many numbered twins a taken name may get before the door refuses.
const MAX_TWINS: u32 = 99;

pub(crate) fn parse(candidate: &str) -> Option<RawWorkflow> {
    nika_schema::parse(candidate, FileId::new(0), ParseMode::Strict).ok()
}

/// What one task does, in the review's words: the verb and the tool or
/// model it names (`default_model` stands in for an `infer` without its
/// own). From the parser, never from prose.
pub(crate) fn task_face(task: &nika_schema::raw::RawTask, default_model: Option<&str>) -> String {
    let what = match &task.action {
        RawAction::Infer(infer) => match infer
            .model
            .as_ref()
            .map(|m| m.value.as_str())
            .or(default_model)
        {
            Some(model) => format!("infer · {model}"),
            None => "infer · (no model named)".to_owned(),
        },
        RawAction::Exec(_) => "exec · runs a program".to_owned(),
        RawAction::Agent(_) => "agent · a bounded multi-turn loop".to_owned(),
        RawAction::Invoke(invoke) => match &invoke.target {
            RawInvokeTarget::Tool(tool) => tool.value.clone(),
            RawInvokeTarget::Workflow(_) => "invoke · another workflow".to_owned(),
        },
        _ => "(a verb this review does not name)".to_owned(),
    };
    let each = if task.for_each.is_some() {
        " · for each item"
    } else {
        ""
    };
    format!("{what}{each}")
}

/// The candidate's own id (`nika:`), kebab-case as the parser accepted it.
#[must_use]
pub fn workflow_id(candidate: &str) -> String {
    parse(candidate)
        .and_then(|wf| wf.workflow.map(|id| id.value))
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| FALLBACK_ID.to_owned())
}

/// Where a fresh candidate lands, relative to the root: `<id>.nika`, under
/// `workflows/` when the project keeps that directory; a taken name gets a
/// numbered twin (`<id>-2.nika` …) so nothing the human did not name is
/// ever replaced. `None` when ninety-nine twins already exist.
#[must_use]
pub fn destination(root: &Path, candidate: &str) -> Option<PathBuf> {
    let id = workflow_id(candidate);
    let dir = if root.join(WORKFLOWS_DIR).is_dir() {
        PathBuf::from(WORKFLOWS_DIR)
    } else {
        PathBuf::new()
    };
    let first = dir.join(format!("{id}.nika"));
    if !taken(root, &first) {
        return Some(first);
    }
    (2..=MAX_TWINS)
        .map(|n| dir.join(format!("{id}-{n}.nika")))
        .find(|twin| !taken(root, twin))
}

/// Whether anything sits at the destination — a file, a directory, or a
/// symlink even when it dangles (`exists()` follows a link and would call
/// a dangling one absent; a candidate never lands on a link of any kind).
fn taken(root: &Path, rel: &Path) -> bool {
    std::fs::symlink_metadata(root.join(rel)).is_ok()
}

/// What the workflow reaches outside the project: the network hosts and
/// programs the bytes DECLARE (the boundary the human accepts, default-deny)
/// joined with the check's inferred floor. The floor alone would print
/// « none » for a loopback webhook: the inference leaves a loopback host
/// out by design (the SSRF floor) while the candidate names it. A face the
/// check could not pin is said so, never folded into « none ».
fn external_effects(candidate: &str, boundary: Option<&EffectivePermits>) -> String {
    let declared = parse(candidate).and_then(|wf| wf.permits.map(|p| p.value));
    let mut hosts: Vec<String> = declared
        .as_ref()
        .and_then(|p| p.net.as_ref())
        .map(|net| net.http.clone())
        .unwrap_or_default();
    let mut exec = declared.as_ref().and_then(|p| p.exec.clone());
    let mut unpinned = Vec::new();
    if let Some(boundary) = boundary {
        if let Some(net) = &boundary.needed.net {
            for host in &net.http {
                if !hosts.contains(host) {
                    hosts.push(host.clone());
                }
            }
        }
        if exec.is_none() {
            exec.clone_from(&boundary.needed.exec);
        }
        if boundary.partial.net && hosts.is_empty() {
            unpinned.push("a network host the check could not pin");
        }
        if boundary.partial.exec && exec.is_none() {
            unpinned.push("a program the check could not pin");
        }
    }
    let mut external = Vec::new();
    if !hosts.is_empty() {
        external.push(format!("network · {}", hosts.join(" · ")));
    }
    match exec {
        Some(nika_cap::ExecPermit::Any) => external.push("runs any program".to_owned()),
        Some(nika_cap::ExecPermit::Programs(p)) if !p.is_empty() => {
            external.push(format!("runs · {}", p.join(" · ")));
        }
        _ => {}
    }
    external.extend(unpinned.into_iter().map(str::to_owned));
    if external.is_empty() {
        "none".to_owned()
    } else {
        external.join(" · ")
    }
}

/// The candidate's tasks in order — one line each: the id, the verb, the
/// tool or model it names, whether it runs per item. From the parser,
/// never from prose.
#[must_use]
pub fn plan_lines(candidate: &str) -> Vec<String> {
    plan_lines_in_order(candidate, &[])
}

/// [`plan_lines`] in RUN order: the check's waves (indices into the file's
/// tasks) first, then anything the waves left out in file order. The file
/// lists its tasks alphabetically; a human reads what runs first, first.
#[must_use]
pub fn plan_lines_in_order(candidate: &str, waves: &[Vec<usize>]) -> Vec<String> {
    let Some(wf) = parse(candidate) else {
        return vec!["(the candidate does not parse; the check below says why)".to_owned()];
    };
    let mut order: Vec<usize> = waves
        .iter()
        .flatten()
        .copied()
        .filter(|i| *i < wf.tasks.len())
        .collect();
    for i in 0..wf.tasks.len() {
        if !order.contains(&i) {
            order.push(i);
        }
    }
    let default_model = wf.model.as_ref().map(|m| m.value.clone());
    order
        .iter()
        .enumerate()
        .filter_map(|(n, i)| wf.tasks.get(*i).map(|t| (n, t)))
        .map(|(i, task)| {
            let task = &task.value;
            format!(
                "  {}. {} · {}",
                i + 1,
                task.value_id(),
                task_face(task, default_model.as_deref())
            )
        })
        .collect()
}

trait TaskId {
    fn value_id(&self) -> &str;
}

impl TaskId for nika_schema::raw::RawTask {
    fn value_id(&self) -> &str {
        &self.id.value
    }
}

/// The tasks that pause for a human answer (`nika:prompt`), by id.
#[must_use]
pub fn gate_tasks(candidate: &str) -> Vec<String> {
    parse(candidate)
        .map(|wf| {
            wf.tasks
                .iter()
                .filter(|t| {
                    matches!(
                        &t.value.action,
                        RawAction::Invoke(invoke)
                            if matches!(&invoke.target, RawInvokeTarget::Tool(tool) if tool.value == "nika:prompt")
                    )
                })
                .map(|t| t.value.id.value.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// The set a consent lands: the candidate's exact bytes at the chosen
/// destination, witnessed and audited by [`ProjectChangeSet::workflow_at`].
/// No repair pass touches compiler output: the bytes reviewed are the
/// bytes written.
///
/// # Errors
/// No destination is free, the candidate is absent from the outcome, or
/// the destination cannot be witnessed.
pub fn propose(
    root: &Path,
    goal: &str,
    out: &CompileOutcome,
) -> Result<ProjectChangeSet, ChangeError> {
    let Some(candidate) = out.candidate.as_deref() else {
        return Err(ChangeError::Unnamed("(no candidate)".to_owned()));
    };
    let Some(path) = destination(root, candidate) else {
        return Err(ChangeError::Unnamed(format!(
            "{}.nika and its ninety-nine numbered twins all exist",
            workflow_id(candidate)
        )));
    };
    ProjectChangeSet::workflow_at(
        root,
        goal,
        &path.display().to_string(),
        candidate.to_owned(),
    )
}

/// The review: what Nika proposes (the tasks · what it reaches · whether a
/// human answers at run), then the set's own preview — the exact bytes,
/// the check of these bytes, the effect rows — and the consent question.
/// `bytes` is the set's preview, computed once by the caller (the
/// proposal's identity is its witness).
#[must_use]
pub fn render(set: &ProjectChangeSet, out: &CompileOutcome, bytes: &str) -> String {
    let Some(change) = set.changes.first() else {
        return bytes.to_owned();
    };
    let candidate = change.content();
    let mut text = format!("Nika proposes `{}`:\n", change.path().display());
    let waves: &[Vec<usize>] = out
        .check_preview
        .as_ref()
        .map_or(&[], |p| p.report.waves.as_slice());
    for line in plan_lines_in_order(candidate, waves) {
        text.push_str(&line);
        text.push('\n');
    }
    let _ = writeln!(
        text,
        "  external effects · {}",
        external_effects(candidate, out.requested_boundary.as_ref())
    );
    let gates = gate_tasks(candidate);
    let _ = writeln!(
        text,
        "  human approval at run · {}",
        if gates.is_empty() {
            "none".to_owned()
        } else {
            gates
                .iter()
                .map(|g| format!("`{g}`"))
                .collect::<Vec<_>>()
                .join(" · ")
        }
    );
    // The boundary and the audits, not every byte: `/show` prints those.
    // The identity beside the question is what a `yes` answers.
    text.push_str(&set.preview_condensed());
    let _ = writeln!(
        text,
        "  identity {} · `/show` for the exact bytes · `yes` applies · `no` discards",
        crate::ProposalId::of(bytes)
    );
    text
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use nika_onboard::compile::{CompileRequest, compile};

    fn ready(intent: &str) -> CompileOutcome {
        let out = compile(&CompileRequest::create(intent)).expect("compiles");
        assert!(
            out.candidate.is_some(),
            "{intent} is a Ready intent for this test"
        );
        out
    }

    #[test]
    fn the_destination_is_the_candidates_id_and_never_replaces_a_file() {
        let root = tempfile::tempdir().expect("root");
        let out = ready("Read ./notes/brief.md and write it to ./out/copy.md");
        let candidate = out.candidate.as_deref().expect("candidate");
        let first = destination(root.path(), candidate).expect("free");
        assert_eq!(first, PathBuf::from("compiled-workflow.nika"));
        std::fs::write(root.path().join(&first), "nika: taken\n").expect("taken");
        assert_eq!(
            destination(root.path(), candidate).expect("twin"),
            PathBuf::from("compiled-workflow-2.nika")
        );
        std::fs::create_dir(root.path().join(WORKFLOWS_DIR)).expect("dir");
        assert_eq!(
            destination(root.path(), candidate).expect("under workflows"),
            PathBuf::from("workflows/compiled-workflow.nika")
        );
    }

    #[test]
    fn the_plan_lines_come_from_the_parser() {
        let out = ready("Read ./notes/brief.md and write it to ./out/copy.md");
        let lines = plan_lines(out.candidate.as_deref().expect("candidate"));
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].contains("read_source · nika:read"), "{lines:?}");
        assert!(lines[1].contains("write_output · nika:write"), "{lines:?}");
        assert!(gate_tasks(out.candidate.as_deref().expect("candidate")).is_empty());
        assert_eq!(
            plan_lines("not: a workflow"),
            vec!["(the candidate does not parse; the check below says why)".to_owned()]
        );
    }

    #[test]
    fn the_set_lands_the_exact_bytes_and_the_review_states_the_facts() {
        let root = tempfile::tempdir().expect("root");
        let out = ready("Read every file in ./rfc/*.md and write them combined into ./all.md");
        let set = propose(root.path(), "the goal", &out).expect("set");
        assert_eq!(set.changes.len(), 1);
        assert_eq!(
            set.changes[0].content(),
            out.candidate.as_deref().expect("candidate")
        );
        assert!(set.run.is_none(), "acceptance is never a run");
        assert!(
            set.repairs.is_empty(),
            "compiler output is not repaired by a ladder"
        );
        let bytes = set.preview();
        let review = render(&set, &out, &bytes);
        assert!(
            review.starts_with("Nika proposes `compiled-workflow.nika`:"),
            "{review}"
        );
        assert!(
            review.contains("read_source · nika:read · for each item"),
            "{review}"
        );
        assert!(review.contains("external effects · none"), "{review}");
        assert!(review.contains("human approval at run · none"), "{review}");
        assert!(review.contains("reads ./rfc/**"), "{review}");
        assert!(review.contains("writes ./all.md"), "{review}");
        assert!(
            review.contains("`/show` prints the exact"),
            "the condensed preview closes the review: {review}"
        );
        assert!(
            !review.contains("expression:"),
            "the internals stay behind /show: {review}"
        );
        assert!(
            review.contains("permits:"),
            "the boundary is shown: {review}"
        );
        assert!(
            review.contains(&format!("identity {}", crate::ProposalId::of(&bytes))),
            "the identity a yes answers is printed: {review}"
        );
    }

    /// A webhook to a loopback host: the check's inferred floor leaves the
    /// host out by design, the bytes declare it, and the human must see it
    /// before consenting. « none » here would hide an effect.
    #[test]
    fn a_loopback_webhook_is_an_external_effect_the_review_names() {
        let out = ready("Read ./report.md and post it to http://127.0.0.1:8767/notify");
        let root = tempfile::tempdir().expect("root");
        let set = propose(root.path(), "post the report", &out).expect("set");
        let review = render(&set, &out, &set.preview());
        assert!(
            review.contains("external effects · network · 127.0.0.1"),
            "{review}"
        );
        assert!(!review.contains("external effects · none"), "{review}");
    }

    #[test]
    fn a_gated_skeleton_names_its_gate() {
        let out = ready("human-gated-ship");
        let candidate = out.candidate.as_deref().expect("candidate");
        assert_eq!(gate_tasks(candidate), vec!["human".to_owned()]);
        let root = tempfile::tempdir().expect("root");
        let set = propose(root.path(), "ship", &out).expect("set");
        let review = render(&set, &out, &set.preview());
        assert!(
            review.contains("human approval at run · `human`"),
            "{review}"
        );
        // The bytes declare a host the inferred floor does not carry (a SLOT the
        // skeleton leaves for its notify): the human sees the declared reach.
        assert!(
            review.contains("external effects · network · hooks.slack.com · runs · echo"),
            "{review}"
        );
        assert!(review.contains("4. human · nika:prompt"), "{review}");
    }
}
