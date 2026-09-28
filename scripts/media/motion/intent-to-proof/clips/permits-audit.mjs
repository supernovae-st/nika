// permits-audit · "The file is the boundary."
// release-brief declares its blast radius in `permits:`, and one task
// fetches a host outside it. The map is drawn from the file itself: the
// declared hosts and write paths, and each task's literal url or path
// (read from scripts/media/fixtures/permits-*.nika, never typed here).
// The verdicts are the real `nika check` transcripts of both fixtures,
// and the fix is the real line diff between them.
import { C, E, seg, smooth, repoLines, NIKA_VERSION, codeCard, terminal, frame, headline, loopFade, panel, cameraPath, frameBox, titleFade, WIDE } from './kit.mjs';
import { text, rrect, circle, bezierPts, poly, cross, check, measure } from '../src/engine/render.mjs';

export const meta = { duration: 23.3, poster: 21.7 };

const before = repoLines('scripts/media/fixtures/permits-escape.nika');
const after = repoLines('scripts/media/fixtures/permits-fits.nika');
const esc = repoLines('media/raw/check-permits-escape.txt');
const fits = repoLines('media/raw/check-permits-fits.txt');
const at = (arr, re) => arr.findIndex(l => re.test(l));

// ── what the file declares and what its tasks reach ─────────────────────
const listIn = (lines, key, field) => {
  const l = lines.find(x => new RegExp(`^\\s+${key}:`).test(x));
  const m = l && l.match(new RegExp(`${field}:\\s*(\\[[^\\]]*\\])`));
  if (!m) throw new Error(`permits.${key}.${field} not found in the fixture`);
  return JSON.parse(m[1]);
};
const reachOf = lines => {
  const out = [];
  let task = null, inTasks = false;
  for (const l of lines) {
    if (/^tasks:/.test(l)) { inTasks = true; continue; }
    const k = inTasks && l.match(/^ {2}([a-z_]+):\s*$/);
    if (k) { task = k[1]; continue; }
    const u = task && l.match(/url:\s*"https?:\/\/([^/"]+)/);
    if (u) out.push({ task, kind: 'http', target: u[1] });
    const p = task && l.match(/path:\s*"([^"]+)"/);
    if (p) out.push({ task, kind: 'write', target: p[1] });
  }
  return out;
};
const HOSTS = listIn(before, 'net', 'http');
const HOSTS_FIXED = listIn(after, 'net', 'http');
const WRITES = listIn(before, 'fs', 'write');
const REACH = reachOf(before);
const ESCAPE = REACH.find(r => r.kind === 'http' && !HOSTS.includes(r.target));
if (!ESCAPE || !HOSTS_FIXED.includes(ESCAPE.target)) throw new Error('the fixtures no longer show one escape and its fix');

// ── transcripts ─────────────────────────────────────────────────────────
// every vector down to the verdict line; hints, the NEXT pointer and the
// file-name note stay out (the typed command already names the file)
const escShown = esc.slice(1, at(esc, /^ ✖ findings above/u) + 1).filter(l => l.trim() && !/^ ↳ /u.test(l));
const fitsShown = fits.slice(1, at(fits, /^ layers ·/u) + 1).filter(l => l.trim() && !/^ ↳ /u.test(l));
const WRAP = /^ ✖ PERMITS /u;

const T = {
  card: 0.35, map: 0.5, term: 0.7,
  permits: 1.0, reach: 1.2,
  fence: 0.8, chips: 1.0, pills: 1.1, edges: 1.2,
  cmd1: 2.0, out1: 3.6,
  red: 8.2,
  fix: 11.1, fixEnd: 12.1,
  clear: 13.3, cmd2: 13.5, out2: 15.2,
  widen: 20.0,
};
// the check prints at once, as the CLI does: pushed in, every streamed row
// would scroll (repaint) the whole frame
const EVERY = 0.012;
T.finding = T.out1 + at(escShown, WRAP) * EVERY;
T.ready = T.out2 + (fitsShown.length - 1) * EVERY;

const CODE = { f: 'MM 400', size: 16 };
const TERM = { f: 'MM 400', size: 16 };
const LH = 22;
const CODEBOX = { x: 64, y: 244, w: 860, h: 780 };
const MAPBOX = { x: 956, y: 244, w: 900, h: 400 };
const TERMBOX = { x: 956, y: 668, w: 900, h: 356 };
const codeRowY = n => CODEBOX.y + 44 + 32 + (n - 4) * LH; // the window opens at line 4

// Few moves: every frame of a camera move repaints the whole GIF frame.
// The room with its boundary, the audit, the finding landing on the
// boundary, the fix, the re-check; the boundary widens as the camera leaves.
const SHOTS = [
  { at: 0, cam: WIDE },
  { at: 2.2, cam: frameBox(TERMBOX, 16), move: 0.7 },
  { at: T.red, cam: frameBox(MAPBOX, 16), move: 0.6 },
  { at: 11.0, cam: frameBox({ x: CODEBOX.x, y: codeRowY(4) - 30, w: CODEBOX.w, h: codeRowY(30) - codeRowY(4) + 50 }), move: 0.6 },
  { at: 13.8, cam: frameBox(TERMBOX, 16), move: 0.6 },
  { at: 19.85, cam: WIDE, move: 0.65 },
];
export const camera = t => cameraPath(t, SHOTS);

export function env() {
  return { bgGlow: 1, gridAlpha: 0.22, gridX: 0, gridY: 0, bgY: 560 };
}

export function chrome(R, t) {
  const a = loopFade(t, meta.duration);
  const cam = camera(t);
  frame(R, t, { kicker: 'permits · the static audit', plate: `output captured from the real cli · nika ${NIKA_VERSION} · map drawn from the file`, alpha: a, scrim: smooth(1, 1.12, cam.s) });
  headline({ ...R, fade: a * titleFade(cam) }, t, 0.2, meta.duration + 1, 'The file is the boundary.', 'PERMITS DECLARED IN THE FILE · AN ESCAPE IS CAUGHT BEFORE ANYTHING RUNS', { accent: 'the boundary.', accentColor: C.teal });
}

// a mono label in a rounded box; returns its width
function chip(R, x, y, label, { color = C.mist, alpha = 1, glow = 0, st = { f: 'MM 400', size: 15 } } = {}) {
  const w = measure(label, st) + 24;
  rrect(R, x, y - 15, w, 30, 9, { color, w: 1.2, alpha, fill: color, fillAlpha: 0.08, glow: glow * 0.5 });
  text(R, label, x + 12, y + 5, { ...st, color, alpha, glow });
  return w;
}

function map(R, t, a) {
  const fixedFile = t >= T.fix;
  panel(R, MAPBOX, { title: `${fixedFile ? 'permits-fits' : 'permits-escape'}.nika · what each task reaches`, alpha: a, k: E.snap(seg(t, T.map, T.map + 0.5)) });
  const caught = smooth(T.red, T.red + 0.4, t) * (1 - smooth(T.widen, T.widen + 0.5, t));
  const widen = E.glide(seg(t, T.widen, T.widen + 0.8));

  // the fence: the declared boundary; its right edge moves out on purpose
  const fx0 = 980, fy0 = 300, fy1 = 612, fx1 = 1600 + (1834 - 1600) * widen;
  const fk = E.snap(seg(t, T.fence, T.fence + 0.6));
  if (fk > 0) {
    rrect(R, fx0, fy0, (fx1 - fx0) * fk, fy1 - fy0, 18, { color: C.teal, w: 1.4, alpha: a * 0.6, dash: [7, 6], fill: C.teal, fillAlpha: 0.025, glow: 0.3 });
    text(R, 'PERMITS · THE DECLARED BOUNDARY', fx0 + 18, fy0 + 26, { f: 'MGW 500', size: 11, tracking: 3, color: C.teal, alpha: a * fk });
  }

  // the tasks that reach something, and what they reach
  const tasks = [...new Set(REACH.map(r => r.task))];
  const rowY = i => 380 + i * 84;
  const pk = smooth(T.pills, T.pills + 0.4, t) * a;
  const pillR = [];
  tasks.forEach((task, i) => {
    pillR[i] = pk > 0 ? 1004 + chip(R, 1004, rowY(i), task, { color: C.ink, alpha: pk }) : 1004;
  });
  const ck = smooth(T.chips, T.chips + 0.4, t) * a;
  REACH.forEach((r, j) => {
    const i = tasks.indexOf(r.task);
    const out = r === ESCAPE;
    const x = out ? 1650 : 1330, y = rowY(i);
    const inside = !out || widen > 0.9;
    const col = out ? (inside ? C.teal : caught > 0 ? C.red : C.amber) : C.mist;
    const label = r.kind === 'http' ? `http · ${r.target}` : `write · ${r.target}`;
    if (ck <= 0) return;
    chip(R, x, y, label, { color: col, alpha: ck, glow: out ? 0.5 : 0 });
    // the reach: task → target, drawn in
    const p = E.inOutCubic(seg(t, T.edges + j * 0.18, T.edges + j * 0.18 + 0.5));
    if (p <= 0 || pk <= 0) return;
    const pts = bezierPts([pillR[i] + 4, y], [pillR[i] + 60, y], [x - 60, y], [x - 4, y], 24);
    poly(R, pts.slice(0, Math.max(2, Math.round(pts.length * p))), { color: col, w: 1.5, alpha: a * 0.85, glow: out ? 0.7 : 0.2 });
    if (out && p >= 1) {
      // where it crosses the boundary: amber until the audit, red once caught
      const cx = 1600;
      if (!inside) {
        circle(R, cx, y, 11, { color: col, w: 1.5, alpha: a, fill: C.bg0, fillAlpha: 0.9, glow: 0.5 });
        if (caught > 0) cross(R, cx, y, 9, 1, { color: C.red, w: 2, alpha: a * caught, glow: 0.8 });
        else text(R, '?', cx, y + 5, { f: 'MM 500', size: 15, color: C.amber, alpha: a, align: 'center' });
        if (caught > 0) text(R, 'NIKA-SEC-004 · OUTSIDE permits.net.http', cx, y - 26, { f: 'MGW 500', size: 11, tracking: 2.2, color: C.red, alpha: a * caught, align: 'center', glow: 0.3 });
      } else {
        check(R, x - 22, y, 12, smooth(T.widen + 0.6, T.widen + 0.9, t), { color: C.teal, w: 2, alpha: a, glow: 0.7 });
      }
    }
  });
  // provenance, inside the map
  text(R, 'drawn from the file: permits: + each task\'s literal url and path', MAPBOX.x + 24, MAPBOX.y + MAPBOX.h - 16, { f: 'MM 400', size: 12, color: C.dim, alpha: a * smooth(T.edges, T.edges + 0.4, t) });
}

export function draw(R, t) {
  const a = loopFade(t, meta.duration);
  const fixing = t >= T.fix;
  const escRow = at(before, /url:\s*"https:\/\/nika\.sh/) + 1;
  const netRow = at(before, /^\s+net:/) + 1;
  const netRowAfter = at(after, /^\s+net:/) + 1;
  const capRow = at(after, /max_tokens:/) + 1;
  codeCard(R, t, CODEBOX, {
    title: fixing ? 'permits-fits.nika' : 'permits-escape.nika',
    alpha: a, k: E.snap(seg(t, T.card, T.card + 0.5)),
    before, after, win: { a: [4, 35], b: [4, 36] },
    reveal: { t0: T.card + 0.2 - 4 * 0.015, every: 0.015 },
    morph: { t0: T.fix, t1: T.fixEnd },
    st: CODE, lh: LH,
    badge: fixing ? { label: 'THE FIX · A REAL DIFF', color: C.teal, alpha: smooth(T.fix, T.fix + 0.3, t) }
      : t > T.finding ? { label: '1 ESCAPE', color: C.red, alpha: smooth(T.finding, T.finding + 0.3, t) } : null,
    highlights: [
      { line: netRow, t0: T.permits, t1: T.fix, c: C.teal, icon: 'none' },
    ],
    marks: [
      { line: escRow, re: /https:\/\/nika\.sh\/changelog/, c: C.amber, t0: T.reach, t1: T.finding, squiggle: true, version: 'before' },
      { line: escRow, re: /https:\/\/nika\.sh\/changelog/, c: C.red, t0: T.finding, t1: T.fix + 0.2, squiggle: true, version: 'before' },
      { line: escRow, re: /nika\.sh/, c: C.amber, t0: T.reach, t1: T.fix + 0.2, version: 'before' },
      { line: netRowAfter, re: /"nika\.sh"/, c: C.teal, t0: T.fixEnd - 0.3, version: 'after' },
      { line: capRow, re: /max_tokens: \d+/, c: C.teal, t0: T.fixEnd - 0.3, version: 'after' },
    ],
  });

  map(R, t, a);

  const ready = smooth(T.ready, T.ready + 0.3, t);
  terminal(R, t, TERMBOX, [
    { t: T.cmd1, cmd: 'nika check permits-escape.nika', dur: 0.95 },
    { t: T.out1, out: escShown, every: EVERY, wrap: WRAP, marks: [{ re: /host `nika\.sh` is outside permits\.net\.http/, c: C.red }, { re: /fix: add "nika\.sh" to permits\.net\.http/, c: C.teal }] },
    { t: T.clear, clear: true },
    { t: T.cmd2, cmd: 'nika check permits-fits.nika', dur: 0.95 },
    { t: T.out2, out: fitsShown, every: EVERY, marks: [{ re: /^ ✔ PERMITS {2}literal \+ const: args fit the boundary/u, c: C.teal, glow: 0.6 }, { re: /run ready ✔/u, c: C.teal, glow: 0.8 }] },
  ], {
    title: 'terminal', alpha: a, k: E.snap(seg(t, T.term, T.term + 0.5)), st: TERM, lh: LH,
    badge: t < T.finding ? null : t < T.clear ? { label: '1 ESCAPE · EXIT 2', color: C.red, alpha: smooth(T.finding, T.finding + 0.3, t) * (1 - smooth(T.clear - 0.3, T.clear, t)) }
      : { label: 'RUN READY', color: C.teal, alpha: ready },
  });
}
