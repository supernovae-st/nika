// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The project a conversation lends the workspace
//! ([`crate::model::Conversation::project`]), routed: what an aside entry
//! opens (`Target`) and the object it resolves to (`Opened`), the live
//! run and the candidate included. The project's data and its pure
//! projections (the header, the conversation panel, the welcome, the workflow
//! and run objects) live in [`nika_tui_view::workspace::project`] (ADR-143);
//! this path keeps the renderer's callers unchanged.

use super::aside::{Aside, Entry, Tab, Verdict};
use super::candidate::Proposed;
use super::live::LiveRun;
use super::object::Object;
use super::pinned::Pinned;
use crate::lang::Voice;
use crate::visual::icon::Icon;
use jiff::tz::TimeZone;
use nika_display::run_story::{ExecutionId, RunState};
#[cfg(test)]
pub(crate) use nika_tui_view::workspace::project::DEMO;
#[cfg(test)]
pub(crate) use nika_tui_view::workspace::project::welcome;
use nika_tui_view::workspace::project::{FILES_NOT_LENT, THIS_CONVERSATION};
pub use nika_tui_view::workspace::project::{ProjectView, WorkflowView};
pub(crate) use nika_tui_view::workspace::project::{place, thread, welcome_in};
use nika_tui_view::workspace::project::{run_object, workflow_object};

/// What an aside entry opens.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum Target {
    /// This conversation: the keys go back to its composer.
    Conversation,
    /// A listed workflow, by its path: it becomes the object in view.
    Workflow(String),
    /// The run the view pins, by its name (`#043`): it becomes the object.
    Run(String),
    /// The candidate the conversation proposes, not saved: it becomes the
    /// object while the conversation lends it.
    Candidate,
    /// The run leg the shell observes: it becomes the object while held.
    Live,
    /// A run leg observed before the one in flight, by its execution: it
    /// becomes the object in view as it was observed, and opening it runs
    /// nothing, changes no candidate and gives no authority.
    Past(ExecutionId),
}

/// What the object in view is, resolved against the current view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum Opened<'a> {
    /// A listed workflow.
    Workflow(&'a WorkflowView),
    /// The pinned run.
    Run(&'a Pinned),
    /// The candidate under review.
    Candidate(&'a Proposed),
    /// The run leg the shell observes.
    Live(&'a LiveRun),
}

impl Opened<'_> {
    /// The object in view, as the conversation panel names it on screen, in
    /// `voice` (a candidate's name; paths and runs as they are).
    #[must_use]
    pub(crate) fn label(self, voice: Voice) -> String {
        match self {
            Self::Workflow(workflow) => workflow.path.clone(),
            Self::Run(run) => format!("{} {}", run.run, run.workflow),
            Self::Candidate(candidate) => candidate.label_in(voice.ascii),
            Self::Live(leg) => format!("{} {}", leg.label_in(voice.lang), leg.workflow()),
        }
    }

    /// The object in view, painted from the facts the view holds; the glyph
    /// column (`ascii`) reaches the words the workspace composes.
    #[must_use]
    pub(crate) fn object(self, ascii: bool) -> Object {
        match self {
            Self::Workflow(workflow) => workflow_object(workflow),
            Self::Run(run) => run_object(run, ascii),
            Self::Candidate(candidate) => Object::Shown {
                icon: Icon::Workflow,
                name: candidate.label(),
                lines: vec![candidate.path().to_owned()],
            },
            Self::Live(leg) => Object::Shown {
                icon: Icon::Run,
                name: leg.label(),
                lines: vec![leg.workflow().to_owned()],
            },
        }
    }
}

/// `target`, resolved against `view` and the candidate the conversation lends:
/// `None` when neither holds it any more (the object then falls back to the
/// welcome and nothing claims to be on screen).
#[must_use]
pub(crate) fn resolve<'a>(
    view: Option<&'a ProjectView>,
    target: Option<&Target>,
    candidate: Option<&'a Proposed>,
    (live, past): (Option<&'a LiveRun>, &'a [LiveRun]),
) -> Option<Opened<'a>> {
    match target? {
        Target::Candidate => return candidate.map(Opened::Candidate),
        Target::Live => return live.map(Opened::Live),
        Target::Past(execution) => {
            return past
                .iter()
                .find(|leg| leg.execution() == Some(*execution))
                .map(Opened::Live);
        }
        _ => {}
    }
    let view = view?;
    match target? {
        Target::Workflow(path) => view.workflow(path).map(Opened::Workflow),
        Target::Run(name) => view
            .pinned
            .as_ref()
            .filter(|run| &run.run == name)
            .map(Opened::Run),
        _ => None,
    }
}

/// The `tab` projection's entries, each with what it opens: this
/// conversation, the candidate it proposes when there is one, the judged
/// workflows, then the pinned run when there is one; the entry `opened` is
/// marked as the object in view.
fn entries(
    view: Option<&ProjectView>,
    tab: Tab,
    opened: Option<&Target>,
    candidate: Option<&Proposed>,
    (live, past): (Option<&LiveRun>, &[LiveRun]),
    (voice, clock): (Voice, &TimeZone),
) -> Vec<(Entry, Target)> {
    if tab != Tab::Nika {
        return Vec::new();
    }
    let mut out = vec![(
        Entry::new(Icon::Conversation, voice.say(THIS_CONVERSATION)),
        Target::Conversation,
    )];
    let mark = |entry: Entry, target: &Target| {
        if opened == Some(target) {
            entry.opened()
        } else {
            entry
        }
    };
    if let Some(candidate) = candidate {
        let entry = Entry::new(Icon::Workflow, candidate.label_in(voice.ascii)).at(1);
        out.push((mark(entry, &Target::Candidate), Target::Candidate));
    }
    // Each launch once, named by when it began ([`LiveRun::label`]) and saying how it ended: a
    // gate that split it in two legs is folded along each leg's `resumed_from`, the later leg
    // listed.
    let legs: Vec<&LiveRun> = live.into_iter().chain(past).collect();
    if let Some(leg) = live {
        let words = format!("{} {}", leg.label_in(voice.lang), leg.workflow());
        let entry = ended(Entry::new(Icon::Run, words).at(1), leg);
        out.push((mark(entry, &Target::Live), Target::Live));
    }
    // The legs observed before it, newest first, by their execution: a leg that never bound
    // one has nothing to reopen and is not listed.
    let listed = (past.iter()).filter(|leg| !continued(leg, &legs));
    for (leg, execution) in listed.filter_map(|leg| Some((leg, leg.execution()?))) {
        let words = (voice.say("{label} {workflow} · earlier"))
            .replace("{workflow}", leg.workflow())
            .replace("{label}", &leg.label_in(voice.lang));
        let target = Target::Past(execution);
        let entry = ended(Entry::new(Icon::Run, words).at(1), leg);
        out.push((mark(entry, &target), target));
    }
    for workflow in view.map_or(&[][..], |v| v.workflows.as_slice()) {
        let target = Target::Workflow(workflow.path.clone());
        // One truth per screen: the last run settled here wins over the check's verdict.
        let verdict = match last_settled(&workflow.path, (live, past)) {
            Some(RunState::Failed) => Verdict::RunFailed,
            _ => workflow.verdict(),
        };
        let entry = Entry::new(Icon::Workflow, &workflow.path).judged(verdict);
        out.push((mark(entry, &target), target.clone()));
        // What its schedule's trigger decided, under it: the runs it fired and its last slot.
        let schedules = view.map_or(&[][..], |v| v.schedules.as_slice());
        if let Some(history) = super::fired::of(schedules, &workflow.path)
            .filter(|history| super::fired::decisions(history) != [0; 4])
        {
            let words = super::fired::listed(history, (voice, clock));
            out.push((Entry::new(Icon::Activation, words).at(1), target));
        }
    }
    if let Some(run) = view.and_then(|v| v.pinned.as_ref()) {
        let target = Target::Run(run.run.clone());
        let entry = Entry::new(Icon::Run, format!("{} {}", run.run, run.workflow));
        out.push((mark(entry, &target), target));
    }
    out
}

/// `entry` with how the run `leg` ended or stands at its end, when that was observed.
fn ended(entry: Entry, leg: &LiveRun) -> Entry {
    match leg.outcome() {
        Some(outcome) => entry.judged(outcome),
        None => entry,
    }
}

/// Whether a later leg listed in `legs` resumed `leg`: one launch a gate split reads as one row,
/// the later leg's (live gauntlet 5, r02 B13: a gate split one launch in two rows).
fn continued(leg: &LiveRun, legs: &[&LiveRun]) -> bool {
    let Some(id) = leg.trace_id() else {
        return false;
    };
    (legs.iter()).any(|other| other.resumed_from() == Some(id.as_str()))
}

/// The state the settlement of the last run of the workflow at `path` observed here reports:
/// the leg in flight first, then the earlier ones, newest first; a leg not settled yet says
/// nothing of it.
fn last_settled(path: &str, (live, past): (Option<&LiveRun>, &[LiveRun])) -> Option<RunState> {
    let same = |asked: &str| asked.trim_start_matches("./") == path.trim_start_matches("./");
    (live.into_iter().chain(past))
        .filter(|leg| same(leg.workflow()))
        .find_map(LiveRun::reported)
}

/// The aside: the `tab` projection of the view and of the conversation's
/// `candidate`, the entry `opened` marked open, and in words what the
/// projection cannot list, said in `voice` with its instants on the machine's
/// `clock`.
#[must_use]
pub(crate) fn aside(
    view: Option<&ProjectView>,
    tab: Tab,
    opened: Option<&Target>,
    candidate: Option<&Proposed>,
    legs: (Option<&LiveRun>, &[LiveRun]),
    (voice, clock): (Voice, &TimeZone),
) -> Aside {
    let listed = entries(view, tab, opened, candidate, legs, (voice, clock))
        .into_iter()
        .map(|(entry, _)| entry)
        .collect();
    let project = view.map_or_else(String::new, |v| v.name.clone());
    let complete = view.is_none_or(|v| v.complete);
    let aside = Aside::new(project, tab, listed, complete).speaking(voice.lang);
    let say = |en: &'static str| voice.say(en);
    match (tab, view) {
        (Tab::Files, _) => aside.noting(say(FILES_NOT_LENT)),
        (_, None) => aside.noting(say("no project is known to this conversation")),
        (_, Some(v)) if v.workflows.is_empty() && v.complete => {
            aside.noting(say("No workflow in {name} yet").replace("{name}", &v.name))
        }
        (_, Some(v)) if v.workflows.is_empty() => {
            aside.noting(say("none found in the listed part"))
        }
        _ => aside,
    }
}

/// What the aside entry at `index` of the `tab` projection opens.
#[must_use]
pub(crate) fn target(
    view: Option<&ProjectView>,
    tab: Tab,
    index: usize,
    candidate: Option<&Proposed>,
    legs: (Option<&LiveRun>, &[LiveRun]),
) -> Option<Target> {
    // What an entry opens does not depend on the words it is named in.
    entries(
        view,
        tab,
        None,
        candidate,
        legs,
        (Voice::default(), &TimeZone::UTC),
    )
    .into_iter()
    .nth(index)
    .map(|(_, target)| target)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use nika_display::state::TaskState;

    use super::*;
    use crate::model::demo_project;
    use crate::workspace::aside::Verdict;

    /// In French the aside names this conversation, the notes and the files
    /// tab's words in French; the workflows' paths and what each entry opens
    /// stay as they are.
    #[test]
    fn the_aside_this_host_lists_speaks_the_persons_language() {
        let french = Voice {
            lang: crate::lang::Lang::Fr,
            ascii: false,
        };
        let demo = demo_project();
        let listed = super::aside(
            Some(&demo),
            Tab::Nika,
            None,
            None,
            (None, &[]),
            (french, &TimeZone::UTC),
        );
        assert_eq!(listed.entries[0].label, "cette conversation");
        let english = aside(Some(&demo), Tab::Nika, None, None, (None, &[]));
        for (french, english) in listed.entries.iter().zip(&english.entries).skip(1) {
            assert_eq!(french.label, english.label, "a workflow's path stays");
        }
        let files = super::aside(
            Some(&demo),
            Tab::Files,
            None,
            None,
            (None, &[]),
            (french, &TimeZone::UTC),
        );
        assert_eq!(
            files.note.as_deref(),
            Some(
                "La liste des fichiers demande un contrat de la Session : l'espace de travail ne lit aucun fichier lui-même."
            )
        );
        let unknown = super::aside(
            None,
            Tab::Nika,
            None,
            None,
            (None, &[]),
            (french, &TimeZone::UTC),
        );
        assert_eq!(
            unknown.note.as_deref(),
            Some("aucun projet n'est connu de cette conversation")
        );
        assert_eq!(
            target(Some(&demo), Tab::Nika, 0, None, (None, &[])),
            Some(Target::Conversation)
        );
    }

    /// The aside in English.
    fn aside(
        view: Option<&ProjectView>,
        tab: Tab,
        opened: Option<&Target>,
        candidate: Option<&Proposed>,
        legs: (Option<&LiveRun>, &[LiveRun]),
    ) -> Aside {
        super::aside(
            view,
            tab,
            opened,
            candidate,
            legs,
            (Voice::default(), &TimeZone::UTC),
        )
    }
    use crate::workspace::header::Manifest;

    fn words(object: &Object) -> Vec<String> {
        match object {
            Object::Welcome { words } => words.clone(),
            Object::Shown { lines, .. } => lines.clone(),
            Object::Workflow { body, .. } => body.iter().map(ToString::to_string).collect(),
            _ => vec!["unsupported object in this fixture".to_owned()],
        }
    }

    #[test]
    fn the_header_names_what_the_session_observed_and_nothing_more() {
        let demo = demo_project();
        let place = place(Some(&demo));
        assert_eq!(place.host, "local");
        assert_eq!(place.project.as_deref(), Some("demo"));
        assert_eq!(place.location.as_deref(), Some("~/Projects/demo"));
        assert_eq!(
            (place.git, place.manifest),
            (Some(true), Some(Manifest::Here))
        );
        let unobserved = super::place(Some(&ProjectView::new("local", "veille", "~/veille")));
        assert_eq!((unobserved.git, unobserved.manifest), (None, None));
        let none = super::place(None);
        assert_eq!((none.host.as_str(), none.project), ("", None));
    }

    #[test]
    fn the_nika_projection_lists_this_conversation_then_the_judged_workflows() {
        let demo = demo_project();
        let enrich = Target::Workflow("enrich.nika".to_owned());
        let aside = aside(Some(&demo), Tab::Nika, Some(&enrich), None, (None, &[]));
        let labels: Vec<(&str, Option<Verdict>, bool)> = aside
            .entries
            .iter()
            .map(|e| (e.label.as_str(), e.verdict, e.open))
            .collect();
        assert_eq!(
            labels,
            [
                ("this conversation", None, false),
                ("release.nika", Some(Verdict::Clean), false),
                ("enrich.nika", Some(Verdict::Findings(2)), true),
                ("flows/weekly-digest.nika", Some(Verdict::Clean), false),
            ]
        );
        assert!(aside.complete && aside.note.is_none());
        assert_eq!(
            target(Some(&demo), Tab::Nika, 0, None, (None, &[])),
            Some(Target::Conversation)
        );
        assert_eq!(
            target(Some(&demo), Tab::Nika, 2, None, (None, &[])),
            Some(Target::Workflow("enrich.nika".to_owned()))
        );
        assert_eq!(target(Some(&demo), Tab::Nika, 4, None, (None, &[])), None);
    }

    /// A leg of `workflow`'s run whose settlement reports `status`.
    fn settled_leg(workflow: &str, status: &str) -> LiveRun {
        let mut leg = LiveRun::asked(workflow.to_owned(), false, true, None);
        let line = format!(
            r#"{{"kind":"run_settled","status":"{status}","cause":"normal","elapsed_ms":2,"execution":{{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"}},"evidence":"unsealed","spend":{{"priced_calls":0,"qualifier":"unmetered","unpriced_calls":0}},"tasks":{{"cancelled":0,"failed":1,"never_started":0,"ok":1,"recovered":0,"skipped":0,"total":2}},"outputs":{{}}}}"#
        );
        leg.apply(nika_display::run_story::RunFrame::decode(&line).expect("a settlement"));
        leg
    }

    /// A leg of `workflow`'s run of execution `uuid`, its start naming the trace it continued
    /// (`from`), settled `status` when one is given.
    fn leg(workflow: &str, uuid: &str, from: Option<&str>, status: Option<&str>) -> LiveRun {
        use nika_display::run_story::RunFrame;
        let mut leg = LiveRun::asked(workflow.to_owned(), from.is_some(), true, None);
        let exec = format!(r#""execution":{{"uuid":"{uuid}"}}"#);
        let link = from.map_or_else(String::new, |from| {
            format!(r#"{{"key":"resumed_from","value":"{from}"}}"#)
        });
        let started = format!(
            r#"{{"correlation":null,{exec},"fields":[{link}],"id":{{"uuid":"01a0ef11-03a7-74fb-bba0-000000000001"}},"kind":"workflow_started","run":null,"timestamp":1}}"#
        );
        leg.apply(RunFrame::decode(&started).expect("a start"));
        if let Some(status) = status {
            let resumed =
                from.map_or_else(String::new, |from| format!(r#","resumed_from":"{from}""#));
            let settled = format!(
                r#"{{"kind":"run_settled","status":"{status}","cause":"normal",{exec},"spend":{{"priced_calls":0,"qualifier":"unmetered","unpriced_calls":0}},"evidence":"unsealed"{resumed}}}"#
            );
            leg.apply(RunFrame::decode(&settled).expect("a settlement"));
        }
        leg
    }

    /// One launch a gate split reads as one row, named by when it began and ending with how it
    /// ended (live gauntlet 5: r02 B13, two rows for one launch; r04 B10, runs named by their
    /// hex, no outcome): the paused leg folds into the leg that resumed it, while that leg runs
    /// (its start names the run it continues) and once it settled; no row names a hex, and
    /// another launch keeps its own row and outcome.
    #[test]
    fn a_launch_a_gate_split_reads_as_one_row_named_by_its_time() {
        let demo = demo_project();
        let paused = "01a148ad-2830-70de-b8b3-99de9427fcc1";
        let from = paused.replace('-', "");
        let runs = |legs: (Option<&LiveRun>, &[LiveRun])| -> Vec<(String, Option<Verdict>)> {
            let aside = aside(Some(&demo), Tab::Nika, None, None, legs);
            (aside.entries.iter())
                .filter(|entry| entry.icon == Icon::Run)
                .map(|entry| (entry.label.clone(), entry.verdict))
                .collect()
        };
        let ran = |label: &str, state| (label.to_owned(), Some(Verdict::Run(state)));
        let other = ran(
            "run 10-16 22:15:00 enrich.nika · earlier",
            TaskState::Failed,
        );
        let past = [
            leg("release.nika", paused, None, Some("paused")),
            leg(
                "enrich.nika",
                "01a146c8-62a0-70de-b8b3-99de9427fcc3",
                None,
                Some("failed"),
            ),
        ];
        let resumed = "01a148b1-75c0-70de-b8b3-99de9427fcc2";
        assert_eq!(
            runs((None, &past)),
            [
                ran(
                    "run 10-17 07:04:30 release.nika · earlier",
                    TaskState::Paused
                ),
                other.clone()
            ]
        );
        let mut running = leg("release.nika", resumed, Some(&from), None);
        running.continuing(&past);
        assert_eq!(
            runs((Some(&running), &past)),
            [
                ran("run 10-17 07:04:30 release.nika", TaskState::Running),
                other.clone()
            ]
        );
        let mut settled = leg("release.nika", resumed, Some(&from), Some("succeeded"));
        settled.continuing(&past);
        assert_eq!(
            runs((Some(&settled), &past)),
            [
                ran("run 10-17 07:04:30 release.nika", TaskState::Ok),
                other.clone()
            ]
        );
        // Behind a newer run, the earlier launch folds the same way and opens its later leg.
        let newer = leg(
            "release.nika",
            "01a148c0-0000-70de-b8b3-99de9427fcc4",
            None,
            None,
        );
        let [first, enrich] = past;
        let past = [settled, first, enrich];
        let listed = runs((Some(&newer), &past));
        assert_eq!(
            listed,
            [
                ran("run 10-17 07:25:04 release.nika", TaskState::Running),
                ran("run 10-17 07:04:30 release.nika · earlier", TaskState::Ok),
                other
            ]
        );
        let execution = past[0].execution().expect("bound");
        assert_eq!(
            target(Some(&demo), Tab::Nika, 2, None, (Some(&newer), &past)),
            Some(Target::Past(execution))
        );
        assert!(
            (listed.iter()).all(|(label, _)| !label.contains("01a148")),
            "{listed:?}"
        );
    }

    /// One truth per screen: a workflow whose last run settled here failed reads
    /// « run failed » in the aside, never the check's `ok`. A newer run that
    /// succeeded gives the check its word back, a newer leg not settled yet
    /// changes nothing, and another workflow's failure is not this one's.
    #[test]
    fn a_workflow_whose_last_run_failed_never_reads_ok() {
        let demo = demo_project();
        let verdict = |legs: (Option<&LiveRun>, &[LiveRun])| {
            let aside = aside(Some(&demo), Tab::Nika, None, None, legs);
            (aside.entries.iter())
                .find(|entry| entry.label == "release.nika")
                .and_then(|entry| entry.verdict)
        };
        assert_eq!(verdict((None, &[])), Some(Verdict::Clean));
        let failed = settled_leg("release.nika", "failed");
        assert_eq!(verdict((Some(&failed), &[])), Some(Verdict::RunFailed));
        let earlier = [settled_leg("./release.nika", "failed")];
        assert_eq!(verdict((None, &earlier)), Some(Verdict::RunFailed));
        let succeeded = settled_leg("release.nika", "succeeded");
        assert_eq!(verdict((Some(&succeeded), &earlier)), Some(Verdict::Clean));
        let asked = LiveRun::asked("release.nika".to_owned(), false, true, None);
        assert_eq!(verdict((Some(&asked), &earlier)), Some(Verdict::RunFailed));
        let other = settled_leg("enrich.nika", "failed");
        assert_eq!(verdict((Some(&other), &[])), Some(Verdict::Clean));
    }

    #[test]
    fn what_cannot_be_listed_is_said_never_left_blank() {
        let demo = demo_project();
        let files = aside(Some(&demo), Tab::Files, None, None, (None, &[]));
        assert!(files.entries.is_empty());
        assert_eq!(files.note.as_deref(), Some(FILES_NOT_LENT));
        assert_eq!(target(Some(&demo), Tab::Files, 0, None, (None, &[])), None);
        let empty = ProjectView::new("local", "veille", "~/veille").listing(Vec::new(), false);
        let nika = aside(Some(&empty), Tab::Nika, None, None, (None, &[]));
        assert_eq!(nika.entries.len(), 1, "this conversation only");
        assert_eq!(nika.note.as_deref(), Some("none found in the listed part"));
        assert!(!nika.complete);
        let fresh = ProjectView::new("local", "veille", "~/veille").listing(Vec::new(), true);
        assert_eq!(
            aside(Some(&fresh), Tab::Nika, None, None, (None, &[]))
                .note
                .as_deref(),
            Some("No workflow in veille yet")
        );
        let unknown = aside(None, Tab::Nika, None, None, (None, &[]));
        assert_eq!(
            unknown.note.as_deref(),
            Some("no project is known to this conversation")
        );
    }

    #[test]
    fn welcome_explains_the_path_without_repeating_inventory_model_or_keys() {
        let demo = demo_project();
        let welcome_words = words(&welcome(Some(&demo), false));
        for expected in [
            crate::visual::logomark::WORDMARK,
            "1  Describe the outcome in the conversation.",
            "2  Answer questions; inspect the proposed plan.",
            "3  Save, then Run with the workflow's models.",
        ] {
            assert!(
                welcome_words.iter().any(|word| word == expected),
                "{expected}"
            );
        }
        for repeated in ["3 workflows", "To prepare", "Ways:", "/intelligence:", "F6"] {
            assert!(
                !welcome_words.iter().any(|word| word.contains(repeated)),
                "the welcome repeats another region's information: {repeated}"
            );
        }
        assert!(
            words(&welcome(Some(&demo), true))
                .iter()
                .all(|word| word.is_ascii())
        );
        assert!(
            words(&welcome(None, false))
                .iter()
                .any(|word| word == "no project is known to this conversation")
        );
        let thread = thread(Some(&demo), Some("a.nika"));
        assert_eq!(thread.intelligence, demo.seat);
        assert_eq!(thread.on_screen.as_deref(), Some("a.nika"));
    }

    /// The welcome once offered « Read orders.csv … » in a project that had no such file; the
    /// example it offers now carries its own records, and it names no input file at all. It
    /// names the Session's own welcome demo, the one « l'exemple de l'écran d'accueil » is given.
    #[test]
    fn the_welcome_example_brings_its_own_data_and_names_no_absent_file() {
        let welcome_words = words(&welcome(None, false));
        let offered = format!("Try: {}", nika_session::WELCOME_DEMO);
        assert!(welcome_words.contains(&offered), "{welcome_words:?}");
        assert_eq!(DEMO, nika_session::WELCOME_DEMO);
        for word in &welcome_words {
            for extension in [".csv", ".json", ".md", ".txt", ".nika"] {
                assert!(!word.contains(extension), "a file is offered: {word}");
            }
        }
    }

    #[test]
    fn empty_and_partial_inventory_stays_in_the_aside() {
        let fresh = ProjectView::new("local", "veille", "~/veille").listing(Vec::new(), true);
        let fresh_words = words(&welcome(Some(&fresh), false));
        assert!(!fresh_words.iter().any(|word| word.contains("No workflow")));
        assert_eq!(
            aside(Some(&fresh), Tab::Nika, None, None, (None, &[]))
                .note
                .as_deref(),
            Some("No workflow in veille yet")
        );
        let cut = ProjectView::new("local", "veille", "~/veille").listing(Vec::new(), false);
        assert!(
            words(&welcome(Some(&cut), true))
                .iter()
                .all(|word| !word.contains("listing partial"))
        );
        assert!(!aside(Some(&cut), Tab::Nika, None, None, (None, &[])).complete);
    }

    #[test]
    fn an_opened_workflow_shows_its_judgement_never_its_bytes() {
        let demo = demo_project();
        let enrich = demo.workflow("enrich.nika").expect("listed");
        let object = workflow_object(enrich);
        assert!(matches!(&object, Object::Shown { name, .. } if name == "enrich"));
        assert_eq!(
            words(&object)[..4],
            [
                "path   enrich.nika",
                "name   enrich",
                "tasks  3",
                "check  2 findings, as checked when this session opened",
            ]
        );
        let unparsed = WorkflowView::new("broken.nika", None, false, 0, 0);
        let object = workflow_object(&unparsed);
        assert!(matches!(&object, Object::Shown { name, .. } if name == "broken.nika"));
        assert_eq!(words(&object)[1], "name   not parsed");
        assert!(words(&object)[3].starts_with("check  not clean"));
    }

    #[test]
    fn the_thread_names_the_project_and_what_is_on_screen() {
        let demo = demo_project();
        let thread = thread(Some(&demo), Some("enrich.nika"));
        assert_eq!(thread.project, "demo");
        assert_eq!(thread.name, THIS_CONVERSATION);
        assert_eq!(thread.on_screen.as_deref(), Some("enrich.nika"));
        assert!(thread.attached.is_empty(), "opening never attaches");
    }

    /// A run is listed only while the view pins one; opened, it shows the
    /// facts it was pinned with, and it resolves only while still pinned.
    #[test]
    fn a_pinned_run_is_listed_and_opened_only_while_pinned() {
        let demo = demo_project();
        let run = Target::Run("#1".to_owned());
        assert!(
            !aside(Some(&demo), Tab::Nika, None, None, (None, &[]))
                .entries
                .iter()
                .any(|e| e.icon == Icon::Run)
        );
        assert_eq!(resolve(Some(&demo), Some(&run), None, (None, &[])), None);
        let pinned = demo.pinning(
            Pinned::new(
                "demo",
                "digest-notes.nika",
                "#1",
                TaskState::Paused,
                "waiting for your answer",
            )
            .offering("answer the gate"),
        );
        let listed = aside(Some(&pinned), Tab::Nika, Some(&run), None, (None, &[]));
        let last = listed.entries.last().expect("the run");
        assert_eq!(
            (last.icon, last.label.as_str(), last.open),
            (Icon::Run, "#1 digest-notes.nika", true)
        );
        assert_eq!(
            target(Some(&pinned), Tab::Nika, 4, None, (None, &[])),
            Some(run.clone())
        );
        let opened = resolve(Some(&pinned), Some(&run), None, (None, &[])).expect("pinned");
        assert_eq!(opened.label(Voice::default()), "#1 digest-notes.nika");
        assert_eq!(
            words(&opened.object(true)),
            [
                "workflow  digest-notes.nika",
                "project   demo",
                "state     waiting for your answer",
                "next      answer the gate - in the conversation",
            ]
        );
        let enrich = Target::Workflow("enrich.nika".to_owned());
        assert_eq!(
            resolve(Some(&pinned), Some(&enrich), None, (None, &[]))
                .map(|opened| opened.label(Voice::default()))
                .as_deref(),
            Some("enrich.nika")
        );
        assert_eq!(resolve(None, Some(&enrich), None, (None, &[])), None);
    }
}
