// typescript-client · "Run it from your app. Prove what ran."
// The quick start of the TypeScript package (@supernovae-st/nika), as a Node
// app runs it: demo.mts calls run() on hello.nika, the engine the package
// bundles checks the file and runs it (mock/echo: a rehearsal), the app gets
// a typed result, and hands the run's receipt back to traceVerify().
//
// Everything on screen that is program, output or proof is read from the
// capture (scripts/media/capture/typescript-client.sh → media/raw/
// typescript-client-*): the program's key lines with their real line
// numbers, TypeScript's own inlay types for them, the terminal output, the
// engine's check verdict and plan, the run's journal (each line's hash,
// chained, ending on the head the verifier prints) and the verifier's words.
// What is illustration: the editor and terminal frames, the package drawn
// as a bridge, the lanes and the packets that travel them.
//
// The receipt ticket carries the journal's head. The capture never prints
// the receipt itself: the program prints "receipt verified", and the
// package says so only when the receipt's chain_head is that journal's
// head and the seal verifies (nika-client, receiptMismatch). The clip
// refuses to render if the output stops saying it.
import { C, E, seg, smooth, clamp, lerp, readRepo, repoLines, cw, mono, panel, pill, terminal, frame, loopFade, cameraPath, titleFade, WIDE } from './kit.mjs';
import { text, rrect, rect, line, circle, arc, poly, check, measure, metrics, bezierPts, light, DW, DH } from '../src/engine/render.mjs';
import { rgba } from '../src/engine/core.mjs';

export const meta = { duration: 22.5, poster: 21.5 };
// in from black and back out, briefly: a faded frame repaints the whole GIF frame
const envelope = t => loopFade(t, meta.duration, 0.2, 0.2);

// ── the capture ─────────────────────────────────────────────────────────
const RAW = 'media/raw/typescript-client';
const demo = repoLines(`${RAW}-demo.mts`);
const hello = repoLines(`${RAW}-hello.nika`);
const output = repoLines(`${RAW}-output.txt`);
const checked = repoLines(`${RAW}-check.txt`);
const verify = repoLines(`${RAW}-verify.txt`);
const journal = JSON.parse(readRepo(`${RAW}-journal.json`));
const types = JSON.parse(readRepo(`${RAW}-types.json`));
const versions = JSON.parse(readRepo(`${RAW}-versions.json`));

const fail = why => { throw new Error(`typescript-client capture: ${why}`); };
const PROMPT = hello.map(l => l.match(/^\s*prompt: "(.*)"$/)?.[1]).find(Boolean) ?? fail('hello.nika has no prompt');
const GREETING = output.find(l => l === `mock(echo) · ${PROMPT}`) ?? fail('the program no longer prints the greeting');
const VERDICT = output.find(l => l === 'receipt verified') ?? fail('the program no longer says "receipt verified"');
const READY = checked.some(l => /^ layers · .*run ready ✔$/u.test(l)) ? 'run ready ✔' : fail('hello.nika is not run ready');
// the plan's one task: "greeting (infer · mock/echo)"
const PLAN = (checked.find(l => /^\s+wave 1 \S+ \(.*\)$/u.test(l)) ?? fail('no plan line')).trim().replace(/^wave 1 /, '');
const OK = verify.find(l => /^OK — \d+ events · chain intact · head [0-9a-f]{64}$/u.test(l)) ?? fail('the verifier no longer says chain intact');
const SEALED = verify.find(l => /^SEALED — the run_sealed signature verifies/u.test(l)) ?? fail('the run is no longer sealed');
const HEAD = OK.match(/head ([0-9a-f]{64})/)[1];
const EVENTS = +OK.match(/(\d+) events/)[1];
if (journal.length !== EVENTS) fail(`the journal holds ${journal.length} lines, the verifier counted ${EVENTS}`);
journal.forEach((j, i) => { if (i && j.chain !== journal[i - 1].hash) fail(`journal line ${i + 1} does not chain`); });
if (journal[EVENTS - 1].hash !== HEAD || journal[EVENTS - 1].kind !== 'run_sealed') fail('the head is not the sealed last line');
if (EVENTS > 6) fail(`the journal holds ${EVENTS} lines: the record panel is laid out for six`);
if (types.diagnostics.length) fail('the program no longer type-checks');
// what the verifier says, verbatim, cut before the hash and the key it names
const VERIFY_ROWS = [
  OK.slice(0, OK.indexOf(' · head')),
  ...SEALED.replace(/ · key .*$/u, '').replace(/^(SEALED — the run_sealed) (signature verifies)$/u, '$1\n$2').split('\n'),
];
const PKG = versions.package.version;
const ENGINE_VER = versions.engine.version;
const BUNDLED = versions.engine.source === 'bundled';
const short = h => `${h.slice(0, 8)}…`;

// the key lines of demo.mts, by what they say (their real numbers are kept)
const lineOf = re => (demo.findIndex(l => re.test(l)) + 1) || fail(`demo.mts lost ${re}`);
const L = {
  imp: lineOf(/^import \{ Nika, isNikaRunSucceeded \} from '@supernovae-st\/nika';$/),
  client: lineOf(/^const nika = new Nika\(/),
  run: lineOf(/^const run = await nika\.run<\{ greeting: string \}>\('hello\.nika'/),
  result: lineOf(/^const result = await run\.result\(\);$/),
  guard: lineOf(/^if \(!isNikaRunSucceeded\(result\) \|\| !result\.receipt\) \{$/),
  log: lineOf(/^console\.log\(result\.outputs\?\.greeting\);$/),
  verify: lineOf(/^const proof = await nika\.traceVerify\(result\.receipt\);$/),
  verdict: lineOf(/^console\.log\(proof\.verified \? 'receipt verified'/),
};
if (!demo.slice(L.guard).includes('}')) fail('the guard has no closing brace');
const ROWS = [L.imp, L.client, L.run, L.result, L.guard, L.log, L.verify, L.verdict];
// TypeScript's inlay types: what an editor shows after a declaration
const inlayAt = ln => types.inlayHints.find(h => h.line === ln && h.kind === 'Type') ?? fail(`no inlay type on line ${ln}`);
const INLAY_RESULT = inlayAt(L.result);
const INLAY_PROOF = inlayAt(L.verify);
if (!/\{ greeting: string; \}/.test(INLAY_RESULT.text)) fail('the result is no longer typed { greeting: string }');

// ── a small TypeScript tokenizer: it colours, it never rewrites ─────────
const KEYWORDS = new Set(['import', 'from', 'const', 'let', 'await', 'new', 'if', 'return', 'export', 'async', 'function', 'typeof', 'as', 'true', 'false', 'null', 'undefined']);
const PRIMITIVES = new Set(['string', 'number', 'boolean', 'unknown', 'void', 'never', 'any']);
const OPS = ['===', '!==', '...', '?.', '||', '&&', '=>', '??'];
function lex(src) {
  const out = [];
  let i = 0;
  while (i < src.length) {
    const ch = src[i];
    let j = i + 1, kind = 'punct';
    if (/\s/.test(ch)) { while (j < src.length && /\s/.test(src[j])) j++; kind = 'ws'; }
    else if (src.startsWith('//', i)) { j = src.length; kind = 'comment'; }
    else if (ch === "'" || ch === '"') { while (j < src.length && src[j] !== ch) j += src[j] === '\\' ? 2 : 1; j++; kind = 'string'; }
    else if (ch === '`') {
      // a template literal: its text is a string, its ${ } holes are code
      let start = i;
      while (j < src.length && src[j] !== '`') {
        if (src.startsWith('${', j)) {
          out.push({ s: src.slice(start, j), kind: 'string' }, { s: '${', kind: 'punct' });
          let k = j + 2, depth = 1;
          for (; k < src.length; k++) { if (src[k] === '{') depth++; if (src[k] === '}' && --depth === 0) break; }
          out.push(...lex(src.slice(j + 2, k)), { s: '}', kind: 'punct' });
          j = k + 1;
          start = j;
        } else j++;
      }
      out.push({ s: src.slice(start, j + 1), kind: 'string' });
      i = j + 1;
      continue;
    } else if (/\d/.test(ch)) { while (j < src.length && /[\d.]/.test(src[j])) j++; kind = 'number'; }
    else if (/[A-Za-z_$]/.test(ch)) { while (j < src.length && /[\w$]/.test(src[j])) j++; kind = 'ident'; }
    else { const op = OPS.find(o => src.startsWith(o, i)); if (op) j = i + op.length; }
    out.push({ s: src.slice(i, j), kind });
    i = j;
  }
  return out;
}
function classify(tokens) {
  const sig = tokens.map((tk, k) => (tk.kind === 'ws' ? -1 : k)).filter(k => k >= 0);
  let braces = 0;
  return tokens.map((tk, k) => {
    if (tk.s === '{' || tk.s === '${') braces++;
    if (tk.s === '}') braces--;
    if (tk.kind !== 'ident') return tk;
    const at = sig.indexOf(k);
    const prev = at > 0 ? tokens[sig[at - 1]].s : '', next = at + 1 < sig.length ? tokens[sig[at + 1]].s : '';
    if (KEYWORDS.has(tk.s)) return { ...tk, kind: 'keyword' };
    if (PRIMITIVES.has(tk.s) || (/^[A-Z]/.test(tk.s) && prev !== '.')) return { ...tk, kind: 'type' };
    if (next === '(' || (next === '<' && prev === '.')) return { ...tk, kind: 'call' };
    if (prev === '.' || prev === '?.' || (next === ':' && braces > 0)) return { ...tk, kind: 'prop' };
    return { ...tk, kind: 'name' };
  });
}
const TYPE = '#C3B2FF';
const TS = { keyword: '#86A8FF', type: TYPE, call: C.cyan, prop: '#B3C5DD', name: C.ink, string: '#E8C79B', number: '#E8C79B', punct: '#7C90B0', comment: C.dim, ws: C.ink };
const spansOf = src => classify(lex(src)).map(tk => ({ s: tk.s, c: TS[tk.kind] }));
const spanLen = spans => spans.reduce((s, sp) => s + sp.s.length, 0);
// characters [a, b) of a line of spans
const slice = (spans, a, b = Infinity) => {
  const out = [];
  let pos = 0;
  for (const sp of spans) {
    const s0 = Math.max(a, pos), s1 = Math.min(b, pos + sp.s.length);
    if (s1 > s0) out.push({ ...sp, s: sp.s.slice(s0 - pos, s1 - pos) });
    pos += sp.s.length;
  }
  return out;
};

// ── layout (design px) ──────────────────────────────────────────────────
const CODE = { f: 'MM 400', size: 14 };
const ADV = cw(CODE);
const LH = 30;
const EDITOR = { x: 64, y: 356, w: 904, h: 324 };
const TERM = { x: 64, y: 704, w: 904, h: 206 };
const ENGINE = { x: 1300, y: 356, w: 556, h: 554 };
const BRIDGE = { x0: EDITOR.x + EDITOR.w, x1: ENGINE.x, cx: (EDITOR.x + EDITOR.w + ENGINE.x) / 2, top: 456, bottom: ENGINE.y + ENGINE.h - 6 };
const GUTTER = 64;
const codeX = EDITOR.x + GUTTER;
const rowY = i => EDITOR.y + 44 + 38 + i * LH;
const rowOfLine = ln => ROWS.indexOf(ln);
const TERM_ST = { f: 'MM 400', size: 22 };
const TERM_LH = 38;
const termRowY = r => TERM.y + 44 + 30 + r * TERM_LH;
// the engine's stations
const EX = ENGINE.x + 28;
const ST = { check: 432, run: 510, record: 588, verify: 784 };
const jRowY = k => 620 + k * 24;
const J = { dot: EX + 8, hash: EX + 30, kind: EX + 150, tick: ENGINE.x + ENGINE.w - 34 };
const TICKET = { x: J.hash - 16, y: 803, w: 166, h: 62 };
const VX = TICKET.x + TICKET.w + 18; // the verifier's words, beside the ticket
// the generic the program types its outputs with, on the run line
const GEN = (() => {
  const src = demo[L.run - 1], i0 = src.indexOf('<{'), i1 = src.indexOf('}>') + 2;
  return { x: codeX + i0 * ADV - 4, y: rowY(rowOfLine(L.run)) - 17, w: (i1 - i0) * ADV + 8, h: 24 };
})();

// ── time (seconds) ──────────────────────────────────────────────────────
const T = {
  win: 0.35, bridge: 0.7,
  type0: 1.05, typeEvery: 0.15, typeDur: 0.24,
  gen: 3.35,
  cmd: 5.75,
  p1: 6.45, p1e: 7.2,
  scan: 7.2, ready: 7.65,
  task: 8.05,
  p2: 9.7, p2e: 10.6,
  inlay: 11.15,
  log: 12.05, greet: 12.25,
  tvHl: 14.15,
  p3: 14.45, p3e: 15.2,
  walk: 15.45, walkEvery: 0.1,
  ok: 16.1, sealed: 16.4, match: 16.8,
  p4: 18.15, p4e: 19.05,
  verdict: 19.12,
  lap: 19.45, lapEach: 0.28,
};
// the journal lands line by line while the task runs; the task is done when
// its task_completed line lands
T.events = journal.map((_, i) => T.task + (1.25 * i) / Math.max(1, EVENTS - 1));
T.taskDone = T.events[journal.findIndex(j => j.kind === 'task_completed')] ?? T.task + 0.7;

// ── lanes across the package ────────────────────────────────────────────
// Each call leaves the code line that makes it, each answer returns to the
// line that reads it, and the engine end sits on the station involved.
// A token of a code line: where it sits, for a packet to leave or land on.
const token = (ln, s) => {
  const col = demo[ln - 1].indexOf(s);
  if (col < 0) fail(`line ${ln} lost ${s}`);
  return { ln, col, len: s.length, x: codeX + (col + s.length / 2) * ADV, y: rowY(rowOfLine(ln)) - 5 };
};
const TOK = {
  file: token(L.run, "'hello.nika'"),
  result: token(L.result, 'result'),
  receipt: token(L.verify, 'result.receipt'),
  verified: token(L.verdict, 'proof.verified'),
};
const LANES = [
  { id: 'run', from: [BRIDGE.x0, TOK.file.y], to: [BRIDGE.x1, ST.check + 26], color: C.ice, label: 'run()', side: 1, t0: T.p1, t1: T.p1e, packet: 'hello.nika', leave: TOK.file },
  { id: 'result', from: [BRIDGE.x1, jRowY(2) - 4], to: [BRIDGE.x0, TOK.result.y], color: C.cyan, label: 'result', side: -1, t0: T.p2, t1: T.p2e, packet: 'outputs + receipt', land: TOK.result },
  { id: 'receipt', from: [BRIDGE.x0, TOK.receipt.y], to: [BRIDGE.x1, TICKET.y + TICKET.h / 2], color: C.gold, label: 'traceVerify()', side: -1, t0: T.p3, t1: T.p3e, packet: null, leave: TOK.receipt },
  // the proof leaves from under the plate while the camera holds on the
  // verifier, so it shows only once the camera pulls back
  { id: 'proof', from: [BRIDGE.x1, TICKET.y + TICKET.h + 26], to: [BRIDGE.x0, TOK.verified.y], color: C.teal, label: 'proof', side: 1, t0: T.p4, t1: T.p4e, packet: 'verified: true', land: TOK.verified, appear: 0.15 },
];
// a polyline with its cumulative lengths, walked at constant speed
const measured = pts => {
  const acc = [0];
  for (let i = 1; i < pts.length; i++) acc.push(acc[i - 1] + Math.hypot(pts[i][0] - pts[i - 1][0], pts[i][1] - pts[i - 1][1]));
  return { pts, acc, len: acc[acc.length - 1] };
};
const along = (path, p) => {
  const d = clamp(p) * path.len;
  let i = 1;
  while (i < path.acc.length - 1 && path.acc[i] < d) i++;
  const u = (d - path.acc[i - 1]) / (path.acc[i] - path.acc[i - 1] || 1);
  return [lerp(path.pts[i - 1][0], path.pts[i][0], u), lerp(path.pts[i - 1][1], path.pts[i][1], u)];
};
for (const ln of LANES) {
  const [x0, y0] = ln.from, [x1, y1] = ln.to;
  const d = (x1 - x0) * 0.42;
  ln.pts = bezierPts([x0, y0], [x0 + d, y0], [x1 - d, y1], [x1, y1], 48);
  ln.path = measured(ln.pts);
  // a packet leaves from its token, or lands on it, along the code line;
  // [a, b] is the part of its trip that crosses the bridge
  ln.trip = measured([...(ln.leave ? [[ln.leave.x, ln.leave.y]] : []), ...ln.pts, ...(ln.land ? [[ln.land.x, ln.land.y]] : [])]);
  const lead = ln.leave ? Math.hypot(ln.pts[0][0] - ln.leave.x, ln.pts[0][1] - ln.leave.y) : 0;
  ln.a = lead / ln.trip.len;
  ln.b = (lead + ln.path.len) / ln.trip.len;
}
const LANE = Object.fromEntries(LANES.map(l => [l.id, l]));
// a lane's name rides the middle of the lane, above or below it
const LANE_LABEL = { f: 'MM 500', size: 22 };
const laneLabel = ln => {
  const [x, my] = along(ln.path, 0.5);
  const w = measure(ln.label, LANE_LABEL);
  return { x, y: my + (ln.side < 0 ? -16 : 32), x0: x - w / 2, x1: x + w / 2 };
};
// how far along its trip a packet is, and along its lane
const tripAt = (ln, t) => E.inOutCubic(seg(t, ln.t0, ln.t1));
const laneAt = (ln, t) => clamp((tripAt(ln, t) - ln.a) / (ln.b - ln.a));

// the code tokens the packets leave from and land on
const MARKS = [
  { tok: TOK.file, color: C.ice, t0: T.p1 - 0.3, t1: T.p1 + 0.7 },
  { tok: TOK.result, color: C.cyan, t0: T.p2e - 0.12, t1: T.p2e + 0.8 },
  { tok: TOK.receipt, color: C.gold, t0: T.p3 - 0.3, t1: T.p3 + 0.7 },
  { tok: TOK.verified, color: C.teal, t0: T.p4e - 0.12, t1: T.p4e + 0.9 },
];

// ── camera ──────────────────────────────────────────────────────────────
// While the camera is in, the world passes under two bands of background
// colour, one under the kicker and one under the plate: opaque to `hold` px
// from the frame's edge, clear at `end`. Each pushed-in shot is framed so
// that every line of text is either wholly under the opaque part or wholly
// clear of the fade.
const SCRIM = { hold: 78, end: 118 };
const scrimAt = cam => smooth(1.02, 1.2, cam.s);
// a line of text as it is drawn: how far its glyphs reach above and below
// its baseline (world px). The kit paints ✔ and its kin as vectors inside
// the line's box, so they are measured as a space.
const span = (str, st, y) => {
  const m = metrics(str.replace(/[✔✖⚠○↳▸╭╰━≥≤↔⋯🦋]/gu, ' '), st);
  return { str, top: y - m.actualBoundingBoxAscent, bottom: y + m.actualBoundingBoxDescent };
};
const area = (str, top, bottom) => ({ str, top, bottom });
// The camera y at scale s that keeps every `clear` line wholly clear of the
// bands and every `above` (`below`) line wholly under the opaque top
// (bottom) band: the middle of that range, or the clip refuses to render.
const bandY = (what, s, { clear = [], above = [], below = [] }) => {
  const open = (DH / 2 - SCRIM.end) / s, shut = (DH / 2 - SCRIM.hold) / s;
  let lo = -Infinity, hi = Infinity;
  for (const l of clear) { hi = Math.min(hi, l.top + open); lo = Math.max(lo, l.bottom - open); }
  for (const l of above) lo = Math.max(lo, l.bottom + shut);
  for (const l of below) hi = Math.min(hi, l.top - shut);
  if (lo > hi) fail(`the ${what} shot cannot keep its text clear of the chrome bands`);
  return (lo + hi) / 2;
};
const PANEL_TITLE = { f: 'MM 500', size: 15 };
const BADGE = { f: 'MGW 500', size: 11, tracking: 2.5 };
const ZONE = { f: 'Geist 600', size: 24, tracking: -0.4 };
const ZONE_SUB = { f: 'MM 400', size: 14 };
const STATION = { f: 'MGW 500', size: 11.5, tracking: 2.5 };
const BIG = { f: 'MM 500', size: 22 };
const CHIP = { f: 'MM 500', size: 16 };
const HASH = { f: 'MM 500', size: 14 };
const KIND = { f: 'MM 400', size: 14 };
const VERIFY_ST = { f: 'MM 400', size: 15 };
const ENGINE_TITLE = BUNDLED ? `nika ${ENGINE_VER} · a child process` : `nika ${ENGINE_VER} · via NIKA_BIN`;
const ENGINE_SUB = BUNDLED ? 'bundled with the package' : 'from PATH, through NIKA_BIN';
// what each line of the editor holds once TypeScript's inlay types show
const rowText = ln => demo[ln - 1] + (ln === L.result ? INLAY_RESULT.text : ln === L.verify ? INLAY_PROOF.text : '');
const codeRows = ROWS.map((ln, i) => span(rowText(ln), CODE, rowY(i)));
const editorHead = [span('demo.mts · the key lines', PANEL_TITLE, EDITOR.y + 28), span(`TSC ${types.typescript} · NO ERRORS`, BADGE, EDITOR.y + 26.5)];
const appZone = [span('Your app', ZONE, EDITOR.y - 22), span(`node ${versions.node} · typescript`, ZONE_SUB, EDITOR.y - 22)];

// the code: the editor at full width, its zone title above it, the
// terminal's header under the plate
const S_CODE = 1840 / (EDITOR.w + 24);
const SHOT_CODE = {
  s: S_CODE, x: EDITOR.x + EDITOR.w / 2,
  y: bandY('code', S_CODE, {
    clear: [...appZone, ...editorHead, ...codeRows, area('the editor', EDITOR.y, EDITOR.y + EDITOR.h)],
    below: [span('terminal', PANEL_TITLE, TERM.y + 28)],
  }),
};
// the typed result: from line 1 down to the greeting the terminal prints,
// the editor's header under the kicker
const SHOT_RESULT = {
  s: S_CODE, x: EDITOR.x + EDITOR.w / 2,
  y: bandY('result', S_CODE, {
    clear: [...codeRows, span('terminal', PANEL_TITLE, TERM.y + 28), span('MOCK/ECHO · A REHEARSAL', BADGE, TERM.y + 26.5), span(`$ ${versions.command}`, TERM_ST, termRowY(0)), span(GREETING, TERM_ST, termRowY(1))],
    above: [...appZone, ...editorHead],
  }),
};
// the verification: all four of the engine's stations and every lane's
// name, the engine's header under the kicker; the view starts at the
// editor's edge, so no line of code is cut by the frame
const S_VERIFY = 1.88;
const SHOT_VERIFY = {
  s: S_VERIFY, x: EDITOR.x + EDITOR.w + 4 + DW / 2 / S_VERIFY,
  y: bandY('verify', S_VERIFY, {
    clear: [
      span('CHECK · BEFORE ANYTHING RUNS', STATION, ST.check), span('hello.nika', CHIP, ST.check + 32), span(READY, BIG, ST.check + 34),
      span('RUN', STATION, ST.run), span('REHEARSAL · NO KEY · NO NETWORK', BADGE, ST.run - 0.5), span(PLAN, BIG, ST.run + 32),
      span(`RECORD · ${EVENTS} EVENTS, HASH-CHAINED`, STATION, ST.record),
      ...journal.flatMap((j, i) => [span(short(j.hash), HASH, jRowY(i)), span(j.kind, KIND, jRowY(i))]),
      span('VERIFY THE RECEIPT', STATION, ST.verify), ...VERIFY_ROWS.map((row, i) => span(row, VERIFY_ST, ST.verify + 28 + i * 24)),
      span('result.receipt', { f: 'MM 500', size: 13 }, TICKET.y + 24), span(short(HEAD), HASH, TICKET.y + 48),
      ...LANES.map(ln => span(ln.label, LANE_LABEL, laneLabel(ln).y)),
    ],
    above: [span(ENGINE_TITLE, PANEL_TITLE, ENGINE.y + 28), span('VERIFIED', BADGE, ENGINE.y + 26.5), span('The Nika engine', ZONE, ENGINE.y - 22), span(ENGINE_SUB, ZONE_SUB, ENGINE.y - 22)],
  }),
};
if (SHOT_VERIFY.x + DW / 2 / S_VERIFY < ENGINE.x + ENGINE.w + 8) fail('the verify shot cuts the engine');
if (SHOT_VERIFY.x - DW / 2 / S_VERIFY > Math.min(...LANES.map(ln => laneLabel(ln).x0))) fail('the verify shot cuts a lane label');
// Five short moves: every frame of a move repaints the whole GIF frame.
const SHOTS = [
  { at: 0, cam: WIDE },
  { at: 2.9, cam: SHOT_CODE, move: 0.6 },
  { at: 5.55, cam: WIDE, move: 0.6 },
  { at: 11.05, cam: SHOT_RESULT, move: 0.6 },
  { at: 15.25, cam: SHOT_VERIFY, move: 0.6 },
  { at: 18.9, cam: WIDE, move: 0.6 },
];
export const camera = t => cameraPath(t, SHOTS);

// the dot grid drifts a quarter as far as the world when the camera moves:
// depth, at no cost while the camera holds
export function env(t) {
  const cam = camera(t);
  return { bgGlow: 1, gridAlpha: 0.22, gridX: -(cam.x - 960) * cam.s * 0.25, gridY: -(cam.y - 540) * cam.s * 0.25, bgY: 560 };
}

// the plate: what is illustration, what was captured with what (the first
// wording that fits the frame's plate line)
const PLATE = (() => {
  const eng = BUNDLED ? `its bundled engine ${ENGINE_VER}` : `engine ${ENGINE_VER} on path (nika_bin)`;
  const options = [
    `editor, lanes and motion: illustration · captured: @supernovae-st/nika ${PKG} + ${eng} · types: tsc ${types.typescript} · a mock/echo rehearsal`,
    `editor, lanes, motion: illustration · captured: @supernovae-st/nika ${PKG} + ${eng} · a mock/echo rehearsal`,
  ];
  return options.find(s => measure(s.toUpperCase(), { f: 'MGW 500', size: 10.5, tracking: 3 }) <= 1680) ?? options[options.length - 1];
})();

// The headline, word by word: each word rises into place, the accent last.
const HEADLINE = { x: 64, y: 172, st: { f: 'Geist 600', size: 58, tracking: -58 * 0.028 } };
const TITLE = (() => {
  const main = 'Run it from your app. Prove what ran.', accent = 'Prove what ran.';
  const words = [];
  let at = 0;
  main.split(' ').forEach((w, i) => {
    const pre = main.slice(0, at);
    words.push({ w, x: HEADLINE.x + (at ? measure(pre, HEADLINE.st) + HEADLINE.st.tracking : 0), hot: at >= main.length - accent.length, i });
    at += w.length + 1;
  });
  return words;
})();
function title(R, t, a) {
  if (a <= 0) return;
  TITLE.forEach(({ w, x, hot, i }) => {
    const t0 = 0.2 + i * 0.07 + (hot ? 0.2 : 0);
    const k = E.snap(seg(t, t0, t0 + 0.55));
    if (k <= 0) return;
    text(R, w, x, HEADLINE.y + 18 * (1 - k), { ...HEADLINE.st, color: hot ? C.teal : C.ink, alpha: a * smooth(t0, t0 + 0.3, t), glow: hot ? 0.4 : 0.18 });
  });
  const sk = smooth(0.85, 1.2, t);
  text(R, 'A CHECKED WORKFLOW · A TYPED RESULT · A RECEIPT THE ENGINE VERIFIES', HEADLINE.x + 2, HEADLINE.y + 32, { f: 'MGW 500', size: 10.5, tracking: 3, color: C.teal, alpha: a * 0.9 * sk });
}

// the bands, in screen space: the background colour, opaque from the
// frame's edge to `hold` (bright text leaks through anything less), then
// eased out; in the glow pass they erase the light of whatever passes
// beneath, so nothing blooms through them
function scrims(R, k) {
  if (k <= 0.001) return;
  const ctx = R.ctx;
  const { hold, end } = SCRIM;
  for (const [edge, dir] of [[0, 1], [DH, -1]]) {
    const g = ctx.createLinearGradient(0, edge, 0, edge + dir * end);
    const stop = (y, al) => g.addColorStop(y / end, rgba(C.bg0, al * k));
    stop(0, 1);
    stop(hold, 1);
    stop(hold + 10, 0.8);
    stop(hold + 20, 0.44);
    stop(hold + 30, 0.14);
    stop(end, 0);
    ctx.save();
    if (R.glowPass) ctx.globalCompositeOperation = 'destination-out';
    ctx.fillStyle = g;
    ctx.fillRect(0, dir > 0 ? edge : edge - end, DW, end);
    ctx.restore();
  }
}

export function chrome(R, t) {
  const a = envelope(t);
  const cam = camera(t);
  scrims(R, scrimAt(cam));
  frame(R, t, { kicker: 'the typescript package · the readme quick start', plate: PLATE, alpha: a });
  title(R, t, a * titleFade(cam));
}

// ── the app: the editor ────────────────────────────────────────────────
const INLAY_ST = { f: 'MM 400', size: CODE.size };
const inlayW = hint => measure(hint, INLAY_ST) + 14;
// an inlay type opening in its line (k 0..1); returns the width it takes
function inlay(R, x, y, hint, k, alpha, lit) {
  if (k <= 0) return 0;
  const w = inlayW(hint);
  rrect(R, x + 3, y - 15, (w - 6) * k, 21, 5, { fill: '#17264a', fillAlpha: 0.95, alpha });
  const ctx = R.ctx;
  ctx.save();
  ctx.beginPath();
  ctx.rect(x, y - 20, w * k, 30);
  ctx.clip();
  const m = hint.match(/^(.*?)(greeting: string)(.*)$/);
  const dim = '#9AAECB';
  const spans = m ? [{ s: m[1], c: dim }, { s: m[2], c: lit > 0 ? C.teal : TYPE, glow: lit * 0.5 }, { s: m[3], c: dim }] : [{ s: hint, c: dim }];
  mono(R, spans, x + 7, y, { alpha, st: INLAY_ST });
  ctx.restore();
  return w * k;
}

function editor(R, t, a) {
  const k = E.snap(seg(t, T.win, T.win + 0.5));
  panel(R, EDITOR, {
    title: 'demo.mts · the key lines', alpha: a, k,
    badge: t > T.type0 + 1.7 ? { label: `TSC ${types.typescript} · NO ERRORS`, color: TYPE, alpha: smooth(T.type0 + 1.7, T.type0 + 2, t) } : null,
  });
  if (k < 1) return;
  // a row lights while its call crosses, or its answer is read
  const hl = [
    { ln: L.run, t0: T.p1 - 0.3, t1: T.p1e + 0.3, c: C.ice },
    { ln: L.result, t0: T.p2e - 0.05, t1: T.log, c: C.cyan },
    { ln: L.log, t0: T.log, t1: T.tvHl, c: C.cyan },
    { ln: L.verify, t0: T.tvHl, t1: T.p3e + 0.3, c: C.gold },
    { ln: L.verdict, t0: T.p4e - 0.05, t1: T.lap + 0.8, c: C.teal },
  ];
  ROWS.forEach((ln, i) => {
    const y = rowY(i);
    const t0 = T.type0 + i * T.typeEvery;
    if (t < t0) return;
    const spans = spansOf(demo[ln - 1]);
    const n = Math.floor(spanLen(spans) * clamp((t - t0) / T.typeDur));
    const typing = n < spanLen(spans);
    for (const h of hl) {
      if (h.ln !== ln || t < h.t0 || t > h.t1) continue;
      const hk = smooth(h.t0, h.t0 + 0.2, t) * (1 - smooth(h.t1 - 0.25, h.t1, t));
      rect(R, EDITOR.x + 6, y - LH + 10, EDITOR.w - 12, LH, { fill: h.c, alpha: a * 0.1 * hk });
      rect(R, EDITOR.x + 6, y - LH + 10, 3, LH, { fill: h.c, alpha: a * hk, glow: 0.8 });
    }
    text(R, String(ln), codeX - 16, y, { f: 'MM 400', size: CODE.size - 2, color: C.dim, alpha: a * 0.85, align: 'right' });
    let shown = typing ? slice(spans, 0, n) : spans;
    // the guard is folded, as an editor folds a block: its first line, ⋯, }
    if (ln === L.guard && !typing) shown = [...spans, { s: '⋯', c: C.dim }, { s: '}', c: TS.punct }];
    const hint = ln === L.result ? { h: INLAY_RESULT, t0: T.inlay } : ln === L.verify ? { h: INLAY_PROOF, t0: T.verdict - 0.1 } : null;
    if (hint && t >= hint.t0 && !typing) {
      // TypeScript's inlay type opens at its column and pushes the rest right
      const col = hint.h.column - 1;
      const ik = E.snap(seg(t, hint.t0 + 0.1, hint.t0 + 0.55));
      const lit = ln === L.result ? 1 - smooth(hint.t0 + 1.4, hint.t0 + 2.4, t) : 0;
      mono(R, slice(shown, 0, col), codeX, y, { alpha: a, st: CODE });
      const dx = inlay(R, codeX + col * ADV, y, hint.h.text, ik, a, lit);
      mono(R, slice(shown, col), codeX + col * ADV + dx, y, { alpha: a, st: CODE });
    } else mono(R, shown, codeX, y, { alpha: a, st: CODE });
    // the token a packet leaves from, or lands on, lights up
    for (const mk of MARKS) {
      if (mk.tok.ln !== ln || t < mk.t0 || t > mk.t1 || typing) continue;
      const mk2 = smooth(mk.t0, mk.t0 + 0.15, t) * (1 - smooth(mk.t1 - 0.3, mk.t1, t));
      const x = codeX + mk.tok.col * ADV;
      rrect(R, x - 3, y - 16, mk.tok.len * ADV + 6, 22, 5, { fill: mk.color, fillAlpha: 0.16, color: mk.color, w: 1, alpha: a * mk2, glow: 0.5 * mk2 });
      mono(R, [{ s: demo[ln - 1].slice(mk.tok.col, mk.tok.col + mk.tok.len), c: mk.color, glow: 0.6 }], x, y, { alpha: a * mk2, st: CODE });
    }
    if (typing || (i === ROWS.length - 1 && t < t0 + T.typeDur + 0.5)) {
      const blink = Math.floor(t * 2.4) % 2 === 0 ? 1 : 0.3;
      rect(R, codeX + (typing ? n : spanLen(shown)) * ADV + 1, y - 14, 2, 18, { fill: C.ice, alpha: a * blink, glow: 0.8 });
    }
  });
  typeBox(R, t, a);
}

// The type you give run<…> is the type your result comes back with: the
// box around the generic moves onto the inlay TypeScript shows for result.
function typeBox(R, t, a) {
  const on = smooth(T.gen, T.gen + 0.3, t);
  if (on <= 0) return;
  const col = INLAY_RESULT.column - 1;
  const rowBase = rowY(rowOfLine(L.result));
  // it flies to the inlay's column, then opens with it, framing only it
  const m = E.snap(seg(t, T.inlay - 0.1, T.inlay + 0.4));
  const ik = E.snap(seg(t, T.inlay + 0.1, T.inlay + 0.55));
  const B = { x: codeX + col * ADV + 3, y: rowBase - 15, w: Math.max(36, (inlayW(INLAY_RESULT.text) - 6) * ik), h: 21 };
  const box = { x: lerp(GEN.x, B.x, m), y: lerp(GEN.y, B.y, m), w: lerp(GEN.w, B.w, m), h: lerp(GEN.h, B.h, m) };
  const lit = 1 - smooth(T.inlay + 1.4, T.inlay + 2.4, t);
  const rest = m < 1 ? 0.55 + 0.45 * smooth(T.gen, T.gen + 0.4, t) * (1 - smooth(T.p1e, T.p1e + 0.6, t)) : 0.35 + 0.65 * lit;
  const c = m >= 1 && lit > 0 ? C.teal : TYPE;
  rrect(R, box.x, box.y, box.w, box.h, 5, { color: c, w: 1.3, alpha: a * on * rest, fill: c, fillAlpha: m < 1 ? 0.06 : 0, glow: 0.5 * rest });
  if (m > 0 && m < 1) light(R, box.x + box.w / 2, box.y + box.h / 2, 60, TYPE, 0.3 * a, 1);
}

// ── the bridge: the package between the app and the engine ─────────────
function bridge(R, t, a) {
  const k = E.snap(seg(t, T.bridge, T.bridge + 0.7));
  if (k <= 0) return;
  const { cx, top, bottom } = BRIDGE;
  const ctx = R.ctx;
  const h = (bottom - top) * k;
  if (!R.glowPass) {
    // a glass column: brightest at its core, dissolving at its ends
    const g = ctx.createLinearGradient(0, top, 0, top + h);
    g.addColorStop(0, rgba('#0f2346', 0));
    g.addColorStop(0.18, rgba('#0f2346', 0.55 * a));
    g.addColorStop(0.82, rgba('#0f2346', 0.55 * a));
    g.addColorStop(1, rgba('#0f2346', 0));
    ctx.fillStyle = g;
    ctx.fillRect(cx - 44, top, 88, h);
    const e = ctx.createLinearGradient(0, top, 0, top + h);
    e.addColorStop(0, rgba(C.ice, 0));
    e.addColorStop(0.2, rgba(C.ice, 0.32 * a));
    e.addColorStop(0.8, rgba(C.ice, 0.32 * a));
    e.addColorStop(1, rgba(C.ice, 0));
    ctx.fillStyle = e;
    ctx.fillRect(cx - 45, top, 1, h);
    ctx.fillRect(cx + 44, top, 1, h);
  }
  // the package's name heads the column while the camera is out, and gives
  // way, as the headline does, while it is in
  const hk = a * k * (1 - scrimAt(camera(t)));
  text(R, '@supernovae-st/nika', cx, EDITOR.y + 30, { f: 'MM 500', size: 22, color: C.ice, alpha: hk, align: 'center', glow: 0.35 });
  text(R, `THE PACKAGE · ${PKG}`, cx, EDITOR.y + 56, { f: 'MGW 500', size: 10.5, tracking: 3, color: C.dim, alpha: hk, align: 'center' });
  text(R, BUNDLED ? 'STARTS ITS BUNDLED ENGINE' : 'STARTS THE ENGINE NIKA_BIN NAMES', cx, EDITOR.y + 76, { f: 'MGW 500', size: 10.5, tracking: 3, color: C.dim, alpha: hk * 0.85, align: 'center' });
}

// the moment a packet is at fraction f of its lane
const laneTime = (ln, f) => {
  let lo = ln.t0, hi = ln.t1;
  for (let i = 0; i < 24; i++) {
    const mid = (lo + hi) / 2;
    if (laneAt(ln, mid) < f) lo = mid; else hi = mid;
  }
  return hi;
};

function laneDraw(R, t, a, ln) {
  const q = laneAt(ln, t);
  if (q <= 0) return;
  // the path behind the packet stays lit
  poly(R, ln.pts, { color: ln.color, w: 1.8, alpha: a * 0.6, glow: 0.3 }, q);
  if (q < 1) {
    // a comet, brighter towards its head
    for (let s = 0; s < 4; s++) poly(R, ln.pts, { color: ln.color, w: 2.4 + s * 0.7, alpha: a * (0.18 + s * 0.24), glow: 0.9 }, q, Math.max(0, q - 0.3 + s * 0.075));
  }
  circle(R, ln.from[0], ln.from[1], 3.4, { fill: ln.color, alpha: a, glow: 0.8 });
  if (q >= 1) circle(R, ln.to[0], ln.to[1], 3.4, { fill: ln.color, alpha: a, glow: 0.8 });
  // where it crosses the column's core: a gate, a flash, and its name
  const [mx, my] = along(ln.path, 0.5);
  const tm = laneTime(ln, 0.5);
  const ck = smooth(tm, tm + 0.15, t);
  if (ck > 0) {
    circle(R, mx, my, 5, { color: ln.color, w: 1.4, alpha: a * ck, fill: C.bg0, fillAlpha: 1, glow: 0.6 });
    const flash = 1 - smooth(tm, tm + 0.7, t);
    if (flash > 0) light(R, mx, my, 100, ln.color, 0.5 * a * flash, 1);
    const lk = smooth(laneTime(ln, 0.8), laneTime(ln, 0.8) + 0.3, t);
    const lbl = laneLabel(ln);
    text(R, ln.label, lbl.x, lbl.y, { ...LANE_LABEL, color: ln.color, alpha: a * lk, align: 'center', glow: 0.3 });
  }
}

function packet(R, t, a, ln) {
  if (!ln.packet || t < ln.t0 || t > ln.t1 + 0.3) return;
  const [x, y] = along(ln.trip, tripAt(ln, t));
  const t0 = ln.t0 + (ln.appear ?? 0);
  const fade = smooth(t0, t0 + 0.15, t) * (1 - smooth(ln.t1 - 0.05, ln.t1 + 0.25, t));
  const st = { f: 'MM 500', size: 16 };
  const w = measure(ln.packet, st) + 28;
  light(R, x, y, 80, ln.color, 0.35 * a * fade, 1);
  rrect(R, x - w / 2, y - 16, w, 32, 16, { color: ln.color, w: 1.5, alpha: a * fade, fill: '#06101f', fillAlpha: 0.96, glow: 0.7 });
  text(R, ln.packet, x, y + 6, { ...st, color: C.ink, alpha: a * fade, align: 'center' });
}

// The receipt: a ticket carrying the head of the run's journal. It rides
// the traceVerify() lane into the engine and docks under the journal, its
// head in the same column as the head the journal ends on.
function ticket(R, t, a) {
  const ln = LANE.receipt;
  if (t < ln.t0) return;
  const dock = [TICKET.x + TICKET.w / 2, TICKET.y + TICKET.h / 2];
  const [lx, ly] = along(ln.trip, tripAt(ln, t));
  const d = E.snap(seg(t, ln.t1 - 0.12, ln.t1 + 0.4));
  const x = lerp(lx, dock[0], d), y = lerp(ly, dock[1], d);
  const fade = smooth(ln.t0, ln.t0 + 0.15, t);
  // it lifts off the code token, growing to its size
  const s = lerp(0.6, 1, E.snap(seg(t, ln.t0, ln.t0 + 0.4)));
  const m = smooth(T.match, T.match + 0.3, t);
  const col = m > 0 ? C.teal : C.gold;
  if (d < 1) light(R, x, y, 90, C.gold, 0.35 * a * fade, 1);
  const ctx = R.ctx;
  ctx.save();
  ctx.translate(x, y);
  ctx.scale(s, s);
  const x0 = -TICKET.w / 2, y0 = -TICKET.h / 2;
  rrect(R, x0, y0, TICKET.w, TICKET.h, 8, { color: col, w: 1.5, alpha: a * fade, fill: '#0c1322', fillAlpha: 0.97, glow: 0.45 + m * 0.35 });
  // a ticket's notches
  for (const nx of [x0, x0 + TICKET.w]) circle(R, nx, 0, 6, { fill: C.bg0, alpha: a * fade, color: col, w: 1.2 });
  text(R, 'result.receipt', x0 + 16, y0 + 24, { f: 'MM 500', size: 13, color: C.gold, alpha: a * fade });
  text(R, short(HEAD), x0 + 16, y0 + 48, { f: 'MM 500', size: 14, color: m > 0 ? C.teal : C.ink, alpha: a * fade, glow: m * 0.5 });
  ctx.restore();
}

// ── the engine ─────────────────────────────────────────────────────────
function station(R, t, a, y, label, t0, t1, lx = EX + 30) {
  const on = smooth(t0 - 0.2, t0, t), done = t >= t1;
  const iconX = lx - 22, iconY = y - 5;
  if (done) {
    circle(R, iconX, iconY, 9, { color: C.teal, w: 1.4, alpha: a, fill: C.teal, fillAlpha: 0.12, glow: 0.5 });
    check(R, iconX, iconY, 10, E.snap(seg(t, t1, t1 + 0.3)), { color: C.teal, w: 2, alpha: a, glow: 0.7 });
  } else {
    circle(R, iconX, iconY, 9, { color: C.faint, w: 1.2, alpha: a });
    if (on > 0) arc(R, iconX, iconY, 9, (t - t0) * 7, (t - t0) * 7 + Math.PI * 1.3, { color: C.ice, w: 1.8, alpha: a * on, glow: 0.7 });
  }
  text(R, label, lx, y, { ...STATION, color: done ? C.teal : on > 0 ? C.ink : C.dim, alpha: a });
}

function engine(R, t, a) {
  const k = E.snap(seg(t, T.win + 0.1, T.win + 0.6));
  const verified = smooth(T.match, T.match + 0.3, t);
  panel(R, ENGINE, {
    title: BUNDLED ? `nika ${ENGINE_VER} · a child process` : `nika ${ENGINE_VER} · via NIKA_BIN`, alpha: a, k,
    badge: t > T.match ? { label: 'VERIFIED', color: C.teal, alpha: verified } : t > T.ready ? { label: 'RUN READY', color: C.teal, alpha: smooth(T.ready, T.ready + 0.3, t) } : null,
  });
  if (k < 1) return;
  // the file arrives: the engine lights up to take it
  const arrive = smooth(T.p1e - 0.1, T.p1e, t) * (1 - smooth(T.p1e + 0.1, T.p1e + 0.9, t));
  if (arrive > 0) rrect(R, ENGINE.x, ENGINE.y, ENGINE.w, ENGINE.h, 14, { color: C.ice, w: 1.4, alpha: a * arrive * 0.55, glow: 0.45 });

  // CHECK: the file is audited before anything runs
  station(R, t, a, ST.check, 'CHECK · BEFORE ANYTHING RUNS', T.scan, T.ready);
  const fk = smooth(T.p1e - 0.05, T.p1e + 0.2, t);
  if (fk > 0) {
    const st = { f: 'MM 500', size: 16 };
    const w = measure('hello.nika', st) + 24;
    const cy = ST.check + 26;
    rrect(R, EX + 30, cy - 15, w, 30, 8, { color: C.ice, w: 1.2, alpha: a * fk, fill: C.ice, fillAlpha: 0.06 });
    text(R, 'hello.nika', EX + 42, cy + 6, { ...st, color: C.ink, alpha: a * fk });
    const s = seg(t, T.scan, T.ready);
    if (s > 0 && s < 1) {
      const sx = EX + 30 + w * E.inOutSine(s);
      line(R, sx, cy - 19, sx, cy + 19, { color: C.ice, w: 2, alpha: a, glow: 1 });
      light(R, sx, cy, 44, C.ice, 0.3 * a, 1);
    }
    const rk = smooth(T.ready, T.ready + 0.25, t);
    if (rk > 0) mono(R, [{ s: READY, c: C.teal, glow: 0.4 }], EX + 30 + w + 22, cy + 8, { alpha: a * rk, st: { f: 'MM 500', size: 22 } });
  }

  // RUN: the one task, on the rehearsal model
  station(R, t, a, ST.run, 'RUN', T.task, T.taskDone);
  const rk = smooth(T.task - 0.1, T.task + 0.2, t);
  if (rk > 0) {
    const m = PLAN.match(/^(\S+) \((.*)\)$/u);
    mono(R, m ? [{ s: m[1], c: C.ink }, { s: ` (${m[2]})`, c: C.mist }] : [{ s: PLAN, c: C.ink }], EX + 30, ST.run + 32, { alpha: a * rk, st: { f: 'MM 500', size: 22 } });
    pill(R, ENGINE.x + ENGINE.w - 24, ST.run - 5, 'REHEARSAL · NO KEY · NO NETWORK', C.amber, a * smooth(T.task + 0.2, T.task + 0.5, t), 'right');
  }

  // RECORD: the journal, each line carrying the hash of the one before it
  station(R, t, a, ST.record, `RECORD · ${EVENTS} EVENTS, HASH-CHAINED`, T.events[0], T.events[EVENTS - 1]);
  journal.forEach((j, i) => {
    const t0 = T.events[i];
    const e = smooth(t0, t0 + 0.15, t);
    if (e <= 0) return;
    const y = jRowY(i), dx = 14 * (1 - E.snap(seg(t, t0, t0 + 0.3)));
    const tw = T.walk + i * T.walkEvery, walked = t >= tw;
    const sealed = j.kind === 'run_sealed';
    const col = walked ? C.teal : sealed ? C.gold : C.ice;
    if (i > 0) {
      const lk = E.snap(seg(t, t0, t0 + 0.2));
      line(R, J.dot, y - 24 + 5, J.dot, y - 24 + 5 + 14 * lk, { color: walked ? C.teal : C.dim, w: 1.4, alpha: a * e, glow: walked ? 0.5 : 0 });
    }
    circle(R, J.dot, y - 5, sealed ? 4.5 : 3.5, { fill: col, alpha: a * e, glow: 0.6 });
    if (sealed) {
      // sealed with the run key: a ring, and a pulse as it lands
      circle(R, J.dot, y - 5, 8.5, { color: C.gold, w: 1.2, alpha: a * e, glow: 0.5 });
      const pu = seg(t, t0, t0 + 0.7);
      if (pu > 0 && pu < 1) circle(R, J.dot, y - 5, 8.5 + 26 * E.outCubic(pu), { color: C.gold, w: 1.4, alpha: a * (1 - pu), glow: 1 });
    }
    const hm = i === EVENTS - 1 ? smooth(T.match, T.match + 0.3, t) : 0;
    text(R, short(j.hash), J.hash + dx, y, { f: 'MM 500', size: 14, color: hm > 0 ? C.teal : walked ? C.mist : C.ink, alpha: a * e, glow: hm * 0.5 });
    text(R, j.kind, J.kind + dx, y, { f: 'MM 400', size: 14, color: sealed ? C.gold : C.mist, alpha: a * e });
    if (walked) check(R, J.tick, y - 5, 10, E.snap(seg(t, tw, tw + 0.2)), { color: C.teal, w: 1.8, alpha: a, glow: 0.6 });
  });
  // the verifier walks the chain
  const wEnd = T.walk + EVENTS * T.walkEvery;
  if (t > T.walk - 0.05 && t < wEnd + 0.15) {
    const yy = lerp(jRowY(0) - 18, jRowY(EVENTS - 1) + 6, clamp((t - T.walk) / (wEnd - T.walk)));
    line(R, EX - 6, yy, ENGINE.x + ENGINE.w - 18, yy, { color: C.teal, w: 1.5, alpha: a * 0.9, glow: 1 });
    light(R, EX + 40, yy, 60, C.teal, 0.25 * a, 1);
  }

  // VERIFY: the verifier's own words, beside the receipt it was handed
  station(R, t, a, ST.verify, 'VERIFY THE RECEIPT', T.walk, T.match, VX);
  VERIFY_ROWS.forEach((row, i) => {
    const vk = smooth(i ? T.sealed : T.ok, (i ? T.sealed : T.ok) + 0.25, t);
    if (vk > 0) text(R, row, VX, ST.verify + 28 + i * 24, { f: 'MM 400', size: 15, color: C.teal, alpha: a * vk });
  });
  // the receipt's head is the head the journal ends on
  const mk = E.snap(seg(t, T.match, T.match + 0.4));
  if (mk > 0) {
    const x = J.hash + 44, y0 = jRowY(EVENTS - 1) + 6, y1 = TICKET.y - 2;
    line(R, x, y0, x, lerp(y0, y1, mk), { color: C.teal, w: 2, alpha: a, glow: 0.9 });
    const cy = (y0 + y1) / 2;
    if (mk > 0.5) {
      circle(R, x, cy, 9, { color: C.teal, w: 1.4, alpha: a, fill: C.bg0, fillAlpha: 1, glow: 0.6 });
      line(R, x - 4, cy - 2.5, x + 4, cy - 2.5, { color: C.teal, w: 1.6, alpha: a });
      line(R, x - 4, cy + 2.5, x + 4, cy + 2.5, { color: C.teal, w: 1.6, alpha: a });
    }
    const pu = seg(t, T.match, T.match + 0.8);
    if (pu < 1) circle(R, x, cy, 10 + 40 * E.outCubic(pu), { color: C.teal, w: 1.5, alpha: a * (1 - pu), glow: 1 });
  }
}

// After the verdict, a light runs the whole loop once: call, answer,
// receipt, proof.
function lap(R, t, a) {
  LANES.forEach((ln, i) => {
    const s = seg(t, T.lap + i * T.lapEach, T.lap + (i + 1) * T.lapEach + 0.1);
    if (s <= 0 || s >= 1) return;
    poly(R, ln.pts, { color: C.teal, w: 3, alpha: a * 0.9 * Math.sin(Math.PI * s), glow: 1 }, E.inOutSine(s), Math.max(0, E.inOutSine(s) - 0.35));
  });
}

export function draw(R, t) {
  const a = envelope(t);
  // zone titles
  const zk = smooth(T.win, T.win + 0.4, t) * a;
  if (zk > 0) {
    const Z = { f: 'Geist 600', size: 24, tracking: -0.4 };
    const SUB = { f: 'MM 400', size: 14, color: C.dim, alpha: zk };
    text(R, 'Your app', EDITOR.x, EDITOR.y - 22, { ...Z, color: C.ink, alpha: zk });
    text(R, `node ${versions.node} · typescript`, EDITOR.x + measure('Your app', Z) + 16, EDITOR.y - 22, SUB);
    text(R, 'The Nika engine', ENGINE.x, EDITOR.y - 22, { ...Z, color: C.ink, alpha: zk });
    text(R, BUNDLED ? 'bundled with the package' : 'from PATH, through NIKA_BIN', ENGINE.x + measure('The Nika engine', Z) + 16, EDITOR.y - 22, SUB);
  }
  bridge(R, t, a);
  editor(R, t, a);
  const reh = smooth(T.greet + 0.2, T.greet + 0.5, t);
  terminal(R, t, TERM, [
    { t: T.cmd, cmd: versions.command, dur: 0.55 },
    { t: T.greet, out: [GREETING], marks: [{ re: /^mock\(echo\) ·/u, c: C.amber, glow: 0.3 }] },
    { t: T.verdict, out: [VERDICT], marks: [{ re: /^receipt verified$/u, c: C.teal, glow: 0.8 }] },
  ], {
    title: 'terminal', alpha: a, k: E.snap(seg(t, T.win + 0.05, T.win + 0.55)), st: TERM_ST, lh: TERM_LH,
    badge: reh > 0 ? { label: 'MOCK/ECHO · A REHEARSAL', color: C.amber, alpha: reh } : null,
  });
  // the verdict row lights, and a check is drawn after it
  const vk = smooth(T.verdict, T.verdict + 0.3, t);
  if (vk > 0) {
    const y = termRowY(2);
    const bloom = seg(t, T.verdict, T.verdict + 1.1);
    if (bloom < 1) light(R, TERM.x + 24 + VERDICT.length * cw(TERM_ST) / 2, y - 8, 230, C.teal, 0.28 * a * Math.sin(Math.PI * Math.sqrt(bloom)), 1);
    rect(R, TERM.x + 6, y - TERM_LH + 10, TERM.w - 12, TERM_LH, { fill: C.teal, alpha: a * 0.07 * vk });
    rect(R, TERM.x + 6, y - TERM_LH + 10, 3, TERM_LH, { fill: C.teal, alpha: a * vk, glow: 0.8 });
    check(R, TERM.x + 24 + (VERDICT.length + 1.4) * cw(TERM_ST), y - 8, 22, E.snap(seg(t, T.verdict + 0.1, T.verdict + 0.45)), { color: C.teal, w: 2.6, alpha: a, glow: 0.9 });
  }
  engine(R, t, a);
  for (const ln of LANES) laneDraw(R, t, a, ln);
  lap(R, t, a);
  for (const ln of LANES) packet(R, t, a, ln);
  ticket(R, t, a);
}
