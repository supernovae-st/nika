// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The workflow the human opened, as the renderer's host adapter looked at
//! it, folded once into the typed facts the four faces of [`crate::workflow()`]
//! read: the exact bytes of the file, their witness, the check facade's
//! verdict, findings and hints ([`Checked`]), and the graph of the same
//! parse. Every fact of one [`Inspected`] comes from one read of one file and
//! is fixed at capture: the fields are private, so a witness, a source and a
//! check of different bytes can never be put together.
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

use crate::{Canvas, Face, Finding, Verdict, Workflow, cells};
use nika_display::check_render::VerdictLayers;
use nika_display::dag_art::GraphDoc;
use nika_display::theme::Role;
use nika_session::Lang;
use ratatui::text::{Line, Span};

use super::lang::say;
use super::text::{fit_head, marks, wrap};
use crate::visual::role;

/// One finding or hint as the check listed it, owned: its code when it has
/// one, its class, its words and the place it names.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Said {
    code: Option<String>,
    class: String,
    message: String,
    place: Option<String>,
}

impl Said {
    /// A finding or a hint: `code` when the check named one, its `class`, its
    /// `message` and the `place` it names (`task <id>`).
    #[must_use]
    pub fn new(
        code: Option<String>,
        class: impl Into<String>,
        message: impl Into<String>,
        place: Option<String>,
    ) -> Self {
        Self {
            code,
            class: class.into(),
            message: message.into(),
            place,
        }
    }

    fn row(&self) -> Finding<'_> {
        Finding::new(
            self.code.as_deref(),
            &self.class,
            &self.message,
            self.place.as_deref(),
        )
    }
}

/// What the check facade said of one look's bytes, judged alone, as plain
/// facts: the file's own name, the run order, the layers over the file alone
/// and what was not captured, the findings and hints in the check's order,
/// the semantic identity and the grade.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Checked {
    name: Option<String>,
    waves: Vec<Vec<usize>>,
    layers: VerdictLayers,
    unknown: String,
    findings: Vec<Said>,
    hints: Vec<Said>,
    identity: Option<String>,
    risk: &'static str,
    imports: Vec<String>,
}

impl Checked {
    /// The check's `layers` over the file alone, its run order (`waves`),
    /// what it did not capture (`unknown`) and its grade (`risk`).
    #[must_use]
    pub fn new(
        layers: VerdictLayers,
        waves: Vec<Vec<usize>>,
        unknown: String,
        risk: &'static str,
    ) -> Self {
        Self {
            name: None,
            waves,
            layers,
            unknown,
            findings: Vec::new(),
            hints: Vec::new(),
            identity: None,
            risk,
            imports: Vec::new(),
        }
    }

    /// With what the file names that the look does not capture (its child
    /// workflows), said after `unknown` in the look's language.
    #[must_use]
    pub fn importing(mut self, imports: Vec<String>) -> Self {
        self.imports = imports;
        self
    }

    /// With the file's own name (`nika:`), when it parsed.
    #[must_use]
    pub fn named(mut self, name: Option<String>) -> Self {
        self.name = name;
        self
    }

    /// With the findings and the hints, in the check's order.
    #[must_use]
    pub fn listing(mut self, findings: Vec<Said>, hints: Vec<Said>) -> Self {
        self.findings = findings;
        self.hints = hints;
        self
    }

    /// With the workflow's semantic identity, when the check stated one.
    #[must_use]
    pub fn identified(mut self, identity: Option<String>) -> Self {
        self.identity = identity;
        self
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
    imports: Vec<String>,
    /// The language the look's own words follow ([`Inspected::speaking`]).
    lang: Lang,
    /// The object holds the keys ([`Inspected::keys_here`]).
    here: bool,
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
    #[must_use]
    pub fn unread(path: impl Into<String>, why: impl Into<String>) -> Self {
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
            imports: Vec::new(),
            lang: Lang::En,
            here: true,
        }
    }

    /// This look with its key cues said from where the keys are: `here` (the
    /// object holds them) names the keys themselves (`r`, `Left/Right`), else
    /// the key that brings them from the box first (`F6 then …`), so no cue
    /// names a key that does nothing where the person types. The object
    /// holds them by default.
    #[must_use]
    pub fn keys_here(mut self, here: bool) -> Self {
        self.here = here;
        self
    }

    /// This look with its own words (the faces' names, the rows around them,
    /// the facts above each face, its notes, the check's reasons) said in
    /// `lang`; English by default. The bytes, the engine's findings, names
    /// and values keep their words.
    #[must_use]
    pub fn speaking(mut self, lang: Lang) -> Self {
        self.lang = lang;
        self
    }

    /// The bytes of `path`, held with `witness`, that no check judged (a
    /// project file a proposal carries): every face says nothing judged them.
    #[must_use]
    pub fn unjudged(path: impl Into<String>, witness: String, source: String) -> Self {
        let mut out = Self::unread(path, "");
        out.unread = None;
        out.witness = Some(witness);
        out.source = source;
        out
    }

    /// The look of `path` over `source`, read once with `witness`, and the
    /// check facade's answer about those same bytes: what it said of them
    /// alone and the graph of its parse, or the parser's refusal (its code
    /// and its words).
    #[must_use]
    pub fn read(
        path: impl Into<String>,
        witness: String,
        source: String,
        checked: Result<(Checked, GraphDoc), (String, String)>,
    ) -> Self {
        let mut out = Self::unjudged(path, witness, source);
        match checked {
            Ok((checked, graph)) => {
                out.graph = Some(Arc::new(graph));
                out.name = checked.name;
                out.waves = Some(checked.waves);
                out.judged = Judged::ParentOnly(checked.layers, checked.unknown);
                out.findings = checked.findings;
                out.hints = checked.hints;
                out.identity = checked.identity;
                out.risk = Some(checked.risk);
                out.imports = checked.imports;
            }
            Err((code, message)) => out.judged = Judged::Refused { code, message },
        }
        out
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
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The graph of these bytes and the check's run order, when both exist.
    #[must_use]
    pub fn graph(&self) -> Option<(&GraphDoc, &[Vec<usize>])> {
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
    #[must_use]
    pub fn face_lines_in(
        &self,
        face: Face,
        width: u16,
        ascii: bool,
        color: bool,
        compact: bool,
    ) -> (Line<'static>, Vec<Line<'static>>) {
        let (sep, cut) = marks(ascii);
        let cells = usize::from(width);
        let said = |en: &str| say(self.lang, ascii, en).replace("{sep}", sep);
        let title = title_row_in(self.title(), face, width, (ascii, color), self.lang);
        let again = format!(
            "{sep}{}{sep}{}",
            said(if self.here {
                "r reads it again"
            } else {
                "F6 then r reads it again"
            }),
            said("Left/Right change the face")
        );
        if self.witness.is_none() {
            let why = self.unread.as_deref().unwrap_or("unknown");
            let body = [
                said("not read: {why}").replace("{why}", why),
                said("nothing was judged: no face is drawn over bytes nobody read"),
                format!("{}{again}", self.path),
            ];
            let body = body.iter().map(|row| Line::from(fit_head(row, cells, cut)));
            return (title, body.collect());
        }
        // Whose bytes these are, never their witness's hex: it names nothing a person acts on
        // here (live gauntlet 4: « ces octets 7594d366fc7c »).
        let read = if face == Face::Graph {
            said("This file as last read{sep}Check for details")
        } else {
            format!("{}{again}", said("this file's bytes, as last read"))
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
        let findings: Vec<Finding<'_>> = self.findings.iter().map(Said::row).collect();
        let hints: Vec<Finding<'_>> = self.hints.iter().map(Said::row).collect();
        let canvas = Canvas::new(width, ascii, color).speaking(self.lang);
        let unknown = self.unknown_in(canvas);
        let mut input = self.workflow(&findings, &hints);
        if let (Judged::ParentOnly(layers, _), Some(unknown)) = (&self.judged, unknown.as_deref()) {
            input.verdict = Verdict::ParentOnly { layers, unknown };
        }
        let mut rendered = crate::workflow(face, &input, canvas);
        let witness = witness_rows(said, width, ascii, color);
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
            body.extend(witness);
            body
        } else {
            let mut body = rendered.head(canvas);
            body.extend(witness);
            body.extend(rendered.lines);
            body
        }
    }

    /// What the look did not capture, in `canvas`'s language, then what the
    /// file names of it; `None` unless the check judged the file alone.
    fn unknown_in(&self, canvas: Canvas) -> Option<String> {
        let Judged::ParentOnly(_, unknown) = &self.judged else {
            return None;
        };
        let mut said = canvas.say(unknown).to_owned();
        if !self.imports.is_empty() {
            let names = canvas.say("; it names {children}");
            said.push_str(&names.replace("{children}", &self.imports.join(", ")));
        }
        Some(said)
    }

    /// A compact graph retains every node and each incoming edge from the
    /// captured projection. No frames or implied connectors can be cut by
    /// the viewport; longer rows wrap and remain reachable by scrolling.
    fn compact_graph(&self, canvas: Canvas) -> Option<Vec<Line<'static>>> {
        let (graph, waves) = self.graph()?;
        // The graph viewer owns availability. Workflow-level facts also
        // exist for refused graphs, so they cannot authorize this projection.
        let checked = crate::graph_cards(graph, waves, canvas, &|_| None);
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
        let heading = canvas.say("Compact graph{sep}exact dependencies");
        let mut rows = vec![Line::styled(
            fit_head(&heading.replace("{sep}", sep), usize::from(width), cut),
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
                let _ = write!(row, "{sep}{}", canvas.say("cleanup"));
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
        let canvas = Canvas::new(width, ascii, color).speaking(self.lang);
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
        let mut facts = format!(
            "{}{sep}{}{sep}{}",
            canvas.say("Definition"),
            cells::count_in(tasks, "task", canvas),
            cells::count_in(edges, "edge", canvas)
        );
        if cleanup > 0 {
            let units = cells::count_in(cleanup, "cleanup unit", canvas);
            let _ = write!(facts, "{sep}{units}");
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
            let only = canvas.say("this file only");
            let mut verdict = format!("{}{sep}{only}", canvas.say(words));
            if !self.findings.is_empty() {
                let count = self.findings.len();
                let phrase = if count == 1 {
                    "1 finding in Check"
                } else {
                    "{n} findings in Check"
                };
                let found = canvas.say(phrase).replace("{n}", &count.to_string());
                let _ = write!(verdict, "{sep}{found}");
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
#[must_use]
pub fn title_row(name: &str, face: Face, width: u16, ascii: bool, color: bool) -> Line<'static> {
    title_row_in(name, face, width, (ascii, color), Lang::En)
}

/// [`title_row`] with the faces' names said in `lang`, in the glyph column
/// and colour in use.
pub(crate) fn title_row_in(
    name: &str,
    face: Face,
    width: u16,
    (ascii, color): (bool, bool),
    lang: Lang,
) -> Line<'static> {
    let (sep, cut) = marks(ascii);
    let label = |f: Face| say(lang, ascii, f.label());
    let tabs: Vec<String> = Face::ALL
        .iter()
        .map(|f| {
            if *f == face {
                format!("[{}]", label(*f))
            } else {
                label(*f).to_owned()
            }
        })
        .collect();
    let tab_width: usize = tabs.iter().map(|tab| cells::width(tab)).sum::<usize>() + tabs.len() - 1;
    if usize::from(width) < tab_width {
        return Line::styled(
            fit_head(&format!("[{}]", label(face)), usize::from(width), cut),
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

/// The row saying whose bytes a face shows and the keys that turn it (`said`), whole on as
/// many rows `width` cells give it: where it is wider than the face it wraps at its words
/// rather than lose the keys at its end (the face cue was cut at about 120 columns).
pub(crate) fn witness_rows(said: &str, width: u16, ascii: bool, color: bool) -> Vec<Line<'static>> {
    let (_, cut) = marks(ascii);
    (wrap(said, usize::from(width), cut).into_iter())
        .map(|row| Line::from(Span::styled(row, role::style(Role::Dim, color))))
        .collect()
}
