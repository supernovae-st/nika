// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The workspace objects a screen names, each with three faces: a label that
//! is always shown, a Unicode glyph drawn only where its terminal cell is
//! certain, and an ASCII twin for CI logs, legacy terminals and `--ascii`.
//!
//! Verbs and task states are not here: they are the theme seam's
//! (`Theme::verb_glyph_bare`, `Theme::glyph`), one vocabulary for every
//! surface. Those columns are padded to two cells, which absorbs an East Asian
//! ambiguous glyph; an icon here sits inline beside its label, unpadded, so a
//! glyph whose width differs between the narrow and the CJK tables falls back
//! to its ASCII twin instead of shifting the line.

use unicode_width::UnicodeWidthStr;

/// A workspace object with a visual identity. A glyph decorates the object;
/// it never grants, implies or replaces an access, an authority or a label.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Icon {
    /// The active project (the selector at the top of the screen).
    Project,
    /// A workflow and its revisions.
    Workflow,
    /// A conversation thread, sibling of the workflows it prepares.
    Conversation,
    /// A run of one workflow revision.
    Run,
    /// An activation (a declared schedule or trigger, armed or not).
    Activation,
    /// A file of the project.
    File,
    /// A memory the project can see.
    Memory,
    /// A connection available to the workspace or the project.
    Connection,
    /// Settings.
    Settings,
    /// A run kept in view while another revision is prepared.
    Pinned,
    /// Search.
    Search,
    /// A choice to open (a menu or a selector).
    Choice,
}

impl Icon {
    /// Every icon, in the order a legend lists them.
    pub const ALL: [Self; 12] = [
        Self::Project,
        Self::Workflow,
        Self::Conversation,
        Self::Run,
        Self::Activation,
        Self::File,
        Self::Memory,
        Self::Connection,
        Self::Settings,
        Self::Pinned,
        Self::Search,
        Self::Choice,
    ];

    /// The words a screen reads for this object; the glyph never replaces them.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Workflow => "workflow",
            Self::Conversation => "conversation",
            Self::Run => "run",
            Self::Activation => "activation",
            Self::File => "file",
            Self::Memory => "memory",
            Self::Connection => "connection",
            Self::Settings => "settings",
            Self::Pinned => "pinned",
            Self::Search => "search",
            Self::Choice => "choose",
        }
    }

    /// The Unicode glyph the visual registry proposes for this object.
    #[must_use]
    pub const fn unicode(self) -> &'static str {
        match self {
            Self::Project => "▱",
            Self::Workflow => "⑂",
            Self::Conversation => "◌",
            Self::Run => "▶",
            Self::Activation => "◷",
            Self::File => "≡",
            Self::Memory => "▤",
            Self::Connection => "⇄",
            Self::Settings => "⚙",
            Self::Pinned => "⌖",
            Self::Search => "⌕",
            Self::Choice => "⌄",
        }
    }

    /// The ASCII twin.
    #[must_use]
    pub const fn ascii(self) -> &'static str {
        match self {
            Self::Project => "[P]",
            Self::Workflow => "[W]",
            Self::Conversation => "[C]",
            Self::Run => ">",
            Self::Activation => "[A]",
            Self::File => "[F]",
            Self::Memory => "[M]",
            Self::Connection => "[+]",
            Self::Settings => "[*]",
            Self::Pinned => "^",
            Self::Search => "/",
            Self::Choice => "v",
        }
    }

    /// The glyph to draw: the Unicode one when the ASCII column was not asked
    /// for and the glyph takes exactly one cell in both width tables, the
    /// ASCII twin otherwise.
    #[must_use]
    pub fn glyph(self, ascii: bool) -> &'static str {
        let unicode = self.unicode();
        if ascii || !one_cell(unicode) {
            self.ascii()
        } else {
            unicode
        }
    }
}

/// Whether `glyph` is one cell wide in the narrow and in the CJK width tables
/// alike (an East Asian ambiguous character is two cells in the second).
fn one_cell(glyph: &str) -> bool {
    glyph.width() == 1 && glyph.width_cjk() == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_has_a_label_and_an_ascii_twin() {
        for icon in Icon::ALL {
            assert!(!icon.label().is_empty(), "{icon:?}");
            assert!(
                icon.ascii().is_ascii() && !icon.ascii().is_empty(),
                "{icon:?}"
            );
            assert!(!icon.ascii().contains(' '), "{icon:?}");
            assert_eq!(icon.glyph(true), icon.ascii(), "{icon:?}");
        }
    }

    #[test]
    fn a_unicode_glyph_is_drawn_only_where_its_cell_is_certain() {
        // East Asian ambiguous glyphs take two cells under a CJK width table:
        // inline beside a label they would shift the line, so the twin is drawn.
        let certain: Vec<Icon> = Icon::ALL
            .into_iter()
            .filter(|icon| icon.glyph(false) == icon.unicode())
            .collect();
        let twin: Vec<Icon> = Icon::ALL
            .into_iter()
            .filter(|icon| icon.glyph(false) == icon.ascii())
            .collect();
        assert_eq!(twin, [Icon::Run, Icon::File, Icon::Memory]);
        assert_eq!(certain.len() + twin.len(), Icon::ALL.len());
        for icon in certain {
            assert_eq!(icon.glyph(false).width(), 1, "{icon:?}");
            assert_eq!(icon.glyph(false).width_cjk(), 1, "{icon:?}");
        }
    }

    #[test]
    fn labels_are_distinct_so_no_two_objects_read_alike() {
        let mut labels: Vec<&str> = Icon::ALL.iter().map(|icon| icon.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), Icon::ALL.len());
        let mut twins: Vec<&str> = Icon::ALL.iter().map(|icon| icon.ascii()).collect();
        twins.sort_unstable();
        twins.dedup();
        assert_eq!(twins.len(), Icon::ALL.len());
    }
}
