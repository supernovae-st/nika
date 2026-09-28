// Render context + drawing primitives.
//
// Every scene draws twice per frame: a MAIN pass at full resolution and a
// GLOW pass into a quarter-resolution emissive buffer. Only elements that
// declare `glow > 0` emit light; the glow buffer is blurred at two radii and
// added back. Glow is therefore a semantic property of an element, never a
// global filter smeared over the frame.
import { createCanvas, GlobalFonts } from '@napi-rs/canvas';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { C, rgba, clamp, TAU, lerp } from './core.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const ROOT = path.resolve(HERE, '../..');
export const DW = 1920;
export const DH = 1080;

let fontsReady = false;
export function loadFonts() {
  if (fontsReady) return;
  const dir = path.join(ROOT, '.cache/fonts');
  if (!fs.existsSync(dir)) throw new Error('Fonts missing: run `python3 tools/build-fonts.py`');
  for (const f of fs.readdirSync(dir)) {
    if (f.endsWith('.ttf')) GlobalFonts.registerFromPath(path.join(dir, f), f.replace('.ttf', '').replace('-', ' '));
  }
  fontsReady = true;
}

// ── surfaces ───────────────────────────────────────────────────────────
export function createSurfaces(scale) {
  const W = Math.round(DW * scale), H = Math.round(DH * scale);
  const gw = Math.round(W / 4), gh = Math.round(H / 4);
  const main = createCanvas(W, H);
  const glow = createCanvas(gw, gh);
  const blurA = createCanvas(gw, gh);
  const blurB = createCanvas(gw, gh);
  const tmp = createCanvas(W, H);
  const tmpGlow = createCanvas(gw, gh);
  return { W, H, gw, gh, scale, main, glow, blurA, blurB, tmp, tmpGlow };
}

export function makeR(surf, pass, t) {
  const canvas = pass === 'glow' ? surf.glow : surf.main;
  const ctx = canvas.getContext('2d');
  const k = pass === 'glow' ? surf.scale / 4 : surf.scale;
  ctx.setTransform(k, 0, 0, k, 0, 0);
  ctx.globalAlpha = 1;
  ctx.globalCompositeOperation = 'source-over';
  ctx.filter = 'none';
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = 'high';
  return { ctx, pass, glowPass: pass === 'glow', k, t, surf, audit: pass === 'main' ? surf.audit : undefined };
}

// Additive bloom from the emissive buffer.
export function compositeGlow(surf, strength = 1) {
  const { main, glow, blurA, blurB, W, H, scale } = surf;
  const a = blurA.getContext('2d'), b = blurB.getContext('2d');
  a.setTransform(1, 0, 0, 1, 0, 0); b.setTransform(1, 0, 0, 1, 0, 0);
  a.clearRect(0, 0, blurA.width, blurA.height);
  b.clearRect(0, 0, blurB.width, blurB.height);
  a.filter = `blur(${(1.6 * scale).toFixed(2)}px)`;
  a.drawImage(glow, 0, 0);
  a.filter = 'none';
  b.filter = `blur(${(7 * scale).toFixed(2)}px)`;
  b.drawImage(glow, 0, 0);
  b.filter = 'none';
  const m = main.getContext('2d');
  m.setTransform(1, 0, 0, 1, 0, 0);
  m.globalCompositeOperation = 'lighter';
  m.globalAlpha = clamp(0.9 * strength, 0, 1);
  m.drawImage(blurA, 0, 0, W, H);
  m.globalAlpha = clamp(0.75 * strength, 0, 1);
  m.drawImage(blurB, 0, 0, W, H);
  m.globalAlpha = 1;
  m.globalCompositeOperation = 'source-over';
}

// ── style helpers ──────────────────────────────────────────────────────
export const font = (f, size) => `${size}px "${f}"`;

function setText(ctx, st) {
  ctx.font = font(st.f || 'Geist 500', st.size || 20);
  ctx.letterSpacing = `${st.tracking || 0}px`;
  ctx.textAlign = st.align || 'left';
  ctx.textBaseline = st.baseline || 'alphabetic';
}

// Visible in this pass? Returns the effective alpha or 0.
export function passAlpha(R, alpha = 1, glow = 0) {
  if (R.fade !== undefined) alpha *= R.fade;
  if (alpha <= 0.002) return 0;
  if (R.glowPass) return glow > 0 ? alpha * glow : 0;
  return alpha;
}

// ── measurement (pass-independent, cached) ─────────────────────────────
const mctx = createCanvas(8, 8).getContext('2d');
const mcache = new Map();
export function measure(str, st) {
  const key = `${st.f}|${st.size}|${st.tracking || 0}|${str}`;
  let w = mcache.get(key);
  if (w === undefined) {
    setText(mctx, { ...st, align: 'left' });
    w = mctx.measureText(str).width;
    // letterSpacing adds trailing space after the last glyph: remove it
    if (st.tracking && str.length) w -= st.tracking;
    mcache.set(key, w);
  }
  return w;
}
// Per-glyph x offsets (kerning-aware, via prefix widths).
const gcache = new Map();
export function glyphs(str, st) {
  const key = `${st.f}|${st.size}|${st.tracking || 0}|${str}`;
  let g = gcache.get(key);
  if (!g) {
    g = [];
    const chars = [...str];
    let prefix = '';
    for (let i = 0; i < chars.length; i++) {
      const x = prefix.length ? measure(prefix, st) + (st.tracking || 0) : 0;
      prefix += chars[i];
      const w = measure(chars[i], st);
      g.push({ ch: chars[i], x, w, i });
    }
    gcache.set(key, g);
  }
  return g;
}

// ── text ───────────────────────────────────────────────────────────────
// st: { f, size, color, alpha, tracking, align, baseline, glow, blur }
export function text(R, str, x, y, st = {}) {
  const a = passAlpha(R, st.alpha ?? 1, st.glow || 0);
  if (!a || !str) return;
  const ctx = R.ctx;
  if (R.audit) R.audit(str, x, y, st, a, ctx.getTransform()); // tools/readability.mjs
  if (st.blur && st.blur > 0.25 && !R.glowPass) return blurredText(R, str, x, y, st, a);
  setText(ctx, st);
  ctx.globalAlpha = a;
  ctx.fillStyle = st.color || C.ink;
  ctx.fillText(str, x, y);
  ctx.globalAlpha = 1;
}

// A canvas filter here blurs a FULL-canvas layer per draw (~1.2 s at 4K),
// so blurred text is rendered into a tight offscreen canvas at the current
// device scale and composited. Same radius semantics: st.blur × R.k device px.
function blurredText(R, str, x, y, st, a) {
  const ctx = R.ctx;
  const m = ctx.getTransform();
  const kx = Math.hypot(m.a, m.b) || 1;
  const size = st.size || 20;
  const w = measure(str, st), h = size * 1.5;
  const bpx = st.blur * R.k;
  const pad = (bpx * 2.6) / kx + 2;
  const ax = st.align === 'center' ? w / 2 : st.align === 'right' ? w : 0;
  const bl = st.baseline || 'alphabetic';
  const ay = bl === 'middle' ? h / 2 : bl === 'top' ? 0.12 * size : size * 1.05;
  const cw = Math.ceil((w + 2 * pad) * kx), ch = Math.ceil((h + 2 * pad) * kx);
  if (cw < 1 || ch < 1 || cw * ch > 24e6) return;
  const c = createCanvas(cw, ch);
  const x2 = c.getContext('2d');
  x2.scale(kx, kx);
  setText(x2, { ...st, align: 'left', baseline: bl });
  x2.filter = `blur(${bpx.toFixed(2)}px)`;
  x2.fillStyle = st.color || C.ink;
  x2.fillText(str, pad, pad + ay);
  ctx.globalAlpha = a;
  ctx.drawImage(c, x - ax - pad, y - ay - pad, cw / kx, ch / kx);
  ctx.globalAlpha = 1;
}

// ── lines & shapes ─────────────────────────────────────────────────────
function strokeStyle(ctx, st, a) {
  ctx.globalAlpha = a;
  ctx.strokeStyle = st.color || C.ice;
  ctx.lineWidth = st.w || 1;
  ctx.lineCap = st.cap || 'round';
  ctx.lineJoin = st.join || 'round';
  if (st.dash) {
    ctx.setLineDash(st.dash);
    ctx.lineDashOffset = st.dashOffset || 0;
  } else ctx.setLineDash([]);
}

export function line(R, x0, y0, x1, y1, st = {}) {
  const a = passAlpha(R, st.alpha ?? 1, st.glow || 0);
  if (!a) return;
  const ctx = R.ctx;
  strokeStyle(ctx, st, a);
  ctx.beginPath();
  ctx.moveTo(x0, y0);
  ctx.lineTo(x1, y1);
  ctx.stroke();
  ctx.globalAlpha = 1;
  ctx.setLineDash([]);
}

// Polyline with draw-on progress p ∈ [0,1] (and optional start p0).
export function poly(R, pts, st = {}, p = 1, p0 = 0) {
  const a = passAlpha(R, st.alpha ?? 1, st.glow || 0);
  if (!a || pts.length < 2 || p <= p0) return;
  let total = 0;
  const seglen = [];
  for (let i = 1; i < pts.length; i++) {
    const l = Math.hypot(pts[i][0] - pts[i - 1][0], pts[i][1] - pts[i - 1][1]);
    seglen.push(l);
    total += l;
  }
  const s0 = total * clamp(p0), s1 = total * clamp(p);
  const ctx = R.ctx;
  strokeStyle(ctx, st, a);
  ctx.beginPath();
  let acc = 0, started = false;
  for (let i = 1; i < pts.length; i++) {
    const l = seglen[i - 1];
    const a0 = acc, a1 = acc + l;
    acc = a1;
    if (a1 < s0 || a0 > s1) continue;
    const u0 = l ? clamp((s0 - a0) / l) : 0, u1 = l ? clamp((s1 - a0) / l) : 1;
    const P = pts[i - 1], Q = pts[i];
    const x0 = lerp(P[0], Q[0], u0), y0 = lerp(P[1], Q[1], u0);
    const x1 = lerp(P[0], Q[0], u1), y1 = lerp(P[1], Q[1], u1);
    if (!started) { ctx.moveTo(x0, y0); started = true; } else ctx.lineTo(x0, y0);
    ctx.lineTo(x1, y1);
  }
  ctx.stroke();
  ctx.globalAlpha = 1;
  ctx.setLineDash([]);
}

// Sample a cubic bezier into points.
export function bezierPts(p0, c0, c1, p1, n = 32) {
  const out = [];
  for (let i = 0; i <= n; i++) {
    const t = i / n, u = 1 - t;
    out.push([
      u * u * u * p0[0] + 3 * u * u * t * c0[0] + 3 * u * t * t * c1[0] + t * t * t * p1[0],
      u * u * u * p0[1] + 3 * u * u * t * c0[1] + 3 * u * t * t * c1[1] + t * t * t * p1[1],
    ]);
  }
  return out;
}
export function bezierAt(p0, c0, c1, p1, t) {
  const u = 1 - t;
  return [
    u * u * u * p0[0] + 3 * u * u * t * c0[0] + 3 * u * t * t * c1[0] + t * t * t * p1[0],
    u * u * u * p0[1] + 3 * u * u * t * c0[1] + 3 * u * t * t * c1[1] + t * t * t * p1[1],
  ];
}

export function circle(R, x, y, r, st = {}) {
  const a = passAlpha(R, st.alpha ?? 1, st.glow || 0);
  if (!a || r <= 0) return;
  const ctx = R.ctx;
  ctx.beginPath();
  ctx.arc(x, y, r, 0, TAU);
  if (st.fill) {
    ctx.globalAlpha = a * (st.fillAlpha ?? 1);
    ctx.fillStyle = st.fill;
    ctx.fill();
  }
  if (st.color || !st.fill) {
    strokeStyle(ctx, st, a);
    ctx.stroke();
  }
  ctx.globalAlpha = 1;
  ctx.setLineDash([]);
}

export function arc(R, x, y, r, a0, a1, st = {}) {
  const a = passAlpha(R, st.alpha ?? 1, st.glow || 0);
  if (!a || r <= 0 || a1 === a0) return;
  const ctx = R.ctx;
  strokeStyle(ctx, st, a);
  ctx.beginPath();
  ctx.arc(x, y, r, a0, a1, a1 < a0);
  ctx.stroke();
  ctx.globalAlpha = 1;
  ctx.setLineDash([]);
}

export function rrect(R, x, y, w, h, r, st = {}) {
  const a = passAlpha(R, st.alpha ?? 1, st.glow || 0);
  if (!a || w <= 0 || h <= 0) return;
  const ctx = R.ctx;
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, Math.min(r, w / 2, h / 2));
  if (st.fill) {
    ctx.globalAlpha = a * (st.fillAlpha ?? 1);
    ctx.fillStyle = st.fill;
    ctx.fill();
  }
  if (st.color) {
    strokeStyle(ctx, st, a);
    ctx.stroke();
  }
  ctx.globalAlpha = 1;
  ctx.setLineDash([]);
}

export function rect(R, x, y, w, h, st = {}) {
  const a = passAlpha(R, st.alpha ?? 1, st.glow || 0);
  if (!a) return;
  const ctx = R.ctx;
  ctx.globalAlpha = a;
  ctx.fillStyle = st.fill || C.ink;
  ctx.fillRect(x, y, w, h);
  ctx.globalAlpha = 1;
}

// Checkmark drawn on with progress p.
export function check(R, x, y, s, p, st = {}) {
  const pts = [[x - s * 0.5, y + s * 0.02], [x - s * 0.14, y + s * 0.36], [x + s * 0.52, y - s * 0.4]];
  poly(R, pts, { w: 2, cap: 'round', ...st }, p);
}
export function cross(R, x, y, s, p, st = {}) {
  poly(R, [[x - s / 2, y - s / 2], [x + s / 2, y + s / 2]], st, clamp(p * 2));
  poly(R, [[x + s / 2, y - s / 2], [x - s / 2, y + s / 2]], st, clamp(p * 2 - 1));
}
// Corner brackets around a box (the instrument's "focus" mark).
export function brackets(R, x, y, w, h, len, st = {}) {
  const L = Math.min(len, w / 2, h / 2);
  poly(R, [[x, y + L], [x, y], [x + L, y]], st);
  poly(R, [[x + w - L, y], [x + w, y], [x + w, y + L]], st);
  poly(R, [[x + w, y + h - L], [x + w, y + h], [x + w - L, y + h]], st);
  poly(R, [[x + L, y + h], [x, y + h], [x, y + h - L]], st);
}
export function diamond(R, x, y, s, st = {}) {
  const a = passAlpha(R, st.alpha ?? 1, st.glow || 0);
  if (!a) return;
  const ctx = R.ctx;
  ctx.beginPath();
  ctx.moveTo(x, y - s); ctx.lineTo(x + s, y); ctx.lineTo(x, y + s); ctx.lineTo(x - s, y); ctx.closePath();
  if (st.fill) { ctx.globalAlpha = a; ctx.fillStyle = st.fill; ctx.fill(); }
  if (st.color) { strokeStyle(ctx, st, a); ctx.stroke(); }
  ctx.globalAlpha = 1;
}

// Radial soft light (for flares, focus pools). Drawn additively.
export function light(R, x, y, r, color, alpha = 1, glow = 0) {
  const a = passAlpha(R, alpha, glow || 1);
  if (!a || r <= 0) return;
  const ctx = R.ctx;
  const g = ctx.createRadialGradient(x, y, 0, x, y, r);
  g.addColorStop(0, rgba(color, 1));
  g.addColorStop(0.25, rgba(color, 0.35));
  g.addColorStop(1, rgba(color, 0));
  ctx.globalCompositeOperation = 'lighter';
  ctx.globalAlpha = a;
  ctx.fillStyle = g;
  ctx.fillRect(x - r, y - r, r * 2, r * 2);
  ctx.globalAlpha = 1;
  ctx.globalCompositeOperation = 'source-over';
}

// Transform helpers (applied to the current pass's ctx).
export function push(R, x = 0, y = 0, s = 1, rot = 0) {
  const ctx = R.ctx;
  ctx.save();
  ctx.translate(x, y);
  if (rot) ctx.rotate(rot);
  if (s !== 1) ctx.scale(s, s);
}
export function pop(R) {
  R.ctx.restore();
}

// Full TextMetrics for a string in a style (pass-independent).
export function metrics(str, st) {
  setText(mctx, { ...st, align: 'left', baseline: 'alphabetic' });
  return mctx.measureText(str);
}

// Anamorphic streak: a thin horizontal line of light (the lens of the instrument).
export function streak(R, x, y, len, alpha, color = '#9FD0FF', glow = 1) {
  const a = passAlpha(R, alpha, glow);
  if (!a) return;
  const ctx = R.ctx;
  const g = ctx.createLinearGradient(x - len, 0, x + len, 0);
  g.addColorStop(0, 'rgba(0,0,0,0)');
  g.addColorStop(0.5, color);
  g.addColorStop(1, 'rgba(0,0,0,0)');
  ctx.globalCompositeOperation = 'lighter';
  ctx.globalAlpha = a;
  ctx.fillStyle = g;
  ctx.fillRect(x - len, y - 1.2, len * 2, 2.4);
  ctx.globalAlpha = a * 0.35;
  ctx.fillRect(x - len * 0.6, y - 5, len * 1.2, 10);
  ctx.globalAlpha = 1;
  ctx.globalCompositeOperation = 'source-over';
}
