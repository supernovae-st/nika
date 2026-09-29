// first-session · "Say it. Nika writes the file."
// The README's first workflow in the native Session, captured offline by
// scripts/media/capture/first-session.sh (media/raw/first-session-*): a
// sentence at the prompt, the proposal Nika writes and checks, `yes`, `run
// it`, `/proof`. No AI model is chosen or called: the deterministic compiler
// settles the request. The Session's plain view is restyled into four
// moments (say · check · run · prove); every line of Session text is cut
// from the transcript, never typed here, and an elision is marked with "…".
// The folder strips and the rows moving from orders.csv into paid.csv
// illustrate what the run did, drawn from the captured files. The clip
// refuses to render if the captures stop telling this story.
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { C, E, clamp, lerp, seg, smooth, REPO, repoLines, readRepo, frame, headline, loopFade, cameraPath, frameBox, titleFade } from './kit.mjs';
import { text, rrect, rect, line, circle, poly, check, measure, bezierPts, light, streak } from '../src/engine/render.mjs';

// ── the captures ────────────────────────────────────────────────────────
const RAW = 'media/raw/first-session';
const LINES = repoLines(`${RAW}-transcript.txt`);
const FACTS = JSON.parse(readRepo(`${RAW}-facts.json`));
const bytes = rel => fs.readFileSync(path.join(REPO, rel));
const sha256 = buf => crypto.createHash('sha256').update(buf).digest('hex');
const csv = rel => readRepo(rel).replace(/\n$/, '').split('\n').map(l => l.split(','));

const story = what => { throw new Error(`the first-session capture no longer tells its story: ${what}`); };
// the index of the first line at or after `from` that matches re
function at(re, what, from = 0) {
  const i = LINES.findIndex((l, k) => k >= from && re.test(l));
  return i < 0 ? story(what) : i;
}
const lineOf = (re, what, from) => LINES[at(re, what, from)];
const esc = s => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

// one Session, in order: the request, the proposal, yes, run it, /proof
const PROJECT = lineOf(/^Nika · \S+$/, 'the Session header').replace('Nika · ', '');
const QUESTION = lineOf(/^What do you want to automate\?$/, 'the opening question');
const ASK = at(/^nika › /, 'the typed request');
// a typed line is its prompt and what the person typed after it
const typed = line => line.match(/^(.+ ›) (.+)$/u).slice(1);
const [NIKA_PROMPT, REQUEST] = typed(LINES[ASK]);
const UNDERSTOOD = LINES[ASK + 1];
if (!/^✓ understood \d+ requirements$/u.test(UNDERSTOOD)) story('Nika no longer answers the request with what it understood');
const PROPOSES = lineOf(/^Nika proposes `[^`]+`:$/, 'the proposal', ASK);
const FILE = PROPOSES.match(/`([^`]+)`/)[1];
const DOES = at(/^Does$/, 'the Does list', ASK);
const STEPS = [];
for (let k = DOES + 1; /^ {2}\d+\. /.test(LINES[k]); k++) {
  const m = LINES[k].match(/^ {2}(\d+)\. (\w+) · (.+)$/);
  if (!m) story(`a step line: ${LINES[k]}`);
  STEPS.push({ n: m[1], name: m[2], what: m[3] });
}
const TOUCH = at(/^Can touch$/, 'the Can touch section', DOES);
const NO_EFFECTS = lineOf(/^ {2}external effects · none$/, 'no external effects', TOUCH).trim();
const WHEN = at(/^ {2}when it runs:$/, 'the when it runs list', TOUCH);
const WHEN_LINE = LINES[WHEN].trim();
const READS = lineOf(/^ {4}· reads \.\/\S+$/, 'the file it reads', WHEN).trim();
const WRITES = lineOf(/^ {4}· writes \.\/\S+$/, 'the file it writes', WHEN).trim();
const SOURCE = READS.replace('· reads ', '');
const CHECKED = lineOf(new RegExp(`^ {2}check of these bytes · \`${esc(FILE)}\` · clean ✔$`), 'a clean check of the proposal', ASK).trim();
const NOTHING = lineOf(/^Nothing has run yet · /, 'nothing has run yet', ASK);
const YES = at(/^apply\? › yes$/, 'the typed yes', ASK);
const [APPLY_PROMPT, YES_TYPED] = typed(LINES[YES]);
const SAVED = lineOf(/^Saved · checked · not active · nothing has run$/, 'saved, checked, nothing run', YES);
const RUN = at(/^nika › run it$/, 'the typed run it', YES);
const RUN_TYPED = typed(LINES[RUN])[1];
const RUNNING = lineOf(new RegExp(`^running \`${esc(FILE)}\` once · ceiling \\$[\\d.]+$`), 'the announced ceiling', RUN);
const CEILING = RUNNING.match(/ceiling (\$[\d.]+)$/)[1];
const TASKS = [];
for (let k = RUN; k < LINES.length && !/^Done · /.test(LINES[k]); k++) {
  const m = LINES[k].match(/^ {2}(ok) (\w+) +invoke · (nika:\w+) +(\d+ms)$/);
  if (m) TASKS.push({ ok: m[1], name: m[2], tool: m[3], ms: m[4] });
}
const DONE = lineOf(/^Done · `[^`]+` · \d+ ms · \d+ tasks ran$/, 'the Done line', RUN);
const RAN = +DONE.match(/(\d+) tasks ran$/)[1];
const PRODUCED = lineOf(/^ {2}produced · \.\/\S+ \(\d+ B\)$/, 'the produced file', RUN).trim();
const [, OUT, OUT_BYTES] = PRODUCED.match(/produced · (\S+) \((\d+) B\)$/);
at(/^ {2}cost · no model usage recorded$/, 'a run with no model usage', RUN);
at(/^ {2}run observed · exit 0 · succeeded/, 'a run that succeeded', RUN);
const PROOF = at(/^nika › \/proof$/, 'the typed /proof', RUN);
const PROOF_TYPED = typed(LINES[PROOF])[1];
const CHAIN = lineOf(/^ {2}chain · OK — \d+ events · chain intact · /, 'an intact chain', PROOF).trim();
const EVENTS = +CHAIN.match(/OK — (\d+) events/)[1];
const BOUNDARY = lineOf(/^ {2}boundary · .*\d+ permit check\(s\) · \d+ allowed · 0 denied$/, 'no denied permit check', PROOF).trim();
const [, CHECKS, ALLOWED] = BOUNDARY.match(/(\d+) permit check\(s\) · (\d+) allowed · 0 denied$/);
const WRITTEN = lineOf(new RegExp(`^ {2}written · ${esc(OUT)} · ${OUT_BYTES} B · sha256 [0-9a-f]+…[0-9a-f]+ · `), 'the written file', PROOF).trim();
const WORKFLOW = lineOf(/^ {2}workflow · \S+ · bytes sha256 [0-9a-f]+…[0-9a-f]+ · /, 'the workflow bytes', PROOF).trim();
const NOT_PROVEN = lineOf(/^ {2}does not prove · that the content is right \(read it\)/, 'what the proof does not prove', PROOF).trim();
const VERSION = lineOf(/^ {2}engine · \S+ · /, 'the engine version', PROOF).match(/engine · (\S+)/)[1];
if (LINES.some(l => /Choose which AI/.test(l))) story('the Session asked which AI to use');

// the captured files agree with what the Session printed about them
const ORDERS = csv(`${RAW}-orders.csv`);
const PAID = csv(`${RAW}-paid.csv`);
const STATUS = ORDERS[0].indexOf('status');
const KEPT = ORDERS.filter((r, i) => i === 0 || r[STATUS] === 'paid');
const hashOk = (buf, lineText) => {
  const [, head, tail] = lineText.match(/sha256 ([0-9a-f]+)…([0-9a-f]+)/);
  const h = sha256(buf);
  return h.startsWith(head) && h.endsWith(tail);
};
const FILTER = readRepo(`${RAW}-workflow.nika`).split('\n').map(l => l.match(/expression: '(\[\.records\[\] \| select\(\.status == "paid"\)\])'$/)).find(Boolean)?.[1];
if (!REQUEST.includes(SOURCE) || !REQUEST.includes(WRITES.replace('· writes ', '')) || !REQUEST.includes(OUT)) story('the request names other files');
if (STEPS.length !== RAN || TASKS.length !== RAN || STEPS.some((s, i) => TASKS[i].name !== s.name)) story('the steps proposed are not the tasks that ran');
if (FACTS.tasks !== RAN || FACTS.run !== 'succeeded' || FACTS.model_usage_recorded !== false) story('the facts disagree with the transcript');
if (+OUT_BYTES !== bytes(`${RAW}-paid.csv`).length || FACTS.written.bytes !== +OUT_BYTES) story('paid.csv is not the size the Session printed');
if (JSON.stringify(PAID) !== JSON.stringify(KEPT) || PAID.length !== 3) story('paid.csv is not the header and the two paid rows');
if (!hashOk(bytes(`${RAW}-paid.csv`), WRITTEN)) story('the proof hash is not the captured paid.csv');
if (!hashOk(bytes(`${RAW}-workflow.nika`), WORKFLOW)) story('the proof hash is not the captured workflow');
if (+ALLOWED !== +CHECKS || !FILTER) story('the boundary or the filter changed');
if (!FACTS.nika.includes(VERSION)) story('the facts and the transcript name different engines');

// A Session line shows some of its " · " segments; the rest is elided
// with "…", never reworded. keep: the indices of the segments shown.
const SEP = ' · ';
function cut(str, keep) {
  const parts = str.split(SEP);
  const out = [];
  let prev = -1;
  for (const i of keep) {
    if (i < 0 || i >= parts.length) story(`a segment of: ${str}`);
    if (i > prev + 1) out.push('…');
    out.push(parts[i]);
    prev = i;
  }
  if (prev < parts.length - 1) out.push('…');
  return out.join(SEP);
}
const lastSegs = (str, n) => {
  const k = str.split(SEP).length;
  return Array.from({ length: n }, (_, i) => k - n + i);
};

// ── layout: four moments in a world the last shot shows whole ───────────
const CARD = [
  { x: 0, y: 0, w: 960, h: 430 },
  { x: 1020, y: 0, w: 960, h: 430 },
  { x: 0, y: 502, w: 960, h: 450 },
  { x: 1020, y: 502, w: 960, h: 450 },
];
const MOMENT = [
  { n: '1', label: 'SAY IT', note: 'in plain words' },
  { n: '2', label: 'CHECK IT', note: 'nika writes a checked file' },
  { n: '3', label: 'RUN IT', note: 'once · when you say' },
  { n: '4', label: 'PROVE IT', note: 'what the run recorded' },
];
// the whole world, under the headline and above the plate
const POSTER = (() => {
  const w = CARD[1].x + CARD[1].w, h = CARD[3].y + CARD[3].h;
  const s = Math.min(1792 / w, 774 / h);
  return { x: w / 2, y: h / 2 - (236 + (h * s) / 2 - 540) / s, s };
})();

const SANS = size => ({ f: 'Geist 500', size });
const MONO = size => ({ f: 'MM 400', size });
const LABEL = { f: 'Geist 500', size: 13 };
const ASK_ST = { f: 'Geist 500', size: 26, tracking: -0.3 };
const BODY = 16;

// ── timeline (seconds) ──────────────────────────────────────────────────
const T = {
  card: 0.12, question: 0.3, type0: 0.62, type1: 2.12, send: 2.16, understood: 2.6,
  go2: 4.3,
  proposes: 4.5, steps: 4.75, stepEvery: 0.09,
  touch: 5.6, checked: 6.3, nothing: 6.9,
  yes: 7.7, saved: 8.5,
  go3: 10.65,
  run: 11.05, running: 11.4, tick0: 11.85, tickEvery: 0.16,
  done: 13.15, produced: 13.45, data: 13.65, drop: 14.05, slide: 14.55,
  go4: 16.25,
  proof: 16.65, chain: 17.05, boundary: 17.65, written: 18.25, limit: 18.85,
};
T.wide = T.limit + 3.25;
export const meta = { duration: +(T.wide + 2).toFixed(2), poster: +(T.wide + 1.45).toFixed(2) };
const tickAt = i => T.tick0 + i * T.tickEvery;

// Moves are short (every frame of a move repaints the whole GIF frame) and
// the holds long; every card is read at the same zoom.
const SHOTS = [
  { at: 0, cam: frameBox(CARD[0], 20) },
  { at: T.go2 + 0.6, cam: frameBox(CARD[1], 20), move: 0.6 },
  { at: T.go3 + 0.7, cam: frameBox(CARD[2], 20), move: 0.7 },
  { at: T.go4 + 0.6, cam: frameBox(CARD[3], 20), move: 0.6 },
  { at: T.wide, cam: POSTER, move: 0.85 },
];
export const camera = t => cameraPath(t, SHOTS);

// The grid behind the cards drifts at a fraction of the camera's travel: a
// depth cue that moves only while the camera does (a hold stays still).
export function env(t) {
  const cam = camera(t);
  return { bgGlow: 1, gridAlpha: 0.22, gridX: -(cam.x - POSTER.x) * 0.1 * cam.s, gridY: -(cam.y - POSTER.y) * 0.1 * cam.s, bgY: 560 };
}

export function chrome(R, t) {
  const a = loopFade(t, meta.duration);
  const cam = camera(t);
  frame(R, t, { kicker: 'the session · from a sentence to a checked file', plate: `the session's plain view, restyled · the folders and the data flow are illustrations · captured from nika ${VERSION} · no ai model involved`, alpha: a, scrim: smooth(1, 1.12, cam.s) });
  // the title waits for the last shot to settle: no card passes under it
  const ha = a * titleFade(cam) * (1 - smooth(POSTER.s * 1.03, POSTER.s * 1.12, cam.s));
  headline({ ...R, fade: ha }, t, 0.2, meta.duration + 1, 'Say it. Nika writes the file.', null, { accent: 'Nika writes the file.', accentColor: C.teal });
  // the promise, one size up from the kit's subline: the poster is read small
  text(R, 'YOU DECIDE WHEN IT RUNS · YOU SEE WHAT IT DID', 66, 212, { f: 'MGW 500', size: 15, tracking: 3.2, color: C.teal, alpha: ha * 0.9 });
}

// ── type ────────────────────────────────────────────────────────────────
// Verbatim text with the Session's code marks set as code: a `span` (the
// backticks are the Session's own marks) and a ./path in mono, cyan; a
// status mark (✓ ✔) drawn as a vector check.
function tokens(str) {
  const out = [];
  const re = /`([^`]+)`|(\.\/[\w./-]+)|(✓|✔)/gu;
  let last = 0, m;
  while ((m = re.exec(str))) {
    if (m.index > last) out.push({ s: str.slice(last, m.index) });
    out.push(m[3] ? { s: m[3], mark: true } : { s: m[1] ?? m[2], code: true });
    last = m.index + m[0].length;
  }
  if (last < str.length) out.push({ s: str.slice(last) });
  return out;
}
const codeSt = size => MONO(+(size * 0.84).toFixed(2));
const tokW = (tk, size) => (tk.mark ? size * 0.95 : measure(tk.s, tk.code ? codeSt(size) : SANS(size)));
const richWidth = (str, size = BODY) => tokens(str).reduce((w, tk) => w + tokW(tk, size), 0);
// the x where the plain part `part` of a rich line starts
const richAt = (str, part, size = BODY) => {
  let x = 0;
  for (const tk of tokens(str)) {
    const i = tk.mark ? -1 : tk.s.indexOf(part);
    if (i >= 0) return x + measure(tk.s.slice(0, i), tk.code ? codeSt(size) : SANS(size));
    x += tokW(tk, size);
  }
  return story(`"${part}" in: ${str}`);
};

// hl: [{ s, c, glow }] recolours a plain part that is exactly s
function rich(R, str, x, y, { size = BODY, color = C.ink, code = C.cyan, alpha = 1, glow = 0, hl = [], markColor = C.teal, markK = 1 } = {}) {
  if (alpha <= 0) return 0;
  let cx = x;
  for (const tk of tokens(str)) {
    const w = tokW(tk, size);
    if (tk.mark) check(R, cx + w * 0.5, y - size * 0.34, size * 0.66, markK, { color: markColor, w: Math.max(1.6, size * 0.12), alpha, glow: 0.8 });
    else {
      // a highlighted part is drawn apart from the words around it
      let parts = [{ s: tk.s }];
      for (const h of hl) {
        parts = parts.flatMap(p => {
          if (p.h || !p.s.includes(h.s)) return [p];
          const i = p.s.indexOf(h.s);
          return [{ s: p.s.slice(0, i) }, { s: h.s, h }, { s: p.s.slice(i + h.s.length) }].filter(q => q.s);
        });
      }
      const st = tk.code ? codeSt(size) : SANS(size);
      let px = cx;
      for (const p of parts) {
        text(R, p.s, px, y, { ...st, color: p.h ? p.h.c : tk.code ? code : color, alpha, glow: p.h ? p.h.glow ?? 0.4 : glow });
        px += measure(p.s, st);
      }
    }
    cx += w;
  }
  return cx - x;
}

// Break a plain line at spaces into rows no wider than maxW.
function wrapRows(str, st, maxW) {
  const rows = [];
  let row = '';
  for (const w of str.split(' ')) {
    const next = row ? `${row} ${w}` : w;
    if (row && measure(next, st) > maxW) { rows.push(row); row = w; } else row = next;
  }
  if (row) rows.push(row);
  return rows;
}

// A line the person types at a Session prompt: the prompt, then the typed
// words in the human's warm white, a caret while it types.
function prompt(R, t, x, y, promptStr, typedStr, t0, dur, a, { size = 19 } = {}) {
  const k = smooth(t0 - 0.3, t0 - 0.05, t) * a;
  if (k <= 0) return;
  text(R, 'YOU', x, y - 1, { f: 'MGW 500', size: 11, tracking: 3, color: C.human, alpha: k * 0.85 });
  const pst = MONO(+(size * 0.84).toFixed(2));
  const px = x + 48;
  text(R, promptStr, px, y, { ...pst, color: C.dim, alpha: k });
  const tx = px + measure(`${promptStr} `, pst) + 2;
  const n = Math.floor(typedStr.length * clamp((t - t0) / dur));
  const shown = typedStr.slice(0, n);
  if (shown) text(R, shown, tx, y, { ...SANS(size), color: C.human, alpha: k, glow: 0.15 });
  if (t < t0 + dur + 0.4) {
    const blink = t >= t0 && t < t0 + dur ? 1 : Math.floor(t * 2.4) % 2 === 0 ? 1 : 0.25;
    rect(R, tx + measure(shown, SANS(size)) + 3, y - size * 0.78, 2.4, size * 0.95, { fill: C.ice, alpha: k * blink, glow: 0.9 });
  }
}

// ── the card of one moment ──────────────────────────────────────────────
const badge = i => ({ x: CARD[i].x + 40, y: CARD[i].y + 29 });
function card(R, t, i, a, focus, done) {
  const b = CARD[i], m = MOMENT[i];
  const k = E.snap(seg(t, T.card, T.card + 0.5));
  if (k <= 0) return;
  rrect(R, b.x, b.y, b.w, b.h, 18, { color: focus > 0.9 ? C.dim : C.faint, w: 1.2, alpha: a * k * (0.55 + 0.45 * focus), fill: '#050c19', fillAlpha: 0.9 });
  // the card being read carries a lit edge
  const edge = smooth(0.85, 1, focus);
  if (edge > 0) line(R, b.x + 26, b.y, b.x + b.w - 26, b.y, { color: C.ice, w: 1.4, alpha: a * k * edge * 0.5, glow: 0.5 });
  const ca = a * k * focus;
  const cy = b.y + 37, bd = badge(i);
  circle(R, bd.x, bd.y, 16, { color: done > 0 ? C.teal : C.dim, w: 1.4, alpha: ca, fill: C.teal, fillAlpha: 0.1 * done, glow: 0.5 * done });
  text(R, m.n, bd.x, bd.y + 6, { f: 'Geist 600', size: 17, color: done > 0 ? C.teal : C.mist, alpha: ca, align: 'center' });
  const lst = { f: 'MGW 500', size: 25, tracking: 4 };
  text(R, m.label, b.x + 72, cy + 2, { ...lst, color: C.ink, alpha: ca });
  check(R, b.x + 72 + measure(m.label, lst) + 26, cy - 8, 18, E.snap(clamp(done)), { color: C.teal, w: 2.5, alpha: ca, glow: 0.8 });
  text(R, m.note, b.x + b.w - 32, cy - 1, { ...SANS(17), color: C.mist, alpha: ca * 0.9, align: 'right' });
  line(R, b.x + 1, b.y + 60, b.x + b.w - 1, b.y + 60, { color: C.line, w: 1, alpha: ca });
}

// The project folder as each moment leaves it: a file appears only when
// the Session writes it (`yes`, then the run). An illustration.
const FILES = [
  { name: SOURCE, at: -1 },
  { name: FILE, at: T.saved },
  { name: OUT, at: T.produced + 0.15 },
];
const CHIP = MONO(13);
const footY = i => CARD[i].y + CARD[i].h - 20;
// where each file's chip sits in card i's footer, after the folder name
const chips = i => {
  let x = CARD[i].x + 30 + 24 + measure(`${PROJECT}/`, CHIP) + 20;
  return FILES.map(f => {
    const c = { x, w: measure(f.name, CHIP) + 42 };
    x += c.w + 12;
    return c;
  });
};
function folder(R, t, i, a) {
  const b = CARD[i];
  const y = footY(i);
  const k = smooth(T.card + 0.3, T.card + 0.7, t) * a;
  if (k <= 0) return;
  line(R, b.x + 1, y - 29, b.x + b.w - 1, y - 29, { color: C.line, w: 1, alpha: k });
  folderIcon(R, b.x + 30, y - 5, C.dim, k);
  text(R, `${PROJECT}/`, b.x + 54, y, { ...CHIP, color: C.mist, alpha: k });
  chips(i).forEach((c, j) => {
    const f = FILES[j];
    if (j > i) return; // not written yet at this moment
    const own = j === i && f.at > 0; // written during this card's moment
    const show = own ? smooth(f.at, f.at + 0.25, t) : 1;
    if (show <= 0) return;
    const fresh = own ? 1 - smooth(f.at + 0.8, f.at + 2.4, t) : 0;
    const lift = own ? 1 - E.outBack(seg(t, f.at, f.at + 0.45)) : 0;
    const yy = y - 16 * lift;
    const hot = fresh > 0.05;
    rrect(R, c.x, yy - 18, c.w, 27, 7, { color: hot ? C.teal : C.faint, w: 1.1, alpha: k * show, fill: C.teal, fillAlpha: 0.04 + 0.12 * fresh, glow: 0.5 * fresh });
    fileIcon(R, c.x + 11, yy - 12, hot ? C.teal : C.dim, k * show);
    text(R, f.name, c.x + 30, yy, { ...CHIP, color: hot ? C.teal : C.mist, alpha: k * show });
  });
}
function fileIcon(R, x, y, color, alpha) {
  poly(R, [[x, y], [x + 7, y], [x + 11, y + 4], [x + 11, y + 15], [x, y + 15], [x, y]], { color, w: 1.1, alpha });
  poly(R, [[x + 7, y], [x + 7, y + 4], [x + 11, y + 4]], { color, w: 1.1, alpha });
}
function folderIcon(R, x, y, color, alpha) {
  poly(R, [[x, y - 8], [x + 6, y - 8], [x + 8, y - 5], [x + 16, y - 5], [x + 16, y + 5], [x, y + 5], [x, y - 8]], { color, w: 1.1, alpha });
}

// ── 1 · SAY IT ──────────────────────────────────────────────────────────
const S1 = CARD[0];
const P1 = { f: 'MM 400', size: 17 };
const L1 = S1.x + 44;
const ASK_X = L1 + measure(`${NIKA_PROMPT} `, P1) + 8;
const ASK_ROWS = wrapRows(REQUEST, ASK_ST, 520);
const ASK_Y = S1.y + 198, ASK_LH = 42;
const ROW_START = (() => {
  let n = 0;
  return ASK_ROWS.map(r => { const s = n; n += r.length + 1; return s; });
})();
const UND_Y = ASK_Y + (ASK_ROWS.length - 1) * ASK_LH + 62;

function say(R, t, a) {
  const b = S1;
  text(R, QUESTION, L1, b.y + 118, { ...SANS(24), color: C.mist, alpha: smooth(T.question, T.question + 0.4, t) * a });
  const pk = smooth(T.question + 0.15, T.question + 0.45, t) * a;
  text(R, 'YOU', L1, ASK_Y - 40, { f: 'MGW 500', size: 11, tracking: 3, color: C.human, alpha: pk * 0.85 });
  text(R, NIKA_PROMPT, L1, ASK_Y, { ...P1, color: C.dim, alpha: pk });
  // the sentence as a person types it: a steady pace with a small,
  // deterministic unevenness
  const u = clamp((t - T.type0) / (T.type1 - T.type0));
  const n = Math.floor(REQUEST.length * clamp(u + 0.02 * Math.sin(u * 31) * u * (1 - u) * 4));
  let caret = null;
  ASK_ROWS.forEach((row, r) => {
    const c = clamp(n - ROW_START[r], 0, row.length);
    if (c <= 0) return;
    const y = ASK_Y + r * ASK_LH;
    const shown = row.slice(0, c);
    const typing = c < row.length;
    // the newest characters rise into place
    const lead = typing ? Math.min(2, shown.length) : 0;
    const stable = shown.slice(0, shown.length - lead);
    if (stable) text(R, stable, ASK_X, y, { ...ASK_ST, color: C.human, alpha: a, glow: 0.1 });
    let x = ASK_X + (stable ? measure(stable, ASK_ST) + ASK_ST.tracking : 0);
    for (let j = 0; j < lead; j++) {
      const ch = shown[shown.length - lead + j];
      const age = (j + 1) / lead;
      text(R, ch, x, y + 6 * age, { ...ASK_ST, color: C.ice, alpha: a * (1 - 0.35 * age), glow: 0.7 });
      x += measure(ch, ASK_ST) + ASK_ST.tracking;
    }
    if (typing || (r === ASK_ROWS.length - 1 && c === row.length)) caret = { x: ASK_X + measure(shown, ASK_ST) + 4, y };
  });
  if (caret && t < T.send + 0.05) {
    const blink = t < T.type1 ? 1 : Math.floor(t * 2.4) % 2 === 0 ? 1 : 0.3;
    rect(R, caret.x, caret.y - 24, 2.6, 30, { fill: C.ice, alpha: a * blink, glow: 1 });
    if (t < T.type1) light(R, caret.x, caret.y - 9, 70, C.ice, 0.14 * a, 1);
  }
  // the caret lights: a line of light through it, once
  const ig = seg(t, T.question + 0.15, T.question + 0.75);
  if (ig > 0 && ig < 1) streak(R, ASK_X, ASK_Y - 9, lerp(80, 520, E.outCubic(ig)), 0.7 * a * (1 - ig) ** 2, C.ice, 1);
  if (t < T.type0 + 0.05) {
    const blink = Math.floor(t * 2.4) % 2 === 0 ? 1 : 0.3;
    rect(R, ASK_X, ASK_Y - 24, 2.6, 30, { fill: C.ice, alpha: pk * blink, glow: 1 });
  }
  // Enter: a line of light runs under the sentence as it is sent
  const sk = seg(t, T.send, T.send + 0.55);
  if (sk > 0 && sk < 1) {
    const yl = ASK_Y + (ASK_ROWS.length - 1) * ASK_LH + 17;
    const x1 = ASK_X + Math.max(...ASK_ROWS.map(r => measure(r, ASK_ST)));
    const hx = lerp(ASK_X, x1, E.outCubic(sk));
    streak(R, hx, yl, 150, 0.75 * a * (1 - sk), C.ice, 1);
    line(R, ASK_X, yl, hx, yl, { color: C.ice, w: 1.2, alpha: a * 0.55 * (1 - smooth(0.55, 1, sk)), glow: 0.6 });
  }
  // Nika's first answer: what it understood
  rich(R, UNDERSTOOD, L1, UND_Y, { size: 19, alpha: smooth(T.understood, T.understood + 0.3, t) * a, markK: E.snap(seg(t, T.understood, T.understood + 0.4)) });
}

// ── 2 · CHECK IT ────────────────────────────────────────────────────────
const S2 = CARD[1];
const L2 = S2.x + 44, R2 = S2.x + 336;
const STEP_Y = S2.y + 180, STEP_LH = 24;
const stepT = i => T.steps + i * T.stepEvery;
const R2Y = S2.y + 146;

// steps as a rail: one node per step, the rail drawn as they arrive
function rail(R, t, x, y0, lh, n, t0, every, a, lit = () => 0) {
  const draw = seg(t, t0, t0 + every * (n - 1) + 0.2);
  if (draw <= 0) return;
  const y1 = y0 + (n - 1) * lh;
  line(R, x, y0, x, lerp(y0, y1, E.inOutCubic(draw)), { color: C.faint, w: 1.4, alpha: a });
  for (let i = 0; i < n; i++) {
    const k = smooth(t0 + i * every, t0 + i * every + 0.2, t);
    if (k <= 0) continue;
    const l = lit(i);
    circle(R, x, y0 + i * lh, 4.4 + 1.2 * l, { color: l > 0 ? C.teal : C.ice, w: 1.3, alpha: a * k, fill: l > 0 ? C.teal : '#050c19', fillAlpha: l > 0 ? 0.9 : 1, glow: 0.7 * l });
  }
}

function checkIt(R, t, a) {
  const b = S2;
  rich(R, PROPOSES, L2, b.y + 108, { size: 19, alpha: smooth(T.proposes, T.proposes + 0.3, t) * a });
  // Does: the steps, in the order they will run
  text(R, LINES[DOES], L2, STEP_Y - 30, { ...LABEL, color: C.dim, alpha: smooth(T.steps - 0.2, T.steps, t) * a });
  rail(R, t, L2 + 5, STEP_Y - 5, STEP_LH, STEPS.length, T.steps, T.stepEvery, a);
  STEPS.forEach((s, i) => {
    const k = smooth(stepT(i), stepT(i) + 0.2, t);
    if (k <= 0) return;
    text(R, s.what, L2 + 24, STEP_Y + i * STEP_LH + 3 * (1 - k), { ...SANS(16), color: C.ink, alpha: k * a });
  });
  // what it can touch, and what it touches when it runs
  const tk = smooth(T.touch, T.touch + 0.3, t) * a;
  text(R, LINES[TOUCH], R2, R2Y, { ...LABEL, color: C.dim, alpha: tk });
  rich(R, NO_EFFECTS, R2, R2Y + 23, { alpha: tk, hl: [{ s: 'none', c: C.teal, glow: 0.5 }] });
  const wk = smooth(T.touch + 0.25, T.touch + 0.55, t) * a;
  text(R, WHEN_LINE, R2, R2Y + 51, { ...LABEL, color: C.dim, alpha: wk });
  rich(R, READS, R2, R2Y + 73, { color: C.mist, alpha: wk });
  rich(R, WRITES, R2, R2Y + 95, { color: C.mist, alpha: wk });
  // the check of these exact bytes
  const ck = smooth(T.checked, T.checked + 0.3, t) * a;
  rich(R, CHECKED, R2, R2Y + 128, { alpha: ck, hl: [{ s: 'clean', c: C.teal, glow: 0.6 }], markK: E.snap(seg(t, T.checked + 0.15, T.checked + 0.5)) });
  const fl = ck > 0 ? 1 - smooth(T.checked + 0.2, T.checked + 1.1, t) : 0;
  if (fl > 0) light(R, R2 + richWidth(CHECKED) - 8, R2Y + 122, 56, C.teal, 0.35 * fl * a, 1);
  rich(R, cut(NOTHING, [0]), R2, R2Y + 154, { color: C.human, alpha: smooth(T.nothing, T.nothing + 0.3, t) * a });
  // yes: the person saves these exact bytes; nothing runs
  prompt(R, t, R2, R2Y + 184, APPLY_PROMPT, YES_TYPED, T.yes, 0.2, a, { size: 18 });
  rich(R, SAVED, R2, R2Y + 208, { alpha: smooth(T.saved, T.saved + 0.3, t) * a, hl: [{ s: 'Saved', c: C.teal, glow: 0.4 }] });
  // the reviewed proposal folds into the one file `yes` writes
  const mp = seg(t, T.yes + 0.25, T.saved + 0.02);
  if (mp > 0 && mp < 1) {
    const q = E.inOutCubic(mp);
    const c = chips(1)[1];
    const from = { x: L2 - 18, y: b.y + 80, w: b.w - 50, h: 250 };
    const to = { x: c.x, y: footY(1) - 18, w: c.w, h: 27 };
    const box = { x: lerp(from.x, to.x, q), y: lerp(from.y, to.y, q), w: lerp(from.w, to.w, q), h: lerp(from.h, to.h, q) };
    rrect(R, box.x, box.y, box.w, box.h, lerp(14, 7, q), { color: C.teal, w: 1.4, alpha: a * (0.3 + 0.6 * q) * (1 - smooth(0.85, 1, mp)), fill: C.teal, fillAlpha: 0.05 * q, glow: 0.6 });
  }
}

// ── 3 · RUN IT ──────────────────────────────────────────────────────────
const S3 = CARD[2];
const L3 = S3.x + 44, R3 = S3.x + 470;
const TASK_Y = S3.y + 190, TASK_LH = 22;

function runIt(R, t, a) {
  const b = S3;
  prompt(R, t, L3, b.y + 106, NIKA_PROMPT, RUN_TYPED, T.run, 0.25, a);
  rich(R, RUNNING, L3, b.y + 144, { alpha: smooth(T.running, T.running + 0.3, t) * a, hl: [{ s: CEILING, c: C.gold, glow: 0.5 }] });
  // the ceiling is announced before anything runs
  const cx0 = L3 + richAt(RUNNING, CEILING), cw0 = measure(CEILING, SANS(BODY));
  const sw = E.snap(seg(t, T.running + 0.25, T.running + 0.65));
  if (sw > 0) line(R, cx0, b.y + 151, cx0 + cw0 * sw, b.y + 151, { color: C.gold, w: 1.6, alpha: a * 0.85, glow: 0.6 });
  // the tasks run in the order they were proposed
  const lit = i => smooth(tickAt(i), tickAt(i) + 0.12, t);
  rail(R, t, L3 + 5, TASK_Y - 5, TASK_LH, TASKS.length, T.running + 0.15, 0.03, a, lit);
  const pr = seg(t, T.tick0 - 0.12, tickAt(TASKS.length - 1));
  if (pr > 0 && pr < 1) circle(R, L3 + 5, TASK_Y - 5 + pr * (TASKS.length - 1) * TASK_LH, 3.4, { fill: C.cyan, alpha: a, glow: 1 });
  TASKS.forEach((task, i) => {
    const k = smooth(T.running + 0.15 + i * 0.03, T.running + 0.35 + i * 0.03, t) * a;
    if (k <= 0) return;
    const y = TASK_Y + i * TASK_LH;
    const l = lit(i);
    text(R, task.name, L3 + 24, y, { ...MONO(13.5), color: l > 0 ? C.ink : C.dim, alpha: k });
    if (l > 0) {
      text(R, task.ok, L3 + 212, y, { ...MONO(13), color: C.teal, alpha: k * l });
      text(R, task.ms, L3 + 300, y, { ...MONO(13), color: C.mist, alpha: k * l, align: 'right' });
    }
  });
  rich(R, cut(DONE, lastSegs(DONE, 1)), L3, TASK_Y + TASKS.length * TASK_LH + 14, { alpha: smooth(T.done, T.done + 0.3, t) * a, hl: [{ s: `${RAN} tasks ran`, c: C.teal, glow: 0.5 }] });
  data(R, t, a);
}

// The rows of orders.csv, and the paid ones the run wrote to paid.csv.
const TABLE = { f: 'MM 400', size: 12 };
const CELL = measure('M', TABLE);
const COLS = (() => {
  const w = ORDERS[0].map((_, c) => Math.max(...ORDERS.map(r => r[c].length)));
  let x = 0;
  return w.map(n => { const s = x; x += n + 2; return s; });
})();
const TW = (COLS[COLS.length - 1] + Math.max(...ORDERS.map(r => r[r.length - 1].length))) * CELL;
const ROW_H = 18;
const SRC = { x: R3, y: S3.y + 196 };
const DST = { x: R3, y: S3.y + 338 };
function row(R, cells, x, y, { alpha, color = C.mist, statusColor = null, strike = 0 } = {}) {
  cells.forEach((v, c) => text(R, v, x + COLS[c] * CELL, y, { ...TABLE, color: c === STATUS && statusColor ? statusColor : color, alpha }));
  if (strike > 0) line(R, x - 4, y - 4.5, x - 4 + (TW + 8) * strike, y - 4.5, { color: C.red, w: 1.3, alpha: alpha * 0.9 });
}

function data(R, t, a) {
  const ok = smooth(T.data, T.data + 0.35, t) * a;
  if (ok <= 0) return;
  rich(R, READS, SRC.x, SRC.y - 24, { size: 13, color: C.dim, alpha: ok });
  ORDERS.forEach((r, i) => {
    const head = i === 0;
    const kept = head || r[STATUS] === 'paid';
    const drop = kept ? 0 : smooth(T.drop, T.drop + 0.35, t);
    // a dropped row stays readable: struck through, its status red
    row(R, r, SRC.x, SRC.y + i * ROW_H, { alpha: ok * (head ? 0.8 : 1), color: head || drop > 0.5 ? C.dim : C.mist, statusColor: head ? null : kept ? C.teal : drop > 0 ? C.red : null, strike: drop });
  });
  // the rule the file applies, quoted from its own bytes
  text(R, FILTER, SRC.x, SRC.y + ORDERS.length * ROW_H + 7, { ...MONO(11), color: C.cyan, alpha: smooth(T.drop - 0.25, T.drop + 0.1, t) * a * 0.9 });
  // paid.csv: the header and the kept rows land in the new file
  rich(R, PRODUCED, DST.x, DST.y - 24, { size: 13, alpha: smooth(T.produced, T.produced + 0.3, t) * a, hl: [{ s: 'produced', c: C.teal, glow: 0.4 }] });
  const pk = smooth(T.slide - 0.2, T.slide + 0.15, t) * a;
  if (pk > 0) rrect(R, DST.x - 12, DST.y - 15, TW + 24, ROW_H * PAID.length + 7, 8, { color: C.teal, w: 1.1, alpha: pk * 0.6, fill: C.teal, fillAlpha: 0.05, glow: 0.3 });
  const keptIdx = ORDERS.map((r, i) => (i === 0 || r[STATUS] === 'paid' ? i : -1)).filter(i => i >= 0);
  // the kept rows are copied: lifted on plates, carried over the table
  // together, set down in paid.csv (orders.csv keeps all four rows)
  keptIdx.forEach((i, j) => {
    const s0 = T.slide + j * 0.05;
    const u = seg(t, s0, s0 + 0.65);
    if (u <= 0) return;
    const p = E.inOutCubic(u);
    const lift = smooth(0, 0.14, u) * (1 - smooth(0.86, 1, u));
    const x = DST.x, y = lerp(SRC.y + i * ROW_H, DST.y + j * ROW_H, p) - 5 * lift;
    if (lift > 0) {
      rrect(R, x - 12, y - 8, TW + 24, ROW_H + 3, 6, { fill: C.bg0, alpha: a * 0.5 * lift });
      rrect(R, x - 12, y - 14, TW + 24, ROW_H + 3, 6, { fill: '#0a1830', alpha: a * 0.97 * lift });
      rrect(R, x - 12, y - 14, TW + 24, ROW_H + 3, 6, { color: C.teal, w: 1, alpha: a * 0.55 * lift, glow: 0.3 * lift });
    }
    row(R, PAID[j], x, y, { alpha: a, color: j === 0 ? C.dim : C.ink, statusColor: j === 0 ? null : C.teal });
  });
}

// ── 4 · PROVE IT ────────────────────────────────────────────────────────
const S4 = CARD[3];
const L4 = S4.x + 44;
const WRITTEN_SHOWN = cut(WRITTEN, [0, 1, 2, 3]);
const HASH = WRITTEN.split(SEP)[3];
function proveIt(R, t, a) {
  const b = S4;
  prompt(R, t, L4, b.y + 106, NIKA_PROMPT, PROOF_TYPED, T.proof, 0.3, a);
  // the chain: every event of the journal, linked to the one before
  rich(R, cut(CHAIN, [0, 1, 2]), L4, b.y + 154, { alpha: smooth(T.chain, T.chain + 0.3, t) * a, hl: [{ s: 'chain intact', c: C.teal, glow: 0.5 }] });
  const lw = 12.5, gap = 4.8;
  for (let i = 0; i < EVENTS; i++) {
    const k = smooth(T.chain + 0.15 + i * 0.016, T.chain + 0.3 + i * 0.016, t) * a;
    if (k <= 0) continue;
    const lx = L4 + 2 + i * (lw + gap);
    rrect(R, lx, b.y + 170, lw, 15, 4, { color: C.teal, w: 1, alpha: k * 0.85, fill: C.teal, fillAlpha: 0.2, glow: 0.3 });
    if (i) line(R, lx - gap, b.y + 177.5, lx, b.y + 177.5, { color: C.teal, w: 1.2, alpha: k * 0.7 });
  }
  // the boundary: every permit check the run made, none denied
  rich(R, cut(BOUNDARY, [0, ...lastSegs(BOUNDARY, 3)]), L4, b.y + 232, { alpha: smooth(T.boundary, T.boundary + 0.3, t) * a, hl: [{ s: `${ALLOWED} allowed`, c: C.teal, glow: 0.5 }, { s: '0 denied', c: C.teal, glow: 0.3 }] });
  for (let i = 0; i < +CHECKS; i++) check(R, L4 + 9 + i * 27, b.y + 256, 15, E.snap(seg(t, T.boundary + 0.2 + i * 0.05, T.boundary + 0.45 + i * 0.05)), { color: C.teal, w: 2, alpha: a, glow: 0.5 });
  // the file it wrote, re-read and hashed: the hash points at that file
  rich(R, WRITTEN_SHOWN, L4, b.y + 308, { alpha: smooth(T.written, T.written + 0.3, t) * a, hl: [{ s: HASH, c: C.teal, glow: 0.4 }] });
  const lk = seg(t, T.written + 0.35, T.written + 0.95);
  if (lk > 0) {
    // out to the right first, clear of the lines below it, then down
    const hx = L4 + richAt(WRITTEN_SHOWN, HASH) + measure(HASH, SANS(BODY)) / 2;
    const c = chips(3)[2], cx = c.x + c.w / 2;
    const pts = bezierPts([hx, b.y + 316], [hx + 130, b.y + 330], [cx, b.y + 330], [cx, footY(3) - 20], 32);
    poly(R, pts, { color: C.teal, w: 1.2, alpha: a * 0.7, glow: 0.5, dash: [3, 4] }, E.inOutCubic(lk));
    if (lk >= 1) circle(R, cx, footY(3) - 20, 3, { fill: C.teal, alpha: a, glow: 0.8 });
  }
  // and what it does not prove: that the content is right
  const nk = smooth(T.limit, T.limit + 0.3, t) * a;
  const [lead, what] = cut(NOT_PROVEN, [0, 1]).split(SEP);
  text(R, lead, L4, b.y + 354, { ...SANS(BODY), color: C.amber, alpha: nk, glow: 0.3 });
  text(R, `${SEP}${what}${SEP}…`, L4 + measure(lead, SANS(BODY)), b.y + 354, { ...SANS(BODY), color: C.human, alpha: nk });
}

// ── the hand-offs: a pulse carries the story to the next moment ─────────
const UND_END = [L1 + richWidth(UNDERSTOOD, 19) + 10, UND_Y - 6];
const HANDOFF = [
  { t: T.go2, from: UND_END, to: [L2 - 10, S2.y + 102], c0: [UND_END[0] + 260, UND_END[1]], c1: [L2 - 200, S2.y + 102] },
  { t: T.go3, from: [chips(1)[1].x + chips(1)[1].w / 2, footY(1) + 10], to: [L3 + 150, S3.y + 100], c0: [chips(1)[1].x, footY(1) + 180], c1: [L3 + 420, S3.y - 40] },
  { t: T.go4, from: [DST.x + TW + 14, DST.y + 12], to: [L4 + 120, S4.y + 100], c0: [DST.x + TW + 200, DST.y + 12], c1: [L4 - 120, S4.y + 100] },
];
function handoffs(R, t, a) {
  for (const h of HANDOFF) {
    const p = seg(t, h.t - 0.05, h.t + 0.65);
    if (p <= 0 || p >= 1) continue;
    const pts = bezierPts(h.from, h.c0, h.c1, h.to, 48);
    const q = E.inOutCubic(p);
    poly(R, pts, { color: C.ice, w: 1.3, alpha: a * 0.6, glow: 0.6 }, q, Math.max(0, q - 0.3));
    const pt = pts[Math.round(q * (pts.length - 1))];
    circle(R, pt[0], pt[1], 4.6, { fill: C.ice, alpha: a, glow: 1 });
    light(R, pt[0], pt[1], 44, C.ice, 0.25 * a, 1);
  }
}

// ── focus: the card the camera reads is lit, the others rest ────────────
const FOCUS = [[-1, T.go2], [T.go2, T.go3], [T.go3, T.go4], [T.go4, Infinity]];
const DONE_AT = [T.understood + 0.2, T.saved + 0.25, T.produced + 0.2, T.limit + 0.45];
function focusOf(i, t) {
  const [f0, f1] = FOCUS[i];
  const on = smooth(f0, f0 + 0.45, t) * (f1 === Infinity ? 1 : 1 - smooth(f1 + 0.1, f1 + 0.55, t));
  return Math.max(on, 0.16, smooth(T.wide - 0.75, T.wide - 0.1, t));
}

// the last shot names the moments in order, once
function recap(R, t, a) {
  CARD.forEach((_, i) => {
    const t0 = T.wide + 0.05 + i * 0.16;
    const k = seg(t, t0, t0 + 0.6);
    if (k <= 0 || k >= 1) return;
    const bd = badge(i);
    circle(R, bd.x, bd.y, 15 + 22 * E.outCubic(k), { color: C.teal, w: 1.6, alpha: a * (1 - k), glow: 1 });
  });
}

export function draw(R, t) {
  const a = loopFade(t, meta.duration);
  const f = CARD.map((_, i) => focusOf(i, t));
  CARD.forEach((_, i) => card(R, t, i, a, f[i], smooth(DONE_AT[i], DONE_AT[i] + 0.35, t)));
  CARD.forEach((_, i) => folder(R, t, i, a * f[i]));
  say(R, t, a * f[0]);
  if (t >= T.proposes - 0.3) checkIt(R, t, a * f[1]);
  if (t >= T.run - 0.35) runIt(R, t, a * f[2]);
  if (t >= T.proof - 0.35) proveIt(R, t, a * f[3]);
  handoffs(R, t, a);
  recap(R, t, a);
}
