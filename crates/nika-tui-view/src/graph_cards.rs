// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cards over the canonical projection, never a second graph or run model.
//! Waves give placement. The existing wire renderer decides whether wires
//! may be drawn; exact dependency rows remain the fallback for other shapes.

use std::collections::BTreeSet;

use nika_display::dag_art::{GraphDoc, wire_graph};
use nika_display::theme::{Role, Theme};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::cells::{self, Sheet, paint};
use crate::{Canvas, Format, Note, Rendered, role};

/// Draw the checked graph as cards, with optional caller-observed status.
///
/// `waves` belongs to the same projection as `doc`. The callback supplies
/// display words and their semantic role, not an inferred state: `None`
/// shows only the definition. The caller owns run/source binding. This
/// function never parses, schedules, acquires data or advances a run.
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
    let topology = wire_graph(doc, waves);
    let wired =
        nika_display::wires::render(&topology, Theme::new(false, canvas.ascii, false)).is_some();
    sheet.facts.push(
        [
            cells::count(waves.iter().map(Vec::len).sum(), "task"),
            cells::count(doc.edges.len(), "edge"),
            cells::count(waves.len(), "wave"),
        ]
        .join(cells::sep(canvas.ascii)),
    );
    if !wired {
        sheet
            .facts
            .push("cards by wave; exact dependencies listed, no inferred wires".to_owned());
    }
    let mut labels = Labels::default();
    let mut cursor = 0;
    for (index, wave) in waves.iter().enumerate() {
        if sheet.body.is_full() {
            sheet.body.overflow();
            break;
        }
        dependencies(&mut sheet, doc, cursor, wave.len(), &mut labels);
        if index > 0 && wired {
            connector(
                &mut sheet,
                &topology.waves[index - 1],
                &topology.waves[index],
                &topology.edges,
            );
        }
        wave_cards(
            &mut sheet,
            doc,
            cursor,
            index + 1,
            wave.len(),
            observed,
            &mut labels,
        );
        cursor += wave.len();
    }
    outside_waves(&mut sheet, doc, cursor, observed, &mut labels);
    labels.report(&mut sheet.notes);
    sheet.finish("graph".to_owned())
}

/// Cleanup units retain their population and every omitted row is reported.
fn outside_waves(
    sheet: &mut Sheet,
    doc: &GraphDoc,
    cursor: usize,
    observed: &dyn Fn(&str) -> Option<(String, Role)>,
    labels: &mut Labels,
) {
    let canvas = sheet.body.canvas();
    // Cleanup units do not belong to task waves. Keep their population
    // visible rather than presenting them as ordinary scheduled tasks.
    if cursor < doc.nodes.len() && sheet.body.is_full() {
        sheet.body.overflow();
    }
    if cursor < doc.nodes.len() && !sheet.body.is_full() {
        sheet.body.push(
            vec![paint("outside task waves", Role::Dim, canvas.color)],
            false,
        );
        dependencies(sheet, doc, cursor, doc.nodes.len() - cursor, labels);
        for index in cursor..doc.nodes.len() {
            for line in card(
                doc,
                index,
                usize::from(canvas.width),
                canvas,
                observed,
                labels,
            ) {
                if !sheet.body.push(line.spans, false) {
                    break;
                }
            }
            if sheet.body.is_full() {
                if index + 1 < doc.nodes.len() {
                    sheet.body.overflow();
                }
                break;
            }
        }
    }
}

/// One checked wave, stacked in bounded rows of at most two cards.
fn wave_cards(
    sheet: &mut Sheet,
    doc: &GraphDoc,
    cursor: usize,
    number: usize,
    total: usize,
    observed: &dyn Fn(&str) -> Option<(String, Role)>,
    labels: &mut Labels,
) {
    let canvas = sheet.body.canvas();
    let heading = format!(
        "wave {}{}{}",
        number,
        cells::sep(canvas.ascii),
        cells::count(total, "task")
    );
    sheet
        .body
        .push(vec![paint(heading, Role::Dim, canvas.color)], false);
    let columns = if canvas.width >= 52 && total > 1 {
        2
    } else {
        1
    };
    for first in (0..total).step_by(columns) {
        let count = (total - first).min(columns);
        let width = (usize::from(canvas.width).saturating_sub((count - 1) * 2)) / count;
        let cards: Vec<_> = (0..count)
            .map(|offset| {
                card(
                    doc,
                    cursor + first + offset,
                    width,
                    canvas,
                    observed,
                    labels,
                )
            })
            .collect();
        for row in 0..5 {
            let mut spans = Vec::new();
            for (column, card) in cards.iter().enumerate() {
                if column > 0 {
                    spans.push(Span::raw("  "));
                }
                if let Some(line) = card.get(row) {
                    spans.extend(line.spans.iter().cloned());
                }
            }
            if !sheet.body.push(spans, false) {
                break;
            }
        }
        if sheet.body.is_full() {
            if first + count < total {
                sheet.body.overflow();
            }
            break;
        }
    }
}

/// Bound the supplied projection before allocating the existing wire view.
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

#[derive(Default)]
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

fn surface(tone: Role, canvas: Canvas) -> Style {
    let style = role::style(tone, canvas.color);
    if canvas.color {
        style.bg(Color::Rgb(23, 33, 53))
    } else {
        style
    }
}

/// One card, always five rows. Colours identify the verb on its own row;
/// only a supplied observation colours the status and the border.
fn card(
    doc: &GraphDoc,
    index: usize,
    width: usize,
    canvas: Canvas,
    observed: &dyn Fn(&str) -> Option<(String, Role)>,
    labels: &mut Labels,
) -> Vec<Line<'static>> {
    let Some(node) = doc.nodes.get(index) else {
        return Vec::new();
    };
    let observation = observed(&node.id);
    let tone = observation.as_ref().map_or(Role::Dim, |(_, tone)| *tone);
    let (tl, tr, bl, br, h, v) = if canvas.ascii {
        ("+", "+", "+", "+", "-", "|")
    } else {
        ("╭", "╮", "╰", "╯", "─", "│")
    };
    let border = |left: &str, right: &str| {
        Line::from(Span::styled(
            format!("{left}{}{right}", h.repeat(width.saturating_sub(2))),
            surface(tone, canvas),
        ))
    };
    let theme = Theme::new(false, canvas.ascii, false);
    let title = format!(
        "{} {}",
        theme.verb_glyph_bare(Some(node.verb)),
        labels.text(&node.id, canvas)
    );
    let target = node
        .tool
        .as_deref()
        .or(node.model.as_deref())
        .unwrap_or(node.kind);
    let meta = format!(
        "{}{}{}",
        node.verb,
        cells::sep(canvas.ascii),
        labels.text(target, canvas)
    );
    let (status, status_role) = observation.unwrap_or_else(|| ("definition".to_owned(), Role::Dim));
    let status = labels.text(&status, canvas);
    let rows = [
        (title, Role::Strong),
        (meta, Role::for_verb(node.verb).unwrap_or(Role::Dim)),
        (status, status_role),
    ];
    let mut out = vec![border(tl, tr)];
    for (text, role) in rows {
        let room = width.saturating_sub(4);
        let (line, cut) = cells::fit(
            vec![Span::styled(text, surface(role, canvas))],
            room,
            false,
            canvas,
        );
        labels.shortened |= cut;
        let used: usize = line.spans.iter().map(|s| cells::width(&s.content)).sum();
        let mut spans = vec![Span::styled(format!("{v} "), surface(tone, canvas))];
        spans.extend(
            line.spans
                .into_iter()
                .map(|span| span.style(surface(role, canvas))),
        );
        spans.push(Span::styled(
            format!("{} {v}", " ".repeat(room.saturating_sub(used))),
            surface(tone, canvas),
        ));
        out.push(Line::from(spans));
    }
    out.push(border(bl, br));
    out
}

/// Dependencies are always exact, including skipped-wave and typed edges.
fn dependencies(
    sheet: &mut Sheet,
    doc: &GraphDoc,
    cursor: usize,
    count: usize,
    labels: &mut Labels,
) {
    let canvas = sheet.body.canvas();
    let Some(nodes) = doc.nodes.get(cursor..cursor + count) else {
        return;
    };
    for edge in &doc.edges {
        if !nodes.iter().any(|node| node.id == edge.to) {
            continue;
        }
        let from = labels.text(&edge.from, canvas);
        let to = labels.text(&edge.to, canvas);
        let kind = labels.text(edge.kind, canvas);
        let predicate = edge
            .predicate
            .map(|p| format!(" / {}", labels.text(p, canvas)))
            .unwrap_or_default();
        let arrow = if canvas.ascii { "->" } else { "→" };
        let text = format!("{from} {arrow} {to} · {kind}{predicate}");
        if !sheet.body.push(
            vec![paint(
                cells::dots(&text, canvas.ascii),
                Role::Dim,
                canvas.color,
            )],
            false,
        ) {
            break;
        }
    }
}

/// A vertical join/fan only after the engine's wire validator accepted
/// the whole graph, and only for the full adjacent component pictured.
/// Other accepted shapes keep their named edges instead of a guessed rail.
fn connector(
    sheet: &mut Sheet,
    from: &[(String, String)],
    to: &[(String, String)],
    edges: &[(String, String)],
) {
    let canvas = sheet.body.canvas();
    if canvas.width < 12
        || from.len() > 2
        || to.len() > 2
        || ((from.len() > 1 || to.len() > 1) && canvas.width < 52)
    {
        return;
    }
    if from.len() == 2 && to.len() == 2 {
        return;
    }
    if !from.iter().all(|(a, _)| {
        to.iter()
            .all(|(b, _)| edges.iter().any(|edge| &edge.0 == a && &edge.1 == b))
    }) {
        return;
    }
    let width = usize::from(canvas.width);
    let center = width / 2;
    let left = (width.saturating_sub(2) / 2) / 2;
    let right = left + width.saturating_sub(2) / 2 + 2;
    let (h, down, join, split) = if canvas.ascii {
        ('-', 'v', '+', '+')
    } else {
        ('─', '▼', '┴', '┬')
    };
    let mut row = vec![' '; width];
    if from.len() == 1 && to.len() == 1 {
        row[center] = down;
    } else {
        row[left..=right].fill(h);
        row[left] = if canvas.ascii {
            '+'
        } else if from.len() == 2 {
            '└'
        } else {
            '┌'
        };
        row[right] = if canvas.ascii {
            '+'
        } else if from.len() == 2 {
            '┘'
        } else {
            '┐'
        };
        row[center] = if from.len() == 2 { join } else { split };
    }
    sheet.body.push(
        vec![paint(
            row.into_iter().collect::<String>(),
            Role::Dim,
            canvas.color,
        )],
        false,
    );
    if to.len() == 2 {
        let mut row = vec![' '; width];
        row[left] = down;
        row[right] = down;
        sheet.body.push(
            vec![paint(
                row.into_iter().collect::<String>(),
                Role::Dim,
                canvas.color,
            )],
            false,
        );
    } else if from.len() == 2 {
        sheet.body.push(
            vec![paint(
                format!("{}{}", " ".repeat(center), down),
                Role::Dim,
                canvas.color,
            )],
            false,
        );
    }
}
