// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! The approved graph composition over real checked projections: one row of
//! compact cards per wave, real wires wherever this layout can draw every
//! edge truthfully, and exact typed dependencies wherever a wire would not
//! say it. Each test stands in for the owner: it audits the fixture bytes
//! once and hands the projection and its waves over, as the Session does.

use std::collections::{BTreeMap, BTreeSet};

use nika_cli_host::oracle::{Audit, AuditOptions, audit_source};
use nika_display::dag_art::{GraphDoc, project};
use nika_display::theme::Role;
use nika_tui_view::{Canvas, Limits, Note, Rendered, graph_cards};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Four tasks, three waves: the diamond the approved object pane shows.
const DIAMOND: &str = r#"nika: view-diamond
permits:
  fs: { read: ["./notes/brief.md"], write: ["./out/a.md", "./out/b.md"] }
  tools: ["nika:read", "nika:write", "nika:log"]
tasks:
  source:
    invoke: { tool: "nika:read", args: { path: "./notes/brief.md" } }
  left:
    with: { t: "${{ tasks.source.output }}" }
    invoke: { tool: "nika:write", args: { path: "./out/a.md", content: "${{ with.t }}" } }
  right:
    with: { t: "${{ tasks.source.output }}" }
    invoke: { tool: "nika:write", args: { path: "./out/b.md", content: "${{ with.t }}" } }
  join:
    with: { a: "${{ tasks.left.output }}", b: "${{ tasks.right.output }}" }
    invoke: { tool: "nika:log", args: { message: "done ${{ with.a }} ${{ with.b }}" } }
"#;

/// The approved screen's shape: a fan-out, a join, then a chain.
const WATCH: &str = r#"nika: project-watch
permits:
  fs: { read: ["./sources.json"], write: ["./brief.md"] }
  tools: ["nika:read", "nika:write", "nika:log"]
tasks:
  read_sources:
    invoke: { tool: "nika:read", args: { path: "./sources.json" } }
  select_updates:
    with: { s: "${{ tasks.read_sources.output }}" }
    invoke: { tool: "nika:log", args: { message: "select ${{ with.s }}" } }
  verify_references:
    with: { s: "${{ tasks.read_sources.output }}" }
    invoke: { tool: "nika:log", args: { message: "verify ${{ with.s }}" } }
  write_brief:
    with: { a: "${{ tasks.select_updates.output }}", b: "${{ tasks.verify_references.output }}" }
    invoke: { tool: "nika:log", args: { message: "brief ${{ with.a }} ${{ with.b }}" } }
  write_result:
    with: { t: "${{ tasks.write_brief.output }}" }
    invoke: { tool: "nika:write", args: { path: "./brief.md", content: "${{ with.t }}" } }
"#;

/// A wave-skipping edge (`read_notes -> save`) no single joint can draw.
const CLEAN: &str = r#"nika: view-clean
permits:
  fs: { read: ["./notes/**"], write: ["./out/summary.md", "./out/digest.md"] }
  tools: ["nika:read", "nika:write", "nika:prompt"]
tasks:
  read_notes:
    invoke:
      tool: "nika:read"
      args: { path: "./notes/brief.md" }
  approve:
    with: { brief: "${{ tasks.read_notes.output }}" }
    invoke:
      tool: "nika:prompt"
      args: { message: "Publish ${{ with.brief }}?" }
  save:
    with: { text: "${{ tasks.read_notes.output }}", go: "${{ tasks.approve.output }}" }
    when: "${{ with.go == true }}"
    invoke:
      tool: "nika:write"
      args: { path: "./out/summary.md", content: "${{ with.text }}" }
  digest:
    with: { text: "${{ tasks.read_notes.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "./out/digest.md", content: "${{ with.text }}" }
"#;

const CHAIN: &str = r#"nika: chain
permits:
  tools: ["nika:log"]
tasks:
  one:
    invoke: { tool: "nika:log", args: { message: "one" } }
  two:
    with: { x: "${{ tasks.one.output }}" }
    invoke: { tool: "nika:log", args: { message: "two ${{ with.x }}" } }
  three:
    with: { x: "${{ tasks.two.output }}" }
    invoke: { tool: "nika:log", args: { message: "three ${{ with.x }}" } }
"#;

/// Ordering edges whose predicates a plain wire cannot say.
const TYPED: &str = r#"nika: typed-chain
permits: { exec: ["true"] }
tasks:
  first:
    exec: { command: ["true"] }
  rescue:
    after: { first: failure }
    exec: { command: ["true"] }
  report:
    after: { rescue: terminal }
    exec: { command: ["true"] }
"#;

const FAN: &str = r#"nika: fan
permits:
  tools: ["nika:log"]
tasks:
  root:
    invoke: { tool: "nika:log", args: { message: "root" } }
  a:
    with: { x: "${{ tasks.root.output }}" }
    invoke: { tool: "nika:log", args: { message: "a ${{ with.x }}" } }
  b:
    with: { x: "${{ tasks.root.output }}" }
    invoke: { tool: "nika:log", args: { message: "b ${{ with.x }}" } }
  c:
    with: { x: "${{ tasks.root.output }}" }
    invoke: { tool: "nika:log", args: { message: "c ${{ with.x }}" } }
  d:
    with: { x: "${{ tasks.root.output }}" }
    invoke: { tool: "nika:log", args: { message: "d ${{ with.x }}" } }
  e:
    with: { x: "${{ tasks.root.output }}" }
    invoke: { tool: "nika:log", args: { message: "e ${{ with.x }}" } }
"#;

/// Two independent chains side by side.
const PARALLEL: &str = r#"nika: parallel
permits:
  tools: ["nika:log"]
tasks:
  a:
    invoke: { tool: "nika:log", args: { message: "a" } }
  b:
    invoke: { tool: "nika:log", args: { message: "b" } }
  c:
    with: { x: "${{ tasks.a.output }}" }
    invoke: { tool: "nika:log", args: { message: "c ${{ with.x }}" } }
  d:
    with: { x: "${{ tasks.b.output }}" }
    invoke: { tool: "nika:log", args: { message: "d ${{ with.x }}" } }
"#;

/// Every task of the first wave feeds every task of the second.
const COMPLETE: &str = r#"nika: complete-join
permits:
  tools: ["nika:log"]
tasks:
  a:
    invoke: { tool: "nika:log", args: { message: "a" } }
  b:
    invoke: { tool: "nika:log", args: { message: "b" } }
  c:
    with: { x: "${{ tasks.a.output }}", y: "${{ tasks.b.output }}" }
    invoke: { tool: "nika:log", args: { message: "c ${{ with.x }} ${{ with.y }}" } }
  d:
    with: { x: "${{ tasks.a.output }}", y: "${{ tasks.b.output }}" }
    invoke: { tool: "nika:log", args: { message: "d ${{ with.x }} ${{ with.y }}" } }
"#;

/// A partial join: one shared joint would claim the missing `b -> c`.
const PARTIAL: &str = r#"nika: partial-join
permits:
  tools: ["nika:log"]
tasks:
  a:
    invoke: { tool: "nika:log", args: { message: "a" } }
  b:
    invoke: { tool: "nika:log", args: { message: "b" } }
  c:
    with: { x: "${{ tasks.a.output }}" }
    invoke: { tool: "nika:log", args: { message: "c ${{ with.x }}" } }
  d:
    with: { x: "${{ tasks.a.output }}", y: "${{ tasks.b.output }}" }
    invoke: { tool: "nika:log", args: { message: "d ${{ with.x }} ${{ with.y }}" } }
"#;

/// Three roots, the outer two joined: the join's drop falls under `b`.
const DROP_UNDER: &str = r#"nika: drop-under
permits:
  tools: ["nika:log"]
tasks:
  a:
    invoke: { tool: "nika:log", args: { message: "a" } }
  b:
    invoke: { tool: "nika:log", args: { message: "b" } }
  c:
    invoke: { tool: "nika:log", args: { message: "c" } }
  d:
    with: { x: "${{ tasks.a.output }}", y: "${{ tasks.c.output }}" }
    invoke: { tool: "nika:log", args: { message: "d ${{ with.x }} ${{ with.y }}" } }
"#;

/// Three roots, the middle one feeding three tasks: the fan's corners stand
/// under the outer roots.
const FAN_UNDER: &str = r#"nika: fan-under
permits:
  tools: ["nika:log"]
tasks:
  a:
    invoke: { tool: "nika:log", args: { message: "a" } }
  b:
    invoke: { tool: "nika:log", args: { message: "b" } }
  c:
    invoke: { tool: "nika:log", args: { message: "c" } }
  d:
    with: { x: "${{ tasks.b.output }}" }
    invoke: { tool: "nika:log", args: { message: "d ${{ with.x }}" } }
  e:
    with: { x: "${{ tasks.b.output }}" }
    invoke: { tool: "nika:log", args: { message: "e ${{ with.x }}" } }
  f:
    with: { x: "${{ tasks.b.output }}" }
    invoke: { tool: "nika:log", args: { message: "f ${{ with.x }}" } }
"#;

/// A cleanup unit (`unwind`) outside the task waves.
const CLEANUP: &str = "nika: cleanup-cards\npermits: { exec: [\"true\"] }\ntasks:\n  first:\n    exec: { command: [\"true\"] }\n  tidy:\n    after: { first: unwind }\n    exec: { command: [\"true\"] }\n  last:\n    after: { first: success }\n    exec: { command: [\"true\"] }\n";

const DIAMOND_AT_62: [&str; 14] = [
    "┌────────────────────────────────────────────────────────────┐",
    "│ source                                                     │",
    "│ invoke · nika:read                                         │",
    "└──────────────────────────────┬─────────────────────────────┘",
    "               ┌───────────────┴───────────────┐",
    "┌──────────────▼─────────────┐  ┌──────────────▼─────────────┐",
    "│ left                       │  │ right                      │",
    "│ invoke · nika:write        │  │ invoke · nika:write        │",
    "└──────────────┬─────────────┘  └──────────────┬─────────────┘",
    "               └───────────────┬───────────────┘",
    "┌──────────────────────────────▼─────────────────────────────┐",
    "│ join                                                       │",
    "│ invoke · nika:log                                          │",
    "└────────────────────────────────────────────────────────────┘",
];

const CLEAN_AT_62: [&str; 16] = [
    "+------------------------------------------------------------+",
    "| read_notes                                                 |",
    "| invoke - nika:read                                         |",
    "+------------------------------------------------------------+",
    "read_notes -> approve - value",
    "read_notes -> digest - value",
    "+----------------------------+  +----------------------------+",
    "| approve                    |  | digest                     |",
    "| invoke - nika:prompt       |  | invoke - nika:write        |",
    "+----------------------------+  +----------------------------+",
    "approve -> save - value",
    "read_notes -> save - value",
    "+------------------------------------------------------------+",
    "| save                                                       |",
    "| invoke - nika:write                                        |",
    "+------------------------------------------------------------+",
];

const CHAIN_AT_40: [&str; 14] = [
    "+--------------------------------------+",
    "| one                                  |",
    "| invoke - nika:log                    |",
    "+-------------------+------------------+",
    "                    |",
    "+-------------------v------------------+",
    "| two                                  |",
    "| invoke - nika:log                    |",
    "+-------------------+------------------+",
    "                    |",
    "+-------------------v------------------+",
    "| three                                |",
    "| invoke - nika:log                    |",
    "+--------------------------------------+",
];

/// Every task of the first wave feeds every task of the second: one bar
/// whose side tees climb into both exits and drop into both entries.
const COMPLETE_AT_56: [&str; 9] = [
    "┌─────────────────────────┐  ┌─────────────────────────┐",
    "│ a                       │  │ b                       │",
    "│ invoke · nika:log       │  │ invoke · nika:log       │",
    "└────────────┬────────────┘  └────────────┬────────────┘",
    "             ├────────────────────────────┤",
    "┌────────────▼────────────┐  ┌────────────▼────────────┐",
    "│ c                       │  │ d                       │",
    "│ invoke · nika:log       │  │ invoke · nika:log       │",
    "└─────────────────────────┘  └─────────────────────────┘",
];

/// The join's drop passes under `b`, which it does not join: `b`'s frame
/// carries no exit and no stroke climbs into it.
const DROP_UNDER_AT_70: [&str; 9] = [
    "┌────────────────────┐  ┌────────────────────┐  ┌────────────────────┐",
    "│ a                  │  │ b                  │  │ c                  │",
    "│ invoke · nika:log  │  │ invoke · nika:log  │  │ invoke · nika:log  │",
    "└──────────┬─────────┘  └────────────────────┘  └──────────┬─────────┘",
    "           └───────────────────────┬───────────────────────┘",
    "┌──────────────────────────────────▼─────────────────────────────────┐",
    "│ d                                                                  │",
    "│ invoke · nika:log                                                  │",
    "└────────────────────────────────────────────────────────────────────┘",
];

/// The fan's corners stand under `a` and `c`, which it does not join, and
/// its cross is the one place where an exit and an entry share a column.
const FAN_UNDER_AT_70: [&str; 9] = [
    "┌────────────────────┐  ┌────────────────────┐  ┌────────────────────┐",
    "│ a                  │  │ b                  │  │ c                  │",
    "│ invoke · nika:log  │  │ invoke · nika:log  │  │ invoke · nika:log  │",
    "└────────────────────┘  └──────────┬─────────┘  └────────────────────┘",
    "           ┌───────────────────────┼───────────────────────┐",
    "┌──────────▼─────────┐  ┌──────────▼─────────┐  ┌──────────▼─────────┐",
    "│ d                  │  │ e                  │  │ f                  │",
    "│ invoke · nika:log  │  │ invoke · nika:log  │  │ invoke · nika:log  │",
    "└────────────────────┘  └────────────────────┘  └────────────────────┘",
];

/// The directions a wire glyph joins: up, down, left, right. `None` for
/// any other character.
fn strokes(cell: char) -> Option<(bool, bool, bool, bool)> {
    Some(match cell {
        '│' => (true, true, false, false),
        '─' => (false, false, true, true),
        '┌' => (false, true, false, true),
        '┐' => (false, true, true, false),
        '└' => (true, false, false, true),
        '┘' => (true, false, true, false),
        '├' => (true, true, false, true),
        '┤' => (true, true, true, false),
        '┬' => (false, true, true, true),
        '┴' => (true, false, true, true),
        '┼' => (true, true, true, true),
        _ => return None,
    })
}

/// What the owner hands over for one source: the audit and its projection.
struct Fixture {
    audit: Audit,
    doc: GraphDoc,
}

impl Fixture {
    fn new(source: &str) -> Self {
        let audit = audit_source(source, "graph.nika", None, None, AuditOptions::default())
            .expect("the fixture parses");
        let doc = project(&audit.wf, &audit.report);
        Self { audit, doc }
    }

    fn cards(&self, canvas: Canvas, observed: &dyn Fn(&str) -> Option<(String, Role)>) -> Rendered {
        graph_cards(&self.doc, &self.audit.report.waves, canvas, observed)
    }

    fn definition(&self, canvas: Canvas) -> Rendered {
        self.cards(canvas, &|_| None)
    }

    /// Every distinct `(from, to)` pair the projection holds.
    fn pairs(&self) -> BTreeSet<(String, String)> {
        self.doc
            .edges
            .iter()
            .map(|edge| (edge.from.clone(), edge.to.clone()))
            .collect()
    }
}

fn text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn rows(rendered: &Rendered) -> Vec<String> {
    rendered.lines.iter().map(text).collect()
}

/// One character per terminal cell: a wide character is followed by a NUL
/// that stands for its second cell, so an index is a column.
fn grid(rows: &[String]) -> Vec<Vec<char>> {
    rows.iter()
        .map(|row| {
            let mut cells = Vec::new();
            for c in row.chars() {
                cells.push(c);
                // A zero-width or control character adds no second cell.
                let padding = c.width().unwrap_or(1).saturating_sub(1);
                cells.extend(std::iter::repeat_n('\0', padding));
            }
            cells
        })
        .collect()
}

/// The column where `needle` starts on `row`.
fn column_of(row: &str, needle: &str) -> usize {
    let at = row.find(needle).expect("the needle is on the row");
    row[..at].width()
}

/// The style painted on the cell at `column`.
fn style_at(line: &Line<'_>, column: usize) -> Style {
    let mut at = 0;
    for span in &line.spans {
        for c in span.content.chars() {
            let cells = c.width().unwrap_or(0);
            if column >= at && column < at + cells.max(1) {
                return span.style;
            }
            at += cells;
        }
    }
    panic!("column {column} is past {:?}", text(line));
}

/// One card as a Unicode drawing shows it: frames, title, entries, exits.
struct Drawn {
    title: String,
    top: usize,
    entries: Vec<usize>,
    exits: Vec<usize>,
}

/// Whether the frame opened at `top` row, `left` and `right` columns is a
/// whole card: two framed rows, then its bottom corners.
fn framed(grid: &[Vec<char>], top: usize, left: usize, right: usize) -> bool {
    let at = |row: usize, col: usize| grid.get(row).and_then(|r| r.get(col)).copied();
    (1..=2).all(|row| at(top + row, left) == Some('│') && at(top + row, right) == Some('│'))
        && at(top + 3, left) == Some('└')
        && at(top + 3, right) == Some('┘')
}

fn drawn_cards(grid: &[Vec<char>]) -> Vec<Drawn> {
    let mut cards = Vec::new();
    for (top, row) in grid.iter().enumerate() {
        let mut from = 0;
        while let Some(left) = (from..row.len()).find(|&c| row[c] == '┌') {
            let Some(right) = (left + 1..row.len()).find(|&c| row[c] == '┐') else {
                break;
            };
            from = right + 1;
            if !framed(grid, top, left, right) {
                continue;
            }
            let inner: String = grid[top + 1][left + 1..right].iter().collect();
            cards.push(Drawn {
                title: inner
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_owned(),
                top,
                entries: (left + 1..right).filter(|&c| row[c] == '▼').collect(),
                exits: (left + 1..right)
                    .filter(|&c| grid[top + 3][c] == '┬')
                    .collect(),
            });
        }
    }
    cards
}

/// The edges a Unicode drawing claims, read stroke by stroke. Each run of
/// wire on the row between two card rows must be one joint: every cell a
/// wire glyph that joins its neighbours, no end left open, a stroke up
/// exactly where a card above has its exit mark and a stroke down exactly
/// where a card below has its entry mark. The joint then links every card
/// whose exit it takes to every card whose entry it feeds. A broken joint,
/// a stroke into a card without a mark, or a mark no wire reaches fails.
fn drawn_edges(rows: &[String]) -> BTreeSet<(String, String)> {
    let grid = grid(rows);
    let cards = drawn_cards(&grid);
    let mut edges = BTreeSet::new();
    let mut reached = BTreeSet::new();
    for (index, wire) in grid.iter().enumerate() {
        let between = cards.iter().any(|card| card.top + 4 == index)
            && cards.iter().any(|card| card.top == index + 1);
        if !between {
            continue;
        }
        let exits: BTreeMap<usize, &Drawn> = cards
            .iter()
            .filter(|card| card.top + 4 == index)
            .flat_map(|card| card.exits.iter().map(move |&x| (x, card)))
            .collect();
        let entries: BTreeMap<usize, &Drawn> = cards
            .iter()
            .filter(|card| card.top == index + 1)
            .flat_map(|card| card.entries.iter().map(move |&x| (x, card)))
            .collect();
        let mut col = 0;
        while col < wire.len() {
            if wire[col] == ' ' {
                col += 1;
                continue;
            }
            let lo = col;
            while col < wire.len() && wire[col] != ' ' {
                col += 1;
            }
            let mut sources = Vec::new();
            let mut targets = Vec::new();
            for (x, &cell) in wire.iter().enumerate().take(col).skip(lo) {
                let Some((up, down, left, right)) = strokes(cell) else {
                    panic!("broken joint, row {index}: {cell:?} at {x} is not a wire");
                };
                assert!(
                    left == (x > lo) && right == (x + 1 < col),
                    "broken joint, row {index}: {cell:?} at {x} does not join its run"
                );
                assert_eq!(
                    up,
                    exits.contains_key(&x),
                    "broken joint, row {index}: a stroke up at {x} meets an exit, only an exit"
                );
                assert_eq!(
                    down,
                    entries.contains_key(&x),
                    "broken joint, row {index}: a stroke down at {x} meets an entry, only an entry"
                );
                if let Some(card) = exits.get(&x) {
                    reached.insert((card.top, x));
                    sources.push(card.title.clone());
                }
                if let Some(card) = entries.get(&x) {
                    reached.insert((card.top, x));
                    targets.push(card.title.clone());
                }
            }
            assert!(
                !sources.is_empty() && !targets.is_empty(),
                "broken joint, row {index}: a wire from nowhere or to nowhere at {lo}"
            );
            for from in &sources {
                for to in &targets {
                    edges.insert((from.clone(), to.clone()));
                }
            }
        }
    }
    for card in &cards {
        for x in card.exits.iter().chain(&card.entries) {
            assert!(
                reached.contains(&(card.top, *x)),
                "{}: a mark at column {x} that no wire reaches",
                card.title
            );
        }
    }
    edges
}

#[test]
fn the_approved_diamond_is_compact_cards_joined_by_real_wires() {
    let diamond = Fixture::new(DIAMOND);
    let rendered = diamond.definition(Canvas::new(62, false, false));
    let rows = rows(&rendered);
    assert_eq!(rows, DIAMOND_AT_62, "\n{}", rows.join("\n"));
    assert_eq!(
        rendered.facts,
        [
            "4 tasks · 4 edges · 3 waves",
            "definition · no task state observed"
        ]
    );
    assert!(rendered.notes.is_empty(), "{:?}", rendered.notes);
    assert_eq!(drawn_edges(&rows), diamond.pairs());
}

#[test]
fn the_approved_five_task_shape_fits_the_object_pane_with_exact_wires() {
    let watch = Fixture::new(WATCH);
    let rendered = watch.definition(Canvas::new(56, false, false));
    let rows = rows(&rendered);
    let all = rows.join("\n");
    assert_eq!(rows.len(), 19, "four rows a card, one a joint:\n{all}");
    assert_eq!(drawn_edges(&rows), watch.pairs(), "\n{all}");
    let cards = drawn_cards(&grid(&rows));
    let titles: Vec<&str> = cards.iter().map(|card| card.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "read_sources",
            "select_updates",
            "verify_references",
            "write_brief",
            "write_result"
        ]
    );
    assert_eq!(cards[1].top, cards[2].top, "the parallel pair shares a row");
    for (row, detail) in [
        (2, "invoke · nika:read"),
        (7, "invoke · nika:log"),
        (17, "invoke · nika:write"),
    ] {
        assert!(rows[row].contains(detail), "{}", rows[row]);
    }
    assert!(
        rows.iter()
            .all(|row| !row.contains('→') && !row.contains("wave")),
        "no dependency prose and no wave heading beside true wires:\n{all}"
    );
    assert!(rendered.notes.is_empty(), "{:?}", rendered.notes);
}

#[test]
fn a_drawn_wire_keeps_the_words_of_a_typed_edge() {
    let typed = Fixture::new(TYPED);
    let rendered = typed.definition(Canvas::new(48, false, false));
    let rows = rows(&rendered);
    assert_eq!(drawn_edges(&rows), typed.pairs(), "\n{}", rows.join("\n"));
    // A wire says that a task follows another, not on which outcome: the
    // predicate of each ordering edge follows the drawing, never inside it.
    assert_eq!(rows.len(), 16, "\n{}", rows.join("\n"));
    assert_eq!(
        rows[14..],
        [
            "first → rescue · control / failure",
            "rescue → report · control / terminal"
        ]
    );
    assert!(
        !rendered
            .facts
            .iter()
            .any(|fact| fact.starts_with("cards by wave"))
    );
}

#[test]
fn a_linear_chain_is_one_rail_through_the_card_centres() {
    let chain = Fixture::new(CHAIN);
    let rendered = chain.definition(Canvas::new(40, true, false));
    let rows = rows(&rendered);
    assert_eq!(rows, CHAIN_AT_40, "\n{}", rows.join("\n"));
    assert!(rendered.notes.is_empty(), "{:?}", rendered.notes);
}

#[test]
fn a_wave_skipping_edge_keeps_exact_dependencies_and_draws_no_wire() {
    let clean = Fixture::new(CLEAN);
    let rendered = clean.definition(Canvas::new(62, true, false));
    let rows = rows(&rendered);
    assert_eq!(rows, CLEAN_AT_62, "\n{}", rows.join("\n"));
    assert_eq!(
        rendered.facts,
        [
            "4 tasks - 4 edges - 3 waves",
            "cards by wave; exact dependencies listed, no inferred wires",
            "definition - no task state observed"
        ]
    );
}

#[test]
fn a_partial_join_is_listed_while_parallel_rails_and_a_full_join_are_drawn() {
    for source in [PARALLEL, COMPLETE] {
        let fixture = Fixture::new(source);
        let rendered = fixture.definition(Canvas::new(56, false, false));
        let rows = rows(&rendered);
        assert_eq!(rows.len(), 9, "\n{}", rows.join("\n"));
        assert_eq!(drawn_edges(&rows), fixture.pairs(), "\n{}", rows.join("\n"));
    }
    let complete = rows(&Fixture::new(COMPLETE).definition(Canvas::new(56, false, false)));
    assert_eq!(complete, COMPLETE_AT_56, "\n{}", complete.join("\n"));
    let partial = Fixture::new(PARTIAL);
    let rendered = partial.definition(Canvas::new(56, false, false));
    let rows = rows(&rendered);
    assert!(
        rows.iter()
            .all(|row| !row.contains('▼') && !row.contains('┬')),
        "one joint would also claim b → c:\n{}",
        rows.join("\n")
    );
    for words in ["a → c · value", "a → d · value", "b → d · value"] {
        assert_eq!(
            rows.iter().filter(|row| row.as_str() == words).count(),
            1,
            "{words}:\n{}",
            rows.join("\n")
        );
    }
    assert!(
        rendered
            .facts
            .iter()
            .any(|fact| fact.starts_with("cards by wave"))
    );
}

/// Three-card waves share the card-centre grid, so a joint can pass under a
/// card it does not join. The drawing stays truthful and is kept: that
/// card's frame carries no mark and no stroke turns into it. Pinned so that
/// changing this composition is a decision, not a drift.
#[test]
fn a_joint_beside_a_card_it_does_not_join_marks_nothing_on_that_card() {
    for (source, expected) in [(DROP_UNDER, DROP_UNDER_AT_70), (FAN_UNDER, FAN_UNDER_AT_70)] {
        let fixture = Fixture::new(source);
        let rendered = fixture.definition(Canvas::new(70, false, false));
        let rows = rows(&rendered);
        assert_eq!(rows, expected, "\n{}", rows.join("\n"));
        assert_eq!(drawn_edges(&rows), fixture.pairs(), "\n{}", rows.join("\n"));
        assert!(rendered.notes.is_empty(), "{:?}", rendered.notes);
    }
}

/// The oracle reads strokes, not runs: side tees that lost their bar are a
/// broken joint, though the same columns would still pair the same cards.
#[test]
#[should_panic(expected = "broken joint")]
fn the_edge_oracle_refuses_side_tees_detached_from_their_bar() {
    let mut rows: Vec<String> = COMPLETE_AT_56.iter().map(|row| (*row).to_owned()).collect();
    rows[4] = rows[4].replace(['├', '┤'], "│");
    drawn_edges(&rows);
}

/// A stroke climbing into a card that has no exit mark is refused, even when
/// the cards it would pair are still paired.
#[test]
#[should_panic(expected = "broken joint")]
fn the_edge_oracle_refuses_a_stroke_into_a_card_without_its_mark() {
    let mut rows: Vec<String> = DROP_UNDER_AT_70
        .iter()
        .map(|row| (*row).to_owned())
        .collect();
    rows[4] = rows[4].replace('┬', "┼");
    drawn_edges(&rows);
}

#[test]
fn a_wave_wider_than_the_pane_wraps_under_its_heading_without_wires() {
    let fan = Fixture::new(FAN);
    let narrow = rows(&fan.definition(Canvas::new(62, false, false)));
    let all = narrow.join("\n");
    let heading = narrow
        .iter()
        .position(|row| row == "wave 2 · 5 tasks")
        .expect("the wrapped wave names itself");
    for (offset, task) in ["a", "b", "c", "d", "e"].iter().enumerate() {
        assert_eq!(
            narrow[heading + 1 + offset],
            format!("root → {task} · value")
        );
    }
    let cards = drawn_cards(&grid(&narrow));
    let row_of = |title: &str| {
        cards
            .iter()
            .find(|card| card.title == title)
            .map(|card| card.top)
            .expect("every task has its card")
    };
    assert_eq!(row_of("a"), row_of("c"), "three cards share a row:\n{all}");
    assert_eq!(row_of("d"), row_of("e"), "{all}");
    assert_eq!(row_of("d"), row_of("a") + 4, "{all}");
    assert!(
        narrow
            .iter()
            .all(|row| !row.contains('▼') && !row.contains('┬')),
        "no wire is guessed for a wrapped wave:\n{all}"
    );
    let wide = rows(&fan.definition(Canvas::new(120, false, false)));
    assert_eq!(drawn_edges(&wide), fan.pairs(), "\n{}", wide.join("\n"));
    assert!(
        wide.iter()
            .all(|row| !row.contains('→') && !row.starts_with("wave")),
        "{}",
        wide.join("\n")
    );
}

#[test]
fn cleanup_units_keep_their_exact_edges_beside_the_wired_waves() {
    let cleanup = Fixture::new(CLEANUP);
    assert_eq!(cleanup.audit.report.waves, vec![vec![0], vec![2]]);
    let rendered = cleanup.definition(Canvas::new(48, false, false));
    let rows = rows(&rendered);
    let all = rows.join("\n");
    assert_eq!(
        drawn_edges(&rows),
        BTreeSet::from([("first".to_owned(), "last".to_owned())]),
        "\n{all}"
    );
    let heading = rows
        .iter()
        .position(|row| row == "outside task waves")
        .expect("the cleanup population is named");
    assert_eq!(rows[heading - 1], "first → last · control / success");
    assert_eq!(rows[heading + 1], "first → tidy · finally / unwind");
    let tidy = drawn_cards(&grid(&rows))
        .into_iter()
        .find(|card| card.title == "tidy")
        .expect("the cleanup unit keeps its card");
    assert_eq!(tidy.top, heading + 2, "{all}");
    assert!(rows[tidy.top + 2].contains("exec · cleanup"), "{all}");
    assert!(
        tidy.entries.is_empty() && tidy.exits.is_empty(),
        "no wire reaches a unit outside the waves:\n{all}"
    );
}

#[test]
fn observed_states_sit_on_their_cards_and_only_unobserved_cards_say_definition() {
    let diamond = Fixture::new(DIAMOND);
    let rendered = diamond.cards(Canvas::new(62, false, false), &|id| {
        (id == "source").then(|| ("✔ succeeded".to_owned(), Role::Good))
    });
    let rows = rows(&rendered);
    assert_eq!(
        rows.len(),
        DIAMOND_AT_62.len(),
        "an observation adds no row"
    );
    assert_eq!(rows[1], format!("│ source{}✔ succeeded │", " ".repeat(41)));
    assert_eq!(
        rows[6],
        format!(
            "│ left{}definition │  │ right{}definition │",
            " ".repeat(12),
            " ".repeat(11)
        )
    );
    assert_eq!(rows[11], format!("│ join{}definition │", " ".repeat(44)));
    let all = rows.join("\n");
    assert_eq!(all.matches("definition").count(), 3);
    assert_eq!(all.matches("✔ succeeded").count(), 1);
    assert_eq!(
        rendered.facts,
        ["4 tasks · 4 edges · 3 waves"],
        "the definition fact belongs to a graph without observations"
    );
    assert_eq!(drawn_edges(&rows), diamond.pairs());
}

#[test]
fn a_running_card_wears_the_approved_violet_and_selection() {
    let violet = Color::Rgb(182, 154, 255);
    let selection = Color::Rgb(37, 39, 62);
    let ground = Color::Rgb(13, 17, 25);
    let border = Color::Rgb(45, 59, 82);
    let muted = Color::Rgb(156, 172, 197);
    let ink = Color::Rgb(229, 233, 242);
    let diamond = Fixture::new(DIAMOND);
    let rendered = diamond.cards(Canvas::new(62, false, true), &|id| match id {
        "left" => Some(("◐ running".to_owned(), Role::Accent)),
        "source" => Some(("✔ succeeded".to_owned(), Role::Good)),
        _ => None,
    });
    let lines = &rendered.lines;
    let rows = rows(&rendered);
    let paint = |row: usize, column: usize| {
        let style = style_at(&lines[row], column);
        (style.fg, style.bg)
    };
    // The running card: the violet frame on the selection fill.
    assert_eq!(paint(5, 0), (Some(violet), Some(selection)));
    assert_eq!(paint(8, 29), (Some(violet), Some(selection)));
    assert_eq!(paint(6, 2), (Some(ink), Some(selection)), "its title");
    let running = column_of(&rows[6], "◐");
    assert_eq!(paint(6, running), (Some(violet), Some(selection)));
    assert_eq!(paint(5, 15), (Some(muted), Some(selection)), "its entry");
    // Idle cards keep the subdued frame on the dark ground.
    for (row, column) in [(0, 0), (5, 32), (10, 0), (13, 61)] {
        assert_eq!(
            paint(row, column),
            (Some(border), Some(ground)),
            "row {row}, column {column}"
        );
    }
    let done = column_of(&rows[1], "✔");
    assert_eq!(
        paint(1, done),
        (Some(Color::Rgb(126, 208, 160)), Some(ground))
    );
    let unobserved = column_of(&rows[6], "definition");
    assert_eq!(paint(6, unobserved), (Some(muted), Some(ground)));
    // Wires and the verbs of the definition recede in the muted ink.
    assert_eq!(paint(4, 15).0, Some(muted), "the joint");
    assert_eq!(paint(7, 2), (Some(muted), Some(selection)), "a verb");
    assert_eq!(paint(12, 2), (Some(muted), Some(ground)), "a verb");
}

#[test]
fn a_static_definition_paints_no_verb_identity_hue() {
    let verbs: Vec<Option<Color>> = [Role::VerbInfer, Role::VerbInvoke, Role::VerbAgent]
        .into_iter()
        .map(|role| nika_tui_view::visual::role::style(role, true).fg)
        .collect();
    for source in [DIAMOND, WATCH, CLEAN] {
        let rendered = Fixture::new(source).definition(Canvas::new(62, false, true));
        for line in &rendered.lines {
            for span in &line.spans {
                assert!(
                    !verbs.contains(&span.style.fg),
                    "a verb hue on a static card: {span:?}"
                );
            }
        }
    }
}

#[test]
fn without_colour_or_unicode_the_cards_keep_their_meaning() {
    let diamond = Fixture::new(DIAMOND);
    let rendered = diamond.cards(Canvas::new(62, true, false), &|id| {
        (id == "left").then(|| ("> running".to_owned(), Role::Accent))
    });
    let rows = rows(&rendered);
    let all = rows.join("\n");
    assert!(all.is_ascii(), "{all}");
    for span in rendered.lines.iter().flat_map(|line| &line.spans) {
        assert_eq!((span.style.fg, span.style.bg), (None, None), "{span:?}");
    }
    let card = |entry: &str| format!("+{}{entry}{}+", "-".repeat(14), "-".repeat(13));
    assert_eq!(rows[3], format!("+{}+{}+", "-".repeat(30), "-".repeat(29)));
    assert_eq!(
        rows[4],
        format!("{}+{}+{}+", " ".repeat(15), "-".repeat(15), "-".repeat(15))
    );
    assert_eq!(rows[5], format!("{}  {}", card("v"), card("v")));
    assert_eq!(rows[8], format!("{}  {}", card("+"), card("+")));
    assert_eq!(
        rows[6],
        format!(
            "| left{}> running |  | right{}definition |",
            " ".repeat(13),
            " ".repeat(11)
        )
    );
    // Weight stands in for hue: the running frame is bold, the rest recedes.
    let weight = |row: usize, column: usize| style_at(&rendered.lines[row], column).add_modifier;
    assert!(weight(5, 0).contains(Modifier::BOLD), "the running frame");
    assert!(weight(5, 32).contains(Modifier::DIM), "an idle frame");
    assert!(weight(4, 15).contains(Modifier::DIM), "a wire");
    assert!(weight(7, 2).contains(Modifier::DIM), "a verb");
    assert_eq!(weight(6, 2), Modifier::empty(), "a title stays plain");
    assert_eq!(rendered.facts, ["4 tasks - 4 edges - 3 waves"]);
}

#[test]
fn clipping_wide_and_control_text_keeps_every_frame_and_wire_whole() {
    let diamond = Fixture::new(DIAMOND);
    let rendered = diamond.cards(Canvas::new(40, false, false), &|_| {
        Some(("状態確認済み\u{7}".to_owned(), Role::Warn))
    });
    let rows = rows(&rendered);
    let all = rows.join("\n");
    for row in &rows {
        assert!(row.width() <= 40, "{row}");
        assert!(!row.chars().any(char::is_control), "{row:?}");
    }
    assert_eq!(
        rows[1],
        format!("│ source{}状態確認済み␇ │", " ".repeat(17))
    );
    assert_eq!(rows[6], "│ left  状態確認… │  │ right 状態確認… │");
    assert_eq!(rows[7], "│ invoke · nika:… │  │ invoke · nika:… │");
    // Each frame closes in its own column and every wire still lands.
    assert_eq!(drawn_cards(&grid(&rows)).len(), 4, "{all}");
    assert_eq!(drawn_edges(&rows), diamond.pairs(), "{all}");
    assert!(
        rendered
            .notes
            .iter()
            .any(|note| matches!(note, Note::Controls { count: 4 })),
        "{:?}",
        rendered.notes
    );
    assert!(
        rendered
            .notes
            .iter()
            .any(|note| matches!(note, Note::Fallback { .. })),
        "a shortened label is disclosed: {:?}",
        rendered.notes
    );
}

/// A line bound stops between whole blocks: a wave with the joint that feeds
/// it (a listed wave with its heading and incoming rows), one typed row, the
/// cleanup section. Never a joint without the cards it feeds, never half a
/// card: the rows kept are the longest whole prefix, and the cut is said.
#[test]
fn a_line_bound_stops_between_whole_blocks_and_says_so() {
    // The rows after each whole block of the full drawing.
    let cases: [(&str, u16, bool, &[usize]); 3] = [
        (WATCH, 56, false, &[4, 9, 14, 19]),
        (CLEAN, 62, true, &[4, 10, 16]),
        (CLEANUP, 48, false, &[4, 9, 10, 16]),
    ];
    for (source, width, ascii, blocks) in cases {
        let fixture = Fixture::new(source);
        let full = rows(&fixture.definition(Canvas::new(width, ascii, false)));
        let total = blocks.last().copied().unwrap_or_default();
        assert_eq!(full.len(), total, "\n{}", full.join("\n"));
        // Below one row per node the item bound refuses the whole graph.
        for limit in fixture.doc.nodes.len().max(blocks[0])..=total {
            let bound = Limits::new(256 * 1024, limit, 4096);
            let rendered = fixture.definition(Canvas::new(width, ascii, false).with_limits(bound));
            let shown = blocks
                .iter()
                .copied()
                .filter(|&block| block <= limit)
                .max()
                .unwrap_or(0);
            let kept = rows(&rendered);
            assert_eq!(kept.as_slice(), &full[..shown], "limit {limit}");
            let cut = rendered
                .notes
                .iter()
                .any(|note| matches!(note, Note::LinesCut { shown: at } if *at == shown));
            assert_eq!(cut, shown < total, "limit {limit}: {:?}", rendered.notes);
            assert!(
                kept.last().is_none_or(|row| !row.starts_with(' ')),
                "limit {limit}: the drawing ends on a joint"
            );
        }
    }
}

/// The summary counts what the cards draw: the cleanup units beside the
/// scheduled tasks, and no such word where there are none.
#[test]
fn the_facts_count_the_cleanup_units_the_cards_draw() {
    let cleanup = Fixture::new(CLEANUP);
    for (ascii, words) in [
        (false, "2 tasks · 1 cleanup unit · 2 edges · 2 waves"),
        (true, "2 tasks - 1 cleanup unit - 2 edges - 2 waves"),
    ] {
        let rendered = cleanup.definition(Canvas::new(48, ascii, false));
        assert_eq!(rendered.facts.first().map(String::as_str), Some(words));
    }
    let diamond = Fixture::new(DIAMOND).definition(Canvas::new(62, false, false));
    assert_eq!(
        diamond.facts.first().map(String::as_str),
        Some("4 tasks · 4 edges · 3 waves")
    );
}

#[test]
fn every_width_keeps_every_line_inside_the_pane() {
    for source in [DIAMOND, WATCH, CLEAN, FAN, CLEANUP, PARTIAL] {
        let fixture = Fixture::new(source);
        for width in 0..=130_u16 {
            for (ascii, color) in [(true, false), (false, false), (false, true)] {
                let canvas = Canvas::new(width, ascii, color);
                let rendered = fixture.definition(canvas);
                let head = rendered.head(canvas);
                for line in rendered.lines.iter().chain(&head) {
                    let row = text(line);
                    assert!(row.width() <= usize::from(width), "{width}: {row}");
                    assert!(!ascii || row.is_ascii(), "{width}: {row}");
                    if !color {
                        assert!(
                            line.spans
                                .iter()
                                .all(|span| span.style.fg.is_none() && span.style.bg.is_none()),
                            "{width}: a hue without colour in {row}"
                        );
                    }
                }
            }
        }
    }
}
