// chat-to-workflow · "Chat is for trying. Nika is for keeping."
// The prompt someone retypes every Monday, and the workflow that keeps it.
// The chat is an illustration (the words a person would type), and so is
// the mapping of its phrases onto tasks. The file is the real
// meeting-actions fixture; its verdict is `nika check` under the run's
// model (media/raw/check-meeting-ollama.txt); the run and the action items
// are the committed capture of a real local model (run-meeting.txt,
// action-items.json). The last frame is the chat-vs-keeping social card.
import { C, E, seg, smooth, repoLines, readRepo, NIKA_VERSION, codeCard, frame, headline, loopFade, panel, pill, mono, cameraPath, frameBox, titleFade, WIDE } from './kit.mjs';
import { text, rrect, line, bezierPts, poly, circle, measure } from '../src/engine/render.mjs';

export const meta = { duration: 18.4, poster: 17.3, social: 'chat-vs-keeping-1600x900.png' };

const program = repoLines('scripts/media/fixtures/meeting-actions.nika');
const check = repoLines('media/raw/check-meeting-ollama.txt');
const run = repoLines('media/raw/run-meeting.txt');
const items = JSON.parse(readRepo('media/raw/action-items.json'));
const MODEL = (run.find(l => /infer ·/u.test(l)) || '').match(/infer · (\S+)/u)?.[1] ?? 'ollama/llama3.2:3b';
const DONE = run.find(l => /\d+\/\d+ done/u.test(l)).replace(/[─\s]+$/u, '').trim();
if (!check.some(l => /run ready ✔/u.test(l))) throw new Error('the meeting audit is no longer run ready');
const lineOf = key => program.findIndex(l => l === `  ${key}:`) + 1;

// the chat: the same request, retyped three Mondays running (illustration)
const ASKS = [
  { who: 'you · two mondays ago', s: 'Can you summarize this meeting, pull out the action items…' },
  { who: 'you · last monday', s: 'Can you summarize this meeting, pull out every action item with owners…' },
  { who: 'you · monday 09:12', s: 'Can you summarize this meeting, pull out every action item with its owner and deadline, and format it as JSON?' },
];
// what each part of the last ask becomes in the file
const MAP = [
  { phrase: 'this meeting', task: 'transcript', c: C.cyan },
  { phrase: 'every action item with its owner and deadline', task: 'extract', c: C.teal },
  { phrase: 'format it as JSON', task: 'save', c: C.gold },
];

const T = {
  chat: 0.35, file: 0.5, result: 0.65,
  asks: [0.5, 0.9, 1.3],
  again: 1.9,
  map0: 6.1, mapEvery: 0.6,
  verdict: 9.6,
  run: 12.7,
  close: 16.8,
};
const CHAT = { x: 64, y: 244, w: 760, h: 780 };
const FILE = { x: 856, y: 244, w: 1000, h: 560 };
const RESULT = { x: 856, y: 874, w: 1000, h: 150 };
const CODE = { f: 'MM 400', size: 16 };
const LH = 19;
// the file's window and the schema folded away inside it
const WIN = [19, 56], FOLD = [34, 48];
const rowOf = ln => ln - WIN[0] - (ln > FOLD[1] ? FOLD[1] - FOLD[0] : 0);
const CAPTION_Y = 846;
const BODY = { f: 'Geist 500', size: 24 };

const SHOTS = [
  { at: 0, cam: WIDE },
  { at: 2.6, cam: frameBox({ x: CHAT.x, y: CHAT.y, w: CHAT.w, h: 560 }, 16), move: 0.7 },
  { at: T.map0, cam: WIDE, move: 0.7 },
  { at: T.verdict + 0.1, cam: frameBox(FILE, 12), move: 0.7 },
  { at: T.run + 0.1, cam: frameBox({ x: RESULT.x, y: RESULT.y - 60, w: RESULT.w, h: RESULT.h + 60 }, 12), move: 0.7 },
  { at: T.close + 0.1, cam: WIDE, move: 0.7 },
];
export const camera = t => cameraPath(t, SHOTS);

export function env() {
  return { bgGlow: 1, gridAlpha: 0.22, gridX: 0, gridY: 0, bgY: 560 };
}

export function chrome(R, t) {
  const a = loopFade(t, meta.duration);
  const cam = camera(t);
  frame(R, t, { kicker: 'chat is for trying · nika is for keeping', plate: `chat and mapping: illustration · file, check (nika ${NIKA_VERSION}) and run: real · run: a captured local model (${MODEL})`, alpha: a, scrim: smooth(1, 1.12, cam.s) });
  headline({ ...R, fade: a * titleFade(cam) }, t, 0.2, meta.duration + 1, 'Chat is for trying. Nika is for keeping.', 'IF YOU ASK IT TWICE, KEEP IT AS A WORKFLOW', { accent: 'Nika is for keeping.', accentColor: C.teal });
}

// word-wrap a proportional line; returns [{ w, x, y }] word boxes
function layoutWords(s, st, x0, y0, maxW, lh) {
  const out = [];
  let x = x0, y = y0;
  const space = measure(' ', st);
  for (const w of s.split(' ')) {
    const ww = measure(w, st);
    if (x > x0 && x + ww > x0 + maxW) { x = x0; y += lh; }
    out.push({ w, x, y, ww });
    x += ww + space;
  }
  return out;
}

// the words of `phrase` inside the words of `s`: [first, last] indices
function phraseRange(s, phrase) {
  const words = s.split(' '), p = phrase.split(' ');
  for (let i = 0; i + p.length <= words.length; i++) {
    if (p.every((w, j) => words[i + j].replace(/[,?.…]$/, '') === w.replace(/[,?.…]$/, ''))) return [i, i + p.length - 1];
  }
  throw new Error(`phrase not in the ask: ${phrase}`);
}

// the bubbles' layout depends on nothing but their text: computed once
const LAST = ASKS.length - 1;
const BUBBLES = (() => {
  let y = CHAT.y + 76;
  return ASKS.map((ask, i) => {
    const st = { ...BODY, size: i < LAST ? 21 : 24 };
    const words = layoutWords(ask.s, st, CHAT.x + 52, y + 60, CHAT.w - 120, st.size * 1.35);
    const b = { y, st, words, h: words[words.length - 1].y - y + 34 };
    y += b.h + 22;
    return b;
  });
})();
const RANGES = MAP.map(m => phraseRange(ASKS[LAST].s, m.phrase));

function bubbles(R, t, a) {
  ASKS.forEach((ask, i) => {
    const k = E.snap(seg(t, T.asks[i], T.asks[i] + 0.4));
    if (k <= 0) return;
    const { y, st, words, h } = BUBBLES[i];
    const old = i < LAST;
    const ba = a * k * (old ? 0.55 : 1);
    rrect(R, CHAT.x + 28, y + 6 * (1 - k), CHAT.w - 56, h, 18, { color: old ? C.faint : C.dim, w: 1.2, alpha: ba, fill: '#0b1628', fillAlpha: 0.9 });
    text(R, ask.who.toUpperCase(), CHAT.x + 52, y + 28, { f: 'MGW 500', size: 11, tracking: 2.5, color: C.dim, alpha: ba });
    words.forEach((wd, j) => {
      const mi = old ? -1 : RANGES.findIndex(r => j >= r[0] && j <= r[1]);
      const on = mi >= 0 ? smooth(T.map0 + mi * T.mapEvery, T.map0 + mi * T.mapEvery + 0.3, t) : 0;
      text(R, wd.w, wd.x, wd.y, { ...st, color: on > 0 ? MAP[mi].c : old ? C.mist : C.ink, alpha: ba, glow: on * 0.3 });
      if (on > 0) line(R, wd.x, wd.y + 7, wd.x + wd.ww, wd.y + 7, { color: MAP[mi].c, w: 2, alpha: ba * on, glow: 0.6 });
    });
  });
  // the same request again: nothing of the last one was kept
  const ak = smooth(T.again, T.again + 0.4, t) * a;
  const bottom = BUBBLES[LAST].y + BUBBLES[LAST].h + 22;
  if (ak > 0) pill(R, CHAT.x + 28, bottom + 10, 'THE SAME REQUEST · THIRD MONDAY IN A ROW', C.amber, ak);
}

export function draw(R, t) {
  const a = loopFade(t, meta.duration);
  panel(R, CHAT, { title: 'a chat · illustration', alpha: a, k: E.snap(seg(t, T.chat, T.chat + 0.5)) });
  if (t > T.chat + 0.4) bubbles(R, t, a);
  const ck = smooth(T.close, T.close + 0.5, t) * a;
  if (ck > 0) text(R, 'scrolled away · retyped · never reviewed', CHAT.x + 32, CAPTION_Y, { f: 'MM 500', size: 18, color: C.amber, alpha: ck });

  codeCard(R, t, FILE, {
    title: 'meeting-actions.nika', alpha: a, k: E.snap(seg(t, T.file, T.file + 0.5)),
    before: program, win: { a: WIN }, fold: [FOLD],
    reveal: { t0: T.file + 0.2 - 19 * 0.015, every: 0.015 }, st: CODE, lh: LH,
    badge: t > T.verdict ? { label: 'NIKA CHECK · RUN READY', color: C.teal, alpha: smooth(T.verdict, T.verdict + 0.3, t) } : null,
    highlights: MAP.map((m, i) => ({ line: lineOf(m.task), t0: T.map0 + i * T.mapEvery + 0.45, c: m.c, icon: 'none' })),
  });

  // phrase → task: each part of the ask lands on the task that keeps it
  {
    MAP.forEach((m, i) => {
      const t0 = T.map0 + i * T.mapEvery;
      const k = seg(t, t0, t0 + 0.5);
      const fade = 1 - smooth(T.verdict - 0.6, T.verdict - 0.2, t);
      if (k <= 0 || fade <= 0) return;
      const w0 = BUBBLES[LAST].words[RANGES[i][1]];
      const x0 = w0.x + w0.ww + 6, y0 = w0.y - 8;
      const ln = lineOf(m.task);
      const x1 = FILE.x + 66, y1 = FILE.y + 44 + 32 + rowOf(ln) * LH - 6;
      const pts = bezierPts([x0, y0], [x0 + 160, y0], [x1 - 160, y1], [x1, y1], 32);
      const p = E.inOutCubic(k);
      poly({ ...R, fade: a * fade }, pts.slice(0, Math.max(2, Math.round(pts.length * p))), { color: m.c, w: 1.6, alpha: 0.9, glow: 0.8 });
      if (p >= 1) circle({ ...R, fade: a * fade }, x1, y1, 4, { fill: m.c, glow: 1 });
    });
  }

  // kept: the run of the kept file, and what it wrote
  panel(R, RESULT, { title: `nika run meeting-actions.nika --model ${MODEL}`, alpha: a, k: E.snap(seg(t, T.result, T.result + 0.5)), badge: t > T.run - 0.2 ? { label: 'TYPED OUTPUT', color: C.teal, alpha: smooth(T.run - 0.2, T.run + 0.1, t) } : null });
  const rk = smooth(T.run - 0.4, T.run, t) * a;
  if (rk > 0) {
    mono(R, [{ s: DONE, c: C.dim }], RESULT.x + 24, RESULT.y + 70, { alpha: rk, st: { f: 'MM 400', size: 15 }, max: 90 });
    items.forEach((it, i) => {
      const ik = smooth(T.run + i * 0.12, T.run + 0.25 + i * 0.12, t) * a;
      const y = RESULT.y + 96 + i * 22;
      text(R, it.owner, RESULT.x + 24, y, { f: 'Geist 600', size: 17, color: C.ink, alpha: ik });
      mono(R, [{ s: it.task, c: C.mist }], RESULT.x + 120, y, { alpha: ik, st: { f: 'MM 400', size: 15 }, max: 58 });
      text(R, it.due ? `due ${it.due}` : 'no due date', RESULT.x + RESULT.w - 24, y, { f: 'MGW 500', size: 11, tracking: 2.5, color: it.due ? C.teal : C.dim, alpha: ik, align: 'right' });
    });
  }
  const kk = smooth(T.close + 0.2, T.close + 0.7, t) * a;
  if (kk > 0) text(R, 'versioned in git · checked before it runs · run again any monday', FILE.x + 8, CAPTION_Y, { f: 'MM 500', size: 18, color: C.teal, alpha: kk });
}
