// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The candidate the conversation proposes, as the Live host adapter folded it
//! (the crate-private `session::candidate`) from its Session's borrowed
//! projection (`nika_session::runtime::Candidate`): the identity a consent
//! names, every change a yes would land (what it creates or replaces, where,
//! over which witnessed bytes), what the Session's audit says each audited
//! workflow reaches when it runs (under its own path when it lands several),
//! what a `save & run` of it would run when the Session's typed method admits
//! one, the words of the rehearsal proof bound to that identity, and the look
//! of the exact pending bytes of ONE workflow (their witness, the check
//! facade's verdict of them alone, their graph); the changes whose bytes are
//! not shown are counted, never passed off as shown. Every fact of one
//! [`Proposed`] belongs to one identity: the fields are private, and a new
//! identity is a new fold.
//!
//! The pending bytes are observed once, when the Session proposed them; they
//! are not a file and are never read again from disk. Nothing here reads a
//! file or a clock, and drawing calls nothing here: the shell renders a face
//! before the frame. Opening the candidate grants nothing: the `yes` typed
//! under `Save? ›` is the only consent, and it answers this identity or none.

use nika_display::theme::Role;
use nika_session::ProposalId;
use nika_session::work::DocumentRevision;
use nika_tui_view::Face;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::cards::review::Review;
use super::inspect::{Inspected, title_row};
use super::text::{fit_head, hang, marks, twins, wrap};
use crate::visual::role;

/// The cells each effect after the first stands in under the review's one
/// `when it runs` heading.
const HUNG: usize = 2;

/// How the faces' witness row says a key turns the face.
const FACES: &str = "Left/Right change the face";

/// What a yes answers while a proposal waits: nothing is saved yet and
/// nothing has run.
fn answers(sep: &str) -> String {
    format!("what a yes answers{sep}not saved{sep}nothing has run on your files")
}

/// Whether a component witness names admitted held reuse: `expanded` (its
/// nodes, as bound) and `invoked` (its calling task, as receipted) are two
/// distinct admitted states; `revised`, `absent`, `unreadable` and
/// `unwitnessed` need attention.
pub(crate) fn admitted(witness: &str) -> bool {
    matches!(witness, "expanded" | "invoked")
}

/// Whether a compile record states an ordinary making of its bytes: a
/// revision by operations, or a creation (`written`, `composed`) with no
/// base. A replacement or a record of another kind needs attention.
fn usual(record: &DocumentRevision) -> bool {
    match record.mode.as_str() {
        "operations" => true,
        "written" | "composed" => record.base_sha256.is_none(),
        _ => false,
    }
}

/// The role a fact wears: a `plain` one its own, any other attention.
fn tone(plain: bool, role: Role) -> Role {
    if plain { role } else { Role::Warn }
}

/// What a `save & run` of a candidate runs once its save checked clean, as
/// the Session's typed method admits it (`ProjectChangeSet::save_run`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RunAfter {
    /// The one workflow the candidate saves, at the run admission's own
    /// spending.
    Saved,
    /// The run the request itself carried, in words: its workflow, the
    /// ceiling it states and the names of its inputs, never their values.
    Asked(String),
}

/// The candidate under review, fixed at its fold.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Proposed {
    id: ProposalId,
    aside: bool,
    /// The compiler's draft while its question waits: no proposal yet, nothing to consent.
    draft: bool,
    changes: Vec<String>,
    effects: Option<Vec<String>>,
    /// How its workflow was revised over the complete document and which components it holds,
    /// each with whether it needs attention: `(words, warn)`.
    revision: Vec<(String, bool)>,
    /// The typed compile record the Session binds to these exact bytes, kept
    /// only where it names the shown workflow ([`Self::recording`]).
    record: Option<DocumentRevision>,
    /// Where each audited workflow reaches as declared, and whether that leaves this machine
    /// (or cannot be told): `(words, outside)`.
    world: Vec<(String, bool)>,
    /// What a `save & run` of it runs, when the Session's typed method admits one.
    after: Option<RunAfter>,
    rehearsed: Option<String>,
    unshown: usize,
    look: Inspected,
}

impl PartialEq for Proposed {
    /// Two folds are the same candidate when every fact they show is the
    /// same: the identity, the standing, the changes, the reach, what a
    /// `save & run` would run, the rehearsal, and the bytes at the same path.
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.aside == other.aside
            && self.draft == other.draft
            && self.changes == other.changes
            && self.effects == other.effects
            && self.revision == other.revision
            && self.record == other.record
            && self.world == other.world
            && self.after == other.after
            && self.rehearsed == other.rehearsed
            && self.unshown == other.unshown
            && self.look == other.look
    }
}

impl Eq for Proposed {}

impl Proposed {
    /// The candidate `id` (set `aside` while a revision's question waits)
    /// over the look of its exact pending bytes.
    pub(crate) fn new(id: ProposalId, aside: bool, look: Inspected) -> Self {
        Self {
            id,
            aside,
            draft: false,
            changes: Vec::new(),
            effects: None,
            revision: Vec::new(),
            record: None,
            world: Vec::new(),
            after: None,
            rehearsed: None,
            unshown: 0,
            look,
        }
    }

    /// The compiler's draft while its question waits, before any proposal: set aside, it names
    /// no consent, lands nothing and was neither audited nor rehearsed.
    pub(crate) fn drafted(mut self) -> Self {
        self.aside = true;
        self.draft = true;
        self
    }

    /// With every change it lands, in words.
    pub(crate) fn changing(mut self, changes: Vec<String>) -> Self {
        self.changes = changes;
        self
    }

    /// With what the Session's audit says its workflow reaches when it runs;
    /// `None` when the Session audited no workflow of it (a project file).
    pub(crate) fn reaching(mut self, effects: Option<Vec<String>>) -> Self {
        self.effects = effects;
        self
    }

    /// With how its workflow was revised over the complete document, as the compile record
    /// states it, and whether each row needs attention.
    pub(crate) fn revising(mut self, revision: Vec<(String, bool)>) -> Self {
        self.revision = revision;
        self
    }

    /// With the typed compile record bound to its shown workflow's bytes:
    /// the check face details it ([`Self::reuse`]).
    pub(crate) fn recording(mut self, record: Option<DocumentRevision>) -> Self {
        self.record = record;
        self
    }

    /// With where each audited workflow reaches as the check declares it, and whether that
    /// leaves this machine (or cannot be told).
    pub(crate) fn declaring(mut self, world: Vec<(String, bool)>) -> Self {
        self.world = world;
        self
    }

    /// With what a `save & run` of it runs once its save checked clean, as
    /// the Session's typed method admits it (`None`: the method refuses).
    pub(crate) fn running(mut self, after: Option<RunAfter>) -> Self {
        self.after = after;
        self
    }

    /// With how many of its changes have bytes the faces do not show.
    pub(crate) fn unshown(mut self, changes: usize) -> Self {
        self.unshown = changes;
        self
    }

    /// With the words of the rehearsal proof bound to its identity.
    pub(crate) fn rehearsed(mut self, words: Option<String>) -> Self {
        self.rehearsed = words;
        self
    }

    /// The identity a consent names.
    #[must_use]
    pub fn id(&self) -> &ProposalId {
        &self.id
    }

    /// Whether a revision's question set it aside (not consentable meanwhile).
    #[must_use]
    pub fn aside(&self) -> bool {
        self.aside
    }

    /// Whether it is the compiler's draft at a question rather than a proposal.
    #[must_use]
    pub fn draft(&self) -> bool {
        self.draft
    }

    /// Where its workflow would land, relative to the project's root.
    #[must_use]
    pub fn path(&self) -> &str {
        self.look.path()
    }

    /// The witness of its exact pending bytes (blake3, hex).
    #[must_use]
    pub fn witness(&self) -> Option<&str> {
        self.look.witness()
    }

    /// The look of its pending bytes (kept when the next fold has the same).
    pub(crate) fn look(&self) -> &Inspected {
        &self.look
    }

    /// First preview: the observed graph when available, otherwise the exact source.
    pub(crate) fn initial_face(&self) -> Face {
        if self.look.graph().is_some() {
            Face::Graph
        } else {
            Face::Source
        }
    }

    /// How the aside and the conversation panel name it. A draft lands
    /// nowhere yet: it takes the title its object shows, never the path it
    /// holds until a proposal says where, and says it is unsaved without one.
    #[must_use]
    pub fn label(&self) -> String {
        if !self.draft {
            return format!("proposal {}", self.path());
        }
        let title = self.look.title();
        if title == self.path() {
            "draft (unsaved)".to_owned()
        } else {
            format!("draft {title}")
        }
    }

    /// The conversation's review of it while a consent can name it (neither
    /// the compiler's draft nor set aside): only what changes the decision,
    /// every change it lands, how it was revised, the run a `save & run`
    /// would ask when the request carried one, what the workflow reaches
    /// when it runs (one `when it runs` heading, each further effect hung
    /// under it) and where, and its rehearsal. Every fact once, each with its
    /// role; the identity a consent names rides the card's border, and the
    /// standing has the status row.
    #[must_use]
    pub(crate) fn review(&self, ascii: bool) -> Option<Review> {
        if self.draft || self.aside {
            return None;
        }
        let (sep, _) = marks(ascii);
        let runs = self.after.is_some();
        Some(Review::new(self.id.clone(), self.facts(sep, true), runs))
    }

    /// The facts above every face: the identity and what a yes answers, the
    /// changes, what the workflow reaches, the rehearsal; each with its role.
    fn head(&self, ascii: bool) -> Vec<(String, Role)> {
        let (sep, _) = marks(ascii);
        if self.draft {
            // Before any proposal: no identity, no audit, no rehearsal to state.
            let first = format!("draft{sep}the compiler's question waits{sep}not a proposal yet");
            return vec![(first, Role::Strong)];
        }
        let id = self.id.to_string();
        let standing = if self.aside {
            format!("set aside while the revision's question waits{sep}not consentable now")
        } else {
            answers(sep)
        };
        let mut rows = vec![(format!("proposal {id}{sep}{standing}"), Role::Strong)];
        let facts = self.facts(sep, false).into_iter();
        rows.extend(facts.map(|(words, role, _)| (words, role)));
        rows
    }

    /// The facts under the standing, in their order, each with its role and
    /// the cells its first row stands in: every change, the changes whose
    /// bytes no face shows, how the workflow was revised, the run its own
    /// request carried, what it reaches when it runs (`grouped`: each effect
    /// after the first hung under one `when it runs` heading), where it
    /// reaches as declared, and its rehearsal.
    fn facts(&self, sep: &str, grouped: bool) -> Vec<(String, Role, usize)> {
        let mut rows: Vec<(String, Role, usize)> = Vec::new();
        rows.extend(self.changes.iter().map(|c| (c.clone(), Role::Accent, 0)));
        if self.unshown > 0 {
            rows.push((
                format!(
                    "{} more change(s) whose bytes these faces do not show{sep}`/show` prints every byte",
                    self.unshown
                ),
                Role::Warn,
                0,
            ));
        }
        rows.extend(self.revision.iter().map(|(words, warn)| {
            let role = if *warn { Role::Warn } else { Role::Dim };
            (words.clone(), role, 0)
        }));
        if let Some(RunAfter::Asked(asked)) = &self.after {
            rows.push((asked.clone(), Role::Accent, 0));
        }
        match self.effects.as_deref() {
            None => rows.push((
                format!("when it runs{sep}not a workflow the Session audited: it runs nothing"),
                Role::Dim,
                0,
            )),
            Some([]) => rows.push((
                format!("when it runs{sep}nothing outside the process"),
                Role::Dim,
                0,
            )),
            Some(effects) => {
                for (at, effect) in effects.iter().enumerate() {
                    let row = if grouped && at > 0 {
                        (effect.clone(), Role::Dim, HUNG)
                    } else {
                        (format!("when it runs{sep}{effect}"), Role::Dim, 0)
                    };
                    rows.push(row);
                }
            }
        }
        rows.extend(self.world.iter().map(|(words, outside)| {
            let role = if *outside { Role::Warn } else { Role::Dim };
            (format!("reaches, as declared{sep}{words}"), role, 0)
        }));
        match &self.rehearsed {
            Some(words) => rows.extend(
                words
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| (format!("rehearsal{sep}{}", l.trim()), Role::Dim, 0)),
            ),
            None => rows.push((
                format!("rehearsal{sep}none bound to this identity"),
                Role::Dim,
                0,
            )),
        }
        rows
    }

    /// One face of the candidate on a region `width` cells wide: its title
    /// row, the facts above every face (wrapped, never cut), the witness of
    /// the pending bytes, then the viewer's face of those bytes. Pure.
    #[must_use]
    pub fn face_lines(
        &self,
        face: Face,
        width: u16,
        ascii: bool,
        color: bool,
    ) -> (Line<'static>, Vec<Line<'static>>) {
        self.face_lines_in(face, width, ascii, color, false)
    }

    /// Short previews keep exact graph rows beside the same proposal facts.
    pub(crate) fn face_lines_in(
        &self,
        face: Face,
        width: u16,
        ascii: bool,
        color: bool,
        compact: bool,
    ) -> (Line<'static>, Vec<Line<'static>>) {
        self.face_with(self.head(ascii), face, width, ascii, color, compact)
    }

    /// The face while the conversation reviews this candidate
    /// ([`Self::review`]): the review is the one home of the facts above
    /// every face and the object's header the one home of its bytes, so the
    /// title names the workflow, the first row says whose exact bytes these
    /// are (the key that turns the face where it fits whole), and the face
    /// follows with no second witness row.
    pub(crate) fn reviewed_face_lines(
        &self,
        face: Face,
        width: u16,
        ascii: bool,
        color: bool,
        compact: bool,
    ) -> (Line<'static>, Vec<Line<'static>>) {
        let (title, mut body) = self.face_with(Vec::new(), face, width, ascii, color, compact);
        let (sep, cut) = marks(ascii);
        let (own, cells) = (self.own_bytes(), usize::from(width));
        let said = format!("{own}{sep}{FACES}");
        let dim = role::style(Role::Dim, color);
        // The face's own witness row, exactly as it is painted, gives way to the first row.
        let witness = Line::from(Span::styled(fit_head(&said, cells, cut), dim));
        if let Some(at) = body.iter().position(|line| *line == witness) {
            body.remove(at);
        }
        let first = [said, own.clone()]
            .into_iter()
            .find(|row| row.width() <= cells)
            .unwrap_or_else(|| fit_head(&own, cells, cut));
        body.insert(0, Line::from(Span::styled(first, dim)));
        (title, body)
    }

    /// Whose exact pending bytes the faces show: their witness, the
    /// candidate's own.
    fn own_bytes(&self) -> String {
        let kind = if self.draft { "draft" } else { "proposal" };
        let short: String = self.witness().unwrap_or("").chars().take(12).collect();
        format!("these bytes {short}, the {kind}'s own")
    }

    /// The title row, `facts` (wrapped, never cut), the witness of the
    /// pending bytes, then the viewer's face of those bytes. The title names
    /// the workflow alone, the first row standing for what it is; a draft
    /// says it is one, never where it would land. Pure.
    fn face_with(
        &self,
        facts: Vec<(String, Role)>,
        face: Face,
        width: u16,
        ascii: bool,
        color: bool,
        compact: bool,
    ) -> (Line<'static>, Vec<Line<'static>>) {
        let (sep, cut) = marks(ascii);
        let name = if self.draft {
            format!("draft{sep}{}", self.look.title())
        } else {
            self.look.title().to_owned()
        };
        let title = title_row(&name, face, width, ascii, color);
        let mut body = Vec::new();
        for (row, tone) in facts {
            for part in wrap(&twins(&row, ascii), usize::from(width), cut) {
                body.push(Line::from(Span::styled(part, role::style(tone, color))));
            }
        }
        let said = format!("{}{sep}{FACES}", self.own_bytes());
        body.extend(
            self.look
                .judged_lines_in(face, width, ascii, color, &said, compact),
        );
        if face == Face::Check {
            body.extend(self.reuse(usize::from(width), ascii, color));
        }
        (title, body)
    }

    /// The compile record bound to these bytes, as one detail of the check
    /// face: how they were made, then each admitted component they hold, what
    /// these bytes show of it now ([`admitted`] or attention), its version,
    /// its release and admitted-file digests and its bindings. Typed facts
    /// only, each row whole ([`hang`]) in the glyph column in use.
    fn reuse(&self, width: usize, ascii: bool, color: bool) -> Vec<Line<'static>> {
        let Some(record) = &self.record else {
            return Vec::new();
        };
        let short = |sha: &str| sha.chars().take(12).collect::<String>();
        let (mode, these) = (&record.mode, short(&record.candidate_sha256));
        let mut making = Vec::new();
        if let Some(base) = &record.base_sha256 {
            making.push(format!("over the base, sha256 {}", short(base)));
        }
        if !record.changed.is_empty() {
            making.push(format!("changed · {}", record.changed.join(", ")));
        }
        if !record.preservation.is_empty() {
            making.push(format!("preservation · {}", record.preservation));
        }
        let heading = format!("compile record · {mode} · binds these bytes, sha256 {these}");
        let mut sections = vec![(heading, tone(usual(record), Role::Strong), making)];
        for component in &record.components {
            let version = component.version.as_deref().unwrap_or("unversioned");
            let (id, witness) = (&component.id, &component.witness);
            let (release, file) = (&component.release, &component.file_sha256);
            let pairs = [("release", release), ("file", file)];
            let digests: Vec<String> = (pairs.into_iter())
                .filter_map(|(label, sha)| Some(format!("{label} {}", short(sha.as_ref()?))))
                .collect();
            // The version stands on its own row: a release name fits whole there.
            let mut facts = vec![version.to_owned()];
            if !digests.is_empty() {
                facts.push(digests.join(" · "));
            }
            let bound = component.bindings.iter();
            facts.extend(bound.map(|b| format!("{} = {}", b.path, b.value)));
            let worn = tone(admitted(witness), Role::Good);
            sections.push((format!("{id} · {witness}"), worn, facts));
        }
        let mut lines = vec![Line::default()];
        let dim = role::style(Role::Dim, color);
        for (title, worn, rows) in sections {
            let style = role::style(worn, color);
            let hung = hang(&twins(&title, ascii), 0, width);
            lines.extend(hung.into_iter().map(|row| Line::styled(row, style)));
            let below = rows.iter().flat_map(|r| hang(&twins(r, ascii), 2, width));
            lines.extend(below.map(|row| Line::styled(row, dim)));
        }
        lines
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
