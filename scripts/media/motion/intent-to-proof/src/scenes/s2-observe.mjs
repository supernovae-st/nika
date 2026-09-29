// S2 · OBSERVE — messy data becomes a clean evidence graph.
//
// JSON fragments stream through depth (depth of field, parallax). Then the
// observed world crystallizes: the four copies of each key merge into ONE
// field (schema induction), values snap into cells, types attach, and
// provenance lines lock each field to the records it was seen in. A
// plausible `id` field is proposed and refused — it was never observed.
import { C, E, lerp, seg, smooth, hash, noise1 } from '../engine/core.mjs';
import { createCanvas } from '@napi-rs/canvas';
import { text, line, poly, circle, rrect, measure, check, cross, light, passAlpha, font } from '../engine/render.mjs';
import { front, project, REF } from '../engine/cam.mjs';
import { background } from '../hud.mjs';
import { T } from '../timeline.mjs';
import { beatTitle } from './shared.mjs';

export const RECORDS = [
  { customer_id: 'ACME', status: 'approved', amount_cents: 12000, sku: 'A120' },
  { customer_id: 'BRAVO', status: 'approved', amount_cents: 10000, sku: 'B200' },
  { customer_id: 'BRAVO', status: 'pending', amount_cents: 800, sku: 'B310' },
  { customer_id: 'CIRRUS', status: 'rejected', amount_cents: 9500, sku: 'C045' },
];
export const FIELDS = ['customer_id', 'status', 'amount_cents', 'sku'];
export const TYPES = ['String', 'Enum · 3 values', 'Integer · cents', 'String'];

// Table geometry (z = 0 plane, front camera → screen 1:1).
const ROW_Y = [438, 506, 574, 642];
const LABEL_X = 700;
const CELL_X = [1010, 1180, 1350, 1520];
const DOT_X = 930;
const CHECK_X = 1700;
const FS = { f: 'MM 400', size: 17.5 };

const qv = v => (typeof v === 'number' ? String(v) : `"${v}"`);

// Depth-of-field sprites: a blurred fragment is rendered once per (text,
// color, quantized blur, device scale) and reused, instead of blurring
// hundreds of text layers per subframe (Skia's blur cost grows with area).
const SPRITE_SIZE = 48;
const BLUR_LEVELS = [1, 1.6, 2.4, 3.4, 4.6, 6, 8, 10.5, 14, 18, 24];
const sprites = new Map();
function sprite(str, color, blur, k) {
  const q = BLUR_LEVELS.reduce((a, b) => (Math.abs(b - blur) < Math.abs(a - blur) ? b : a));
  const key = `${str}|${color}|${q}|${k}`;
  let sp = sprites.get(key);
  if (!sp) {
    const st = { f: 'MM 400', size: SPRITE_SIZE };
    const pad = q * 3 + 4;
    const w = measure(str, st) + pad * 2, h = SPRITE_SIZE * 1.3 + pad * 2;
    const c = createCanvas(Math.ceil(w * k), Math.ceil(h * k));
    const ctx = c.getContext('2d');
    ctx.scale(k, k);
    ctx.filter = `blur(${(q * k).toFixed(2)}px)`;
    ctx.font = font(st.f, st.size);
    ctx.textBaseline = 'middle';
    ctx.fillStyle = color;
    ctx.fillText(str, pad, h / 2);
    sp = { c, w, h };
    sprites.set(key, sp);
  }
  return sp;
}
function blurredText(R, str, x, y, size, color, alpha, blur, glow) {
  const a = passAlpha(R, alpha, glow);
  if (!a) return;
  const s = size / SPRITE_SIZE;
  const sp = sprite(str, color, blur / s, R.k * Math.min(2, Math.max(0.5, s)));
  const ctx = R.ctx;
  ctx.globalAlpha = a;
  ctx.drawImage(sp.c, x - (sp.w * s) / 2, y - (sp.h * s) / 2, sp.w * s, sp.h * s);
  ctx.globalAlpha = 1;
}

// Fragments: structured (key / value) and noise.
let FR = null;
function fragments() {
  if (FR) return FR;
  FR = [];
  let n = 0;
  RECORDS.forEach((r, ri) => {
    FIELDS.forEach((f, fi) => {
      FR.push({ id: n++, kind: 'key', text: `"${f}":`, ri, fi });
      FR.push({ id: n++, kind: 'val', text: qv(r[f]), ri, fi, num: typeof r[f] === 'number' });
    });
    FR.push({ id: n++, kind: 'punct', text: '{' });
    FR.push({ id: n++, kind: 'punct', text: '},' });
  });
  const pool = ['"customer_id":', '"status":', '"approved"', '"amount_cents":', '12000', '"sku":', '"A120"', '"pending"', '10000', '"BRAVO"', '800', '"rejected"', '"CIRRUS"', '9500', '[', ']', '{', '},', ',', '"B310"', '"C045"', '"ACME"', '"amou', 'nt_cents"', 'null', ':'];
  for (let i = 0; i < 330; i++) {
    const s = pool[Math.floor(hash(i, 41) * pool.length)];
    FR.push({ id: n++, kind: 'noise', text: s });
  }
  // world start positions (a volume in front of the camera)
  for (const f of FR) {
    const h1 = hash(f.id, 1), h2 = hash(f.id, 2), h3 = hash(f.id, 3);
    f.p0 = [lerp(-700, 2620, h1), lerp(-380, 1460, h2), lerp(150, 3900, h3)];
    f.size = lerp(16, 40, hash(f.id, 4));
    f.bright = hash(f.id, 6) < 0.16;
    f.spin = (hash(f.id, 5) - 0.5) * 0.5;
  }
  return FR;
}

function fragColor(f) {
  if (f.kind === 'key') return C.ice;
  if (f.kind === 'val') return f.num ? C.ink : C.mist;
  if (f.kind === 'punct') return C.dim;
  return f.text.startsWith('"') && f.text.endsWith(':') ? C.ice : C.mist;
}

// Where a structured fragment settles in the table.
function target(f) {
  if (f.kind === 'key') return [LABEL_X - measure(FIELDS[f.fi], { f: 'MM 500', size: 19 }) / 2, ROW_Y[f.fi] - 6, 0];
  if (f.kind === 'val') return [CELL_X[f.ri] + measure(f.text, FS) / 2, ROW_Y[f.fi] - 6, 0];
  return null;
}

const cam = front();

// Storm position at time t (fragments stream toward the camera).
function stormPos(f, t) {
  const u = t - T.dive;
  const z = f.p0[2] - u * 760;
  const x = f.p0[0] + noise1(f.id * 0.37 + u * 0.6, 7) * 40;
  const y = f.p0[1] + noise1(f.id * 0.51 + u * 0.6, 9) * 30;
  return [x, y, z];
}

// Crystallization progress for a structured fragment.
function crystalP(f, t) {
  const tf = T.fields[f.fi] - 0.42 + f.ri * 0.035 + (f.kind === 'val' ? 0.04 : 0);
  return E.inOutCubic(seg(t, tf, tf + 0.55));
}

// The compacted evidence docks at the bottom of the frame for the plan.
export const DOCK = { s: 0.46, dx: -150, dy: 368 };
export function compact(t) {
  return E.inOutCubic(seg(t, T.rise, T.rise + 0.75));
}
// Screen position of a table point once docked.
export function dockPos(x, y) {
  return [960 + (x - 960) * DOCK.s + DOCK.dx, 540 + (y - 540) * DOCK.s + DOCK.dy];
}
// Field rows leave the dock when the plan pulls them into its sockets.
export const FIELD_SOCKET = { 1: 1, 0: 2, 2: 3 }; // field index → socket index
export function fieldAnchor(fi) {
  return dockPos(LABEL_X, ROW_Y[fi] - 6);
}
export const FILE_ANCHOR = () => dockPos(520 + 60, 342);
export const CAP_ANCHOR = () => dockPos(LABEL_X, 694);

export function env(t) {
  const c = compact(t);
  return {
    bgGlow: 1.15 - 0.25 * c,
    gridAlpha: smooth(T.observe + 0.2, T.observe + 0.8, t) * 0.8,
    gridY: 260 * E.inOutCubic(seg(t, T.rise, T.foundry + 0.3)),
    bgY: 560 + 200 * c,
  };
}

export function draw(R, t) {
  if (t < T.observe + 0.28) return; // before this the dive shows the world through the glyphs
  beatTitle(R, t, T.crystal - 0.05, T.rise + 0.35, 'Observed, not guessed.', 'FIELDS EXIST BECAUSE THEY WERE SEEN · `id` NEVER WAS', { accentColor: C.teal });
  const fade = 1 - smooth(T.alive, T.alive + 0.4, t);
  if (fade <= 0) return;
  R.ctx.globalAlpha = 1;
  drawWorld({ ...R, fade }, t, {});
}

export function drawWorld(R, t, { inMask }) {
  if (inMask) background(R, t, { bgGlow: 1.45, gridAlpha: 0.5 });
  const frs = fragments();
  const c = compact(t);
  const ctx = R.ctx;
  // global transform for the compaction move
  const cs = lerp(1, DOCK.s, c);
  const cx = lerp(0, DOCK.dx, c), cy = lerp(0, DOCK.dy, c);
  ctx.save();
  ctx.translate(960 + cx, 540 + cy);
  ctx.scale(cs, cs);
  ctx.translate(-960, -540);

  // volumetric light in the storm
  const pool = (1 - smooth(T.crystal, T.crystal + 0.8, t)) * smooth(T.dive, T.dive + 0.3, t);
  if (pool > 0) light(R, 960, 520, 820, C.blue, 0.22 * pool, 0.25);

  // ── fragments ────────────────────────────────────────────────────────
  for (const f of frs) {
    const sp = stormPos(f, t);
    let p = sp, a = 1, blurK = 1;
    let structured = f.kind === 'key' || f.kind === 'val';
    let cp = 0;
    if (structured) {
      cp = crystalP(f, t);
      const tg = target(f);
      p = [lerp(sp[0], tg[0], cp), lerp(sp[1], tg[1], cp), lerp(sp[2], tg[2], cp)];
      blurK = 1 - cp;
      // keys merge: 4 copies converge into one label, 3 of them fade
      if (f.kind === 'key' && f.ri > 0) a *= 1 - smooth(0.7, 1, cp);
      if (cp >= 1) continue; // settled: the table draws the crisp version
    } else {
      // noise dissolves as the world crystallizes
      const dz = hash(f.id, 11);
      a *= 1 - smooth(T.crystal - 0.1 + dz * 0.4, T.crystal + 0.45 + dz * 0.4, t);
      if (a <= 0.01) continue;
    }
    const pr = project(cam, p);
    if (!pr.ok || pr.z < 120) continue;
    const size = f.size * pr.s * lerp(1, 18 / f.size, cp);
    if (size < 3) continue;
    // depth of field around the table plane (z = 0 → view depth REF)
    const coc = Math.abs(pr.z - REF) / pr.z;
    const blur = Math.min(11, coc * 16) * blurK;
    // fade in on arrival and when rushing past the lens
    a *= smooth(T.dive - 0.2, T.dive + 0.25, t) * smooth(160, 520, pr.z) * smooth(5600, 4200, pr.z);
    if (a <= 0.01) continue;
    const col = cp > 0.5 && f.kind === 'key' ? C.ice : fragColor(f);
    const hot = f.bright || f.kind === 'val';
    const fa = a * (f.kind === 'noise' ? (f.bright ? 0.95 : 0.72) : 0.95);
    const glow = hot ? 0.3 * (1 - blurK * 0.4) : 0;
    const fc = f.bright ? C.ink : col;
    ctx.save();
    ctx.translate(pr.x, pr.y);
    ctx.rotate(f.spin * (1 - cp) * 0.3);
    if (blur > 0.8) blurredText(R, f.text, 0, 0, size, fc, fa, blur, glow);
    else text(R, f.text, 0, 0, { f: 'MM 400', size, color: fc, alpha: fa, align: 'center', baseline: 'middle', glow });
    ctx.restore();
  }

  // ── the evidence table ───────────────────────────────────────────────
  const tb = smooth(T.crystal - 0.2, T.crystal + 0.2, t);
  if (tb > 0) {
    text(R, 'OBSERVED WORLD', 520, 348, { f: 'MGW 500', size: 13, tracking: 5, color: C.ice, alpha: tb, glow: 0.3 });
    text(R, 'invoices.json · 4 records · read under the project root', 520, 372, { f: 'MM 400', size: 11.5, color: C.dim, alpha: tb * 0.95 });
    RECORDS.forEach((r, ri) => {
      text(R, `#${ri + 1}`, CELL_X[ri], 404, { f: 'MM 400', size: 10.5, tracking: 1, color: C.dim, alpha: tb * 0.8 });
    });
    line(R, 520, 386, 1740, 386, { color: C.faint, w: 1, alpha: tb * 0.8 });
  }
  FIELDS.forEach((fname, fi) => {
    const tf = T.fields[fi];
    const lock = smooth(tf, tf + 0.12, t);
    const k = E.snap(seg(t, tf, tf + 0.45));
    const y = ROW_Y[fi];
    // crisp settled content
    const settledKey = crystalP({ fi, ri: 0, kind: 'key' }, t) >= 1;
    if (settledKey) text(R, fname, LABEL_X, y, { f: 'MM 500', size: 19, color: C.ice, alpha: 1, align: 'center', glow: 0.3 * (1 - k) + 0.15 });
    RECORDS.forEach((r, ri) => {
      if (crystalP({ fi, ri, kind: 'val' }, t) >= 1) text(R, qv(r[fname]), CELL_X[ri], y, { ...FS, color: typeof r[fname] === 'number' ? C.ink : C.mist, alpha: 1 });
    });
    if (lock <= 0) return;
    // type badge
    const tw = measure(TYPES[fi], { f: 'MM 400', size: 11.5 });
    const bx = LABEL_X + 92, by = y - 17;
    rrect(R, bx, by, tw + 18, 22, 5, { color: C.faint, w: 1, alpha: k, fill: C.bg1, fillAlpha: 0.6 });
    text(R, TYPES[fi], bx + 9, y - 2, { f: 'MM 400', size: 11.5, color: C.mist, alpha: k });
    // provenance: the field fans out to every cell it was observed in
    circle(R, DOT_X, y - 6, 3.2, { fill: C.ice, alpha: k, glow: 0.8 });
    RECORDS.forEach((_, ri) => {
      const pk = E.snap(seg(t, tf + 0.03 * ri, tf + 0.3 + 0.03 * ri));
      const x1 = CELL_X[ri] - 12;
      poly(R, [[DOT_X + 4, y - 6], [lerp(DOT_X + 4, x1, 0.4), y - 6 - 12 + ri * 6], [x1, y - 6]], { color: C.ice, w: 0.8, alpha: 0.35 * pk }, pk);
    });
    // observed ✓
    check(R, CHECK_X, y - 7, 13, E.snap(seg(t, tf + 0.15, tf + 0.4)), { color: C.teal, w: 2, glow: 0.8, alpha: 1 });
    text(R, 'OBSERVED', CHECK_X + 16, y - 2, { f: 'MGW 500', size: 9.5, tracking: 2.2, color: C.teal, alpha: k * 0.9 });
    // lock flash
    const fl = seg(t, tf, tf + 0.35);
    if (fl > 0 && fl < 1) line(R, 520, y + 14, lerp(520, 1760, E.outCubic(fl)), y + 14, { color: C.ice, w: 1, alpha: 0.5 * (1 - fl), glow: 1 });
  });

  // the currency seed: amount_cents names no currency
  const cs2 = smooth(T.currencySeed, T.currencySeed + 0.3, t);
  if (cs2 > 0) {
    const y = ROW_Y[2];
    const x = LABEL_X + 92;
    const pulse = 0.75 + 0.25 * Math.sin(t * 7);
    line(R, x + 8, y + 5, x + 8, y + 13, { color: C.amber, w: 1, alpha: cs2 * 0.8 });
    rrect(R, x, y + 13, 96, 21, 5, { color: C.amber, w: 1, alpha: cs2 * pulse, glow: 0.5 });
    text(R, 'currency ?', x + 9, y + 28, { f: 'MM 400', size: 11.5, color: C.amber, alpha: cs2, glow: 0.4 });
  }

  // project state is observed too: the permitted destination for effects
  const capA = smooth(T.ghostNo + 0.2, T.ghostNo + 0.45, t);
  if (capA > 0) {
    const y = 700;
    text(R, 'capability', LABEL_X, y, { f: 'MM 500', size: 15, color: C.ice, alpha: capA * 0.9, align: 'center' });
    text(R, 'nika:fetch → payments.example.invalid · permitted in net.http', CELL_X[0] - 12, y, { f: 'MM 400', size: 14, color: C.mist, alpha: capA * 0.9 });
    check(R, CHECK_X, y - 6, 12, E.snap(seg(t, T.ghostNo + 0.25, T.ghostNo + 0.5)), { color: C.teal, w: 1.8, glow: 0.6 });
  }

  // the refused field: plausible, familiar — and never observed
  const g0 = T.ghost, g1 = T.ghostNo;
  const ga = smooth(g0, g0 + 0.15, t) * (1 - smooth(g1 + 0.35, g1 + 0.6, t));
  if (ga > 0) {
    const y = 752;
    const jitter = (1 - smooth(g1 - 0.05, g1, t)) * Math.sin(t * 90) * 1.5;
    const col = t < g1 ? C.dim : C.red;
    rrect(R, LABEL_X - 36 + jitter, y - 22, 72, 30, 6, { color: C.dim, w: 1, alpha: ga * 0.9, dash: [3, 3], dashOffset: t * 20 });
    text(R, 'id', LABEL_X + jitter, y - 1, { f: 'MM 500', size: 19, color: C.dim, alpha: ga, align: 'center' });
    RECORDS.forEach((_, ri) => text(R, '—', CELL_X[ri], y - 1, { ...FS, color: C.faint, alpha: ga }));
    const no = smooth(g1, g1 + 0.12, t);
    if (no > 0) {
      cross(R, CHECK_X, y - 7, 11, E.snap(seg(t, g1, g1 + 0.2)), { color: C.red, w: 1.8, alpha: ga * 0.9 });
      text(R, 'NOT OBSERVED', CHECK_X + 16, y - 2, { f: 'MGW 500', size: 9.5, tracking: 2.2, color: col, alpha: ga * no * 0.9 });
      line(R, LABEL_X - 40, y - 7, lerp(LABEL_X - 40, 1640, E.snap(seg(t, g1, g1 + 0.3))), y - 7, { color: C.red, w: 1, alpha: ga * 0.55 });
    }
  }

  ctx.restore();

  // dock label (the evidence that will ground the plan)
  const lab = smooth(T.rise + 0.4, T.rise + 0.7, t);
  if (lab > 0) {
    const [lx, ly] = dockPos(520, 300);
    text(R, 'EVIDENCE', lx, ly, { f: 'MGW 500', size: 10.5, tracking: 4, color: C.teal, alpha: lab * 0.9 });
  }
}
