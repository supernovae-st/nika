// Shared visual constructs that travel between scenes.
import { C, E, TAU, clamp, seg, smooth } from '../engine/core.mjs';
import { text, line, poly, circle, measure } from '../engine/render.mjs';
import { ATOMS, atomX, OP, ARG, KIND, OP_Y, ARG_Y, KIND_Y } from './s1-intent.mjs';

// The six obligations as a row, drawn under a (cx, cy, s) transform that
// maps the S1 layout (centered on 960, OP_Y) to a docked position.
// opts: alpha, sockets (0..1 visibility of empty sockets), filled[i] (0..1)
export function atomRow(R, t, { cx = 960, cy = OP_Y, s = 1, alpha = 1, sockets = 1, glow = 0.2, only = null, hideArgs = false } = {}) {
  const ctx = R.ctx;
  ctx.save();
  ctx.translate(cx, cy);
  ctx.scale(s, s);
  ctx.translate(-960, -OP_Y);
  ATOMS.forEach((a, i) => {
    if (only && !only(i)) return;
    const x = atomX(i);
    text(R, `${String(i + 1).padStart(2, '0')}  ${a.kind}`, x, KIND_Y, { ...KIND, color: a.human ? C.human : C.dim, alpha: 0.9 * alpha, align: 'center' });
    text(R, a.op, x, OP_Y, { ...OP, color: a.human ? C.human : C.ink, alpha, align: 'center', glow });
    if (!hideArgs) {
      text(R, a.arg, x, ARG_Y, { ...ARG, color: C.mist, alpha, align: 'center' });
      if (a.socket && sockets > 0) {
        const aw = measure(a.arg, ARG);
        circle(R, x + aw / 2 + 14, ARG_Y - 6, 6.5, { color: C.ice, w: 1.1, alpha: 0.85 * alpha * sockets, dash: [2.2, 2.6], dashOffset: t * 14 });
      }
    }
  });
  for (let i = 0; i < 5; i++) {
    if (only && !(only(i) && only(i + 1))) continue;
    const xa = atomX(i) + measure(ATOMS[i].op, OP) / 2 + 20;
    const xb = atomX(i + 1) - measure(ATOMS[i + 1].op, OP) / 2 - 20;
    const y = OP_Y - 19;
    line(R, xa, y, xb, y, { color: C.ice, w: 1.2, alpha: 0.7 * alpha });
    poly(R, [[xb - 7, y - 4.5], [xb, y], [xb - 7, y + 4.5]], { color: C.ice, w: 1.2, alpha: 0.8 * alpha });
  }
  ctx.restore();
}

// A small instrument label with a leading tick.
export function tag(R, str, x, y, { color = C.dim, alpha = 1, size = 10.5, glow = 0 } = {}) {
  line(R, x, y - 4, x + 10, y - 4, { color, w: 1, alpha: alpha * 0.8 });
  text(R, str, x + 16, y, { f: 'MGW 500', size, tracking: 3, color, alpha, glow });
}

export { atomX };

// Guilloche seal: a procedural rosette like the security engraving on a
// banknote — the film's mark for "certified", drawn on with progress p.
export function seal(R, cx, cy, r, p, { color = C.teal, alpha = 1, t = 0, label = 'NIKA · READY · REV 7 · ', spin = 0.15 } = {}) {
  if (p <= 0 || alpha <= 0) return;
  const N = 18;
  for (let j = 0; j < N; j++) {
    const ph = (j / N) * TAU;
    const pts = [];
    for (let i = 0; i <= 160; i++) {
      const th = (i / 160) * TAU;
      const rr = r * (0.66 + 0.1 * Math.sin(7 * th + ph) + 0.05 * Math.sin(13 * th - 2 * ph));
      pts.push([cx + Math.cos(th + t * spin) * rr, cy + Math.sin(th + t * spin) * rr]);
    }
    poly(R, pts, { color, w: 0.6, alpha: alpha * 0.42, glow: 0.35 }, clamp(p * 1.3 - j * 0.012));
  }
  circle(R, cx, cy, r * 0.93, { color, w: 1, alpha: alpha * clamp(p * 2 - 0.4) * 0.8 });
  circle(R, cx, cy, r * 1.0, { color, w: 1.4, alpha: alpha * clamp(p * 2 - 0.6), glow: 0.4 });
  for (let k = 0; k < 96; k++) {
    const an = (k / 96) * TAU - t * spin * 0.5;
    const l = k % 8 === 0 ? 0.07 : 0.035;
    line(R, cx + Math.cos(an) * r * 1.02, cy + Math.sin(an) * r * 1.02, cx + Math.cos(an) * r * (1.02 + l), cy + Math.sin(an) * r * (1.02 + l), { color, w: 0.7, alpha: alpha * clamp(p * 2 - 0.8) * 0.8 });
  }
  // engraved legend around the rim
  const st = { f: 'MGW 500', size: r * 0.12, tracking: 0 };
  const rt = r * 1.2;
  let an = -Math.PI / 2 - t * spin * 0.5;
  const la = alpha * clamp(p * 2 - 1);
  if (la > 0) {
    const full = label.repeat(3);
    for (const ch of full) {
      const w = measure(ch, st) + r * 0.02;
      const a2 = an + w / 2 / rt;
      if (a2 > -Math.PI / 2 - t * spin * 0.5 + TAU - 0.02) break;
      const ctx = R.ctx;
      ctx.save();
      ctx.translate(cx + Math.cos(a2) * rt, cy + Math.sin(a2) * rt);
      ctx.rotate(a2 + Math.PI / 2);
      text(R, ch, 0, 0, { ...st, color, alpha: la * 0.9, align: 'center' });
      ctx.restore();
      an += w / rt;
    }
  }
}

// The film's title system: one big line + an instrument subline, top-left
// (or bottom-left for the principles). Consistent across beats.
export function beatTitle(R, t, t0, t1, main, sub, { x = 150, y = 206, accent = null, accentColor = C.cyan, size = 60 } = {}) {
  const a = Math.min(smooth(t0, t0 + 0.3, t), 1 - smooth(t1 - 0.3, t1, t));
  if (a <= 0) return;
  const k = E.snap(seg(t, t0, t0 + 0.55));
  const st = { f: 'Geist 600', size, tracking: -size * 0.028 };
  const yy = y + 14 * (1 - k);
  if (accent && main.endsWith(accent)) {
    const head = main.slice(0, main.length - accent.length);
    text(R, head, x, yy, { ...st, color: C.ink, alpha: a, glow: 0.18 });
    text(R, accent, x + measure(head, st), yy, { ...st, color: accentColor, alpha: a * smooth(t0 + 0.12, t0 + 0.35, t), glow: 0.4 });
  } else text(R, main, x, yy, { ...st, color: C.ink, alpha: a, glow: 0.18 });
  if (sub) text(R, sub, x + 2, y + 32, { f: 'MGW 500', size: 10.5, tracking: 3, color: accentColor, alpha: a * 0.9 * smooth(t0 + 0.15, t0 + 0.45, t) });
}
