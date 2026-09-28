// Reading-time audit (the /brag readability law): a line the viewer must
// read stays fully visible and settled for about 0.3 s per word, at least
// 1.2 s for a sentence and 0.8 s for a label of one to three words,
// counted from when the whole line is on screen.
//
//   node tools/readability.mjs [--min 22] [--all] [--clip <name>]
//
// Samples the main pass at 30 fps and records every text draw: its screen
// size, alpha and position. A draw is "settled" while its alpha is within
// 10% of that line's peak and it moves less than 3 px per sample. Lines at
// least --min px tall on a 1920×1080 screen count as must-read; smaller
// type is instrument texture and is reported only with --all. A counter's
// intermediate values (05/10 → 06/10, rolling odometer digits) are marked
// "transient", as is a command's prefix while it is typed ("typed"),
// and a line that never holds still (words in flight, a camera fly-by)
// is marked "motion": none of them is a read. A line that settles
// but not for long enough is a miss (✖); --strict makes misses fail.
import { init, drawFrame } from '../src/film.mjs';
import { DURATION } from '../src/timeline.mjs';
import { loadClip, initClip, drawClip } from '../src/clip.mjs';

const args = process.argv.slice(2);
const opt = (n, d) => (args.includes(`--${n}`) ? args[args.indexOf(`--${n}`) + 1] : d);
const MIN = +opt('min', 22);
const FPS = 30;
const clipName = opt('clip', null);
const clip = clipName ? await loadClip(clipName) : null;
const surf = clip ? initClip(0.25) : init(0.25);
const LENGTH = clip ? clip.meta.duration : DURATION;
const seen = new Map(); // line → one sample per frame (its largest draw)

let frame = 0;
surf.audit = (str, x, y, st, a, m) => {
  const key = str.trim();
  if (!key) return;
  const k = Math.hypot(m.a, m.b) / surf.scale; // design px per text unit
  const px = (st.size || 20) * k;
  const sx = (m.a * x + m.c * y + m.e) / surf.scale, sy = (m.b * x + m.d * y + m.f) / surf.scale;
  if (sx < -200 || sx > 2120 || sy < -100 || sy > 1180) return; // off screen
  let s = seen.get(key);
  if (!s) seen.set(key, (s = []));
  const last = s[s.length - 1];
  if (last && last.f === frame) {
    if (px > last.px) s[s.length - 1] = { f: frame, a, px, sx, sy };
  } else s.push({ f: frame, a, px, sx, sy });
};
for (frame = 0; frame < LENGTH * FPS; frame++) {
  if (clip) drawClip(surf, clip, frame / FPS);
  else drawFrame(surf, frame / FPS);
}

const words = s => s.split(/\s+/).filter(w => /[\p{L}\p{N}€]/u.test(w)).length;
const rows = [];
for (const [str, all] of seen) {
  // an appearance: a run of must-read-sized samples with gaps of at most 2 frames
  let run = [];
  const flush = () => {
    if (!run.length) return;
    const peak = Math.max(...run.map(r => r.a));
    const px = Math.max(...run.map(r => r.px));
    let best = 0, cur = 0, prev = null;
    for (const r of run) {
      const still = prev && prev.f === r.f - 1 && Math.hypot(r.sx - prev.sx, r.sy - prev.sy) < 3 && Math.abs(r.px - prev.px) / r.px < 0.015;
      const ok = r.a >= 0.9 * peak;
      cur = ok && (still || !prev) ? cur + 1 : ok ? 1 : 0;
      best = Math.max(best, cur);
      prev = r;
    }
    const n = words(str);
    const floor = n <= 3 ? 0.8 : Math.max(1.2, 0.3 * n);
    rows.push({ t: run[0].f / FPS, end: run[run.length - 1].f / FPS, str, px, settled: best / FPS, floor, n, sx: run[0].sx, sy: run[0].sy });
    run = [];
  };
  let last = -9;
  for (const r of all) {
    if (r.px < MIN && !args.includes('--all')) continue;
    if (r.f - last > 2) flush();
    run.push(r);
    last = r.f;
  }
  flush();
}
rows.sort((p, q) => p.t - q.t);
// A counting sequence: a numeric line of the same shape and size takes its
// place within a few samples of it ending.
const shape = r => r.str.replace(/\d/g, '0');
// A typed line: a longer line it is the start of takes its place, at the
// same spot, within a sample or two (a command being typed, key by key).
for (const r of rows) {
  r.transient = /\d/.test(r.str) && rows.some(q => q !== r && shape(q) === shape(r) && q.str !== r.str && q.t >= r.end - 0.2 && q.t <= r.end + 0.15 && q.end > r.end);
  r.typed = rows.some(q => q !== r && q.str.length > r.str.length && q.str.startsWith(r.str) && Math.abs(q.t - r.end) <= 0.1 && Math.hypot(q.sx - r.sx, q.sy - r.sy) < 3);
  r.transient ||= r.typed;
}
const must = rows.filter(r => r.px >= MIN && r.n > 0);
const show = args.includes('--all') ? rows : must;
let short = 0;
for (const r of show) {
  r.motion = !r.transient && r.settled < 0.1;
  const miss = r.px >= MIN && !r.transient && !r.motion && r.settled < r.floor;
  if (miss) short++;
  const note = r.typed ? '  (typed)' : r.transient ? '  (transient)' : r.motion ? '  (motion)' : '';
  const mark = r.transient || r.motion ? '~' : miss ? '✖' : '✔';
  console.log(`${mark} ${r.t.toFixed(2).padStart(6)}–${r.end.toFixed(2).padEnd(6)} ${r.px.toFixed(0).padStart(4)}px  settled ${r.settled.toFixed(2)}s / floor ${r.floor.toFixed(1)}s  ${JSON.stringify(r.str).slice(0, 70)}${note}`);
}
console.log(`${must.length} must-read lines (≥ ${MIN}px) · ${must.filter(r => r.transient).length} transient · ${must.filter(r => r.motion).length} in motion · ${short} below the reading floor`);
process.exitCode = short && args.includes('--strict') ? 1 : 0;
