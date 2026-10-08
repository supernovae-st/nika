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
//! witnesses, what the workflow reaches, the rehearsal, the standing) is taken
//! from the Session anew. The fold grants nothing and joins no consent.

use nika_session::SessionRuntime;
use nika_session::change::{ProjectChange, Witness};

use super::look::judge;
use crate::workspace::candidate::Proposed;
use crate::workspace::inspect::Inspected;

/// The runtime's candidate, folded; `kept` is the previous fold.
pub(crate) fn take(runtime: &SessionRuntime, kept: Option<&Proposed>) -> Option<Proposed> {
    let candidate = runtime.candidate()?;
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
    let fold = Proposed::new(candidate.id.clone(), candidate.aside, look)
        .changing(set.changes.iter().map(words).collect())
        .reaching(effects)
        .revising(
            runtime
                .pending_revision()
                .map_or_else(Vec::new, |r| revised(&r)),
        )
        .declaring(world)
        .unshown(set.changes.len().saturating_sub(1))
        .rehearsed(candidate.rehearsed.map(str::to_owned));
    Some(fold)
}

/// How the pending workflow was revised over its complete document, as the compile record states
/// it: what changed, never that it is what was meant. A whole replacement claims no preservation
/// and each component not witnessed as bound on these bytes needs attention.
fn revised(revision: &nika_session::work::DocumentRevision) -> Vec<(String, bool)> {
    let mut rows = vec![match revision.mode.as_str() {
        "operations" if revision.changed.is_empty() => {
            ("revised in place · no node changed".to_owned(), false)
        }
        "operations" => (
            format!("revised in place · {}", revision.changed.join(", ")),
            false,
        ),
        _ => (
            "rewritten whole · no preservation of the earlier bytes is claimed".to_owned(),
            true,
        ),
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
        (words, component.witness != "expanded")
    }));
    rows
}

/// Whether a reach leaves this machine or cannot say where it leads: a connected service, a
/// remote model, an MCP tool or a program. Such a row is shown as a warning.
fn reaches_outside(reach: nika_session::world::Reach) -> bool {
    use nika_session::world::Reach;
    !matches!(reach, Reach::Local | Reach::LocalServices)
}

/// One change in words: what it creates or replaces, where, how long, and
/// over which witnessed bytes a replacement stands.
fn words(change: &ProjectChange) -> String {
    let path = change.path().display().to_string();
    let lines = change.content().lines().count();
    match change.witness() {
        None => format!("creates {path} · {lines} lines · new"),
        Some(before) => format!(
            "replaces {path} · {lines} lines · over the bytes witnessed {}",
            before.short()
        ),
    }
}

#[cfg(test)]
#[cfg(unix)]
#[allow(clippy::expect_used, clippy::panic)]
mod candidate_tests;
