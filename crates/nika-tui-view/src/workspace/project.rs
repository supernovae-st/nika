// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The project a conversation lends the workspace: a read-only view of what
//! its Session observed when it opened, and the pure projections of that view
//! into the header ([`Place`]), the conversation panel ([`Thread`]) and the
//! object in view ([`Object`]). Nothing here reads a file, the environment or
//! a clock. An absent fact is named, never guessed: no git, no `nika.yaml` or
//! a refused one, a partial listing, no intelligence chosen yet, no project at
//! all.
//!
//! Opening a workflow shows what the Session judged of it (its path, its name,
//! its tasks, the checker's verdict) without reading the file, and attaches
//! nothing to the next message. The source, the graph and the proof belong to
//! the viewers; [`workflow_object`] is the seam they replace. What an aside
//! entry opens stays with the renderer, which holds the live run.

use nika_session::Lang;

use super::aside::Verdict;
use super::conversation::Thread;
use super::header::{Manifest, Place};
use super::lang::say;
use super::object::Object;
use super::pinned::Pinned;
use super::text::marks;
use crate::visual::icon::Icon;
use crate::visual::logomark::WORDMARK;

/// The name the workspace gives the one conversation a Session holds: no
/// catalogue of conversations is lent yet, so none is named for it.
pub const THIS_CONVERSATION: &str = "this conversation";

/// What the Files projection says while no Session lends a file listing.
pub const FILES_NOT_LENT: &str =
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
    pub fn verdict(&self) -> Verdict {
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
    /// What the trigger of each schedule `nika.yaml` declares decided, as the Session's
    /// arm-ledger reader says it (`SessionRuntime::schedule_history`, read only): empty
    /// without one.
    pub schedules: Vec<nika_session::ScheduleHistory>,
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
            schedules: Vec::new(),
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

    /// This view with what the trigger of each declared schedule decided.
    #[must_use]
    pub fn scheduling(mut self, schedules: Vec<nika_session::ScheduleHistory>) -> Self {
        self.schedules = schedules;
        self
    }

    /// The listed workflow at `path`.
    #[must_use]
    pub fn workflow(&self, path: &str) -> Option<&WorkflowView> {
        self.workflows.iter().find(|w| w.path == path)
    }
}

/// The header: where the view stands, or no project when none is lent.
#[must_use]
pub fn place(view: Option<&ProjectView>) -> Place {
    let Some(view) = view else {
        return Place::on("");
    };
    let mut place = Place::on(view.host.clone()).with_project(&view.name, &view.location);
    place.git = view.git;
    place.manifest.clone_from(&view.manifest);
    place
}

/// The conversation panel's thread, with the object `on_screen` consulted
/// (never attached).
#[must_use]
pub fn thread(view: Option<&ProjectView>, on_screen: Option<&str>) -> Thread {
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
/// a line, it reaches the Session like any other request. It is the Session's
/// own [`nika_session::WELCOME_DEMO`], the one it gives a person asking for
/// « l'exemple de l'écran d'accueil »: the screen and that answer cannot drift.
pub const DEMO: &str = nika_session::WELCOME_DEMO;

/// The welcome explains the work once. The aside owns inventory, the
/// conversation owns intelligence/context, and its hint owns the keys. The
/// example it offers brings its own data; a request about the user's own
/// files names them.
#[must_use]
pub fn welcome(view: Option<&ProjectView>, ascii: bool) -> Object {
    welcome_in(view, ascii, Lang::En)
}

/// [`welcome`] in `lang`, the person's language once the Session reads it (live gauntlet 5: the
/// welcome stayed English beside a French conversation), in the glyph column in use; the
/// example's name stays as it is.
#[must_use]
pub fn welcome_in(view: Option<&ProjectView>, ascii: bool, lang: Lang) -> Object {
    let said = |en: &str| say(lang, ascii, en).to_owned();
    let mut words = vec![
        WORDMARK.to_owned(),
        said("Turn an intention into a workflow."),
        String::new(),
        said("1  Describe the outcome in the conversation."),
        said("2  Answer questions; inspect the proposed plan."),
        said("3  Save, then Run with the workflow's models."),
        String::new(),
        said("Try: {demo}").replace("{demo}", DEMO),
        said("a demo with its own records: totals per"),
        said("region, you name the currency."),
        said("For your own data, name the files to read."),
        String::new(),
    ];
    if view.is_none() {
        words.push(said("no project is known to this conversation"));
    }
    Object::Welcome { words }
}

/// An opened workflow as the object in view: what the Session judged of it,
/// never its bytes. The viewers (graph, source, proof) replace this
/// projection, reading through the Session, never through the workspace.
#[must_use]
pub fn workflow_object(workflow: &WorkflowView) -> Object {
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
pub fn run_object(run: &Pinned, ascii: bool) -> Object {
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
mod welcome_tests {
    use super::*;

    /// The words of `object`, a welcome.
    fn words(object: &Object) -> Vec<String> {
        match object {
            Object::Welcome { words } => words.clone(),
            _ => Vec::new(),
        }
    }

    /// The welcome speaks the person's language once the Session reads it (live gauntlet 5:
    /// it stayed English beside a French conversation): its line, its three steps, the example
    /// (whose name stays) and the absence of a project, the accents folded in the ASCII column;
    /// English stays the default.
    #[test]
    fn the_welcome_speaks_the_persons_language() {
        let french = words(&welcome_in(None, false, Lang::Fr));
        for line in [
            "Transformez une intention en workflow.",
            "1  Décrivez le résultat voulu dans la conversation.",
            "2  Répondez aux questions ; examinez le plan proposé.",
            "3  Enregistrez, puis lancez avec les modèles du workflow.",
            "aucun projet n'est connu de cette conversation",
        ] {
            assert!(french.iter().any(|said| said == line), "{line}: {french:?}");
        }
        assert!(french.contains(&format!("Essayez : {DEMO}")), "{french:?}");
        let plain = words(&welcome_in(None, true, Lang::Fr));
        assert!(
            plain.contains(&"1  Decrivez le resultat voulu dans la conversation.".to_owned()),
            "{plain:?}"
        );
        assert_eq!(welcome(None, false), welcome_in(None, false, Lang::En));
        assert!(
            words(&welcome(None, false)).contains(&"Turn an intention into a workflow.".to_owned())
        );
    }
}
