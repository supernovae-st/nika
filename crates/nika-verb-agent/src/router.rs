// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Per-turn tool routing — deterministic active discovery (ADR-093).
//!
//! Injecting EVERY tool definition every turn does not scale: large
//! universes crowd the context and degrade selection quality (the
//! observation behind MCP-Zero's active discovery · Fei · Zheng · Feng
//! 2025, arxiv.org/abs/2506.01056, and the retrieval line from Gorilla ·
//! Patil et al. 2023, arxiv.org/abs/2305.15334). Nika's answer is
//! SOVEREIGN: rank definitions against the live task context with the
//! engine's own BM25 satellite (`nika-bm25` — BM25S eager scoring,
//! L2→L1 downward dep), zero LLM calls, bit-reproducible.
//!
//! Selection law (deterministic, in order):
//! 1. PASSTHROUGH below `min_universe` definitions — small tool sets
//!    ship whole; stable prompts stay provider-cache-friendly.
//! 2. PINNED always ride: loop intrinsics (`agent:*`) + the `nika:done`
//!    sentinel — terminals must never be routed away.
//! 3. RECENT tools (used within `recency_turns`) stay offered — the
//!    model just saw their results; yanking them mid-thought breaks the
//!    transcript's referential integrity.
//! 4. TOP-K by BM25 over (name + description) against (prompt + last
//!    assistant text + last observations).
//! 5. FAIL-OPEN: if ranking matches nothing (zero lexical overlap), the
//!    full universe ships — a blind model is worse than a crowded one.

use std::collections::BTreeMap;

use nika_bm25::{BmIndex, BmParams, EagerIndex};
use nika_kernel::ai::provider::ToolDef;

use crate::DONE_TOOL;
use crate::config::RouterConfig;
use crate::observe::{SourceCounts, ToolSource};

/// Belt-and-braces cap on the query CHARS fed to the ranker (the caller
/// already budgets per-component in `routing_query`; this bounds any
/// future direct caller — the query is built from model output + tool
/// results, both untrusted sizes).
const MAX_QUERY_CHARS: usize = 4096;

/// One routing decision, for telemetry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Selection {
    /// Definitions offered this turn.
    pub(crate) offered: u32,
    /// Whitelisted universe size.
    pub(crate) universe: u32,
    /// Per-source breakdown of the offered set.
    pub(crate) by_source: SourceCounts,
}

/// The per-run router. Owns the frozen ranking index over the
/// whitelisted universe + the per-tool recency ledger.
#[derive(Debug)]
pub(crate) struct ToolRouter {
    cfg: RouterConfig,
    /// `Some` only when routing is active (universe ≥ `min_universe`).
    eager: Option<EagerIndex>,
    /// Tool name → last turn it was dispatched (`BTreeMap`: workspace
    /// determinism discipline — iteration order is part of the contract).
    last_used: BTreeMap<String, u32>,
}

impl ToolRouter {
    /// Build over the whitelisted universe (defs order is frozen for the
    /// run — doc ids index into it).
    pub(crate) fn new(defs: &[ToolDef], cfg: RouterConfig) -> Self {
        let eager = if defs.len() >= cfg.min_universe {
            let mut index = BmIndex::new(BmParams::default());
            for (id, def) in defs.iter().enumerate() {
                // Name tokens weighted ×3 over the description: a query
                // mentioning a tool by name should out-rank description
                // overlap (deterministic boost · no per-field weights in
                // the satellite's scorer by design).
                let text = format!(
                    "{name} {name} {name} {desc}",
                    name = def.name.replace([':', '/', '_', '-'], " "),
                    desc = def.description
                );
                #[allow(clippy::cast_possible_truncation)] // bounded by min_universe ≪ u32::MAX
                index.add_document(id as u32, &text);
            }
            index.finalize();
            EagerIndex::build(&index)
        } else {
            None
        };
        Self {
            cfg,
            eager,
            last_used: BTreeMap::new(),
        }
    }

    /// Whether per-turn routing is active for this run.
    pub(crate) fn is_routing(&self) -> bool {
        self.eager.is_some()
    }

    /// Record a dispatch (feeds the recency pin).
    pub(crate) fn note_used(&mut self, name: &str, turn: u32) {
        self.last_used.insert(name.to_owned(), turn);
    }

    /// Decide this turn's definition set. Returns the defs to offer (in
    /// universe order — stable across turns for identical decisions) +
    /// the telemetry payload.
    pub(crate) fn select(
        &self,
        defs: &[ToolDef],
        query: &str,
        turn: u32,
    ) -> (Vec<ToolDef>, Selection) {
        #[allow(clippy::cast_possible_truncation)] // universe ≪ u32::MAX
        let universe = defs.len() as u32;
        let Some(eager) = &self.eager else {
            return (defs.to_vec(), selection_of(defs, universe));
        };

        let mut keep = vec![false; defs.len()];
        for (i, def) in defs.iter().enumerate() {
            let pinned = matches!(ToolSource::classify(&def.name), ToolSource::Intrinsic)
                || def.name == DONE_TOOL;
            let recent = self
                .last_used
                .get(&def.name)
                .is_some_and(|&t| turn.saturating_sub(t) <= self.cfg.recency_turns);
            if pinned || recent {
                keep[i] = true;
            }
        }

        let bounded: String = query.chars().take(MAX_QUERY_CHARS).collect();
        let ranked = eager.top_k(&bounded, self.cfg.top_k);
        let mut any_ranked = false;
        for (doc, score) in ranked {
            // `> 0.0` is belt-and-braces: the Lucene-form idf is strictly
            // positive (`ln(x + 1)` · scorer.rs), so a RETURNED doc can't
            // score 0.0 — the `>=` mutant here is equivalent by that
            // invariant (documented, not chased).
            if score > 0.0
                && let Some(slot) = keep.get_mut(doc as usize)
            {
                *slot = true;
                any_ranked = true;
            }
        }

        // Fail-open: zero lexical overlap means the ranker has no signal
        // this turn — ship the whole universe rather than blind the model.
        if !any_ranked {
            return (defs.to_vec(), selection_of(defs, universe));
        }

        let offered: Vec<ToolDef> = defs
            .iter()
            .zip(keep.iter())
            .filter(|&(_, &k)| k)
            .map(|(def, _)| def.clone())
            .collect();
        let selection = selection_of(&offered, universe);
        (offered, selection)
    }
}

fn selection_of(offered: &[ToolDef], universe: u32) -> Selection {
    #[allow(clippy::cast_possible_truncation)] // offered ≤ universe ≪ u32::MAX
    Selection {
        offered: offered.len() as u32,
        universe,
        by_source: SourceCounts::tally(offered.iter().map(|d| d.name.as_str())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str, desc: &str) -> ToolDef {
        ToolDef::new(name, desc, serde_json::json!({}))
    }

    fn big_universe() -> Vec<ToolDef> {
        // 26 defs ≥ default min_universe (24) → routing active.
        let mut defs: Vec<ToolDef> = (0..24)
            .map(|i| {
                def(
                    &format!("mcp:srv/tool{i}"),
                    &format!("unrelated capability number {i} for widgets"),
                )
            })
            .collect();
        defs.push(def(
            "nika:fetch",
            "fetch a url over http and extract content",
        ));
        defs.push(def("nika:done", "finish the task"));
        defs
    }

    #[test]
    fn small_universe_is_passthrough() {
        let defs = vec![def("nika:read", "read a file"), def("nika:done", "finish")];
        let router = ToolRouter::new(&defs, RouterConfig::new());
        assert!(!router.is_routing());
        let (offered, sel) = router.select(&defs, "anything", 1);
        assert_eq!(offered.len(), 2);
        assert_eq!(sel.offered, 2);
        assert_eq!(sel.universe, 2);
    }

    #[test]
    fn routing_narrows_to_relevant_plus_pinned() {
        let defs = big_universe();
        let router = ToolRouter::new(&defs, RouterConfig::new());
        assert!(router.is_routing());
        let (offered, sel) = router.select(&defs, "fetch the url and extract the page", 1);
        assert!(sel.offered < sel.universe, "routing must narrow: {sel:?}");
        let names: Vec<&str> = offered.iter().map(|d| d.name.as_str()).collect();
        assert!(
            names.contains(&"nika:fetch"),
            "the relevant tool rides: {names:?}"
        );
        assert!(
            names.contains(&"nika:done"),
            "the sentinel is pinned: {names:?}"
        );
    }

    #[test]
    fn recently_used_tools_stay_offered() {
        let defs = big_universe();
        let mut router = ToolRouter::new(&defs, RouterConfig::new());
        router.note_used("mcp:srv/tool7", 1);
        // Turn 2 query has zero overlap with tool7's text — recency keeps it.
        let (offered, _) = router.select(&defs, "fetch the url please", 2);
        assert!(
            offered.iter().any(|d| d.name == "mcp:srv/tool7"),
            "a tool used last turn must stay visible"
        );
        // Far past the recency window it drops back to ranked-only.
        let (later, _) = router.select(&defs, "fetch the url please", 9);
        assert!(!later.iter().any(|d| d.name == "mcp:srv/tool7"));
    }

    #[test]
    fn the_sentinel_is_pinned_even_with_zero_lexical_reach() {
        // The query overlaps the FILLER tools only ("widgets capability
        // number") and shares NO term with the sentinel's "finish the
        // task" — `nika:done` must ride on the PIN alone (kills the
        // `||` → `&&` mutant on the pinned clause, under which the
        // sentinel would need to be Intrinsic-classified AND
        // name-matched — impossible).
        let defs = big_universe();
        let router = ToolRouter::new(&defs, RouterConfig::new());
        let (offered, _) = router.select(&defs, "unrelated widgets capability number", 1);
        assert!(
            offered.iter().any(|d| d.name == "nika:done"),
            "the sentinel rides the pin, not the ranking"
        );
        assert!(
            offered.len() < defs.len(),
            "ranking DID narrow (the pin is not fail-open in disguise)"
        );
    }

    #[test]
    fn zero_overlap_query_fails_open_to_the_full_universe() {
        let defs = big_universe();
        let router = ToolRouter::new(&defs, RouterConfig::new());
        let (offered, sel) = router.select(&defs, "zzz qqq xxyyzz", 1);
        assert_eq!(offered.len(), defs.len(), "fail-open: {sel:?}");
    }

    #[test]
    fn selection_is_deterministic_across_calls() {
        let defs = big_universe();
        let router = ToolRouter::new(&defs, RouterConfig::new());
        let (a, sa) = router.select(&defs, "extract content from the url", 3);
        let (b, sb) = router.select(&defs, "extract content from the url", 3);
        let an: Vec<&str> = a.iter().map(|d| d.name.as_str()).collect();
        let bn: Vec<&str> = b.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(an, bn);
        assert_eq!(sa, sb);
    }
}
