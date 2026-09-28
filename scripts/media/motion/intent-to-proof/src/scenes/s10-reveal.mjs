// S10 · REVEAL — the whole architecture as one coherent system.
//
// The camera pulls back out of the receipt: every stage the intent passed
// through is one instrument. Intelligence above the boundary proposes;
// Rust proves; the human gate authorizes; the runtime below executes. A
// pulse retraces the journey, then everything collapses to a single point.
import { C, E, lerp, seg, smooth } from '../engine/core.mjs';
import { text, line, poly, circle, rrect, rect, measure, light, bezierPts } from '../engine/render.mjs';
import { camera, project } from '../engine/cam.mjs';
import { T } from '../timeline.mjs';

const TOP = 330, BOUND = 560, BOT = 790;
// id, label, x, y, kind
export const MAP = [
  ['user', 'YOU', 120, TOP, 'human'],
  ['session', 'SESSION', 285, TOP, 'core'],
  ['reader', 'INTENT READER', 450, TOP, 'core'],
  ['world', 'OBSERVED WORLD', 615, TOP, 'core'],
  ['plan', 'SEMANTIC PLAN', 800, TOP, 'hero'],
  ['proofs', 'RUST PROOFS', 985, TOP, 'proof'],
  ['lower', 'LOWERING', 1150, TOP, 'core'],
  ['nika', '.nika', 1300, TOP, 'core'],
  ['check', 'NIKA CHECK', 1445, TOP, 'proof'],
  ['meaning', 'MEANING', 1600, TOP, 'proof'],
  ['ready', 'READY', 1765, TOP, 'proof'],
  ['review', 'REVIEW', 1765, 450, 'human'],
  ['consent', 'CONSENT', 1765, BOUND, 'gate'],
  ['runtime', 'RUNTIME', 1765, BOT, 'exec'],
  ['effect', 'EFFECT', 1400, BOT, 'exec'],
  ['result', 'RESULT + PROOF', 1000, BOT, 'result'],
];
const SATS = [
  ['FOUNDRY', 590, 168], ['REFLEX', 720, 142], ['CLM', 850, 142], ['JEV · DECISIONSEAT', 1000, 168],
];
const ASK = [902, 452]; // centered in the gap between the PLAN and PROOFS labels

function node(id) {
  return MAP.find(n => n[0] === id);
}

export function camAt(t) {
  const p = E.inOutCubic(seg(t, T.reveal, T.wide));
  const r = node('result');
  const zNear = 40, zFar = 1540 + 90 * seg(t, T.wide, T.title);
  const z = -Math.exp(lerp(Math.log(zNear), Math.log(zFar), p));
  const ex = lerp(r[2], 944, p), ey = lerp(r[3], 440, p);
  const tx = lerp(r[2], 950, p), ty = lerp(r[3], 488, p);
  return camera([ex, ey, z], [tx, ty, 0]);
}

// Screen rect of the RESULT node (for the result scene's handoff).
export function resultNode(t) {
  const cam = camAt(t);
  const r = node('result');
  const p = project(cam, [r[2], r[3], 0]);
  // the full-frame composition shrinks to about a 150-unit-wide card
  return { x: p.x, y: p.y, s: Math.max(0.0001, (150 * p.s) / 1920) };
}

const P = (cam, x, y) => project(cam, [x, y, 0]);

// collapse: everything rushes to the center point before the title
function collapse(t) {
  return E.inExpo(seg(t, T.collapse, T.title));
}
function cpos(p, c) {
  return [lerp(p.x, 960, c), lerp(p.y, 540, c)];
}

export function env(t) {
  return { bgGlow: 1.2, gridAlpha: 0.3 * (1 - collapse(t)), gridY: 0, gridX: 0, bgY: 520, railLift: smooth(T.reveal + 0.2, T.reveal + 0.6, t) };
}

export function draw(R, t) {
  const cam = camAt(t);
  const on = smooth(T.reveal + 0.1, T.reveal + 0.6, t);
  if (on <= 0) return;
  const c = collapse(t);
  const a = on * (1 - smooth(T.title - 0.05, T.title + 0.05, t));
  if (a <= 0) return;
  const pos = id => {
    const n = node(id);
    const p = P(cam, n[2], n[3]);
    const [x, y] = cpos(p, c);
    return { x, y, s: p.s * (1 - c * 0.8) };
  };

  // boundary
  const b0 = P(cam, 60, BOUND), b1 = P(cam, 1860, BOUND);
  const [bx0, by0] = cpos(b0, c), [bx1, by1] = cpos(b1, c);
  line(R, bx0, by0, bx1, by1, { color: C.ice, w: 1, alpha: a * 0.55 });
  text(R, 'UNDERSTAND · PROVE · AUTHORIZE', bx0 + 6, by0 - 12 * b0.s, { f: 'MGW 500', size: 10 * b0.s, tracking: 3, color: C.dim, alpha: a * (1 - c) });
  text(R, 'DETERMINISTIC EXECUTION', bx0 + 6, by0 + 22 * b0.s, { f: 'MGW 500', size: 10 * b0.s, tracking: 3, color: C.ice, alpha: a * (1 - c) });

  // satellites feed the plan (dashed): they propose, never authorize
  const pl = pos('plan');
  SATS.forEach(([lab, x, y], i) => {
    const p = P(cam, x, y);
    const [sx, sy] = cpos(p, c);
    line(R, sx, sy + 8 * p.s, pl.x, pl.y - 18 * pl.s, { color: C.cyan, w: 0.9, alpha: a * 0.5, dash: [3, 4], dashOffset: -t * 20 });
    circle(R, sx, sy, 5 * p.s, { color: C.cyan, w: 1.2, alpha: a, fill: C.bg0, glow: 0.5 });
    text(R, lab, sx, sy - 15 * p.s, { f: 'MGW 500', size: 11 * p.s, tracking: 2.2, color: C.cyan, alpha: a * (1 - c), align: 'center' });
  });

  // edges along the main path
  for (let i = 0; i < MAP.length - 1; i++) {
    const p = pos(MAP[i][0]), q = pos(MAP[i + 1][0]);
    line(R, p.x, p.y, q.x, q.y, { color: C.ice, w: 1.1, alpha: a * 0.5 });
  }
  // the unknown loop: a detour between plan and proofs, down to the human
  // and back, threaded through the gap between their labels
  const [qx, qy] = ASK;
  const loop = bezierPts([qx + 26, TOP + 1], [qx + 29, TOP + 62], [qx + 24, qy], [qx, qy], 24)
    .concat(bezierPts([qx, qy], [qx - 24, qy], [qx - 29, TOP + 62], [qx - 26, TOP + 1], 24))
    .map(([x, y]) => cpos(P(cam, x, y), c));
  poly(R, loop, { color: C.amber, w: 1.3, alpha: a * 0.85, glow: 0.6 });
  const ak = P(cam, qx, qy);
  const [ax, ay] = cpos(ak, c);
  circle(R, ax, ay, 4.5 * ak.s, { color: C.amber, w: 1.2, alpha: a, fill: C.bg0, glow: 0.6 });
  text(R, 'UNKNOWN → ASK', ax, ay + 24 * ak.s, { f: 'MGW 500', size: 9.5 * ak.s, tracking: 2.4, color: C.amber, alpha: a * (1 - c), align: 'center' });

  // the pulse retraces the journey
  const pp = seg(t, T.reveal + 0.35, T.title - 0.35);
  const idx = pp * (MAP.length - 1);
  MAP.forEach((n, i) => {
    const p = pos(n[0]);
    const hit = smooth(i - 0.2, i + 0.1, idx);
    const kind = n[4];
    const col = kind === 'human' || kind === 'gate' ? C.human : kind === 'proof' ? C.teal : kind === 'exec' ? C.ice : kind === 'result' ? C.teal : C.ice;
    const r = (kind === 'hero' ? 14 : kind === 'result' ? 13 : 8.5) * p.s;
    if (kind === 'gate') {
      rect(R, p.x - 7 * p.s, p.y - 16 * p.s, 3 * p.s, 32 * p.s, { fill: C.human, alpha: a });
      rect(R, p.x + 4 * p.s, p.y - 16 * p.s, 3 * p.s, 32 * p.s, { fill: C.human, alpha: a });
    } else if (kind === 'exec') {
      rrect(R, p.x - r, p.y - r, r * 2, r * 2, 2 * p.s, { color: col, w: 1.2, alpha: a, fill: C.bg0, glow: 0.4 * hit });
    } else {
      circle(R, p.x, p.y, r, { color: col, w: 1.3, alpha: a, fill: C.bg0, glow: 0.4 + 0.6 * hit });
      if (kind === 'hero') circle(R, p.x, p.y, r * 1.8, { color: col, w: 0.8, alpha: a * 0.6 });
    }
    if (hit > 0) circle(R, p.x, p.y, r * 0.5, { fill: col, alpha: a * hit, glow: 1 });
    const lx = n[0] === 'review' || n[0] === 'consent' ? p.x - 22 * p.s : p.x;
    // the top row is dense: two-word labels stack so neighbours never touch
    const size = kind === 'hero' ? 13.5 : 11.5;
    const words = n[3] === TOP ? n[1].split(' ') : [n[1]];
    words.forEach((w, j) => {
      text(R, w, lx, p.y + (32 + j * size * 1.3) * p.s, { f: 'MGW 500', size: size * p.s, tracking: 2.2, color: kind === 'hero' ? C.ink : col, alpha: a * (1 - c) * lerp(0.75, 1, hit), align: n[0] === 'review' || n[0] === 'consent' ? 'right' : 'center', glow: 0.2 * hit });
    });
  });
  if (pp > 0 && pp < 1) {
    const i = Math.min(MAP.length - 2, Math.floor(idx));
    const p = pos(MAP[i][0]), q = pos(MAP[i + 1][0]);
    const u = idx - i;
    const x = lerp(p.x, q.x, u), y = lerp(p.y, q.y, u);
    circle(R, x, y, 4 * p.s, { fill: C.ink, alpha: a, glow: 1 });
    light(R, x, y, 90 * p.s, C.cyan, 0.5 * a, 1);
  }

  // the four principles, beside the regions they govern
  const PR = [
    ['Intelligence', 'proposes.', C.cyan, 795, 88, 'center'],
    ['Rust', 'proves.', C.teal, 1300, 432, 'center'],
    ['Humans', 'authorize.', C.human, 1700, 642, 'right'],
    ['Nika', 'executes.', C.ice, 66, 705, 'left'], // heads the execution band
  ];
  PR.forEach(([w1, w2, col, x, y, al], i) => {
    const tp = T.principles[i];
    const k = E.snap(seg(t, tp, tp + 0.4));
    if (k <= 0) return;
    const p = P(cam, x, y);
    const [px, py] = cpos(p, c);
    const st = { f: 'Geist 600', size: 38 * p.s, tracking: -0.8 * p.s };
    const w = measure(`${w1} ${w2}`, { f: 'Geist 600', size: 38, tracking: -0.8 }) * p.s;
    const x0 = al === 'center' ? px - w / 2 : al === 'right' ? px - w : px;
    const aa = a * k * (1 - c);
    text(R, w1, x0, py + 10 * (1 - k), { ...st, color: C.ink, alpha: aa, glow: 0.2 });
    text(R, w2, x0 + measure(`${w1} `, { f: 'Geist 600', size: 38, tracking: -0.8 }) * p.s, py + 10 * (1 - k), { ...st, color: col, alpha: aa, glow: 0.45 });
  });

  // the point everything collapses into (the caret of the first frame)
  if (c > 0) light(R, 960, 540, 60 + 260 * c, C.ice, 0.8 * c, 1);
}
