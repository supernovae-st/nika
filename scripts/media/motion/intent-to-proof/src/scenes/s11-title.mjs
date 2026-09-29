// S11 · TITLE — the point of light (the caret of the first frame) opens
// into the mark. From intent to proof.
import fs from 'node:fs';
import path from 'node:path';
import { Path2D, createCanvas } from '@napi-rs/canvas';
import { C, E, lerp, seg, smooth } from '../engine/core.mjs';
import { text, line, circle, light, measure, passAlpha, ROOT, streak } from '../engine/render.mjs';
import { T, DURATION } from '../timeline.mjs';

let LOGO = null;
function logo() {
  if (!LOGO) {
    const svg = fs.readFileSync(path.resolve(ROOT, '../../../../media/brand/nika-logomark.svg'), 'utf8');
    LOGO = new Path2D(svg.match(/ d="([^"]+)"/)[1]);
  }
  return LOGO;
}

const WORD = { f: 'MGU 800', size: 76 };

export function env(t) {
  return { bgGlow: lerp(1.2, 0.9, seg(t, T.title, T.title + 1)), gridAlpha: 0, bgY: 470 };
}

export function draw(R, t) {
  const t0 = T.title;
  if (t < t0 - 0.3) return;
  const end = 1 - smooth(DURATION - 0.28, DURATION - 0.02, t); // fade to black: the loop restarts on black
  // impact flash + shockwave
  const f = seg(t, t0, t0 + 0.7);
  if (f > 0 && f < 1) {
    light(R, 960, 540, 200 + 700 * E.outCubic(f), C.ice, 0.55 * (1 - f) * end, 1);
    circle(R, 960, 540, 20 + 900 * E.outExpo(f), { color: C.ice, w: 1.4, alpha: 0.6 * (1 - f) * end, glow: 1 });
    streak(R, 960, 540, lerp(300, 1500, E.outCubic(f)), 0.85 * (1 - f) ** 2 * end, C.ice, 1);
  }
  // the mark
  const lk = E.snap(seg(t, t0 + 0.02, t0 + 0.6));
  if (lk > 0) {
    const size = 150 * lerp(0.55, 1, lk);
    const a = passAlpha(R, lk * end, 0.55);
    if (a > 0) {
      const ctx = R.ctx;
      const blur = !R.glowPass && lk < 0.98 ? (1 - lk) * 14 * R.k : 0;
      if (blur > 0.3) {
        // focus pull in a tight offscreen canvas (a canvas filter here would blur the whole frame)
        const px = Math.ceil(size * R.k + blur * 6);
        const c = createCanvas(px, px);
        const x2 = c.getContext('2d');
        x2.translate(blur * 3, blur * 3);
        x2.scale((size * R.k) / 1100, (size * R.k) / 1100);
        x2.filter = `blur(${blur.toFixed(2)}px)`;
        x2.fillStyle = C.ice;
        x2.fill(logo());
        ctx.globalAlpha = a;
        const d = blur * 3 / R.k;
        ctx.drawImage(c, 960 - size / 2 - d, 330 - size / 2 - d, px / R.k, px / R.k);
        ctx.globalAlpha = 1;
      } else {
        ctx.save();
        ctx.translate(960 - size / 2, 330 - size / 2);
        ctx.scale(size / 1100, size / 1100);
        ctx.globalAlpha = a;
        ctx.fillStyle = C.ice;
        ctx.fill(logo());
        ctx.restore();
      }
    }
  }
  // the wordmark: letters settle from wide to exact
  const wk = E.snap(seg(t, t0 + 0.12, t0 + 0.85));
  if (wk > 0) {
    const tracking = lerp(90, 30, wk);
    const st = { ...WORD, tracking };
    const w = measure('NIKA', st);
    text(R, 'NIKA', 960 - w / 2, 540, { ...st, color: C.ink, alpha: wk * end, glow: 0.3 });
  }
  // the line
  const words = ['From', 'intent', 'to', 'proof.'];
  const st = { f: 'Geist 400', size: 54, tracking: -1 };
  const full = words.join(' ');
  let x = 960 - measure(full, st) / 2;
  words.forEach((wd, i) => {
    const k = E.snap(seg(t, t0 + 0.4 + i * 0.09, t0 + 0.9 + i * 0.09));
    if (k > 0) text(R, wd, x, 640 + 10 * (1 - k), { ...st, color: i === 3 ? C.ice : C.mist, alpha: k * end, blur: (1 - k) * 6, glow: i === 3 ? 0.4 : 0 });
    x += measure(wd + ' ', st);
  });
  // the thesis, one colour per role, then where to get it
  const th = { f: 'MGW 500', size: 15, tracking: 3.5 };
  const gap = 46;
  const widths = THESIS.map(([who, verb]) => measure(`${who} ${verb}`, th));
  let tx = 960 - (widths.reduce((s, w) => s + w, 0) + gap * (THESIS.length - 1)) / 2;
  THESIS.forEach(([who, verb, col], i) => {
    const k = smooth(t0 + 0.55 + i * 0.06, t0 + 0.75 + i * 0.06, t) * end;
    if (k > 0) {
      text(R, who, tx, 706, { ...th, color: C.mist, alpha: k });
      text(R, verb, tx + measure(`${who} `, th), 706, { ...th, color: col, alpha: k, glow: 0.25 });
      if (i > 0) circle(R, tx - gap / 2, 701, 1.6, { fill: C.dim, alpha: k });
    }
    tx += widths[i] + gap;
  });
  const uk = smooth(t0 + 0.7, t0 + 0.95, t);
  if (uk > 0) {
    line(R, 960 - 90 * uk, 738, 960 + 90 * uk, 738, { color: C.faint, w: 1, alpha: uk * end });
    text(R, 'nika.sh', 960, 776, { f: 'MM 500', size: 20, tracking: 1.5, color: C.ice, alpha: uk * end * 0.95, align: 'center', glow: 0.2 });
  }
}
const THESIS = [['INTELLIGENCE', 'PROPOSES', C.cyan], ['RUST', 'PROVES', C.teal], ['HUMANS', 'AUTHORIZE', C.human], ['NIKA', 'EXECUTES', C.ice]];
