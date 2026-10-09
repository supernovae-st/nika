// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The candidate the Live host adapter folds from its Session after every
//! turn, on the turn's own thread (never while drawing): the Session lends its
//! proposal borrowed (`SessionRuntime::candidate`: the identity a consent
//! names, the exact set a yes lands, the words of the rehearsal proof bound to
//! that identity), and the pending bytes of its workflow are witnessed and
//! judged here ONCE, by the same fold as an opened file
//! ([`super::look::judge`]: the shared check facade over these bytes alone).
//! The bytes are the proposal's, held in memory: no file is read.
//!
//! A previous fold lends only its look, and only to the very same bytes at the
//! very same path; every other fact (the identity, the changes and their
//! witnesses, what the workflow reaches, what a `save & run` would run, the
//! typed compile record bound to its bytes, the rehearsal, the standing) is
//! taken from the Session anew. The fold grants nothing and joins no consent.

use std::fmt::Write as _;

use nika_session::change::{ProjectChange, ProjectChangeSet, Witness};
use nika_session::work::{DocumentRevision, Waiting};
use nika_session::{ProposalId, SessionRuntime};

use super::look::judge;
use crate::workspace::candidate::{Proposed, RunAfter, admitted};
use crate::workspace::cards::review::KEEP;
use crate::workspace::inspect::Inspected;

/// The runtime's candidate, folded; `kept` is the previous fold.
pub(crate) fn take(runtime: &SessionRuntime, kept: Option<&Proposed>) -> Option<Proposed> {
    let Some(candidate) = runtime.candidate() else {
        return draft(runtime, kept);
    };
    let set = candidate.set;
    let shown = (set.changes.iter().find(|c| c.is_workflow())).or_else(|| set.changes.first())?;
    let path = shown.path().display().to_string();
    let source = shown.content();
    let witness = Witness::of(source.as_bytes()).0;
    let look = match kept.map(Proposed::look) {
        Some(look) if look.path() == path && look.witness() == Some(witness.as_str()) => {
            look.clone()
        }
        _ if shown.is_workflow() => judge(path, witness, source.to_owned()),
        _ => Inspected::unjudged(path, witness, source.to_owned()),
    };
    let effects = if set.audits.len() > 1 {
        // Several workflows: what each one reaches, under its own path.
        let each = set.audits.iter().flat_map(|audit| {
            let path = audit.path.display().to_string();
            let rows = if audit.effects.is_empty() {
                vec!["nothing outside the process".to_owned()]
            } else {
                audit.effects.clone()
            };
            rows.into_iter().map(move |row| format!("{path} · {row}"))
        });
        Some(each.collect())
    } else {
        (set.audits.iter())
            .find(|audit| audit.path == shown.path())
            .map(|audit| audit.effects.clone())
    };
    // Where each audited workflow's bytes reach, as the check declares it (never observed):
    // a local contract server is told apart from a connected service before any yes.
    let several = set.audits.len() > 1;
    let world = (set.audits.iter())
        .filter(|audit| several || audit.path == shown.path())
        .map(|audit| {
            let summary = audit.world.summary();
            let words = if several {
                format!("{} · {summary}", audit.path.display())
            } else {
                summary
            };
            (words, reaches_outside(audit.world.reach))
        })
        .collect();
    let revision = runtime.pending_revision();
    let fold = Proposed::new(candidate.id.clone(), candidate.aside, look)
        .changing(set.changes.iter().map(words).collect())
        .reaching(effects)
        .revising(revision.as_ref().map_or_else(Vec::new, revised))
        .recording(bound(set, revision))
        .declaring(world)
        .running(run_after(set))
        .unshown(set.changes.len().saturating_sub(1))
        .rehearsed(candidate.rehearsed.map(str::to_owned));
    Some(fold)
}

/// What a `save & run` of `set` runs once its save checked clean, as the
/// Session's own typed method admits it (`ProjectChangeSet::save_run`): the
/// run its request carried, in words (the workflow, the ceiling it states and
/// the names of its inputs, never their values), else the one workflow it
/// saves; `None` where the method refuses. Nothing here reads a file.
fn run_after(set: &ProjectChangeSet) -> Option<RunAfter> {
    let run = set.save_run().ok()?;
    if set.run.is_none() {
        return Some(RunAfter::Saved);
    }
    let mut words = format!("asked run · {} once", run.workflow.display());
    if let Some(ceiling) = run.max_cost_usd {
        let _ = write!(words, " · ceiling ${ceiling:.2}");
    }
    // `name=value`: the name alone, as the work snapshot keeps it.
    let names: Vec<&str> = (run.vars.iter())
        .filter_map(|var| var.split_once('=').map(|(name, _)| name))
        .collect();
    if !names.is_empty() {
        let _ = write!(words, " · inputs {}", names.join(", "));
    }
    Some(RunAfter::Asked(words))
}

/// Where the draft's look is named: it lands nowhere until a proposal says where.
const DRAFT: &str = "draft.nika";

/// The compiler's draft while its question waits and nothing is proposed, read from the Session's
/// own work snapshot (the bytes every host reads) and judged once like a candidate: it names no
/// consent, so a `yes` can never answer it.
fn draft(runtime: &SessionRuntime, kept: Option<&Proposed>) -> Option<Proposed> {
    if !matches!(runtime.waiting(), Waiting::Question { .. }) {
        return None;
    }
    let source = runtime.work().authoring?.draft?;
    let witness = Witness::of(source.as_bytes()).0;
    let look = match kept.map(Proposed::look) {
        Some(look) if look.witness() == Some(witness.as_str()) => look.clone(),
        _ => judge(DRAFT.to_owned(), witness, source.clone()),
    };
    Some(Proposed::new(ProposalId::of(&source), true, look).drafted())
}

/// The typed compile record, kept with the fold where it names the file it binds: the Session
/// binds it to one workflow's bytes by digest, so with one workflow in the set those are the
/// shown file's. Several workflows leave that file unnamed here, and no record is kept.
fn bound(set: &ProjectChangeSet, record: Option<DocumentRevision>) -> Option<DocumentRevision> {
    let workflows = set.changes.iter().filter(|c| c.is_workflow()).count();
    record.filter(|_| workflows == 1)
}

/// How the typed compile record describes the pending document: a creation without earlier
/// bytes, literal edits, or a whole replacement. An unknown mode claims none of those. A
/// component `expanded` or `invoked` on these bytes is admitted held reuse ([`admitted`]);
/// any other witness needs attention.
fn revised(revision: &DocumentRevision) -> Vec<(String, bool)> {
    let mut rows = vec![match revision.mode.as_str() {
        "operations" if revision.changed.is_empty() => {
            ("revised in place · no node changed".to_owned(), false)
        }
        "operations" => (
            format!("revised in place · {}", revision.changed.join(", ")),
            false,
        ),
        "written" | "composed" if revision.base_sha256.is_none() => {
            (format!("created · {}", revision.mode), false)
        }
        "replaced" => (
            "rewritten whole · no preservation of the earlier bytes is claimed".to_owned(),
            true,
        ),
        _ => ("document record · mode not described".to_owned(), true),
    }];
    rows.extend(revision.components.iter().map(|component| {
        let bound: Vec<String> = (component.bindings.iter())
            .map(|b| format!("{} = {}", b.path, b.value))
            .collect();
        let version = component.version.as_deref().unwrap_or("unversioned");
        let mut words = format!(
            "component · {} {version} · {}",
            component.id, component.witness
        );
        if !bound.is_empty() {
            words.push_str(" · ");
            words.push_str(&bound.join(", "));
        }
        (words, !admitted(&component.witness))
    }));
    rows
}

/// Whether a reach leaves this machine or cannot say where it leads: a connected service, a
/// remote model, an MCP tool or a program. Such a row is shown as a warning.
fn reaches_outside(reach: nika_session::world::Reach) -> bool {
    use nika_session::world::Reach;
    !matches!(reach, Reach::Local | Reach::LocalServices)
}

/// One change in words: what it creates or replaces, where, how long (the
/// count bound to its unit, [`KEEP`]), and over which witnessed bytes a
/// replacement stands.
fn words(change: &ProjectChange) -> String {
    let path = change.path().display().to_string();
    let lines = change.content().lines().count();
    match change.witness() {
        None => format!("creates {path} ({lines}{KEEP}lines)"),
        Some(before) => format!(
            "replaces {path} ({lines}{KEEP}lines) · over the bytes witnessed {}",
            before.short()
        ),
    }
}

#[cfg(test)]
#[cfg(unix)]
#[allow(clippy::expect_used, clippy::panic)]
mod candidate_tests;
