// Building blocks for the feature clips, in the film's visual language.
// Everything a clip shows as program text or CLI output is read from a real
// file (a fixture, or a transcript captured from the real binary by
// scripts/media/capture-transcripts.sh). The kit only lays it out, colours
// it and animates its arrival. A line too long for its card is cut with an
// ellipsis after a verbatim prefix, never rewritten.
import fs from 'node:fs';
import path from 'node:path';
import { C, E, clamp, lerp, seg, smooth, rgba } from '../src/engine/core.mjs';
import { text, rrect, rect, line, circle, arc, poly, check, cross, measure, ROOT, DW, DH } from '../src/engine/render.mjs';
import { beatTitle } from '../src/scenes/shared.mjs';

export const REPO = path.resolve(ROOT, '../../../..');
export const readRepo = rel => fs.readFileSync(path.join(REPO, rel), 'utf8');
export const repoLines = rel => readRepo(rel).replace(/\n$/, '').split('\n');

// The binary the transcripts were captured with (media/raw/nika-version.txt).
export const NIKA_VERSION = readRepo('media/raw/nika-version.txt').trim().split(/\s+/)[1];

export const MONO = { f: 'MM 400', size: 17 };
export const MONOB = { f: 'MM 500', size: 17 };
export const LH = 25;
export const cw = (st = MONO) => measure('M', st);

// ── coloured monospace spans ────────────────────────────────────────────
// Martian Mono has no glyph for the CLI's status marks and box corners, and
// a system fallback font would make the render depend on the machine. These
// characters are drawn as vectors in their own monospace cell instead: the
// text is unchanged, only how the glyph is painted.
const VECTOR = new Set([...'✔✖⚠○↳▸╭╰━≥↔⋯🦋']);

function glyph(R, ch, x0, y, st, color, alpha, glow) {
  const adv = cw(st), s = st.size;
  const w = ch === '🦋' ? 2 * adv : adv;
  const cx = x0 + w / 2, cy = y - s * 0.34;
  const S = { color, w: Math.max(1.3, s * 0.1), alpha, glow };
  switch (ch) {
    case '✔': check(R, cx, cy, s * 0.62, 1, S); break;
    case '✖': cross(R, cx, cy, s * 0.46, 1, { ...S, w: Math.max(1.6, s * 0.13) }); break;
    case '⚠': {
      const r = s * 0.36;
      poly(R, [[cx, cy - r], [cx + r * 1.05, cy + r * 0.78], [cx - r * 1.05, cy + r * 0.78], [cx, cy - r]], S);
      line(R, cx, cy - r * 0.35, cx, cy + r * 0.25, S);
      circle(R, cx, cy + r * 0.52, s * 0.035, { fill: color, alpha, glow });
      break;
    }
    case '○': circle(R, cx, cy, s * 0.26, S); break;
    case '↳': poly(R, [[cx - adv * 0.25, cy - s * 0.32], [cx - adv * 0.25, cy + s * 0.12], [cx + adv * 0.38, cy + s * 0.12]], S);
      poly(R, [[cx + adv * 0.15, cy - s * 0.06], [cx + adv * 0.38, cy + s * 0.12], [cx + adv * 0.15, cy + s * 0.3]], S); break;
    case '▸': poly(R, [[cx - adv * 0.2, cy - s * 0.2], [cx + adv * 0.25, cy], [cx - adv * 0.2, cy + s * 0.2], [cx - adv * 0.2, cy - s * 0.2]], { ...S, fill: color }); break;
    case '╭': poly(R, [[x0 + adv, cy], [cx + adv * 0.2, cy], [cx, cy + s * 0.25], [cx, y + s * 0.5]], S); break;
    case '╰': poly(R, [[cx, y - s * 1.1], [cx, cy - s * 0.25], [cx + adv * 0.2, cy], [x0 + adv, cy]], S); break;
    case '━': line(R, x0, cy, x0 + adv, cy, { ...S, w: Math.max(2, s * 0.16) }); break;
    case '≥': poly(R, [[cx - adv * 0.25, cy - s * 0.3], [cx + adv * 0.25, cy - s * 0.1], [cx - adv * 0.25, cy + s * 0.1]], S);
      line(R, cx - adv * 0.25, cy + s * 0.28, cx + adv * 0.25, cy + s * 0.28, S); break;
    case '↔': line(R, x0 + adv * 0.1, cy, x0 + adv * 0.9, cy, S);
      poly(R, [[x0 + adv * 0.3, cy - s * 0.16], [x0 + adv * 0.1, cy], [x0 + adv * 0.3, cy + s * 0.16]], S);
      poly(R, [[x0 + adv * 0.7, cy - s * 0.16], [x0 + adv * 0.9, cy], [x0 + adv * 0.7, cy + s * 0.16]], S); break;
    case '⋯': for (let i = -1; i <= 1; i++) circle(R, cx + i * adv * 0.28, cy + s * 0.12, s * 0.05, { fill: color, alpha }); break;
    case '🦋': {
      const r = s * 0.3;
      for (const d of [-1, 1]) {
        poly(R, [[cx, cy], [cx + d * r * 1.5, cy - r * 1.2], [cx + d * r * 1.7, cy - r * 0.1], [cx, cy]], { ...S, color: C.ice, fill: C.ice });
        poly(R, [[cx, cy], [cx + d * r * 1.2, cy + r * 1.1], [cx + d * r * 0.3, cy + r * 1.2], [cx, cy]], { ...S, color: C.ice, fill: C.ice });
      }
      break;
    }
    default: break;
  }
}

// spans: [{ s, c?, b?, glow? }], drawn left to right on a fixed advance.
export function mono(R, spans, x, y, { alpha = 1, st = MONO, max = Infinity } = {}) {
  const adv = cw(st);
  let col = 0;
  for (const sp of spans) {
    if (col >= max) break;
    let s = sp.s;
    if (col + s.length > max) s = s.slice(0, Math.max(0, max - col - 1)) + '…';
    const style = { ...(sp.b ? { ...st, f: 'MM 500' } : st), color: sp.c || C.mist, alpha, glow: sp.glow || 0 };
    let run = '', runCol = col;
    // a terminal shows every character: where the mono font would join
    // two punctuation marks into a ligature (--, ==, ->, …), the run is
    // drawn one character per cell instead
    const flush = () => {
      if (!run.trim()) { run = ''; return; }
      if (/[!-/:-@[-`{-~]{2}/.test(run)) [...run].forEach((ch, i) => ch !== ' ' && text(R, ch, x + (runCol + i) * adv, y, style));
      else text(R, run, x + runCol * adv, y, style);
      run = '';
    };
    for (const ch of s) {
      if (VECTOR.has(ch)) {
        flush();
        glyph(R, ch, x + col * adv, y, st, style.color, alpha, style.glow);
        col += ch.length;
        runCol = col;
      } else {
        if (!run) runCol = col;
        run += ch;
        col += ch.length;
      }
    }
    flush();
  }
}

// Mark tokens in a line: [{ re, c, glow }] recolours matches. Each regex
// runs over the whole line, so a match may span the colourer's own spans
// (a YAML key and its colon); the first mark to claim a character keeps it.
function applyMarks(spans, marks = []) {
  if (!marks.length) return spans;
  const full = spans.map(sp => sp.s).join('');
  const owner = new Array(full.length).fill(null);
  for (const mk of marks) {
    const re = new RegExp(mk.re.source, mk.re.flags.includes('g') ? mk.re.flags : `${mk.re.flags}g`);
    let m;
    while ((m = re.exec(full))) {
      if (!m[0]) { re.lastIndex++; continue; }
      const end = m.index + m[0].length;
      if (owner.slice(m.index, end).every(o => o === null)) owner.fill(mk, m.index, end);
    }
  }
  const out = [];
  let pos = 0;
  for (const sp of spans) {
    for (let i = 0; i < sp.s.length;) {
      const mk = owner[pos + i];
      let j = i + 1;
      while (j < sp.s.length && owner[pos + j] === mk) j++;
      const s = sp.s.slice(i, j);
      out.push(mk ? { ...sp, s, c: mk.c, b: true, glow: mk.glow ?? 0.5 } : { ...sp, s });
      i = j;
    }
    pos += sp.s.length;
  }
  return out;
}

const MARK = { '✔': C.teal, '✖': C.red, '⚠': C.amber, '○': C.dim, '↳': C.cyan };

// A line of `nika check` / `nika run` output, coloured by its status mark.
export function cliSpans(raw, marks) {
  let spans;
  const m = raw.match(/^(\s*)(✔|✖|⚠|○|↳)(\s+)(\S+)(.*)$/u);
  if (m) {
    const [, ind, mk, sp, label, rest] = m;
    const col = MARK[mk];
    spans = [{ s: ind }, { s: mk, c: col, b: true, glow: mk === '○' ? 0 : 0.5 }, { s: sp }, { s: label, c: col, b: true }, { s: rest, c: mk === '✖' ? C.ink : C.mist }];
  } else if (/^\s*\$ /.test(raw)) {
    const i = raw.indexOf('$');
    spans = [{ s: raw.slice(0, i) }, { s: '$ ', c: C.dim }, { s: raw.slice(i + 2), c: C.ink, b: true }];
  } else if (/^nika (check|run)/.test(raw)) {
    spans = [{ s: raw, c: C.ink, b: true }];
  } else if (/^\s*\d+ │/.test(raw)) {
    const k = raw.indexOf('│');
    spans = [{ s: raw.slice(0, k + 1), c: C.dim }, { s: raw.slice(k + 1), c: C.mist }];
  } else if (/^\s*(╭|│|╰|fix:|──|note ·|layers ·)/u.test(raw)) {
    spans = [{ s: raw, c: /✔/.test(raw) ? C.teal : C.dim }];
  } else if (/^\s*🦋/u.test(raw)) {
    spans = [{ s: raw, c: C.ice, b: true }];
  } else {
    spans = [{ s: raw, c: C.mist }];
  }
  return applyMarks(spans, marks);
}

// A line of a `.nika` file, in the film's YAML colours.
export function yamlSpans(raw, marks) {
  let spans;
  if (/^\s*#/.test(raw)) spans = [{ s: raw, c: C.dim }];
  else {
    const m = raw.match(/^(\s*)(- )?([\w.-]+)(:)(.*)$/);
    if (!m) spans = [{ s: raw, c: C.mist }];
    else {
      const [, ind, dash = '', key, colon, rest] = m;
      const top = ind.length === 0;
      spans = [{ s: ind }, { s: dash, c: C.dim }, { s: key, c: top ? C.ice : ind.length === 2 ? C.ink : '#8DB4FF', b: top || ind.length === 2 }, { s: colon, c: C.dim }];
      const re = /(\$\{\{.*?\}\})|("[^"]*"|'[^']*')|(nika:[\w-]+)|(#.*$)/g;
      let last = 0, mm;
      while ((mm = re.exec(rest))) {
        if (mm.index > last) spans.push({ s: rest.slice(last, mm.index), c: C.mist });
        spans.push({ s: mm[0], c: mm[1] ? '#6FA0FF' : mm[2] ? '#C9D8EE' : mm[3] ? C.cyan : C.dim });
        last = mm.index + mm[0].length;
      }
      if (last < rest.length) spans.push({ s: rest.slice(last), c: C.mist });
    }
  }
  return applyMarks(spans, marks);
}

// ── panels ──────────────────────────────────────────────────────────────
// A card with a header bar; k unrolls it downwards (0..1).
export function panel(R, box, { title = '', alpha = 1, k = 1, badge = null } = {}) {
  if (alpha <= 0 || k <= 0) return;
  const { x, y, w, h } = box;
  rrect(R, x, y, w, Math.max(44, h * k), 14, { color: C.faint, w: 1, alpha, fill: '#050c19', fillAlpha: 0.9 });
  for (let i = 0; i < 3; i++) circle(R, x + 22 + i * 15, y + 22, 4, { fill: C.faint, alpha });
  if (title) text(R, title, x + 74, y + 28, { f: 'MM 500', size: 15, color: C.mist, alpha });
  line(R, x + 1, y + 44, x + w - 1, y + 44, { color: C.line, w: 1, alpha });
  if (badge) pill(R, x + w - 18, y + 22, badge.label, badge.color, alpha * (badge.alpha ?? 1), 'right');
}

export function pill(R, x, y, label, color, alpha, align = 'left') {
  if (alpha <= 0) return;
  const st = { f: 'MGW 500', size: 11, tracking: 2.5 };
  const w = measure(label, st) + 24;
  const x0 = align === 'right' ? x - w : x;
  rrect(R, x0, y - 12, w, 24, 12, { color, w: 1.2, alpha, fill: color, fillAlpha: 0.1, glow: 0.35 });
  text(R, label, x0 + 12, y + 4.5, { ...st, color, alpha, glow: 0.3 });
}

// Break a line of spans at spaces into rows of at most `max` cells; the
// continuation rows start `indent` cells in. Only where the line breaks
// changes: every character, colour and mark stays.
export function wrapSpans(spans, max, indent = 0) {
  const full = spans.map(sp => sp.s).join('');
  if (full.length <= max) return [spans];
  const cuts = [];
  let start = 0, width = max;
  while (full.length - start > width) {
    let cut = full.lastIndexOf(' ', start + width);
    if (cut <= start) cut = start + width;
    cuts.push([start, cut]);
    start = cut;
    while (full[start] === ' ') start++;
    width = max - indent;
  }
  cuts.push([start, full.length]);
  return cuts.map(([a, b], r) => {
    const row = r ? [{ s: ' '.repeat(indent) }] : [];
    let pos = 0;
    for (const sp of spans) {
      const s0 = Math.max(a, pos), s1 = Math.min(b, pos + sp.s.length);
      if (s1 > s0) row.push({ ...sp, s: full.slice(s0, s1) });
      pos += sp.s.length;
    }
    return row;
  });
}

// The terminal rows a block of captured lines occupies: [{ line, spans }].
// `wrap` (true, or a regex a line must match) breaks long lines instead of
// cutting them; the rest keep one row, cut with an ellipsis if too long.
export function layoutRows(lines, { box, st = MONO, marks, wrap = false, indent = 12 } = {}) {
  const max = Math.floor((box.w - 48) / cw(st));
  const rows = [];
  lines.forEach((l, line) => {
    const spans = cliSpans(l, marks);
    const w = wrap === true || (wrap instanceof RegExp && wrap.test(l));
    for (const row of w ? wrapSpans(spans, max, indent) : [spans]) rows.push({ line, spans: row });
  });
  return rows;
}

// ── terminal ────────────────────────────────────────────────────────────
// steps (clip clock, seconds):
//   { t, cmd: 'nika check file.nika', dur }         types a command after "$ "
//   { t, out: ['line', …], every = 0.06, marks, wrap } streams captured lines
//                                                    (wrap: see layoutRows)
//   { t, clear: true }                               clears the screen
// The newest line stays in view: the buffer scrolls smoothly upwards.
export function terminal(R, t, box, steps, { title = '', alpha = 1, k = 1, badge = null, lh = LH, st = MONO } = {}) {
  panel(R, box, { title, alpha, k, badge });
  if (k < 1 || alpha <= 0) return;
  const adv = cw(st);
  const max = Math.floor((box.w - 48) / adv);
  let rows = [];
  let fadeOld = 1;
  for (const sp of steps) {
    if (sp.clear && t >= sp.t - 0.25 && t < sp.t) fadeOld = 1 - smooth(sp.t - 0.25, sp.t, t);
    if (t < sp.t) break;
    if (sp.clear) { rows = []; continue; }
    if (sp.cmd) {
      const n = Math.floor(sp.cmd.length * clamp((t - sp.t) / (sp.dur ?? 0.9)));
      rows.push({ spans: cliSpans(`$ ${sp.cmd.slice(0, n)}`), born: sp.t, caret: n < sp.cmd.length || t < sp.t + (sp.dur ?? 0.9) + 0.25 });
    } else if (sp.out) {
      const every = sp.every ?? 0.06;
      for (const r of layoutRows(sp.out, { box, st, marks: sp.marks, wrap: sp.wrap, indent: sp.indent })) {
        const born = sp.t + r.line * every;
        if (t >= born) rows.push({ spans: r.spans, born });
      }
    } else if (sp.gap) rows.push({ spans: [], born: sp.t });
  }
  const top = box.y + 44 + 30, cap = Math.floor((box.h - 44 - 40) / lh);
  // scroll: the view follows the newest row, eased
  const over = Math.max(0, rows.length - cap);
  const lastBorn = rows.length ? rows[rows.length - 1].born : 0;
  const scrollK = E.outCubic(clamp((t - lastBorn) / 0.18));
  const scroll = Math.max(0, over - 1 + scrollK) * lh * (over > 0 ? 1 : 0);
  const ctx = R.ctx;
  ctx.save();
  ctx.beginPath();
  ctx.rect(box.x, box.y + 46, box.w, box.h - 50);
  ctx.clip();
  rows.forEach((r, i) => {
    const y = top + i * lh - scroll;
    if (y < box.y + 30 || y > box.y + box.h + lh) return;
    const a = alpha * fadeOld * smooth(r.born, r.born + 0.12, t);
    mono(R, r.spans, box.x + 24, y + 6 * (1 - smooth(r.born, r.born + 0.12, t)), { alpha: a, st, max });
    if (r.caret) {
      const n = r.spans.reduce((s, sp) => s + sp.s.length, 0);
      const blink = Math.floor(t * 2.4) % 2 === 0 ? 1 : 0.25;
      rect(R, box.x + 24 + n * adv + 2, y - st.size + 2, 2.5, st.size + 3, { fill: C.ice, alpha: alpha * blink, glow: 0.8 });
    }
  });
  ctx.restore();
}

// ── code card with a real diff ──────────────────────────────────────────
// Line diff (LCS) between two versions of a file: [{ op: '='|'-'|'+', a, b, text }]
export function lineDiff(A, B) {
  const n = A.length, m = B.length;
  const L = Array.from({ length: n + 1 }, () => new Int16Array(m + 1));
  for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) L[i][j] = A[i] === B[j] ? L[i + 1][j + 1] + 1 : Math.max(L[i + 1][j], L[i][j + 1]);
  const ops = [];
  let i = 0, j = 0;
  while (i < n || j < m) {
    if (i < n && j < m && A[i] === B[j]) { ops.push({ op: '=', a: i, b: j, text: A[i] }); i++; j++; }
    else if (j < m && (i === n || L[i][j + 1] >= L[i + 1][j])) { ops.push({ op: '+', b: j, text: B[j] }); j++; }
    else { ops.push({ op: '-', a: i, text: A[i] }); i++; }
  }
  return ops;
}

// 1 until t1 - d, eased to 0 at t1. A window with no end never fades:
// smooth(Infinity - d, Infinity, t) is NaN, which draws nothing.
const until = (t1, d, t) => (t1 === undefined || t1 === Infinity ? 1 : 1 - smooth(t1 - d, t1, t));

// Draws a code file. Before `morph.t0` it shows `before`; across
// [morph.t0, morph.t1] deleted lines fold away and added lines open with a
// teal flash; after, `after`. `win.a` / `win.b` are 1-based [first, last]
// line windows of each version. marks: [{ line, re, c, t0, t1, squiggle,
// version }] recolour or underline a token on a line of the version shown.
// fold: [[first, last]] hides a block behind one "⋯ n lines" row (single
// version only). highlights: [{ line, t0, t1?, c, running?, icon? }] light
// a row: a bar behind it and, in the gutter, a check (a pulse and a
// spinner while running; icon: 'recover' for a task that ended on its
// fallback; icon: 'none' marks a row without a verdict).
export function codeCard(R, t, box, { title = '', alpha = 1, k = 1, badge = null, before, after = null, win = {}, reveal = null, morph = null, marks = [], lh = LH, st = MONO, fold = [], highlights = [] } = {}) {
  const [fa, ta] = win.a ?? [1, Infinity], [fb, tb] = win.b ?? win.a ?? [1, Infinity];
  panel(R, box, { title, alpha, k, badge });
  if (k < 1 || alpha <= 0) return;
  const adv = cw(st);
  const gutter = 58;
  const max = Math.floor((box.w - gutter - 36) / adv);
  const mp = morph ? E.inOutCubic(seg(t, morph.t0, morph.t1)) : 0;
  const ops = after ? lineDiff(before, after) : before.map((text, a) => ({ op: '=', a, b: a, text }));
  let y = box.y + 44 + 32;
  const ctx = R.ctx;
  ctx.save();
  ctx.beginPath();
  ctx.rect(box.x, box.y + 46, box.w, box.h - 50);
  ctx.clip();
  for (const o of ops) {
    const na = o.a !== undefined ? o.a + 1 : null, nb = o.b !== undefined ? o.b + 1 : null;
    const inA = na !== null && na >= fa && na <= ta, inB = nb !== null && nb >= fb && nb <= tb;
    if (!inA && !inB) continue;
    const fr = !after && fold.find(([f0, f1]) => na >= f0 && na <= f1);
    if (fr) {
      if (na === fr[0]) {
        const rv0 = reveal ? smooth(reveal.t0 + na * reveal.every, reveal.t0 + na * reveal.every + 0.12, t) : 1;
        const ind = (before[na - 1].match(/^\s*/) || [''])[0].length;
        mono(R, [{ s: `⋯ ${fr[1] - fr[0] + 1} lines folded`, c: C.dim }], box.x + gutter + 12 + ind * adv, y, { alpha: alpha * rv0 * 0.9, st });
        y += lh;
      }
      continue;
    }
    // row height: deleted rows fold, added rows open, and a kept row
    // outside the new window folds away while one entering it opens
    const wa = inA ? 1 : 0, wb = inB ? 1 : 0;
    const hk = o.op === '=' ? wa + (wb - wa) * mp : o.op === '-' ? (1 - mp) * wa : mp * wb;
    if (hk <= 0.001) continue;
    const rowY = y + lh * 0.5 * (hk - 1);
    const shown = o.op === '-' ? 1 - smooth(0, 0.6, mp) : o.op === '+' ? smooth(0.35, 1, mp) : 1;
    const rv = reveal ? smooth(reveal.t0 + (inA ? na : nb) * reveal.every, reveal.t0 + (inA ? na : nb) * reveal.every + 0.12, t) : 1;
    const a = alpha * shown * rv;
    if (a > 0.004) {
      // a row opening or folding shows only the part of it its slot holds
      const partial = hk < 0.999;
      if (partial) {
        ctx.save();
        ctx.beginPath();
        ctx.rect(box.x, y - lh + 7, box.w, lh * hk);
        ctx.clip();
      }
      const num = mp < 0.5 ? na ?? nb : nb ?? na;
      text(R, String(num), box.x + gutter - 14, rowY, { f: 'MM 400', size: st.size - 3, color: C.dim, alpha: a * 0.8, align: 'right' });
      if (o.op !== '=' && morph && t >= morph.t0) {
        const fl = o.op === '+' ? smooth(0.35, 0.6, mp) * (1 - smooth(1, 1, mp)) : smooth(0, 0.25, mp) * (1 - smooth(0.25, 0.6, mp));
        const flash = o.op === '+' ? 0.5 + 0.5 * (1 - smooth(morph.t1, morph.t1 + 0.4, t)) : fl;
        rect(R, box.x + 6, rowY - lh + 7, box.w - 12, lh, { fill: o.op === '+' ? C.teal : C.red, alpha: a * 0.1 * flash });
        text(R, o.op, box.x + gutter - 2, rowY, { f: 'MM 500', size: st.size - 2, color: o.op === '+' ? C.teal : C.red, alpha: a * flash });
      }
      const lineNo = o.op === '-' || (mp < 0.5 && o.op === '=') ? na : nb;
      for (const hl of highlights) {
        if (hl.line !== lineNo || t < hl.t0 || t > (hl.t1 ?? Infinity)) continue;
        const hk = smooth(hl.t0, hl.t0 + 0.2, t) * until(hl.t1, 0.2, t);
        const pulse = hl.running ? 0.55 + 0.45 * Math.sin((t - hl.t0) * 7) : 1;
        rect(R, box.x + 6, rowY - lh + 7, box.w - 12, lh, { fill: hl.c, alpha: a * 0.12 * hk * pulse });
        rect(R, box.x + 6, rowY - lh + 7, 3, lh, { fill: hl.c, alpha: a * hk, glow: 0.8 });
        const gx = box.x + gutter + 2, gy = rowY - st.size * 0.34;
        const icon = hl.icon ?? (hl.running ? 'spin' : 'check');
        if (icon === 'check') check(R, gx, gy, st.size * 0.6, E.snap(seg(t, hl.t0, hl.t0 + 0.3)), { color: hl.c, w: 1.8, alpha: a * hk, glow: 0.7 });
        else if (icon === 'spin') arc(R, gx, gy, st.size * 0.3, (t - hl.t0) * 7, (t - hl.t0) * 7 + Math.PI * 1.4, { color: hl.c, w: 1.8, alpha: a * hk, glow: 0.7 });
        else if (icon === 'recover') {
          // a loop back: the task ended on its fallback, not its own result
          const r = st.size * 0.3, a0 = -Math.PI * 0.25, a1 = Math.PI * 1.3;
          const k = E.snap(seg(t, hl.t0, hl.t0 + 0.4));
          const S = { color: hl.c, w: 1.8, alpha: a * hk, glow: 0.7 };
          arc(R, gx, gy, r, a0, a0 + (a1 - a0) * k, S);
          if (k > 0.98) {
            const ex = gx + r * Math.cos(a1), ey = gy + r * Math.sin(a1);
            const tx = -Math.sin(a1), ty = Math.cos(a1), nx = Math.cos(a1), ny = Math.sin(a1);
            poly(R, [[ex + nx * 3, ey + ny * 3], [ex + tx * 3.5, ey + ty * 3.5], [ex - nx * 3, ey - ny * 3]], S);
          }
        }
      }
      const lineMarks = marks.filter(mk => mk.line === lineNo && (!mk.version || mk.version === (mp < 0.5 ? 'before' : 'after')) && t >= (mk.t0 ?? 0) && t <= (mk.t1 ?? Infinity));
      mono(R, yamlSpans(o.text, lineMarks.filter(mk => mk.re && !mk.squiggle)), box.x + gutter + 12, rowY, { alpha: a, st, max });
      for (const mk of lineMarks) {
        if (!mk.squiggle) continue;
        const m = o.text.match(mk.re);
        if (!m) continue;
        const k2 = smooth(mk.t0 ?? 0, (mk.t0 ?? 0) + 0.35, t) * until(mk.t1, 0.2, t);
        squiggle(R, box.x + gutter + 12 + m.index * adv, box.x + gutter + 12 + (m.index + m[0].length) * adv, rowY + 6, mk.c, a * k2, t);
      }
      if (partial) ctx.restore();
    }
    y += lh * hk;
  }
  ctx.restore();
}

export function squiggle(R, x0, x1, y, color, alpha, t = 0) {
  if (alpha <= 0) return;
  const pts = [];
  for (let x = x0; x <= x1; x += 2) pts.push([x, y + Math.sin((x - x0) * 0.55 + t * 6) * 2.2]);
  const ctx = R.ctx;
  ctx.save();
  ctx.globalAlpha = alpha;
  ctx.strokeStyle = color;
  ctx.lineWidth = 1.6;
  ctx.beginPath();
  pts.forEach(([x, yy], i) => (i ? ctx.lineTo(x, yy) : ctx.moveTo(x, yy)));
  ctx.stroke();
  ctx.restore();
}

// ── camera ──────────────────────────────────────────────────────────────
// The area a framed shot fills: the whole frame inside the kicker and the
// plate. The title band is part of it, so a clip dims its title while the
// camera is pushed in (titleFade).
export const SAFE = { x: 40, y: 92, w: 1840, h: 916 };

// The camera that shows world rectangle `box` as large as it fits in
// `area`, centred there, with `pad` world px of margin around it.
export function frameBox(box, pad = 24, area = SAFE) {
  const s = Math.min(area.w / (box.w + 2 * pad), area.h / (box.h + 2 * pad));
  const ax = area.x + area.w / 2, ay = area.y + area.h / 2;
  return { s, x: box.x + box.w / 2 - (ax - DW / 2) / s, y: box.y + box.h / 2 - (ay - DH / 2) / s };
}

// The resting camera: the world as laid out, unscaled.
export const WIDE = { x: DW / 2, y: DH / 2, s: 1 };

// shots: [{ at, cam, move = 0.9 }] in time order. The camera holds each
// shot and eases into the next one over the `move` seconds before its
// `at`, starting from wherever it was when that move began. Zoom
// interpolates in log space, so a push-in reads at a constant rate
// whatever its depth.
export function cameraPath(t, shots, ease = E.glide) {
  let k = 0;
  while (k + 1 < shots.length && t >= shots[k + 1].at - (shots[k + 1].move ?? 0.9)) k++;
  const b = shots[k];
  const t0 = b.at - (b.move ?? 0.9);
  if (k === 0 || t >= b.at) return b.cam;
  const a = cameraPath(t0, shots.slice(0, k), ease);
  const p = ease(seg(t, t0, b.at));
  return { s: Math.exp(lerp(Math.log(a.s), Math.log(b.cam.s), p)), x: lerp(a.x, b.cam.x, p), y: lerp(a.y, b.cam.y, p) };
}

// The title stays readable at rest and gives way while the camera is in.
export const titleFade = cam => 1 - smooth(1.03, 1.18, cam.s);

// ── frame ───────────────────────────────────────────────────────────────
// The clip's instrument frame: the mark and kicker top-left, provenance
// bottom-left, nika.sh bottom-right. `alpha` fades it with the loop;
// `scrim` (0..1) darkens the bands behind it while the camera is in.
export function frame(R, t, { kicker, plate, alpha = 1, scrim = 0 }) {
  if (alpha <= 0) return;
  // while the camera is in, the world passes under dark bands that keep
  // the kicker and the plate readable
  if (scrim > 0 && !R.glowPass) {
    const ctx = R.ctx;
    for (const [y0, y1, top] of [[0, 104, true], [996, DH, false]]) {
      const g = ctx.createLinearGradient(0, y0, 0, y1);
      g.addColorStop(top ? 0 : 1, rgba(C.bg0, 0.94 * scrim * alpha));
      g.addColorStop(top ? 1 : 0, rgba(C.bg0, 0));
      ctx.fillStyle = g;
      ctx.fillRect(0, y0, DW, y1 - y0);
    }
  }
  text(R, 'NIKA', 64, 66, { f: 'MGW 500', size: 13, tracking: 6, color: C.ice, alpha, glow: 0.3 });
  if (kicker) text(R, kicker.toUpperCase(), 150, 66, { f: 'MGW 500', size: 11, tracking: 3, color: C.dim, alpha });
  if (plate) text(R, plate.toUpperCase(), 64, 1038, { f: 'MGW 500', size: 10.5, tracking: 3, color: C.dim, alpha: alpha * 0.9 });
  text(R, 'nika.sh', 1856, 1038, { f: 'MM 500', size: 14, tracking: 1, color: C.ice, alpha: alpha * 0.85, align: 'right' });
  const ctx = R.ctx;
  for (const [x, y, dx, dy] of [[36, 36, 1, 1], [1884, 36, -1, 1], [36, 1044, 1, -1], [1884, 1044, -1, -1]]) {
    line(R, x, y, x + 18 * dx, y, { color: C.faint, w: 1, alpha });
    line(R, x, y, x, y + 18 * dy, { color: C.faint, w: 1, alpha });
  }
  ctx.globalAlpha = 1;
}

// The headline: the film's title system (one line, an accent tail, a subline).
export function headline(R, t, t0, t1, main, sub, opts = {}) {
  beatTitle(R, t, t0, t1, main, sub, { x: 64, y: 172, size: 58, ...opts });
}

// Loop envelope: fade in from black and back out, so the GIF loops cleanly.
// Short: every faded frame repaints the whole GIF frame.
export const loopFade = (t, dur, a = 0.2, b = 0.3) => Math.min(smooth(0, a, t), 1 - smooth(dur - b, dur, t));

export { C, E, clamp, lerp, seg, smooth };
