// full-loop · "Compile. Check. Run. Verify."
// The README's front door, as it runs: `nika compile hello hello.nika`,
// `nika check`, `nika run` (mock/echo: a rehearsal, and it says so) and
// `nika trace verify`, captured offline by capture-transcripts.sh
// (media/raw/loop-*). The proof card compares the chain the run printed
// with the head verify read back; the clip refuses to render if they differ.
import { C, E, seg, smooth, repoLines, readRepo, NIKA_VERSION, codeCard, terminal, frame, headline, loopFade, panel, mono, cameraPath, frameBox, titleFade, WIDE } from './kit.mjs';
import { text, rrect, line, check } from '../src/engine/render.mjs';

export const meta = { duration: 23.5, poster: 22.4 };

const compile = repoLines('media/raw/loop-compile.txt');
const program = readRepo('media/raw/loop-hello.nika').replace(/\n$/, '').split('\n');
const checkOut = repoLines('media/raw/loop-check.txt');
const run = repoLines('media/raw/loop-run.txt');
const verify = repoLines('media/raw/loop-verify.txt');
const CHAIN = run.join('\n').match(/chain ([0-9a-f]{64})/)?.[1];
const HEAD = verify.join('\n').match(/head ([0-9a-f]{64})/)?.[1];
if (!CHAIN || CHAIN !== HEAD) throw new Error('the loop capture: the verified head is not the chain the run printed');
if (!checkOut.some(l => /run ready ✔/u.test(l))) throw new Error('the loop capture: hello.nika is not run ready');

// the lines each step shows: hints and pointers stay out; verify keeps
// its verdict and the caveat that comes with it
const checkShown = checkOut.slice(1);
const runShown = run.filter(l => !/^\s*(explore:|see it whole:)/u.test(l));
const verifyShown = verify.slice(0, 4);
const WRAP = /^rehearsal:|^\s+trace:|^OK —/u;

const T = {
  strip: 0.35, code: 0.5, term: 0.6, proof: 0.75,
  cmd1: 2.1, out1: 3.3,
  cmd2: 4.0, out2: 5.2,
  clear: 10.7, cmd3: 10.9, out3: 12.1,
  cmd4: 13.3, out4: 14.4,
  match: 14.9,
};
T.checked = T.out2 + (checkShown.length - 1) * 0.02;
T.ran = T.out3 + (runShown.length - 1) * 0.05;
const STEPS = [
  { label: 'compile', at: T.out1 + 0.1 },
  { label: 'check', at: T.checked + 0.1 },
  { label: 'run', at: T.ran + 0.1 },
  { label: 'verify', at: T.match },
];
const TERM = { f: 'MM 400', size: 16 };
const LH = 19;
const STRIP = { x: 64, y: 244, w: 1792, h: 60 };
const CODEBOX = { x: 64, y: 328, w: 620, h: 300 };
const PROOF = { x: 64, y: 652, w: 620, h: 372 };
const TERMBOX = { x: 716, y: 328, w: 1140, h: 696 };

// each terminal page is framed by the rows it fills, not the whole box
const pageBox = rows => ({ x: TERMBOX.x, y: TERMBOX.y, w: TERMBOX.w, h: 44 + 30 + rows * LH + 16 });
const PAGE1 = 1 + compile.length + 1 + checkShown.length;
const PAGE2 = 22;
const SHOTS = [
  { at: 0, cam: WIDE },
  { at: 2.3, cam: frameBox(pageBox(PAGE1), 12), move: 0.7 },
  { at: T.cmd3 + 0.2, cam: frameBox(pageBox(PAGE2), 12), move: 0.6 },
  { at: 18.7, cam: frameBox({ x: CODEBOX.x, y: CODEBOX.y, w: CODEBOX.w, h: PROOF.y + PROOF.h - CODEBOX.y }, 12), move: 0.7 },
  { at: 21.5, cam: WIDE, move: 0.7 },
];
export const camera = t => cameraPath(t, SHOTS);

export function env() {
  return { bgGlow: 1, gridAlpha: 0.22, gridX: 0, gridY: 0, bgY: 560 };
}

export function chrome(R, t) {
  const a = loopFade(t, meta.duration);
  const cam = camera(t);
  frame(R, t, { kicker: 'the loop you keep · the readme\'s first file', plate: `all four commands captured from the real cli · nika ${NIKA_VERSION} · the run is a mock/echo rehearsal`, alpha: a, scrim: smooth(1, 1.12, cam.s) });
  headline({ ...R, fade: a * titleFade(cam) }, t, 0.2, meta.duration + 1, 'Compile. Check. Run. Verify.', 'OFFLINE · ZERO KEYS · EVERY STEP LEAVES SOMETHING YOU CAN READ', { accent: 'Verify.', accentColor: C.teal });
}

function strip(R, t, a) {
  const k = E.snap(seg(t, T.strip, T.strip + 0.5));
  if (k <= 0) return;
  const w = (STRIP.w - 3 * 28) / 4;
  STEPS.forEach((s, i) => {
    const x = STRIP.x + i * (w + 28), y = STRIP.y;
    const on = smooth(s.at, s.at + 0.3, t);
    rrect(R, x, y, w * k, STRIP.h, 12, { color: on > 0 ? C.teal : C.faint, w: 1.2 + on * 0.6, alpha: a, fill: C.teal, fillAlpha: 0.03 + on * 0.08, glow: on * 0.5 });
    text(R, `${i + 1}`, x + 24, y + 38, { f: 'MGW 500', size: 13, tracking: 2, color: C.dim, alpha: a * k });
    text(R, `nika ${s.label === 'verify' ? 'trace verify' : s.label}`, x + 54, y + 38, { f: 'MM 500', size: 20, color: on > 0 ? C.ink : C.mist, alpha: a * k });
    check(R, x + w - 34, y + 30, 16, E.snap(seg(t, s.at, s.at + 0.35)), { color: C.teal, w: 2.2, alpha: a, glow: 0.8 });
    if (i < 3) line(R, x + w + 4, y + 30, x + w + 24, y + 30, { color: on > 0 ? C.teal : C.faint, w: 1.4, alpha: a * k });
  });
}

function proof(R, t, a) {
  panel(R, PROOF, { title: 'the proof · the chain', alpha: a, k: E.snap(seg(t, T.proof, T.proof + 0.5)), badge: t > T.match ? { label: 'SAME 64 HEX DIGITS', color: C.teal, alpha: smooth(T.match, T.match + 0.3, t) } : null });
  const rows = [
    ['NIKA RUN PRINTED', CHAIN, T.ran],
    ['NIKA TRACE VERIFY READ', HEAD, T.match - 0.2],
  ];
  const st = { f: 'MM 500', size: 19 };
  rows.forEach(([label, hash, at], i) => {
    const k = smooth(at, at + 0.3, t) * a;
    if (k <= 0) return;
    const y = PROOF.y + 92 + i * 118;
    text(R, label, PROOF.x + 28, y, { f: 'MGW 500', size: 11, tracking: 2.5, color: C.dim, alpha: k });
    const m = smooth(T.match, T.match + 0.4, t);
    for (let r = 0; r < 2; r++) mono(R, [{ s: hash.slice(r * 32, r * 32 + 32), c: m > 0 ? C.teal : C.ink, glow: m * 0.4 }], PROOF.x + 28, y + 32 + r * 28, { alpha: k, st });
  });
  const ck = smooth(T.match + 0.2, T.match + 0.5, t) * a;
  if (ck > 0) {
    text(R, 'chain intact · tamper-evident, not tamper-proof', PROOF.x + 28, PROOF.y + PROOF.h - 30, { f: 'MM 400', size: 15, color: C.mist, alpha: ck });
  }
}

export function draw(R, t) {
  const a = loopFade(t, meta.duration);
  strip(R, t, a);
  const wrote = smooth(T.out1 + 0.1, T.out1 + 0.4, t);
  codeCard(R, t, CODEBOX, {
    title: 'hello.nika · written by nika compile', alpha: a, k: E.snap(seg(t, T.code, T.code + 0.5)),
    before: program, reveal: { t0: T.out1 + 0.1 - 0.03, every: 0.03 }, st: { f: 'MM 400', size: 17 }, lh: 24,
    badge: wrote > 0 ? { label: t > T.checked ? 'RUN READY' : 'COMPILED', color: C.teal, alpha: wrote } : null,
  });
  proof(R, t, a);
  terminal(R, t, TERMBOX, [
    { t: T.cmd1, cmd: 'nika compile hello hello.nika', dur: 0.9 },
    { t: T.out1, out: compile, every: 0.08 },
    { t: T.cmd2, cmd: 'nika check hello.nika', dur: 0.8 },
    { t: T.out2, out: checkShown, every: 0.02, marks: [{ re: /run ready ✔/u, c: C.teal, glow: 0.8 }] },
    { t: T.clear, clear: true },
    { t: T.cmd3, cmd: 'nika run hello.nika', dur: 0.7 },
    { t: T.out3, out: runShown, every: 0.05, wrap: WRAP, marks: [{ re: /rehearsal/u, c: C.amber }, { re: /[0-9a-f]{64}/, c: C.teal, glow: 0.5 }] },
    { t: T.cmd4, cmd: 'nika trace verify', dur: 0.7 },
    { t: T.out4, out: verifyShown, every: 0.08, wrap: WRAP, marks: [{ re: /OK — .*chain intact/u, c: C.teal }, { re: /[0-9a-f]{64}/, c: C.teal, glow: 0.5 }] },
  ], { title: 'terminal · offline', alpha: a, k: E.snap(seg(t, T.term, T.term + 0.5)), st: TERM, lh: LH });
}
