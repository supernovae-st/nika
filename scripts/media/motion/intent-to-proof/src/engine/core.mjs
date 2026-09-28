// Core math, easing, timing and noise. Everything in the film is a pure
// function of time: no state carries from one frame to the next, so any
// frame renders alone (parallel workers, random-access stills).

export const TAU = Math.PI * 2;
export const clamp = (x, a = 0, b = 1) => (x < a ? a : x > b ? b : x);
export const lerp = (a, b, t) => a + (b - a) * t;
export const seg = (t, a, b) => clamp((t - a) / (b - a));
export const smooth = (a, b, x) => {
  const t = clamp((x - a) / (b - a));
  return t * t * (3 - 2 * t);
};
// 0 → 1 → 0 window with eased shoulders.
export const win = (t, a, b, fadeIn = 0.2, fadeOut = 0.2) =>
  Math.min(smooth(a, a + fadeIn, t), 1 - smooth(b - fadeOut, b, t));

// ── easing ──────────────────────────────────────────────────────────────
function cubicBezier(x1, y1, x2, y2) {
  const cx = 3 * x1, bx = 3 * (x2 - x1) - cx, ax = 1 - cx - bx;
  const cy = 3 * y1, by = 3 * (y2 - y1) - cy, ay = 1 - cy - by;
  const sx = u => ((ax * u + bx) * u + cx) * u;
  const sy = u => ((ay * u + by) * u + cy) * u;
  const dx = u => (3 * ax * u + 2 * bx) * u + cx;
  return x => {
    if (x <= 0) return 0;
    if (x >= 1) return 1;
    let u = x;
    for (let i = 0; i < 8; i++) {
      const e = sx(u) - x;
      if (Math.abs(e) < 1e-6) break;
      const d = dx(u);
      if (Math.abs(d) < 1e-6) break;
      u -= e / d;
    }
    u = clamp(u);
    return sy(u);
  };
}

export const E = {
  lin: t => t,
  inQuad: t => t * t,
  outQuad: t => 1 - (1 - t) * (1 - t),
  inOutQuad: t => (t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2),
  inCubic: t => t * t * t,
  outCubic: t => 1 - (1 - t) ** 3,
  inOutCubic: t => (t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2),
  inQuart: t => t ** 4,
  outQuart: t => 1 - (1 - t) ** 4,
  inOutQuart: t => (t < 0.5 ? 8 * t ** 4 : 1 - (-2 * t + 2) ** 4 / 2),
  inQuint: t => t ** 5,
  outQuint: t => 1 - (1 - t) ** 5,
  inOutQuint: t => (t < 0.5 ? 16 * t ** 5 : 1 - (-2 * t + 2) ** 5 / 2),
  inExpo: t => (t <= 0 ? 0 : 2 ** (10 * t - 10)),
  outExpo: t => (t >= 1 ? 1 : 1 - 2 ** (-10 * t)),
  inOutExpo: t =>
    t <= 0 ? 0 : t >= 1 ? 1 : t < 0.5 ? 2 ** (20 * t - 10) / 2 : (2 - 2 ** (-20 * t + 10)) / 2,
  inSine: t => 1 - Math.cos((t * Math.PI) / 2),
  outSine: t => Math.sin((t * Math.PI) / 2),
  inOutSine: t => -(Math.cos(Math.PI * t) - 1) / 2,
  outBack: t => {
    const s = 1.4;
    return 1 + (s + 1) * (t - 1) ** 3 + s * (t - 1) ** 2;
  },
  outBackSoft: t => {
    const s = 0.9;
    return 1 + (s + 1) * (t - 1) ** 3 + s * (t - 1) ** 2;
  },
  // The film's signature curves.
  snap: cubicBezier(0.16, 1, 0.3, 1), // hard in, long precise settle
  glide: cubicBezier(0.65, 0, 0.35, 1), // symmetric, calm
  whip: cubicBezier(0.8, 0, 0.1, 1), // camera whips
  lift: cubicBezier(0.3, 0, 0, 1), // UI settles
  drop: cubicBezier(0.55, 0, 1, 0.45), // accelerate away
};
export const bezier = cubicBezier;

// Ease a segment in one call.
export const ez = (t, a, b, f = E.snap) => f(seg(t, a, b));

// ── deterministic noise ─────────────────────────────────────────────────
export function hash(i, j = 0, k = 0) {
  let h = (i * 374761393 + j * 668265263 + k * 2147483647) >>> 0;
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
}
const fade = t => t * t * t * (t * (t * 6 - 15) + 10);
export function noise1(x, seed = 0) {
  const i = Math.floor(x), f = x - i;
  return lerp(hash(i, seed) * 2 - 1, hash(i + 1, seed) * 2 - 1, fade(f));
}
// ── color ───────────────────────────────────────────────────────────────
const cache = new Map();
export function rgb(hex) {
  let v = cache.get(hex);
  if (!v) {
    const h = hex.replace('#', '');
    v = [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16)];
    cache.set(hex, v);
  }
  return v;
}
export function rgba(hex, a = 1) {
  const [r, g, b] = rgb(hex);
  return `rgba(${r},${g},${b},${clamp(a).toFixed(4)})`;
}
// ── palette (Nika blue scientific canon) ────────────────────────────────
export const C = {
  bg0: '#010308',
  bg1: '#061022',
  bg2: '#0B1A33',
  ink: '#EAF4FF', // primary text
  mist: '#A9BCD6', // secondary text
  dim: '#5E7597', // tertiary
  faint: '#2A3C5C', // hairlines
  line: '#18294A',
  blue: '#2F7BFF', // electric
  ice: '#9FD0FF', // logomark
  cyan: '#62E3FF',
  teal: '#3CE3B4', // verified
  amber: '#FFB547', // unknown — safe, not error
  gold: '#FFD68A',
  red: '#FF5A6E', // refusal, used sparingly
  human: '#FFF1DE', // human authority — warm, matte
};
