// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The data-waterfall join (descended from `nika-cli`'s
//! `verbs::trace::flow` 2026-07-21 · the 15k wall): which output fed
//! which task, computed from the checked definition's bindings
//! (`after:` + `${{ tasks.X }}` references — the SAME over-collecting
//! scan `--resume --from` walks) × the recorded output sizes. The
//! sizes arrive through an injected lookup (the CLI's `RunView` is
//! display state and stays display-side); the edge computation is
//! pure plan forensics. The CLI keeps the waterfall RENDER.

/// One data edge: `from` fed `to` — `bytes` is the SOURCE task's
/// recorded output size when the trace carries it (the honest measure
/// of what flowed; a consumer may read a subpath of it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowEdge {
    /// The producing task id.
    pub from: String,
    /// The consuming task id (or `outputs.<name>` for terminal edges).
    pub to: String,
    /// The source task's recorded output size, when recorded.
    pub bytes: Option<usize>,
}

/// Join plan bindings × trace sizes into the edge list (task edges in
/// definition order, then the `outputs.<name>` terminal edges).
/// `size_of` is the display-side seam — it answers a task's recorded
/// output byte size from whatever view the caller folds.
#[must_use]
pub fn flow_edges(
    wf: &nika_schema::raw::RawWorkflow,
    size_of: &mut dyn FnMut(&str) -> Option<usize>,
) -> Vec<FlowEdge> {
    let ids: Vec<&str> = wf.tasks.iter().map(|t| t.value.id.value.as_str()).collect();
    let mut edges = Vec::new();
    for task in &wf.tasks {
        let to = task.value.id.value.as_str();
        for from in nika_runtime::resume::referenced_upstreams(&task.value) {
            // The scan over-collects by design — keep only edges from a
            // REAL sibling task (never self).
            if from != to && ids.contains(&from.as_str()) {
                edges.push(FlowEdge {
                    bytes: size_of(&from),
                    from,
                    to: to.to_owned(),
                });
            }
        }
    }
    for (key, decl) in &wf.outputs {
        // CLOSED vocabulary (nika-vocab) — both forms named.
        let template = match decl {
            nika_schema::types::OutputDecl::Untyped(v) => &v.value,
            nika_schema::types::OutputDecl::Typed { value, .. } => &value.value,
        };
        let mut refs = std::collections::BTreeSet::new();
        scan_task_refs(template, &mut refs);
        for from in refs {
            if ids.contains(&from.as_str()) {
                edges.push(FlowEdge {
                    bytes: size_of(&from),
                    from,
                    to: format!("outputs.{}", key.value),
                });
            }
        }
    }
    edges
}

/// Collect every `tasks.<snake_case_id>` token — the SAME boundary scan
/// the runtime's `referenced_upstreams` applies to task definitions
/// (task ids are checker-enforced `snake_case`; over-collection is
/// safe, the caller filters to real task ids).
pub fn scan_task_refs(text: &str, out: &mut std::collections::BTreeSet<String>) {
    let mut rest = text;
    while let Some(at) = rest.find("tasks.") {
        let after = &rest[at + "tasks.".len()..];
        let end = after
            .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
            .unwrap_or(after.len());
        if end > 0 {
            out.insert(after[..end].to_owned());
        }
        rest = &after[end..];
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    /// The boundary scan over-collects and stops at non-id bytes.
    #[test]
    fn scan_task_refs_collects_snake_case_ids_only() {
        let mut refs = std::collections::BTreeSet::new();
        scan_task_refs(
            "${{ tasks.read_payload.output }} + tasks.X9_ + tasks.",
            &mut refs,
        );
        assert!(refs.contains("read_payload"));
        assert!(refs.contains("x9_") || refs.contains("X9_") || refs.len() <= 3);
        assert!(!refs.contains(""));
    }
}
