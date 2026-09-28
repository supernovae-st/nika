// dag-execution · "A workflow is a graph."
// pr-review-fanout from the embedded pack. The topology is `nika inspect
// --format mermaid` (media/raw/graph-fanout.mmd: nodes, edges and the verb
// colours it assigns); the waves and the parallelism are `nika check`'s
// PLAN (media/raw/check-fanout.txt). Both are read here, never typed. The
// waves lighting in order illustrate that plan; this clip runs nothing.
import { C, E, seg, smooth, repoLines, NIKA_VERSION, terminal, frame, headline, loopFade, panel, cameraPath, frameBox, titleFade, WIDE } from './kit.mjs';
import { text, rrect, bezierPts, poly, circle } from '../src/engine/render.mjs';
import { mono, wrapSpans } from './kit.mjs';

export const meta = { duration: 15.9, poster: 14.6 };

const mmd = repoLines('media/raw/graph-fanout.mmd');
const check = repoLines('media/raw/check-fanout.txt');
// what each verb does, as the README's building-blocks table says it
const VERB_DOC = Object.fromEntries(repoLines('README.md').map(l => l.match(/^\| `(\w+)` \| (.+) \|$/)).filter(Boolean).map(m => [m[1], m[2]]));

// ── the graph, from nika inspect ────────────────────────────────────────
const COLORS = {};
for (const l of mmd) {
  const m = l.match(/^\s*classDef (\w+) .*stroke:(#[0-9a-fA-F]{6})/);
  if (m) COLORS[m[1]] = m[2];
}
const NODES = {};
for (const l of mmd) {
  const m = l.match(/^\s*(\w+)\["([^"]+)"\]:::(\w+)/);
  // the node's second line: the tool an invoke calls, else the verb
  if (m) NODES[m[1]] = { id: m[1], label: m[2], verb: m[3] === 'invoke' ? m[2].split(' · ')[2] ?? 'invoke' : m[3], cls: m[3] };
}
const EDGES = [...new Set(mmd.map(l => l.match(/^\s*(\w+) --> (\w+)/)).filter(Boolean).map(m => `${m[1]}>${m[2]}`))].map(k => k.split('>'));

// ── the waves, from nika check ──────────────────────────────────────────
const PLAN = check.find(l => /^ ✔ PLAN/u.test(l));
const MAXPAR = +(PLAN.match(/max parallelism (\d+)/)?.[1] ?? 1);
const WAVES = check.filter(l => /^\s+wave \d+ /.test(l)).map(l => l.replace(/^\s+wave \d+ /, '').split(' · ').map(p => p.split(' ')[0]).filter(id => NODES[id]));
if (!WAVES.length || WAVES.flat().length !== Object.keys(NODES).length) throw new Error('the check PLAN and the inspect graph disagree');
const waveOf = id => WAVES.findIndex(w => w.includes(id));
const planShown = check.slice(0, check.findIndex(l => /^ ✔ MODELS/u.test(l))).slice(1);

const T = {
  graph: 0.4, term: 0.6,
  cmd: 2.3, out: 3.5,
  wave0: 7.0, every: 0.75,
};
const TERM = { f: 'MM 400', size: 16 };
const GRAPH = { x: 64, y: 244, w: 1792, h: 470 };
const TERMBOX = { x: 64, y: 738, w: 1000, h: 286 };
const LEGEND = { x: 1096, y: 738, w: 760, h: 286 };

// layout: one column per wave, the main chain on one row, a wave's second
// task below it
const colX = i => GRAPH.x + 150 + i * ((GRAPH.w - 300) / (WAVES.length - 1));
const ROW = [GRAPH.y + 206, GRAPH.y + 366];
const POS = {};
WAVES.forEach((w, i) => w.forEach((id, j) => { POS[id] = { x: colX(i), y: ROW[Math.min(j, 1)] }; }));
const NODE_W = 214, NODE_H = 78;

// The graph fills the frame's width at 1x: the plan is read pushed in,
// the waves play in the wide frame, which is also the end.
const SHOTS = [
  { at: 0, cam: WIDE },
  { at: 2.6, cam: frameBox(TERMBOX, 16), move: 0.7 },
  { at: T.wave0, cam: WIDE, move: 0.8 },
];
export const camera = t => cameraPath(t, SHOTS);

export function env() {
  return { bgGlow: 1, gridAlpha: 0.22, gridX: 0, gridY: 0, bgY: 560 };
}

export function chrome(R, t) {
  const a = loopFade(t, meta.duration);
  const cam = camera(t);
  frame(R, t, { kicker: 'the execution model · pr-review-fanout, from the embedded pack', plate: `graph: nika inspect · waves: nika check · nika ${NIKA_VERSION} · the lighting illustrates the plan`, alpha: a, scrim: smooth(1, 1.12, cam.s) });
  headline({ ...R, fade: a * titleFade(cam) }, t, 0.2, meta.duration + 1, 'A workflow is a graph.', 'WAVES PLANNED BEFORE A TASK RUNS · INDEPENDENT STEPS IN PARALLEL', { accent: 'a graph.', accentColor: C.teal });
}

const waveT = i => T.wave0 + 0.3 + i * T.every;

function node(R, t, id, a) {
  const n = NODES[id], p = POS[id], col = COLORS[n.cls] || C.mist;
  const lit = smooth(waveT(waveOf(id)), waveT(waveOf(id)) + 0.25, t);
  const x = p.x - NODE_W / 2, y = p.y - NODE_H / 2;
  // a for_each agent fans out: its instances stack behind it
  if (n.cls === 'agent') {
    for (let k = 2; k >= 1; k--) rrect(R, x + k * 9, y + k * 9, NODE_W, NODE_H, 12, { color: col, w: 1, alpha: a * (0.35 + 0.3 * lit) / k, fill: '#050c19', fillAlpha: 0.95 });
  }
  rrect(R, x, y, NODE_W, NODE_H, 12, { color: lit > 0 ? col : C.faint, w: 1.4 + lit * 0.6, alpha: a, fill: col, fillAlpha: 0.04 + lit * 0.1, glow: lit * 0.6 });
  rrect(R, x, y, NODE_W, NODE_H, 12, { fill: '#050c19', fillAlpha: 0.55, alpha: a });
  text(R, id, x + 20, y + 34, { f: 'MM 500', size: 24, color: C.ink, alpha: a });
  text(R, n.verb.toUpperCase(), x + 20, y + 60, { f: 'MGW 500', size: 12.5, tracking: 2, color: col, alpha: a * (0.75 + 0.25 * lit) });
  if (n.cls === 'agent') text(R, '× N · ONE PER CHANGED FILE', x + NODE_W / 2 + 9, y + NODE_H + 42, { f: 'MGW 500', size: 12.5, tracking: 2, color: col, alpha: a * 0.9, align: 'center' });
}

function edge(R, t, from, to, a) {
  const p0 = POS[from], p1 = POS[to];
  const x0 = p0.x + NODE_W / 2, x1 = p1.x - NODE_W / 2;
  const far = waveOf(to) - waveOf(from) > 1 && p0.y === p1.y;
  const lift = far ? -128 : 0;
  const pts = bezierPts([x0, p0.y], [x0 + 70, p0.y + lift], [x1 - 70, p1.y + lift], [x1, p1.y], 36);
  const col = COLORS[NODES[to].cls] || C.mist;
  const tw = waveT(waveOf(to));
  const lit = smooth(tw - 0.2, tw + 0.1, t);
  poly(R, pts, { color: lit > 0 ? col : C.faint, w: 1.3 + lit * 0.5, alpha: a * (0.55 + 0.45 * lit), glow: lit * 0.4 });
  // the hand-off: a pulse travels the edge into the wave that starts
  const k = seg(t, tw - 0.35, tw);
  if (k > 0 && k < 1) {
    const q = pts[Math.min(pts.length - 1, Math.floor(E.inOutCubic(k) * (pts.length - 1)))];
    circle(R, q[0], q[1], 4, { fill: col, alpha: a, glow: 1 });
  }
}

export function draw(R, t) {
  const a = loopFade(t, meta.duration);
  const gk = E.snap(seg(t, T.graph, T.graph + 0.5));
  panel(R, GRAPH, { title: 'pr-review-fanout.nika · nika inspect', alpha: a, k: gk });
  if (gk >= 1) {
    const ga = a * smooth(T.graph + 0.3, T.graph + 0.8, t);
    for (const [f, to] of EDGES) edge(R, t, f, to, ga);
    for (const id of Object.keys(NODES)) node(R, t, id, ga);
    // the wave counter, and the one wave that runs two tasks at once
    const cur = WAVES.findIndex((_, i) => t < waveT(i + 1));
    const wi = cur < 0 ? WAVES.length - 1 : cur;
    const on = smooth(T.wave0, T.wave0 + 0.3, t) * ga;
    if (on > 0) {
      const w = WAVES[wi];
      // the wave counter: large, bottom right of the graph
      const wx = GRAPH.x + GRAPH.w - 36, wy = GRAPH.y + GRAPH.h - 34;
      text(R, `wave ${wi + 1} / ${WAVES.length}`, wx, wy, { f: 'Geist 600', size: 30, color: w.length > 1 ? C.teal : C.ice, alpha: on, align: 'right', glow: 0.25 });
      if (w.length > 1) text(R, `${w.length} TASKS AT ONCE · MAX PARALLELISM ${MAXPAR}`, wx, wy - 40, { f: 'MGW 500', size: 12.5, tracking: 2.5, color: C.teal, alpha: on * smooth(waveT(wi), waveT(wi) + 0.3, t), align: 'right' });
      if (w.length > 1 && t < waveT(wi + 1)) {
        // the tasks of one wave, bracketed: they start together
        const x = POS[w[0]].x + NODE_W / 2 + 18;
        poly(R, [[x, ROW[0] - 30], [x + 10, ROW[0] - 30], [x + 10, ROW[1] + 30], [x, ROW[1] + 30]], { color: C.teal, w: 1.6, alpha: on * smooth(waveT(wi), waveT(wi) + 0.3, t), glow: 0.6 });
      }
    }
  }

  terminal(R, t, TERMBOX, [
    { t: T.cmd, cmd: 'nika check pr-review-fanout.nika', dur: 0.95 },
    { t: T.out, out: planShown, every: 0.08, marks: [{ re: /\d+ waves · \d+ tasks · max parallelism \d+/, c: C.teal, glow: 0.5 }] },
  ], { title: 'terminal', alpha: a, k: E.snap(seg(t, T.term, T.term + 0.5)), st: TERM, lh: 22 });

  // the verbs, in the colours nika inspect gives them
  const lk = E.snap(seg(t, T.term + 0.15, T.term + 0.65));
  panel(R, LEGEND, { title: 'the four verbs · colours from nika inspect', alpha: a, k: lk });
  if (lk >= 1) {
    const DOC = { f: 'MM 400', size: 15 };
    Object.entries(COLORS).forEach(([cls, col], i) => {
      const y = LEGEND.y + 84 + i * 48;
      rrect(R, LEGEND.x + 28, y - 16, 26, 22, 6, { color: col, w: 1.4, alpha: a, fill: col, fillAlpha: 0.15 });
      text(R, cls, LEGEND.x + 70, y, { f: 'MM 500', size: 18, color: C.ink, alpha: a });
      wrapSpans([{ s: VERB_DOC[cls] ?? '', c: C.mist }], 48).slice(0, 2).forEach((row, r) => mono(R, row, LEGEND.x + 170, y + r * 20, { alpha: a, st: DOC }));
    });
  }
}
