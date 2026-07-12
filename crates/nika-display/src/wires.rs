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

use crate::theme::{Role, Theme};

/// Widest a drawing may grow (display cells) — beyond this the caller's
/// listing fallback reads better than horizontal scrolling.
const MAX_CELLS: usize = 78;

/// Tallest a wave may stack before the art stops being a glance.
const MAX_ROWS: usize = 4;

/// One gutter's edge set, as row indices (source row → target rows).
type GutterEdges = Vec<(usize, usize)>;

/// The live probe — « is this node RUNNING right now? ». A borrowed
/// closure instead of a materialized set: the caller already holds the
/// state, and a 10 Hz repaint should not allocate to ask a question.
pub type LiveProbe<'a> = &'a dyn Fn(&str) -> bool;

/// The decoupled wire topology — what the drawing needs and nothing
/// more: (id · verb) per wave slot, and id→id edges. The CLI builds it
/// from its checked projection; any surface with waves + deps can.
pub struct WireGraph {
    /// (task id · verb) per wave, wave-major (projection order).
    pub waves: Vec<Vec<(String, String)>>,
    /// `depends_on` edges (from · to) by id.
    pub edges: Vec<(String, String)>,
}

/// Render the wave-column wiring for a projected graph, or `None` when
/// the one-rail-per-gutter drawing could not be truthful (the caller
/// falls back — never a wrong picture).
#[must_use]
pub fn render(graph: &WireGraph, theme: Theme) -> Option<String> {
    render_with(
        graph,
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
#[must_use]
pub fn render_with(
    graph: &WireGraph,
    theme: Theme,
    node: &dyn Fn(&str, &str) -> (String, String),
    live: Option<(LiveProbe<'_>, usize)>,
) -> Option<String> {
    let waves = &graph.waves;
    if waves.is_empty() || waves.iter().any(|w| w.is_empty() || w.len() > MAX_ROWS) {
        return None;
    }
    // Node id → (wave, row) via the projection order law (node order IS
    // wave order — the same slicing inspect trusts). BTreeMap per the
    // workspace determinism policy (HashMap iteration order is banned).
    let mut coords = std::collections::BTreeMap::new();
    for (w, wave) in waves.iter().enumerate() {
        for (r, (id, _)) in wave.iter().enumerate() {
            coords.insert(id.as_str(), (w, r));
        }
    }
    // Gutter edge sets — refuse a drawing any edge would falsify.
    let mut gutters: Vec<GutterEdges> = vec![Vec::new(); waves.len().saturating_sub(1)];
    for (from, to) in &graph.edges {
        let &(fw, fr) = coords.get(from.as_str())?;
        let &(tw, tr) = coords.get(to.as_str())?;
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
    let widths: Vec<usize> = waves
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
        for (w, col) in waves.iter().enumerate() {
            let (id, verb) = col
                .get(row)
                .map_or(("", ""), |(i, v)| (i.as_str(), v.as_str()));
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

/// Which rows of wave w+1 hold a RUNNING node — those gutters' target
/// cells pulse (the incoming edge carries the run's energy).
fn hot_rows(
    coords: &std::collections::BTreeMap<&str, (usize, usize)>,
    gutter_count: usize,
    live: Option<(LiveProbe<'_>, usize)>,
) -> Vec<std::collections::BTreeSet<usize>> {
    let mut hot = vec![std::collections::BTreeSet::new(); gutter_count];
    if let Some((running, _)) = live {
        for (id, &(w, r)) in coords {
            if w > 0 && running(id) {
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

    fn g(waves: &[&[(&str, &str)]], edges: &[(&str, &str)]) -> WireGraph {
        WireGraph {
            waves: waves
                .iter()
                .map(|w| {
                    w.iter()
                        .map(|(i, v)| ((*i).to_owned(), (*v).to_owned()))
                        .collect()
                })
                .collect(),
            edges: edges
                .iter()
                .map(|(f, t)| ((*f).to_owned(), (*t).to_owned()))
                .collect(),
        }
    }
    const PLAIN: Theme = Theme::new(false, false, false);

    /// A chain draws one flat rail; a diamond fans and joins truthfully.
    #[test]
    fn chains_and_diamonds_draw_truthfully() {
        let chain = g(&[&[("a", "exec")], &[("b", "exec")]], &[("a", "b")]);
        let art = render(&chain, PLAIN).expect("drawable");
        assert!(
            art.contains(char::from(0x61))
                && art.contains("───▶")
                && art.contains(char::from(0x62)),
            "{art}"
        );

        let diamond = g(
            &[
                &[("root", "exec")],
                &[("l", "exec"), ("r", "exec")],
                &[("join", "exec")],
            ],
            &[("root", "l"), ("root", "r"), ("l", "join"), ("r", "join")],
        );
        let art = render(&diamond, PLAIN).expect("drawable");
        assert!(art.lines().count() == 2, "two rows: {art}");
    }

    /// A wave-skipping edge refuses the drawing (one rail cannot say it).
    #[test]
    fn wave_skipping_edges_refuse_the_drawing() {
        let skip = g(
            &[&[("a", "exec")], &[("b", "exec")], &[("c", "exec")]],
            &[("a", "b"), ("b", "c"), ("a", "c")],
        );
        assert!(render(&skip, PLAIN).is_none());
    }

    /// The incoming rail's last segment cycles while its target runs —
    /// and a still map is byte-stable under ticks (no idle flicker).
    #[test]
    fn the_incoming_edge_pulses_into_the_running_node() {
        let chain = g(&[&[("a", "exec")], &[("b", "exec")]], &[("a", "b")]);
        let node = |id: &str, verb: &str| (PLAIN.verb_glyph(verb), id.to_owned());
        let running = |id: &str| id == "b";

        let t0 = render_with(&chain, PLAIN, &node, Some((&running, 0))).expect("art");
        let t1 = render_with(&chain, PLAIN, &node, Some((&running, 1))).expect("art");
        assert!(
            t0.contains('╍') && t1.contains('╌'),
            "the pulse cycles: {t0} | {t1}"
        );
        assert_ne!(t0, t1, "two ticks · two frames");

        let cold = |_: &str| false;
        let c0 = render_with(&chain, PLAIN, &node, Some((&cold, 0)));
        let c9 = render_with(&chain, PLAIN, &node, Some((&cold, 9)));
        assert_eq!(c0, c9, "a still map never flickers");
    }
}
