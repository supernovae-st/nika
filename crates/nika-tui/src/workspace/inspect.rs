// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The workflow the human opened, as the Live host adapter looked at it (the
//! crate-private `session::look`), folded once into the typed facts the four
//! faces of `nika_tui_view::workflow` read: the exact bytes of the file, their
//! witness, the check facade's verdict, findings and hints, and the graph of
//! the same parse. Every fact of one [`Inspected`] comes from one read of one
//! file and is fixed at capture: the fields are private, so a witness, a
//! source and a check of different bytes can never be put together.
//!
//! A look reads the opened file ALONE. What it imports (child workflows, skill
//! files, registry references) is not captured, so the check face says so and
//! shows RUN READY as unknown: a look never projects the readiness or the
//! permissions of the composed workflow. A source the parser refuses stays
//! visible beside its refusal.
//!
//! Nothing here reads a file, a clock or the environment, and drawing calls
//! nothing here: the shell renders a face ([`Inspected::face_lines`]) when the
//! look, the face or the region changes, and the frame paints those lines. A
//! look grants nothing: it is not a consent, not a Save, not a Run, and it is
//! never sent to a model. Later bytes need a new look.

use std::sync::Arc;

use nika_display::check_render::VerdictLayers;
use nika_display::dag_art::GraphDoc;
use nika_display::theme::Role;
use nika_tui_view::{Canvas, Face, Finding, Verdict, Workflow};
use ratatui::text::{Line, Span};

use super::text::{fit_head, marks};
use crate::visual::icon::Icon;
use crate::visual::role;

/// What every audited look says it left out.
pub(crate) const NOT_CAPTURED: &str =
    "not captured (UNKNOWN): child workflows, skills, registry references; judged: this file alone";

/// One finding or hint as the check listed it, owned.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Said {
    code: Option<String>,
    class: String,
    message: String,
    place: Option<String>,
}

impl Said {
    fn row(&self) -> Finding<'_> {
        Finding::new(
            self.code.as_deref(),
            &self.class,
            &self.message,
            self.place.as_deref(),
        )
    }
}

/// What the check said about the bytes of one look.
#[derive(Clone, Debug)]
enum Judged {
    /// The layers over the file alone, and what was not captured.
    ParentOnly(VerdictLayers, String),
    /// The parser refused the bytes.
    Refused { code: String, message: String },
    /// The bytes were not read, so nothing was judged.
    Unread,
}

/// The facts of one look, fixed at capture. Clones share the graph.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Inspected {
    path: String,
    name: Option<String>,
    witness: Option<String>,
    unread: Option<String>,
    source: String,
    waves: Option<Vec<Vec<usize>>>,
    graph: Option<Arc<GraphDoc>>,
    judged: Judged,
    findings: Vec<Said>,
    hints: Vec<Said>,
    identity: Option<String>,
    risk: Option<&'static str>,
}

impl PartialEq for Inspected {
    /// Two looks are the same when they name the same path and read the
    /// same bytes: everything else was derived from those bytes.
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path && self.witness == other.witness && self.unread == other.unread
    }
}

impl Eq for Inspected {}

impl Inspected {
    /// The look of `path` whose bytes could not be read, and why.
    pub(crate) fn unread(path: impl Into<String>, why: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            name: None,
            witness: None,
            unread: Some(why.into()),
            source: String::new(),
            waves: None,
            graph: None,
            judged: Judged::Unread,
            findings: Vec::new(),
            hints: Vec::new(),
            identity: None,
            risk: None,
        }
    }

    /// The bytes of `path`, held with `witness`, that no check judged (a
    /// project file a proposal carries): every face says nothing judged them.
    pub(crate) fn unjudged(path: impl Into<String>, witness: String, source: String) -> Self {
        let mut out = Self::unread(path, "");
        out.unread = None;
        out.witness = Some(witness);
        out.source = source;
        out
    }

    /// The look of `path` over `source`, read once with `witness`, and the
    /// check facade's answer about those same bytes: its audit and the graph
    /// of its parse, or the parser's refusal (its code and its words).
    pub(crate) fn read(
        path: impl Into<String>,
        witness: String,
        source: String,
        audit: Result<(&nika_cli_host::oracle::Audit, GraphDoc), (String, String)>,
    ) -> Self {
        let mut out = Self::unjudged(path, witness, source);
        match audit {
            Ok((audit, graph)) => {
                out.graph = Some(Arc::new(graph));
                out.audited(audit);
            }
            Err((code, message)) => out.judged = Judged::Refused { code, message },
        }
        out
    }

    /// Keep what the faces read of one audit: the waves, the layers over the
    /// file alone, the findings and hints in the check's order, the identity
    /// and the grade.
    fn audited(&mut self, audit: &nika_cli_host::oracle::Audit) {
        let report = &audit.report;
        let verdict = &audit.verdict;
        self.name = audit.wf.workflow.as_ref().map(|n| n.value.clone());
        self.waves = Some(report.waves.clone());
        let mut unknown = NOT_CAPTURED.to_owned();
        if !verdict.children.is_empty() {
            unknown = format!("{unknown}; it names {}", verdict.children.join(", "));
        }
        self.judged = Judged::ParentOnly(verdict.layers.clone(), unknown);
        self.findings = report
            .findings
            .iter()
            .map(|f| Said {
                code: f.code.clone(),
                class: f.kind.to_owned(),
                message: f.message.clone(),
                place: f.task.as_ref().map(|task| format!("task {task}")),
            })
            .collect();
        self.findings
            .extend(verdict.models.findings.iter().map(|m| Said {
                code: m.code.clone(),
                class: "model".to_owned(),
                message: format!("{} · {}", m.model, m.why),
                place: (!m.tasks.is_empty()).then(|| format!("task {}", m.tasks.join(", "))),
            }));
        self.hints = report
            .hints
            .iter()
            .map(|h| Said {
                code: h.code.map(str::to_owned),
                class: h.kind.to_owned(),
                message: h.advice.clone(),
                place: (h.task != "-").then(|| format!("task {}", h.task)),
            })
            .collect();
        self.identity.clone_from(&report.workflow_semantic);
        self.risk = Some(verdict.grade.as_str());
    }

    /// The listed path, relative to the project's root.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The witness of the bytes read (blake3, hex); `None` when unread.
    #[must_use]
    pub fn witness(&self) -> Option<&str> {
        self.witness.as_deref()
    }

    /// Why the bytes could not be read, when they could not.
    #[must_use]
    pub fn why_unread(&self) -> Option<&str> {
        self.unread.as_deref()
    }

    /// The name the object shows: the file's own name, else its path.
    #[must_use]
    pub fn title(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.path)
    }

    /// The bytes read, as text (empty when unread).
    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    /// The graph of these bytes and the check's run order, when both exist.
    pub(crate) fn graph(&self) -> Option<(&GraphDoc, &[Vec<usize>])> {
        Some((self.graph.as_deref()?, self.waves.as_deref()?))
    }

    /// The viewer's facts for this look, borrowed for one rendering.
    fn workflow<'a>(
        &'a self,
        findings: &'a [Finding<'a>],
        hints: &'a [Finding<'a>],
    ) -> Workflow<'a> {
        let mut input = Workflow::new(&self.path, &self.source);
        input.waves = self.waves.as_deref();
        input.graph = self.graph.as_deref();
        input.verdict = match &self.judged {
            Judged::ParentOnly(layers, unknown) => Verdict::ParentOnly { layers, unknown },
            Judged::Refused { code, message } => Verdict::Refused { code, message },
            Judged::Unread => Verdict::NotJudged,
        };
        input.findings = findings;
        input.hints = hints;
        input.identity = self.identity.as_deref();
        input.risk = self.risk;
        input
    }

    /// One face of this look on a region `width` cells wide: its title row
    /// (the icon, the name, the faces with the one in view marked), then the
    /// viewer's facts and notes, the witness of the bytes shown and how to
    /// look again, then the face's body. Bytes nobody read show why, and no
    /// face is drawn over them. Pure: no file, no clock.
    #[must_use]
    pub fn face_lines(
        &self,
        face: Face,
        width: u16,
        ascii: bool,
        color: bool,
    ) -> (Line<'static>, Vec<Line<'static>>) {
        let (sep, cut) = marks(ascii);
        let cells = usize::from(width);
        let title = title_row(self.title(), face, width, ascii, color);
        let again = format!("{sep}r reads it again{sep}Left/Right change the face");
        let Some(witness) = self.witness.as_deref() else {
            let why = self.unread.as_deref().unwrap_or("unknown");
            let body = [
                format!("not read: {why}"),
                "nothing was judged: no face is drawn over bytes nobody read".to_owned(),
                format!("{}{again}", self.path),
            ];
            let body = body.iter().map(|row| Line::from(fit_head(row, cells, cut)));
            return (title, body.collect());
        };
        let short: String = witness.chars().take(12).collect();
        let read = format!("this file's bytes {short}, as last read{again}");
        (title, self.judged_lines(face, width, ascii, color, &read))
    }

    /// One face of these bytes as the viewers render it: their facts and
    /// notes, then `said` (one dim row: whose bytes these are), then the
    /// face's body. Pure: no file, no clock.
    pub(crate) fn judged_lines(
        &self,
        face: Face,
        width: u16,
        ascii: bool,
        color: bool,
        said: &str,
    ) -> Vec<Line<'static>> {
        let (_, cut) = marks(ascii);
        let findings: Vec<Finding<'_>> = self.findings.iter().map(Said::row).collect();
        let hints: Vec<Finding<'_>> = self.hints.iter().map(Said::row).collect();
        let canvas = Canvas::new(width, ascii, color);
        let rendered = nika_tui_view::workflow(face, &self.workflow(&findings, &hints), canvas);
        let mut body = rendered.head(canvas);
        body.push(Line::from(Span::styled(
            fit_head(said, usize::from(width), cut),
            role::style(Role::Dim, color),
        )));
        body.extend(rendered.lines);
        body
    }
}

/// The title row of an object with faces: the icon, `name`, the faces with
/// the one in view marked, fitted to `width` cells.
pub(crate) fn title_row(
    name: &str,
    face: Face,
    width: u16,
    ascii: bool,
    color: bool,
) -> Line<'static> {
    let (sep, cut) = marks(ascii);
    let tabs: Vec<String> = Face::ALL
        .iter()
        .map(|f| {
            if *f == face {
                format!("[{}]", f.label())
            } else {
                f.label().to_owned()
            }
        })
        .collect();
    let glyph = Icon::Workflow.glyph(ascii);
    let head = format!("{glyph} {name}{sep}{}", tabs.join(" "));
    Line::from(Span::styled(
        fit_head(&head, usize::from(width), cut),
        role::style(Role::Strong, color),
    ))
}
