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

use nika_session::change::{ProjectChange, ProjectChangeSet, Witness};
use nika_session::work::{DocumentRevision, Waiting, Work};
use nika_session::{ProposalId, SessionRuntime};
use nika_tui_view::workspace::text::KEEP;

use super::look::judge;
use crate::lang::{Lang, Voice};
use crate::workspace::candidate::{Proposed, RunAfter, admitted};
use crate::workspace::inspect::Inspected;

/// The runtime's candidate, folded; `kept` is the previous fold. The words built here (each
/// change, how it was revised, the run it asked) are said in the person's language and the
/// glyph column in use (`ascii`); the Session's facts keep their own.
pub(crate) fn take(
    runtime: &SessionRuntime,
    kept: Option<&Proposed>,
    voice: Voice,
) -> Option<Proposed> {
    let Some(candidate) = runtime.candidate() else {
        return draft(runtime, kept).map(|draft| draft.speaking(voice.lang));
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
                vec![voice.say("nothing outside the process").to_owned()]
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
    // A declaration's schedule, in the person's words as the Session says it: never its cron.
    let schedule = (runtime.work().candidate).and_then(|shown| shown.schedule);
    let fold = Proposed::new(candidate.id.clone(), candidate.aside, look)
        .changing(
            set.changes
                .iter()
                .map(|change| words(change, voice))
                .collect(),
        )
        .reaching(effects)
        .revising((revision.as_ref()).map_or_else(Vec::new, |revision| revised(revision, voice)))
        .recording(bound(set, revision))
        .declaring(world)
        .running(run_after(set, voice))
        .scheduling(schedule)
        .unshown(set.changes.len().saturating_sub(1))
        .rehearsed(candidate.rehearsed.map(str::to_owned))
        .speaking(voice.lang);
    Some(fold)
}

/// What a `save & run` of `set` runs once its save checked clean, as the
/// Session's own typed method admits it (`ProjectChangeSet::save_run`): the
/// run its request carried, in words (the workflow, the ceiling it states and
/// the names of its inputs, never their values), else the one workflow it
/// saves; `None` where the method refuses. Nothing here reads a file.
fn run_after(set: &ProjectChangeSet, voice: Voice) -> Option<RunAfter> {
    let run = set.save_run().ok()?;
    if set.run.is_none() {
        return Some(RunAfter::Saved);
    }
    let workflow = run.workflow.display().to_string();
    let mut words = voice
        .say("asked run · {workflow} once")
        .replace("{workflow}", &workflow);
    if let Some(ceiling) = run.max_cost_usd {
        let stated = nika_display::vocab::stated_usd(ceiling);
        words.push_str(
            &voice
                .say(" · ceiling ${stated}")
                .replace("{stated}", &stated),
        );
    }
    // `name=value`: the name alone, as the work snapshot keeps it.
    let names: Vec<&str> = (run.vars.iter())
        .filter_map(|var| var.split_once('=').map(|(name, _)| name))
        .collect();
    if !names.is_empty() {
        let inputs = voice.say(" · inputs {names}");
        words.push_str(&inputs.replace("{names}", &names.join(", ")));
    }
    Some(RunAfter::Asked(words))
}

/// Where the draft's look is named: it lands nowhere until a proposal says where.
const DRAFT: &str = "draft.nika";

/// The compiler's draft while its question waits and nothing is proposed, read from the Session's
/// own work snapshot (the bytes every host reads) and judged once like a candidate: it names no
/// consent, so a `yes` can never answer it.
fn draft(runtime: &SessionRuntime, kept: Option<&Proposed>) -> Option<Proposed> {
    draft_of(&runtime.work(), kept)
}

/// [`draft`] from the work snapshot `work`: only the draft of the round that asks the question
/// waiting, its key among that round's questions. A draft the snapshot keeps from another round
/// (a discarded or earlier one, beside a question it does not ask; live gauntlet 5, r07 B4: F6
/// showed a discarded draft while the change was asked of the saved file) shows nothing, so the
/// saved file, or nothing, stands instead.
fn draft_of(work: &Work, kept: Option<&Proposed>) -> Option<Proposed> {
    let Waiting::Question { key, .. } = &work.waiting else {
        return None;
    };
    let authoring = work.authoring.as_ref()?;
    if !authoring.questions.iter().any(|asked| asked == key) {
        return None;
    }
    let source = authoring.draft.clone()?;
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

/// How the typed compile record describes the pending document, in a person's words (live
/// gauntlet 4: « révisé sur place · tasks.clean.invoke.args.expression », « composant ·
/// block:csv-clean-dedupe-report knowledge-0.123.0-r3.5-foundry-20261016 · expanded »): a
/// creation without earlier bytes, literal edits by the step or value they changed
/// ([`changed_words`]), or a whole replacement. An unknown mode claims none of those. A
/// component reads by its name, what the bytes show of it ([`witness_words`]) and the values
/// it was given; its kind, release and digests stay with the check face's compile record. A
/// component `expanded` or `invoked` on these bytes is admitted held reuse ([`admitted`]);
/// any other witness needs attention.
fn revised(revision: &DocumentRevision, voice: Voice) -> Vec<(String, bool)> {
    let said = |en: &'static str| voice.say(en);
    let mut rows = vec![match revision.mode.as_str() {
        "operations" if revision.changed.is_empty() => {
            (said("revised in place · no node changed").to_owned(), false)
        }
        "operations" => (
            said("revised in place · {changed}")
                .replace("{changed}", &changed_words(&revision.changed, voice)),
            false,
        ),
        "written" if revision.base_sha256.is_none() => {
            (said("created · written whole").to_owned(), false)
        }
        "composed" if revision.base_sha256.is_none() => {
            (said("created · composed from components").to_owned(), false)
        }
        "replaced" => (
            said("rewritten whole · no preservation of the earlier bytes is claimed").to_owned(),
            true,
        ),
        _ => (
            said("document record · mode not described").to_owned(),
            true,
        ),
    }];
    let component = said("component");
    rows.extend(revision.components.iter().map(|part| {
        let bound = (part.bindings.iter())
            .map(|b| format!("{} = {}", hole(&b.path), b.value))
            .reduce(|all, one| format!("{all}, {one}"))
            .map_or_else(String::new, |all| format!(" · {all}"));
        let witness = voice.say(witness_words(&part.witness));
        let words = format!("{component} · {} · {witness}{bound}", named(&part.id));
        (words, !admitted(&part.witness))
    }));
    rows
}

/// A component by its name, its kind aside (`block:stale-filter-report` reads
/// `stale-filter-report`).
fn named(id: &str) -> &str {
    id.split_once(':').map_or(id, |(_, name)| name)
}

/// A component's hole by its name (`const.max_age_hours` reads `max_age_hours`).
fn hole(path: &str) -> &str {
    path.strip_prefix("const.").unwrap_or(path)
}

/// What the bytes show of a component, in words: reused or called as bound, changed since,
/// gone, unreadable or never checked; a witness these words do not know as it came.
fn witness_words(witness: &str) -> &str {
    match witness {
        "expanded" => "reused as bound",
        "invoked" => "called as bound",
        "revised" => "reused, then changed",
        "absent" => "no longer in these bytes",
        "unreadable" => "unreadable in these bytes",
        "unwitnessed" => "not checked in these bytes",
        other => other,
    }
}

/// The nodes a revision changed, by what a person calls them: `tasks.clean.invoke.args.x`
/// reads « step clean », `const.key` « value key », `component block:x` « component x »;
/// each once, in order; any other path as it is.
fn changed_words(paths: &[String], voice: Voice) -> String {
    let mut said: Vec<String> = Vec::new();
    for path in paths {
        let words = if let Some(component) = path.strip_prefix("component ") {
            voice
                .say("component {name}")
                .replace("{name}", named(component))
        } else {
            let mut parts = path.split('.');
            let head = parts.next().unwrap_or_default();
            match (head, parts.next()) {
                ("tasks", Some(name)) => voice.say("step {name}").replace("{name}", name),
                ("const", Some(name)) => voice.say("value {name}").replace("{name}", name),
                ("inputs", Some(name)) => voice.say("input {name}").replace("{name}", name),
                ("outputs", Some(name)) => voice.say("output {name}").replace("{name}", name),
                ("permits", _) => voice.say("the permissions").to_owned(),
                ("model", _) => voice.say("the model").to_owned(),
                ("run", _) => voice.say("the run settings").to_owned(),
                _ => path.clone(),
            }
        };
        if !said.contains(&words) {
            said.push(words);
        }
    }
    said.join(", ")
}

/// Whether a reach leaves this machine or cannot say where it leads: a connected service, a
/// remote model, an MCP tool or a program. Such a row is shown as a warning.
fn reaches_outside(reach: nika_session::world::Reach) -> bool {
    use nika_session::world::Reach;
    !matches!(reach, Reach::Local | Reach::LocalServices)
}

/// One change in words: what it creates or replaces, where, how long (the
/// count bound to its unit, [`KEEP`]), and that a replacement stands over
/// the bytes the Session read.
fn words(change: &ProjectChange, voice: Voice) -> String {
    let path = change.path().display().to_string();
    let lines = change.content().lines().count();
    // English keeps its one unit; French writes the singular below two.
    let unit = if voice.lang == Lang::Fr && lines <= 1 {
        voice.say("line")
    } else {
        voice.say("lines")
    };
    let count = format!("{lines}{KEEP}{unit}");
    let said = match change.witness() {
        None => voice.say("creates {path} ({count})").to_owned(),
        // Over the bytes the Session read, without their witness's hex (live gauntlet 4:
        // « sur les octets attestés 62ab013d »): it names nothing a person acts on here.
        Some(_) => voice
            .say("replaces {path} ({count}) · over its bytes as read")
            .to_owned(),
    };
    said.replace("{count}", &count).replace("{path}", &path)
}

#[cfg(test)]
#[cfg(unix)]
#[allow(clippy::expect_used, clippy::panic)]
mod candidate_tests;
