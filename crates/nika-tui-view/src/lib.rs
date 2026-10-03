// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-tui-view` — the viewers of the object in view (task
//! T-nika-tui-viewers), a size-cap member of the `nika-tui` unit (ADR-143):
//! the renderer depends on this crate, never the reverse.
//!
//! A viewer is a pure function. In: bytes or text the caller already holds,
//! the facts the caller knows about them ([`Meta`]) and the cells the
//! region offers ([`Canvas`]). Out: a [`Rendered`] value, a title, the
//! facts under it, the styled lines of the body and the [`Note`]s that say
//! what was cut, what fell back and what disagreed. Nothing here reads a
//! file, a clock or the environment, spawns, blocks or stores: a decoder
//! reads at most [`Limits::bytes`], produces at most [`Limits::lines`],
//! examines at most [`Limits::line_bytes`] of any one line, and says so
//! when it stops.
//!
//! Two families:
//!
//! - the workflow in view ([`workflow()`] with a [`Face`]): its source with
//!   verb-aware highlighting, its plan in run order
//!   (`nika_session::review::plan_lines_in_order`), its graph
//!   (`nika_display::dag_art::ascii_art`, escape-free, restyled here) and
//!   its check, the layers and every finding with its code, from typed
//!   facts their owner computed and handed over as a [`Workflow`] (the
//!   view never audits, judges or reads a permit);
//! - every artifact a workflow produces ([`artifact()`]), classified by its
//!   declared type, then its extension, then its magic bytes
//!   ([`classify()`]), and shown honestly: an image or a sound by the facts
//!   its header states, never guessed and never drawn through a terminal
//!   image protocol; an opaque file by its facts and a short hex head.
//!
//! Null, missing, empty and unknown stay four different sentences. When the
//! caller marks the object protected, every value that looks like a secret
//! or sits under a key naming a credential is masked, however deep and
//! however many lines it spans. A glyph decorates and the words carry the
//! meaning, so every view reads the same under `NO_COLOR` and in the ASCII
//! column, and a control character in a file reaches the screen as a
//! visible mark, never as a command to the terminal.

use ratatui::text::Line;

mod artifact;
mod cells;
mod classify;
mod data;
mod diff;
mod json;
mod markdown;
mod mask;
mod media;
mod opaque;
mod role;
mod secret;
mod source;
mod table;
mod text;
mod workflow;

#[cfg(test)]
mod adversarial_tests;
#[cfg(test)]
mod faces_tests;
#[cfg(test)]
mod protected_tests;

pub use classify::{Format, classify, for_builtin};
pub use workflow::{Face, Finding, Verdict, Workflow, workflow};

/// Whether the object is where its record says, as the caller observed it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Availability {
    /// The caller found it where the record says.
    Present,
    /// Nothing is at the recorded place any more.
    Missing,
    /// Something is there, and it differs from what was recorded.
    Changed,
    /// Nobody looked.
    #[default]
    Unknown,
}

/// What the caller hands over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Content<'a> {
    /// The object's bytes (possibly empty).
    Bytes(&'a [u8]),
    /// The object's text (possibly empty).
    Text(&'a str),
    /// The value is `null`: neither empty nor missing.
    Null,
    /// The caller holds no bytes: see [`Meta::availability`] for why.
    Unread,
}

/// The facts the caller knows about an object. This module never reads
/// them from anywhere; the field names follow the builtin results that
/// carry them (`mime_type`, `format`, `width`, `height`, `size_bytes`,
/// `sha256`, `duration_ms`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Meta {
    /// The object's name (a file name, a path or an output key).
    pub name: String,
    /// The declared media type (`image/png`), as recorded.
    pub mime_type: Option<String>,
    /// The declared format word or extension (`png`, `csv`), as recorded.
    pub format: Option<String>,
    /// The declared size in bytes.
    pub size_bytes: Option<u64>,
    /// The declared width in pixels.
    pub width: Option<u32>,
    /// The declared height in pixels.
    pub height: Option<u32>,
    /// The declared duration in milliseconds.
    pub duration_ms: Option<u64>,
    /// The recorded SHA-256 of the bytes (hex).
    pub sha256: Option<String>,
    /// The builtin or verb that produced it (`nika:chart`, `infer`).
    pub producer: Option<String>,
    /// Where it comes from, in the caller's words (`run #043 · summarize`).
    pub provenance: Option<String>,
    /// Whether it is where the record says.
    pub availability: Availability,
    /// The object may hold secrets: values that look like one are masked.
    pub protected: bool,
}

impl Meta {
    /// The facts of an object known only by its name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }
}

/// The bounds every decoder keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Limits {
    /// Bytes a decoder reads; the rest is never looked at.
    pub bytes: usize,
    /// Lines a view produces.
    pub lines: usize,
    /// Bytes of one line a decoder examines before it cuts the line.
    pub line_bytes: usize,
}

impl Limits {
    /// Explicit bounds (each at least one).
    #[must_use]
    pub const fn new(bytes: usize, lines: usize, line_bytes: usize) -> Self {
        Self {
            bytes: if bytes == 0 { 1 } else { bytes },
            lines: if lines == 0 { 1 } else { lines },
            line_bytes: if line_bytes == 0 { 1 } else { line_bytes },
        }
    }
}

impl Default for Limits {
    /// 256 KiB read, 1000 lines, 4 KiB of any one line.
    fn default() -> Self {
        Self::new(256 * 1024, 1000, 4096)
    }
}

/// The cells a view may use and how it paints them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Canvas {
    /// The region's width in cells: every line fits in it.
    pub width: u16,
    /// Draw the ASCII twins instead of the Unicode glyphs.
    pub ascii: bool,
    /// Let roles carry their hues (off under `NO_COLOR`).
    pub color: bool,
    /// The decoders' bounds.
    pub limits: Limits,
}

impl Canvas {
    /// A canvas `width` cells wide with the default bounds.
    #[must_use]
    pub fn new(width: u16, ascii: bool, color: bool) -> Self {
        Self {
            width,
            ascii,
            color,
            limits: Limits::default(),
        }
    }

    /// The same canvas with other bounds.
    #[must_use]
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }
}

/// What a viewer says about what it cut, what fell back and what disagreed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Note {
    /// Only the first `read` of `total` bytes were decoded.
    BytesCut {
        /// Bytes decoded.
        read: usize,
        /// Bytes held by the caller.
        total: usize,
    },
    /// The body stops after `shown` lines; more followed.
    LinesCut {
        /// Lines kept.
        shown: usize,
    },
    /// `lines` lines were wider than `width` cells and end at the edge.
    WidthCut {
        /// Lines cut.
        lines: usize,
        /// The canvas width.
        width: u16,
    },
    /// Byte sequences that are not UTF-8, each shown as one mark.
    InvalidUtf8 {
        /// Invalid sequences.
        sequences: usize,
    },
    /// Control characters shown as visible marks.
    Controls {
        /// Characters marked.
        count: usize,
    },
    /// Values that look like secrets, masked because the object is protected.
    Masked {
        /// Values masked.
        count: usize,
    },
    /// The declared type and the bytes disagree.
    Mismatch {
        /// The type as declared.
        declared: String,
        /// What the bytes are.
        found: Format,
    },
    /// A decoder gave the object to a plainer view.
    Fallback {
        /// The view that shows it.
        to: Format,
        /// Why.
        why: &'static str,
    },
    /// The object changed since it was recorded.
    Changed,
    /// Nesting deeper than `depth` levels keeps the last indent.
    DepthCapped {
        /// The deepest indent drawn.
        depth: usize,
    },
    /// `shown` of `total` table columns fit the width.
    ColumnsCut {
        /// Columns drawn.
        shown: usize,
        /// Columns in the widest row read.
        total: usize,
    },
    /// `shown` of `total` table rows are drawn.
    RowsCut {
        /// Rows drawn.
        shown: usize,
        /// Rows read.
        total: usize,
    },
}

impl Note {
    /// Whether the note reports a disagreement or a risk (drawn amber),
    /// rather than a bound the viewer kept.
    #[must_use]
    pub const fn is_warning(&self) -> bool {
        matches!(
            self,
            Self::Mismatch { .. } | Self::Changed | Self::Masked { .. } | Self::InvalidUtf8 { .. }
        )
    }

    /// The note in words.
    #[must_use]
    pub fn text(&self, ascii: bool) -> String {
        match self {
            Self::BytesCut { read, total } => format!(
                "read the first {} of {}; the rest was not read",
                cells::size_words(*read),
                cells::size_words(*total)
            ),
            Self::LinesCut { shown } => {
                format!(
                    "the first {} shown; more follow",
                    cells::count(*shown, "line")
                )
            }
            Self::WidthCut { lines, width } => format!(
                "{} wider than {width} columns, cut at the edge",
                cells::count(*lines, "line")
            ),
            Self::InvalidUtf8 { sequences } => format!(
                "{} not UTF-8, one {} per sequence",
                cells::count(*sequences, "byte sequence"),
                cells::invalid_mark(ascii)
            ),
            Self::Controls { count } => {
                format!("{} made visible", cells::count(*count, "control character"))
            }
            Self::Masked { count } => format!(
                "{} masked: secret-looking in a protected object",
                cells::count(*count, "value")
            ),
            Self::Mismatch { declared, found } => {
                let declared = cells::clean(declared, ascii).0;
                match found {
                    Format::Opaque => {
                        format!("declared {declared}, but the bytes do not carry its signature")
                    }
                    found => format!("declared {declared}, but the bytes are {}", found.label()),
                }
            }
            Self::Fallback { to, why } => format!("shown as {}: {why}", to.label()),
            Self::Changed => "the object changed since it was recorded".to_owned(),
            Self::DepthCapped { depth } => format!(
                "nesting deeper than {} keeps the last indent",
                cells::count(*depth, "level")
            ),
            Self::ColumnsCut { shown, total } => {
                format!("the width holds {shown} of {total} columns")
            }
            Self::RowsCut { shown, total } => format!("rows drawn: the first {shown} of {total}"),
        }
    }
}

/// One object as a viewer shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Rendered {
    /// The object's name.
    pub title: String,
    /// What the viewer took the object for.
    pub format: Format,
    /// Short facts under the title (kind, size, dimensions, provenance…).
    pub facts: Vec<String>,
    /// The body, every line fitted to the canvas width.
    pub lines: Vec<Line<'static>>,
    /// What was cut, what fell back, what disagreed.
    pub notes: Vec<Note>,
}

impl Rendered {
    /// The rows a region draws under its title: the facts, wrapped to the
    /// width, then one row per note (a disagreement marked `!`, a bound
    /// kept marked `~`, in both glyph columns).
    #[must_use]
    pub fn head(&self, canvas: Canvas) -> Vec<Line<'static>> {
        cells::head(&self.facts, &self.notes, canvas)
    }

    /// The number of body lines: what a scroll runs over.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether the body holds no line.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// Show one artifact: its state (null, missing, empty, unknown), its
/// facts, and its body through the viewer its classification names.
#[must_use]
pub fn artifact(content: Content<'_>, meta: &Meta, canvas: Canvas) -> Rendered {
    artifact::show(content, meta, canvas)
}
