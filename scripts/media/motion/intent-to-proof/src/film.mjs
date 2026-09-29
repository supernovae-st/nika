// Frame compositor: background → scenes → HUD, twice (main + glow),
// then bloom, vignette and optional temporal supersampling (motion blur).
import { createSurfaces, makeR, compositeGlow, loadFonts } from './engine/render.mjs';
import { background, vignette, hud } from './hud.mjs';
import { mbSamples, SCENES } from './timeline.mjs';
import * as intent from './scenes/s1-intent.mjs';
import * as observe from './scenes/s2-observe.mjs';
import * as propose from './scenes/s3-propose.mjs';
import * as plan from './scenes/s4-plan.mjs';
import * as prove from './scenes/s5-prove.mjs';
import * as lower from './scenes/s6-lower.mjs';
import * as consent from './scenes/s7-consent.mjs';
import * as run from './scenes/s8-run.mjs';
import * as result from './scenes/s9-result.mjs';
import * as reveal from './scenes/s10-reveal.mjs';
import * as title from './scenes/s11-title.mjs';

const LIST = [
  ['intent', intent], ['observe', observe], ['propose', propose], ['plan', plan],
  ['prove', prove], ['lower', lower], ['consent', consent], ['run', run],
  ['result', result], ['reveal', reveal], ['title', title],
];

export function init(scale) {
  loadFonts();
  return createSurfaces(scale);
}

function active(id, t) {
  const [a, b] = SCENES[id];
  return t >= a && t < b;
}

export function envAt(t) {
  const env = {};
  for (const [id, S] of LIST) if (S.env && active(id, t)) Object.assign(env, S.env(t) || {});
  return env;
}

export function drawFrame(surf, t) {
  const env = envAt(t);
  for (const pass of ['main', 'glow']) {
    const R = makeR(surf, pass, t);
    if (pass === 'glow') {
      R.ctx.save();
      R.ctx.setTransform(1, 0, 0, 1, 0, 0);
      R.ctx.clearRect(0, 0, surf.gw, surf.gh);
      R.ctx.restore();
    }
    background(R, t, env);
    for (const [id, S] of LIST) {
      if (!active(id, t)) continue;
      R.ctx.save();
      S.draw(R, t, env);
      R.ctx.restore();
    }
    hud(R, t, env);
  }
  compositeGlow(surf, env.glowStrength ?? 1);
  vignette(makeR(surf, 'main', t));
}

let acc = null;
// Returns an RGBA Buffer for the frame at time t (seconds).
export function renderFrame(surf, t, fps, motionBlur = true) {
  const n = motionBlur ? mbSamples(t) : 1;
  if (n <= 1) {
    drawFrame(surf, t);
    return surf.main.data();
  }
  const size = surf.W * surf.H * 4;
  if (!acc || acc.length !== size) acc = new Uint16Array(size);
  else acc.fill(0);
  const shutter = 0.36 / fps; // ~130° shutter: smooth streaks, no smear
  for (let k = 0; k < n; k++) {
    const tk = t - shutter / 2 + ((k + 0.5) / n) * shutter;
    drawFrame(surf, Math.max(0, tk));
    const d = surf.main.data();
    for (let i = 0; i < size; i++) acc[i] += d[i];
  }
  const out = Buffer.allocUnsafe(size);
  const half = n >> 1;
  for (let i = 0; i < size; i++) out[i] = ((acc[i] + half) / n) | 0;
  return out;
}
