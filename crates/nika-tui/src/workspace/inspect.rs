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

use std::fmt::Write as _;
use std::sync::Arc;

use nika_display::check_render::VerdictLayers;
use nika_display::dag_art::GraphDoc;
use nika_display::theme::Role;
use nika_tui_view::{Canvas, Face, Finding, Verdict, Workflow};
use ratatui::text::{Line, Span};

use super::text::{fit_head, marks, wrap};
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
    /// face's body and evidence. The graph leads with its content, keeping
    /// truncation notes above it and capture details below it. Bytes nobody
    /// read show why, and no
    /// face is drawn over them. Pure: no file, no clock.
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

    /// A short object uses exact dependency rows rather than partial cards.
    pub(crate) fn face_lines_in(
        &self,
        face: Face,
        width: u16,
        ascii: bool,
        color: bool,
        compact: bool,
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
        let read = if face == Face::Graph {
            format!("Read {short}{sep}Check for details")
        } else {
            format!("this file's bytes {short}, as last read{again}")
        };
        (
            title,
            self.judged_lines_in(face, width, ascii, color, &read, compact),
        )
    }

    /// A viewport-specific projection over the same fixed graph and audit.
    pub(crate) fn judged_lines_in(
        &self,
        face: Face,
        width: u16,
        ascii: bool,
        color: bool,
        said: &str,
        compact: bool,
    ) -> Vec<Line<'static>> {
        let (_, cut) = marks(ascii);
        let findings: Vec<Finding<'_>> = self.findings.iter().map(Said::row).collect();
        let hints: Vec<Finding<'_>> = self.hints.iter().map(Said::row).collect();
        let canvas = Canvas::new(width, ascii, color);
        let mut rendered = nika_tui_view::workflow(face, &self.workflow(&findings, &hints), canvas);
        let witness = Line::from(Span::styled(
            fit_head(said, usize::from(width), cut),
            role::style(Role::Dim, color),
        ));
        if face == Face::Graph {
            // Keep material drawing limits beside the graph. Routine capture
            // details follow it in this same scrollable object.
            let facts = std::mem::take(&mut rendered.facts);
            let mut body = rendered.head(canvas);
            if compact
                && rendered.notes.is_empty()
                && let Some(rows) = self.compact_graph(canvas)
            {
                body.extend(rows);
            } else {
                body.push(Line::default());
                body.append(&mut rendered.lines);
            }
            body.push(Line::default());
            if let Some(summary) = self.graph_summary(width, ascii, color) {
                body.extend(summary);
            } else {
                rendered.facts = facts;
                rendered.notes.clear();
                body.extend(rendered.head(canvas));
            }
            body.push(witness);
            body
        } else {
            let mut body = rendered.head(canvas);
            body.push(witness);
            body.extend(rendered.lines);
            body
        }
    }

    /// A compact graph retains every node and each incoming edge from the
    /// captured projection. No frames or implied connectors can be cut by
    /// the viewport; longer rows wrap and remain reachable by scrolling.
    fn compact_graph(&self, canvas: Canvas) -> Option<Vec<Line<'static>>> {
        let (graph, waves) = self.graph()?;
        // The graph viewer owns availability. Workflow-level facts also
        // exist for refused graphs, so they cannot authorize this projection.
        let checked = nika_tui_view::graph_cards(graph, waves, canvas, &|_| None);
        if checked.facts.is_empty() || !checked.notes.is_empty() {
            return None;
        }
        let Canvas {
            width,
            ascii,
            color,
            ..
        } = canvas;
        let columns = usize::from(width).checked_sub(2)?;
        // Only plain value dependencies can be collapsed into an incoming
        // list. Keep typed predicates, unsafe or shortened identities and
        // all graph validation refusals in the original bounded viewer.
        if graph
            .edges
            .iter()
            .any(|edge| edge.kind != "value" || edge.predicate.is_some())
            || graph.nodes.iter().any(|node| {
                !node.id.bytes().all(|byte| byte.is_ascii_graphic())
                    || node.id.len().saturating_add(1) > columns
            })
        {
            return None;
        }
        let (sep, cut) = marks(ascii);
        let arrow = if ascii { " <- " } else { " ← " };
        let mut rows = vec![Line::styled(
            fit_head(
                &format!("Compact graph{sep}exact dependencies"),
                usize::from(width),
                cut,
            ),
            role::style(Role::Dim, color),
        )];
        for node in &graph.nodes {
            let incoming: Vec<_> = graph
                .edges
                .iter()
                .filter(|edge| edge.to == node.id)
                .map(|edge| edge.from.as_str())
                .collect();
            let mut row = node.id.clone();
            if !incoming.is_empty() {
                row.push_str(arrow);
                row.push_str(&incoming.join(", "));
            }
            let _ = write!(row, "{sep}{}", node.verb);
            if node.kind == "finally" {
                let _ = write!(row, "{sep}cleanup");
            }
            // Reserve a hanging indent so a wrapped source never reads as
            // another node. Every identity, including a trailing comma, fits.
            let wrapped = wrap(&row, columns, cut);
            if rows.len().saturating_add(wrapped.len()) > canvas.limits.lines {
                return None;
            }
            rows.extend(wrapped.into_iter().enumerate().map(|(index, line)| {
                let line = if index == 0 {
                    line
                } else {
                    format!("  {line}")
                };
                Line::styled(line, role::style(Role::Strong, color))
            }));
        }
        Some(rows)
    }

    /// The definition's useful facts beside its cards. Full audit layers,
    /// byte size and semantic identity remain on Source and Check; this
    /// summary grants no readiness to a composed workflow or a future Run.
    fn graph_summary(&self, width: u16, ascii: bool, color: bool) -> Option<Vec<Line<'static>>> {
        let graph = self.graph.as_ref()?;
        let (sep, cut) = marks(ascii);
        let tasks = graph
            .nodes
            .iter()
            .filter(|node| node.kind == "task")
            .count();
        let cleanup = graph
            .nodes
            .iter()
            .filter(|node| node.kind == "finally")
            .count();
        let edges = graph.edges.len();
        let task_word = if tasks == 1 { "task" } else { "tasks" };
        let edge_word = if edges == 1 { "edge" } else { "edges" };
        let mut facts = format!("Definition{sep}{tasks} {task_word}{sep}{edges} {edge_word}");
        if cleanup > 0 {
            let unit_word = if cleanup == 1 { "unit" } else { "units" };
            let _ = write!(facts, "{sep}{cleanup} cleanup {unit_word}");
        }
        let mut rows: Vec<_> = wrap(&facts, usize::from(width), cut)
            .into_iter()
            .map(|line| Line::styled(line, role::style(Role::Dim, color)))
            .collect();
        if let Judged::ParentOnly(layers, _) = &self.judged {
            let (words, tone) = if layers.valid {
                ("Structure checked", Role::Good)
            } else {
                ("Structure needs attention", Role::Warn)
            };
            let mut verdict = format!("{words}{sep}this file only");
            if !self.findings.is_empty() {
                let count = self.findings.len();
                let noun = if count == 1 { "finding" } else { "findings" };
                let _ = write!(verdict, "{sep}{count} {noun} in Check");
            }
            rows.extend(
                wrap(&verdict, usize::from(width), cut)
                    .into_iter()
                    .map(|line| Line::styled(line, role::style(tone, color))),
            );
        }
        Some(rows)
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
    let tab_width: usize = tabs.iter().map(String::len).sum::<usize>() + tabs.len() - 1;
    if usize::from(width) < tab_width {
        return Line::styled(
            fit_head(&format!("[{}]", face.label()), usize::from(width), cut),
            role::style(Role::Accent, color),
        );
    }
    let room = usize::from(width).saturating_sub(tab_width + sep.chars().count());
    let mut spans = Vec::new();
    if room > 0 {
        spans.push(Span::styled(
            fit_head(name, room, cut),
            role::style(Role::Strong, color),
        ));
        spans.push(Span::styled(sep, role::style(Role::Dim, color)));
    }
    for (index, (tab, item)) in tabs.into_iter().zip(Face::ALL).enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        let style = if item == face {
            role::style(Role::Accent, color).add_modifier(ratatui::style::Modifier::BOLD)
        } else {
            role::style(Role::Dim, color)
        };
        spans.push(Span::styled(tab, style));
    }
    Line::from(spans)
}
