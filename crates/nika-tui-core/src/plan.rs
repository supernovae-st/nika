// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The plan board — the seating law, ported line for line from the
//! studio's `plan.ts`.
//!
//! ⭐⭐ THE SLOT LAW, and what it deliberately costs:
//!
//!   a slot assigned at birth NEVER moves · a freed slot stays EMPTY
//!   forever.
//!
//! The second half is the one one is tempted to betray: when a step
//! disappears, everything asks to compact. Compacting moves, under the
//! reader's eye, things they had already located. **The white hole is the
//! price, and it buys stability everywhere else.**
//!
//! ⭐ The unit of appearance is the WHOLE PLAN, never the node: the graph
//! changes only at the instants where the ENGINE spoke, and each instant
//! carries its revision number (r1 · r2 · r3). A graph that grows while
//! the model thinks would show the model acting — the image this product
//! may never show.
//!
//! ⚠️ The graph is NOT ours — it is the engine's canonical projection
//! (`nika inspect --format json` · `graph_format: 2`, carried here by
//! [`crate::ingress::GraphDoc`]).

use std::collections::{BTreeMap, BTreeSet};

use crate::ingress::{GraphDoc, Node};

/// What a revision did to a slot — serialized as the studio's glyph, so
/// the caller never translates the vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum Mark {
    /// Born this revision.
    #[serde(rename = "+")]
    Born,
    /// Changed this revision (the engine describes it otherwise).
    #[serde(rename = "~")]
    Changed,
    /// Died this revision — the slot stays empty forever.
    #[serde(rename = "−")]
    Dead,
    /// Untouched.
    #[serde(rename = " ")]
    Kept,
}

/// The board: slots in birth order, one mark and one print per revision.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct Board {
    /// The revision number — r1 · r2 · …
    pub rev: u32,
    /// The slots, in birth order — `None` = freed, and stays freed. An
    /// entry is never removed; it is emptied.
    pub slots: Vec<Option<String>>,
    /// Each slot's mark AT THIS REVISION — same length as `slots`.
    pub marks: Vec<Mark>,
    /// Each live task's fingerprint — decides the `Changed` mark next
    /// revision.
    pub prints: BTreeMap<String, String>,
}

impl Board {
    /// The constructor the `#[non_exhaustive]` marker requires (invariant
    /// #19): a field added later must not break a caller's build. Four of
    /// the eight markers shipped without one — two Gate-11 lenses found the
    /// same gap independently, which is the whole point of running three.
    #[must_use]
    pub const fn new(
        rev: u32,
        slots: Vec<Option<String>>,
        marks: Vec<Mark>,
        prints: BTreeMap<String, String>,
    ) -> Self {
        Self {
            rev,
            slots,
            marks,
            prints,
        }
    }
}

/// ⭐ The fingerprint ignores the id and keeps EVERYTHING ELSE — a field
/// the engine adds tomorrow counts as a change without anyone having to
/// foresee it. A hand-written watchlist would expire at the engine's
/// first enrichment.
///
/// The print is an INTERNAL fingerprint: JSON with the id removed and
/// keys canonicalized (`BTree` order). The studio's print keeps emission
/// order; both are only ever compared within their own implementation, so
/// the orders never meet — what matters is that ANY change flips the
/// print.
fn print_of(n: &Node) -> String {
    let mut v = serde_json::to_value(n).unwrap_or(serde_json::Value::Null);
    if let serde_json::Value::Object(ref mut map) = v {
        map.remove("id");
    }
    v.to_string()
}

/// The very first plan — every slot is born.
#[must_use]
pub fn seat_first(g: &GraphDoc) -> Board {
    Board {
        rev: 1,
        slots: g.nodes.iter().map(|n| Some(n.id.clone())).collect(),
        marks: g.nodes.iter().map(|_| Mark::Born).collect(),
        prints: g
            .nodes
            .iter()
            .map(|n| (n.id.clone(), print_of(n)))
            .collect(),
    }
}

/// The next revision — births go to the END, deaths empty their slot, and
/// nobody moves.
#[must_use]
pub fn seat_next(prev: &Board, g: &GraphDoc) -> Board {
    let vus: BTreeMap<&str, &Node> = g.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    // ⭐ ONE fingerprint pass. `print_of` used to run once per survivor for
    // the comparison, then again for EVERY node to fill the board — each
    // survivor serialized twice per revision, for the same bytes. Computed
    // here, read twice.
    let prints: BTreeMap<String, String> = g
        .nodes
        .iter()
        .map(|n| (n.id.clone(), print_of(n)))
        .collect();
    let mut slots = prev.slots.clone();
    let mut marks: Vec<Mark> = prev.slots.iter().map(|_| Mark::Kept).collect();

    // ① the dead — their slot empties and is never reattributed
    for (i, id) in slots.iter_mut().enumerate() {
        if let Some(name) = id.as_deref()
            && !vus.contains_key(name)
        {
            *id = None;
            marks[i] = Mark::Dead;
        }
    }

    // ② the survivors — `Changed` when the engine describes them otherwise
    for (i, id) in slots.iter().enumerate() {
        let Some(name) = id.as_deref() else { continue };
        if vus.contains_key(name)
            && let Some(now) = prints.get(name)
            && prev.prints.get(name).is_none_or(|before| before != now)
        {
            marks[i] = Mark::Changed;
        }
    }

    // ③ the births — at the END, always. Sliding one into a freed hole
    // would resurrect a name where the eye had recorded an absence — that
    // would be compaction under another name.
    let present: BTreeSet<String> = slots.iter().flatten().cloned().collect();
    for n in &g.nodes {
        if present.contains(n.id.as_str()) {
            continue;
        }
        slots.push(Some(n.id.clone()));
        marks.push(Mark::Born);
    }

    Board {
        rev: prev.rev.saturating_add(1), // a hostile board's u32::MAX stays MAX — wrapping to 0 would renumber history
        slots,
        marks,
        prints,
    }
}
