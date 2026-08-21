// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika inspect` — the static anatomy as a terminal DAG (spec §6).
//!
//! Derives from the SAME projection as `--format json|mermaid|dot` (one projector · N
//! renderers): waves rendered as bordered visual groups ("N in parallel")
//! joined by flow arrows — the parallelism the scheduler proved, made
//! visible. Static facts only — run overlays belong to the trace surface.

use std::fmt::Write as _;

use crate::display::theme::{Role, Theme};
use crate::verbs::graph::{GraphDoc, Node, project};
use crate::verbs::{VerbOutput, load_checked};

/// Widest a wave box may grow (2-indent + corners lands the line ≤ 78 —
/// graceful under 80 columns).
const BOX_INNER_CAP: usize = 74;

/// The glyph column for the wave-group render — unicode default, ASCII
/// parity first-class (the same two-theme law as the run storyboard).
struct Glyphs {
    /// Box corners `(top-left, top-right, bottom-left, bottom-right)`.
    corners: [char; 4],
    /// Horizontal rule.
    h: char,
    /// Vertical rail.
    v: char,
    /// Inter-wave flow arrow (`↓` / `v`).
    arrow: &'static str,
    /// Truncation mark when a row outgrows the box cap.
    ellipsis: &'static str,
}

const UNICODE_GLYPHS: Glyphs = Glyphs {
    corners: ['╭', '╮', '╰', '╯'],
    h: '─',
    v: '│',
    arrow: "↓",
    ellipsis: "…",
};

const ASCII_GLYPHS: Glyphs = Glyphs {
    corners: ['+', '+', '+', '+'],
    h: '-',
    v: '|',
    arrow: "v",
    ellipsis: "~",
};

/// The `nika inspect <file>` verb. The theme carries the glyph column
/// (`--ascii` · CI logs · legacy terminals) AND the colour capability —
/// verb identity rides the tokens-SSOT glyph chips, chrome stays dim.
#[must_use]
pub fn run(path: &str, theme: Theme) -> VerbOutput {
    match load_checked(path) {
        Ok((wf, report)) => render_pair(&wf, &report, theme),
        Err(out) => out,
    }
}

/// Render a checked pair without re-reading the file. This keeps the human
/// dry-run card on the same model-overridden pair as its JSON twin (#1051).
pub(crate) fn render_pair(
    wf: &nika_schema::raw::RawWorkflow,
    report: &nika_check::CheckReport,
    theme: Theme,
) -> VerbOutput {
    if !report.conformance.is_empty() {
        let mut text = String::from("cannot inspect: no valid DAG order while conformance fails\n");
        for c in &report.conformance {
            let _ = writeln!(text, "  [{}] {}", c.code, c.message);
        }
        return VerbOutput::file(text);
    }
    let doc = project(wf, report);

    let ceiling = if report.cost.tasks.is_empty() {
        // One voice with the COST rung (`check/render.rs`), narrowed for
        // the same measured reason: this lane prices `infer:`/`agent:`
        // and nothing else, so `$0.00` was a claim about a bill it never
        // saw. A lone `exec: ["claude", "-p", …]` reaches here.
        "no infer/agent tasks · $0.00 · exec + mcp spend unpriced".to_owned()
    } else if report.cost.has_unbounded {
        // One voice with the COST rung (check/render.rs): `≥` claimed a
        // floor over a number that bounds nothing from below — render.rs
        // documents the 126× measurement. Claim neither bound; show the
        // priced portion.
        format!(
            "est unbounded · bounded portion ${}",
            crate::text::usd(report.cost.bounded_total_usd)
        )
    } else {
        // `est out` + the vocab seam, one voice with the audited card:
        // a flat `≤ $X` read as the whole bill (render.rs documents the
        // 328× measurement — prompts, exec + mcp are unpriced), and the
        // hardcoded `≤` leaked unicode under `--ascii` (the same class
        // as the verdict glyph the card already fixed).
        format!(
            "est out {}${}",
            crate::display::vocab::at_most(theme.ascii),
            crate::text::usd(report.cost.bounded_total_usd)
        )
    };
    // The NEP-0018 §4 aggregate — energy beside cost, same honesty
    // markers, computed by the SAME classification the check ladder's
    // ENERGY rung renders (one classifier · two surfaces).
    let energy = crate::verbs::check::energy::inspect_fragment(report, theme.ascii)
        .map(|f| format!(" · {f}"))
        .unwrap_or_default();

    let mut out = format!(
        "{} · {} · {} · {ceiling}{energy}\n",
        theme.paint(Role::Strong, &doc.workflow),
        crate::text::count(doc.nodes.len(), "task"),
        crate::text::count(report.waves.len(), "wave"),
    );
    // The pinch set (DagAnalysis · « nothing else runs while these
    // run ») — drawn ON the graph, not only said under it: the P1 of
    // the analytics-on-the-drawing plan (buck2 computes, nika draws).
    let pinch: std::collections::BTreeSet<&str> = report
        .analysis
        .as_ref()
        .map(|a| a.pinch_points.iter().map(String::as_str).collect())
        .unwrap_or_default();
    let wave_sizes: Vec<usize> = report.waves.iter().map(Vec::len).collect();
    render_waves(&mut out, &doc, &wave_sizes, theme, &pinch);
    // The spec §6 footer verbatim — NIKA-DAG-001 is the conformance code
    // the ladder proved clean to get here.
    out.push_str("  (no orphans · DAG check NIKA-DAG-001 clean)\n");
    render_analysis(&mut out, report.analysis.as_ref());
    VerbOutput::ok(out)
}

/// The engineering read in the anatomy surface — the scheduler-
/// independent facts the report already computed (check/analysis.rs):
/// exact width with its witness antichain, pinch points, the widest
/// failure blast radii. Single-task workflows render nothing extra
/// (width 1 of 1 is noise) and an absent read (oversized workflow ·
/// honest skip) renders nothing — never a claim it cannot back.
fn render_analysis(out: &mut String, analysis: Option<&nika_check::DagAnalysis>) {
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

/// The static facts one task row carries (verb · tool · model · cost ·
/// fan-out · gate) — the SAME vocabulary the old tree drew, per node.
fn node_meta(node: &Node) -> String {
    let mut meta: Vec<String> = vec![node.verb.to_owned()];
    if let Some(tool) = &node.tool {
        meta.push(tool.clone());
    }
    if let Some(model) = &node.model {
        meta.push(model.clone());
    }
    if let Some([min, max]) = node.cost_interval {
        meta.push(format!(
            "~${}-{}",
            crate::text::usd(min),
            crate::text::usd(max)
        ));
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
    meta.join(" · ")
}

/// Waves as visual groups: a bordered box per parallel wave ("N in
/// parallel"), a bare row for a single-task wave, flow arrows between
/// waves. The projection's node order IS wave order (one projector law),
/// so `wave_sizes` slices it without re-deriving anything.
fn render_waves(
    out: &mut String,
    doc: &GraphDoc,
    wave_sizes: &[usize],
    theme: Theme,
    pinch: &std::collections::BTreeSet<&str>,
) {
    let g = if theme.ascii {
        &ASCII_GLYPHS
    } else {
        &UNICODE_GLYPHS
    };
    let id_width = doc
        .nodes
        .iter()
        .map(|n| n.id.chars().count())
        .max()
        .unwrap_or(0);
    let mut cursor = 0usize;
    for (i, &size) in wave_sizes.iter().enumerate() {
        let end = cursor.saturating_add(size).min(doc.nodes.len());
        let members = &doc.nodes[cursor..end];
        cursor = end;
        if i > 0 {
            let _ = writeln!(out, "    {}", theme.paint(Role::Dim, g.arrow));
        }
        if members.len() > 1 {
            render_wave_group(out, i + 1, members, id_width, g, theme, pinch);
        } else if let Some(node) = members.first() {
            let _ = writeln!(
                out,
                "  {}{:<id_width$}{} {}",
                theme.verb_glyph(node.verb),
                node.id,
                pinch_mark(&node.id, pinch, theme),
                theme.paint(Role::Dim, &node_meta(node)),
            );
        }
    }
}

/// The pinch marker — `⧗` Warn (`!` ASCII) beside a task that gates
/// the whole graph (« nothing else runs while it runs »), a single
/// stable cell so the id column never jitters; a dim space otherwise.
fn pinch_mark(id: &str, pinch: &std::collections::BTreeSet<&str>, theme: Theme) -> String {
    if pinch.contains(id) {
        theme.paint(Role::Warn, if theme.ascii { "!" } else { "⧗" })
    } else {
        " ".to_owned()
    }
}

/// One bordered wave group — header names the wave + its parallelism, each
/// member row aligned on the shared id column, width capped so the box
/// stays graceful under 80 columns (overlong rows truncate with a mark).
fn render_wave_group(
    out: &mut String,
    n: usize,
    members: &[Node],
    id_width: usize,
    g: &Glyphs,
    theme: Theme,
    pinch: &std::collections::BTreeSet<&str>,
) {
    // Width math runs on RAW text (ANSI escapes break cell arithmetic —
    // theme.rs law); paint happens at emission, segment by segment.
    let header = format!(" wave {n} {h}{h} {} in parallel ", members.len(), h = g.h);
    let raw_metas: Vec<String> = members.iter().map(node_meta).collect();
    let inner = raw_metas
        .iter()
        .map(|m| 2 + id_width + 2 + m.chars().count() + 2)
        .chain(std::iter::once(header.chars().count() + 1))
        .max()
        .unwrap_or(0)
        .min(BOX_INNER_CAP);
    let rule: String =
        std::iter::repeat_n(g.h, inner.saturating_sub(header.chars().count())).collect();
    let _ = writeln!(
        out,
        "  {}",
        theme.paint(
            Role::Dim,
            &format!("{}{header}{rule}{}", g.corners[0], g.corners[1])
        )
    );
    let v = theme.paint(Role::Dim, &g.v.to_string());
    for (node, raw_meta) in members.iter().zip(&raw_metas) {
        let meta_room = inner.saturating_sub(2).saturating_sub(2 + id_width + 2);
        let fitted = fit(raw_meta, meta_room, g.ellipsis);
        let pad = inner
            .saturating_sub(2)
            .saturating_sub(2 + id_width + 2 + fitted.chars().count());
        let _ = writeln!(
            out,
            "  {v} {}{:<id_width$}{} {}{} {v}",
            theme.verb_glyph(node.verb),
            node.id,
            pinch_mark(&node.id, pinch, theme),
            theme.paint(Role::Dim, &fitted),
            " ".repeat(pad),
        );
    }
    let bottom: String = std::iter::repeat_n(g.h, inner).collect();
    let _ = writeln!(
        out,
        "  {}",
        theme.paint(
            Role::Dim,
            &format!("{}{bottom}{}", g.corners[2], g.corners[3])
        )
    );
}

/// Truncate a row to `width` display cells, marking the cut — the box
/// border stays intact when a meta string outgrows the cap.
fn fit(s: &str, width: usize, ellipsis: &str) -> String {
    if s.chars().count() <= width {
        return s.to_owned();
    }
    let keep = width.saturating_sub(ellipsis.chars().count());
    let mut out: String = s.chars().take(keep).collect();
    out.push_str(ellipsis);
    out
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
            "nika: anatomy\n\nmodel: mock/echo\n\ntasks:\n  root:\n    infer: { prompt: \"r\", max_tokens: 10 }\n  left:\n    after:\n      root: success\n    infer: { prompt: \"l\", max_tokens: 10 }\n  right:\n    after:\n      root: success\n    infer: { prompt: \"x\", max_tokens: 10 }\n  join:\n    after:\n      left: success\n      right: success\n    infer: { prompt: \"j\", max_tokens: 10 }\noutputs:\n  result: ${{ tasks.join.output }}\n",
        );
        let out = run(
            path.to_str().expect("utf-8 tmp path"),
            Theme::new(false, false, false),
        );
        std::fs::remove_file(&path).ok();
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(out.text.contains("parallelism  width 2"), "{}", out.text);
        // P1 · the pinch is DRAWN, not only said: root and join carry
        // the ⧗ mark on their own rows (left/right stay unmarked).
        assert!(out.text.contains("root ⧗"), "pinch on root: {}", out.text);
        assert!(out.text.contains("join ⧗"), "pinch on join: {}", out.text);
        assert!(!out.text.contains("left ⧗"), "{}", out.text);
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
        // The diamond's middle wave is a bordered group; the solo root
        // and join render as bare rows joined by flow arrows.
        assert!(
            out.text.contains("╭ wave 2 ── 2 in parallel "),
            "wave group header: {}",
            out.text
        );
        assert!(
            out.text.contains("│ ◇ left ") && out.text.contains("│ ◇ right"),
            "boxed members: {}",
            out.text
        );
        assert!(out.text.contains("  ◇ root "), "bare root: {}", out.text);
        assert_eq!(
            out.text.matches("    ↓").count(),
            2,
            "two flow arrows join three waves: {}",
            out.text
        );
    }

    /// The header aggregates BOTH honesty ladders (NEP-0018 §4): the
    /// cost ceiling speaks `est out` + the vocab seam (a flat `≤ $X`
    /// read as the whole bill · 328× measured — and leaked unicode
    /// under `--ascii`), and the energy aggregate rides beside it, from
    /// the SAME classification the check ladder renders.
    #[test]
    fn header_carries_cost_and_energy_aggregates() {
        let body = "nika: agg\n\nmodel: groq/qwen/qwen3-32b\n\ntasks:\n  only:\n    infer: { prompt: \"x\", max_tokens: 1000 }\noutputs:\n  result: ${{ tasks.only.output }}\n";
        let path = tmp(body);
        let out = run(
            path.to_str().expect("utf-8 tmp path"),
            Theme::new(false, false, false),
        );
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(
            out.text.contains("est out ≤$"),
            "cost speaks est-out, never a bare bill: {}",
            out.text
        );
        assert!(
            out.text.contains(" Wh (gpu)"),
            "the energy aggregate rides the header with its scope: {}",
            out.text
        );
        let ascii = run(
            path.to_str().expect("utf-8 tmp path"),
            Theme::new(false, true, false),
        );
        std::fs::remove_file(&path).ok();
        assert_eq!(ascii.code, exit::OK, "{}", ascii.text);
        assert!(
            ascii.text.contains("est out <=$") && ascii.text.contains("<= "),
            "ascii twins ride the seam: {}",
            ascii.text
        );
        assert!(
            !ascii.text.contains('≤'),
            "no unicode bound mark under --ascii: {}",
            ascii.text
        );
    }

    #[test]
    fn single_task_anatomy_stays_quiet() {
        // width 1 of 1 is noise, not insight — no engineering section.
        let path = tmp(
            "nika: solo\n\nmodel: mock/echo\n\ntasks:\n  only:\n    infer: { prompt: \"x\", max_tokens: 10 }\noutputs:\n  result: ${{ tasks.only.output }}\n",
        );
        let out = run(
            path.to_str().expect("utf-8 tmp path"),
            Theme::new(false, false, false),
        );
        std::fs::remove_file(&path).ok();
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(!out.text.contains("parallelism"), "{}", out.text);
        assert!(!out.text.contains("blast"), "{}", out.text);
        // One wave of one task: no box, no arrow — a bare row only.
        assert!(
            !out.text.contains('╭'),
            "no box for a solo wave: {}",
            out.text
        );
        assert!(
            !out.text.contains('↓'),
            "no arrow with one wave: {}",
            out.text
        );
        assert!(out.text.contains("  ◇ only"), "{}", out.text);
    }

    /// A fan-out of 5 renders as ONE bordered wave group ("5 in parallel")
    /// under the root's bare row, and the witness antichain (width 5)
    /// truncates to 4 names + an ellipsis (the `> 4` truncation).
    #[test]
    fn fan_out_groups_the_wave_and_truncates_the_witness() {
        let path = tmp(
            "nika: fan5\ntasks:\n  root:\n    exec: { command: [\"echo\", \"r\"] }\n  c1:\n    after:\n      root: success\n    exec: { command: [\"echo\", \"1\"] }\n  c2:\n    after:\n      root: success\n    exec: { command: [\"echo\", \"2\"] }\n  c3:\n    after:\n      root: success\n    exec: { command: [\"echo\", \"3\"] }\n  c4:\n    after:\n      root: success\n    exec: { command: [\"echo\", \"4\"] }\n  c5:\n    after:\n      root: success\n    exec: { command: [\"echo\", \"5\"] }\n",
        );
        let out = run(
            path.to_str().expect("utf-8 tmp path"),
            Theme::new(false, false, false),
        );
        std::fs::remove_file(&path).ok();
        assert_eq!(out.code, exit::OK, "{}", out.text);
        // Wave 1 = the bare root · arrow · wave 2 = the bordered fan of 5.
        assert!(
            out.text.contains("  ▷ root"),
            "bare root (exec chip): {}",
            out.text
        );
        assert!(
            out.text.contains("╭ wave 2 ── 5 in parallel "),
            "fan group header: {}",
            out.text
        );
        for c in ["c1", "c2", "c3", "c4", "c5"] {
            assert!(
                out.text.contains(&format!("│ ▷ {c}")),
                "{c} boxed: {}",
                out.text
            );
        }
        assert!(out.text.contains("    ↓"), "flow arrow: {}", out.text);
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

    /// ASCII parity is first-class: every wave-group glyph has an ASCII
    /// twin (`◆→#` · box → `+-|` · `↓→v`) and NO unicode leaks through.
    #[test]
    fn ascii_theme_draws_the_same_waves() {
        let path = tmp(
            "nika: fanscii\ntasks:\n  root:\n    exec: { command: [\"echo\", \"r\"] }\n  c1:\n    after:\n      root: success\n    exec: { command: [\"echo\", \"1\"] }\n  c2:\n    after:\n      root: success\n    exec: { command: [\"echo\", \"2\"] }\n",
        );
        let out = run(
            path.to_str().expect("utf-8 tmp path"),
            Theme::new(false, true, false),
        );
        std::fs::remove_file(&path).ok();
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(
            out.text.contains("+ wave 2 -- 2 in parallel "),
            "ascii header: {}",
            out.text
        );
        assert!(
            out.text.contains("| $ c1"),
            "ascii member (exec chip): {}",
            out.text
        );
        assert!(
            out.text.contains("  $ root"),
            "ascii bare row (exec chip): {}",
            out.text
        );
        assert!(out.text.contains("    v\n"), "ascii arrow: {}", out.text);
        for glyph in ['◆', '╭', '╮', '╰', '╯', '│', '─', '↓'] {
            assert!(
                !out.text.contains(glyph),
                "unicode {glyph} leaked into --ascii: {}",
                out.text
            );
        }
    }

    /// A wide diamond (4 middles · join) pins the truncation BOUNDARY: width 4
    /// shows all 4 witness names with NO ellipsis (`> 4` is false), and the
    /// blast report caps at 3 with a `+2 more` suffix (the `more > 0` guard).
    #[test]
    fn width_four_has_no_ellipsis_and_blast_caps_at_three() {
        let path = tmp(
            "nika: wide-diamond\ntasks:\n  root:\n    exec: { command: [\"echo\", \"r\"] }\n  m1:\n    after:\n      root: success\n    exec: { command: [\"echo\", \"1\"] }\n  m2:\n    after:\n      root: success\n    exec: { command: [\"echo\", \"2\"] }\n  m3:\n    after:\n      root: success\n    exec: { command: [\"echo\", \"3\"] }\n  m4:\n    after:\n      root: success\n    exec: { command: [\"echo\", \"4\"] }\n  join:\n    after:\n      m1: success\n      m2: success\n      m3: success\n      m4: success\n    exec: { command: [\"echo\", \"j\"] }\n",
        );
        let out = run(
            path.to_str().expect("utf-8 tmp path"),
            Theme::new(false, false, false),
        );
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
