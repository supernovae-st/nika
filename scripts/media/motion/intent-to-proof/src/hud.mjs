// The instrument frame: background field, corner registration, session act
// readout, timecode and the stage rail. The rail is the viewer's map — it
// always says where the intent is in the pipeline — and at the end it lifts
// into the full architecture.
import { C, clamp, lerp, smooth, E, ez, rgba } from './engine/core.mjs';
import { text, line, poly, diamond, rect, measure } from './engine/render.mjs';
import { T, STAGES, DURATION } from './timeline.mjs';
import { hashes, short } from './facts.mjs';

// ── background ─────────────────────────────────────────────────────────
// Background params are functions of time so scenes can breathe.
export function background(R, t, env) {
  if (R.glowPass) return;
  const ctx = R.ctx;
  ctx.fillStyle = C.bg0;
  ctx.fillRect(0, 0, 1920, 1080);
  const cx = env.bgX ?? 960, cy = env.bgY ?? 560;
  const g = ctx.createRadialGradient(cx, cy, 0, cx, cy, 1150);
  const k = env.bgGlow ?? 1;
  g.addColorStop(0, rgba(C.bg2, 0.9 * k));
  g.addColorStop(0.45, rgba(C.bg1, 0.55 * k));
  g.addColorStop(1, rgba(C.bg0, 0));
  ctx.fillStyle = g;
  ctx.fillRect(0, 0, 1920, 1080);
  // engineering-paper dot grid (parallax offset from the active scene)
  const ga = (env.gridAlpha ?? 1) * 0.42;
  if (ga > 0.01) {
    const sp = 32, ox = (((env.gridX ?? 0) % sp) + sp) % sp, oy = (((env.gridY ?? 0) % sp) + sp) % sp;
    ctx.fillStyle = rgba(C.faint, ga);
    for (let y = oy - sp; y < 1080 + sp; y += sp)
      for (let x = ox - sp; x < 1920 + sp; x += sp) ctx.fillRect(x - 0.6, y - 0.6, 1.2, 1.2);
    // every 4th cross, a tiny registration plus
    ctx.fillStyle = rgba(C.faint, ga * 1.4);
    for (let y = oy - sp * 4; y < 1080 + sp * 4; y += sp * 4)
      for (let x = ox - sp * 4; x < 1920 + sp * 4; x += sp * 4) {
        ctx.fillRect(x - 3, y - 0.4, 6, 0.8);
        ctx.fillRect(x - 0.4, y - 3, 0.8, 6);
      }
  }
}

export function vignette(R) {
  if (R.glowPass) return;
  const ctx = R.ctx;
  const g = ctx.createRadialGradient(960, 540, 380, 960, 540, 1180);
  g.addColorStop(0, 'rgba(0,0,0,0)');
  g.addColorStop(1, 'rgba(0,0,0,0.62)');
  ctx.fillStyle = g;
  ctx.fillRect(0, 0, 1920, 1080);
}

// ── HUD ────────────────────────────────────────────────────────────────
const RAIL = { x0: 150, x1: 1770, y: 1008 };
export const railX = i => lerp(RAIL.x0, RAIL.x1, i / (STAGES.length - 1));

// Continuous stage position (index + fraction) at time t.
export function stagePos(t) {
  let i = 0;
  for (let k = 0; k < STAGES.length; k++) if (t >= STAGES[k][1]) i = k;
  const t0 = STAGES[i][1];
  const t1 = i + 1 < STAGES.length ? STAGES[i + 1][1] : DURATION - 2;
  return { i, f: clamp((t - t0) / (t1 - t0)) };
}

// Session act (the human's conversational act, as the Session types it).
function actAt(t) {
  if (t >= T.consent + 0.35) return ['REQUEST RUN', `consented · proposal ${short(hashes.proposal_blake3, 8)} · rev 7`];
  if (t >= T.approve + 0.2) return ['REQUEST RUN', 'consent bound to this revision'];
  if (t >= T.discuss) return ['DISCUSS', 'a question is not an authorization'];
  if (t >= T.answer + 0.15) return ['ANSWER', 'slot · currency'];
  if (t >= T.act) return ['NEW WORK', 'original goal kept · raw message kept'];
  return null;
}

export function hud(R, t, env) {
  const on = smooth(T.act - 0.15, T.act + 0.45, t) * (1 - smooth(T.title - 0.5, T.title - 0.1, t));
  const railLift = env.railLift ?? 0; // 0..1 when the rail becomes the map
  if (on <= 0.001) return;
  const boot = ez(t, T.act - 0.15, T.act + 0.7, E.snap);

  // corner registration
  const L = 16 * boot, ins = 34;
  const cst = { color: C.dim, w: 1, alpha: 0.7 * on };
  poly(R, [[ins, ins + L], [ins, ins], [ins + L, ins]], cst);
  poly(R, [[1920 - ins - L, ins], [1920 - ins, ins], [1920 - ins, ins + L]], cst);
  poly(R, [[ins, 1080 - ins - L], [ins, 1080 - ins], [ins + L, 1080 - ins]], cst);
  poly(R, [[1920 - ins - L, 1080 - ins], [1920 - ins, 1080 - ins], [1920 - ins, 1080 - ins - L]], cst);

  // top-left: device + session act
  text(R, 'NIKA', 64, 58, { f: 'MGW 700', size: 12, tracking: 5, color: C.ink, alpha: on });
  text(R, 'SESSION 0412', 150, 58, { f: 'MM 400', size: 10.5, tracking: 1, color: C.dim, alpha: on * 0.9 });
  const act = actAt(t);
  if (act) {
    const [name, note] = act;
    let tAct = T.act;
    if (t >= T.consent + 0.35) tAct = T.consent + 0.35;
    else if (t >= T.approve + 0.2) tAct = T.approve + 0.2;
    else if (t >= T.discuss) tAct = T.discuss;
    else if (t >= T.answer + 0.15) tAct = T.answer + 0.15;
    const k = ez(t, tAct, tAct + 0.35, E.snap);
    text(R, 'ACT', 262, 58, { f: 'MM 400', size: 10.5, tracking: 1, color: C.dim, alpha: on * 0.9 });
    const nameShown = name.slice(0, Math.ceil(name.length * clamp(k * 1.4)));
    text(R, nameShown, 294, 58, { f: 'MM 500', size: 10.5, tracking: 1.2, color: C.human, alpha: on, glow: 0.35 });
    const nw = measure(name, { f: 'MM 500', size: 10.5, tracking: 1.2 });
    text(R, note, 294 + nw + 16, 58, { f: 'MM 400', size: 10.5, tracking: 0.3, color: C.dim, alpha: on * 0.85 * k });
    // blink marker on change
    const flash = 1 - smooth(tAct, tAct + 0.5, t);
    if (flash > 0) rect(R, 286, 50, 3, 10, { fill: C.human, alpha: on * flash, glow: 0.8 });
  }

  // top-right: timecode + honesty tag
  const sec = Math.floor(t), cs = Math.floor((t - sec) * 100);
  const tc = `00:${String(sec).padStart(2, '0')}.${String(cs).padStart(2, '0')}`;
  text(R, tc, 1856, 58, { f: 'MM 400', size: 11, tracking: 1, color: C.mist, alpha: on * 0.85, align: 'right' });
  text(R, '60 FPS · 120 BPM', 1856 - 92, 58, { f: 'MM 400', size: 10.5, tracking: 0.8, color: C.dim, alpha: on * 0.7, align: 'right' });
  text(R, 'ILLUSTRATION · FIXTURE DATA', 1856, 76, { f: 'MM 400', size: 8.5, tracking: 1.1, color: C.dim, alpha: on * 0.55, align: 'right' });

  // stage rail
  const railOn = on * (1 - railLift);
  if (railOn > 0.01) drawRail(R, t, railOn, boot);
}

function drawRail(R, t, a, boot) {
  const { i, f } = stagePos(t);
  const n = STAGES.length;
  const y = RAIL.y;
  const drawTo = lerp(RAIL.x0, RAIL.x1, boot);
  line(R, RAIL.x0, y, drawTo, y, { color: C.faint, w: 1, alpha: a * 0.9 });
  // progress fill: to the active stage tick, then creeping with the stage's progress
  const px = Math.min(drawTo, lerp(railX(i), railX(Math.min(i + 1, n - 1)), f * 0.5));
  line(R, RAIL.x0, y, px, y, { color: C.ice, w: 1.2, alpha: a * 0.85, glow: 0.4 });
  for (let k = 0; k < n; k++) {
    const x = railX(k);
    if (x > drawTo + 1) continue;
    const [label] = STAGES[k];
    const done = k < i, active = k === i;
    const human = label === 'CONSENT';
    const col = active ? (human ? C.human : C.ink) : done ? (human ? C.human : C.mist) : C.dim;
    const la = active ? 1 : done ? 0.62 : 0.34;
    const tAct = STAGES[k][1];
    const pop = active ? ez(t, tAct, tAct + 0.35, E.snap) : 1;
    if (active) {
      diamond(R, x, y, 4.2 * pop, { fill: human ? C.human : C.ice, alpha: a, glow: 1 });
      line(R, x, y - 12, x, y - 5, { color: col, w: 1, alpha: a * 0.8 });
    } else {
      line(R, x, y - 4, x, y + 4, { color: col, w: 1, alpha: a * la });
    }
    text(R, label, x, y + 25, { f: 'MGW 500', size: 9, tracking: 2.4, color: col, alpha: a * la, align: 'center', glow: active ? 0.25 : 0 });
  }
  // the ASK excursion above PROVE while the unknown is open
  const askA = smooth(T.unknown, T.unknown + 0.2, t) * (1 - smooth(T.returned, T.returned + 0.4, t));
  if (askA > 0.01) {
    const x = railX(4);
    poly(R, [[x, y - 6], [x + 10, y - 22], [x + 40, y - 22]], { color: C.amber, w: 1, alpha: a * askA, glow: 0.6 });
    text(R, 'ASK', x + 46, y - 18.5, { f: 'MGW 500', size: 9, tracking: 2.4, color: C.amber, alpha: a * askA, glow: 0.5 });
  }
}

// Stage index helper exported for scenes that want the same x positions.
export { RAIL };
