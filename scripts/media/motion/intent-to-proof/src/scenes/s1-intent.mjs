// S1 · INTENT — a human sentence becomes six typed obligations.
//
// The sentence arrives in the human's warm white. The Intent Reader sweeps
// it: meaningful words turn machine-blue, filler dims. Then the meaningful
// words LIFT OFF the sentence (the raw message stays behind, shrinking into
// provenance under the obligations it produced) and snap into an ordered
// operation chain. "Pay only after I approve" crosses over itself: the gate
// must precede the payment, so Nika reorders by meaning, not word order.
import { C, E, clamp, lerp, seg, smooth, ez, win } from '../engine/core.mjs';
import {
  text, line, poly, circle, rect, measure, glyphs, bezierAt, light, passAlpha, metrics, brackets, streak,
} from '../engine/render.mjs';
import { T } from '../timeline.mjs';
import * as observe from './s2-observe.mjs';
import { beatTitle } from './shared.mjs';
import { Path2D } from '@napi-rs/canvas';
import fs from 'node:fs';
import path from 'node:path';
import { ROOT } from '../engine/render.mjs';

// Vector outlines of the big word (Skia will not rasterize text at 90x zoom).
let OUT = null;
function outlines() {
  if (!OUT) {
    const j = JSON.parse(fs.readFileSync(path.join(ROOT, '.cache/glyphs/geist800.json'), 'utf8'));
    OUT = { upem: j.unitsPerEm, g: {} };
    for (const [ch, v] of Object.entries(j.glyphs)) OUT.g[ch] = new Path2D(v.d);
  }
  return OUT;
}
// The big word as vector glyphs in design coordinates (built once).
let WORD = null;
function wordGlyphs(B) {
  if (WORD) return WORD;
  const o = outlines();
  const k = BIG.size / o.upem;
  WORD = [];
  for (const gl of glyphs(BIG_WORD, BIG)) {
    const src = o.g[gl.ch];
    if (!src) continue;
    const gp = new Path2D(src);
    gp.transform({ a: k, b: 0, c: 0, d: -k, e: B.x0 + gl.x, f: BIG_BASE });
    WORD.push({ ch: gl.ch, svg: gp.toSVGString(), b: gp.getBounds() });
  }
  return WORD;
}
const pathCache = new Map();
// Only the glyphs the camera can see, so device coordinates stay inside the
// rasterizer's fixed-point range at any zoom. Returns null when the view is
// entirely inside the period (no mask needed: we are through the window).
function visibleWord(B, zoom, q) {
  const cx = lerp(B.zx, 960, q), cy = lerp(B.zy, 540, q);
  const x0 = B.zx + (0 - cx) / zoom, x1 = B.zx + (1920 - cx) / zoom;
  const y0 = B.zy + (0 - cy) / zoom, y1 = B.zy + (1080 - cy) / zoom;
  const W = wordGlyphs(B);
  const dot = W.find(g => g.ch === '.');
  const [dx0, dy0, dx1, dy1] = dot.b;
  const insetX = (dx1 - dx0) * 0.14, insetY = (dy1 - dy0) * 0.14;
  if (x0 > dx0 + insetX && x1 < dx1 - insetX && y0 > dy0 + insetY && y1 < dy1 - insetY) return null;
  const mx = (x1 - x0) * 0.1, my = (y1 - y0) * 0.1;
  const vis = W.filter(g => g.b[2] > x0 - mx && g.b[0] < x1 + mx && g.b[3] > y0 - my && g.b[1] < y1 + my);
  const key = vis.map(g => g.ch + g.b[0].toFixed(0)).join('|');
  let p = pathCache.get(key);
  if (!p) {
    p = new Path2D(vis.map(g => g.svg).join(' '));
    pathCache.set(key, p);
  }
  return p;
}

const S = { f: 'Geist 400', size: 80, tracking: -1.8 };
const BASE = [412, 506, 600, 694];
let X0 = 250; // centered in getLayout()
const LINES = [
  ['Read', 'my', 'invoices.'],
  ['Ignore', 'rejected', 'ones.'],
  ['Sum', 'by', 'customer.'],
  ['Pay', 'only', 'after', 'I', 'approve.'],
];
const KEY = new Set(['0:0', '0:2', '1:0', '1:1', '2:0', '2:1', '2:2', '3:0', '3:2', '3:4']);

export const ATOMS = [
  { op: 'READ', arg: 'invoices', kind: 'SOURCE', socket: true },
  { op: 'FILTER', arg: 'rejected', kind: 'RULE', socket: true },
  { op: 'GROUP', arg: 'customer', kind: 'KEY', socket: true },
  { op: 'SUM', arg: 'amount', kind: 'AGGREGATE', socket: true },
  { op: 'GATE', arg: 'my approval', kind: 'HUMAN', human: true },
  { op: 'PAY', arg: 'payment', kind: 'EFFECT', socket: true, effect: true },
];
export const atomX = i => 960 + (i - 2.5) * 282;
export const OP = { f: 'Geist 600', size: 54, tracking: -1 };
export const ARG = { f: 'MM 400', size: 17, tracking: 0.2 };
export const KIND = { f: 'MM 400', size: 11.5, tracking: 1.6 };
export const OP_Y = 520, ARG_Y = 560, KIND_Y = 458;

// Which clause each atom came from (provenance) and the lifts.
const CLAUSE_ATOMS = [[0], [1], [2, 3], [4, 5]];
const LIFT = [
  { l: 0, w: 0, atom: 0, role: 'op' },
  { l: 0, w: 2, atom: 0, role: 'arg' },
  { l: 1, w: 0, atom: 1, role: 'op' },
  { l: 1, w: 1, atom: 1, role: 'arg' },
  { l: 2, w: 1, atom: 2, role: 'op' },
  { l: 2, w: 2, atom: 2, role: 'arg' },
  { l: 2, w: 0, atom: 3, role: 'op' },
  { l: 3, w: 2, atom: 4, role: 'op' },
  { l: 3, w: 4, atom: 4, role: 'arg' },
  { l: 3, w: 0, atom: 5, role: 'op' },
];

let layout = null;
function getLayout() {
  if (layout) return layout;
  X0 = Math.round(960 - Math.max(...LINES.map(ws => measure(ws.join(' '), S))) / 2);
  const words = [];
  LINES.forEach((ws, l) => {
    const str = ws.join(' ');
    let pos = 0;
    ws.forEach((w, i) => {
      const x = X0 + (pos ? measure(str.slice(0, pos), S) + S.tracking : 0);
      const wd = measure(w, S);
      words.push({ l, i, w, x, wd, cx: x + wd / 2, y: BASE[l], key: KEY.has(`${l}:${i}`) });
      pos += w.length + 1;
    });
  });
  const lineW = LINES.map(ws => measure(ws.join(' '), S));
  layout = { words, lineW };
  return layout;
}
const wordAt = (l, i) => getLayout().words.find(w => w.l === l && w.i === i);

// Time each word appears.
function tWord(w) {
  return T.lines[w.l] + w.i * 0.06;
}
// Reading head position.
const HEAD = () => [X0 - 40, X0 + 960];
function headX(t) {
  const [a, b] = HEAD();
  return lerp(a, b, E.inOutSine(seg(t, T.readA, T.readB)));
}
function tRead(w) {
  // time the head crosses the word's center
  const [a, b] = HEAD();
  const u = clamp((w.cx - a) / (b - a));
  // invert inOutSine: u = -(cos(pi p)-1)/2 → p = acos(1-2u)/pi
  const p = Math.acos(1 - 2 * u) / Math.PI;
  return lerp(T.readA, T.readB, p);
}

// Provenance target of clause l: small, centered under its atoms.
const CLAUSE_SIZE = 15;
function clauseTarget(l) {
  const ids = CLAUSE_ATOMS[l];
  const cx = (atomX(ids[0]) + atomX(ids[ids.length - 1])) / 2;
  const s = CLAUSE_SIZE / S.size;
  const w = getLayout().lineW[l] * s;
  return { x: cx - w / 2, y: 752, s };
}
function clauseScale(t, l) {
  return E.outCubic(seg(t, T.fracture + 0.02 + l * 0.03, T.fracture + 0.3 + l * 0.03));
}
function clauseProgress(t, l) {
  return E.inOutCubic(seg(t, T.fracture + 0.14 + l * 0.04, T.fracture + 0.52 + l * 0.04));
}

export function env(t) {
  const on = smooth(T.act - 0.2, T.act + 0.6, t);
  const dive = smooth(T.dive, T.dive + 0.3, t);
  return {
    bgGlow: lerp(0.15, 1, on) * (1 - 0.8 * dive),
    gridAlpha: on * (1 - dive),
    gridX: -30 * seg(t, 0, 4),
  };
}

export function draw(R, t) {
  const L = getLayout();
  const diveOut = smooth(T.dive - 0.1, T.dive + 0.25, t); // everything but the dive word recedes
  const recede = 1 - diveOut;

  // ── the caret (before and while typing) ──────────────────────────────
  if (t < T.readA + 0.1) {
    let cx = X0, cy = BASE[0];
    for (const w of L.words) if (t >= tWord(w)) { cx = w.x + w.wd + 10; cy = w.y; }
    const typing = t >= T.lines[0] && t < T.lines[3] + 0.35;
    const blink = typing ? 1 : (Math.floor(t * 2.2) % 2 === 0 ? 1 : 0.15);
    const boot = ez(t, 0, 0.18, E.outExpo);
    const fadeOut = 1 - smooth(T.readA - 0.1, T.readA + 0.1, t);
    rect(R, cx, cy - 58, 3.5, 66 * boot, { fill: C.ice, alpha: blink * fadeOut, glow: 1 });
    if (t < 0.5) light(R, cx + 2, cy - 25, 160 * (1 - ez(t, 0, 0.5, E.outCubic)) + 30, C.ice, 0.18 * (1 - seg(t, 0.1, 0.5)), 1);
    // the ignition: an anamorphic streak through the caret on the first impact
    const ig = 1 - seg(t, 0.02, 0.7);
    if (ig > 0 && t < 0.7) streak(R, cx + 2, cy - 25, lerp(120, 900, E.outCubic(seg(t, 0, 0.35))), 0.9 * ig * ig, C.ice, 1);
  }

  // speaker label (the human)
  const spk = smooth(0.15, 0.5, t) * recede * (1 - smooth(T.fracture, T.fracture + 0.3, t));
  text(R, 'YOU', X0, 322, { f: 'MGW 500', size: 11, tracking: 4, color: C.human, alpha: 0.75 * spk });
  text(R, '09:41', X0 + 52, 322, { f: 'MM 400', size: 11, tracking: 0.5, color: C.dim, alpha: 0.9 * spk });

  // ── the sentence (and its provenance afterlife) ──────────────────────
  for (let l = 0; l < 4; l++) {
    const cp = clauseProgress(t, l);
    const tgt = clauseTarget(l);
    const s = lerp(1, tgt.s, clauseScale(t, l));
    // shrink about the line's center, then slide to the provenance slot
    const lw = L.lineW[l];
    const cx0 = X0 + lw / 2, cy0 = BASE[l] - 28;
    const sx0 = cx0 - (lw * s) / 2, sy0 = cy0 + 28 * s;
    const ox = lerp(sx0, tgt.x, cp), oy = lerp(sy0, tgt.y, cp);
    const ctx = R.ctx;
    ctx.save();
    ctx.translate(ox, oy);
    ctx.scale(s, s);
    for (const w of L.words.filter(q => q.l === l)) {
      const ta = tWord(w);
      const k = E.snap(seg(t, ta, ta + 0.36));
      if (k <= 0) continue;
      const read = smooth(tRead(w), tRead(w) + 0.16, t);
      let color = C.human, alpha = k, glow = 0;
      if (w.key) {
        color = read > 0.5 ? C.ice : C.human;
        glow = 0.5 * read * (1 - cp);
      } else alpha *= lerp(1, 0.32, read);
      if (w.key && t >= T.fracture) alpha *= lerp(0.22, 1, cp);
      // after the fracture the clause is provenance: calm, readable, warm
      if (cp > 0) {
        color = C.human;
        alpha = lerp(alpha, w.key ? 0.9 : 0.62, cp);
        glow = glow * (1 - cp);
      }
      const blur = (1 - k) * 9;
      const dy = (1 - k) * 16;
      text(R, w.w, w.x - X0, dy, { ...S, color, alpha: alpha * recede, glow, blur: blur / Math.max(s, 0.2) });
      // underline under key words, drawn by the reader
      if (w.key && read > 0) {
        const u = E.snap(seg(t, tRead(w), tRead(w) + 0.2));
        const ua = (1 - cp) * recede;
        line(R, w.x - X0, 16, w.x - X0 + w.wd * u, 16, { color: C.ice, w: 2.2, alpha: 0.8 * ua, glow: 0.7 * ua });
      }
    }
    ctx.restore();
  }

  // ── the Intent Reader head ───────────────────────────────────────────
  const hA = win(t, T.readA - 0.05, T.readB + 0.1, 0.08, 0.15);
  if (hA > 0) {
    const hx = headX(t);
    line(R, hx, 330, hx, 730, { color: C.ice, w: 1.2, alpha: 0.85 * hA, glow: 1 });
    rect(R, hx - 18, 330, 36, 400, { fill: C.ice, alpha: 0.035 * hA });
    for (const y of BASE) line(R, hx - 6, y + 16, hx + 6, y + 16, { color: C.ice, w: 1, alpha: 0.9 * hA, glow: 0.6 });
    text(R, 'INTENT READER', hx + 10, 322, { f: 'MGW 500', size: 10, tracking: 3, color: C.ice, alpha: 0.95 * hA, glow: 0.4 });
  }

  // ── lifted words: hover as the sentence falls away, then route ───────
  for (let n = 0; n < LIFT.length; n++) {
    const it = LIFT[n];
    const w = wordAt(it.l, it.w);
    const atom = ATOMS[it.atom];
    const tLift = T.fracture + n * 0.012;
    if (t < tLift) continue;
    const tRoute = T.fracture + 0.1 + it.atom * 0.035 + (it.role === 'arg' ? 0.02 : 0);
    const tLand = T.atoms[it.atom] + (it.role === 'arg' ? 0.06 : 0);
    const hover = E.outCubic(seg(t, tLift, tLift + 0.2));
    const src = [w.cx, w.y - 27 - 22 * hover];
    const tgtStyle = it.role === 'op' ? OP : ARG;
    const tgtText = it.role === 'op' ? atom.op : atom.arg;
    const ty = it.role === 'op' ? OP_Y : ARG_Y;
    const dst = [atomX(it.atom), ty - (it.role === 'op' ? 19 : 6)];
    const p = E.inOutCubic(seg(t, tRoute, tLand));
    // separated arcs: the payment arcs high over the gate (the reorder)
    const arc = it.atom === 5 || it.atom === 3 ? 270 : it.atom === 4 || it.atom === 2 ? 40 : 130;
    const c0 = [lerp(src[0], dst[0], 0.25), Math.min(src[1], dst[1]) - arc];
    const c1 = [lerp(src[0], dst[0], 0.8), dst[1] - arc * 0.55];
    const [x, y] = p <= 0 ? src : p < 1 ? bezierAt(src, c0, c1, dst, p) : dst;
    const m = smooth(0.5, 0.88, p);
    const land = E.snap(seg(t, tLand, tLand + 0.45));
    const tgtCol = it.role === 'arg' ? C.mist : atom.human ? C.human : C.ink;
    const pulse = 0.35 + 0.65 * (1 - seg(t, tLift, tLift + 0.5));
    const ctx = R.ctx;
    if (m < 1) {
      ctx.save();
      ctx.translate(x, y);
      const s = lerp(1, tgtStyle.size / S.size, E.inCubic(p));
      ctx.scale(s, s);
      text(R, w.w, 0, 0, { ...S, color: C.ice, alpha: (1 - m) * recede, align: 'center', baseline: 'middle', glow: 0.55 * pulse, blur: m * 9 });
      ctx.restore();
    }
    if (m > 0) {
      ctx.save();
      ctx.translate(x, y);
      const s = lerp(S.size / tgtStyle.size, 1, E.inCubic(p));
      ctx.scale(s, s);
      const isDiveArg = it.atom === 0 && it.role === 'arg';
      const alpha = m * (isDiveArg ? 1 - smooth(T.dive - 0.1, T.dive + 0.05, t) : recede);
      const glow = it.role === 'op' ? lerp(0.55, 0.22, land) : 0.15;
      text(R, tgtText, 0, 0, { ...tgtStyle, color: tgtCol, alpha, align: 'center', baseline: 'middle', glow, blur: (1 - m) * 7 });
      ctx.restore();
    }
  }

  // inferred arguments (no source word): decode in place
  for (const [i, delay] of [[3, 0.1], [5, 0.1]]) {
    const ta = T.atoms[i] + delay;
    const k = smooth(ta, ta + 0.3, t);
    if (k <= 0) continue;
    const a = ATOMS[i];
    const str = a.arg.slice(0, Math.ceil(a.arg.length * k));
    text(R, str, atomX(i), ARG_Y, { ...ARG, color: C.mist, alpha: k * recede, align: 'center' });
  }

  // atom furniture: kind labels, sockets, landing flashes
  ATOMS.forEach((a, i) => {
    const ta = T.atoms[i];
    const k = smooth(ta, ta + 0.25, t);
    if (k <= 0) return;
    const x = atomX(i);
    const kind = `${String(i + 1).padStart(2, '0')}  ${a.kind}`;
    const shown = kind.slice(0, Math.ceil(kind.length * clamp(k * 1.3)));
    text(R, shown, x, KIND_Y, { ...KIND, color: a.human ? C.human : C.dim, alpha: 0.9 * recede, align: 'center' });
    // precision lock: brackets snap onto the obligation
    const f = seg(t, ta, ta + 0.28);
    if (f > 0) {
      const ow = measure(a.op, OP);
      const grow = lerp(1.7, 1, E.outCubic(f));
      const bw = (ow + 34) * grow, bh = 66 * grow;
      const ba = lerp(0.95, 0.28, seg(t, ta + 0.15, ta + 0.8)) * recede;
      brackets(R, x - bw / 2, OP_Y - 19 - bh / 2, bw, bh, 9, { color: a.human ? C.human : C.ice, w: 1.1, alpha: ba, glow: 0.6 * (1 - f) });
    }
    // empty semantic socket: this obligation still needs grounded evidence
    if (a.socket) {
      const sk = smooth(ta + 0.15, ta + 0.4, t) * recede;
      const aw = measure(a.arg, ARG);
      circle(R, x + aw / 2 + 14, ARG_Y - 6, 6.5, { color: C.ice, w: 1.1, alpha: 0.85 * sk, dash: [2.2, 2.6], dashOffset: t * 14 });
    }
  });

  // chain connectors: the order of operations
  for (let i = 0; i < 5; i++) {
    const p = E.snap(seg(t, T.chain + i * 0.045, T.chain + 0.3 + i * 0.045));
    if (p <= 0) continue;
    const xa = atomX(i) + measure(ATOMS[i].op, OP) / 2 + 20;
    const xb = atomX(i + 1) - measure(ATOMS[i + 1].op, OP) / 2 - 20;
    const y = OP_Y - 19;
    line(R, xa, y, lerp(xa, xb, p), y, { color: C.ice, w: 1.2, alpha: 0.7 * recede, glow: 0.35 });
    if (p > 0.9) poly(R, [[xb - 7, y - 4.5], [xb, y], [xb - 7, y + 4.5]], { color: C.ice, w: 1.2, alpha: 0.8 * recede });
  }
  // a pulse runs the chain once it is whole
  const pr = seg(t, T.chain + 0.3, T.dive);
  if (pr > 0 && pr < 1) {
    const u = E.inOutSine(pr) * 5;
    const i = Math.min(4, Math.floor(u));
    const xa = atomX(i) + measure(ATOMS[i].op, OP) / 2 + 20;
    const xb = atomX(i + 1) - measure(ATOMS[i + 1].op, OP) / 2 - 20;
    const px = lerp(xa, xb, clamp(u - i));
    circle(R, px, OP_Y - 19, 3, { fill: C.cyan, alpha: recede * (1 - diveOut), glow: 1 });
  }

  // provenance brackets: the raw message is kept under what it produced
  for (let l = 0; l < 4; l++) {
    const p = E.snap(seg(t, T.chain + 0.05 + l * 0.05, T.chain + 0.4 + l * 0.05));
    if (p <= 0) continue;
    const ids = CLAUSE_ATOMS[l];
    const xa = atomX(ids[0]) - 108, xb = atomX(ids[ids.length - 1]) + 108;
    const cx = (xa + xb) / 2, hw = ((xb - xa) / 2) * p;
    const y = 704;
    poly(R, [[cx - hw, y - 5], [cx - hw, y], [cx + hw, y], [cx + hw, y - 5]], { color: C.human, w: 1, alpha: 0.4 * recede });
    line(R, cx, y, cx, y + 26 * p, { color: C.human, w: 1, alpha: 0.4 * recede });
  }
  // the title: one message, six obligations (fixed, so it reads while the chips strum in)
  if (t >= T.fracture + 0.25) beatTitle({ ...R, fade: recede }, t, T.fracture + 0.25, T.dive + 0.25, '6 obligations', 'EXTRACTED FROM ONE MESSAGE · NOT YET GROUNDED');

  // ── the dive: "invoices" becomes a window into the observed world ────
  diveWindow(R, t);
}

// Big word geometry for the dive.
const BIG = { f: 'Geist 800', size: 260, tracking: -8 };
const BIG_WORD = 'invoices.json';
const BIG_BASE = 620;
function bigLayout() {
  const w = measure(BIG_WORD, BIG);
  const x0 = 960 - w / 2;
  const g = glyphs(BIG_WORD, BIG);
  // zoom through the period between name and format: a solid dot near center
  const dot = g.find(q => q.ch === '.');
  const dm = metrics('.', BIG);
  return { w, x0, zx: x0 + dot.x + dot.w * 0.5, zy: BIG_BASE - dm.actualBoundingBoxAscent * 0.5 };
}

function diveWindow(R, t) {
  const a0 = T.dive - 0.05, a1 = T.dive + 0.3; // grow into the big word
  const z0 = T.dive + 0.24, z1 = T.observe + 0.28; // zoom through it
  if (t < a0 || t > z1) return;
  const B = bigLayout();
  const grow = E.inOutCubic(seg(t, a0, a1));
  const zp = seg(t, z0, z1);
  const lz = E.inCubic(zp);
  const zoom = Math.exp(lz * Math.log(90));
  const q = E.inOutSine(clamp(lz * 1.6));
  const ctx = R.ctx;

  // phase A: the small argument travels to the center and grows
  if (zp <= 0) {
    const cx = lerp(atomX(0), 960, grow), by = lerp(ARG_Y, BIG_BASE, grow);
    const s = lerp(ARG.size / BIG.size, 1, E.inCubic(grow));
    const m = smooth(0.1, 0.55, grow);
    ctx.save();
    ctx.translate(cx, by);
    ctx.scale(s, s);
    const wSmall = measure('invoices', { f: 'MM 400', size: BIG.size });
    text(R, 'invoices', -wSmall / 2, 0, { f: 'MM 400', size: BIG.size, color: C.mist, alpha: 1 - m });
    text(R, BIG_WORD, -B.w / 2, 0, { ...BIG, color: C.ice, alpha: m, glow: 0.3 });
    ctx.restore();
    return;
  }

  // phase B: the word is a mask; the observed world lives inside the glyphs
  const apply = c => {
    c.translate(lerp(B.zx, 960, q), lerp(B.zy, 540, q));
    c.scale(zoom, zoom);
    c.translate(-B.zx, -B.zy);
  };
  const vis = visibleWord(B, zoom, q);
  if (!vis) {
    // through the window: the observed world fills the frame
    ctx.save();
    observe.drawWorld(R, t, { inMask: true });
    ctx.restore();
    return;
  }
  // the glyphs are a clip: the observed world is drawn only inside them
  ctx.save();
  apply(ctx);
  ctx.clip(vis);
  ctx.setTransform(R.k, 0, 0, R.k, 0, 0);
  observe.drawWorld(R, t, { inMask: true });
  ctx.restore();
  // the filled word dissolves into the window
  const fillA = passAlpha(R, 1 - smooth(0, 0.35, zp), 0.3);
  if (fillA > 0) {
    ctx.save();
    apply(ctx);
    ctx.fillStyle = C.ice;
    ctx.globalAlpha = fillA;
    ctx.fill(vis);
    ctx.restore();
  }
  // glyph outline: the window frame
  const oa = passAlpha(R, 1 - smooth(0.6, 0.92, zp), 0.9);
  if (oa > 0) {
    ctx.save();
    apply(ctx);
    ctx.strokeStyle = C.ice;
    ctx.lineWidth = 1.8 / zoom;
    ctx.globalAlpha = oa;
    ctx.stroke(vis);
    ctx.restore();
  }
}
