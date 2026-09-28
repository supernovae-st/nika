// workflow-gallery · "Start from a workflow."
// The gallery bare `nika try` prints (media/raw/try-gallery.txt): the
// embedded jobs with their verb glyphs and the one line the CLI shows for
// each (its own truncation included), and the counts of its two sections.
// Nothing here is typed by hand; a job the CLI lists without a line (the
// snippets) is counted, not drawn.
import { C, E, seg, smooth, repoLines, NIKA_VERSION, frame, headline, loopFade, cameraPath, frameBox, titleFade, WIDE } from './kit.mjs';
import { text, rrect, poly, measure } from '../src/engine/render.mjs';

export const meta = { duration: 13.5, poster: 12.4 };

const listing = repoLines('media/raw/try-gallery.txt');
const count = re => +(listing.find(l => re.test(l))?.match(re)?.[1] ?? NaN);
const PATH_STEPS = count(/^◆ the path — (\d+) steps/u);
const JOB_COUNT = count(/^◆ the jobs — (\d+) of them/u);
const GLYPH = new Set([...'◇▷◆✦']);
const jobsAt = listing.findIndex(l => /^◆ the jobs/u.test(l));
const ROWS = listing.slice(jobsAt + 1).filter(l => l.startsWith('│')).map(l => {
  const [, name, rest] = l.match(/^│\s+(\S+?)\.nika\s*(.*)$/u);
  const toks = rest.split(' ');
  const glyphs = [];
  while (toks.length && GLYPH.has(toks[0])) glyphs.push(toks.shift());
  return { name, glyphs, line: toks.join(' ').trim() };
});
const JOBS = ROWS.filter(r => r.line && !r.name.includes('/'));
const SNIPPETS = ROWS.length - JOBS.length;
if (!PATH_STEPS || ROWS.length !== JOB_COUNT || !JOBS.length) throw new Error('the try gallery listing changed shape');
// the verbs, as the listing's legend names them
const VERBS = Object.fromEntries([...(listing.find(l => /^verbs ·/.test(l)) || '').matchAll(/([◇▷◆✦]) (\w+) \(([^)]+)\)/gu)].map(m => [m[1], { verb: m[2], gloss: m[3] }]));
// the colours nika inspect gives the verbs (media/raw/graph-fanout.mmd)
const INSPECT = Object.fromEntries(repoLines('media/raw/graph-fanout.mmd').map(l => l.match(/^\s*classDef (\w+) .*stroke:(#[0-9a-fA-F]{6})/)).filter(Boolean).map(m => [m[1], m[2]]));
const colorOf = g => INSPECT[VERBS[g]?.verb] ?? C.mist;

const T = { cards: 0.5, every: 0.05, legend: 10.9 };
const COLS = 6, GAP = 16;
const GRID = { x: 64, y: 244, w: 1792, h: 700 };
const CW = (GRID.w - (COLS - 1) * GAP) / COLS;
const ROWS_N = Math.ceil((JOBS.length + 1) / COLS);
const CH = (GRID.h - (ROWS_N - 1) * GAP) / ROWS_N;
const cell = i => ({ x: GRID.x + (i % COLS) * (CW + GAP), y: GRID.y + Math.floor(i / COLS) * (CH + GAP), w: CW, h: CH });
const idx = name => JOBS.findIndex(j => j.name === name);
// two neighbourhoods to read closely: the jobs other clips show, and theirs
const around = (name, cols) => {
  const i = idx(name), c = cell(i), c0 = Math.max(0, Math.min(COLS - cols, (i % COLS) - 1));
  const left = cell(Math.floor(i / COLS) * COLS + c0);
  return { x: left.x, y: c.y, w: cols * CW + (cols - 1) * GAP, h: 2 * CH + GAP };
};
const FOCUS = ['meeting-actions', 'pr-review-fanout'].filter(n => idx(n) >= 0);

const SHOTS = [
  { at: 0, cam: WIDE },
  { at: 3.3, cam: frameBox(around(FOCUS[0], 3), 12), move: 0.7 },
  { at: 7.1, cam: frameBox(around(FOCUS[1] ?? FOCUS[0], 3), 12), move: 0.7 },
  { at: 10.7, cam: WIDE, move: 0.7 },
];
export const camera = t => cameraPath(t, SHOTS);

export function env() {
  return { bgGlow: 1, gridAlpha: 0.22, gridX: 0, gridY: 0, bgY: 560 };
}

export function chrome(R, t) {
  const a = loopFade(t, meta.duration);
  const cam = camera(t);
  frame(R, t, { kicker: `nika try · the embedded gallery · ${PATH_STEPS} path steps · ${JOB_COUNT} jobs`, plate: `names, verbs and lines: the real \`nika try\` listing · nika ${NIKA_VERSION}`, alpha: a, scrim: smooth(1, 1.12, cam.s) });
  headline({ ...R, fade: a * titleFade(cam) }, t, 0.2, meta.duration + 1, 'Start from a workflow.', `${JOB_COUNT} JOBS AND A ${PATH_STEPS}-STEP PATH, IN THE BINARY · NIKA TRY <NAME>`, { accent: 'a workflow.', accentColor: C.teal });
}

// the listing's verb marks, drawn as shapes in the verbs' colours
function glyph(R, g, x, y, s, alpha) {
  const col = colorOf(g), S = { color: col, w: 1.6, alpha, glow: 0.4 };
  if (g === '◇' || g === '◆') poly(R, [[x, y - s], [x + s, y], [x, y + s], [x - s, y], [x, y - s]], g === '◆' ? { ...S, fill: col } : S);
  else if (g === '▷') poly(R, [[x - s * 0.8, y - s], [x + s, y], [x - s * 0.8, y + s], [x - s * 0.8, y - s]], S);
  else if (g === '✦') {
    const pts = [];
    for (let k = 0; k < 8; k++) { const r = k % 2 ? s * 0.38 : s * 1.1, an = -Math.PI / 2 + k * Math.PI / 4; pts.push([x + r * Math.cos(an), y + r * Math.sin(an)]); }
    poly(R, [...pts, pts[0]], { ...S, fill: col });
  }
}

// a line of prose, word-wrapped to the card
function wrap(s, st, maxW) {
  const out = [];
  let cur = '';
  for (const w of s.split(' ')) {
    const next = cur ? `${cur} ${w}` : w;
    if (cur && measure(next, st) > maxW) { out.push(cur); cur = w; } else cur = next;
  }
  if (cur) out.push(cur);
  return out;
}

function card(R, t, i, job, a) {
  const c = cell(i);
  const t0 = T.cards + ((i % COLS) + Math.floor(i / COLS)) * T.every * 2;
  const k = E.snap(seg(t, t0, t0 + 0.4));
  if (k <= 0) return;
  const focus = FOCUS.includes(job.name) ? smooth(3.3, 3.7, t) : 0;
  const y = c.y + 10 * (1 - k);
  rrect(R, c.x, y, c.w, c.h, 14, { color: focus > 0 ? C.teal : C.faint, w: 1.2 + focus * 0.5, alpha: a * k, fill: '#050c19', fillAlpha: 0.9, glow: focus * 0.4 });
  text(R, job.name, c.x + 18, y + 34, { f: 'MM 500', size: 16, color: C.ink, alpha: a * k });
  // the verbs sit in the corner, clear of a long name
  job.glyphs.forEach((g, j) => glyph(R, g, c.x + c.w - 22 - (job.glyphs.length - 1 - j) * 20, y + c.h - 20, 6, a * k));
  const st = { f: 'Geist 400', size: 14 };
  wrap(job.line, st, c.w - 36).slice(0, 3).forEach((ln, j) => text(R, ln, c.x + 18, y + 62 + j * 20, { ...st, color: C.mist, alpha: a * k }));
}

export function draw(R, t) {
  const a = loopFade(t, meta.duration);
  JOBS.forEach((job, i) => card(R, t, i, job, a));
  // the last cell: what the wall leaves out, counted
  const i = JOBS.length, c = cell(i);
  const t0 = T.cards + ((i % COLS) + Math.floor(i / COLS)) * T.every * 2;
  const k = E.snap(seg(t, t0, t0 + 0.4)) * a;
  if (k > 0) {
    rrect(R, c.x, c.y, c.w, c.h, 14, { color: C.faint, w: 1, alpha: k, dash: [5, 5] });
    text(R, `+ ${SNIPPETS} snippets`, c.x + 18, c.y + 38, { f: 'MM 500', size: 16, color: C.mist, alpha: k });
    text(R, `+ the path · ${PATH_STEPS} steps`, c.x + 18, c.y + 66, { f: 'MM 500', size: 16, color: C.mist, alpha: k });
    text(R, 'nika try 01-hello · offline', c.x + 18, c.y + 100, { f: 'MM 400', size: 14, color: C.teal, alpha: k });
  }
  // the legend, as the listing prints it
  const lk = smooth(T.legend, T.legend + 0.4, t) * a;
  if (lk > 0) {
    let x = GRID.x;
    for (const [g, v] of Object.entries(VERBS)) {
      glyph(R, g, x + 8, GRID.y + GRID.h + 44, 7, lk);
      const label = `${v.verb} · ${v.gloss}`;
      text(R, label, x + 26, GRID.y + GRID.h + 50, { f: 'MM 400', size: 16, color: C.mist, alpha: lk });
      x += measure(label, { f: 'MM 400', size: 16 }) + 72;
    }
  }
}
