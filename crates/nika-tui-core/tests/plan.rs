// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The seating law's proofs — synthetic graphs, one law per test.

use nika_tui_core::ingress::{GraphDoc, Node};
use nika_tui_core::model::Verb;
use nika_tui_core::plan::{Mark, seat_first, seat_next};

fn node(id: &str, tool: Option<&str>) -> Node {
    let mut n = Node::new(id.to_owned(), Verb::Invoke);
    n.tool = tool.map(str::to_owned);
    n
}

fn graph(ids: &[&str]) -> GraphDoc {
    GraphDoc::new(
        2,
        "test".to_owned(),
        ids.iter().map(|id| node(id, None)).collect(),
        Vec::new(),
    )
}

/// The first plan: every slot is born, the revision is r1.
#[test]
fn the_first_plan_births_every_slot() {
    let b = seat_first(&graph(&["a", "b", "c"]));
    assert_eq!(b.rev, 1);
    assert_eq!(
        b.slots,
        vec![
            Some("a".to_owned()),
            Some("b".to_owned()),
            Some("c".to_owned())
        ]
    );
    assert_eq!(b.marks, vec![Mark::Born, Mark::Born, Mark::Born]);
}

/// THE SLOT LAW — a dead step's slot empties forever, a birth goes to the
/// END, nobody moves.
#[test]
fn a_freed_slot_stays_empty_and_births_go_to_the_end() {
    let b1 = seat_first(&graph(&["a", "b", "c"]));
    let b2 = seat_next(&b1, &graph(&["a", "c", "d"]));
    assert_eq!(b2.rev, 2);
    assert_eq!(
        b2.slots,
        vec![
            Some("a".to_owned()),
            None, // b's hole stays a hole
            Some("c".to_owned()),
            Some("d".to_owned()), // the birth at the END
        ]
    );
    assert_eq!(
        b2.marks,
        vec![Mark::Kept, Mark::Dead, Mark::Kept, Mark::Born]
    );
    // r3 — b does not resurrect its slot
    let b3 = seat_next(&b2, &graph(&["a", "c", "d", "b"]));
    assert_eq!(b3.slots[1], None, "the hole is permanent");
    assert_eq!(
        b3.slots[4],
        Some("b".to_owned()),
        "the return is a NEW birth at the end"
    );
    assert_eq!(b3.marks[4], Mark::Born);
}

/// A change the engine makes flips the mark to `Changed` — the print
/// ignores the id and keeps everything else.
#[test]
fn a_changed_description_marks_the_slot() {
    let b1 = seat_first(&graph(&["a", "b"]));
    let mut g2 = graph(&["a", "b"]);
    g2.nodes[1] = node("b", Some("nika:jq")); // same id, another tool
    let b2 = seat_next(&b1, &g2);
    assert_eq!(b2.marks, vec![Mark::Kept, Mark::Changed]);
}

/// A no-change revision keeps every mark calm.
#[test]
fn a_calm_revision_marks_nothing() {
    let b1 = seat_first(&graph(&["a", "b"]));
    let b2 = seat_next(&b1, &graph(&["a", "b"]));
    assert_eq!(b2.marks, vec![Mark::Kept, Mark::Kept]);
    assert_eq!(b2.rev, 2);
}

/// ⭐ THE PRINT'S OWN PROMISE — "a field the engine adds tomorrow counts as
/// a change without anyone having to foresee it". The typed `Node` broke it
/// silently: serde drops what the struct does not declare, so the print was
/// computed over the KNOWN keys only. A field arriving on the wire was
/// already gone before the fingerprint saw it.
///
/// The case that makes it a P0 rather than a nicety: a graph whose only
/// diff is `sandbox: off → ON-DANGEROUS` read as `Kept`. The board would
/// tell an operator that nothing changed on the revision where the sandbox
/// came down.
///
/// The surfaces disagreed here too — the browser's `JSON.parse` keeps every
/// key, so it would have flagged the change this crate called calm.
#[test]
fn a_field_the_engine_adds_tomorrow_still_flips_the_print() {
    let with_sandbox = |v: &str| {
        let doc = serde_json::json!({
            "graph_format": 2,
            "workflow": "test",
            "nodes": [{
                "id": "a", "verb": "exec", "permits": [], "outputs": [],
                "sandbox": v,                       // ⟵ undeclared on `Node`
            }],
            "edges": [],
        });
        serde_json::from_value::<GraphDoc>(doc).expect("graph")
    };

    let b1 = seat_first(&with_sandbox("off"));
    let b2 = seat_next(&b1, &with_sandbox("ON-DANGEROUS"));
    assert_eq!(
        b2.marks,
        vec![Mark::Changed],
        "the sandbox came down and the board said nothing changed"
    );

    // and the converse, so the fix cannot be « mark everything Changed »
    let b3 = seat_next(&b2, &with_sandbox("ON-DANGEROUS"));
    assert_eq!(
        b3.marks,
        vec![Mark::Kept],
        "an unchanged unknown field must stay calm"
    );
}
