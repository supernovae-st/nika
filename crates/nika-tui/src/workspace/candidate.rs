// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The candidate the conversation proposes, as the Live host adapter folded it
//! (the crate-private `session::candidate`) from its Session's borrowed
//! projection (`nika_session::runtime::Candidate`): the identity a consent
//! names, every change a yes would land (what it creates or replaces, where,
//! over which witnessed bytes), what the Session's audit says each audited
//! workflow reaches when it runs (under its own path when it lands several),
//! the words of the rehearsal proof bound to that identity, and the look of
//! the exact pending bytes of ONE workflow (their witness, the check facade's
//! verdict of them alone, their graph); the changes whose bytes are not shown
//! are counted, never passed off as shown. Every fact of one
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
use nika_tui_view::Face;
use ratatui::text::{Line, Span};

use super::inspect::{Inspected, title_row};
use super::text::{marks, twins, wrap};
use crate::visual::role;

/// The candidate under review, fixed at its fold.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Proposed {
    id: ProposalId,
    aside: bool,
    changes: Vec<String>,
    effects: Option<Vec<String>>,
    /// How its workflow was revised over the complete document and which components it holds,
    /// each with whether it needs attention: `(words, warn)`.
    revision: Vec<(String, bool)>,
    /// Where each audited workflow reaches as declared, and whether that leaves this machine
    /// (or cannot be told): `(words, outside)`.
    world: Vec<(String, bool)>,
    rehearsed: Option<String>,
    unshown: usize,
    look: Inspected,
}

impl PartialEq for Proposed {
    /// Two folds are the same candidate when every fact they show is the
    /// same: the identity, the standing, the changes, the reach, the
    /// rehearsal, and the bytes at the same path.
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.aside == other.aside
            && self.changes == other.changes
            && self.effects == other.effects
            && self.revision == other.revision
            && self.world == other.world
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
            changes: Vec::new(),
            effects: None,
            revision: Vec::new(),
            world: Vec::new(),
            rehearsed: None,
            unshown: 0,
            look,
        }
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

    /// With where each audited workflow reaches as the check declares it, and whether that
    /// leaves this machine (or cannot be told).
    pub(crate) fn declaring(mut self, world: Vec<(String, bool)>) -> Self {
        self.world = world;
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

    /// How the aside and the conversation panel name it.
    #[must_use]
    pub fn label(&self) -> String {
        format!("proposal {}", self.path())
    }

    /// The facts above every face: the identity and what a yes answers, the
    /// changes, what the workflow reaches, the rehearsal; each with its role.
    fn head(&self, ascii: bool) -> Vec<(String, Role)> {
        let (sep, _) = marks(ascii);
        let id = self.id.to_string();
        let standing = if self.aside {
            format!("set aside while the revision's question waits{sep}not consentable now")
        } else {
            format!("what a yes answers{sep}not saved{sep}nothing has run on your files")
        };
        let mut rows = vec![(format!("proposal {id}{sep}{standing}"), Role::Strong)];
        rows.extend(self.changes.iter().map(|c| (c.clone(), Role::Accent)));
        if self.unshown > 0 {
            rows.push((
                format!(
                    "{} more change(s) whose bytes these faces do not show{sep}`/show` prints every byte",
                    self.unshown
                ),
                Role::Warn,
            ));
        }
        rows.extend(
            self.revision
                .iter()
                .map(|(words, warn)| (words.clone(), if *warn { Role::Warn } else { Role::Dim })),
        );
        match self.effects.as_deref() {
            None => rows.push((
                format!("when it runs{sep}not a workflow the Session audited: it runs nothing"),
                Role::Dim,
            )),
            Some([]) => rows.push((
                format!("when it runs{sep}nothing outside the process"),
                Role::Dim,
            )),
            Some(effects) => rows.extend(
                effects
                    .iter()
                    .map(|effect| (format!("when it runs{sep}{effect}"), Role::Dim)),
            ),
        }
        rows.extend(self.world.iter().map(|(words, outside)| {
            let role = if *outside { Role::Warn } else { Role::Dim };
            (format!("reaches, as declared{sep}{words}"), role)
        }));
        match &self.rehearsed {
            Some(words) => rows.extend(
                words
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| (format!("rehearsal{sep}{}", l.trim()), Role::Dim)),
            ),
            None => rows.push((
                format!("rehearsal{sep}no proof is bound to this identity"),
                Role::Dim,
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
        let (sep, cut) = marks(ascii);
        let name = format!("proposal{sep}{}", self.look.title());
        let title = title_row(&name, face, width, ascii, color);
        let mut body = Vec::new();
        for (row, tone) in self.head(ascii) {
            for part in wrap(&twins(&row, ascii), usize::from(width), cut) {
                body.push(Line::from(Span::styled(part, role::style(tone, color))));
            }
        }
        let short: String = self.witness().unwrap_or("").chars().take(12).collect();
        let said =
            format!("these bytes {short}, the proposal's own{sep}Left/Right change the face");
        body.extend(self.look.judged_lines(face, width, ascii, color, &said));
        (title, body)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
