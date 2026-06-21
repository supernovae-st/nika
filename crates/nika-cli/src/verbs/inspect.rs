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
    // The spec §6 footer verbatim — NIKA-DAG-001 is the conformance code
    // the ladder proved clean to get here.
    out.push_str("└─ (no orphans · DAG check NIKA-DAG-001 clean)\n");
    render_analysis(&mut out, report.analysis.as_ref());
    VerbOutput::ok(out)
}

/// The engineering read in the anatomy surface — the scheduler-
/// independent facts the report already computed (check/analysis.rs):
/// exact width with its witness antichain, pinch points, the widest
/// failure blast radii. Single-task workflows render nothing extra
/// (width 1 of 1 is noise) and an absent read (oversized workflow ·
/// honest skip) renders nothing — never a claim it cannot back.
fn render_analysis(out: &mut String, analysis: Option<&nika_schema::check::DagAnalysis>) {
    let Some(a) = analysis else { return };
    if a.width_witness.len() < 2 {
        return;
    }
    let mut witness: Vec<&str> = a.width_witness.iter().map(String::as_str).collect();
    witness.truncate(4);
    let ellipsis = if a.width_witness.len() > 4 {
        " · …"
    } else {
        ""
    };
    let _ = writeln!(
        out,
        "\nparallelism  width {} · can run together: {}{ellipsis}",
        a.width,
        witness.join(" · "),
    );
    if !a.pinch_points.is_empty() {
        let _ = writeln!(
            out,
            "pinch        {} · nothing else runs while these run",
            a.pinch_points.join(" · "),
        );
    }
    // Failure economics at a glance — the report sorts widest-first.
    let top: Vec<String> = a
        .blast_radius
        .iter()
        .take(3)
        .map(|b| format!("{} blocks {}", b.task, b.blocks))
        .collect();
    if !top.is_empty() {
        let more = a.blast_radius.len().saturating_sub(3);
        let suffix = if more > 0 {
            format!(" · +{more} more")
        } else {
            String::new()
        };
        let _ = writeln!(out, "blast        {}{suffix}", top.join(" · "));
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verbs::exit;

    fn tmp(content: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "nika-inspect-{}-{}.nika.yaml",
            std::process::id(),
            content.len(),
        ));
        std::fs::write(&path, content).expect("fixture written");
        path
    }

    #[test]
    fn anatomy_carries_the_engineering_read() {
        // The diamond: width 2 ({left,right}) · pinch {root,join} ·
        // root blocks 3 — the report computed it, inspect must SAY it.
        let path = tmp(
            "nika: v1\nworkflow: anatomy\n\nmodel: mock/echo\n\ntasks:\n  - id: root\n    infer: { prompt: \"r\", max_tokens: 10 }\n  - id: left\n    depends_on: [root]\n    infer: { prompt: \"l\", max_tokens: 10 }\n  - id: right\n    depends_on: [root]\n    infer: { prompt: \"x\", max_tokens: 10 }\n  - id: join\n    depends_on: [left, right]\n    infer: { prompt: \"j\", max_tokens: 10 }\noutputs:\n  result: ${{ tasks.join.output }}\n",
        );
        let out = run(path.to_str().expect("utf-8 tmp path"));
        std::fs::remove_file(&path).ok();
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(out.text.contains("parallelism  width 2"), "{}", out.text);
        assert!(
            out.text.contains("left") && out.text.contains("right"),
            "{}",
            out.text
        );
        assert!(
            out.text.contains("pinch        root · join"),
            "{}",
            out.text
        );
        assert!(
            out.text.contains("blast        root blocks 3"),
            "{}",
            out.text
        );
    }

    #[test]
    fn single_task_anatomy_stays_quiet() {
        // width 1 of 1 is noise, not insight — no engineering section.
        let path = tmp(
            "nika: v1\nworkflow: solo\n\nmodel: mock/echo\n\ntasks:\n  - id: only\n    infer: { prompt: \"x\", max_tokens: 10 }\noutputs:\n  result: ${{ tasks.only.output }}\n",
        );
        let out = run(path.to_str().expect("utf-8 tmp path"));
        std::fs::remove_file(&path).ok();
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(!out.text.contains("parallelism"), "{}", out.text);
        assert!(!out.text.contains("blast"), "{}", out.text);
    }

    /// A fan-out of 5 nests every child under its first parent (the box-drawing
    /// tree) and the witness antichain (width 5) truncates to 4 names + an
    /// ellipsis. The exact rails pin the recursion (a mutated child-match or
    /// last-detection breaks the nesting); the `· …` pins the `> 4` truncation.
    #[test]
    fn fan_out_nests_children_and_truncates_the_witness() {
        let path = tmp(
            "nika: v1\nworkflow: fan5\ntasks:\n  - id: root\n    exec: { command: \"echo r\" }\n  - id: c1\n    depends_on: [root]\n    exec: { command: \"echo 1\" }\n  - id: c2\n    depends_on: [root]\n    exec: { command: \"echo 2\" }\n  - id: c3\n    depends_on: [root]\n    exec: { command: \"echo 3\" }\n  - id: c4\n    depends_on: [root]\n    exec: { command: \"echo 4\" }\n  - id: c5\n    depends_on: [root]\n    exec: { command: \"echo 5\" }\n",
        );
        let out = run(path.to_str().expect("utf-8 tmp path"));
        std::fs::remove_file(&path).ok();
        assert_eq!(out.code, exit::OK, "{}", out.text);
        // The tree: root on the spine, c1 the FIRST branch, c5 the LAST.
        assert!(
            out.text.contains("└─ root"),
            "root on the spine: {}",
            out.text
        );
        assert!(
            out.text.contains("   ├─ c1"),
            "c1 nested + branch rail: {}",
            out.text
        );
        assert!(
            out.text.contains("   └─ c5"),
            "c5 nested + closing rail: {}",
            out.text
        );
        // Width 5 → witness shows 4 names + the ellipsis.
        assert!(
            out.text
                .contains("width 5 · can run together: c1 · c2 · c3 · c4 · …"),
            "{}",
            out.text
        );
        // One blast entry (root) ≤ 3 → NO "+N more" suffix.
        assert!(
            !out.text.contains("more"),
            "single blast → no suffix: {}",
            out.text
        );
    }

    /// A wide diamond (4 middles · join) pins the truncation BOUNDARY: width 4
    /// shows all 4 witness names with NO ellipsis (`> 4` is false), and the
    /// blast report caps at 3 with a `+2 more` suffix (the `more > 0` guard).
    #[test]
    fn width_four_has_no_ellipsis_and_blast_caps_at_three() {
        let path = tmp(
            "nika: v1\nworkflow: wide-diamond\ntasks:\n  - id: root\n    exec: { command: \"echo r\" }\n  - id: m1\n    depends_on: [root]\n    exec: { command: \"echo 1\" }\n  - id: m2\n    depends_on: [root]\n    exec: { command: \"echo 2\" }\n  - id: m3\n    depends_on: [root]\n    exec: { command: \"echo 3\" }\n  - id: m4\n    depends_on: [root]\n    exec: { command: \"echo 4\" }\n  - id: join\n    depends_on: [m1, m2, m3, m4]\n    exec: { command: \"echo j\" }\n",
        );
        let out = run(path.to_str().expect("utf-8 tmp path"));
        std::fs::remove_file(&path).ok();
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(
            out.text
                .contains("width 4 · can run together: m1 · m2 · m3 · m4"),
            "{}",
            out.text
        );
        assert!(
            !out.text.contains("m4 · …"),
            "no ellipsis at the boundary: {}",
            out.text
        );
        assert!(
            out.text.contains("+2 more"),
            "blast caps at 3 + suffix: {}",
            out.text
        );
    }
}
