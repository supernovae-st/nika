// S3 · PROPOSE — Foundry knowledge + selective intelligence.
//
// A versioned library of checked compositions fills the depth of the frame.
// Reflex sweeps it and ranks; almost everything goes dark. CLM scores the
// survivors, Jev resolves a CLOSED choice. The chosen block flies forward —
// a template with EMPTY sockets. Intelligence proposes; nothing here is
// proof, and nothing here can authorize anything.
import { C, E, clamp, lerp, seg, smooth, hash, win } from '../engine/core.mjs';
import { text, line, poly, circle, rrect, rect, measure } from '../engine/render.mjs';
import { camera, project, REF } from '../engine/cam.mjs';
import { T } from '../timeline.mjs';
import { atomRow, tag, atomX } from './shared.mjs';

// ── the library ────────────────────────────────────────────────────────
const GROUPS = ['PATTERNS', 'NIKABLOCKS', 'SKELETONS', 'FAMILIES', 'EXAMPLES', 'COUNTEREXAMPLES'];
const MW = 150, MH = 84;
const NAMES = ['observe·filter·group', 'fetch·extract·cite', 'watch·diff·alert', 'read·join·sum', 'search·rank·quote', 'draft·review·send', 'parse·validate·store', 'poll·gate·notify'];
let LIB = null;
function library() {
  if (LIB) return LIB;
  LIB = [];
  let id = 0;
  const ranks = [0, 420, 840, 1260, 1680, 2100, 2520];
  ranks.forEach((z, r) => {
    const cols = 22 + r * 5;
    const span = cols * 196;
    for (let c = 0; c < cols; c++) {
      for (let row = 0; row < 3; row++) {
        const x = 960 - span / 2 + c * 196 + (row % 2) * 56;
        const y = 300 + row * 118 - r * 24;
        const h = hash(id, 13);
        if (h < 0.1) { id++; continue; }
        const n = 3 + Math.floor(hash(id, 17) * 4);
        const shape = Math.floor(hash(id, 19) * 4);
        LIB.push({ id, x, y, z, r, c, row, n, shape, name: NAMES[Math.floor(hash(id, 23) * NAMES.length)] });
        id++;
      }
    }
  });
  return LIB;
}
// Candidate blocks (front rank).
const CAND = { A: null, B: null, C: null };
function candidates() {
  const lib = library();
  if (!CAND.A) {
    const front = lib.filter(m => m.r === 0 && m.row === 1);
    CAND.A = front.find(m => m.x > 900 && m.x < 1100) || front[12];
    CAND.B = front.find(m => m.x > 500 && m.x < 700) || front[9];
    CAND.C = lib.find(m => m.r === 1 && m.row === 0 && m.x > 1300 && m.x < 1500) || front[15];
    CAND.A.n = 6; CAND.A.shape = 0;
    CAND.B.n = 4; CAND.B.shape = 0;
    CAND.C.n = 3; CAND.C.shape = 0;
  }
  return CAND;
}

function camAt(t) {
  const u = seg(t, T.rise, T.block + 0.6);
  const px = lerp(-160, 160, E.inOutSine(u));
  const dolly = E.inOutSine(seg(t, T.rise, T.block + 0.2)); // a slow push through the archive
  const back = E.inOutCubic(seg(t, T.block, T.plan + 0.2)); // the library falls away as the block comes forward
  return camera([960 + px, 400 - 60 * back, -REF - 380 + 560 * dolly - 1100 * back], [960 + px * 0.6, 520, 900]);
}

// Mini topology inside a module (the shape of a composition).
function topo(R, m, x, y, s, a, col) {
  const n = m.n;
  const pts = [];
  for (let i = 0; i < n; i++) {
    let px, py;
    if (m.shape === 0) { px = -40 + (80 * i) / (n - 1); py = 0; }
    else if (m.shape === 1) { px = -40 + (80 * i) / (n - 1); py = i % 2 ? -8 : 8; }
    else if (m.shape === 2) { px = i === 0 ? -40 : i === n - 1 ? 40 : 0; py = i === 0 || i === n - 1 ? 0 : -16 + (32 * (i - 1)) / Math.max(1, n - 3); }
    else { const an = (i / n) * Math.PI * 2; px = Math.cos(an) * 26; py = Math.sin(an) * 12; }
    pts.push([x + px * s, y + py * s]);
  }
  const st = { color: col, w: Math.max(0.6, 1 * s), alpha: a * 0.8 };
  if (m.shape === 2) {
    for (let i = 1; i < n - 1; i++) { poly(R, [pts[0], pts[i]], st); poly(R, [pts[i], pts[n - 1]], st); }
  } else if (m.shape === 3) {
    poly(R, [...pts, pts[0]], st);
  } else poly(R, pts, st);
  for (const p of pts) circle(R, p[0], p[1], 2.2 * s, { fill: col, alpha: a });
}

// Reflex ranking: score in [0,1] after the sweep passes.
function scanX(t) {
  return lerp(-300, 2250, E.inOutSine(seg(t, T.reflex, T.reflex + 0.75)));
}
function survives(m) {
  const c = candidates();
  return m === c.A || m === c.B || m === c.C;
}

export function env(t) {
  const u = seg(t, T.rise, T.block + 0.6);
  return {
    bgGlow: 1,
    gridAlpha: 0.55,
    gridX: -lerp(-140, 140, E.inOutSine(u)) * 0.35,
    gridY: 260 + 120 * seg(t, T.foundry, T.plan),
    bgY: 470,
  };
}

export function draw(R, t) {
  const cam = camAt(t);
  const lib = library();
  const cand = candidates();
  const sx = scanX(t);
  const fall = smooth(T.block, T.block + 0.55, t); // library dims as the choice is made
  const appearBase = T.foundry;

  // group headers (front rank)
  GROUPS.forEach((g, gi) => {
    const wx = -1150 + gi * 900 + 450;
    const pr = project(cam, [wx, 250, 0]);
    if (!pr.ok) return;
    const k = smooth(appearBase + 0.05 * gi, appearBase + 0.35 + 0.05 * gi, t) * (1 - fall);
    text(R, g, pr.x, pr.y, { f: 'MGU 400', size: 10.5 * pr.s * 1.2, tracking: 4, color: g === 'NIKABLOCKS' ? C.ice : C.dim, alpha: k * 0.9, align: 'center' });
  });

  // modules, far to near
  const sorted = lib.slice().sort((a, b) => b.z - a.z);
  for (const m of sorted) {
    const pr = project(cam, [m.x, m.y, m.z]);
    if (!pr.ok || pr.x < -150 || pr.x > 2070 || pr.y < -100 || pr.y > 1180) continue;
    const d = Math.hypot(m.x - 960, (m.y - 430) * 2, m.z * 0.6);
    const ta = appearBase + d / 2600;
    const k = smooth(ta, ta + 0.3, t);
    if (k <= 0) continue;
    const isC = survives(m);
    let a = k * lerp(1, 0.28, m.r / 4);
    // after the reflex sweep: most go dark
    const passed = pr.x < sx;
    const ranked = passed ? smooth(0, 1, clamp((sx - pr.x) / 220)) : 0;
    if (!isC) a *= lerp(1, 0.18, ranked);
    if (m === cand.A && t > T.block) continue; // the chosen block is drawn by the flight below
    a *= 1 - fall * (isC ? 0.2 : 0.75);
    if (a <= 0.02) continue;
    const s = pr.s;
    const col = isC && ranked > 0 ? C.ice : m.r > 3 ? C.faint : m.r > 1 ? C.dim : '#7A9CC8';
    const w = MW * s, h = MH * s;
    const front = m.r <= 1;
    rrect(R, pr.x - w / 2, pr.y - h / 2, w, h, 7 * s, { color: col, w: Math.max(0.5, (isC && ranked ? 1.5 : 1) * Math.min(1.2, s * 1.2)), alpha: a * (m.r > 2 ? 0.6 : 1), fill: C.bg0, fillAlpha: 0.6, glow: isC && ranked ? 0.6 : front ? 0.12 : 0 });
    topo(R, m, pr.x, pr.y - 8 * s, s * 1.15, a * 0.95, isC && ranked ? C.ice : col);
    if (front && s > 0.7) text(R, m === cand.A ? 'observe·filter·group·aggregate·gate·effect' : m.name, pr.x, pr.y + 26 * s, { f: 'MM 400', size: 9.5 * s, color: isC && ranked ? C.ice : C.dim, alpha: a * 0.9, align: 'center' });
    // arrival flash
    const fl = seg(t, ta, ta + 0.25);
    if (fl > 0 && fl < 1 && m.r < 2) rrect(R, pr.x - w / 2, pr.y - h / 2, w, h, 6 * s, { color: C.ice, w: 1, alpha: 0.6 * (1 - fl), glow: 0.8 });
    // candidate scores
    if (isC && ranked > 0) {
      const score = m === cand.A ? '0.81' : m === cand.B ? '0.12' : '0.05';
      const lab = m === cand.A ? 'A' : m === cand.B ? 'B' : 'C';
      text(R, `${lab} ${score}`, pr.x + w / 2 + 6 * s, pr.y - h / 2 + 10 * s, { f: 'MM 500', size: 11 * Math.max(0.8, s), color: m === cand.A ? C.cyan : C.mist, alpha: ranked * (1 - fall * 0.5), glow: m === cand.A ? 0.6 : 0 });
    }
  }

  // Reflex sweep
  const swA = win(t, T.reflex - 0.05, T.reflex + 0.8, 0.08, 0.2);
  if (swA > 0) {
    line(R, sx, 205, sx, 800, { color: C.cyan, w: 1.2, alpha: 0.9 * swA, glow: 1 });
    rect(R, sx - 60, 205, 60, 595, { fill: C.cyan, alpha: 0.03 * swA });
    text(R, 'REFLEX', sx + 10, 222, { f: 'MGW 700', size: 11, tracking: 4, color: C.cyan, alpha: swA, glow: 0.5 });
    text(R, 'route · retrieve · rank · witness · abstain', sx + 10, 240, { f: 'MM 400', size: 10.5, color: C.mist, alpha: swA * 0.9 });
  }

  // FOUNDRY title
  const ft = smooth(T.foundry, T.foundry + 0.3, t) * (1 - fall);
  tag(R, 'FOUNDRY · VERSIONED KNOWLEDGE', 150, 262, { color: C.ice, alpha: ft, size: 11 });
  text(R, 'patterns · blocks · skeletons · families · examples · counterexamples', 166, 282, { f: 'MM 400', size: 10.5, color: C.dim, alpha: ft * 0.9 });

  // CLM + Jev satellites (side panels — never on the execution path)
  satellites(R, t, fall);

  // the chosen block comes forward, a template with EMPTY sockets
  chosenFlight(R, t, cam);

  // tagline
  const tg = win(t, T.tagPropose, T.plan + 0.1, 0.25, 0.3);
  if (tg > 0) {
    const k = E.snap(seg(t, T.tagPropose, T.tagPropose + 0.5));
    text(R, 'Intelligence', 150, 700 + 10 * (1 - k), { f: 'Geist 600', size: 60, tracking: -1.5, color: C.ink, alpha: tg, glow: 0.15 });
    text(R, 'proposes.', 150 + measure('Intelligence ', { f: 'Geist 600', size: 60, tracking: -1.5 }), 700 + 10 * (1 - k), { f: 'Geist 600', size: 60, tracking: -1.5, color: C.cyan, alpha: tg * smooth(T.tagPropose + 0.15, T.tagPropose + 0.4, t), glow: 0.35 });
  }

  // intent, docked above (what was asked)
  const dk = E.snap(seg(t, T.rise + 0.25, T.rise + 0.85));
  if (dk > 0 && t < T.plan + 0.05) {
    atomRow(R, t, { cx: 960, cy: lerp(40, 150, dk), s: 0.42, alpha: dk * 0.9, glow: 0.1 });
    text(R, 'INTENT', 150, lerp(40, 150, dk) - 34, { f: 'MGW 500', size: 10.5, tracking: 4, color: C.human, alpha: dk * 0.9 });
  }
}

function satellites(R, t, fall) {
  const x0 = 1340, y0 = 250;
  const k = smooth(T.clm - 0.1, T.clm + 0.25, t) * (1 - smooth(T.plan - 0.1, T.plan + 0.25, t));
  if (k <= 0) return;
  rrect(R, x0 - 20, y0 - 30, 470, 222, 10, { color: C.faint, w: 1, alpha: k, fill: C.bg0, fillAlpha: 0.72 });
  text(R, 'CLM', x0, y0, { f: 'MGW 700', size: 11, tracking: 4, color: C.cyan, alpha: k, glow: 0.4 });
  text(R, 'contrastive ranking over closed actions', x0 + 52, y0, { f: 'MM 400', size: 10.5, color: C.dim, alpha: k });
  const rows = [
    ['A', 'observe · filter · group · aggregate · gate · effect', 0.81],
    ['B', 'observe · group · aggregate · effect', 0.12],
    ['C', 'observe · filter · aggregate', 0.05],
    ['∅', 'UNKNOWN — abstain', 0.02],
  ];
  rows.forEach(([id, desc, sc], i) => {
    const y = y0 + 30 + i * 30;
    const g = E.snap(seg(t, T.clm + 0.1 + i * 0.125, T.clm + 0.5 + i * 0.125));
    const best = i === 0;
    text(R, id, x0, y, { f: 'MM 500', size: 12, color: best ? C.cyan : C.mist, alpha: k });
    text(R, desc, x0 + 22, y, { f: 'MM 400', size: 10.5, color: best ? C.ink : C.dim, alpha: k });
    const bw = 90 * sc * g;
    rect(R, x0 + 330, y - 8, Math.max(1, bw), 6, { fill: best ? C.cyan : C.dim, alpha: k * 0.9, glow: best ? 0.6 : 0 });
    text(R, sc.toFixed(2), x0 + 428, y, { f: 'MM 400', size: 11, color: best ? C.cyan : C.dim, alpha: k * g, align: 'right' });
  });
  text(R, 'score ≠ truth', x0, y0 + 160, { f: 'MM 400', size: 10.5, color: C.amber, alpha: k * 0.9 });
  // Jev · DecisionSeat: a closed choice
  const jk = smooth(T.jev - 0.35, T.jev - 0.1, t) * k;
  if (jk > 0) {
    const y = y0 + 238;
    rrect(R, x0 - 20, y - 26, 470, 70, 10, { color: C.faint, w: 1, alpha: jk, fill: C.bg0, fillAlpha: 0.72 });
    text(R, 'JEV · DECISIONSEAT', x0, y - 4, { f: 'MGW 700', size: 10.5, tracking: 3, color: C.cyan, alpha: jk, glow: 0.3 });
    text(R, 'one offered option — or none', x0 + 190, y - 4, { f: 'MM 400', size: 10.5, color: C.dim, alpha: jk });
    const sel = smooth(T.jev, T.jev + 0.12, t);
    ['A', 'B', 'C', '∅'].forEach((o, i) => {
      const bx = x0 + i * 40, by = y + 8;
      const on = i === 0 ? sel : 0;
      rrect(R, bx, by, 30, 22, 5, { color: on ? C.cyan : C.faint, w: 1.1, alpha: jk, fill: on ? C.cyan : null, fillAlpha: 0.18 * on, glow: on ? 0.6 : 0 });
      text(R, o, bx + 15, by + 15.5, { f: 'MM 500', size: 11.5, color: on ? C.ink : C.dim, alpha: jk, align: 'center' });
    });
    text(R, 'judgement ≠ proof', x0 + 180, y + 23, { f: 'MM 400', size: 10.5, color: C.amber, alpha: jk * 0.9 });
  }
}

// ── the chosen NikaBlock ───────────────────────────────────────────────
export const SLOTS = [
  { name: 'OBSERVE', needs: 'source' },
  { name: 'FILTER', needs: 'filter field' },
  { name: 'GROUP', needs: 'group key' },
  { name: 'AGGREGATE', needs: 'aggregate field' },
  { name: 'GATE', needs: 'human decision' },
  { name: 'EFFECT', needs: 'destination' },
];
export const BLOCK = { x: 110, y: 438, w: 1700, h: 214 };
// Plan-node rows shared with the plan scene.
export const ROW = { kind: 476, op: 522, socket: 566, detail: 603, badge: 631 };

export function blockProgress(t) {
  return E.inOutCubic(seg(t, T.block + 0.05, T.plan + 0.15));
}

function chosenFlight(R, t, cam) {
  const cand = candidates();
  const m = cand.A;
  if (t < T.block || t >= T.plan + 0.15) return; // the plan scene takes the block over
  const p = blockProgress(t);
  const pr = project(camAt(T.block), [m.x, m.y, m.z]);
  const s0 = pr.s;
  // from the module's rect to the block's rect
  const x = lerp(pr.x - (MW * s0) / 2, BLOCK.x, p);
  const y = lerp(pr.y - (MH * s0) / 2, BLOCK.y, p);
  const w = lerp(MW * s0, BLOCK.w, p);
  const h = lerp(MH * s0, BLOCK.h, p);
  drawBlock(R, t, { x, y, w, h, p, alpha: 1 });
}

// The block template: slots + empty sockets. Exported for the plan scene.
export function drawBlock(R, t, { x, y, w, h, p = 1, alpha = 1, fill = null, dissolve = 0 }) {
  const a = alpha * (1 - dissolve);
  rrect(R, x, y, w, h, 12 * clamp(p * 2), { color: C.cyan, w: 1.3, alpha: a, fill: C.bg0, fillAlpha: 0.78 * (1 - dissolve), glow: 0.5 * (1 - p * 0.6) });
  if (p < 0.35) {
    // still small: the mini topology
    topo(R, { n: 6, shape: 0 }, x + w / 2, y + h / 2, w / MW, a, C.cyan);
    return;
  }
  const k = smooth(0.35, 0.9, p);
  text(R, 'NIKABLOCK', x + 22, y + 26, { f: 'MGW 700', size: 11, tracking: 4, color: C.cyan, alpha: a * k, glow: 0.3 });
  text(R, 'observe → filter → group → aggregate → gate → effect · checked · v3 · a template, not a program', x + 150, y + 26, { f: 'MM 400', size: 10.5, color: C.dim, alpha: a * k });
  const n = SLOTS.length;
  for (let i = 0; i < n; i++) {
    // slots sit exactly where the obligations will land
    const cx = lerp(x + w / 2, atomX(i), k);
    if (i < n - 1) {
      const cx2 = lerp(x + w / 2, atomX(i + 1), k);
      line(R, cx + 80, ROW.op - 14, cx2 - 80, ROW.op - 14, { color: C.cyan, w: 1, alpha: a * k * 0.5, dash: [3, 4], dashOffset: -t * 30 });
    }
    const fillK = fill ? fill[i] : 0;
    const nameK = fill ? (fill.names ? fill.names[i] : fillK) : 0;
    text(R, SLOTS[i].name, cx, ROW.op - 6, { f: 'MGW 500', size: 13, tracking: 3.5, color: C.cyan, alpha: a * k * (1 - nameK), align: 'center' });
    if (SLOTS[i].name !== 'GATE') {
      circle(R, cx, ROW.socket, 9, { color: C.cyan, w: 1.2, alpha: a * k * (1 - fillK), dash: [2.5, 3], dashOffset: t * 16 });
      text(R, SLOTS[i].needs, cx, ROW.detail, { f: 'MM 400', size: 11, color: C.mist, alpha: a * k * (1 - fillK), align: 'center' });
    } else {
      poly(R, [[cx - 7, ROW.socket - 10], [cx - 7, ROW.socket + 10]], { color: C.human, w: 1.6, alpha: a * k * (1 - nameK) });
      poly(R, [[cx + 7, ROW.socket - 10], [cx + 7, ROW.socket + 10]], { color: C.human, w: 1.6, alpha: a * k * (1 - nameK) });
      text(R, SLOTS[i].needs, cx, ROW.detail, { f: 'MM 400', size: 11, color: C.human, alpha: a * k * 0.8 * (1 - nameK), align: 'center' });
    }
  }
}
