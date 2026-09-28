// editor-diagnostics · "The audit, as you type."
// The editor is an illustration; everything it reports is real: the
// diagnostics `nika lsp` publishes (media/raw/lsp-*.json, captured by
// capture-transcripts.sh) for the broken fixture, for the same file once
// the `asses` typo is fixed with one keystroke, and for the fixed fixture.
// Squiggles sit on the ranges the server sends; hints (severity 4) draw
// as dots and stay out of the problems list, as editors show them.
import { C, E, seg, smooth, repoLines, readRepo, NIKA_VERSION, codeCard, frame, headline, loopFade, panel, mono, wrapSpans, cw, squiggle, cameraPath, frameBox, titleFade, WIDE } from './kit.mjs';
import { text, rrect, line, circle, poly } from '../src/engine/render.mjs';

export const meta = { duration: 16.5, poster: 15.6 };

const broken = repoLines('scripts/media/fixtures/broken-pr-review.nika');
const fixed = repoLines('scripts/media/fixtures/fixed-pr-review.nika');
const TYPO = broken.findIndex(l => /tasks\.asses\./.test(l));
const typoFixed = broken.map((l, i) => (i === TYPO ? l.replace('tasks.asses.', 'tasks.assess.') : l));
const DIAG = {
  broken: JSON.parse(readRepo('media/raw/lsp-broken.json')),
  typo: JSON.parse(readRepo('media/raw/lsp-typo-fixed.json')),
  fixed: JSON.parse(readRepo('media/raw/lsp-fixed.json')),
};
const errors = d => d.filter(x => x.severity === 1);
const HOVER = errors(DIAG.broken).find(d => d.range.start.line === TYPO);
if (!HOVER || errors(DIAG.typo).some(d => d.range.start.line === TYPO) || errors(DIAG.fixed).length) {
  throw new Error('the LSP captures no longer tell the typo story');
}

const T = {
  win: 0.35, diag: 1.3,
  pointer: 2.7, hover: 3.3, unhover: 6.3,
  type: 6.8, rediag: 7.1,
  fix: 12.1, fixEnd: 13.5,
};
const CODE = { f: 'MM 400', size: 17 };
const LH = 21;
const CODEBOX = { x: 64, y: 244, w: 1216, h: 780 };
const PROBS = { x: 1304, y: 244, w: 552, h: 780 };
const WIN_A = 9;
const rowY = line1 => CODEBOX.y + 44 + 32 + (line1 - WIN_A) * LH; // 1-based line, before the morph
const colX = ch => CODEBOX.x + 58 + 12 + ch * cw(CODE);
const typoRow = TYPO + 1;

const SHOTS = [
  { at: 0, cam: WIDE },
  { at: 2.6, cam: frameBox({ x: CODEBOX.x, y: rowY(typoRow) - 150, w: 980, h: 330 }, 12), move: 0.7 },
  { at: 8.9, cam: frameBox({ x: PROBS.x, y: PROBS.y, w: PROBS.w, h: 470 }, 12), move: 0.7 },
  { at: T.fix - 0.1, cam: WIDE, move: 0.7 },
];
export const camera = t => cameraPath(t, SHOTS);

export function env() {
  return { bgGlow: 1, gridAlpha: 0.22, gridX: 0, gridY: 0, bgY: 560 };
}

export function chrome(R, t) {
  const a = loopFade(t, meta.duration);
  const cam = camera(t);
  frame(R, t, { kicker: 'nika lsp · the editor extension', plate: `diagnostics: nika lsp ${NIKA_VERSION} on the broken and fixed fixtures · editor: illustration`, alpha: a, scrim: smooth(1, 1.12, cam.s) });
  headline({ ...R, fade: a * titleFade(cam) }, t, 0.2, meta.duration + 1, 'The audit, as you type.', 'THE SAME CHECKS IN THE EDITOR · FROM THE LANGUAGE SERVER · BEFORE A RUN', { accent: 'as you type.', accentColor: C.teal });
}

// which diagnostics are live at t
const state = t => (t >= T.fixEnd ? 'fixed' : t >= T.rediag ? 'typo' : 'broken');

function squiggles(R, t, a) {
  const on = smooth(T.diag, T.diag + 0.3, t) * (1 - smooth(T.fix - 0.3, T.fix, t));
  if (on <= 0) return;
  const lines = t >= T.type ? typoFixed : broken;
  for (const d of DIAG[state(t)]) {
    const ln = d.range.start.line;
    const src = lines[ln] ?? '';
    const c0 = d.range.start.character;
    // a point range underlines the rest of its line, as the server's
    // excerpt does; a span underlines itself
    const c1 = d.range.end.line === ln && d.range.end.character > c0 ? d.range.end.character : src.trimEnd().length;
    const y = rowY(ln + 1) + 4;
    if (d.severity === 4) {
      for (let i = 0; i < 3; i++) circle(R, colX(c0) + 3 + i * 5, y + 1, 1.3, { fill: C.dim, alpha: a * on });
      continue;
    }
    const k = d === HOVER ? 1 - smooth(T.rediag - 0.2, T.rediag, t) : 1;
    squiggle(R, colX(c0), colX(c1), y, d.severity === 1 ? C.red : C.amber, a * on * k, t);
  }
}

function problems(R, t, a) {
  const k = E.snap(seg(t, T.win + 0.15, T.win + 0.65));
  const errs = errors(DIAG[state(t)]);
  const on = smooth(T.diag, T.diag + 0.3, t);
  panel(R, PROBS, { title: `problems · ${t < T.diag ? '…' : errs.length}`, alpha: a, k, badge: t >= T.fixEnd ? { label: 'NO PROBLEMS', color: C.teal, alpha: smooth(T.fixEnd, T.fixEnd + 0.3, t) } : errs.length ? { label: `${errs.length} ERRORS`, color: C.red, alpha: on } : null });
  if (k < 1 || on <= 0) return;
  let y = PROBS.y + 84;
  const st = { f: 'MM 400', size: 14 };
  for (const d of errs) {
    const head = `${d.code ?? 'permits'} · Ln ${d.range.start.line + 1}, Col ${d.range.start.character + 1}`;
    mono(R, [{ s: '✖', c: C.red }], PROBS.x + 24, y, { alpha: a * on, st: { f: 'MM 500', size: 15 } });
    mono(R, [{ s: head, c: C.ink, b: true }], PROBS.x + 48, y, { alpha: a * on, st });
    const rows = wrapSpans([{ s: d.message.replace(/\n/g, ' · '), c: C.mist }], 46, 0).slice(0, 5);
    rows.forEach((r, i) => mono(R, r, PROBS.x + 48, y + 22 + i * 19, { alpha: a * on, st: { f: 'MM 400', size: 13 }, max: 48 }));
    y += 22 + rows.length * 19 + 22;
  }
  if (t >= T.fixEnd) {
    const hints = DIAG.fixed.filter(d => d.severity === 4).length;
    const fk = smooth(T.fixEnd, T.fixEnd + 0.4, t) * a;
    text(R, 'No problems in this file.', PROBS.x + 28, PROBS.y + 96, { f: 'Geist 600', size: 22, color: C.teal, alpha: fk });
    text(R, `${hints} hint${hints === 1 ? '' : 's'} · editors show hints in the code, not here`, PROBS.x + 28, PROBS.y + 128, { f: 'MM 400', size: 14, color: C.dim, alpha: fk });
  }
}

function hover(R, t, a) {
  const k = smooth(T.hover, T.hover + 0.25, t) * (1 - smooth(T.unhover, T.unhover + 0.25, t));
  // the pointer glides onto the typo, rests, then leaves for the caret
  const pk = E.inOutCubic(seg(t, T.pointer, T.hover));
  const px = colX(HOVER.range.start.character + 12) + (1 - pk) * 180, py = rowY(typoRow) - 4 + (1 - pk) * 120;
  const pa = a * smooth(T.pointer, T.pointer + 0.2, t) * (1 - smooth(T.unhover, T.unhover + 0.2, t));
  if (pa > 0) poly(R, [[px, py], [px, py + 20], [px + 5, py + 15], [px + 9, py + 23], [px + 12, py + 21], [px + 8, py + 14], [px + 14, py + 13], [px, py]], { color: C.ink, w: 1.2, alpha: pa, fill: C.bg0 });
  if (k <= 0) return;
  const box = { x: colX(HOVER.range.start.character) - 20, y: rowY(typoRow) + 18, w: 640, h: 132 };
  rrect(R, box.x, box.y, box.w, box.h, 10, { color: C.faint, w: 1, alpha: a * k, fill: '#0b1628', fillAlpha: 0.97 });
  mono(R, [{ s: HOVER.code, c: C.red, b: true }, { s: ` · ${HOVER.source}`, c: C.dim }], box.x + 18, box.y + 30, { alpha: a * k, st: { f: 'MM 400', size: 15 } });
  wrapSpans([{ s: HOVER.message, c: C.ink }], 60, 0).slice(0, 3).forEach((r, i) => mono(R, r, box.x + 18, box.y + 60 + i * 22, { alpha: a * k, st: { f: 'MM 400', size: 15 }, max: 62 }));
}

function caret(R, t, a) {
  if (t < T.unhover || t > T.rediag + 1.2) return;
  const src = broken[TYPO];
  const at = src.indexOf('asses') + 5 + (t >= T.type ? 1 : 0);
  const blink = Math.floor(t * 2.4) % 2 === 0 ? 1 : 0.25;
  line(R, colX(at), rowY(typoRow) - 17, colX(at), rowY(typoRow) + 4, { color: C.ice, w: 2, alpha: a * blink, glow: 0.8 });
}

export function draw(R, t) {
  const a = loopFade(t, meta.duration);
  const fixing = t >= T.fix;
  codeCard(R, t, CODEBOX, {
    title: 'pr-review.nika', alpha: a, k: E.snap(seg(t, T.win, T.win + 0.5)),
    before: t >= T.type ? typoFixed : broken, after: fixed, win: { a: [WIN_A, broken.length], b: [14, fixed.length] },
    reveal: { t0: T.win + 0.2 - WIN_A * 0.015, every: 0.015 },
    morph: { t0: T.fix, t1: T.fixEnd },
    st: CODE, lh: LH,
    badge: fixing ? { label: 'THE REST OF THE FIX', color: C.teal, alpha: smooth(T.fix, T.fix + 0.3, t) } : null,
  });
  squiggles(R, t, a);
  caret(R, t, a);
  hover(R, t, a);
  problems(R, t, a);
  // the status bar: what the server reports right now
  const errs = errors(DIAG[state(t)]).length;
  const sy = CODEBOX.y + CODEBOX.h - 14;
  const sk = smooth(T.diag, T.diag + 0.3, t) * a;
  if (sk > 0) mono(R, [{ s: 'nika lsp · ', c: C.dim }, errs ? { s: `✖ ${errs} errors`, c: C.red } : { s: '✔ no problems', c: C.teal }], CODEBOX.x + 24, sy, { alpha: sk, st: { f: 'MM 500', size: 14 } });
}
