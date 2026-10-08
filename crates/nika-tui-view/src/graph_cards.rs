// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cards over the canonical projection, never a second graph or run model.
//! Each checked wave is one row of compact cards: the task, its verb and
//! target, and only what a caller observed. Real wires join two waves only
//! when every edge links adjacent waves and each joint is complete between
//! its own sources and targets, in columns no other joint shares: the
//! honesty law of `nika_display::wires`, measured in this layout's columns.
//! A wire carries a dependency of any kind: an edge that is not a value
//! dependency also keeps its exact typed row after the drawing, and exact
//! rows replace the wires for every other shape. A line bound keeps whole
//! blocks only (a wave with the joint that feeds it, a listed wave, a typed
//! row, the cleanup section) and says what it cut.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use nika_display::dag_art::GraphDoc;
use nika_display::theme::Role;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::cells::{self, Sheet, paint};
use crate::visual::role;
use crate::{Canvas, Format, Note, Rendered};

/// Cells between two cards of one row.
const GAP: usize = 2;
/// The narrowest card that shares its row.
const MIN_CARD: usize = 16;
/// The narrowest lone card a wired drawing keeps.
const MIN_ALONE: usize = 8;
/// A card's rows: frame, title, definition, frame.
const CARD_ROWS: usize = 4;

/// The directions one wire cell joins.
const UP: u8 = 1;
const DOWN: u8 = 2;
const LEFT: u8 = 4;
const RIGHT: u8 = 8;

/// Draw the checked graph as cards, with optional caller-observed status.
///
/// `waves` belongs to the same projection as `doc`. The callback supplies
/// display words and their semantic role, not an inferred state: `None`
/// shows only the definition. One role is a convention: an observation in
/// [`Role::Accent`] is the running task, as the theme's task states paint
/// it, and its card is framed in the accent on the selection fill, so give
/// the accent to running work only. The caller owns run/source binding.
/// This function never parses, schedules, acquires data or advances a run.
/// Unsupported wire shapes retain the exact typed dependencies in words.
#[must_use]
pub fn graph_cards(
    doc: &GraphDoc,
    waves: &[Vec<usize>],
    canvas: Canvas,
    observed: &dyn Fn(&str) -> Option<(String, Role)>,
) -> Rendered {
    let mut sheet = Sheet::new(canvas, Format::Workflow);
    if let Some(why) = unavailable(doc, waves, canvas) {
        sheet
            .body
            .push(vec![paint(why, Role::Warn, canvas.color)], false);
        return sheet.finish("graph".to_owned());
    }
    let rows = ranges(waves);
    let scheduled = rows.last().map_or(0, |row| row.end);
    let states: Vec<Option<(String, Role)>> =
        doc.nodes.iter().map(|node| observed(&node.id)).collect();
    let watched = states.iter().any(Option::is_some);
    let wiring = Wiring::plan(doc, &rows, canvas);
    let sep = cells::sep(canvas.ascii);
    // The units outside the waves are drawn too: the summary names them.
    let outside = doc.nodes.get(scheduled..).unwrap_or_default();
    let cleanup = outside.iter().filter(|node| node.kind == "finally").count();
    let other = outside.len().saturating_sub(cleanup);
    let mut counted = vec![cells::count(scheduled, "task")];
    if cleanup > 0 {
        counted.push(cells::count(cleanup, "cleanup unit"));
    }
    if other > 0 {
        counted.push(format!(
            "{} outside task waves",
            cells::count(other, "unit")
        ));
    }
    counted.push(cells::count(doc.edges.len(), "edge"));
    counted.push(cells::count(rows.len(), "wave"));
    sheet.facts.push(counted.join(sep));
    if wiring.is_none() {
        sheet
            .facts
            .push("cards by wave; exact dependencies listed, no inferred wires".to_owned());
    }
    if !watched {
        sheet
            .facts
            .push(format!("definition{sep}no task state observed"));
    }
    let mut draw = Draw {
        doc,
        canvas,
        states: &states,
        watched,
        inside: doc
            .nodes
            .get(..scheduled)
            .unwrap_or_default()
            .iter()
            .map(|node| node.id.as_str())
            .collect(),
        labels: Labels::default(),
        left: canvas.limits.lines,
    };
    let kept = match &wiring {
        Some(wiring) => draw.wired(&mut sheet, &rows, wiring),
        None => draw.listed(&mut sheet, &rows),
    };
    if kept {
        draw.outside(&mut sheet, scheduled);
    }
    draw.labels.report(&mut sheet.notes);
    sheet.finish("graph".to_owned())
}

/// The node range of each wave: node order is wave order in the projection.
fn ranges(waves: &[Vec<usize>]) -> Vec<Range<usize>> {
    let mut start = 0;
    waves
        .iter()
        .map(|wave| {
            let range = start..start + wave.len();
            start = range.end;
            range
        })
        .collect()
}

/// Whether `count` cards share one row of `width` cells legibly.
const fn fits(count: usize, width: usize) -> bool {
    match count {
        0 => true,
        1 => width >= MIN_ALONE,
        _ => {
            count
                .saturating_mul(MIN_CARD)
                .saturating_add((count - 1).saturating_mul(GAP))
                <= width
        }
    }
}

/// The cards one row holds: all of a wave that fits, else as many as fit.
fn per_row(count: usize, width: usize) -> usize {
    if count <= 1 || fits(count, width) {
        count.max(1)
    } else {
        ((width + GAP) / (MIN_CARD + GAP)).clamp(1, count)
    }
}

/// `(x, width)` of each of `count` cards filling `width` cells.
fn place(count: usize, width: usize) -> Vec<(usize, usize)> {
    if count <= 1 {
        return vec![(0, width)];
    }
    let room = width.saturating_sub((count - 1) * GAP);
    let (base, rest) = (room / count, room % count);
    let mut x = 0;
    (0..count)
        .map(|index| {
            let size = base + usize::from(index < rest);
            let at = x;
            x += size + GAP;
            (at, size)
        })
        .collect()
}

/// The column where wires meet a card placed at `(x, width)`.
const fn centre((x, width): (usize, usize)) -> usize {
    x + width / 2
}

/// The drawing's glyphs in the glyph column in use.
struct Glyphs {
    top: (char, char),
    bottom: (char, char),
    across: char,
    side: char,
    exit: char,
    entry: char,
}

const fn glyphs(ascii: bool) -> Glyphs {
    if ascii {
        Glyphs {
            top: ('+', '+'),
            bottom: ('+', '+'),
            across: '-',
            side: '|',
            exit: '+',
            entry: 'v',
        }
    } else {
        Glyphs {
            top: ('┌', '┐'),
            bottom: ('└', '┘'),
            across: '─',
            side: '│',
            exit: '┬',
            entry: '▼',
        }
    }
}

/// One wire cell from the directions it joins (`UP`, `DOWN`, `LEFT`, `RIGHT`).
const fn wire_cell(links: u8, ascii: bool) -> char {
    let (up, down) = ((links & UP) != 0, (links & DOWN) != 0);
    let (left, right) = ((links & LEFT) != 0, (links & RIGHT) != 0);
    let vertical = up || down;
    if ascii {
        return match (vertical, left || right) {
            (true, true) => '+',
            (true, false) => '|',
            _ => '-',
        };
    }
    match (up, down, left, right) {
        (true, false, false, true) => '└',
        (true, false, true, false) => '┘',
        (false, true, false, true) => '┌',
        (false, true, true, false) => '┐',
        (true, true, false, true) => '├',
        (true, true, true, false) => '┤',
        (true, false, true, true) => '┴',
        (false, true, true, true) => '┬',
        (true, true, true, true) => '┼',
        _ if vertical => '│',
        _ => '─',
    }
}

/// Where each card of a wired drawing sits, and the joints between waves.
struct Wiring {
    /// Per wave, each card's `(x, width)`.
    columns: Vec<Vec<(usize, usize)>>,
    /// Per wave, the columns where a wire enters a card from above.
    entries: Vec<BTreeSet<usize>>,
    /// Per wave, the columns where a wire leaves a card downward.
    exits: Vec<BTreeSet<usize>>,
    /// The wire row under every wave but the last.
    joints: Vec<String>,
}

impl Wiring {
    /// The truthful drawing of these waves at this width, or `None` when a
    /// wire would skip a wave, share another joint's columns or claim an
    /// edge the projection does not hold.
    fn plan(doc: &GraphDoc, rows: &[Range<usize>], canvas: Canvas) -> Option<Self> {
        let width = usize::from(canvas.width);
        if rows.iter().any(|row| !fits(row.len(), width)) {
            return None;
        }
        let mut slots = BTreeMap::new();
        for (wave, row) in rows.iter().enumerate() {
            for (column, node) in doc.nodes.get(row.clone())?.iter().enumerate() {
                slots.insert(node.id.as_str(), (wave, column));
            }
        }
        let mut gutters = vec![BTreeSet::new(); rows.len().saturating_sub(1)];
        for edge in &doc.edges {
            let (Some(&(from, a)), Some(&(to, b))) =
                (slots.get(edge.from.as_str()), slots.get(edge.to.as_str()))
            else {
                continue; // a unit outside the waves keeps its exact row
            };
            if to != from + 1 {
                return None;
            }
            gutters.get_mut(from)?.insert((a, b));
        }
        let mut wiring = Self {
            columns: rows.iter().map(|row| place(row.len(), width)).collect(),
            entries: vec![BTreeSet::new(); rows.len()],
            exits: vec![BTreeSet::new(); rows.len()],
            joints: Vec::with_capacity(gutters.len()),
        };
        for (wave, pairs) in gutters.iter().enumerate() {
            let joint = wiring.gutter_row(wave, pairs, width, canvas.ascii)?;
            wiring.joints.push(joint);
        }
        Some(wiring)
    }

    /// The wire row between `wave` and the next, recording each card's exit
    /// and entry, or `None` when one row of wire cannot say these pairs.
    fn gutter_row(
        &mut self,
        wave: usize,
        pairs: &BTreeSet<(usize, usize)>,
        width: usize,
        ascii: bool,
    ) -> Option<String> {
        let mut spans = Vec::new();
        for (sources, targets) in components(pairs)? {
            let up: BTreeSet<usize> = sources
                .iter()
                .map(|&column| self.columns.get(wave)?.get(column).copied().map(centre))
                .collect::<Option<_>>()?;
            let down: BTreeSet<usize> = targets
                .iter()
                .map(|&column| self.columns.get(wave + 1)?.get(column).copied().map(centre))
                .collect::<Option<_>>()?;
            let lo = *up.first()?.min(down.first()?);
            let hi = *up.last()?.max(down.last()?);
            spans.push((lo, hi, up, down));
        }
        spans.sort_by_key(|span| span.0);
        // A blank cell between joints, or one row would merge two claims.
        if spans.windows(2).any(|pair| pair[1].0 <= pair[0].1 + 1) {
            return None;
        }
        let mut row = vec![' '; width];
        for (lo, hi, up, down) in spans {
            for (at, cell) in row.iter_mut().enumerate().take(hi + 1).skip(lo) {
                let mut links = 0;
                if up.contains(&at) {
                    links |= UP;
                }
                if down.contains(&at) {
                    links |= DOWN;
                }
                if at > lo {
                    links |= LEFT;
                }
                if at < hi {
                    links |= RIGHT;
                }
                *cell = wire_cell(links, ascii);
            }
            self.exits.get_mut(wave)?.extend(up);
            self.entries.get_mut(wave + 1)?.extend(down);
        }
        Some(row.into_iter().collect::<String>().trim_end().to_owned())
    }
}

/// One joint: the columns of its sources and of its targets.
type Joint = (BTreeSet<usize>, BTreeSet<usize>);

/// The joints of one gutter's `(source, target)` column pairs: groups that
/// share a source or a target. `None` for an empty gutter, or unless every
/// group is complete, all its sources feeding all its targets.
fn components(pairs: &BTreeSet<(usize, usize)>) -> Option<Vec<Joint>> {
    if pairs.is_empty() {
        return None; // waves without a joining edge have no truthful wire
    }
    let mut groups: Vec<Joint> = Vec::new();
    for &(from, to) in pairs {
        let mut sources = BTreeSet::from([from]);
        let mut targets = BTreeSet::from([to]);
        loop {
            let before = groups.len();
            groups.retain(|(held, fed)| {
                if held.is_disjoint(&sources) && fed.is_disjoint(&targets) {
                    return true;
                }
                sources.extend(held);
                targets.extend(fed);
                false
            });
            if groups.len() == before {
                break;
            }
        }
        groups.push((sources, targets));
    }
    let complete = groups.iter().all(|(sources, targets)| {
        let held = pairs
            .iter()
            .filter(|(from, _)| sources.contains(from))
            .count();
        held == sources.len() * targets.len()
    });
    complete.then_some(groups)
}

/// Bound the supplied projection before allocating the drawing.
fn unavailable(doc: &GraphDoc, waves: &[Vec<usize>], canvas: Canvas) -> Option<&'static str> {
    if doc.nodes.len() > canvas.limits.lines.max(1)
        || doc.edges.len() > canvas.limits.lines.saturating_mul(4)
        || waves.len() > canvas.limits.lines
    {
        return Some("graph exceeds the view's item bound; no partial topology drawn");
    }
    if waves.is_empty() || waves.iter().any(Vec::is_empty) {
        return Some("no graph: the check found no valid run order");
    }
    let Some(total) = waves
        .iter()
        .try_fold(0usize, |sum, wave| sum.checked_add(wave.len()))
    else {
        return Some("no graph: the projection and the waves disagree");
    };
    if total > doc.nodes.len() {
        return Some("no graph: the projection and the waves disagree");
    }
    let mut bytes = 0usize;
    for node in &doc.nodes {
        for text in [
            node.id.as_str(),
            node.verb,
            node.kind,
            node.tool.as_deref().unwrap_or(""),
            node.model.as_deref().unwrap_or(""),
        ] {
            bytes = bytes.saturating_add(text.len());
        }
    }
    for edge in &doc.edges {
        bytes = bytes
            .saturating_add(edge.from.len())
            .saturating_add(edge.to.len())
            .saturating_add(edge.kind.len());
        bytes = bytes.saturating_add(edge.predicate.map_or(0, str::len));
    }
    if bytes > canvas.limits.bytes {
        return Some("graph exceeds the view's byte bound; no partial topology drawn");
    }
    let scheduled: BTreeSet<_> = waves.iter().flatten().copied().collect();
    if scheduled.len() != total || scheduled.iter().any(|index| *index >= doc.nodes.len()) {
        return Some("no graph: the projection and the waves disagree");
    }
    let ids: BTreeSet<_> = doc.nodes.iter().map(|node| node.id.as_str()).collect();
    if ids.len() != doc.nodes.len()
        || doc
            .edges
            .iter()
            .any(|edge| !ids.contains(edge.from.as_str()) || !ids.contains(edge.to.as_str()))
    {
        return Some("no graph: the projection contains duplicate or missing node identities");
    }
    None
}

#[derive(Clone, Copy, Default)]
struct Labels {
    shortened: bool,
    controls: usize,
}

impl Labels {
    fn text(&mut self, raw: &str, canvas: Canvas) -> String {
        let end = cells::floor_boundary(raw, canvas.limits.line_bytes);
        self.shortened |= end < raw.len();
        let (clean, controls) = cells::clean(&cells::dots(&raw[..end], canvas.ascii), canvas.ascii);
        self.controls += controls;
        clean
    }

    fn report(self, notes: &mut Vec<Note>) {
        if self.shortened {
            notes.push(Note::Fallback {
                to: Format::Workflow,
                why: "card labels shortened to fit; source and plan retain details",
            });
        }
        if self.controls > 0 {
            notes.push(Note::Controls {
                count: self.controls,
            });
        }
    }
}

/// What one card says: the task id, its definition, and an observation.
struct Card {
    title: String,
    detail: String,
    tail: Option<(String, Role)>,
    /// Observed in the accent: the active task, framed on the selection.
    active: bool,
}

/// The rows of one card, each exactly as wide as the card.
type CardRows = [Vec<Span<'static>>; CARD_ROWS];

/// Rows the line bound keeps whole or not at all.
type Block = Vec<Vec<Span<'static>>>;

/// One drawing in progress: the facts it reads and the labels it cut.
struct Draw<'a> {
    doc: &'a GraphDoc,
    canvas: Canvas,
    states: &'a [Option<(String, Role)>],
    /// Some task was observed: every card then says its state or `definition`.
    watched: bool,
    /// The ids scheduled in the task waves.
    inside: BTreeSet<&'a str>,
    labels: Labels,
    /// Rows the line bound still holds.
    left: usize,
}

impl Draw<'_> {
    /// Build one block and keep it whole, or drop it whole: what building it
    /// counted (shortened labels, controls) goes with it. False, with the cut
    /// recorded, when the line bound cannot hold it.
    fn whole(&mut self, sheet: &mut Sheet, build: impl FnOnce(&mut Self) -> Block) -> bool {
        let counted = self.labels;
        let block = build(self);
        if block.len() > self.left {
            self.labels = counted;
            sheet.body.overflow();
            return false;
        }
        self.left -= block.len();
        for row in block {
            sheet.body.push(row, false);
        }
        true
    }

    /// Each wave as one row of cards under the wire row that feeds it (the
    /// two kept together), then the exact rows of the edges a wire cannot
    /// word. False once the bound is reached.
    fn wired(&mut self, sheet: &mut Sheet, rows: &[Range<usize>], wiring: &Wiring) -> bool {
        for (wave, row) in rows.iter().enumerate() {
            let kept = self.whole(sheet, |draw| {
                let mut block = Vec::with_capacity(CARD_ROWS + 1);
                let feeding = wave
                    .checked_sub(1)
                    .and_then(|above| wiring.joints.get(above));
                if let Some(joint) = feeding {
                    block.push(vec![paint(joint.clone(), Role::Dim, draw.canvas.color)]);
                }
                block.extend(draw.wired_row(wave, row.clone(), wiring));
                block
            });
            if !kept {
                return false;
            }
        }
        let doc = self.doc;
        for edge in &doc.edges {
            let between =
                self.inside.contains(edge.from.as_str()) && self.inside.contains(edge.to.as_str());
            if between && edge.kind != "value" {
                let kept = self.whole(sheet, |draw| {
                    vec![vec![draw.edge_line(
                        &edge.from,
                        &edge.to,
                        edge.kind,
                        edge.predicate,
                    )]]
                });
                if !kept {
                    return false;
                }
            }
        }
        true
    }

    /// The card row of `wave`, each card marked where its wires meet it.
    fn wired_row(&mut self, wave: usize, row: Range<usize>, wiring: &Wiring) -> Block {
        let mut cards = Vec::with_capacity(row.len());
        for (column, index) in row.enumerate() {
            let Some(&slot) = wiring
                .columns
                .get(wave)
                .and_then(|placed| placed.get(column))
            else {
                continue;
            };
            let at = centre(slot);
            let mark = |marks: &[BTreeSet<usize>]| {
                marks
                    .get(wave)
                    .is_some_and(|here| here.contains(&at))
                    .then_some(at - slot.0)
            };
            let entry = mark(wiring.entries.as_slice());
            let exit = mark(wiring.exits.as_slice());
            if let Some(card) = self.card(index) {
                cards.push(self.card_rows(&card, slot.1, entry, exit));
            }
        }
        row_lines(cards)
    }

    /// Each wave as one block: its heading when it wraps, its exact incoming
    /// dependencies, then all its cards. False once the bound is reached.
    fn listed(&mut self, sheet: &mut Sheet, rows: &[Range<usize>]) -> bool {
        let canvas = self.canvas;
        let width = usize::from(canvas.width);
        let doc = self.doc;
        for (wave, row) in rows.iter().enumerate() {
            let kept = self.whole(sheet, |draw| {
                let mut block = Vec::new();
                let count = row.len();
                if per_row(count, width) < count {
                    let heading = format!(
                        "wave {}{}{}",
                        wave + 1,
                        cells::sep(canvas.ascii),
                        cells::count(count, "task")
                    );
                    block.push(vec![paint(heading, Role::Dim, canvas.color)]);
                }
                let nodes = doc.nodes.get(row.clone()).unwrap_or_default();
                for edge in &doc.edges {
                    if nodes.iter().any(|node| node.id == edge.to) {
                        let line = draw.edge_line(&edge.from, &edge.to, edge.kind, edge.predicate);
                        block.push(vec![line]);
                    }
                }
                block.extend(draw.rows_of_cards(row.clone()));
                block
            });
            if !kept {
                return false;
            }
        }
        true
    }

    /// Units outside the task waves (cleanup), as one block: their
    /// population, their exact edges and their cards, never a wire.
    fn outside(&mut self, sheet: &mut Sheet, scheduled: usize) {
        let canvas = self.canvas;
        let doc = self.doc;
        let rest = scheduled..doc.nodes.len();
        if rest.is_empty() {
            return;
        }
        self.whole(sheet, |draw| {
            let mut block = vec![vec![paint("outside task waves", Role::Dim, canvas.color)]];
            for edge in &doc.edges {
                let between = draw.inside.contains(edge.from.as_str())
                    && draw.inside.contains(edge.to.as_str());
                if !between {
                    let line = draw.edge_line(&edge.from, &edge.to, edge.kind, edge.predicate);
                    block.push(vec![line]);
                }
            }
            block.extend(draw.rows_of_cards(rest));
            block
        });
    }

    /// The cards of `nodes` in rows of as many as fit, without wires.
    fn rows_of_cards(&mut self, nodes: Range<usize>) -> Block {
        let width = usize::from(self.canvas.width);
        let columns = place(per_row(nodes.len(), width), width);
        let indices: Vec<usize> = nodes.collect();
        let mut lines = Vec::new();
        for chunk in indices.chunks(columns.len().max(1)) {
            let mut cards = Vec::with_capacity(chunk.len());
            for (&index, &(_, size)) in chunk.iter().zip(&columns) {
                if let Some(card) = self.card(index) {
                    cards.push(self.card_rows(&card, size, None, None));
                }
            }
            lines.extend(row_lines(cards));
        }
        lines
    }

    /// What the card of node `index` says.
    fn card(&mut self, index: usize) -> Option<Card> {
        let canvas = self.canvas;
        let doc = self.doc;
        let node = doc.nodes.get(index)?;
        let sep = cells::sep(canvas.ascii);
        let mut detail = node.verb.to_owned();
        if let Some(target) = node.tool.as_deref().or(node.model.as_deref()) {
            detail.push_str(sep);
            detail.push_str(&self.labels.text(target, canvas));
        }
        match node.kind {
            "task" => {}
            "finally" => {
                detail.push_str(sep);
                detail.push_str("cleanup");
            }
            other => {
                detail.push_str(sep);
                detail.push_str(&self.labels.text(other, canvas));
            }
        }
        let state = self.states.get(index).cloned().flatten();
        let active = matches!(state, Some((_, Role::Accent)));
        let tail = match state {
            Some((words, tone)) => Some((self.labels.text(&words, canvas), tone)),
            None if self.watched => Some(("definition".to_owned(), Role::Dim)),
            None => None,
        };
        Some(Card {
            title: self.labels.text(&node.id, canvas),
            detail,
            tail,
            active,
        })
    }

    /// The four rows of `card`, `width` cells wide: its frame, marked where
    /// a wire enters or leaves it; its title, any observation at the right;
    /// its definition in the muted ink; its frame.
    fn card_rows(
        &mut self,
        card: &Card,
        width: usize,
        entry: Option<usize>,
        exit: Option<usize>,
    ) -> CardRows {
        let color = self.canvas.color;
        let g = glyphs(self.canvas.ascii);
        let ground = if card.active {
            role::surface(color, false).patch(role::selection(color))
        } else {
            role::surface(color, false)
        };
        // Without colour the active frame keeps a weight, never a hue.
        let frame = ground.patch(match (card.active, color) {
            (true, true) => role::style(Role::Accent, true),
            (true, false) => role::style(Role::Strong, false),
            (false, _) => role::border(color),
        });
        let wire = ground.patch(role::style(Role::Dim, color));
        let title = Span::styled(card.title.clone(), ground);
        let tail = card.tail.as_ref().map(|(words, tone)| {
            Span::styled(words.clone(), ground.patch(role::style(*tone, color)))
        });
        let detail = Span::styled(
            card.detail.clone(),
            ground.patch(role::style(Role::Dim, color)),
        );
        let top = border(
            width,
            g.top,
            g.across,
            entry.map(|at| (at, g.entry)),
            frame,
            wire,
        );
        let bottom = border(
            width,
            g.bottom,
            g.across,
            exit.map(|at| (at, g.exit)),
            frame,
            wire,
        );
        let first = self.inner(width, title, tail, g.side, frame, ground);
        let second = self.inner(width, detail, None, g.side, frame, ground);
        [top, first, second, bottom]
    }

    /// One framed text row of a card `width` cells wide.
    fn inner(
        &mut self,
        width: usize,
        left: Span<'static>,
        tail: Option<Span<'static>>,
        side: char,
        frame: Style,
        ground: Style,
    ) -> Vec<Span<'static>> {
        if width < 4 {
            return self.split(left, tail, width, ground);
        }
        let mut spans = vec![Span::styled(format!("{side} "), frame)];
        spans.extend(self.split(left, tail, width - 4, ground));
        spans.push(Span::styled(format!(" {side}"), frame));
        spans
    }

    /// `left`, then `tail` against the right edge, in exactly `room` cells.
    /// When both cannot fit whole the tail keeps at most half the room and
    /// gives back what the left does not use; every cut is a shortened label.
    fn split(
        &mut self,
        left: Span<'static>,
        tail: Option<Span<'static>>,
        room: usize,
        ground: Style,
    ) -> Vec<Span<'static>> {
        let canvas = self.canvas;
        let left_cells = cells::width(&left.content);
        let tail_cells = tail.as_ref().map_or(0, |span| cells::width(&span.content));
        let (left_room, tail_room) = match &tail {
            None => (room, 0),
            Some(_) if left_cells + 1 + tail_cells <= room => (left_cells, tail_cells),
            Some(_) => {
                let tail_room = tail_cells.min(room / 2);
                let left_room = room.saturating_sub(tail_room + 1);
                if left_cells <= left_room {
                    (left_cells, room.saturating_sub(left_cells + 1))
                } else {
                    (left_room, tail_room)
                }
            }
        };
        let (left, left_cut) = cells::fit(vec![left], left_room, false, canvas);
        let (tail, tail_cut) = match tail {
            Some(span) => cells::fit(vec![span], tail_room, false, canvas),
            None => (Line::default(), false),
        };
        self.labels.shortened |= left_cut || tail_cut;
        let used: usize = left
            .spans
            .iter()
            .chain(&tail.spans)
            .map(|span| cells::width(&span.content))
            .sum();
        let mut spans = left.spans;
        spans.push(Span::raw(" ".repeat(room.saturating_sub(used))));
        spans.extend(tail.spans);
        // A cut mark and the padding take the card's ground as well.
        spans
            .into_iter()
            .map(|span| {
                let style = ground.patch(span.style);
                Span::styled(span.content, style)
            })
            .collect()
    }

    /// One exact dependency, typed: `from → to · kind / predicate`.
    fn edge_line(
        &mut self,
        from: &str,
        to: &str,
        kind: &str,
        predicate: Option<&str>,
    ) -> Span<'static> {
        let canvas = self.canvas;
        let from = self.labels.text(from, canvas);
        let to = self.labels.text(to, canvas);
        let kind = self.labels.text(kind, canvas);
        let predicate = predicate
            .map(|p| format!(" / {}", self.labels.text(p, canvas)))
            .unwrap_or_default();
        let arrow = if canvas.ascii { "->" } else { "→" };
        let text = format!("{from} {arrow} {to} · {kind}{predicate}");
        paint(cells::dots(&text, canvas.ascii), Role::Dim, canvas.color)
    }
}

/// A frame row `width` cells wide between `corners`, with one wire `mark`
/// (its column inside the frame and its glyph) in the wire's ink.
fn border(
    width: usize,
    corners: (char, char),
    across: char,
    mark: Option<(usize, char)>,
    frame: Style,
    wire: Style,
) -> Vec<Span<'static>> {
    let (left, right) = corners;
    let run = |cells: usize| across.to_string().repeat(cells);
    match width {
        0 => Vec::new(),
        1 => vec![Span::styled(left.to_string(), frame)],
        _ => match mark.filter(|&(at, _)| at > 0 && at < width - 1) {
            Some((at, glyph)) => vec![
                Span::styled(format!("{left}{}", run(at - 1)), frame),
                Span::styled(glyph.to_string(), wire),
                Span::styled(format!("{}{right}", run(width - at - 2)), frame),
            ],
            None => vec![Span::styled(
                format!("{left}{}{right}", run(width - 2)),
                frame,
            )],
        },
    }
}

/// One row of cards `GAP` cells apart, as its four lines.
fn row_lines(mut cards: Vec<CardRows>) -> Block {
    (0..CARD_ROWS)
        .map(|row| {
            let mut spans = Vec::new();
            for (index, card) in cards.iter_mut().enumerate() {
                if index > 0 {
                    spans.push(Span::raw(" ".repeat(GAP)));
                }
                spans.append(&mut card[row]);
            }
            spans
        })
        .collect()
}
