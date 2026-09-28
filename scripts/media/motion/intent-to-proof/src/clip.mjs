// Frame compositor for the short feature clips (clips/*.mjs): the film's
// background, emissive glow and vignette around one clip's own drawing.
// Like the film, every frame is a pure function of time.
//
// A clip paints its panels in world coordinates with draw(). An optional
// camera(t) → { x, y, s } puts world point (x, y) at the frame's centre at
// zoom s, so a clip can push in on the line the viewer should read; an
// optional chrome() paints the title, kicker and plate in screen space,
// above the world and unmoved by the camera.
import { createSurfaces, makeR, compositeGlow, loadFonts, DW, DH } from './engine/render.mjs';
import { background, vignette } from './hud.mjs';

export const CLIPS = [
  'nika-hero', 'static-check-fix', 'chat-to-workflow', 'dag-execution', 'permits-audit',
  'on-error-recover', 'editor-diagnostics', 'workflow-gallery', 'full-loop', 'pr-check-comment',
  'trace-proof', 'spec-anatomy', 'agent-plugin',
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
    const cam = clip.camera ? clip.camera(t) : null;
    if (cam) {
      R.ctx.translate(DW / 2, DH / 2);
      R.ctx.scale(cam.s, cam.s);
      R.ctx.translate(-cam.x, -cam.y);
    }
    clip.draw(R, t, env);
    R.ctx.restore();
    if (clip.chrome) {
      R.ctx.save();
      clip.chrome(R, t, env);
      R.ctx.restore();
    }
  }
  compositeGlow(surf, env.glowStrength ?? 1);
  vignette(makeR(surf, 'main', t));
}

// RGBA buffer for the clip frame at time t (seconds).
export function renderClipFrame(surf, clip, t) {
  drawClip(surf, clip, t);
  return surf.main.data();
}
