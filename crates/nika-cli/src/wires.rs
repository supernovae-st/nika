// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The terminal DAG — waves as columns, tasks as rows, real wires.
//!
//! ```text
//!         ╭─▶ left  ─╮
//! root ──▶┤          ├──▶ join        (a diamond, drawn truthfully)
//!         ╰─▶ right ─╯
//! ```
//!
//! One law: **draw only what the drawing can say truthfully**. Each
//! gutter between adjacent waves owns ONE vertical rail; a rail can
//! merge lines, and a merged line claims « everything left of me feeds
//! everything right of me ». That claim is true exactly when the
//! gutter's edges form complete-bipartite components (chain · fan-out ·
//! join · diamond · disjoint parallels — every shape a template or a
//! first workflow has). Anything the rail would LIE about (crossings ·
//! partial fans · wave-skipping edges) returns `None` and the caller
//! falls back to a listing — a wrong picture is worse than no picture.
//!
//! Colour: rails and arrows are Dim chrome, task ids stay plain text —
//! semantic, never decorative (theme.rs law). ASCII twins every glyph.

use crate::verbs::graph::GraphDoc;
use nika_display::theme::{Role, Theme};

/// Widest a drawing may grow (display cells) — beyond this the caller's
/// listing fallback reads better than horizontal scrolling.
const MAX_CELLS: usize = 78;

/// Tallest a wave may stack before the art stops being a glance.
const MAX_ROWS: usize = 4;

/// One gutter's edge set, as row indices (source row → target rows).
type GutterEdges = Vec<(usize, usize)>;

/// Render the wave-column wiring for a projected graph, or `None` when
/// the one-rail-per-gutter drawing could not be truthful (the caller
/// falls back — never a wrong picture).
#[must_use]
pub(crate) fn render(doc: &GraphDoc, waves: &[Vec<usize>], theme: Theme) -> Option<String> {
    render_with(
        doc,
        waves,
        theme,
        &|id, verb| (theme.verb_glyph(verb), id.to_owned()),
        None,
    )
}

/// The same wire drawing with the NODE PAINTER injected — the live run
/// map paints each node by its STATE (spinner while running · Good/Bad
/// settled · dim pending) while the geometry stays this module's law.
/// The painter returns (chip · id) ALREADY painted; width math still
/// counts the RAW id cells (paint-after-measure · theme.rs law), and
/// every chip MUST hold the 2-cell contract the spin/glyph seams keep.
pub(crate) fn render_with(
    doc: &GraphDoc,
    waves: &[Vec<usize>],
    theme: Theme,
    node: &dyn Fn(&str, &str) -> (String, String),
    live: Option<(&std::collections::BTreeSet<String>, usize)>,
) -> Option<String> {
    if waves.is_empty() || waves.iter().any(|w| w.is_empty() || w.len() > MAX_ROWS) {
        return None;
    }
    // Node id → (wave, row) via the projection order law (node order IS
    // wave order — the same slicing inspect trusts). BTreeMap per the
    // workspace determinism policy (HashMap iteration order is banned).
    let mut coords = std::collections::BTreeMap::new();
    let mut cursor = 0usize;
    for (w, wave) in waves.iter().enumerate() {
        for (r, _) in wave.iter().enumerate() {
            let node = doc.nodes.get(cursor)?;
            coords.insert(node.id.as_str(), (w, r));
            cursor += 1;
        }
    }
    // Gutter edge sets — refuse a drawing any edge would falsify.
    let mut gutters: Vec<GutterEdges> = vec![Vec::new(); waves.len().saturating_sub(1)];
    for e in &doc.edges {
        let &(fw, fr) = coords.get(e.from.as_str())?;
        let &(tw, tr) = coords.get(e.to.as_str())?;
        if tw != fw + 1 {
            return None; // a wave-skipping wire cannot ride one rail
        }
        gutters[fw].push((fr, tr));
    }
    let spans: Vec<Vec<(usize, usize)>> = gutters
        .iter()
        .map(honest_gutter)
        .collect::<Option<Vec<_>>>()?;

    // (id, verb) pairs — the verb rides as the tokens-SSOT glyph chip
    // (◇▷◆✦ · 2 cells) in front of every id; width math counts the RAW
    // cells (paint after measuring · theme.rs law).
    let labels = wave_labels(doc, waves);
    let widths: Vec<usize> = labels
        .iter()
        .map(|col| {
            col.iter()
                .map(|(id, _)| 2 + id.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let rows = waves.iter().map(Vec::len).max().unwrap_or(1);
    let total: usize = widths.iter().sum::<usize>() + gutters.len() * 6 + 2;
    if total > MAX_CELLS {
        return None;
    }

    let hot = hot_rows(&coords, gutters.len(), live);
    let tick = live.map_or(0, |(_, t)| t);
    let g = Glyphs::for_theme(theme);
    let mut lines = Vec::with_capacity(rows);
    for row in 0..rows {
        let mut wire_row = String::from("  ");
        for (w, col) in labels.iter().enumerate() {
            let (id, verb) = col.get(row).copied().unwrap_or(("", ""));
            let pad = widths[w] - (2 + id.chars().count());
            if id.is_empty() {
                wire_row.push_str("  ");
                wire_row.push_str(&" ".repeat(pad));
            } else {
                let (chip, painted_id) = node(id, verb);
                wire_row.push_str(&chip);
                wire_row.push_str(&painted_id);
                wire_row.push_str(&" ".repeat(pad));
            }
            if w < gutters.len() {
                wire_row.push_str(&gutter_cell(
                    &gutters[w],
                    &spans[w],
                    row,
                    theme,
                    &g,
                    hot[w].contains(&row).then_some(tick),
                ));
            }
        }
        lines.push(wire_row.trim_end().to_owned());
    }
    Some(lines.join("\n"))
}

/// The (id · verb) columns, wave by wave — the projection order law
/// (node order IS wave order · the same slicing inspect trusts).
fn wave_labels<'d>(doc: &'d GraphDoc, waves: &[Vec<usize>]) -> Vec<Vec<(&'d str, &'d str)>> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    for wave in waves {
        let col: Vec<(&str, &str)> = doc.nodes[cursor..cursor + wave.len()]
            .iter()
            .map(|n| (n.id.as_str(), n.verb))
            .collect();
        cursor += wave.len();
        out.push(col);
    }
    out
}

/// Which rows of wave w+1 hold a RUNNING node — those gutters' target
/// cells pulse (the incoming edge carries the run's energy).
fn hot_rows(
    coords: &std::collections::BTreeMap<&str, (usize, usize)>,
    gutter_count: usize,
    live: Option<(&std::collections::BTreeSet<String>, usize)>,
) -> Vec<std::collections::BTreeSet<usize>> {
    let mut hot = vec![std::collections::BTreeSet::new(); gutter_count];
    if let Some((running, _)) = live {
        for (id, &(w, r)) in coords {
            if w > 0 && running.contains(*id) {
                hot[w - 1].insert(r);
            }
        }
    }
    hot
}

/// The honesty check for one gutter: group its edges into connected
/// components (via shared endpoints); every component must be complete
/// bipartite (all its sources feed all its targets) and no two
/// components' row spans may overlap — otherwise the single rail would
/// draw a claim the DAG does not make. Returns the per-component spans
/// (sorted) so the renderer draws each component's rail ONLY inside its
/// own rows — a global span merged disjoint parallels into a false rail.
/// Union-find root with path compression (the component grouping seam).
fn find(parent: &mut Vec<usize>, i: usize) -> usize {
    if parent[i] != i {
        let root = find(parent, parent[i]);
        parent[i] = root;
    }
    parent[i]
}

fn honest_gutter(edges: &GutterEdges) -> Option<Vec<(usize, usize)>> {
    if edges.is_empty() {
        return None; // a disconnected wave boundary has no truthful wire
    }
    // Union-find over edges: same component when sharing a source row
    // or a target row.
    let n = edges.len();
    let mut parent: Vec<usize> = (0..n).collect();
    for i in 0..n {
        for j in (i + 1)..n {
            if edges[i].0 == edges[j].0 || edges[i].1 == edges[j].1 {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                parent[ri] = rj;
            }
        }
    }
    let mut comps: std::collections::BTreeMap<usize, (Vec<usize>, Vec<usize>)> =
        std::collections::BTreeMap::new();
    for (i, &(fr, tr)) in edges.iter().enumerate() {
        let root = find(&mut parent, i);
        let entry = comps.entry(root).or_default();
        entry.0.push(fr);
        entry.1.push(tr);
    }
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for (mut sources, mut targets) in comps.into_values() {
        sources.sort_unstable();
        sources.dedup();
        targets.sort_unstable();
        targets.dedup();
        // Complete bipartite: |edges in component| == |S| × |T|.
        let count = edges
            .iter()
            .filter(|(fr, tr)| sources.contains(fr) && targets.contains(tr))
            .count();
        if count != sources.len() * targets.len() {
            return None;
        }
        let lo = *sources.iter().chain(targets.iter()).min()?;
        let hi = *sources.iter().chain(targets.iter()).max()?;
        spans.push((lo, hi));
    }
    spans.sort_unstable();
    for pair in spans.windows(2) {
        if pair[1].0 <= pair[0].1 {
            return None; // overlapping components would share the rail
        }
    }
    Some(spans)
}

/// The glyph column — unicode default, ASCII first-class.
/// The energy flowing INTO a running node — the incoming rail's last
/// segment cycles density (unicode) or dashes (ascii twin), painted
/// Accent: the map shows WHERE the run is spending its now.
const PULSE: [char; 4] = ['╍', '╌', '┄', '┈'];
const PULSE_ASCII: [char; 4] = ['=', '-', '~', '-'];

struct Glyphs {
    h: char,
    v: char,
    arrow: char,
    tee_r: char,    // ├  rail continues, line exits right
    tee_l: char,    // ┤  line enters from left, rail continues
    cross: char,    // ┼
    top_r: char,    // ╭  rail starts downward, exits right
    top_l: char,    // ╮  enters left, rail starts downward
    bot_r: char,    // ╰  rail ends upward, exits right
    bot_l: char,    // ╯  enters left, rail ends upward
    tee_down: char, // ┬ enters left, exits right, rail starts downward
    tee_up: char,   // ┴  enters left, exits right, rail ends upward
}

impl Glyphs {
    fn for_theme(theme: Theme) -> Self {
        if theme.ascii {
            Self {
                h: '-',
                v: '|',
                arrow: '>',
                tee_r: '+',
                tee_l: '+',
                cross: '+',
                top_r: '+',
                top_l: '+',
                bot_r: '+',
                bot_l: '+',
                tee_down: '+',
                tee_up: '+',
            }
        } else {
            Self {
                h: '─',
                v: '│',
                arrow: '▶',
                tee_r: '├',
                tee_l: '┤',
                cross: '┼',
                top_r: '╭',
                top_l: '╮',
                bot_r: '╰',
                bot_l: '╯',
                tee_down: '┬',
                tee_up: '┴',
            }
        }
    }
}

/// Render one gutter's 6-cell block for one row: ` {L}{J}{R}{A} ` where
/// L = left stub (source exits) · J = the rail junction · R/A = right
/// stub + arrowhead (target enters) · a breathing space each side.
/// The rail exists at a row ONLY inside the row's own component span
/// (per-component spans from [`honest_gutter`] — a global span merged
/// disjoint parallels into a false rail). Painted Dim as one chrome run.
fn gutter_cell(
    edges: &GutterEdges,
    spans: &[(usize, usize)],
    row: usize,
    theme: Theme,
    g: &Glyphs,
    pulse_tick: Option<usize>,
) -> String {
    let is_source = edges.iter().any(|&(fr, _)| fr == row);
    let is_target = edges.iter().any(|&(_, tr)| tr == row);
    // This row's own component (spans are disjoint + sorted).
    let span = spans
        .iter()
        .find(|&&(lo, hi)| row >= lo && row <= hi)
        .copied();
    let (up, down) = match span {
        Some((lo, hi)) if lo != hi => (row > lo, row < hi),
        _ => (false, false),
    };
    let junction = match (up, down, is_source, is_target) {
        (true, true, true, true) => g.cross,
        (true, true, true, false) => g.tee_l,
        (true, true, false, true) => g.tee_r,
        (true, true, false, false) => g.v,
        (false, true, true, true) => g.tee_down,
        (false, true, true, false) => g.top_l,
        (false, true, false, true) => g.top_r,
        (true, false, true, true) => g.tee_up,
        (true, false, true, false) => g.bot_l,
        (true, false, false, true) => g.bot_r,
        (false, false, true, true) => g.h,
        _ => return " ".repeat(6),
    };
    let left = if is_source { g.h } else { ' ' };
    let (right, head) = if is_target {
        (g.h, g.arrow)
    } else {
        (' ', ' ')
    };
    // The pulse: this row's target node is RUNNING — the last rail
    // segment + arrowhead cycle density, painted Accent (energy in
    // motion); the cold part of the cell stays one Dim chrome run.
    if let (Some(tick), true) = (pulse_tick, is_target) {
        let frames = if theme.ascii { PULSE_ASCII } else { PULSE };
        let cold: String = [' ', left, junction].iter().collect();
        let warm: String = [frames[tick % frames.len()], g.arrow, ' '].iter().collect();
        return format!(
            "{}{}",
            theme.paint(Role::Dim, &cold),
            theme.paint(Role::Accent, &warm)
        );
    }
    let raw: String = [' ', left, junction, right, head, ' '].iter().collect();
    theme.paint(Role::Dim, &raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verbs::graph::project;
    use crate::verbs::load_checked;

    /// Unique fixture path per CALL — tests run on parallel threads and
    /// two same-length fixtures once shared a `{pid}-{len}` path (the
    /// crossing test read the parallel test's file · a live flake).
    fn fixture(yaml: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "nika-wires-{}-{}.nika.yaml",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::write(&path, yaml).expect("fixture");
        path
    }

    fn draw(yaml: &str) -> Option<String> {
        let path = fixture(yaml);
        let (wf, report) = load_checked(path.to_str().expect("utf8")).expect("checks");
        std::fs::remove_file(&path).ok();
        let doc = project(&wf, &report);
        render(&doc, &report.waves, Theme::new(false, false, false))
    }

    fn exec_task(id: &str, deps: &[&str]) -> String {
        let deps = if deps.is_empty() {
            String::new()
        } else {
            format!("\n    depends_on: [{}]", deps.join(", "))
        };
        format!("  - id: {id}{deps}\n    exec: {{ command: \"echo x\" }}\n")
    }

    fn wf(tasks: &[String]) -> String {
        format!("nika: v1\nworkflow: wires\ntasks:\n{}", tasks.concat())
    }

    #[test]
    fn honest_gutter_verdicts_directly() {
        // The crossing: two 1×1 components whose spans overlap → refuse.
        assert!(honest_gutter(&vec![(1, 0), (0, 1)]).is_none());
        // Disjoint parallels: two 1×1 components, disjoint spans → two spans.
        assert_eq!(
            honest_gutter(&vec![(0, 0), (1, 1)]),
            Some(vec![(0, 0), (1, 1)])
        );
        // A fan: one complete 1×3 component.
        assert_eq!(
            honest_gutter(&vec![(0, 0), (0, 1), (0, 2)]),
            Some(vec![(0, 2)])
        );
        // A partial fan (2 sources × 2 targets · 3 edges) → refuse.
        assert!(honest_gutter(&vec![(0, 0), (0, 1), (1, 1)]).is_none());
    }

    /// The incoming rail's last segment cycles while its target runs —
    /// and a still map is byte-stable under ticks (no idle flicker).
    #[test]
    fn the_incoming_edge_pulses_into_the_running_node() {
        let path = fixture(&wf(&[exec_task("a", &[]), exec_task("b", &["a"])]));
        let (wf_, report) = load_checked(&path.to_string_lossy()).expect("checked");
        let doc = project(&wf_, &report);
        let theme = Theme::new(false, false, false);
        let running: std::collections::BTreeSet<String> = ["b".to_owned()].into();
        let node = |id: &str, verb: &str| (theme.verb_glyph(verb), id.to_owned());

        let t0 = render_with(&doc, &report.waves, theme, &node, Some((&running, 0))).expect("art");
        let t1 = render_with(&doc, &report.waves, theme, &node, Some((&running, 1))).expect("art");
        assert!(
            t0.contains('╍') && t1.contains('╌'),
            "the pulse cycles: {t0} | {t1}"
        );
        assert_ne!(t0, t1, "two ticks · two frames");

        // Nothing running → the cold rail, byte-stable under ticks.
        let cold0 = render_with(
            &doc,
            &report.waves,
            theme,
            &node,
            Some((&std::collections::BTreeSet::new(), 0)),
        );
        let cold9 = render_with(
            &doc,
            &report.waves,
            theme,
            &node,
            Some((&std::collections::BTreeSet::new(), 9)),
        );
        assert_eq!(cold0, cold9, "a still map never flickers");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_chain_is_one_flat_line() {
        let art = draw(&wf(&[
            exec_task("gather", &[]),
            exec_task("think", &["gather"]),
            exec_task("persist", &["think"]),
        ]))
        .expect("chain draws");
        assert_eq!(art, "  ▷ gather ───▶ ▷ think ───▶ ▷ persist");
    }

    #[test]
    fn a_diamond_draws_fan_and_join_truthfully() {
        let art = draw(&wf(&[
            exec_task("root", &[]),
            exec_task("left", &["root"]),
            exec_task("right", &["root"]),
            exec_task("join", &["left", "right"]),
        ]))
        .expect("diamond draws");
        let expect = concat!(
            "  ▷ root ─┬─▶ ▷ left  ─┬─▶ ▷ join\n",
            "          ╰─▶ ▷ right ─╯",
        );
        assert_eq!(art, expect, "\ngot:\n{art}\nwant:\n{expect}");
    }

    #[test]
    fn a_fan_out_of_three_rails_down() {
        let art = draw(&wf(&[
            exec_task("root", &[]),
            exec_task("a", &["root"]),
            exec_task("b", &["root"]),
            exec_task("c", &["root"]),
        ]))
        .expect("fan draws");
        let expect = concat!(
            "  ▷ root ─┬─▶ ▷ a\n",
            "          ├─▶ ▷ b\n",
            "          ╰─▶ ▷ c",
        );
        assert_eq!(art, expect, "\ngot:\n{art}\nwant:\n{expect}");
    }

    #[test]
    fn parallel_independent_chains_stay_disjoint() {
        // Two 1×1 components with disjoint spans — two flat rows, no
        // rail invented between them.
        let art = draw(&wf(&[
            exec_task("a", &[]),
            exec_task("b", &[]),
            exec_task("x", &["a"]),
            exec_task("y", &["b"]),
        ]))
        .expect("parallel draws");
        let expect = concat!("  ▷ a ───▶ ▷ x\n", "  ▷ b ───▶ ▷ y");
        assert_eq!(art, expect, "\ngot:\n{art}\nwant:\n{expect}");
    }

    #[test]
    fn the_projector_uncrosses_and_the_drawing_stays_truthful() {
        // In SOURCE order a→y and b→x cross — but the projector places
        // wave members under their upstreams (y rides a's row, x rides
        // b's), so the projected space has NO crossing and the drawing
        // is two truthful flat wires. The refusal guarantee for an
        // IRREDUCIBLE crossing is pinned directly on `honest_gutter`
        // (see `honest_gutter_verdicts_directly`).
        let art = draw(&wf(&[
            exec_task("a", &[]),
            exec_task("b", &[]),
            exec_task("x", &["b"]),
            exec_task("y", &["a"]),
        ]))
        .expect("the uncrossed layout draws");
        assert_eq!(art, concat!("  ▷ a ───▶ ▷ y\n", "  ▷ b ───▶ ▷ x"));
    }

    #[test]
    fn a_partial_fan_refuses_to_lie() {
        // sources {a,b} · targets {x,y} · edges a→x, a→y, b→y (NOT b→x):
        // not complete bipartite — the rail would claim b→x too.
        let art = draw(&wf(&[
            exec_task("a", &[]),
            exec_task("b", &[]),
            exec_task("x", &["a"]),
            exec_task("y", &["a", "b"]),
        ]));
        assert!(art.is_none(), "a partial fan must not draw: {art:?}");
    }

    #[test]
    fn a_wave_skipping_edge_refuses_to_lie() {
        // root→join skips the middle wave — no truthful single-gutter
        // wire exists for it.
        let art = draw(&wf(&[
            exec_task("root", &[]),
            exec_task("mid", &["root"]),
            exec_task("join", &["mid", "root"]),
        ]));
        assert!(art.is_none(), "a skip edge must not draw: {art:?}");
    }

    #[test]
    fn ascii_theme_swaps_every_wire_glyph() {
        let path = fixture(&wf(&[
            exec_task("root", &[]),
            exec_task("left", &["root"]),
            exec_task("right", &["root"]),
            exec_task("join", &["left", "right"]),
        ]));
        let (wf_parsed, report) = load_checked(path.to_str().expect("utf8")).expect("checks");
        std::fs::remove_file(&path).ok();
        let doc = project(&wf_parsed, &report);
        let art = render(&doc, &report.waves, Theme::new(false, true, false)).expect("draws");
        for glyph in ['─', '│', '▶', '├', '┤', '╭', '╮', '╰', '╯', '┬', '┴', '┼']
        {
            assert!(
                !art.contains(glyph),
                "unicode {glyph} leaked into --ascii:\n{art}"
            );
        }
        assert!(art.contains('>'), "{art}");
    }

    #[test]
    fn every_embedded_example_draws_or_declines_within_width() {
        // The pack property: each embedded example either renders inside
        // the 78-cell budget or honestly declines — never a wide or
        // wrong picture. At least the plain chain MUST draw (the shape a
        // stranger meets first).
        let mut drew = 0usize;
        for slug in nika_pack::example_slugs() {
            let Some(body) = nika_pack::example(&slug) else {
                continue;
            };
            let path = std::env::temp_dir().join(format!(
                "nika-wires-ex-{}-{}.nika.yaml",
                std::process::id(),
                slug.replace('/', "-")
            ));
            std::fs::write(&path, body).expect("fixture");
            let Ok((wf_parsed, report)) = load_checked(path.to_str().expect("utf8")) else {
                std::fs::remove_file(&path).ok();
                continue;
            };
            std::fs::remove_file(&path).ok();
            if !report.conformance.is_empty() {
                continue;
            }
            let doc = project(&wf_parsed, &report);
            if let Some(art) = render(&doc, &report.waves, Theme::new(false, false, false)) {
                drew += 1;
                for line in art.lines() {
                    assert!(
                        line.chars().count() <= MAX_CELLS,
                        "{slug} exceeds the width budget: `{line}`"
                    );
                }
            }
        }
        assert!(drew >= 3, "at least the simple shapes draw ({drew} drew)");
    }
}
