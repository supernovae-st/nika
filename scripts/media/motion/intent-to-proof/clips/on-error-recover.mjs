// on-error-recover · "Resilience is declared."
// rates-with-fallback reads a live feed that is gone. The file says what
// happens then: `on_error: recover` hands the task the cached snapshot,
// the sink still writes, and the snapshot says it is stale. Everything is
// real and offline: the run and the file it published (media/raw/
// run-recover.txt, recover-rates.json), the failure as the run's trace
// records it (recover-event.json) and what its code means (`nika explain`,
// explain-exec-001.txt), all captured by capture-transcripts.sh.
import { C, E, seg, smooth, repoLines, readRepo, NIKA_VERSION, codeCard, terminal, frame, headline, loopFade, panel, mono, cameraPath, frameBox, titleFade, WIDE } from './kit.mjs';
import { text, line } from '../src/engine/render.mjs';

export const meta = { duration: 20, poster: 18.6 };

const program = repoLines('scripts/media/fixtures/recover-fallback.nika');
const run = repoLines('media/raw/run-recover.txt');
const rates = readRepo('media/raw/recover-rates.json').trim();
const event = JSON.parse(readRepo('media/raw/recover-event.json').trim().split('\n')[0]);
const explain = repoLines('media/raw/explain-exec-001.txt');
const field = key => event.fields.find(f => f.key === key)?.value;
const CODE_ID = field('code'), TASK = field('task');
if (!CODE_ID || !TASK || !explain[0].startsWith(CODE_ID)) throw new Error('the recover capture no longer names its failure');
// "NIKA-EXEC-001 · process_error · transient: false" + "non-zero exit code (…)"
const KIND = explain[0].split(' · ')[1];
const MEANING = explain.find((l, i) => i > 0 && l.trim())?.trim();
const lineOf = re => program.findIndex(l => re.test(l)) + 1;

// the run's own lines; the `explore:` pointer is a hint and stays out
const runShown = run.filter(l => !/^\s*explore:/u.test(l));

const T = {
  card: 0.35, term: 0.5, rec: 0.65,
  cmd1: 1.9, out1: 3.4,
  cmd2: 6.3, out2: 7.4,
  code: 10.3,
  record: 14.3,
};
const CODE = { f: 'MM 400', size: 17 };
const TERM = { f: 'MM 400', size: 16 };
const CODE_LH = 26, LH = 24;
const CODEBOX = { x: 64, y: 244, w: 800, h: 780 };
const TERMBOX = { x: 896, y: 244, w: 960, h: 470 };
const RECBOX = { x: 896, y: 738, w: 960, h: 286 };
const codeRowY = n => CODEBOX.y + 44 + 32 + (n - 12) * CODE_LH; // the window opens at line 12

// Few moves: every frame of a camera move repaints the whole GIF frame.
const SHOTS = [
  { at: 0, cam: WIDE },
  { at: 2.2, cam: frameBox(TERMBOX, 16), move: 0.7 },
  { at: T.code, cam: frameBox({ x: CODEBOX.x, y: codeRowY(12) - 30, w: CODEBOX.w, h: codeRowY(32) - codeRowY(12) + 50 }), move: 0.7 },
  { at: T.record, cam: frameBox(RECBOX, 16), move: 0.7 },
  { at: 17.7, cam: WIDE, move: 0.7 },
];
export const camera = t => cameraPath(t, SHOTS);

export function env() {
  return { bgGlow: 1, gridAlpha: 0.22, gridX: 0, gridY: 0, bgY: 560 };
}

export function chrome(R, t) {
  const a = loopFade(t, meta.duration);
  const cam = camera(t);
  frame(R, t, { kicker: 'on_error · recover · a real offline run', plate: `run, file and trace captured from the real cli · nika ${NIKA_VERSION} · offline`, alpha: a, scrim: smooth(1, 1.12, cam.s) });
  headline({ ...R, fade: a * titleFade(cam) }, t, 0.2, meta.duration + 1, 'Resilience is declared.', 'ON_ERROR: RECOVER IS IN THE FILE · THE RUN FINISHES · THE OUTPUT SAYS STALE', { accent: 'declared.', accentColor: C.teal });
}

export function draw(R, t) {
  const a = loopFade(t, meta.duration);
  const c0 = T.code - 0.2;
  codeCard(R, t, CODEBOX, {
    title: 'recover-fallback.nika', alpha: a, k: E.snap(seg(t, T.card, T.card + 0.5)),
    before: program, win: { a: [12, 36] },
    reveal: { t0: T.card + 0.2 - 12 * 0.015, every: 0.015 }, st: CODE, lh: CODE_LH,
    badge: t > c0 ? { label: '3/3 DONE · 1 RECOVERED', color: C.amber, alpha: smooth(c0, c0 + 0.3, t) } : null,
    highlights: [
      { line: lineOf(/^ {2}cache:/), t0: c0, c: C.teal },
      { line: lineOf(new RegExp(`^ {2}${TASK}:`)), t0: c0 + 0.3, c: C.amber, icon: 'recover' },
      { line: lineOf(/^ {2}publish:/), t0: c0 + 0.6, c: C.teal },
      { line: lineOf(/^ {4}on_error:/), t0: c0 + 0.9, c: C.teal, icon: 'none' },
      { line: lineOf(/^ {6}recover:/), t0: c0 + 0.9, c: C.teal, icon: 'none' },
    ],
    marks: [
      // marked while the camera travels, so the line lands already split
      { line: lineOf(/command:/), re: /live-rates\.json/, c: C.red, t0: T.code - 0.9, squiggle: true },
      { line: lineOf(/command:/), re: /live-rates\.json/, c: C.red, t0: T.code - 0.9 },
    ],
  });

  terminal(R, t, TERMBOX, [
    { t: T.cmd1, cmd: 'nika run recover-fallback.nika', dur: 0.95 },
    { t: T.out1, out: runShown, every: 0.06, marks: [{ re: /· recovered$/u, c: C.amber }, { re: /1 recovered/, c: C.amber }, { re: /wrote \.\/out\/rates\.json \(\d+B\)/, c: C.teal }] },
    { t: T.cmd2, cmd: 'cat out/rates.json', dur: 0.7 },
    { t: T.out2, out: [rates], marks: [{ re: /"stale":true/, c: C.amber, glow: 0.6 }] },
  ], {
    title: 'terminal', alpha: a, k: E.snap(seg(t, T.term, T.term + 0.5)), st: TERM, lh: LH,
    badge: t > T.out1 + 0.5 ? { label: 'EXIT 0', color: C.teal, alpha: smooth(T.out1 + 0.5, T.out1 + 0.8, t) } : null,
  });

  // the record: the failure the recover absorbed, as the trace keeps it
  panel(R, RECBOX, { title: 'the run\'s trace · .nika/traces', alpha: a, k: E.snap(seg(t, T.rec, T.rec + 0.5)), badge: t > T.record - 0.3 ? { label: 'RECORDED', color: C.amber, alpha: smooth(T.record - 0.3, T.record, t) } : null });
  const rows = [
    ['TRACE EVENT', [{ s: event.kind, c: C.ink, b: true }, { s: ' · task ', c: C.dim }, { s: TASK, c: C.ink }, { s: ' · code ', c: C.dim }, { s: CODE_ID, c: C.amber, b: true, glow: 0.4 }]],
    ['NIKA EXPLAIN', [{ s: `${CODE_ID} · `, c: C.dim }, { s: KIND, c: C.ink }]],
    ['', [{ s: MEANING, c: C.mist }]],
  ];
  rows.forEach(([label, spans], i) => {
    const rk = smooth(T.record - 0.5 + i * 0.15, T.record - 0.2 + i * 0.15, t) * a;
    if (rk <= 0) return;
    const y = RECBOX.y + 104 + i * 54;
    if (label) text(R, label, RECBOX.x + 28, y - 3, { f: 'MGW 500', size: 11, tracking: 2.5, color: C.dim, alpha: rk });
    mono(R, spans, RECBOX.x + 200, y, { alpha: rk, st: { f: 'MM 400', size: 18 }, max: 58 });
    if (i === 1) line(R, RECBOX.x + 24, y - 34, RECBOX.x + RECBOX.w - 24, y - 34, { color: C.line, w: 1, alpha: rk });
  });
}
