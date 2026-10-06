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
//! of a file the human did not name. A revision of a saved workflow is the
//! one replacement: that file, over the exact bytes the revision compiled.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[doc(inline)]
pub use display_review::{plan_lines, plan_lines_in_order, task_face};
use nika_display::check_render::review as display_review;
use nika_onboard::compile::{
    CompileOutcome, DiagnosticKind, TriggerKind, TriggerRequirement, TriggerStatus,
};
use nika_schema::raw::{RawAction, RawInvokeTarget, RawWorkflow};
use nika_schema::{FileId, ParseMode};

use crate::change::{ChangeError, ProjectChange, ProjectChangeSet, Witness};

/// The directory a project keeps its workflows under, when it keeps one.
pub const WORKFLOWS_DIR: &str = "workflows";
/// The id a candidate carries when the compiler named none it could read.
const FALLBACK_ID: &str = "workflow";

/// The candidate as the engine's parser reads it (strict), or `None` when
/// it does not parse.
#[must_use]
pub fn parse(candidate: &str) -> Option<RawWorkflow> {
    nika_schema::parse(candidate, FileId::new(0), ParseMode::Strict).ok()
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
/// ever replaced. No product quota limits the number of siblings; `None` only if
/// the representable suffix space is exhausted.
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
    (2..=u64::MAX)
        .map(|n| dir.join(format!("{id}-{n}.nika")))
        .find(|twin| !taken(root, twin))
}

/// Whether anything sits at the destination — a file, a directory, or a
/// symlink even when it dangles (`exists()` follows a link and would call
/// a dangling one absent; a candidate never lands on a link of any kind).
fn taken(root: &Path, rel: &Path) -> bool {
    std::fs::symlink_metadata(root.join(rel)).is_ok()
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
            "no representable unused destination for {}.nika",
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

/// The set a saved workflow's revision lands: the candidate's exact bytes over that file,
/// witnessed now and refused unless the file still holds the base the revision compiled —
/// never a fresh destination beside it, never an update over bytes the compiler did not read.
///
/// # Errors
/// The candidate is absent, the path cannot be witnessed, or the file no longer holds `base`.
pub fn propose_over(
    root: &Path,
    goal: &str,
    path: &Path,
    base: &Witness,
    out: &CompileOutcome,
) -> Result<ProjectChangeSet, ChangeError> {
    let Some(candidate) = out.candidate.as_deref() else {
        return Err(ChangeError::Unnamed("(no candidate)".to_owned()));
    };
    let shown = path.display().to_string();
    let set = ProjectChangeSet::workflow_at(root, goal, &shown, candidate.to_owned())?;
    match set.changes.first() {
        Some(ProjectChange::UpdateWorkflow { before, .. }) if before == base => Ok(set),
        _ => Err(ChangeError::Stale(shown)),
    }
}

/// The review's line on execution: nothing has run. A proposal whose copy was rehearsed in a
/// room replaces it with what that rehearsal did, never on the originals (the session's
/// `runtime/rehearsed.rs`).
pub const NOTHING_RAN: &str = "Nothing has run yet · `yes` saves these exact bytes and checks them · running is its own line (« run it »)\n";

/// The review: what Nika proposes, in the order a human decides — what it
/// DOES (the tasks, first what runs first), when it RUNS (by hand, or the
/// schedule the request asked for, which saving never activates), what it
/// CAN TOUCH (what the bytes reach, the human gates), what CHANGES on disk,
/// what it still NEEDS — then the fact that nothing has run, the set's own
/// condensed preview (the boundary, the check of these exact bytes, `/show`
/// for every byte) and the consent question. Every line has an owner: the
/// parser, the check, the change set, the compiler's requirements; none is
/// prose a model wrote. `bytes` is the set's preview, computed once by the
/// caller (the proposal's identity is its witness).
#[must_use]
pub fn render(set: &ProjectChangeSet, out: &CompileOutcome, bytes: &str) -> String {
    let Some(change) = set.changes.first() else {
        return bytes.to_owned();
    };
    let candidate = change.content();
    let mut text = format!("Nika proposes `{}`:\n", change.path().display());
    text.push_str("Does\n");
    let waves: &[Vec<usize>] = out
        .check_preview
        .as_ref()
        .map_or(&[], |p| p.report.waves.as_slice());
    for line in plan_lines_in_order(candidate, waves) {
        text.push_str(&line);
        text.push('\n');
    }
    text.push_str("Runs\n");
    text.push_str(&runs_line(out.requested_trigger.as_ref()));
    text.push('\n');
    text.push_str("Can touch\n");
    let _ = writeln!(
        text,
        "  external effects · {}",
        display_review::external_effects(candidate, out.requested_boundary.as_ref())
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
    text.push_str("Changes\n");
    for c in &set.changes {
        let lines = c.content().lines().count();
        match c.witness() {
            None => {
                let _ = writeln!(text, "  + `{}` · {lines} lines · new", c.path().display());
            }
            Some(w) => {
                let _ = writeln!(
                    text,
                    "  ~ `{}` · {lines} lines · replaces the file as it is now (witnessed {})",
                    c.path().display(),
                    w.short()
                );
            }
        }
    }
    text.push_str("Needs\n");
    text.push_str(&needs_lines(out));
    text.push_str(NOTHING_RAN);
    // The boundary and the audits, not every byte: `/show` prints those.
    // The identity beside the question is what a `yes` answers.
    text.push_str(&set.preview_condensed());
    let _ = writeln!(
        text,
        "  identity {} · `/show` the exact bytes · `/meaning` your request clause by clause · `yes` applies · `no` discards",
        crate::outcome::ProposalId::of(bytes)
    );
    text
}

/// The Runs line: by hand, or the schedule the request stated — kept
/// beside the program (saving never activates it) — or a trigger the
/// compiler read but cannot express.
fn runs_line(trigger: Option<&TriggerRequirement>) -> String {
    let Some(t) = trigger else {
        return "  when you ask (« run it ») · no schedule was asked".to_owned();
    };
    let quoted = t
        .source_hint
        .as_deref()
        .filter(|w| !w.is_empty())
        .map_or(String::new(), |w| format!(" (« {w} »)"));
    if t.status == TriggerStatus::Unsupported {
        return format!(
            "  ! the request asks for a trigger{quoted} the compiler cannot express yet · the workflow runs when you ask"
        );
    }
    match t.kind {
        TriggerKind::Schedule => {
            let when = match (t.cadence.as_deref(), t.at.as_deref()) {
                (Some(c), Some(at)) => format!("{c} at {at}"),
                (Some(c), None) => c.to_owned(),
                (None, Some(at)) => format!("at {at}"),
                (None, None) => "on a schedule".to_owned(),
            };
            let exact = t.cron.as_deref().map_or_else(
                || "incomplete, conflicting or unsupported cadence · activation refuses until restated with an explicit supported period/time".to_owned(),
                |fields| format!("proposed cron `{fields}` · timezone still belongs to activation"),
            );
            format!(
                "  ↗ {when}{quoted} · a schedule to activate AFTER saving · saving alone activates nothing\n    {exact}"
            )
        }
        TriggerKind::Webhook => format!(
            "  ↗ on an incoming call{quoted} · a binding to set up AFTER saving · saving alone arms nothing"
        ),
        TriggerKind::Event => format!(
            "  ↗ on each incoming item{quoted} · a binding to set up AFTER saving · saving alone arms nothing"
        ),
        // Manual, and any kind a later compiler adds: by hand.
        _ => "  when you ask (« run it »)".to_owned(),
    }
}

/// The Needs lines: the requirements outside the bytes and the parts of
/// the request the compiler named as missed, unknown or refused.
fn needs_lines(out: &CompileOutcome) -> String {
    let mut lines = Vec::new();
    if let Some(t) = &out.requested_trigger
        && t.status == TriggerStatus::RequiresBinding
    {
        lines.push(
            "  ↗ the schedule or trigger above · bound when you activate, not by saving".to_owned(),
        );
    }
    for d in &out.diagnostics {
        let glyph = match d.kind {
            DiagnosticKind::Missed | DiagnosticKind::Unknown => "!",
            DiagnosticKind::Refused => "×",
            _ => continue,
        };
        lines.push(format!("  {glyph} {} · {}", d.target, d.message));
    }
    if lines.is_empty() {
        "  nothing more from you\n".to_owned()
    } else {
        lines.join("\n") + "\n"
    }
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
    fn creation_past_ninety_nine_siblings_preserves_every_existing_file() {
        let root = tempfile::tempdir().expect("root");
        let out = ready("Read ./notes/brief.md and write it to ./out/copy.md");
        let candidate = out.candidate.as_deref().expect("candidate");
        for n in 1..=120 {
            let name = if n == 1 {
                "compiled-workflow.nika".to_owned()
            } else {
                format!("compiled-workflow-{n}.nika")
            };
            std::fs::write(root.path().join(name), "existing work").expect("existing sibling");
        }
        let proposal = propose(root.path(), "another workflow", &out).expect("proposal");
        assert_eq!(
            proposal.changes[0].path(),
            PathBuf::from("compiled-workflow-121.nika")
        );
        assert_eq!(
            destination(root.path(), candidate),
            Some(proposal.changes[0].path())
        );
        assert_eq!(std::fs::read_dir(root.path()).expect("files").count(), 120);
        for entry in std::fs::read_dir(root.path()).expect("files") {
            assert_eq!(
                std::fs::read(entry.expect("file").path()).expect("bytes"),
                b"existing work"
            );
        }
    }

    #[test]
    fn the_plan_lines_come_from_the_parser() {
        let out = ready("Read ./notes/brief.md and write it to ./out/copy.md");
        let lines = plan_lines(out.candidate.as_deref().expect("candidate"));
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].contains("read_source · reads a file"), "{lines:?}");
        assert!(
            lines[1].contains("write_output · writes a file"),
            "{lines:?}"
        );
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
            review.contains("read_source · reads a file · for each item"),
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
            review.contains(&format!(
                "identity {}",
                crate::outcome::ProposalId::of(&bytes)
            )),
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
        assert!(review.contains("4. human · asks a human"), "{review}");
    }
}
