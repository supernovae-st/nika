// static-check-fix · "Check before it runs."
// The real broken fixture, the real `nika check` verdict against it, the
// real fix (computed as a line diff between the two fixtures), and the real
// re-check. Every line on screen is read from scripts/media/fixtures/ and
// media/raw/ (captured from the binary by capture-transcripts.sh).
import { C, E, seg, smooth, repoLines, NIKA_VERSION, codeCard, terminal, frame, headline, loopFade } from './kit.mjs';
import { line, circle, bezierPts, poly } from '../src/engine/render.mjs';
import { cw } from './kit.mjs';

export const meta = { duration: 20, poster: 17.2, social: 'check-before-run-1600x900.png' };

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
  cmd1: 1.3, out1: 2.45,
  typo: 3.1, hoist: 3.6,
  fix: 8.2, fixEnd: 9.7,
  clear: 9.9, cmd2: 10.1, out2: 11.25,
  ready: 11.25 + (fixedShown.length - 1) * 0.085,
};

const CODE = { f: 'MM 400', size: 17 };
const TERM = { f: 'MM 400', size: 16 };
const LH = 22;
const CODEBOX = { x: 64, y: 244, w: 860, h: 780 }, TERMBOX = { x: 956, y: 244, w: 900, h: 780 };
// where the CLI's excerpt of line 28 sits in the terminal, and line 28 in the file
const EXCERPT = brokenShown.findIndex(l => /^28 │/u.test(l));
const termRowY = i => TERMBOX.y + 44 + 30 + (i + 1) * LH; // +1: the typed command row
const codeRowY = n => CODEBOX.y + 44 + 32 + (n - 15) * LH;

export function env() {
  return { bgGlow: 1, gridAlpha: 0.22, gridX: 0, gridY: 0, bgY: 560 };
}

export function draw(R, t) {
  const a = loopFade(t, meta.duration);
  frame(R, t, { kicker: 'nika check · the static audit', plate: `output captured from the real cli · nika ${NIKA_VERSION}`, alpha: a });
  headline({ ...R, fade: a }, t, 0.2, meta.duration + 1, 'Check before it runs.', 'NOTHING RUNS, NO TOKEN IS SPENT · EVERY TASK AUDITED FIRST', { accent: 'before it runs.', accentColor: C.teal });

  const fixing = t >= T.fix;
  codeCard(R, t, CODEBOX, {
    title: fixing ? 'fixed-pr-review.nika' : 'broken-pr-review.nika',
    alpha: a, k: E.snap(seg(t, T.card, T.card + 0.5)),
    before, after, win: { a: [15, 35], b: [14, 44] },
    reveal: { t0: T.card + 0.2 - 15 * 0.03, every: 0.03 },
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
    { t: T.out1, out: brokenShown, every: 0.07, marks: [{ re: /\basses\b/, c: C.red }, { re: /did you mean `assess`\?/, c: C.teal }] },
    { t: T.clear, clear: true },
    { t: T.cmd2, cmd: 'nika check fixed-pr-review.nika', dur: 0.95 },
    { t: T.out2, out: fixedShown, every: 0.085, marks: [{ re: /run ready ✔/u, c: C.teal, glow: 0.8 }] },
  ], {
    title: 'terminal', alpha: a, k: E.snap(seg(t, T.term, T.term + 0.5)), st: TERM, lh: LH,
    badge: t < T.out1 + 0.3 ? null : t < T.clear ? { label: `${findings} FINDINGS · EXIT 2`, color: C.red, alpha: smooth(T.out1 + 0.3, T.out1 + 0.6, t) * (1 - smooth(T.clear - 0.3, T.clear, t)) }
      : { label: 'RUN READY', color: C.teal, alpha: ready },
  });

  // the finding points at the exact line: a beam from the CLI's excerpt of
  // line 28 to the typo in the file, held until the fix lands
  const beam = smooth(T.out1 + (EXCERPT + 0.5) * 0.07, T.out1 + (EXCERPT + 0.5) * 0.07 + 0.45, t) * (1 - smooth(T.fix - 0.2, T.fix + 0.1, t));
  if (beam > 0 && EXCERPT >= 0) {
    const src = before[27];
    const x0 = TERMBOX.x + 24 + (brokenShown[EXCERPT].indexOf('asses')) * cw(TERM), y0 = termRowY(EXCERPT) - 6;
    const x1 = CODEBOX.x + 70 + (src.indexOf('asses') + 5) * cw(CODE), y1 = codeRowY(28) - 6;
    const pts = bezierPts([x0 - 4, y0], [x0 - 150, y0], [x1 + 150, y1], [x1 + 4, y1], 40);
    const p = E.inOutCubic(seg(t, T.out1 + (EXCERPT + 0.5) * 0.07, T.out1 + (EXCERPT + 0.5) * 0.07 + 0.45));
    poly({ ...R, fade: a }, pts.slice(0, Math.max(2, Math.round(pts.length * p))), { color: C.red, w: 1.6, alpha: beam * 0.85, glow: 0.9 });
    circle({ ...R, fade: a }, x1 + 4, y1, 3.5, { fill: C.red, alpha: beam * smooth(0.9, 1, p), glow: 1 });
  }

  // the verdict line gets its moment
  if (ready > 0) {
    const sweep = seg(t, T.ready, T.ready + 0.9);
    const x = 980 + 840 * E.inOutCubic(sweep);
    if (sweep < 1) line({ ...R, fade: a }, x - 120, 996, x, 996, { color: C.teal, w: 2, alpha: (1 - sweep) * 0.9, glow: 1 });
  }
}
