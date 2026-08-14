// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shared graph reads over the explicit `depends_on` edges — the same
//! edge set the engine's own `analyze` schedules with. Presentation
//! math only: findings about the graph stay in the check ladder.

use nika_schema::raw::RawWorkflow;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// The ids downstream of `id` — { t : id →⁺ t }, ONE reverse-adjacency
/// build + ONE BFS = O(V+E). `id` itself is NOT in the set.
///
/// The previous shape re-scanned every task per queue pop with a
/// `Vec::contains` visited check — O(V²·d) — and was the ×20 hover
/// slope the W0 baseline caught on diamond-10k (engine#579; the same
/// lesson the vscode client learned in its #101, now applied
/// server-side).
pub(super) fn downstream_ids<'a>(wf: &'a RawWorkflow, id: &str) -> Vec<&'a str> {
    let mut children: BTreeMap<String, Vec<&'a str>> = BTreeMap::new();
    for t in &wf.tasks {
        let child = t.value.id.value.as_str();
        for producer in nika_check::analyzer::edges::producer_ids(&t.value) {
            children.entry(producer).or_default().push(child);
        }
    }
    let mut seen: BTreeSet<&'a str> = BTreeSet::new();
    let mut order: Vec<&'a str> = Vec::new();
    let mut queue: VecDeque<&str> = VecDeque::from([id]);
    while let Some(cur) = queue.pop_front() {
        if let Some(kids) = children.get(cur) {
            for &kid in kids {
                if seen.insert(kid) {
                    order.push(kid);
                    queue.push_back(kid);
                }
            }
        }
    }
    order
}

/// The ids task `id` may legally REFERENCE — everything except itself
/// and its downstream closure. Referencing downstream creates a cycle
/// (a template ref is an implicit edge) or, in `recover:`, a deadlock
/// (DAG-004): either way the check refuses it, so completion never
/// offers it. A set, because every caller filters candidates through
/// membership tests (`contains` must be O(log V), not O(V) — `BTree` by
/// workspace law: std hash types are disallowed for determinism).
pub(super) fn illegal_reference_targets<'a>(wf: &'a RawWorkflow, id: &'a str) -> BTreeSet<&'a str> {
    let mut set: BTreeSet<&'a str> = downstream_ids(wf, id).into_iter().collect();
    set.insert(id);
    set
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::{FileId, ParseMode, parse};

    // the diamond: a → {b, c} → d (control edges via `after:` — the
    // W2 boundary; `producer_ids` reads the same derived edge set for
    // `with:` refs too)
    const DIAMOND: &str = "nika: w\ntasks:\n  a:\n    exec: { command: [\"x\"] }\n  b:\n    after: { a: success }\n    exec: { command: [\"x\"] }\n  c:\n    after: { a: success }\n    exec: { command: [\"x\"] }\n  d:\n    after: { b: success, c: success }\n    exec: { command: [\"x\"] }\n";

    #[test]
    fn downstream_of_the_root_is_everything_else() {
        let wf = parse(DIAMOND, FileId::new(0), ParseMode::Lenient).expect("parses");
        let mut down = downstream_ids(&wf, "a");
        down.sort_unstable();
        assert_eq!(down, vec!["b", "c", "d"]);
        assert!(downstream_ids(&wf, "d").is_empty(), "terminal task");
    }

    #[test]
    fn illegal_targets_are_self_plus_closure() {
        let wf = parse(DIAMOND, FileId::new(0), ParseMode::Lenient).expect("parses");
        let mut illegal: Vec<&str> = illegal_reference_targets(&wf, "b").into_iter().collect();
        illegal.sort_unstable();
        // b may not reference itself nor d (which waits on b) — a and c
        // stay legal (ancestor · parallel-independent).
        assert_eq!(illegal, vec!["b", "d"]);
    }
}
