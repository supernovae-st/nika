// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The content-taint pass (NEP-0002 v2.0 · the INTEGRITY half of the flow
//! lattice — [`super::flow`] tracks secret CONFIDENTIALITY): does untrusted
//! content actually REACH a task's effect surface, and from which ingress?
//! Sources: invoked `nika:fetch` · `mcp:*` (fail-closed) · `agent:` with an
//! ingress whitelist. Propagation mirrors [`super::flow`] with three
//! inversions: `infer:`/`agent:` outputs ARE tainted when their prompt sees
//! it · an `exec:` is tainted when a tainted WRITER ran earlier under a
//! declared `fs.read` (the file channel argv cannot see) · recover reads
//! propagate. One sweep = the least fixpoint (the DAG is acyclic).

use std::collections::BTreeMap;

use nika_cap::TaintWitness;

use super::flow::{action_effect_fields, collect_json_strings, refs_in_str};
use nika_schema::expression::NamespaceRef;
use nika_schema::raw::{RawAction, RawWorkflow};
use nika_schema::types::OnErrorAction;

/// Compute the per-task content-taint witness. `mcp_trusted(server_id)`
/// answers the catalog's `content_trust` mark (`true` = trusted content).
pub(super) fn analyze_content_flow(
    wf: &RawWorkflow,
    waves: &[Vec<usize>],
    mcp_trusted: &dyn Fn(&str) -> bool,
) -> Vec<TaintWitness> {
    let n = wf.tasks.len();
    let id_of: BTreeMap<&str, usize> = wf
        .tasks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.value.id.value.as_str(), i))
        .collect();
    let fs_read_declared = wf
        .permits
        .as_ref()
        .and_then(|p| p.value.fs.as_ref())
        .is_some_and(|fs| !fs.read.is_empty());

    // output[i] / effect[i] = the untrusted origin reaching each (None = clean).
    let mut output: Vec<Option<String>> = vec![None; n];
    let mut effect: Vec<Option<String>> = vec![None; n];
    let mut any_writer_so_far: Option<String> = None;

    for wave in waves {
        for &i in wave {
            let task = &wf.tasks[i].value;

            let mut with_src: BTreeMap<&str, String> = BTreeMap::new();
            for (key, value) in &task.with {
                for r in collect_json_strings(&value.value)
                    .into_iter()
                    .flat_map(refs_in_str)
                {
                    if let Some(src) = source_of(&r, &with_src, None, &output, &id_of) {
                        with_src.insert(key.value.as_str(), src);
                        break;
                    }
                }
            }
            let item_src: Option<String> = task.for_each.as_ref().and_then(|f| match &f.value {
                nika_schema::raw::ForEachValue::Expression(src) => refs_in_str(src)
                    .into_iter()
                    .find_map(|r| source_of(&r, &with_src, None, &output, &id_of)),
                nika_schema::raw::ForEachValue::List(_) => None,
            #[allow(
                clippy::unreachable,
                reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
            )]
            other => unreachable!("unknown for_each form: {other:?}"),
            });

            let mut effect_src: Option<String> = action_effect_fields(&task.action)
                .into_iter()
                .flat_map(refs_in_str)
                .find_map(|r| source_of(&r, &with_src, item_src.as_ref(), &output, &id_of));
            if effect_src.is_none() && matches!(task.action, RawAction::Exec(_)) && fs_read_declared
            {
                effect_src.clone_from(&any_writer_so_far);
            }

            // Output taint: born-source wins; else effect flows out (inversion).
            let mut out_src = if classify(&task.action, mcp_trusted).0 {
                Some(task.id.value.clone())
            } else {
                effect_src.clone()
            };
            if out_src.is_none()
                && let Some(on_error) = &task.on_error
                && let OnErrorAction::Recover(value) = &on_error.value.action
            {
                out_src = collect_json_strings(&value.value)
                    .into_iter()
                    .flat_map(refs_in_str)
                    .find_map(|r| source_of(&r, &with_src, item_src.as_ref(), &output, &id_of));
            }

            if out_src.is_some()
                && any_writer_so_far.is_none()
                && classify(&task.action, mcp_trusted).1
            {
                any_writer_so_far.clone_from(&out_src);
            }
            effect[i] = effect_src;
            output[i] = out_src;
        }
    }

    (0..n)
        .map(|i| TaintWitness::new(effect[i].is_some(), effect[i].clone()))
        .collect()
}

fn source_of(
    r: &NamespaceRef,
    with_src: &BTreeMap<&str, String>,
    item_src: Option<&String>,
    output: &[Option<String>],
    id_of: &BTreeMap<&str, usize>,
) -> Option<String> {
    match r {
        NamespaceRef::Tasks { id, .. } => id_of
            .get(id.as_str())
            .and_then(|&u| output.get(u).cloned().flatten()),
        NamespaceRef::With(key) => with_src.get(key.as_str()).cloned(),
        NamespaceRef::Item => item_src.cloned(),
        _ => None,
    }
}

/// The invoke-side classifications, ONE effect-table walk: `(born_ingress,
/// writes_fs)` — the module doc's source table, and the writer clause.
pub(super) fn classify(action: &RawAction, mcp_trusted: &dyn Fn(&str) -> bool) -> (bool, bool) {
    match action {
        RawAction::Exec(_) => (false, true),
        RawAction::Agent(a) => (
            a.tools.iter().any(|t| {
                let g = t.value.as_str();
                !g.starts_with('!')
                    && (g.starts_with("mcp:") || nika_cap::glob_matches(g, "nika:fetch"))
            }),
            false,
        ),
        RawAction::Invoke(inv) => {
            let Some(tool) = inv.tool() else {
                return (false, false); // a child call — spec 14 owns its boundary
            };
            let id = tool.value.as_str();
            if id == "nika:fetch" {
                return (true, false);
            }
            if let Some(server) = id.strip_prefix("mcp:") {
                // Server content untrusted UNLESS the catalog marks it
                // (absent/unknown = untrusted) · server writes are opaque.
                return (
                    !mcp_trusted(server.split('/').next().unwrap_or(server)),
                    true,
                );
            }
            let args = inv.args.as_ref().map(|a| &a.value);
            let writes = match nika_cap::builtin_effect(id, args) {
                Some(nika_cap::BuiltinEffect::Fs { writes, .. }) => writes,
                _ => false,
            };
            (false, writes)
        }
        _ => (false, false),
    }
}
