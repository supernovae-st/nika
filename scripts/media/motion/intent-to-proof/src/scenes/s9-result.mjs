// S9 · RESULT + PROOF — not "Success ✓": the world changed, and here is
// the evidence. The number lands on an odometer; the receipt assembles from
// what can be checked — the engine's computed totals, the program hash that
// matches the certificate, the consent, the hash-chained trace — and the
// same seal that certified the plan now certifies the execution.
import { C, E, lerp, seg, smooth } from '../engine/core.mjs';
import { text, line, circle, rrect, rect, measure, check } from '../engine/render.mjs';
import { T } from '../timeline.mjs';
import { seal } from './shared.mjs';
import { hashes, short } from '../facts.mjs';
import { resultNode } from './s10-reveal.mjs';

const NUM = { f: 'Geist 600', size: 196, tracking: -6 };
const X0 = 150, BASE = 600;

export function env(t) {
  return { bgGlow: 1.1, gridAlpha: 0.22 * (1 - smooth(T.reveal, T.reveal + 0.5, t)), gridY: 0, gridX: 0, bgY: 520 };
}

// Where the whole composition goes as the camera pulls back (into the map).
function handoff(R, t) {
  const p = E.inOutCubic(seg(t, T.reveal, T.wide));
  if (p <= 0) return 1;
  const n = resultNode(t);
  const s = lerp(1, n.s, p);
  const ctx = R.ctx;
  ctx.translate(lerp(0, n.x - 960 * s, p), lerp(0, n.y - 540 * s, p));
  ctx.scale(s, s);
  return 1 - smooth(0.55, 1, p);
}

export function draw(R, t) {
  const a0 = smooth(T.result - 0.02, T.result + 0.12, t);
  if (a0 <= 0) return;
  const ctx = R.ctx;
  ctx.save();
  const k = handoff(R, t);
  const a = a0 * k;
  if (a > 0.005) {
    result(R, t, a);
    receipt(R, t, a);
  }
  ctx.restore();
}

// Odometer: each digit column rolls into place, staggered.
function result(R, t, a) {
  text(R, 'RESULT · THE REAL WORLD CHANGED', X0 + 6, 372, { f: 'MGW 500', size: 11, tracking: 4, color: C.dim, alpha: a * smooth(T.result, T.result + 0.2, t) });
  const chars = '€228.00'.split('');
  const dW = Math.max(...'0123456789'.split('').map(d => measure(d, NUM)));
  let x = X0;
  const ctx = R.ctx;
  chars.forEach((ch, i) => {
    const isDigit = /\d/.test(ch);
    const w = isDigit ? dW : measure(ch, NUM);
    if (!isDigit) {
      text(R, ch, x, BASE, { ...NUM, color: C.ink, alpha: a * smooth(T.result, T.result + 0.15, t), glow: 0.25 });
    } else {
      const target = +ch;
      const t0 = T.result + 0.02, t1 = T.result + 0.42 + i * 0.07;
      const p = E.snap(seg(t, t0, t1));
      const spins = 2 + i;
      const pos = target + 10 * spins * (1 - p); // scroll position (in digits)
      ctx.save();
      ctx.beginPath();
      ctx.rect(x - 4, BASE - NUM.size * 0.76, w + 8, NUM.size * 0.76 + 8);
      ctx.clip();
      const base = Math.floor(pos);
      for (let d = base - 1; d <= base + 1; d++) {
        const off = (d - pos) * NUM.size * 0.9;
        const dig = ((d % 10) + 10) % 10;
        const dw = measure(String(dig), NUM);
        const blur = Math.min(10, (1 - p) * 30);
        text(R, String(dig), x + (w - dw) / 2, BASE + off, { ...NUM, color: C.ink, alpha: a * smooth(T.result, T.result + 0.1, t), glow: 0.25, blur });
      }
      ctx.restore();
    }
    x += w + NUM.tracking;
  });
  const sub = smooth(T.result + 0.45, T.result + 0.7, t);
  text(R, 'PAID · 1 EFFECT · EXACTLY ONCE', X0 + 8, BASE + 56, { f: 'MGW 500', size: 12, tracking: 4, color: C.teal, alpha: a * sub, glow: 0.3 });
  const rows = [['ACME', '€120.00', ''], ['BRAVO', '€108.00', '€100.00 + €8.00 pending'], ['CIRRUS', '€95.00', 'rejected · not paid']];
  rows.forEach(([id, v, note], i) => {
    const y = BASE + 110 + i * 32;
    const rk = smooth(T.result + 0.55 + i * 0.08, T.result + 0.75 + i * 0.08, t);
    const rej = i === 2;
    text(R, id, X0 + 8, y, { f: 'MM 500', size: 16, color: rej ? C.dim : C.ink, alpha: a * rk });
    text(R, v, X0 + 250, y, { f: 'MM 500', size: 16, color: rej ? C.dim : C.ink, alpha: a * rk, align: 'right' });
    if (note) text(R, note, X0 + 272, y, { f: 'MM 400', size: 13, color: rej ? C.red : C.dim, alpha: a * rk * 0.9 });
    if (rej) line(R, X0 + 6, y - 6, X0 + 252, y - 6, { color: C.red, w: 1, alpha: a * rk * 0.7 });
  });
}

function receipt(R, t, a) {
  const k = smooth(T.receipt - 0.1, T.receipt + 0.15, t);
  if (k <= 0) return;
  const x = 1060, y = 236, w = 710, h = 640;
  const feed = E.lift(seg(t, T.receipt - 0.1, T.receipt + 0.45));
  rrect(R, x, y, w, h * feed, 14, { color: C.faint, w: 1, alpha: a * k, fill: '#050c19', fillAlpha: 0.94 });
  const vis = yy => (yy < y + h * feed - 6 ? 1 : 0);
  const x0 = x + 38, x1 = x + w - 38;
  if (vis(y + 110)) {
    text(R, 'PROOF', x0, y + 106, { f: 'Geist 700', size: 80, tracking: -2.5, color: C.ink, alpha: a * k, glow: 0.25 });
    text(R, 'EXECUTION RECEIPT · WHAT ACTUALLY HAPPENED', x0 + 3, y + 138, { f: 'MGW 500', size: 10, tracking: 3, color: C.teal, alpha: a * k });
  }
  line(R, x0, y + 166, x1, y + 166, { color: C.faint, w: 1, alpha: a * k * vis(y + 166) });
  const rows = [
    ['RESULT', '€228.00 · effects 1/1'],
    ['TOTALS', 'ACME 12000 · BRAVO 10800 (cents)'],
    ['PROGRAM', `${short(hashes.program_sha256, 8)} = certificate`],
    ['CONSENT', `proposal ${short(hashes.proposal_blake3, 8)} · rev 7`],
    ['TRACE', 'hash-chained · chain intact'],
  ];
  rows.forEach(([kk, v], i) => {
    const yy = y + 214 + i * 50;
    if (!vis(yy)) return;
    const tk = T.receiptRows[i];
    const rk = smooth(tk, tk + 0.08, t);
    text(R, kk, x0, yy, { f: 'MGW 500', size: 10.5, tracking: 2.6, color: C.dim, alpha: a * rk });
    text(R, v, x1 - 32, yy, { f: 'MM 500', size: 15, color: C.ink, alpha: a * rk, align: 'right' });
    check(R, x1 - 8, yy - 5, 11, E.snap(seg(t, tk, tk + 0.2)), { color: C.teal, w: 1.7, glow: 0.7, alpha: a });
    line(R, x0, yy + 18, x1, yy + 18, { color: C.line, w: 1, alpha: a * rk });
  });
  // verdict + seal: the same mark that certified the plan
  const vy = y + 214 + rows.length * 50 + 44;
  const vk = smooth(T.seal - 0.05, T.seal + 0.1, t);
  if (vis(vy)) {
    text(R, 'RECEIPT', x0, vy, { f: 'MGW 500', size: 10.5, tracking: 2.6, color: C.dim, alpha: a * vk });
    text(R, 'VERIFIED', x0 + 130, vy + 4, { f: 'MGW 700', size: 22, tracking: 8, color: C.teal, alpha: a * vk, glow: 0.6 });
  }
  const sp = E.snap(seg(t, T.receipt + 0.2, T.seal));
  const st = seg(t, T.seal, T.seal + 0.35);
  const ss = 1 + 0.3 * (1 - E.outBack(st)) * (t >= T.seal ? 1 : 0);
  if (vis(y + 90)) seal(R, x1 - 66, y + 86, 60 * ss, sp, { color: C.teal, alpha: a * k, t, label: 'NIKA · PROOF · VERIFIED · ' });
  if (t >= T.seal && st < 1) circle(R, x1 - 66, y + 86, 64 + 90 * E.outCubic(st), { color: C.teal, w: 1.6, alpha: a * (1 - st), glow: 1 });
}
