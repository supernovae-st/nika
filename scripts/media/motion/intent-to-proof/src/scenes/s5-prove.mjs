// S5 · PROVE — a detector ring sweeps the plan like a beamline.
//
// Rust proves what the plan states explicitly: provenance, capabilities,
// types, keys, closure, units, gate structure, effect class, permits, state
// transitions. Proof marks appear only where the evidence exists. One fact
// cannot be proven — the currency of amount_cents — so Nika does not guess:
// an amber loop leaves the plan, asks the human, and returns. Unknown is a
// feature: safe incomplete beats unsafe success.
import { C, E, TAU, lerp, seg, smooth, win } from '../engine/core.mjs';
import { text, line, poly, circle, rrect, measure, check, light, bezierPts, bezierAt } from '../engine/render.mjs';
import { camera, project } from '../engine/cam.mjs';
import { T } from '../timeline.mjs';
import { drawNodes, nodeX, currencyResolved, camAt as planCam } from './s4-plan.mjs';
import { ROW } from './s3-propose.mjs';

const CY = 560; // beam axis height (center of the node column)
const PROOFS = ['PROVENANCE', 'CAPABILITY', 'TYPES', 'KEYS', 'CLOSURE', 'UNITS', 'GATE', 'EFFECTS', 'PERMITS', 'STATES'];
const UNITS = 5;
const R0 = 300;

// the ring sweeps the chain at constant speed; it crosses SUM on the UNITS check
const SW0 = T.pass - 0.05, SW1 = T.checks[9] + 0.03;
export function ringX(t) {
  return lerp(nodeX(0) - 180, nodeX(5) + 180, seg(t, SW0, SW1));
}
function crossT(i) {
  const x0 = nodeX(0) - 180, x1 = nodeX(5) + 180;
  return lerp(SW0, SW1, (nodeX(i) - x0) / (x1 - x0));
}

// camera keyframes
function camAt(t) {
  const rx = ringX(t);
  const track = { eye: [rx - 620, 330, -1080], target: [rx + 320, 575, 90] };
  const wide = { eye: [760, 300, -1900], target: [1000, 470, 0] };
  const axis = { eye: [nodeX(5) + 180 - 1100, CY, -40], target: [nodeX(5) + 180 + 1200, CY, 0] };
  const thru = { eye: [nodeX(5) + 180 + 700, CY, -10], target: [nodeX(5) + 180 + 2600, CY, 0] };
  const mix = (a, b, u) => ({ eye: a.eye.map((v, i) => lerp(v, b.eye[i], u)), target: a.target.map((v, i) => lerp(v, b.target[i], u)) });
  let k;
  if (t < SW0) {
    const s4 = planCam(T.ring);
    const u = E.inOutCubic(seg(t, T.ring, SW0));
    const tr0 = { eye: [ringX(SW0) - 620, 330, -1080], target: [ringX(SW0) + 320, 575, 90] };
    k = mix({ eye: s4.eye, target: s4.target }, tr0, u);
  } else if (t < SW1) k = track;
  else if (t < T.verified + 0.05) k = mix({ eye: [ringX(SW1) - 620, 330, -1080], target: [ringX(SW1) + 320, 575, 90] }, wide, E.inOutCubic(seg(t, SW1, SW1 + 0.55)));
  else if (t < T.tunnel) k = mix(wide, axis, E.inOutCubic(seg(t, T.verified + 0.05, T.tunnel)));
  else k = mix(axis, thru, E.inExpo(seg(t, T.tunnel, T.lower + 0.3)));
  return camera(k.eye, k.target);
}

const P = (cam, x, y, z = 0) => project(cam, [x, y, z]);

function sectorAngle(k) {
  return -Math.PI / 2 + (k / PROOFS.length) * TAU;
}
function checkState(t, k) {
  return smooth(T.checks[k], T.checks[k] + 0.1, t);
}

function ring(R, cam, t, alpha) {
  const rx = ringX(t);
  const build = E.snap(seg(t, T.ring, T.ring + 0.5));
  const circ = (r, st, a0 = 0, a1 = TAU, n = 96) => {
    const pts = [];
    for (let i = 0; i <= n; i++) {
      const an = lerp(a0, a1, i / n);
      const p = P(cam, rx, CY + Math.sin(an) * r, Math.cos(an) * r);
      if (p.ok) pts.push([p.x, p.y]);
    }
    poly(R, pts, st, build);
  };
  // structure
  circ(R0 - 44, { color: C.faint, w: 1, alpha: alpha * 0.8 });
  circ(R0, { color: C.ice, w: 1.6, alpha: alpha * 0.9, glow: 0.4 });
  circ(R0 + 14, { color: C.dim, w: 0.8, alpha: alpha * 0.7 });
  // ticks
  for (let i = 0; i < 120; i++) {
    const an = (i / 120) * TAU + t * 0.25;
    const long = i % 10 === 0;
    const a = P(cam, rx, CY + Math.sin(an) * (R0 + 16), Math.cos(an) * (R0 + 16));
    const b = P(cam, rx, CY + Math.sin(an) * (R0 + (long ? 34 : 24)), Math.cos(an) * (R0 + (long ? 34 : 24)));
    if (a.ok && b.ok) line(R, a.x, a.y, b.x, b.y, { color: C.dim, w: 0.8, alpha: alpha * build * (long ? 0.9 : 0.5) });
  }
  // proof sectors and labels
  for (let k = 0; k < PROOFS.length; k++) {
    const an = sectorAngle(k);
    const st = checkState(t, k);
    const amber = k === UNITS && currencyResolved(t) < 0.5;
    const col = amber ? C.amber : C.teal;
    if (st > 0) circ(R0 + 50, { color: col, w: 3.2, alpha: alpha * st, glow: 0.8 }, an - 0.26, an + 0.26, 16);
    else circ(R0 + 50, { color: C.faint, w: 1.2, alpha: alpha * 0.7 }, an - 0.26, an + 0.26, 16);
    const pl = P(cam, rx, CY + Math.sin(an) * (R0 + 96), Math.cos(an) * (R0 + 96));
    if (!pl.ok) continue;
    const la = alpha * build * lerp(0.45, 1, st);
    text(R, PROOFS[k], pl.x, pl.y, { f: 'MGW 500', size: 10 * pl.s, tracking: 2.4, color: st > 0 ? col : C.dim, alpha: la, align: 'center', glow: st > 0 ? 0.35 : 0 });
    if (st > 0) {
      if (amber) text(R, '?', pl.x, pl.y + 17 * pl.s, { f: 'Geist 600', size: 15 * pl.s, color: C.amber, alpha: la, align: 'center', glow: 0.6 });
      else check(R, pl.x, pl.y + 12 * pl.s, 9 * pl.s, E.snap(seg(t, T.checks[k], T.checks[k] + 0.2)), { color: C.teal, w: 1.6 * pl.s, glow: 0.7, alpha: la });
    }
  }
  // the scan plane: light where the ring cuts the beam
  const hot = win(t, SW0, SW1, 0.1, 0.1);
  const pc = P(cam, rx, CY, 0);
  if (hot > 0 && pc.ok) light(R, pc.x, pc.y, 170 * pc.s, C.cyan, 0.3 * hot * alpha, 0.8);
  // label
  const top = P(cam, rx, CY - R0 - 150, 0);
  if (top.ok) {
    text(R, 'RUST PROOFS', top.x, top.y, { f: 'MGW 700', size: 12 * top.s, tracking: 5, color: C.ice, alpha: alpha * build, align: 'center', glow: 0.4 });
    text(R, 'deterministic · explicit semantic facts only', top.x, top.y + 18 * top.s, { f: 'MM 400', size: 10.5 * top.s, color: C.dim, alpha: alpha * build, align: 'center' });
  }
}

export function env(t) {
  return { bgGlow: 1.15, gridAlpha: 0.35 * (1 - smooth(T.tunnel - 0.2, T.tunnel + 0.1, t)), gridX: -ringX(t) * 0.15, gridY: 380, bgY: 540 };
}

export function draw(R, t) {
  const cam = camAt(t);
  const inA = smooth(T.ring - 0.1, T.ring + 0.2, t);
  const outA = 1 - smooth(T.tunnel + 0.15, T.lower + 0.35, t);
  const alpha = inA * outA;
  if (alpha <= 0 && t < T.verified) return;

  // the beam axis
  const b0 = P(cam, -400, CY, 0), b1 = P(cam, 2600, CY, 0);
  if (b0.ok && b1.ok) line(R, b0.x, b0.y, b1.x, b1.y, { color: C.faint, w: 1, alpha: alpha * 0.6 });

  // nodes (continuous with the plan scene); verified as the ring passes
  const verified = i => (i === 3 ? currencyResolved(t) : 1) * smooth(crossT(i), crossT(i) + 0.12, t);
  const nodesOut = 1 - smooth(T.verified + 0.02, T.tunnel + 0.02, t); // the plan leaves the frame before the fly-through
  if (t >= T.ring) drawNodes(R, cam, t, { alpha: outA * nodesOut, verified, fills: () => 1 });
  // SUM holds an amber mark until the answer returns
  const sumP = P(cam, nodeX(3), ROW.kind - 26, 0);
  const amberMark = smooth(crossT(3), crossT(3) + 0.12, t) * (1 - currencyResolved(t));
  if (amberMark > 0 && sumP.ok) text(R, '?', sumP.x, sumP.y + 6 * sumP.s, { f: 'Geist 600', size: 22 * sumP.s, color: C.amber, alpha: alpha * amberMark * nodesOut, align: 'center', glow: 0.8 });

  ring(R, cam, t, alpha);

  // proof counter
  const n = T.checks.filter(c => t >= c).length;
  const ca = alpha * smooth(T.pass - 0.1, T.pass + 0.1, t) * (1 - smooth(T.verified + 0.1, T.verified + 0.4, t));
  if (ca > 0) {
    const shown = n > UNITS && currencyResolved(t) < 0.5 ? n - 1 : n;
    text(R, 'PROVEN', 150, 190, { f: 'MGW 500', size: 10, tracking: 3, color: C.dim, alpha: ca });
    text(R, `${String(shown).padStart(2, '0')}/10`, 150, 232, { f: 'Geist 300', size: 40, color: C.teal, alpha: ca, glow: 0.35 });
    if (n > UNITS) {
      const ua = currencyResolved(t) < 0.5 ? 1 : 0;
      text(R, 'UNKNOWN', 290, 190, { f: 'MGW 500', size: 10, tracking: 3, color: C.amber, alpha: ca * ua });
      text(R, '01', 290, 232, { f: 'Geist 300', size: 40, color: C.amber, alpha: ca * ua, glow: 0.35 });
    }
  }

  askLoop(R, t, cam, alpha);

  // verdict
  const v = win(t, T.verified, T.lower + 0.3, 0.12, 0.22);
  if (v > 0) {
    const k = E.snap(seg(t, T.verified, T.verified + 0.4));
    text(R, 'Rust', 150, 930 + 12 * (1 - k), { f: 'Geist 600', size: 64, tracking: -1.8, color: C.ink, alpha: v, glow: 0.2 });
    text(R, 'proves.', 150 + measure('Rust ', { f: 'Geist 600', size: 64, tracking: -1.8 }), 930 + 12 * (1 - k), { f: 'Geist 600', size: 64, tracking: -1.8, color: C.teal, alpha: v * smooth(T.verified + 0.08, T.verified + 0.3, t), glow: 0.4 });
    text(R, 'VERIFIED · 10/10 · 0 MATERIAL UNKNOWNS', 152, 962, { f: 'MGW 500', size: 10.5, tracking: 3, color: C.teal, alpha: v * 0.9 });
  }
}

// ── UNKNOWN → ASK → back into the plan ─────────────────────────────────
const CARD = { x: 560, y: 150, w: 560, h: 214 };
function askLoop(R, t, cam, alpha) {
  const open = smooth(T.unknown, T.unknown + 0.3, t);
  const close = 1 - smooth(T.verified - 0.02, T.tunnel, t);
  const a = alpha * open * close;
  if (a <= 0) return;
  const src = P(cam, nodeX(3), ROW.badge + 27, 0);
  if (!src.ok) return;
  const dst = [CARD.x + CARD.w - 80, CARD.y + CARD.h];
  const c0 = [src.x + 40, src.y - 240], c1 = [dst[0] + 160, dst[1] + 200];
  const draw = E.snap(seg(t, T.unknown, T.ask));
  const back = seg(t, T.answer + 0.05, T.returned);
  const pts = bezierPts([src.x, src.y], c0, c1, dst, 60);
  // the loop: amber out, teal on the way back
  poly(R, pts, { color: C.amber, w: 1.6, alpha: a * (1 - back * 0.6), glow: 0.8 }, draw);
  if (back > 0) {
    const rev = pts.slice().reverse();
    poly(R, rev, { color: C.teal, w: 1.8, alpha: a, glow: 0.9 }, E.inOutCubic(back));
    const q = bezierAt(dst, c1, c0, [src.x, src.y], E.inOutCubic(back));
    circle(R, q[0], q[1], 4, { fill: C.human, alpha: a, glow: 1 });
    text(R, 'EUR', q[0] + 10, q[1] - 8, { f: 'MM 500', size: 12, color: C.human, alpha: a * (1 - smooth(0.85, 1, back)) });
  }
  // traveling spark on the way out
  if (draw < 1) {
    const q = bezierAt([src.x, src.y], c0, c1, dst, draw);
    circle(R, q[0], q[1], 3.5, { fill: C.amber, alpha: a, glow: 1 });
  }
  text(R, 'UNKNOWN → ASK', src.x + 24, src.y - 150, { f: 'MGW 500', size: 10, tracking: 3, color: C.amber, alpha: a * smooth(T.unknown + 0.2, T.unknown + 0.4, t) * (1 - back) });

  // the question card
  const ca = a * smooth(T.ask - 0.12, T.ask + 0.12, t);
  if (ca <= 0) return;
  const k = E.snap(seg(t, T.ask - 0.12, T.ask + 0.35));
  const y = CARD.y + 16 * (1 - k);
  rrect(R, CARD.x, y, CARD.w, CARD.h, 12, { color: C.amber, w: 1.2, alpha: ca, fill: C.bg0, fillAlpha: 0.88, glow: 0.35 });
  text(R, 'UNKNOWN · UNIT', CARD.x + 28, y + 36, { f: 'MGW 700', size: 10.5, tracking: 3.5, color: C.amber, alpha: ca, glow: 0.3 });
  text(R, 'amount_cents = 12000 — cents of which currency?', CARD.x + 28, y + 60, { f: 'MM 400', size: 13, color: C.mist, alpha: ca });
  text(R, 'EUR or USD?', CARD.x + 26, y + 118, { f: 'Geist 600', size: 46, tracking: -1.2, color: C.ink, alpha: ca, glow: 0.15 });
  const tap = smooth(T.answer, T.answer + 0.12, t);
  ['EUR', 'USD'].forEach((o, i) => {
    const bx = CARD.x + 28 + i * 118, by = y + 144;
    const on = i === 0 ? tap : 0;
    rrect(R, bx, by, 104, 40, 20, { color: on ? C.human : C.dim, w: 1.2, alpha: ca, fill: on ? C.human : null, fillAlpha: 0.95 * on });
    text(R, o, bx + 52, by + 26, { f: 'Geist 600', size: 17, color: on ? C.bg0 : C.mist, alpha: ca, align: 'center' });
  });
  // the human tap ripple
  const r = seg(t, T.answer, T.answer + 0.5);
  if (r > 0 && r < 1) circle(R, CARD.x + 28 + 52, y + 164, 20 + 60 * E.outCubic(r), { color: C.human, w: 1.2, alpha: ca * (1 - r) });
  text(R, 'never guess · safe incomplete > unsafe success', CARD.x + 270, y + 170, { f: 'MM 400', size: 11, color: C.amber, alpha: ca * 0.9 });
}
