// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika inspect` — the static anatomy as a terminal DAG (spec §6).
//!
//! Derives from the SAME projection as `nika graph` (one projector · N
//! renderers): a box-drawing tree in first-parent order, each task drawn
//! once, the footer attesting the DAG check. Static facts only — run
//! overlays belong to the trace surface.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::verbs::graph::{GraphDoc, project};
use crate::verbs::{VerbOutput, load_checked};

/// The `nika inspect <file>` verb.
#[must_use]
pub fn run(path: &str) -> VerbOutput {
    let (wf, report) = match load_checked(path) {
        Ok(pair) => pair,
        Err(out) => return out,
    };
    if !report.conformance.is_empty() {
        let mut text = String::from("cannot inspect: no valid DAG order while conformance fails\n");
        for c in &report.conformance {
            let _ = writeln!(text, "  [{}] {}", c.code, c.message);
        }
        return VerbOutput::file(text);
    }
    let doc = project(&wf, &report);

    let ceiling = if report.cost.tasks.is_empty() {
        "no inference tasks · $0.00".to_owned()
    } else if report.cost.has_unbounded {
        format!(
            "≥ ${:.4} (floor — unbounded tasks)",
            report.cost.bounded_total_usd
        )
    } else {
        format!("≤ ${:.4}", report.cost.bounded_total_usd)
    };

    let mut out = format!(
        "{} · {} task(s) · {} wave(s) · {ceiling}\n",
        doc.workflow,
        doc.nodes.len(),
        report.waves.len(),
    );
    render_tree(&mut out, &doc);
    out.push_str("└─ (no orphans · DAG order proven by the check ladder)\n");
    VerbOutput::ok(out)
}

/// Box-drawing tree: roots at depth 0, children under their FIRST parent
/// (a DAG joins; the tree shows one spine and the graph verb shows all
/// edges). Every node printed exactly once, in projection (wave) order.
fn render_tree(out: &mut String, doc: &GraphDoc) {
    // first-parent map + root list, both in stable projection order.
    let mut first_parent: BTreeMap<&str, &str> = BTreeMap::new();
    for edge in &doc.edges {
        first_parent.entry(edge.to.as_str()).or_insert(&edge.from);
    }
    let children = |id: &str| -> Vec<&str> {
        doc.nodes
            .iter()
            .filter(|n| first_parent.get(n.id.as_str()) == Some(&id))
            .map(|n| n.id.as_str())
            .collect()
    };
    let mut printed: BTreeSet<&str> = BTreeSet::new();

    let roots: Vec<&str> = doc
        .nodes
        .iter()
        .filter(|n| !first_parent.contains_key(n.id.as_str()))
        .map(|n| n.id.as_str())
        .collect();
    let id_width = doc.nodes.iter().map(|n| n.id.len()).max().unwrap_or(0);

    for (i, root) in roots.iter().enumerate() {
        let last = i + 1 == roots.len();
        walk(out, doc, root, "", last, &children, &mut printed, id_width);
    }
}

/// Recursive spine walk — `prefix` carries the rail glyphs of ancestors.
#[allow(clippy::too_many_arguments)] // a local recursion, not an API
fn walk<'a>(
    out: &mut String,
    doc: &'a GraphDoc,
    id: &'a str,
    prefix: &str,
    last: bool,
    children: &dyn Fn(&str) -> Vec<&'a str>,
    printed: &mut BTreeSet<&'a str>,
    id_width: usize,
) {
    if !printed.insert(id) {
        return;
    }
    // Projection nodes cover every edge endpoint (same source data) —
    // a miss would be a projector bug; render nothing rather than panic.
    let Some(node) = doc.nodes.iter().find(|n| n.id == id) else {
        return;
    };

    let mut meta: Vec<String> = vec![node.verb.to_owned()];
    if let Some(tool) = &node.tool {
        meta.push(tool.clone());
    }
    if let Some(model) = &node.model {
        meta.push(model.clone());
    }
    if let Some([min, max]) = node.cost_interval {
        meta.push(format!("~${min:.4}-{max:.4}"));
    }
    if let Some(fan) = &node.fan_out {
        match fan.count {
            Some(n) => meta.push(format!("for_each ×{n}")),
            None => meta.push("for_each ×?".to_owned()),
        }
    }
    if let Some(when) = &node.when {
        meta.push(format!("when: {when}"));
    }

    let branch = if last { "└─" } else { "├─" };
    let _ = writeln!(
        out,
        "{prefix}{branch} {:<id_width$}  {}",
        node.id,
        meta.join(" · "),
    );

    let kids = children(id);
    let child_prefix = format!("{prefix}{}", if last { "   " } else { "│  " });
    for (i, kid) in kids.iter().enumerate() {
        let kid_last = i + 1 == kids.len();
        walk(
            out,
            doc,
            kid,
            &child_prefix,
            kid_last,
            children,
            printed,
            id_width,
        );
    }
}
