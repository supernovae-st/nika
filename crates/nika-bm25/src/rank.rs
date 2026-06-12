// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Top-k selection — the ONE ranking order, the ONE selection algorithm.
//!
//! Every retrieval path ranks by the same total order — score
//! descending, then `doc_id` ascending (`f64::total_cmp`, so NaN has a
//! definite position and the order really is total). Before this
//! module that order lived in three call sites; an order tweak in one
//! would have silently forked the others (the parallel-taxonomy smell).
//!
//! Selection is the textbook bounded min-heap (Manning · Raghavan ·
//! Schütze 2008 *IIR* ch. 7 §7.1 — selecting the k best with a heap):
//! keep the k best seen so far in a heap whose top is the WORST kept;
//! a candidate either beats that worst (pop + push · `O(log k)`) or is
//! discarded `O(1)`. Total `O(N log k + k log k)` versus the previous
//! full sort `O(N log N)` — at corpus scale with small k (the recall
//! pipeline asks 10-100), the win is structural, and the output is
//! IDENTICAL: the k-best set under a total order is unique, pinned by
//! the property test below and by the golden/parity suites end-to-end.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// A scored document under the canonical ranking order.
///
/// `Ord` is INVERTED on purpose: `greater` means « worse » (lower
/// score, then higher `doc_id`), so a max-`BinaryHeap` of `Ranked`
/// keeps its WORST element on top — exactly what bounded selection
/// evicts first.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Ranked(u32, f64);

impl Eq for Ranked {}

impl Ord for Ranked {
    fn cmp(&self, other: &Self) -> Ordering {
        // worse = lower score · then HIGHER doc id (self vs other in
        // doc order — the previous inverted form made the heap keep
        // HIGH doc ids on ties; the tie unit test caught it)
        self.1
            .total_cmp(&other.1)
            .reverse()
            .then(self.0.cmp(&other.0))
    }
}

impl PartialOrd for Ranked {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Rank `(doc_id, score)` pairs by the canonical order (score
/// descending · `doc_id` ascending) and keep the best `k`.
///
/// Single pass over the iterator with a bounded heap; the returned
/// vector is fully sorted. Identical output to sort-then-truncate
/// (the selection under a total order is unique).
pub(crate) fn select_top_k(scored: impl Iterator<Item = (u32, f64)>, k: usize) -> Vec<(u32, f64)> {
    if k == 0 {
        return Vec::new();
    }
    let mut heap: BinaryHeap<Ranked> = BinaryHeap::with_capacity(k.saturating_add(1));
    for (doc, score) in scored {
        let cand = Ranked(doc, score);
        if heap.len() < k {
            heap.push(cand);
        } else if let Some(worst) = heap.peek()
            && cand < *worst
        {
            // strictly better than the worst kept (under the inverted
            // order, « less » = « better ») — evict and admit
            heap.pop();
            heap.push(cand);
        }
    }
    let mut out: Vec<(u32, f64)> = heap.into_iter().map(|Ranked(d, s)| (d, s)).collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// The reference: full sort, then truncate.
    fn by_full_sort(mut scored: Vec<(u32, f64)>, k: usize) -> Vec<(u32, f64)> {
        scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        scored.truncate(k);
        scored
    }

    #[test]
    fn ties_resolve_by_ascending_doc_id() {
        let scored = vec![(7, 1.0), (3, 1.0), (5, 2.0), (9, 1.0)];
        assert_eq!(
            select_top_k(scored.into_iter(), 3),
            vec![(5, 2.0), (3, 1.0), (7, 1.0)],
        );
    }

    #[test]
    fn k_zero_and_k_beyond_len_are_total() {
        assert_eq!(select_top_k(vec![(1, 1.0)].into_iter(), 0), vec![]);
        assert_eq!(
            select_top_k(vec![(2, 1.0), (1, 3.0)].into_iter(), 10),
            vec![(1, 3.0), (2, 1.0)],
        );
    }

    proptest! {
        /// IIR ch.7 heap selection ≡ full sort + truncate, over random
        /// score sets INCLUDING heavy ties (small score alphabet forces
        /// the tiebreak path constantly).
        #[test]
        fn heap_selection_equals_full_sort(
            scored in proptest::collection::vec(
                (0u32..50, prop_oneof![Just(0.5f64), Just(1.0), Just(2.0), 0.0f64..10.0]),
                0..120,
            ),
            k in 0usize..15,
        ) {
            let heap = select_top_k(scored.clone().into_iter(), k);
            let sorted = by_full_sort(scored, k);
            prop_assert_eq!(heap, sorted);
        }
    }
}
