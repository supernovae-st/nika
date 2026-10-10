// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The project a conversation lends the workspace
//! ([`crate::model::Conversation::project`]), routed: what an aside entry
//! opens (`Target`) and the object it resolves to (`Opened`), the live
//! run and the candidate included. The project's data and its pure
//! projections (the header, the conversation panel, the welcome, the workflow
//! and run objects) live in [`nika_tui_view::workspace::project`] (ADR-143);
//! this path keeps the renderer's callers unchanged.

use super::aside::{Aside, Entry, Tab};
use super::candidate::Proposed;
use super::live::LiveRun;
use super::object::Object;
use super::pinned::Pinned;
use crate::visual::icon::Icon;
use nika_display::run_story::ExecutionId;
#[cfg(test)]
pub(crate) use nika_tui_view::workspace::project::DEMO;
use nika_tui_view::workspace::project::{FILES_NOT_LENT, THIS_CONVERSATION};
pub use nika_tui_view::workspace::project::{ProjectView, WorkflowView};
pub(crate) use nika_tui_view::workspace::project::{place, thread, welcome};
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
    /// The object in view, as the conversation panel names it on screen.
    #[must_use]
    pub(crate) fn label(self) -> String {
        match self {
            Self::Workflow(workflow) => workflow.path.clone(),
            Self::Run(run) => format!("{} {}", run.run, run.workflow),
            Self::Candidate(candidate) => candidate.label(),
            Self::Live(leg) => format!("{} {}", leg.label(), leg.workflow()),
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
                lines: vec![format!("proposal {}", candidate.id())],
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
) -> Vec<(Entry, Target)> {
    if tab != Tab::Nika {
        return Vec::new();
    }
    let mut out = vec![(
        Entry::new(Icon::Conversation, THIS_CONVERSATION),
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
        let entry = Entry::new(Icon::Workflow, candidate.label()).at(1);
        out.push((mark(entry, &Target::Candidate), Target::Candidate));
    }
    if let Some(leg) = live {
        let entry = Entry::new(Icon::Run, format!("{} {}", leg.label(), leg.workflow())).at(1);
        out.push((mark(entry, &Target::Live), Target::Live));
    }
    // The legs observed before it, newest first, by their execution: a leg that never bound
    // one has nothing to reopen and is not listed.
    for (leg, execution) in past.iter().filter_map(|leg| Some((leg, leg.execution()?))) {
        let words = format!("{} {} · earlier", leg.label(), leg.workflow());
        let target = Target::Past(execution);
        out.push((mark(Entry::new(Icon::Run, words).at(1), &target), target));
    }
    for workflow in view.map_or(&[][..], |v| v.workflows.as_slice()) {
        let target = Target::Workflow(workflow.path.clone());
        let entry = Entry::new(Icon::Workflow, &workflow.path).judged(workflow.verdict());
        out.push((mark(entry, &target), target));
    }
    if let Some(run) = view.and_then(|v| v.pinned.as_ref()) {
        let target = Target::Run(run.run.clone());
        let entry = Entry::new(Icon::Run, format!("{} {}", run.run, run.workflow));
        out.push((mark(entry, &target), target));
    }
    out
}

/// The aside: the `tab` projection of the view and of the conversation's
/// `candidate`, the entry `opened` marked open, and in words what the
/// projection cannot list.
#[must_use]
pub(crate) fn aside(
    view: Option<&ProjectView>,
    tab: Tab,
    opened: Option<&Target>,
    candidate: Option<&Proposed>,
    legs: (Option<&LiveRun>, &[LiveRun]),
) -> Aside {
    let listed = entries(view, tab, opened, candidate, legs)
        .into_iter()
        .map(|(entry, _)| entry)
        .collect();
    let project = view.map_or_else(String::new, |v| v.name.clone());
    let complete = view.is_none_or(|v| v.complete);
    let aside = Aside::new(project, tab, listed, complete);
    match (tab, view) {
        (Tab::Files, _) => aside.noting(FILES_NOT_LENT),
        (_, None) => aside.noting("no project is known to this conversation"),
        (_, Some(v)) if v.workflows.is_empty() && v.complete => {
            aside.noting(format!("No workflow in {} yet", v.name))
        }
        (_, Some(v)) if v.workflows.is_empty() => aside.noting("none found in the listed part"),
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
    entries(view, tab, None, candidate, legs)
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
            "N I K A",
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
    /// example it offers now carries its own records, and it names no input file at all.
    #[test]
    fn the_welcome_example_brings_its_own_data_and_names_no_absent_file() {
        let welcome_words = words(&welcome(None, false));
        assert!(
            welcome_words
                .iter()
                .any(|word| *word == format!("Try: {DEMO}")),
            "{welcome_words:?}"
        );
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
        assert_eq!(opened.label(), "#1 digest-notes.nika");
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
                .map(Opened::label)
                .as_deref(),
            Some("enrich.nika")
        );
        assert_eq!(resolve(None, Some(&enrich), None, (None, &[])), None);
    }
}
