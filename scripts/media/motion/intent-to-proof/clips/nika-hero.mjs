// nika-hero · "Audit first, then run."
// The audit-then-run story other repositories embed (media/nika-hero.gif).
// The check is captured from the current binary under the same model the
// run used (check never dials a server); the run is the committed
// capture of a real local model (ollama/llama3.2:3b), which
// capture-transcripts.sh keeps when no Ollama server is reachable. Its
// action items are that run's real output (media/raw/action-items.json).
// The camera follows the reading: the audit down to its verdict, the run,
// the file while the model task runs, then the items the run wrote.
import { C, E, seg, smooth, repoLines, readRepo, NIKA_VERSION, codeCard, terminal, frame, headline, loopFade, panel, mono, cameraPath, frameBox, titleFade, WIDE } from './kit.mjs';
import { text, line } from '../src/engine/render.mjs';

export const meta = { duration: 19.8, poster: 17.5, alsoGif: 'nika-hero.gif' };

const program = repoLines('scripts/media/fixtures/meeting-actions.nika');
const check = repoLines('media/raw/check-meeting-ollama.txt');
const run = repoLines('media/raw/run-meeting.txt');
const items = JSON.parse(readRepo('media/raw/action-items.json'));
const MODEL = (run.find(l => /infer ·/u.test(l)) || '').match(/infer · (\S+)/u)?.[1] ?? 'ollama/llama3.2:3b';
const at = (arr, re) => arr.findIndex(l => re.test(l));
const lineOf = key => program.findIndex(l => l === `  ${key}:`) + 1;

// the audit: every vector through the verdict and the layers line
const checkShown = check.slice(1).filter(l => !/^ ↳ (HINT|NEXT)/u.test(l) && l.trim());
// the run, row by row, on its own clock (the model call takes a while)
const runRow = re => run[at(run, re)];

const T = {
  card: 0.35, term: 0.7,
  cmd1: 1.9, out1: 3.1,
  clear: 6.6, cmd2: 6.8,
  head: 8.3, transcript: 8.55, running: 8.85, extract: 10.75, save: 11.0, done: 11.25, wrote: 11.45, trace: 11.75,
  items: 12.2,
};
T.ready = T.out1 + (checkShown.length - 1) * 0.075;
const CODE = { f: 'MM 400', size: 17 };
const TERM = { f: 'MM 400', size: 16 };
const LH = 24;
const CODEBOX = { x: 64, y: 244, w: 760, h: 780 };
const TERMBOX = { x: 856, y: 244, w: 1000, h: 780 };
const ITEMS = { x: 890, y: 610, w: 932, h: 300 };
const termRowY = r => TERMBOX.y + 44 + 30 + r * LH; // r = 0: the typed command
const codeRowY = r => CODEBOX.y + 44 + 32 + r * LH; // r = 0: the window's first line

// Few moves: every frame of a camera move repaints the whole GIF frame.
// The terminal shot holds the whole audit, then the run on the same screen.
const SHOTS = [
  { at: 0, cam: WIDE },
  { at: 2.1, cam: frameBox({ x: TERMBOX.x, y: TERMBOX.y, w: TERMBOX.w, h: termRowY(checkShown.length) + 30 - TERMBOX.y }), move: 0.7 },
  // the file while the model task runs: transcript done, extract spinning
  { at: 9.6, cam: frameBox({ x: CODEBOX.x, y: codeRowY(0) - 30, w: CODEBOX.w, h: codeRowY(17) - codeRowY(0) + 60 }), move: 0.6 },
  // the finished run and what it wrote
  { at: 12.6, cam: frameBox({ x: TERMBOX.x, y: termRowY(4) - 30, w: TERMBOX.w, h: ITEMS.y + ITEMS.h - termRowY(4) + 40 }), move: 0.6 },
  { at: 16.6, cam: WIDE, move: 0.8 },
];
export const camera = t => cameraPath(t, SHOTS);

export function env() {
  return { bgGlow: 1, gridAlpha: 0.22, gridX: 0, gridY: 0, bgY: 560 };
}

export function chrome(R, t) {
  const a = loopFade(t, meta.duration);
  const cam = camera(t);
  frame(R, t, { kicker: 'nika check · nika run · the audit-then-run story', plate: `check: nika ${NIKA_VERSION} · run: a captured local model run (${MODEL})`, alpha: a, scrim: smooth(1, 1.12, cam.s) });
  headline({ ...R, fade: a * titleFade(cam) }, t, 0.2, meta.duration + 1, 'Audit first, then run.', 'EVERY TASK AUDITED · RUN LOCALLY · A HASH-CHAINED TRACE', { accent: 'then run.', accentColor: C.teal });
}

export function draw(R, t) {
  const a = loopFade(t, meta.duration);
  codeCard(R, t, CODEBOX, {
    title: 'meeting-actions.nika', alpha: a, k: E.snap(seg(t, T.card, T.card + 0.5)),
    before: program, win: { a: [19, 56] }, fold: [[34, 47]],
    reveal: { t0: T.card + 0.2 - 19 * 0.015, every: 0.015 }, st: CODE, lh: LH,
    badge: t > T.ready + 0.3 ? { label: 'AUDITED · RUN READY', color: C.teal, alpha: smooth(T.ready + 0.3, T.ready + 0.6, t) } : null,
    highlights: [
      { line: lineOf('transcript'), t0: T.transcript, c: C.teal },
      { line: lineOf('extract'), t0: T.running, t1: T.extract, c: C.cyan, running: true },
      { line: lineOf('extract'), t0: T.extract, c: C.teal },
      { line: lineOf('save'), t0: T.save, c: C.teal },
    ],
  });

  terminal(R, t, TERMBOX, [
    { t: T.cmd1, cmd: `nika check meeting-actions.nika --model ${MODEL}`, dur: 1.1 },
    { t: T.out1, out: checkShown, every: 0.075, marks: [{ re: /run ready ✔/u, c: C.teal, glow: 0.8 }] },
    { t: T.clear, clear: true },
    { t: T.cmd2, cmd: `nika run meeting-actions.nika --model ${MODEL}`, dur: 1.2 },
    { t: T.head, out: run.slice(0, 3), every: 0.08 },
    { t: T.transcript, out: [runRow(/✔ {2}transcript/u)] },
    { t: T.running, out: [runRow(/^still running/u)] },
    { t: T.extract, out: [runRow(/✔ {2}extract/u)] },
    { t: T.save, out: [runRow(/✔ {2}save/u)] },
    { t: T.done, out: [runRow(/3\/3 done/u)] },
    { t: T.wrote, out: [runRow(/wrote /u)] },
    { t: T.trace, out: [runRow(/trace:/u)], marks: [{ re: /chain [0-9a-f]+/, c: C.teal, glow: 0.6 }] },
  ], { title: 'terminal', alpha: a, k: E.snap(seg(t, T.term, T.term + 0.5)), st: TERM, lh: LH });

  // the run's real output: the action items it wrote
  const ik = E.snap(seg(t, T.items, T.items + 0.5));
  if (ik > 0) {
    panel(R, ITEMS, { title: 'action-items.json · written by nika:write', alpha: a, k: ik, badge: { label: 'TYPED OUTPUT', color: C.teal, alpha: smooth(T.items + 0.3, T.items + 0.6, t) } });
    items.forEach((it, i) => {
      const rk = smooth(T.items + 0.35 + i * 0.12, T.items + 0.55 + i * 0.12, t) * a;
      if (rk <= 0) return;
      const y = ITEMS.y + 98 + i * 62;
      text(R, it.owner, ITEMS.x + 28, y, { f: 'Geist 600', size: 24, color: C.ink, alpha: rk });
      mono(R, [{ s: it.task, c: C.mist }], ITEMS.x + 170, y - 2, { alpha: rk, st: { f: 'MM 400', size: 16 }, max: 52 });
      text(R, it.due ? `due ${it.due}` : 'no due date', ITEMS.x + ITEMS.w - 28, y, { f: 'MGW 500', size: 11, tracking: 2.5, color: it.due ? C.teal : C.dim, alpha: rk, align: 'right' });
      if (i < items.length - 1) line(R, ITEMS.x + 24, y + 26, ITEMS.x + ITEMS.w - 24, y + 26, { color: C.line, w: 1, alpha: rk });
    });
  }
}
