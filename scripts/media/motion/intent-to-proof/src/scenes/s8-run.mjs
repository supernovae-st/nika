// S8 · RUN — the existing runtime, intentionally boring in the best way.
//
// Below the boundary nothing interprets language any more: the runtime
// executes the .nika bytes, wave by wave. Records flow and transform —
// the rejected invoice drops out, BRAVO's two invoices merge, totals are
// computed — and exactly one effect leaves the machine.
import { C, E, lerp, seg, smooth, win } from '../engine/core.mjs';
import { text, line, poly, circle, rrect, rect, measure, check, light, streak } from '../engine/render.mjs';
import { T } from '../timeline.mjs';
import { atomX } from './s1-intent.mjs';
import { RECORDS } from './s2-observe.mjs';
import { runDuration, hashes, short } from '../facts.mjs';

const TASKS = [
  ['read_invoices', 'nika:read'],
  ['keep_valid', 'nika:jq'],
  ['by_customer', 'nika:jq'],
  ['totals', 'nika:jq'],
  ['approval', 'nika:prompt'],
  ['pay', 'nika:fetch'],
];
const RAIL_Y = 612;
const BOUND_Y = 236;
const eur = c => `€${(c / 100).toFixed(2)}`;

function drop(t) {
  // the world above slides out as we descend below the boundary
  return E.inOutCubic(seg(t, T.consent + 0.05, T.waves[0] - 0.05));
}

export function env(t) {
  const d = drop(t);
  return { bgGlow: lerp(1, 0.75, d), gridAlpha: 0.2, gridY: -420 * d, gridX: 0, bgY: 700, glowStrength: 1 };
}

export function draw(R, t) {
  const d = drop(t);
  const out = 1 - smooth(T.effect + 0.08, T.result + 0.02, t);
  if (out <= 0 || d <= 0) return;
  const oy = lerp(700, 0, d); // the machine rises into frame
  const ctx = R.ctx;
  ctx.save();
  ctx.translate(0, oy);
  const a = out * smooth(0, 0.35, d);

  // the boundary
  const bk = E.snap(seg(t, T.consent + 0.15, T.consent + 0.7));
  line(R, 110, BOUND_Y, lerp(110, 1810, bk), BOUND_Y, { color: C.ice, w: 1.2, alpha: a * 0.8, glow: 0.5 });
  text(R, 'ABOVE · UNDERSTAND · PROVE · AUTHORIZE', 150, BOUND_Y - 16, { f: 'MGW 500', size: 10, tracking: 3, color: C.dim, alpha: a * bk });
  text(R, 'DETERMINISTIC EXECUTION · THE EXISTING NIKA RUNTIME', 150, BOUND_Y + 28, { f: 'MGW 500', size: 10, tracking: 3, color: C.ice, alpha: a * bk });
  text(R, `executes the .nika bytes · program ${short(hashes.program_sha256)} · no reinterpretation`, 1770, BOUND_Y + 28, { f: 'MM 400', size: 11.5, color: C.dim, alpha: a * bk, align: 'right' });

  // machine rail + precise grid
  for (let x = 150; x <= 1770; x += 40) line(R, x, RAIL_Y + 150, x, RAIL_Y + (x % 200 === 150 ? 162 : 156), { color: C.faint, w: 1, alpha: a * 0.8 });
  line(R, 150, RAIL_Y, 1770, RAIL_Y, { color: C.faint, w: 2, alpha: a });

  // the program drops through the open gate into the machine
  const pd = seg(t, T.consent + 0.1, T.waves[0]);
  if (pd > 0 && pd < 1) {
    const y = lerp(-120, RAIL_Y - 150, E.inCubic(pd));
    const x = lerp(1668, 150 + 60, E.inOutCubic(pd));
    rrect(R, x - 70, y - 22, 190, 44, 8, { color: C.ice, w: 1.2, alpha: a, fill: C.bg0, fillAlpha: 0.9, glow: 0.5 });
    text(R, 'invoice-payments.nika', x - 58, y + 5, { f: 'MM 500', size: 12, color: C.ink, alpha: a });
  }

  // stations
  TASKS.forEach(([id, tool], i) => {
    const x = atomX(i);
    const tw = T.waves[i];
    const run = smooth(tw - 0.14, tw, t);
    const done = smooth(tw, tw + 0.06, t);
    const human = id === 'approval', effect = id === 'pay';
    const col = human ? C.human : effect ? C.cyan : C.ice;
    const s = 142;
    rrect(R, x - s / 2, RAIL_Y - s / 2, s, s, 6, { color: done ? col : C.faint, w: done ? 1.6 : 1.2, alpha: a, fill: C.bg0, fillAlpha: 0.9, glow: done ? 0.5 : 0 });
    // inner progress (a precise fill, not a spinner)
    if (run > 0) rect(R, x - s / 2 + 8, RAIL_Y + s / 2 - 14, (s - 16) * run, 4, { fill: col, alpha: a * (1 - done * 0.5), glow: 0.6 });
    text(R, `W${i + 1}`, x - s / 2 + 10, RAIL_Y - s / 2 + 18, { f: 'MGW 700', size: 9.5, tracking: 2, color: done ? col : C.dim, alpha: a });
    text(R, id, x, RAIL_Y - s / 2 - 18, { f: 'MM 500', size: 17, color: C.ink, alpha: a, align: 'center' });
    text(R, tool, x, RAIL_Y - 4, { f: 'MM 400', size: 13.5, color: done ? col : C.dim, alpha: a, align: 'center' });
    if (done > 0) {
      check(R, x - 14, RAIL_Y + 30, 14, E.snap(seg(t, tw, tw + 0.2)), { color: human ? C.human : C.teal, w: 2, glow: 0.7, alpha: a });
      const dur = effect ? '1 effect' : human ? 'consent · rev 7' : runDuration(id);
      text(R, dur, x + 2, RAIL_Y + 36, { f: 'MM 500', size: 13, color: human ? C.human : C.mist, alpha: a * done });
    }
    if (i < 5) {
      const x2 = atomX(i + 1);
      const flow = seg(t, tw, T.waves[i + 1]);
      line(R, x + s / 2, RAIL_Y, x2 - s / 2, RAIL_Y, { color: done ? C.ice : C.faint, w: 1.4, alpha: a * (done ? 0.7 : 0.6) });
      if (flow > 0 && flow < 1) circle(R, lerp(x + s / 2, x2 - s / 2, flow), RAIL_Y, 3, { fill: C.cyan, alpha: a, glow: 1 });
    }
  });

  records(R, t, a);
  trace(R, t, a);

  // exactly one effect leaves the machine
  const ef = seg(t, T.effect, T.effect + 0.35);
  if (ef > 0) {
    const x = atomX(5);
    const beamTop = lerp(RAIL_Y - 60, -oy - 40, E.outExpo(ef));
    const fade = 1 - smooth(0.55, 1, ef);
    rect(R, x - 3, beamTop, 6, RAIL_Y - 60 - beamTop, { fill: C.cyan, alpha: a * fade, glow: 1 });
    rect(R, x - 18, beamTop, 36, RAIL_Y - 60 - beamTop, { fill: C.cyan, alpha: a * fade * 0.12, glow: 1 });
    light(R, x, RAIL_Y - 60, 260 * (1 - ef * 0.5), C.cyan, 0.5 * a * fade, 1);
    streak(R, x, RAIL_Y - 60, lerp(200, 1100, E.outCubic(ef)), 0.8 * a * (1 - ef), C.cyan, 1);
    text(R, 'POST payments · exactly once', x - 26, BOUND_Y - 44, { f: 'MM 500', size: 13, color: C.cyan, alpha: a * smooth(0.1, 0.3, ef), align: 'right', glow: 0.5 });
  }
  ctx.restore();

  // the principle
  const ta = win(t, T.waves[0] - 0.1, T.effect + 0.2, 0.2, 0.2);
  if (ta > 0) {
    const k = E.snap(seg(t, T.waves[0] - 0.1, T.waves[0] + 0.4));
    text(R, 'Nika', 150, 950 + 12 * (1 - k), { f: 'Geist 600', size: 64, tracking: -1.8, color: C.ink, alpha: ta * out, glow: 0.2 });
    text(R, 'executes.', 150 + measure('Nika ', { f: 'Geist 600', size: 64, tracking: -1.8 }), 950 + 12 * (1 - k), { f: 'Geist 600', size: 64, tracking: -1.8, color: C.ice, alpha: ta * out * smooth(T.waves[0], T.waves[0] + 0.2, t), glow: 0.35 });
  }
}

// Records flow along the rail and transform, wave by wave.
function records(R, t, a) {
  if (t < T.waves[0]) return;
  const cards = RECORDS.map((r, i) => ({ ...r, i }));
  const W = 136, H = 40;
  cards.forEach(c => {
    const rejected = c.status === 'rejected';
    // position timeline: after W1 → gap(1..2) → gap(2..3) → gap(3..4)
    const stage = t < T.waves[1] ? 0 : t < T.waves[2] ? 1 : t < T.waves[3] ? 2 : 3;
    const gapX = k => (atomX(k) + atomX(k + 1)) / 2;
    const u0 = E.inOutCubic(seg(t, T.waves[0], T.waves[0] + 0.2));
    let x = lerp(atomX(0), gapX(0), u0), y = RAIL_Y - 170 - c.i * (H + 10);
    let al = a * u0;
    if (stage >= 1) {
      const u = E.inOutCubic(seg(t, T.waves[1] - 0.05, T.waves[1] + 0.18));
      if (rejected) {
        // dropped by the rule, in place: it falls below the rail and never reaches the effect
        const f = seg(t, T.waves[1] - 0.05, T.waves[1] + 0.55);
        x = lerp(gapX(0), atomX(1), E.outCubic(f));
        y = lerp(y, RAIL_Y + 130, E.inOutCubic(f));
        al *= 1 - smooth(0.7, 1, f);
      } else {
        x = lerp(gapX(0), gapX(1), u);
        y = RAIL_Y - 170 - [0, 1, 2][c.i] * (H + 10);
      }
    }
    if (stage >= 2 && !rejected) {
      const u = E.inOutCubic(seg(t, T.waves[2] - 0.05, T.waves[2] + 0.18));
      x = lerp(gapX(1), gapX(2), u);
      // BRAVO's two invoices stack into one group
      const g = c.customer_id === 'ACME' ? 0 : 1;
      const within = c.i === 2 ? 1 : 0;
      y = lerp(y, RAIL_Y - 180 - g * (H + 30) + within * 7, u);
    }
    if (stage >= 3 && !rejected) {
      const u = E.inOutCubic(seg(t, T.waves[3] - 0.05, T.waves[3] + 0.18));
      x = lerp(gapX(2), gapX(3), u);
      al *= c.i === 2 ? 1 - u : 1; // merged into BRAVO's total
    }
    if (t >= T.waves[4] - 0.05 && !rejected) return; // totals travel on (drawn below)
    if (al <= 0.01) return;
    const col = rejected && stage >= 1 ? C.red : C.ice;
    rrect(R, x - W / 2, y - H / 2, W, H, 5, { color: col, w: 1, alpha: al, fill: C.bg0, fillAlpha: 0.9 });
    text(R, c.customer_id, x - W / 2 + 10, y + 2, { f: 'MM 500', size: 13, color: C.ink, alpha: al });
    text(R, eur(c.amount_cents), x + W / 2 - 10, y + 2, { f: 'MM 400', size: 13, color: C.mist, alpha: al, align: 'right' });
    text(R, c.status, x - W / 2 + 10, y + 15, { f: 'MM 400', size: 10, color: rejected ? C.red : C.dim, alpha: al * 0.9 });
    if (rejected && stage >= 1) text(R, 'status = "rejected" → dropped', x - W / 2, y + 44, { f: 'MM 500', size: 13, color: C.red, alpha: al * 0.95 });
  });
  // computed totals (the real engine's output for this data)
  if (t >= T.waves[3] + 0.05) {
    const u = E.snap(seg(t, T.waves[3] + 0.05, T.waves[3] + 0.3));
    const tot = [['ACME', 12000], ['BRAVO', 10800]];
    tot.forEach(([id, cents], g) => {
      const stage2 = seg(t, T.waves[4] - 0.05, T.waves[4] + 0.18);
      const stage3 = seg(t, T.waves[5] - 0.08, T.waves[5] + 0.05);
      const gapX = k => (atomX(k) + atomX(k + 1)) / 2;
      let x = gapX(3), y = RAIL_Y - 180 - g * 70;
      x = lerp(x, gapX(4), E.inOutCubic(stage2));
      x = lerp(x, atomX(5), E.inCubic(stage3));
      const al = a * u * (1 - smooth(0.6, 1, stage3));
      if (al <= 0.01) return;
      rrect(R, x - 80, y - 24, 160, 48, 7, { color: C.teal, w: 1.3, alpha: al, fill: C.bg0, fillAlpha: 0.92, glow: 0.4 });
      text(R, id, x - 68, y + 6, { f: 'MM 500', size: 15, color: C.ink, alpha: al });
      const shown = Math.round(cents * E.outCubic(u));
      text(R, eur(shown), x + 68, y + 6, { f: 'MM 500', size: 15, color: C.teal, alpha: al, align: 'right', glow: 0.3 });
    });
  }
}

// The trace: every wave leaves a hash-chained event.
function trace(R, t, a) {
  const y = RAIL_Y + 200;
  const k = smooth(T.waves[0] - 0.2, T.waves[0], t);
  if (k <= 0) return;
  text(R, 'TRACE · HASH-CHAINED EVENTS', 150, y - 22, { f: 'MGW 500', size: 10, tracking: 3, color: C.dim, alpha: a * k });
  line(R, 150, y, 1770, y, { color: C.faint, w: 1, alpha: a * k });
  const evs = [...T.waves];
  evs.forEach((tw, i) => {
    const e = smooth(tw, tw + 0.06, t);
    if (e <= 0) return;
    const x = atomX(i);
    line(R, x, y - 8, x, y + 8, { color: i === 5 ? C.cyan : C.teal, w: 1.6, alpha: a * e, glow: 0.6 });
    const kind = ['read', 'filter', 'group', 'sum', 'consent', 'effect'][i];
    text(R, `#${i + 1} ${kind}`, x + 8, y + 22, { f: 'MM 400', size: 10.5, color: i === 5 ? C.cyan : C.dim, alpha: a * e });
    if (i > 0) {
      const xp = atomX(i - 1);
      poly(R, [[xp + 70, y + 30], [lerp(xp + 70, x - 4, 0.5), y + 36], [x - 4, y + 30]], { color: C.teal, w: 0.8, alpha: a * e * 0.6 });
    }
  });
}
