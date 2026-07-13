// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shared graph reads over the explicit `depends_on` edges — the same
//! edge set the engine's own `analyze` schedules with. Presentation
//! math only: findings about the graph stay in the check ladder.

use nika_schema::raw::RawWorkflow;

/// The ids downstream of `id` — { t : id →⁺ t }, BFS over the explicit
/// reverse edges. `id` itself is NOT in the set.
pub(super) fn downstream_ids<'a>(wf: &'a RawWorkflow, id: &str) -> Vec<&'a str> {
    let mut seen: Vec<&'a str> = Vec::new();
    let mut queue: Vec<&str> = vec![id];
    while let Some(cur) = queue.pop() {
        for t in &wf.tasks {
            if t.value.depends_on.iter().any(|d| d.value == cur) {
                let child = t.value.id.value.as_str();
                if !seen.contains(&child) {
                    seen.push(child);
                    queue.push(child);
                }
            }
        }
    }
    seen
}

/// The ids task `id` may legally REFERENCE — everything except itself
/// and its downstream closure. Referencing downstream creates a cycle
/// (a template ref is an implicit edge) or, in `recover:`, a deadlock
/// (DAG-004): either way the check refuses it, so completion never
/// offers it.
pub(super) fn illegal_reference_targets<'a>(wf: &'a RawWorkflow, id: &'a str) -> Vec<&'a str> {
    let mut set = downstream_ids(wf, id);
    set.push(id);
    set
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::{FileId, ParseMode, parse};

    // the diamond: a → {b, c} → d
    const DIAMOND: &str = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    exec: { command: \"x\" }\n  - id: b\n    depends_on: [a]\n    exec: { command: \"x\" }\n  - id: c\n    depends_on: [a]\n    exec: { command: \"x\" }\n  - id: d\n    depends_on: [b, c]\n    exec: { command: \"x\" }\n";

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
        let mut illegal = illegal_reference_targets(&wf, "b");
        illegal.sort_unstable();
        // b may not reference itself nor d (which waits on b) — a and c
        // stay legal (ancestor · parallel-independent).
        assert_eq!(illegal, vec!["b", "d"]);
    }
}
