// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The look binds one read of one listed file: its bytes, their witness, the
//! check facade's audit and the graph of that same parse; it reads nothing
//! else, refuses what is not listed, and never stands for later bytes.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use nika_session::ProjectSnapshot;
use nika_session::change::Witness;
use nika_tui_view::Face;

use super::{LOOK_CAP, take};
use crate::workspace::inspect::NOT_CAPTURED;

/// A temporary directory, removed when the test ends.
struct Room(PathBuf);

impl Room {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path = std::env::temp_dir().join(format!(
            "nika-tui-look-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("room");
        Self(path)
    }

    fn write(&self, rel: &str, body: &str) {
        let path = self.0.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("dir");
        }
        std::fs::write(path, body).expect("write");
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A four-task diamond: one root, two branches, one join.
pub(crate) const DIAMOND: &str = r#"nika: diamond
permits: {}
tasks:
  a:
    invoke: { tool: "nika:log", args: { message: a } }
  b:
    with: { x: "${{ tasks.a.output }}" }
    invoke: { tool: "nika:log", args: { message: b } }
  c:
    with: { x: "${{ tasks.a.output }}" }
    invoke: { tool: "nika:log", args: { message: c } }
  d:
    with: { l: "${{ tasks.b.output }}", r: "${{ tasks.c.output }}" }
    invoke: { tool: "nika:log", args: { message: d } }
"#;

/// A second, distinct workflow: one task.
const SINGLE: &str = r#"nika: single
permits: {}
tasks:
  only:
    invoke: { tool: "nika:log", args: { message: only } }
"#;

fn text(
    face: &(
        ratatui::text::Line<'static>,
        Vec<ratatui::text::Line<'static>>,
    ),
) -> Vec<String> {
    std::iter::once(&face.0)
        .chain(face.1.iter())
        .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect()
}

fn observe(root: &Path) -> ProjectSnapshot {
    ProjectSnapshot::observe(root)
}

/// Source, plan, graph and check of one look all come from the bytes read,
/// named by their witness; the second workflow shows its own bytes.
#[test]
fn every_face_of_a_look_comes_from_the_one_read() {
    let room = Room::new("faces");
    room.write("diamond.nika", DIAMOND);
    room.write("single.nika", SINGLE);
    let snapshot = observe(&room.0);
    let look = take(&snapshot, "diamond.nika").expect("listed");
    let witness = Witness::of(DIAMOND.as_bytes()).0;
    assert_eq!(look.witness(), Some(witness.as_str()));
    assert_eq!((look.path(), look.title()), ("diamond.nika", "diamond"));
    let short: String = witness.chars().take(12).collect();
    for face in Face::ALL {
        let rows = text(&look.face_lines(face, 78, true, false));
        assert!(rows[0].contains(&format!("[{}]", face.label())), "{rows:?}");
        assert!(
            rows.iter().any(|r| r.contains(&short)),
            "{face:?}: {rows:?}"
        );
    }
    let source = text(&look.face_lines(Face::Source, 78, true, false)).join("\n");
    assert!(source.contains("message: d"), "{source}");
    let plan = text(&look.face_lines(Face::Plan, 78, true, false)).join("\n");
    for task in ["a", "b", "c", "d"] {
        assert!(plan.contains(task), "{plan}");
    }
    let graph = text(&look.face_lines(Face::Graph, 78, true, false)).join("\n");
    assert!(!graph.contains("no graph"), "{graph}");
    let other = take(&snapshot, "single.nika").expect("listed");
    assert_ne!(other.witness(), look.witness());
    let summary = text(&other.face_lines(Face::Graph, 78, true, false));
    assert!(
        summary
            .iter()
            .any(|row| row == "Definition - 1 task - 0 edges")
    );
    let single = text(&other.face_lines(Face::Source, 78, true, false)).join("\n");
    assert!(single.contains("message: only") && !single.contains("message: d"));
}

/// An ordering predicate never becomes a plain value dependency in the
/// short view. The original typed graph remains intact and scrollable.
#[test]
fn compact_graph_preserves_the_typed_predicate_fallback() {
    let room = Room::new("compact-predicates");
    room.write("typed.nika", "nika: typed\npermits: { exec: [\"true\"] }\ntasks:\n  first:\n    exec: { command: [\"true\"] }\n  rescue:\n    after: { first: failure }\n    exec: { command: [\"true\"] }\n");
    let look = take(&observe(&room.0), "typed.nika").expect("listed");
    for ascii in [false, true] {
        let short = text(&look.face_lines_in(Face::Graph, 80, ascii, false, true));
        let ordinary = text(&look.face_lines(Face::Graph, 80, ascii, false));
        assert_eq!(short, ordinary, "the typed edge stays in its original view");
        let arrow = if ascii { "->" } else { "→" };
        let sep = if ascii { " - " } else { " · " };
        assert!(
            short
                .iter()
                .any(|row| row.contains(&format!("first {arrow} rescue{sep}control / failure"))),
            "{short:?}"
        );
        assert!(!short.iter().any(|row| row.contains("Compact graph")));
    }
}

/// A compact projection must retain the graph viewer's conformance
/// refusal, including a cycle or a reference to an absent task.
#[test]
fn compact_graph_keeps_the_original_invalid_order_refusal() {
    let room = Room::new("compact-invalid-order");
    for (name, source) in [
        (
            "cycle.nika",
            "nika: broken\npermits: {}\ntasks:\n  a:\n    with: { x: \"${{ tasks.b.output }}\" }\n    invoke: { tool: \"nika:log\", args: { message: a } }\n  b:\n    with: { x: \"${{ tasks.a.output }}\" }\n    invoke: { tool: \"nika:log\", args: { message: b } }\n",
        ),
        (
            "missing.nika",
            "nika: broken\npermits: {}\ntasks:\n  a:\n    with: { x: \"${{ tasks.missing.output }}\" }\n    invoke: { tool: \"nika:log\", args: { message: a } }\n",
        ),
    ] {
        room.write(name, source);
        let look = take(&observe(&room.0), name).expect("listed");
        for ascii in [false, true] {
            let ordinary = text(&look.face_lines(Face::Graph, 80, ascii, false));
            assert!(
                ordinary
                    .iter()
                    .any(|row| row == "no graph: the check found no valid run order"),
                "{name}: {ordinary:?}"
            );
            let compact = text(&look.face_lines_in(Face::Graph, 80, ascii, false, true));
            assert_eq!(
                compact, ordinary,
                "{name}: the original graph refusal stays"
            );
            assert!(!compact.iter().any(|row| row.contains("Compact graph")));
        }
    }
}

/// A wide fan-in makes compact dependency rows wrap at the target width.
fn fan_in(count: usize) -> String {
    let mut source = "nika: fan-in\npermits: { exec: [\"true\"] }\ntasks:\n".to_owned();
    for i in 1..=count {
        writeln!(
            source,
            "  source_{i:02}:\n    exec: {{ command: [\"true\"] }}"
        )
        .expect("text");
    }
    source.push_str("  merge:\n    with:\n");
    for i in 1..=count {
        writeln!(
            source,
            "      x{i}: \"${{{{ tasks.source_{i:02}.output }}}}\""
        )
        .expect("text");
    }
    source.push_str("    exec: { command: [\"true\"] }\n");
    source
}

/// A wrapped dependency remains a continuation, never a second node row.
#[test]
fn compact_graph_fan_in_continuations_keep_their_row_identity() {
    let room = Room::new("compact-fan-in");
    room.write("fanin.nika", &fan_in(7));
    let look = take(&observe(&room.0), "fanin.nika").expect("listed");
    for ascii in [false, true] {
        let rows = text(&look.face_lines_in(Face::Graph, 80, ascii, false, true));
        let sep = if ascii { " - " } else { " · " };
        assert!(
            rows.iter().any(|row| row.starts_with("Compact graph")),
            "{rows:?}"
        );
        for i in 1..=7 {
            let expected = format!("source_{i:02}{sep}exec");
            assert_eq!(
                rows.iter().filter(|row| **row == expected).count(),
                1,
                "{rows:?}"
            );
        }
        let start = rows
            .iter()
            .position(|row| row.starts_with("merge "))
            .expect("merge");
        let end = rows[start..]
            .iter()
            .position(String::is_empty)
            .map(|i| start + i)
            .expect("end");
        assert!(end > start + 1, "the dependencies wrap at 80");
        assert!(
            rows[start + 1..end].iter().all(|row| row.starts_with("  ")),
            "{rows:?}"
        );
        assert!(
            rows.iter()
                .all(|row| unicode_width::UnicodeWidthStr::width(row.as_str()) <= 80)
        );
    }
}

/// A graph whose compact rows scroll restarts its visual projection at the
/// top when expansion changes it to cards; its candidate identity stays.
#[test]
fn compact_graph_format_change_starts_at_the_top_without_changing_the_candidate() {
    use crate::workspace::{
        candidate::Proposed,
        desk::{Desk, Route},
        focus::Region,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let room = Room::new("compact-format");
    room.write("fanin.nika", &fan_in(20));
    let look = take(&observe(&room.0), "fanin.nika").expect("listed");
    let witness = look.witness().map(str::to_owned);
    let mut desk = Desk::new();
    desk.proposed(Some(Proposed::new(
        nika_session::ProposalId::of("fan"),
        false,
        look,
    )));
    desk.face = Face::Graph;
    desk.focus.region = Region::Object;
    let size = (80, 48);
    desk.prepare(size, false, false);
    let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
    assert_eq!(desk.route(key(KeyCode::End), size), Route::Repaint);
    assert!(desk.focus.scroll > 0, "the compact graph really scrolls");
    assert_eq!(desk.route(key(KeyCode::F(4)), size), Route::Repaint);
    desk.prepare(size, false, false);
    assert_eq!(desk.focus.scroll, 0, "the card projection starts whole");
    assert_eq!(
        desk.candidate
            .as_ref()
            .and_then(|c| c.witness())
            .map(str::to_owned),
        witness
    );
    assert_eq!(desk.face, Face::Graph);
    assert_eq!(desk.focus.region, Region::Object);
}

/// Compact rows preserve all four exact diamond dependencies without
/// partial card borders; both glyph columns retain the same fixed witness.
#[test]
fn compact_graph_keeps_every_node_and_exact_dependency_in_a_short_object() {
    let room = Room::new("compact-graph");
    room.write("diamond.nika", DIAMOND);
    let look = take(&observe(&room.0), "diamond.nika").expect("listed");
    for ascii in [false, true] {
        let rows = text(&look.face_lines_in(Face::Graph, 80, ascii, false, true));
        let arrow = if ascii { " <- " } else { " ← " };
        let sep = if ascii { " - " } else { " · " };
        for (node, inputs) in [("a", ""), ("b", "a"), ("c", "a"), ("d", "b, c")] {
            let dependency = if inputs.is_empty() {
                String::new()
            } else {
                format!("{arrow}{inputs}")
            };
            let expected = format!("{node}{dependency}{sep}invoke");
            assert_eq!(
                rows.iter().filter(|row| **row == expected).count(),
                1,
                "{rows:?}"
            );
        }
        assert!(
            rows.iter()
                .any(|row| row == &format!("Structure checked{sep}this file only"))
        );
        assert!(
            !rows
                .iter()
                .any(|row| row.contains("RUN READY") || row.contains('┌'))
        );
        assert_eq!(rows.len(), 10, "one title and nine compact content rows");
        assert!(
            rows.iter()
                .all(|row| unicode_width::UnicodeWidthStr::width(row.as_str()) <= 80)
        );
    }
}

/// The check face of a look says it judged the file alone: what it imports
/// is UNKNOWN and RUN READY is never claimed from it.
#[test]
fn the_check_face_never_claims_the_composed_readiness() {
    let room = Room::new("parent");
    room.write("diamond.nika", DIAMOND);
    let snapshot = observe(&room.0);
    let look = take(&snapshot, "diamond.nika").expect("listed");
    let check = text(&look.face_lines(Face::Check, 100, true, false));
    let imports = check
        .iter()
        .find(|r| r.contains("IMPORTS"))
        .expect("an IMPORTS row");
    let head: String = NOT_CAPTURED.chars().take(30).collect();
    assert!(imports.contains(&head), "{imports}");
    let run = check
        .iter()
        .find(|r| r.contains("RUN READY"))
        .expect("a RUN READY row");
    assert!(run.contains("unknown"), "{run}");
    assert!(!run.contains("nothing known blocks a run"), "{run}");
    let graph = text(&look.face_lines(Face::Graph, 100, true, false)).join("\n");
    assert!(graph.contains("Definition - 4 tasks - 4 edges"), "{graph}");
    assert!(
        graph.contains("Structure checked - this file only"),
        "{graph}"
    );
    assert!(!graph.contains("RUN READY"), "{graph}");
}

/// Only an exactly listed path is looked at: a needle, an escaping path, a
/// project file and an unlisted file are refused.
#[test]
fn only_a_listed_path_is_looked_at() {
    let room = Room::new("listed");
    room.write("diamond.nika", DIAMOND);
    room.write("nika.yaml", "nika: demo\n");
    let snapshot = observe(&room.0);
    assert!(take(&snapshot, "diamond.nika").is_some());
    for refused in [
        "diamond",
        "./diamond.nika",
        "../diamond.nika",
        "nika.yaml",
        "absent.nika",
    ] {
        assert!(take(&snapshot, refused).is_none(), "{refused}");
    }
}

/// A source the parser refuses stays visible beside its refusal, and no
/// graph is invented for it.
#[test]
fn a_refused_source_stays_visible_with_its_refusal() {
    let room = Room::new("refused");
    let bad = "nika: bad\nbogus: 1\ntasks: {}\n";
    room.write("bad.nika", bad);
    let snapshot = observe(&room.0);
    let look = take(&snapshot, "bad.nika").expect("an unparsed file is still listed");
    assert_eq!(look.witness(), Some(Witness::of(bad.as_bytes()).0.as_str()));
    let source = text(&look.face_lines(Face::Source, 78, true, false)).join("\n");
    assert!(source.contains("bogus: 1"), "{source}");
    let check = text(&look.face_lines(Face::Check, 78, true, false)).join("\n");
    assert!(check.contains("NIKA-PARSE"), "{check}");
    let graph = text(&look.face_lines(Face::Graph, 78, true, false)).join("\n");
    assert!(graph.contains("no graph"), "{graph}");
    assert!(!graph.contains("Structure checked"), "{graph}");
}

/// Changed bytes need a new look: the old look keeps its witness and its
/// check, the new one judges the new bytes alone.
#[test]
fn changed_bytes_never_borrow_the_old_look() {
    let room = Room::new("changed");
    room.write("flow.nika", DIAMOND);
    let snapshot = observe(&room.0);
    let before = take(&snapshot, "flow.nika").expect("listed");
    room.write("flow.nika", SINGLE);
    let after = take(&snapshot, "flow.nika").expect("still listed");
    assert_eq!(
        before.witness(),
        Some(Witness::of(DIAMOND.as_bytes()).0.as_str())
    );
    assert_eq!(
        after.witness(),
        Some(Witness::of(SINGLE.as_bytes()).0.as_str())
    );
    assert_ne!(before, after);
    let old = text(&before.face_lines(Face::Plan, 78, true, false)).join("\n");
    let new = text(&after.face_lines(Face::Plan, 78, true, false)).join("\n");
    assert!(
        old.contains("4 tasks") && new.contains("1 task") && new.contains("only"),
        "{old}\n{new}"
    );
}

/// A listed path that became a symlink, grew past the bound or is not text
/// is not read: the look says why and judges nothing.
#[test]
fn a_symlink_an_oversized_or_a_binary_file_is_not_read() {
    let room = Room::new("unread");
    let outside = Room::new("outside");
    outside.write("secret.nika", DIAMOND);
    room.write("link.nika", DIAMOND);
    room.write("big.nika", DIAMOND);
    room.write("bin.nika", DIAMOND);
    let snapshot = observe(&room.0);
    std::fs::remove_file(room.0.join("link.nika")).expect("unlink");
    std::os::unix::fs::symlink(outside.0.join("secret.nika"), room.0.join("link.nika"))
        .expect("symlink");
    let cap = usize::try_from(LOOK_CAP).expect("cap");
    room.write("big.nika", &"#".repeat(cap + 1));
    std::fs::write(room.0.join("bin.nika"), [0xff_u8, 0xfe, 0x00]).expect("binary");
    for path in ["link.nika", "big.nika", "bin.nika"] {
        let look = take(&snapshot, path).expect("listed");
        assert!(look.witness().is_none(), "{path}");
        assert!(look.why_unread().is_some(), "{path}");
        let rows = text(&look.face_lines(Face::Check, 78, true, false)).join("\n");
        assert!(
            rows.contains("not read") && !rows.contains("VALID"),
            "{path}: {rows}"
        );
    }
}

/// A look changes nothing a reasoner receives.
#[test]
fn a_look_joins_no_reasoner_facts() {
    let room = Room::new("facts");
    room.write("diamond.nika", DIAMOND);
    let snapshot = observe(&room.0);
    let before = snapshot.facts_lines();
    let _look = take(&snapshot, "diamond.nika").expect("listed");
    assert_eq!(snapshot.facts_lines(), before);
    assert!(before.iter().all(|l| !l.contains("message: d")));
}

/// The project root the operator selected may be reached through a link: it
/// is resolved once, and a listed workflow below it is looked at; a link
/// inside the project stays unread.
#[test]
fn a_project_root_reached_through_a_link_is_looked_at_and_its_children_stay_unfollowed() {
    let room = Room::new("linked");
    let outer = Room::new("outer");
    room.write("diamond.nika", DIAMOND);
    room.write("alias.nika", SINGLE);
    let linked = outer.0.join("project");
    std::os::unix::fs::symlink(&room.0, &linked).expect("a linked root");
    let snapshot = observe(&linked);
    let look = take(&snapshot, "diamond.nika").expect("listed");
    assert!(look.why_unread().is_none(), "{:?}", look.why_unread());
    assert_eq!(
        look.witness(),
        Some(Witness::of(DIAMOND.as_bytes()).0.as_str())
    );
    std::fs::remove_file(room.0.join("alias.nika")).expect("unlink");
    std::os::unix::fs::symlink(room.0.join("diamond.nika"), room.0.join("alias.nika"))
        .expect("a child link");
    let child = take(&snapshot, "alias.nika").expect("listed");
    assert!(
        child.witness().is_none() && child.why_unread().is_some(),
        "a child link"
    );
}
