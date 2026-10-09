// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The project a conversation lends the workspace
//! ([`crate::model::Conversation::project`]): a read-only view of what its
//! Session observed when it opened, and the pure projections of that view into
//! the header ([`Place`]), the aside ([`Aside`]), the conversation panel
//! ([`Thread`]) and the object in view ([`Object`]). Nothing here reads a
//! file, the environment or a clock. An absent fact is named, never guessed:
//! no git, no `nika.yaml` or a refused one, a partial listing, no intelligence
//! chosen yet, no project at all.
//!
//! Opening a workflow shows what the Session judged of it (its path, its name,
//! its tasks, the checker's verdict) without reading the file, and attaches
//! nothing to the next message. The source, the graph and the proof belong to
//! the viewers; the crate-private `workflow_object` is the seam they replace.

use super::aside::{Aside, Entry, Tab, Verdict};
use super::candidate::Proposed;
use super::conversation::Thread;
use super::header::{Manifest, Place};
use super::live::LiveRun;
use super::object::Object;
use super::pinned::Pinned;
use super::text::marks;
use crate::visual::icon::Icon;
use nika_display::run_story::ExecutionId;

/// The name the workspace gives the one conversation a Session holds: no
/// catalogue of conversations is lent yet, so none is named for it.
pub(crate) const THIS_CONVERSATION: &str = "this conversation";

/// What the Files projection says while no Session lends a file listing.
pub(crate) const FILES_NOT_LENT: &str =
    "The file listing needs a Session contract: the workspace reads no file itself.";

/// One workflow the Session's bounded walk listed, with the checker's verdict.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct WorkflowView {
    /// The path relative to the project's root.
    pub path: String,
    /// The file's own name (`nika:`), when it parsed.
    pub name: Option<String>,
    /// The installed checker's verdict when the Session looked.
    pub clean: bool,
    /// The findings the checker counted.
    pub findings: usize,
    /// The tasks the file declares.
    pub tasks: usize,
}

impl WorkflowView {
    /// A workflow at `path`, named `name` when it parsed, as judged.
    #[must_use]
    pub fn new(
        path: impl Into<String>,
        name: Option<&str>,
        clean: bool,
        findings: usize,
        tasks: usize,
    ) -> Self {
        Self {
            path: path.into(),
            name: name.map(str::to_owned),
            clean,
            findings,
            tasks,
        }
    }

    /// The checker's verdict.
    #[must_use]
    pub(crate) fn verdict(&self) -> Verdict {
        if self.clean {
            Verdict::Clean
        } else {
            Verdict::Findings(self.findings)
        }
    }
}

/// The project a conversation stands in, as its Session observed it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProjectView {
    /// Where the session runs (`local`: in this process, on this machine).
    pub host: String,
    /// The project's name (its root's own name).
    pub name: String,
    /// The root as the human reads it (home-relative when under the home).
    pub location: String,
    /// Whether a Git work tree holds the root; `None` when not observed.
    pub git: Option<bool>,
    /// What governs the root; `None` when not observed.
    pub manifest: Option<Manifest>,
    /// The workflows the Session's bounded walk listed, in its order.
    pub workflows: Vec<WorkflowView>,
    /// Whether the listing is complete: `false` when the walk or the list was
    /// cut, and every count is then a lower bound.
    pub complete: bool,
    /// The intelligence the session reasons with, in words; `None` while
    /// none was chosen.
    pub seat: Option<String>,
    /// The run that asks for attention, when one really does.
    pub pinned: Option<Pinned>,
}

impl ProjectView {
    /// The project `name` at `location` on `host`: nothing observed about
    /// it yet, no workflow listed (a listing not lent is not complete).
    #[must_use]
    pub fn new(
        host: impl Into<String>,
        name: impl Into<String>,
        location: impl Into<String>,
    ) -> Self {
        Self {
            host: host.into(),
            name: name.into(),
            location: location.into(),
            git: None,
            manifest: None,
            workflows: Vec::new(),
            complete: false,
            seat: None,
            pinned: None,
        }
    }

    /// This view with the observed Git fact.
    #[must_use]
    pub fn with_git(mut self, git: bool) -> Self {
        self.git = Some(git);
        self
    }

    /// This view governed by `manifest`.
    #[must_use]
    pub fn governed(mut self, manifest: Manifest) -> Self {
        self.manifest = Some(manifest);
        self
    }

    /// This view listing `workflows`, `complete` or partial.
    #[must_use]
    pub fn listing(mut self, workflows: Vec<WorkflowView>, complete: bool) -> Self {
        self.workflows = workflows;
        self.complete = complete;
        self
    }

    /// This view with the intelligence the session reasons with.
    #[must_use]
    pub fn seated(mut self, seat: impl Into<String>) -> Self {
        self.seat = Some(seat.into());
        self
    }

    /// This view with `run` pinned.
    #[must_use]
    pub fn pinning(mut self, run: Pinned) -> Self {
        self.pinned = Some(run);
        self
    }

    /// The listed workflow at `path`.
    #[must_use]
    pub(crate) fn workflow(&self, path: &str) -> Option<&WorkflowView> {
        self.workflows.iter().find(|w| w.path == path)
    }
}

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

/// The header: where the view stands, or no project when none is lent.
#[must_use]
pub(crate) fn place(view: Option<&ProjectView>) -> Place {
    let Some(view) = view else {
        return Place::on("");
    };
    let mut place = Place::on(view.host.clone()).with_project(&view.name, &view.location);
    place.git = view.git;
    place.manifest.clone_from(&view.manifest);
    place
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

/// The conversation panel's thread, with the object `on_screen` consulted
/// (never attached).
#[must_use]
pub(crate) fn thread(view: Option<&ProjectView>, on_screen: Option<&str>) -> Thread {
    let project = view.map_or_else(String::new, |v| v.name.clone());
    let thread =
        Thread::new(project, THIS_CONVERSATION).seated(view.and_then(|view| view.seat.clone()));
    match on_screen {
        Some(object) => thread.viewing(object),
        None => thread,
    }
}

/// The demo the welcome offers: an exact skeleton of the pack whose records
/// travel inside the example, so following it needs no file of the user's, no
/// service and no model; its one open value (the currency) is asked. Typed as
/// a line, it reaches the Session like any other request.
pub(crate) const DEMO: &str = "aggregate-by-key";

/// The welcome explains the work once. The aside owns inventory, the
/// conversation owns intelligence/context, and its hint owns the keys. The
/// example it offers brings its own data; a request about the user's own
/// files names them.
#[must_use]
pub(crate) fn welcome(view: Option<&ProjectView>, _ascii: bool) -> Object {
    let mut words = vec![
        "N I K A".to_owned(),
        "Turn an intention into a workflow.".to_owned(),
        String::new(),
        "1  Describe the outcome in the conversation.".to_owned(),
        "2  Answer questions; inspect the proposed plan.".to_owned(),
        "3  Save, then Run with the workflow's models.".to_owned(),
        String::new(),
        format!("Try: {DEMO}"),
        "a demo with its own records: totals per".to_owned(),
        "region, you name the currency.".to_owned(),
        "For your own data, name the files to read.".to_owned(),
        String::new(),
    ];
    if view.is_none() {
        words.push("no project is known to this conversation".to_owned());
    }
    Object::Welcome { words }
}

/// An opened workflow as the object in view: what the Session judged of it,
/// never its bytes. The viewers (graph, source, proof) replace this
/// projection, reading through the Session, never through the workspace.
#[must_use]
pub(crate) fn workflow_object(workflow: &WorkflowView) -> Object {
    let when = "as checked when this session opened";
    let check = match workflow.verdict() {
        Verdict::Clean => format!("ok, {when}"),
        other => format!("{}, {when}", other.words()),
    };
    let lines = vec![
        format!("path   {}", workflow.path),
        format!(
            "name   {}",
            workflow.name.as_deref().unwrap_or("not parsed")
        ),
        format!("tasks  {}", workflow.tasks),
        format!("check  {check}"),
        String::new(),
        "The source is not shown: the workspace reads no file itself.".to_owned(),
    ];
    Object::Shown {
        icon: Icon::Workflow,
        name: workflow
            .name
            .clone()
            .unwrap_or_else(|| workflow.path.clone()),
        lines,
    }
}

/// The pinned run as the object in view: the facts it was pinned with, in
/// the Session's words. The run viewer (tasks, attempts, costs, the gate)
/// replaces this projection.
#[must_use]
pub(crate) fn run_object(run: &Pinned, ascii: bool) -> Object {
    let (sep, _) = marks(ascii);
    let mut lines = vec![
        format!("workflow  {}", run.workflow),
        format!("project   {}", run.project),
        format!("state     {}", run.words),
    ];
    if let Some(action) = &run.action {
        lines.push(format!("next      {action}{sep}in the conversation"));
    }
    Object::Shown {
        icon: Icon::Run,
        name: format!("{} {}", run.run, run.workflow),
        lines,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use nika_display::state::TaskState;

    use super::*;
    use crate::model::demo_project;

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
