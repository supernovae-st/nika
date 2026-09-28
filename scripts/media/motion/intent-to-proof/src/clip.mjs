// Frame compositor for the short feature clips (clips/*.mjs): the film's
// background, emissive glow and vignette around one clip's own drawing.
// Like the film, every frame is a pure function of time.
import { createSurfaces, makeR, compositeGlow, loadFonts } from './engine/render.mjs';
import { background, vignette } from './hud.mjs';

export const CLIPS = [
  'static-check-fix', 'chat-to-workflow', 'dag-execution', 'permits-audit',
  'on-error-recover', 'editor-diagnostics', 'workflow-gallery',
];

export async function loadClip(name) {
  if (!CLIPS.includes(name)) throw new Error(`unknown clip: ${name} (one of ${CLIPS.join(', ')})`);
  return import(`../clips/${name}.mjs`);
}

export function initClip(scale) {
  loadFonts();
  return createSurfaces(scale);
}

export function drawClip(surf, clip, t) {
  const env = clip.env ? clip.env(t) : {};
  for (const pass of ['main', 'glow']) {
    const R = makeR(surf, pass, t);
    if (pass === 'glow') {
      R.ctx.save();
      R.ctx.setTransform(1, 0, 0, 1, 0, 0);
      R.ctx.clearRect(0, 0, surf.gw, surf.gh);
      R.ctx.restore();
    }
    background(R, t, env);
    R.ctx.save();
    clip.draw(R, t, env);
    R.ctx.restore();
  }
  compositeGlow(surf, env.glowStrength ?? 1);
  vignette(makeR(surf, 'main', t));
}

// RGBA buffer for the clip frame at time t (seconds).
export function renderClipFrame(surf, clip, t) {
  drawClip(surf, clip, t);
  return surf.main.data();
}
