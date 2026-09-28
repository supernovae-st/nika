// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `--task` scoping: the ancestor-cone cut of a workflow, a pure graph walk
//! over [`crate::edges::producer_ids`]. Descended verbatim from
//! `nika-runtime`'s `admit.rs` (B11 · 2026-09-28) to sit beside the edges it
//! reads, and to give the runtime's 15k wall room for the cleanup lane's
//! money law. `nika-runtime` re-exports [`scope_to_task`] at its historical
//! path; the run verb and the Service keep the gate and the re-check around
//! it, and no admission state moved.

use nika_schema::raw::RawWorkflow;

/// `--task` scoping — the ancestor-cone cut behind the regenerate-one-
/// block move (its gate + re-check live in the run verb; this is the
/// pure graph walk · descended from the run verb 2026-07-22, then from the
/// runtime's launch-gate module to this plane 2026-09-28).
///
/// Ancestors must run — their outputs feed the target's bindings; nothing
/// downstream or sibling executes. Document order is preserved (stable
/// waves) and workflow `outputs:` drop (they may reference tasks outside
/// the scope — the target's own output IS the point of the run). Unknown
/// ids fail with the available set (environment class · exit 3 · before
/// any effect — the same lane as an unknown `--var` key).
///
/// # Errors
///
/// A human-readable refusal naming the declared task ids.
pub fn scope_to_task(mut wf: RawWorkflow, target: &str) -> Result<RawWorkflow, String> {
    use std::collections::{BTreeSet, VecDeque};

    let mut deps_of: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for t in &wf.tasks {
        deps_of.insert(
            t.value.id.value.as_str().to_owned(),
            crate::edges::producer_ids(&t.value),
        );
    }
    if !deps_of.contains_key(target) {
        let known = deps_of.keys().cloned().collect::<Vec<_>>().join(" · ");
        return Err(format!(
            "--task `{target}` names no task in this workflow — tasks: {known}"
        ));
    }

    let mut keep: BTreeSet<String> = BTreeSet::new();
    let mut queue: VecDeque<String> = VecDeque::from([target.to_owned()]);
    while let Some(id) = queue.pop_front() {
        if !keep.insert(id.clone()) {
            continue;
        }
        if let Some(deps) = deps_of.get(&id) {
            for d in deps {
                queue.push_back(d.clone());
            }
        }
    }

    wf.tasks
        .retain(|t| keep.contains(t.value.id.value.as_str()));
    wf.outputs.clear();
    Ok(wf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn ids(wf: &RawWorkflow) -> Vec<&str> {
        wf.tasks.iter().map(|t| t.value.id.value.as_str()).collect()
    }

    /// The cone of a chain keeps the target and every ancestor in document
    /// order, drops the downstream task, and clears `outputs:`.
    #[test]
    fn the_cone_keeps_ancestors_in_document_order() {
        let wf = parse(
            "nika: chain\ntasks:\n  a:\n    infer: { prompt: a }\n  b:\n    with: { x: \"${{ tasks.a.output }}\" }\n    infer: { prompt: \"${{ with.x }}\" }\n  c:\n    with: { y: \"${{ tasks.b.output }}\" }\n    infer: { prompt: \"${{ with.y }}\" }\noutputs:\n  last: ${{ tasks.c.output }}\n",
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("fixture parses");
        let cone = scope_to_task(wf, "b").expect("b scopes");
        assert_eq!(ids(&cone), ["a", "b"]);
        assert!(cone.outputs.is_empty(), "outputs drop under scope");
    }

    /// A dependency cycle (which the analyzer refuses elsewhere) still ends:
    /// each task enters the cone once.
    #[test]
    fn a_cycle_ends_with_each_task_once() {
        let wf = parse(
            "nika: cycle\ntasks:\n  a:\n    after: { b: success }\n    infer: { prompt: a }\n  b:\n    after: { a: success }\n    infer: { prompt: b }\n  c:\n    infer: { prompt: c }\n",
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("fixture parses");
        let cone = scope_to_task(wf, "a").expect("a scopes");
        assert_eq!(ids(&cone), ["a", "b"]);
    }

    /// An unknown id is refused with the declared set, in its exact words.
    #[test]
    fn an_unknown_id_names_the_declared_tasks() {
        let wf = parse(
            "nika: pair\ntasks:\n  a:\n    infer: { prompt: a }\n  b:\n    infer: { prompt: b }\n",
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("fixture parses");
        assert_eq!(
            scope_to_task(wf, "nope").err().as_deref(),
            Some("--task `nope` names no task in this workflow — tasks: a · b")
        );
    }
}
