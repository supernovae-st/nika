// static-check-fix · "Check before it runs."
// The real broken fixture, the real `nika check` verdict against it, the
// real fix (computed as a line diff between the two fixtures), and the real
// re-check. Every line on screen is read from scripts/media/fixtures/ and
// media/raw/ (captured from the binary by capture-transcripts.sh).
// The camera pushes in on whatever is being read: the verdict, the line it
// names, the diff, the re-check; it pulls back for the settled last frame.
import { C, E, seg, smooth, repoLines, NIKA_VERSION, codeCard, terminal, frame, headline, loopFade, cw, cameraPath, frameBox, titleFade, WIDE, layoutRows } from './kit.mjs';
import { line, circle, bezierPts, poly } from '../src/engine/render.mjs';

export const meta = { duration: 21, poster: 18.4, social: 'check-before-run-1600x900.png' };

const before = repoLines('scripts/media/fixtures/broken-pr-review.nika');
const after = repoLines('scripts/media/fixtures/fixed-pr-review.nika');
const broken = repoLines('media/raw/check-broken.txt');
const fixed = repoLines('media/raw/check-fixed.txt');
const at = (arr, re) => arr.findIndex(l => re.test(l));

// the verdict against the broken file: both conformance findings with their
// source excerpts, the two permits findings, and the summary line
// (the CLI's own "nika check · file" header is left out: the typed command
// above it already names the file)
const brokenShown = [
  ...broken.slice(1, at(broken, /^ PLAN/)),
  ...broken.filter(l => /^ ✖ (PERMITS|findings above)/u.test(l)),
];
// the re-check: every vector up to the hints, then the verdict and the layers
const fixedShown = [
  ...fixed.slice(1, at(fixed, /^ ↳ HINT/u)),
  ...fixed.filter(l => /^ ⚠ audited|^ layers ·/u.test(l)),
];

const T = {
  card: 0.35, term: 0.7,
  cmd1: 1.9, out1: 3.0,
  beam: 6.4, typo: 7.0, hoist: 7.5,
  fix: 9.4, fixEnd: 10.4,
  clear: 12.0, cmd2: 12.3, out2: 13.3,
};
T.ready = T.out2 + (fixedShown.length - 1) * 0.085;

const CODE = { f: 'MM 400', size: 17 };
const TERM = { f: 'MM 400', size: 16 };
const LH = 22;
const CODEBOX = { x: 64, y: 244, w: 860, h: 780 }, TERMBOX = { x: 956, y: 244, w: 900, h: 780 };
// a finding's message wraps (its fix is the point); the source excerpts,
// the re-check's vectors and the summary keep one row each
const WRAP = /^ ✖ (CONFORM|PERMITS) /u;
const brokenRows = layoutRows(brokenShown, { box: TERMBOX, st: TERM, wrap: WRAP });
// where the CLI's excerpt of line 28 sits in the terminal, and line 28 in the file
const EXCERPT = brokenShown.findIndex(l => /^28 │/u.test(l));
const EXCERPT_ROW = brokenRows.findIndex(r => r.line === EXCERPT);
const termRowY = i => TERMBOX.y + 44 + 30 + (i + 1) * LH; // +1: the typed command row
const codeRowY = n => CODEBOX.y + 44 + 32 + (n - 15) * LH;

// shots: the terminal's first screen, the file around the two findings,
// the re-check following its output down to the verdict, then the room
// Few moves: every frame of a camera move repaints the whole GIF frame.
// One terminal shot holds either verdict whole, so nothing scrolls away.
const termAll = frameBox({ x: TERMBOX.x, y: TERMBOX.y, w: TERMBOX.w, h: termRowY(Math.max(brokenRows.length, fixedShown.length) - 1) + 30 - TERMBOX.y });
const codeFix = frameBox({ x: CODEBOX.x, y: codeRowY(15) - 60, w: CODEBOX.w, h: 520 });
const SHOTS = [
  { at: 0, cam: WIDE },
  { at: 2.0, cam: termAll, move: 0.7 },
  { at: T.beam + 0.75, cam: codeFix, move: 0.75 },
  { at: T.cmd2 + 0.2, cam: termAll, move: 0.6 },
  { at: T.ready + 2.0, cam: WIDE, move: 0.8 },
];
export const camera = t => cameraPath(t, SHOTS);

export function env() {
  return { bgGlow: 1, gridAlpha: 0.22, gridX: 0, gridY: 0, bgY: 560 };
}

export function chrome(R, t) {
  const a = loopFade(t, meta.duration);
  const cam = camera(t);
  frame(R, t, { kicker: 'nika check · the static audit', plate: `output captured from the real cli · nika ${NIKA_VERSION}`, alpha: a, scrim: smooth(1, 1.12, cam.s) });
  headline({ ...R, fade: a * titleFade(cam) }, t, 0.2, meta.duration + 1, 'Check before it runs.', 'NOTHING RUNS, NO TOKEN IS SPENT · EVERY TASK AUDITED FIRST', { accent: 'before it runs.', accentColor: C.teal });
}

export function draw(R, t) {
  const a = loopFade(t, meta.duration);
  const fixing = t >= T.fix;
  codeCard(R, t, CODEBOX, {
    title: fixing ? 'fixed-pr-review.nika' : 'broken-pr-review.nika',
    alpha: a, k: E.snap(seg(t, T.card, T.card + 0.5)),
    before, after, win: { a: [15, 35], b: [14, 44] },
    reveal: { t0: T.card + 0.2 - 15 * 0.015, every: 0.015 },
    morph: { t0: T.fix, t1: T.fixEnd },
    st: CODE, lh: LH,
    badge: t < T.out1 + 0.4 ? null : fixing ? { label: 'THE FIX · A REAL DIFF', color: C.teal, alpha: smooth(T.fix, T.fix + 0.3, t) } : { label: '2 CONFORMANCE FINDINGS', color: C.red, alpha: smooth(T.out1 + 0.4, T.out1 + 0.7, t) },
    marks: [
      { line: 28, re: /asses\b/, c: C.red, t0: T.typo, t1: T.fix + 0.2, squiggle: true, version: 'before' },
      { line: 28, re: /asses\b/, c: C.red, t0: T.typo, t1: T.fix + 0.2, version: 'before' },
      { line: 19, re: /\$\{\{ tasks\.diff\.output\.stdout \}\}/, c: C.amber, t0: T.hoist, t1: T.fix + 0.2, squiggle: true, version: 'before' },
      { line: 31, re: /assess\b/, c: C.teal, t0: T.fixEnd - 0.3, version: 'after' },
      { line: 16, re: /diff:/, c: C.teal, t0: T.fixEnd - 0.3, version: 'after' },
    ],
  });

  const ready = smooth(T.ready, T.ready + 0.3, t);
  const findings = broken.filter(l => /^ ✖ (CONFORM|PERMITS)/u.test(l)).length;
  terminal(R, t, TERMBOX, [
    { t: T.cmd1, cmd: 'nika check broken-pr-review.nika', dur: 0.95 },
    { t: T.out1, out: brokenShown, every: 0.07, wrap: WRAP, marks: [{ re: /\basses\b/, c: C.red }, { re: /did you mean `assess`\?/, c: C.teal }] },
    { t: T.clear, clear: true },
    { t: T.cmd2, cmd: 'nika check fixed-pr-review.nika', dur: 0.95 },
    { t: T.out2, out: fixedShown, every: 0.085, marks: [{ re: /run ready ✔/u, c: C.teal, glow: 0.8 }] },
  ], {
    title: 'terminal', alpha: a, k: E.snap(seg(t, T.term, T.term + 0.5)), st: TERM, lh: LH,
    badge: t < T.out1 + 0.3 ? null : t < T.clear ? { label: `${findings} FINDINGS · EXIT 2`, color: C.red, alpha: smooth(T.out1 + 0.3, T.out1 + 0.6, t) * (1 - smooth(T.clear - 0.3, T.clear, t)) }
      : { label: 'RUN READY', color: C.teal, alpha: ready },
  });

  // the finding points at the exact line: a beam from the CLI's excerpt of
  // line 28 to the typo in the file, drawn as the camera follows it, and
  // held until the fix lands
  const beam = smooth(T.beam, T.beam + 0.3, t) * (1 - smooth(T.fix - 0.2, T.fix + 0.1, t));
  if (beam > 0 && EXCERPT_ROW >= 0) {
    // it leaves the terminal at the excerpt row's edge, not through its text
    const src = before[27];
    const x0 = TERMBOX.x + 12, y0 = termRowY(EXCERPT_ROW) - 6;
    const x1 = CODEBOX.x + 70 + (src.indexOf('asses') + 5) * cw(CODE), y1 = codeRowY(28) - 6;
    const pts = bezierPts([x0, y0], [x0 - 120, y0], [x1 + 150, y1], [x1 + 4, y1], 40);
    const p = E.inOutCubic(seg(t, T.beam, T.beam + 0.75));
    circle({ ...R, fade: a }, x0, y0, 3.5, { fill: C.red, alpha: beam, glow: 1 });
    poly({ ...R, fade: a }, pts.slice(0, Math.max(2, Math.round(pts.length * p))), { color: C.red, w: 1.6, alpha: beam * 0.85, glow: 0.9 });
    circle({ ...R, fade: a }, x1 + 4, y1, 3.5, { fill: C.red, alpha: beam * smooth(0.9, 1, p), glow: 1 });
  }

  // the verdict line gets its moment
  if (ready > 0) {
    const sweep = seg(t, T.ready, T.ready + 0.9);
    const y = termRowY(fixedShown.length - 1) + 8;
    const x = TERMBOX.x + 24 + (TERMBOX.w - 48) * E.inOutCubic(sweep);
    if (sweep < 1) line({ ...R, fade: a }, x - 120, y, x, y, { color: C.teal, w: 2, alpha: (1 - sweep) * 0.9, glow: 1 });
  }
}
