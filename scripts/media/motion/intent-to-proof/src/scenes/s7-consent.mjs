// S7 · READY → REVIEW → CONSENT.
//
// READY is a certification, not a green button: it prints from the proofs,
// bound to the plan and the candidate program by hash. Then the human.
// A question is typed DISCUSS — it cannot open the gate. Only consent,
// bound to the exact proposal shown, opens it. Intelligence is not authority.
import { C, E, lerp, seg, smooth, win } from '../engine/core.mjs';
import { text, line, poly, circle, rrect, rect, measure, check, light } from '../engine/render.mjs';
import { T } from '../timeline.mjs';
import { seal } from './shared.mjs';
import { hashes, short } from '../facts.mjs';

const CERT = { x: 660, y: 150, w: 600, h: 760 };
const ROWS = [
  ['OBLIGATIONS', '6/6 realized'],
  ['GROUNDED FIELDS', '3/3 observed'],
  ['TYPES · UNITS', 'proven · Money<EUR·cents>'],
  ['EFFECTS', '1 · POST payments'],
  ['GATE', 'human · before the effect'],
  ['PERMITS', 'fs 1 · net 1 · tools 4'],
  ['UNKNOWNS', '0 open · 1 answered'],
  ['NIKA CHECK', 'pass · 0 errors'],
  ['MEANING', '6/6 represented'],
];
const GATE_X = 1668;

function certShift(t) {
  return E.inOutCubic(seg(t, T.question - 0.25, T.question + 0.2));
}

export function env(t) {
  return { bgGlow: 1.05, gridAlpha: 0.28, gridX: -200 * certShift(t), gridY: 0, bgY: 560 };
}

export function draw(R, t) {
  const out = 1 - smooth(T.consent + 0.45, T.consent + 0.75, t);
  if (out <= 0) return;
  const lift = E.inOutCubic(seg(t, T.consent + 0.05, T.waves[0] - 0.05)); // same move as the runtime's rise
  R.ctx.save();
  R.ctx.translate(0, -700 * lift);
  certificate(R, t, out);
  review(R, t, out);
  gate(R, t);
  R.ctx.restore();
  principle(R, t);
}

// The principle stays put while the world moves: it hands over in place.
function principle(R, t) {
  const ha = win(t, T.consent - 0.15, T.consent + 0.75, 0.2, 0.2);
  if (ha > 0) {
    const k = E.snap(seg(t, T.consent - 0.15, T.consent + 0.35));
    text(R, 'Humans', 150, 930 + 12 * (1 - k), { f: 'Geist 600', size: 64, tracking: -1.8, color: C.ink, alpha: ha, glow: 0.2 });
    text(R, 'authorize.', 150 + measure('Humans ', { f: 'Geist 600', size: 64, tracking: -1.8 }), 930 + 12 * (1 - k), { f: 'Geist 600', size: 64, tracking: -1.8, color: C.human, alpha: ha * smooth(T.consent - 0.05, T.consent + 0.15, t), glow: 0.35 });
    text(R, 'NO SCORE, JUDGEMENT OR MODEL CONFIDENCE CAN OPEN THIS GATE', 152, 962, { f: 'MGW 500', size: 10.5, tracking: 3, color: C.human, alpha: ha * 0.85 });
  }
}

function certificate(R, t, out) {
  const appear = smooth(T.ready - 0.05, T.ready + 0.15, t);
  if (appear <= 0) return;
  const sh = certShift(t);
  const ctx = R.ctx;
  ctx.save();
  const s = lerp(1, 0.82, sh);
  ctx.translate(lerp(0, -470, sh), lerp(0, 40, sh));
  ctx.translate(CERT.x, CERT.y);
  ctx.scale(s, s);
  ctx.translate(-CERT.x, -CERT.y);
  const a = appear * out;
  // the paper feeds out of the closure: a printer reveal from the top
  const feed = E.lift(seg(t, T.ready, T.ready + 0.5));
  const h = CERT.h * feed;
  rrect(R, CERT.x, CERT.y, CERT.w, h, 14, { color: C.faint, w: 1, alpha: a, fill: '#050c19', fillAlpha: 0.94 });
  line(R, CERT.x + 1, CERT.y + h, CERT.x + CERT.w - 1, CERT.y + h, { color: C.teal, w: 1.4, alpha: a * (1 - smooth(0.95, 1, feed)), glow: 1 });
  const clipY = CERT.y + h;
  const vis = y => (y < clipY - 4 ? 1 : 0);
  const x0 = CERT.x + 40, x1 = CERT.x + CERT.w - 40;
  // title + seal
  if (vis(CERT.y + 110)) {
    text(R, 'READY', x0, CERT.y + 112, { f: 'Geist 700', size: 86, tracking: -3, color: C.ink, alpha: a, glow: 0.25 });
    text(R, 'COMPILE STATUS · READY · REVISION 7', x0 + 2, CERT.y + 142, { f: 'MGW 500', size: 10, tracking: 3, color: C.teal, alpha: a });
  }
  const sp = E.snap(seg(t, T.ready + 0.15, T.stamp));
  const stampK = seg(t, T.stamp, T.stamp + 0.35);
  const ss = 1 + 0.25 * (1 - E.outBack(stampK)) * (t >= T.stamp ? 1 : 0);
  if (vis(CERT.y + 100)) seal(R, x1 - 58, CERT.y + 92, 56 * ss, sp, { color: C.teal, alpha: a, t, label: 'NIKA · READY · REV 7 · ' });
  if (t >= T.stamp && stampK < 1) circle(R, x1 - 58, CERT.y + 92, 60 + 70 * E.outCubic(stampK), { color: C.teal, w: 1.5, alpha: a * (1 - stampK), glow: 1 });
  line(R, x0, CERT.y + 170, x1, CERT.y + 170, { color: C.faint, w: 1, alpha: a * vis(CERT.y + 170) });
  // rows print on 32nd notes
  ROWS.forEach(([k, v], i) => {
    const y = CERT.y + 214 + i * 42;
    if (!vis(y)) return;
    const tk = T.ready + 0.12 + i * 0.045;
    const rk = smooth(tk, tk + 0.1, t);
    text(R, k, x0, y, { f: 'MGW 500', size: 10.5, tracking: 2.6, color: C.dim, alpha: a * rk });
    text(R, v, x1 - 30, y, { f: 'MM 500', size: 14, color: C.ink, alpha: a * rk, align: 'right' });
    check(R, x1 - 8, y - 5, 10, E.snap(seg(t, tk + 0.05, tk + 0.25)), { color: C.teal, w: 1.6, glow: 0.6, alpha: a });
    line(R, x0, y + 16, x1, y + 16, { color: C.line, w: 1, alpha: a * rk * 0.8 });
  });
  const hy = CERT.y + 214 + ROWS.length * 42 + 22;
  const hk = smooth(T.ready + 0.5, T.ready + 0.62, t) * vis(hy + 60);
  [['plan_sha256', short(hashes.plan_sha256, 16)], ['candidate_sha256', short(hashes.program_sha256, 16)], ['revision', '7']].forEach(([k, v], i) => {
    text(R, k, x0, hy + i * 24, { f: 'MM 400', size: 12, color: C.dim, alpha: a * hk });
    text(R, v, x1, hy + i * 24, { f: 'MM 400', size: 12, color: C.mist, alpha: a * hk, align: 'right' });
  });
  ctx.restore();
}

function bubble(R, t, t0, who, str, y, opts = {}) {
  const k = E.snap(seg(t, t0, t0 + 0.3));
  if (k <= 0) return 0;
  const human = who === 'YOU';
  const x = 800;
  text(R, who, x, y - (human ? 52 : 40), { f: 'MGW 500', size: 10.5, tracking: 4, color: human ? C.human : C.ice, alpha: k * 0.9 });
  if (human) text(R, str, x, y + 8 * (1 - k), { f: 'Geist 400', size: opts.size || 46, tracking: -0.9, color: C.human, alpha: k });
  else {
    line(R, x - 16, y - 26, x - 16, y + 34, { color: C.ice, w: 2, alpha: k, glow: 0.6 });
    text(R, str, x, y, { f: 'MM 500', size: 19, color: C.ink, alpha: k, glow: 0.1 });
    if (opts.sub) text(R, opts.sub, x, y + 28, { f: 'MM 400', size: 13, color: C.dim, alpha: k });
  }
  return k;
}

function review(R, t, out) {
  const a = out * smooth(T.question - 0.15, T.question + 0.1, t);
  if (a <= 0) return;
  const RA = { ...R, fade: a };
  // the question
  bubble(RA, t, T.question, 'YOU', 'What will it do exactly?', 292);
  // typed by the session: a question is not an authorization
  const dk = smooth(T.discuss, T.discuss + 0.15, t);
  if (dk > 0) {
    rrect(RA, 800, 326, 118, 30, 15, { color: C.human, w: 1.2, alpha: dk });
    text(RA, 'DISCUSS', 859, 346, { f: 'MGW 700', size: 11, tracking: 3, color: C.human, alpha: dk, align: 'center' });
    text(RA, 'a question is not an authorization', 934, 346, { f: 'MM 500', size: 15, color: C.human, alpha: dk });
  }
  // Nika explains with the exact proposal
  bubble(RA, t, T.preview, 'NIKA', '1 payment · ACME €120.00 · BRAVO €108.00 · €228.00', 450, { sub: `POST payments.example.invalid · proposal ${short(hashes.proposal_blake3, 8)} · rev 7` });
  // consent
  bubble(RA, t, T.approve, 'YOU', 'Approve.', 612, { size: 64 });
  const ck = smooth(T.approve + 0.25, T.approve + 0.4, t);
  if (ck > 0) {
    rrect(RA, 800, 646, 126, 30, 15, { color: C.human, w: 1, alpha: ck, fill: C.human, fillAlpha: 0.95 });
    text(RA, 'CONSENT', 863, 666, { f: 'MGW 700', size: 11, tracking: 3, color: C.bg0, alpha: ck, align: 'center' });
    text(RA, `bound to proposal ${short(hashes.proposal_blake3, 8)} · rev 7`, 944, 666, { f: 'MM 500', size: 15, color: C.human, alpha: ck * 0.95 });
    text(RA, 'a changed workflow needs new consent', 944, 690, { f: 'MM 400', size: 12.5, color: C.dim, alpha: ck * 0.9 });
  }
}

// The consent gate: two matte, warm bars. Physical, not glowing intelligence.
export function gate(R, t) {
  const a = smooth(T.question - 0.1, T.question + 0.25, t) * (1 - smooth(T.consent + 0.5, T.consent + 0.9, t));
  if (a <= 0) return;
  const open = E.snap(seg(t, T.consent - 0.04, T.consent + 0.3));
  const hold = win(t, T.discuss, T.approve, 0.1, 0.2);
  const gx = GATE_X, top = 210, bot = 830;
  const gap = 5 + 200 * open;
  // a shaft of warm light through the opening
  if (open > 0 && !R.glowPass) {
    const ctx = R.ctx;
    const g = ctx.createLinearGradient(gx - gap, 0, gx + gap, 0);
    g.addColorStop(0, 'rgba(255,241,222,0)');
    g.addColorStop(0.5, `rgba(255,241,222,${(0.22 * open * a).toFixed(3)})`);
    g.addColorStop(1, 'rgba(255,241,222,0)');
    ctx.globalCompositeOperation = 'lighter';
    ctx.fillStyle = g;
    ctx.fillRect(gx - gap, top, gap * 2, bot - top);
    ctx.globalCompositeOperation = 'source-over';
  }
  if (open > 0) {
    rect(R, gx - 1, top, 2, bot - top, { fill: C.human, alpha: 0.5 * open * a * (1 - open * 0.5), glow: 1 });
    light(R, gx, (top + bot) / 2, 300 * open, C.human, 0.22 * open * a, 0.8);
  }
  for (const side of [-1, 1]) {
    const x = gx + side * gap;
    rect(R, x - 3, top, 6, bot - top, { fill: C.human, alpha: a * 0.95 });
    line(R, x + side * 14, top + 20, x + side * 14, bot - 20, { color: C.human, w: 1, alpha: a * 0.35 });
  }
  // lock while a question is being discussed
  if (hold > 0) {
    const y = (top + bot) / 2;
    rrect(R, gx - 16, y - 12, 32, 24, 5, { color: C.human, w: 1.2, alpha: hold * a, fill: C.bg0, fillAlpha: 1 });
    poly(R, [[gx - 8, y - 12], [gx - 8, y - 20], [gx + 8, y - 20], [gx + 8, y - 12]], { color: C.human, w: 1.2, alpha: hold * a });
  }
  text(R, 'CONSENT GATE', gx, top - 26, { f: 'MGW 500', size: 10, tracking: 3.5, color: C.human, alpha: a * 0.9, align: 'center' });
  text(R, open > 0.5 ? 'OPEN · REV 7' : 'CLOSED', gx, bot + 32, { f: 'MGW 500', size: 10, tracking: 3.5, color: C.human, alpha: a * 0.9, align: 'center' });
}
