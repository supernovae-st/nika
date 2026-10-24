// sdk-doors · "Your app asks. The engine decides. The receipt proves it."
// The TypeScript SDK (@supernovae-st/nika) on one screen: one client, two
// doors. new Nika() starts the engine the package bundles, as a child
// process; new Nika({ url, token }) reaches a nika serve with the same
// methods and other options. Two lines of the quick start take the server
// door's: line 3 its URL and token (plain HTTP to loopback only with
// allowInsecureHttp), line 6 the idempotencyKey a server run requires where
// a local run takes maxCostUsd, which a server run refuses. The engine owns
// every decision: it checks a file before it admits it (a refused file makes
// run() throw NikaOperationError, and no run exists), runs it, streams its
// events in one vocabulary (six frames from the local engine, two sequenced
// frames from the server) and keeps a hash-chained record sealed by
// run_sealed. The receipt comes back to the app, and traceVerify() has the
// engine re-read the trace: verified locally, sealed from the server. One
// changed byte breaks the chain.
//
// Everything on screen that is program, output, verdict, hash, code or
// version is read from the capture (scripts/media/capture/sdk-doors.mjs →
// media/raw/sdk-doors-*), taken from the published npm package, the engine it
// bundles and a nika serve the capture started: the README's demo.mts with
// its real line numbers (the capture also ran it and kept what it printed),
// the server door's lines 3 and 6 as the capture's own remote driver writes
// and ran them (with the package's refusal of the quick start's own options
// over HTTP), the check reports, the event kinds in their order, the
// journal's kinds and hashes, the receipt's head, length and seal, both
// verifiers' verdicts, the refusal and the tampered chain's first line. The
// clip refuses to render if the capture stops saying any of it. A line wider
// than the code card wraps at a space, as an editor with word wrap shows it:
// every character stays.
// What is illustration: the cards, the column, the lanes and every packet
// that travels them, and the pacing (a run of hello.nika takes milliseconds;
// the capture ran the refused file before the quick start, as here). The
// tamper beat re-enacts on the record what the capture did to its scratch
// trace: one byte of line 4 changed, then traceVerify() on the kept receipt.
import { C, E, seg, smooth, clamp, lerp, readRepo, repoLines, cw, mono, panel, pill, frame } from './kit.mjs';
import { text, rrect, rect, line, circle, arc, poly, check, cross, measure, bezierPts, light } from '../src/engine/render.mjs';
import { rgba } from '../src/engine/core.mjs';

export const meta = { duration: 25, poster: 24.3 };
// In from the empty stage, and no fade out: a faded frame repaints every
// pixel of a GIF frame (one cost this clip about 240 KB), so the clip ends on
// its poster and the loop cuts back to the empty stage it starts from.
const envelope = t => smooth(0, 0.12, t);

// ── the capture ─────────────────────────────────────────────────────────
const RAW = 'media/raw/sdk-doors';
const demo = repoLines(`${RAW}-demo.mts`);
const hello = repoLines(`${RAW}-hello.nika`);
const checkHello = repoLines(`${RAW}-check.txt`);
const checkRed = repoLines(`${RAW}-check-red.txt`);
const versions = JSON.parse(readRepo(`${RAW}-versions.json`));
const local = JSON.parse(readRepo(`${RAW}-local.json`));
const remote = JSON.parse(readRepo(`${RAW}-remote.json`));
// the server door's lines, as the capture's remote driver writes them
const remoteDriver = repoLines('scripts/media/capture/sdk-doors/remote.mts');

const fail = why => { throw new Error(`sdk-doors capture: ${why}`); };
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const PKG = versions.package.version;
const ENGINE_VER = versions.engine.engineVersion;
const BUILD = versions.engine.buildSha;
const PAYLOAD = versions.engine.payload;
if (versions.package.name !== '@supernovae-st/nika') fail('the capture is not of @supernovae-st/nika');
if (PKG !== ENGINE_VER || remote.health.engineVersion !== ENGINE_VER) fail(`package ${PKG}, bundled engine ${ENGINE_VER}, served engine ${remote.health.engineVersion}: they must be one version`);
if (versions.engine.version_line !== `nika ${ENGINE_VER} (${BUILD})` || remote.health.buildSha !== BUILD) fail('the two doors do not name the same engine build');
if (versions.engine.machineProtocolVersion !== 2 || remote.health.machineProtocolVersion !== 2) fail('machine protocol is not 2 on both doors');
if (versions.engine.source !== 'bundled' || !PAYLOAD.sha256_matches_integrity) fail('the engine is not the bundled payload its INTEGRITY.json names');
if (PAYLOAD.version !== PKG || PAYLOAD.release.tag !== `v${ENGINE_VER}` || !PAYLOAD.release.commit.startsWith(BUILD)) fail('the payload does not come from this release');
if (versions.package.npm_added !== 2 || !versions.package.installed.includes(PAYLOAD.name.split('/')[1])) fail('npm no longer installs the client and its payload, two packages');
const PROMPT = hello.map(l => l.match(/^\s*prompt: "(.*)"$/)?.[1]).find(Boolean) ?? fail('hello.nika has no prompt');
const GREETING = `mock(echo) · ${PROMPT}`;
if (local.result.greeting !== GREETING || remote.result.greeting !== GREETING) fail('a door no longer returns the greeting');
// the quick start itself, run as the README says: the greeting, then the proof
if (!same(local.demo?.stdout, [GREETING, 'receipt verified'])) fail('node demo.mts no longer prints the greeting and receipt verified');
if (local.transportKind !== 'native-process' || remote.transportKind !== 'http') fail('the transports are not native-process and http');
// admission: the quick start is clean, the red file is refused before any run
if (!local.check.clean || local.check.exitCode !== 0 || !remote.check.clean) fail('hello.nika is not clean on both doors');
const READY = checkHello.some(l => /^ layers · .*run ready ✔$/u.test(l)) ? 'run ready ✔' : fail('hello.nika is not run ready');
const REFUSAL = local.refused;
if (!REFUSAL.refused || REFUSAL.name !== 'NikaOperationError' || REFUSAL.operation !== 'run' || REFUSAL.code !== 'NIKA-SEC-004') fail('run() no longer refuses the red file with NIKA-SEC-004');
if (REFUSAL.tracesWritten !== 0) fail('the refused file left a trace: a run existed');
if (local.checkRed.clean !== false || local.checkRed.errors[0]?.code !== REFUSAL.code) fail('check() no longer shows the refusal run() throws');
// events: one vocabulary, the transport decides how many
const LOCAL_KINDS = ['run.started', 'task.scheduled', 'task.started', 'task.completed', 'engine.event', 'run.settled'];
if (!same(local.events.map(e => e.kind), LOCAL_KINDS)) fail(`the local kinds are ${local.events.map(e => e.kind).join(', ')}`);
if (!same(remote.events.map(e => [e.sequence, e.kind]), [[1, 'run.started'], [2, 'run.settled']])) fail('the server no longer streams #1 run.started and #2 run.settled');
if (local.events[5].status !== 'succeeded' || remote.events[1].status !== 'succeeded') fail('a run did not settle succeeded');
if (local.result.cause !== 'normal' || local.result.tasks?.ok !== 1 || local.result.tasks?.total !== 1) fail('the local settlement is not normal, 1/1 tasks ok');
// the record: six lines, each chaining to the one before, sealed last
const journal = local.journal;
if (!local.receipt.sealed || local.receipt.chain_len !== 6 || journal.length !== 6) fail('the receipt is not sealed over six lines');
journal.forEach((j, i) => { if (i && j.chain !== journal[i - 1].hash) fail(`journal line ${i + 1} does not chain`); });
if (journal[5].kind !== 'run_sealed' || journal[5].hash !== local.receipt.chain_head) fail('the receipt head is not the sealed last line');
const OK_LINE = local.verify.first;
if (!local.verify.verified || local.verify.verdict !== 'verified' || OK_LINE !== `OK — 6 events · chain intact · head ${local.receipt.chain_head}`) fail('the local verifier no longer says verified, chain intact');
if (!remote.verify.verified || remote.verify.verdict !== 'sealed') fail('the server no longer answers sealed');
if (!remote.retry.sameJob) fail('a retry with the same idempotencyKey no longer returns the same job');
// one changed byte: the verifier stops at the next line
const BROKEN = local.tamper.first.match(/^BROKEN at line (\d+) — recorded chain ([0-9a-f]{16}) · computed ([0-9a-f]{16})$/u) ?? fail('the tampered trace no longer reads BROKEN');
const [, BROKEN_LINE, RECORDED, COMPUTED] = BROKEN;
const CHANGED = local.tamper.line - 1; // the journal row whose byte changed
if (+BROKEN_LINE !== local.tamper.line + 1 || !journal[CHANGED].hash.startsWith(RECORDED)) fail('BROKEN does not name the line after the changed one');
if (local.tamper.verified || local.tamper.verdict !== 'invalid') fail('traceVerify() no longer refuses the tampered trace');
const short = h => `${h.slice(0, 8)}…`;

// the key lines of demo.mts, by what they say (their real numbers are kept)
const lineOf = re => (demo.findIndex(l => re.test(l)) + 1) || fail(`demo.mts lost ${re}`);
const L = {
  imp: lineOf(/^import \{ Nika, isNikaRunSucceeded \} from '@supernovae-st\/nika';$/),
  client: lineOf(/^const nika = new Nika\(\{ cwd: process\.cwd\(\) \}\);$/),
  run: lineOf(/^const run = await nika\.run<\{ greeting: string \}>\('hello\.nika'/),
  result: lineOf(/^const result = await run\.result\(\);$/),
  log: lineOf(/^console\.log\(result\.outputs\?\.greeting\);$/),
  verify: lineOf(/^const proof = await nika\.traceVerify\(result\.receipt\);$/),
  verdict: lineOf(/^console\.log\(proof\.verified \? 'receipt verified'/),
};
const ROWS = [L.imp, L.client, L.run, L.result, L.log, L.verify, L.verdict];
// Lines 3 and 6, local and remote: each keeps one shared frame and only its
// options differ, the constructor's, then run()'s. The server door's lines
// are the remote driver's, which the capture ran against nika serve.
const driverLine = (re, what) => (remoteDriver.find(l => re.test(l)) ?? fail(`the remote driver lost its ${what}`)).replace(/\s*\/\/.*$/, '');
const EDITS = [
  { ln: L.client, lag: 0, head: 'const nika = new Nika({ ', tail: ' });', remote: driverLine(/^const nika = new Nika\(\{ url, token, allowInsecureHttp: true \}\);/, 'constructor') },
  { ln: L.run, lag: 0.15, head: "const run = await nika.run<{ greeting: string }>('hello.nika', { ", tail: ' });', remote: driverLine(/^const run = await nika\.run<\{ greeting: string \}>\('hello\.nika', \{ idempotencyKey \}\);$/, 'run line') },
].map(e => {
  const local = demo[e.ln - 1];
  if (![local, e.remote].every(s => s.startsWith(e.head) && s.endsWith(e.tail))) fail(`line ${e.ln}: the two doors no longer share its frame`);
  return { ...e, local, from: local.slice(e.head.length, -e.tail.length), to: e.remote.slice(e.head.length, -e.tail.length) };
});
// why they change: over HTTP the package refuses the quick start's own
// options before any request (the capture asked it), and the server door's
// lines carry what each refusal names
const REFUSED = remote.refusals ?? fail('the capture no longer asks the server door to refuse the quick start');
for (const [k, name, word] of [['plainHttp', 'NikaConfigurationError', 'allowInsecureHttp'], ['maxCostUsd', 'NikaCompatibilityError', 'maxCostUsd'], ['noKey', 'NikaConfigurationError', 'idempotencyKey']]) {
  if (!REFUSED[k]?.refused || REFUSED[k].name !== name || !REFUSED[k].message.includes(word)) fail(`the server door no longer refuses ${k} with ${name}`);
}
if (!EDITS[0].to.includes('allowInsecureHttp') || !EDITS[1].from.includes('maxCostUsd') || !EDITS[1].to.includes('idempotencyKey')) fail('lines 3 and 6 no longer differ by what the server door refuses');

// the engine's own report lines, verbatim: the file, the plan, the gates
// that decide here, and its last word
const reportOf = (lines, file, permits, tail) => [
  lines[0] === `nika check · ${file}` ? lines[0] : fail(`the ${file} report has no header`),
  lines.find(l => /^ ✔ PLAN /u.test(l)) ?? fail(`the ${file} report has no plan`),
  lines.find(l => /^ ✔ COST /u.test(l)) ?? fail(`the ${file} report has no cost`),
  lines.find(l => permits.test(l)) ?? fail(`the ${file} report has no permits verdict`),
  lines.find(l => /^ ✔ TRIFECTA /u.test(l)) ?? fail(`the ${file} report has no trifecta`),
  lines.find(l => tail.test(l)) ?? fail(`the ${file} report has no last word`),
];
const REPORT_RED = reportOf(checkRed, 'list-files.nika', new RegExp(`^ ✖ PERMITS +\\[${REFUSAL.code} · exec\\]`, 'u'), /^ ✖ findings above · /u);
const REPORT_HELLO = reportOf(checkHello, 'hello.nika', /^ ✔ PERMITS /u, /^ layers · .*run ready ✔$/u);
const PLAN = (checkHello.find(l => /^\s+wave 1 \S+ \(.*\)$/u.test(l)) ?? fail('no plan line')).trim().replace(/^wave 1 /, '');

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
    if (prev === '.' || prev === '?.' || ([':', ',', '}'].includes(next) && braces > 0)) return { ...tk, kind: 'prop' };
    return { ...tk, kind: 'name' };
  });
}
// the Nika blue canon: no violet
const TS = { keyword: '#86A8FF', type: C.ice, call: C.cyan, prop: '#B3C5DD', name: C.ink, string: '#E8C79B', number: '#E8C79B', punct: '#7C90B0', comment: C.dim, ws: C.ink };
const spansOf = src => classify(lex(src)).map(tk => ({ s: tk.s, c: TS[tk.kind] }));
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
const CODE_ST = { f: 'MM 400', size: 14 };
const ADV = cw(CODE_ST);
const CODE = { x: 64, y: 286, w: 712, h: 308 };
const CONSOLE = { x: 64, y: 618, w: 712, h: 362 };
const LOCAL = { x: 1200, y: 286, w: 656, h: 426 };
const REMOTE = { x: 1200, y: 762, w: 656, h: 218 };
const GUTTER = 56;
const codeX = CODE.x + GUTTER;
const CODE_MAX = Math.floor((CODE.w - GUTTER - 20) / ADV);
// A line wider than the card wraps at a space, its continuation indented, as
// an editor with word wrap shows it: each row is the characters [a, b) of
// the line, and only where it breaks changes.
const WRAP_INDENT = 2;
const wrapRanges = s => {
  const out = [];
  let start = 0, width = CODE_MAX;
  while (s.length - start > width) {
    let cut = s.lastIndexOf(' ', start + width);
    if (cut <= start) cut = start + width;
    out.push([start, cut]);
    start = cut;
    while (s[start] === ' ') start++;
    width = CODE_MAX - WRAP_INDENT;
  }
  out.push([start, s.length]);
  return out;
};
// An edited line keeps the rows its local form wraps to while its options
// change, so the options stay in place on its last row.
const editOf = ln => EDITS.find(e => e.ln === ln);
const rangesOf = (ln, src) => {
  const rows = wrapRanges(demo[ln - 1]);
  if (!editOf(ln)) return rows;
  return rows.map(([a, b], r) => [a, r === rows.length - 1 ? src.length : b]);
};
for (const e of EDITS) {
  const [lr, rr] = [wrapRanges(e.local), wrapRanges(e.remote)];
  if (!same(lr.map(r => r[0]), rr.map(r => r[0]))) fail(`line ${e.ln}: the two doors break it on different rows`);
  if (e.head.length < lr[lr.length - 1][0]) fail(`line ${e.ln}: its options are not on its last row`);
}
// where an edited line's options sit: their row and column
const argAt = (e, ranges) => {
  const r = ranges.findLastIndex(([a]) => a <= e.head.length);
  return { row: r, col: (r ? WRAP_INDENT : 0) + e.head.length - ranges[r][0] };
};
const ROW_H = 27;
const rowY = i => CODE.y + 44 + 28 + i * ROW_H;
// the first row of each line shown, and how many rows they take
const ROW_START = new Map();
let ROW_COUNT = 0;
for (const ln of ROWS) {
  ROW_START.set(ln, ROW_COUNT);
  ROW_COUNT += wrapRanges(demo[ln - 1]).length;
}
if (rowY(ROW_COUNT - 1) + 7 > CODE.y + CODE.h - 12) fail('the quick start no longer fits its card');
// the package between the app and the doors: a glass column, one junction
const COL = { cx: (CODE.x + CODE.w + LOCAL.x) / 2, top: 300, bottom: 980, half: 44 };
const J = { x: COL.cx, y: (CODE.y + CODE.h + CONSOLE.y) / 2 };
// each door's port on its card
const PORT = { local: [LOCAL.x, 472], remote: [REMOTE.x, 848] };
const LX = LOCAL.x + 52; // a door card's content column
const ST = { check: 388, hello: 420, red: 455, run: 498, runText: 532, record: 568 };
const RST = { check: 870, run: 908, verify: 946 };
const jRowY = k => 596 + k * 20;
const J_COL = { dot: LOCAL.x + 32, hash: LX, kind: LX + 96, tick: LX + 290 };
const V = { x: LOCAL.x + 380, label: ST.record, ticket: 580, verdict: 676, words: 702 };
const TICKET = { w: 238, h: 62 };
const CHIP_ST = { f: 'MM 500', size: 16 };
const CHIP_HELLO_W = measure('hello.nika', CHIP_ST) + 24, CHIP_RED_W = measure('list-files.nika', CHIP_ST) + 24;
const VERDICT_X = LX + CHIP_RED_W + 18;
const BIG = { f: 'MM 500', size: 22 };
const SMALL = { f: 'MM 400', size: 13 };
const STATION = { f: 'MGW 500', size: 11.5, tracking: 2.5 };
// the console: what the app reads, both doors side by side
const EV_ST = { f: 'MM 500', size: 15 };
const EV_ADV = cw(EV_ST);
const evY = k => CONSOLE.y + 118 + k * 32;
const EVX = { local: CONSOLE.x + 24, detail: CONSOLE.x + 24 + 16 * EV_ADV, remote: CONSOLE.x + 436 };
const FOOT = { y1: CONSOLE.y + 316, y2: CONSOLE.y + 342 };

// ── time (seconds) ──────────────────────────────────────────────────────
const T = {
  win: 0.35, column: 0.6,
  type0: 1.0, typeEvery: 0.14, typeDur: 0.24,
  // two doors
  local: 2.3, localDraw: 2.75, payload: 3.2,
  m1: 3.6, remote: 3.95, remoteDraw: 4.4, remoteSub: 4.9, m2: 5.5,
  // admission: the refused file first, then the quick start
  red: 6.3, redAt: 6.85, redStamp: 7.35, redBack: 7.6, redCaught: 8.45,
  hello: 8.15, helloAt: 8.85, ready: 9.4,
  rcheck: 9.05, rcheckAt: 9.75,
  // the run and its events
  reportOut: 9.7, task: 9.95, ev0: 10.05, evEvery: 0.36, evTrip: 0.75,
  rrun: 10.2, rrunAt: 10.85, rev: [11.0, 12.15], counts: 13.0,
  // the record and the result
  record: 13.45, j0: 13.6, jEvery: 0.17, sealed: 14.5,
  result: 14.8, resultAt: 15.6, rresult: 15.2, rresultAt: 16.0,
  // the proof
  tvHl: 16.85, tv: 17.1, tvAt: 17.85, dock: 17.9, walk: 18.15, walkEvery: 0.1, ok: 18.8, verified: 19.0,
  rtv: 18.25, rtvAt: 19.0, proof: 19.35, proofAt: 20.1,
  tamper: 20.3, tamperEnd: 22.1,
  // settle
  footer: 22.45, lap: 22.65,
};
T.taskDone = T.ev0 + 3 * T.evEvery; // task.completed leaves the engine
T.jRow = k => T.j0 + k * T.jEvery;

// ── lanes ───────────────────────────────────────────────────────────────
// A polyline with its cumulative lengths, walked at constant speed.
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
const curve = (a, b, n = 40) => {
  const d = (b[0] - a[0]) * 0.45;
  return bezierPts(a, [a[0] + d, a[1]], [b[0] - d, b[1]], b, n);
};
const rev = pts => [...pts].reverse();
// the doors' branches: from the junction to each card's port
const BRANCH = { local: curve([J.x, J.y], PORT.local), remote: curve([J.x, J.y], PORT.remote) };
// the app side: from a point of the app, along its line to the card's edge,
// then into the junction
const EDGE = CODE.x + CODE.w;
const appSide = (x, y) => [[x, y], [EDGE, y], ...curve([EDGE, y], [COL.cx - COL.half, J.y]).slice(1), [J.x, J.y]];
const callTrip = (from, door) => measured([...appSide(...from), ...BRANCH[door].slice(1)]);
const answerTrip = (door, to) => measured([...rev(BRANCH[door]), ...rev(appSide(...to)).slice(1)]);

// a token of a code line: where it sits, for a packet to leave or land on
const token = (ln, s) => {
  const col = demo[ln - 1].indexOf(s);
  if (col < 0) fail(`line ${ln} lost ${s}`);
  if (col + s.length > wrapRanges(demo[ln - 1])[0][1]) fail(`line ${ln}: ${s} is not on its first row`);
  return { ln, col, len: s.length, x: codeX + (col + s.length / 2) * ADV, y: rowY(ROW_START.get(ln)) - 5 };
};
const TOK = {
  client: token(L.client, 'nika'),
  file: token(L.run, "'hello.nika'"),
  result: token(L.result, 'result'),
  receipt: token(L.verify, 'result.receipt'),
  proof: token(L.verify, 'proof'),
};
// inside a door: the station each packet reaches from the port
// (the light from the port reaches a chip's edge or a station's icon, so it
// never strikes through a word)
const STP = {
  hello: [LX - 4, ST.hello - 6],
  red: [LX - 4, ST.red - 6],
  run: [LX - 22, ST.run - 5],
  sealed: [J_COL.dot, jRowY(5) - 5],
  dock: [V.x + TICKET.w / 2, V.ticket + TICKET.h / 2],
  rcheck: [LX - 22, RST.check - 5],
  rrun: [LX - 22, RST.run - 5],
  rverify: [LX - 22, RST.verify - 5],
};
const remoteRow = seq => (seq === 1 ? 0 : 5); // aligned with the local row of the same kind
const evLand = (side, k) => [side === 'local' ? EVX.local + 70 : EVX.remote + 120, evY(k) - 5];

// Every packet: what it says, the door, where it starts or lands, when.
// kind: pill (a call or an answer) · chip (an event) · ticket (the receipt)
// · spark (no payload). A call ends at its door's port and a light runs on
// to the station inside; an answer starts with a light from the station.
const PACKETS = [
  { id: 'born', kind: 'spark', door: 'local', color: C.ice, t0: T.local, t1: T.localDraw, trip: measured(appSide(TOK.client.x, TOK.client.y)) },
  { id: 'bornR', kind: 'spark', door: 'remote', color: C.cyan, t0: T.remote, t1: T.remoteDraw, trip: measured(appSide(TOK.client.x, TOK.client.y)) },
  { id: 'red', label: 'list-files.nika', door: 'local', color: C.ice, t0: T.red, t1: T.redAt, trip: measured(BRANCH.local), into: STP.red, branch: 'run()' },
  { id: 'redBack', label: REFUSAL.code, door: 'local', color: C.red, t0: T.redBack, t1: T.redCaught, trip: answerTrip('local', evLand('local', 0)), from: STP.red, branch: 'throws', bad: true },
  { id: 'hello', label: 'hello.nika', door: 'local', color: C.ice, t0: T.hello, t1: T.helloAt, trip: callTrip([TOK.file.x, TOK.file.y], 'local'), into: STP.hello, branch: 'run()' },
  { id: 'rcheck', label: 'hello.nika', door: 'remote', color: C.cyan, t0: T.rcheck, t1: T.rcheckAt, trip: callTrip([TOK.file.x, TOK.file.y], 'remote'), into: STP.rcheck, branch: 'check()' },
  ...local.events.map((e, k) => ({
    id: `ev${k}`, label: e.kind, door: 'local', color: C.ice, kind: 'chip',
    t0: T.ev0 + k * T.evEvery, t1: T.ev0 + k * T.evEvery + T.evTrip,
    trip: answerTrip('local', evLand('local', k)), from: STP.run,
    branch: k === 0 ? 'run.events()' : null, until: T.ev0 + 5 * T.evEvery + 0.35,
  })),
  { id: 'rrun', label: 'hello.nika', door: 'remote', color: C.cyan, t0: T.rrun, t1: T.rrunAt, trip: callTrip([TOK.file.x, TOK.file.y], 'remote'), into: STP.rrun, branch: 'run()' },
  ...remote.events.map((e, k) => ({
    id: `rev${k}`, label: `#${e.sequence} ${e.kind}`, door: 'remote', color: C.cyan, kind: 'chip',
    t0: T.rev[k], t1: T.rev[k] + T.evTrip, trip: answerTrip('remote', evLand('remote', remoteRow(e.sequence))), from: STP.rrun,
    branch: k === 0 ? 'run.events()' : null, until: T.rev[1] + 0.35,
  })),
  { id: 'result', label: 'outputs + receipt', door: 'local', color: C.cyan, t0: T.result, t1: T.resultAt, trip: answerTrip('local', [TOK.result.x, TOK.result.y]), from: STP.sealed, branch: 'run.result()' },
  { id: 'rresult', label: 'outputs + receipt', door: 'remote', color: C.cyan, t0: T.rresult, t1: T.rresultAt, trip: answerTrip('remote', [TOK.result.x, TOK.result.y]), from: STP.rrun, branch: 'run.result()' },
  { id: 'tv', kind: 'ticket', door: 'local', color: C.gold, t0: T.tv, t1: T.tvAt, trip: callTrip([TOK.receipt.x, TOK.receipt.y], 'local'), into: STP.dock, branch: 'traceVerify()' },
  { id: 'rtv', label: 'result.receipt', door: 'remote', color: C.gold, t0: T.rtv, t1: T.rtvAt, trip: callTrip([TOK.receipt.x, TOK.receipt.y], 'remote'), into: STP.rverify, branch: 'traceVerify()' },
  { id: 'proof', label: 'verified: true', door: 'local', color: C.teal, t0: T.proof, t1: T.proofAt, trip: answerTrip('local', [TOK.proof.x, TOK.proof.y]), from: STP.dock, branch: 'proof' },
];
const P = Object.fromEntries(PACKETS.map(p => [p.id, p]));
const tripAt = (p, t) => E.inOutCubic(seg(t, p.t0, p.t1));
const landed = (id, t, d = 0.15) => smooth(P[id].t1 + 0.05, P[id].t1 + 0.05 + d, t);

// ── the plate: what is illustration, what was captured with what ───────
const PLATE_ST = { f: 'MGW 500', size: 10.5, tracking: 3 };
const PLATE = (() => {
  const options = [
    `cards, lanes and motion: illustration · captured: @supernovae-st/nika ${PKG}, its bundled engine ${ENGINE_VER} (${BUILD}) and nika serve · a mock/echo rehearsal`,
    `cards, lanes, motion: illustration · captured: @supernovae-st/nika ${PKG} + bundled engine ${ENGINE_VER} (${BUILD}) + nika serve · mock/echo rehearsal`,
    `lanes and motion: illustration · captured: @supernovae-st/nika ${PKG} + bundled engine ${ENGINE_VER} + nika serve · mock/echo`,
  ];
  return options.find(s => measure(s.toUpperCase(), PLATE_ST) <= 1680) ?? fail('no plate wording fits the frame');
})();

// The headline, word by word: each word rises into place, the accent last.
const HEADLINE = { x: 64, y: 142, st: { f: 'Geist 600', size: 58, tracking: -58 * 0.028 } };
const TITLE = (() => {
  const main = 'Your app asks. The engine decides. The receipt proves it.', accent = 'The receipt proves it.';
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
    const t0 = 0.2 + i * 0.06 + (hot ? 0.2 : 0);
    const k = E.snap(seg(t, t0, t0 + 0.55));
    if (k <= 0) return;
    text(R, w, x, HEADLINE.y + 18 * (1 - k), { ...HEADLINE.st, color: hot ? C.teal : C.ink, alpha: a * smooth(t0, t0 + 0.3, t), glow: hot ? 0.4 : 0.18 });
  });
}

// One caption at a time, under the headline: the old one leaves before the
// new one arrives.
const CAPTION = { x: 64, y: 206, st: { f: 'Geist 500', size: 24, tracking: -0.2 } };
const CAPTIONS = [
  [0.9, 3.3, 'One client. Two doors.'],
  [3.5, 6.2, 'Same methods, other options.'],
  [6.4, 9.95, 'Checked first: a refused file throws, and no run exists.'],
  [10.15, 13.85, 'One vocabulary for events. The transport decides how many.'],
  [14.05, 17.15, 'The engine keeps a hash-chained record and seals it.'],
  [17.35, 22.05, 'The engine re-reads the trace. One changed byte breaks it.'],
  [22.25, Infinity, 'Checked before it runs · typed when it finishes · verifiable afterwards.'],
];
for (const [, , s] of CAPTIONS) if (measure(s, CAPTION.st) > 1792) fail(`a caption is too wide: ${s}`);
function captions(R, t, a) {
  for (const [t0, t1, s] of CAPTIONS) {
    if (t < t0 || t > t1) continue;
    const k = E.snap(seg(t, t0, t0 + 0.45));
    const out = t1 === Infinity ? 1 : 1 - smooth(t1 - 0.22, t1, t);
    const al = a * smooth(t0, t0 + 0.25, t) * out;
    // the separators of the last caption are the accent
    const parts = s.split(' · ');
    let x = CAPTION.x;
    parts.forEach((part, i) => {
      text(R, part, x, CAPTION.y + 8 * (1 - k), { ...CAPTION.st, color: C.ink, alpha: al, glow: 0.12 });
      x += measure(part, CAPTION.st);
      if (i < parts.length - 1) {
        text(R, ' · ', x, CAPTION.y + 8 * (1 - k), { ...CAPTION.st, color: C.teal, alpha: al });
        x += measure(' · ', CAPTION.st);
      }
    });
  }
}

export function chrome(R, t) {
  const a = envelope(t);
  frame(R, t, { kicker: `the typescript sdk · @supernovae-st/nika ${PKG}`, plate: PLATE, alpha: a });
  title(R, t, a);
  captions(R, t, a);
  // where to get it, once the story is told
  const fk = a * smooth(T.footer, T.footer + 0.4, t);
  if (fk > 0) mono(R, [{ s: 'npm install @supernovae-st/nika', c: C.ice, glow: 0.3 }, { s: ' · ', c: C.dim }, { s: 'docs.nika.sh/sdk', c: C.mist }], 64, 1008, { alpha: fk, st: { f: 'MM 500', size: 16 } });
}

export function env() {
  return { bgGlow: 1, gridAlpha: 0.22, bgY: 600 };
}

// ── zone titles ─────────────────────────────────────────────────────────
const ZONE = { f: 'Geist 600', size: 24, tracking: -0.4 };
const ZONE_SUB = { f: 'MM 400', size: 14 };
const DOOR = { f: 'MM 500', size: 22 };
function zones(R, t, a) {
  const zk = smooth(T.win, T.win + 0.4, t) * a;
  if (zk <= 0) return;
  text(R, 'Your app', CODE.x, CODE.y - 20, { ...ZONE, color: C.ink, alpha: zk });
  text(R, `node ${versions.node} · TypeScript`, CODE.x + measure('Your app', ZONE) + 16, CODE.y - 20, { ...ZONE_SUB, color: C.dim, alpha: zk });
  text(R, '@supernovae-st/nika', COL.cx, CODE.y - 20, { ...DOOR, color: C.ice, alpha: zk, align: 'center', glow: 0.35 });
  text(R, `THE PACKAGE · ${PKG}`, COL.cx, CODE.y + 4, { f: 'MGW 500', size: 10.5, tracking: 3, color: C.dim, alpha: zk, align: 'center' });
  // each door is named by its constructor
  const lk = a * smooth(T.localDraw, T.localDraw + 0.35, t), rk = a * smooth(T.remoteDraw, T.remoteDraw + 0.35, t);
  doorTitle(R, 'new Nika()', 'on your machine · the bundled engine', LOCAL, C.ice, Math.max(zk * 0.4, lk));
  doorTitle(R, 'new Nika({ url, token })', 'on your server · nika serve', REMOTE, C.cyan, Math.max(zk * 0.4, rk));
}
function doorTitle(R, ctor, sub, box, color, al) {
  text(R, ctor, box.x, box.y - 20, { ...DOOR, color, alpha: al, glow: 0.3 });
  text(R, sub, box.x + measure(ctor, DOOR) + 16, box.y - 20, { ...ZONE_SUB, color: C.dim, alpha: al });
}

// ── the app: demo.mts, the README quick start ──────────────────────────
// A line as it reads at t. Lines 3 and 6 are edited into the server door's
// and back, line 6 just after line 3: only their options are erased and
// typed (characters are erased and typed, never invented).
function lineAt(ln, t) {
  const e = editOf(ln);
  if (!e) return { src: demo[ln - 1] };
  const [go, back] = [T.m1 + e.lag, T.m2 + e.lag];
  const edit = (t0, from, to) => {
    if (t < t0) return { arg: from, editing: false };
    const k = seg(t, t0, t0 + 0.3), ty = seg(t, t0 + 0.34, t0 + 0.34 + 0.02 * to.length);
    if (k < 1) return { arg: from.slice(0, Math.round(from.length * (1 - k))), editing: true };
    return { arg: to.slice(0, Math.round(to.length * ty)), editing: ty < 1 };
  };
  const s = t < back ? edit(go, e.from, e.to) : edit(back, e.to, e.from);
  return { src: e.head + s.arg + e.tail, arg: s.arg, editing: s.editing, remote: t >= go + 0.3 && t < back + 0.3, edit: e, go, back };
}

function editor(R, t, a) {
  const k = E.snap(seg(t, T.win, T.win + 0.5));
  panel(R, CODE, { title: 'demo.mts · the readme quick start', alpha: a, k });
  if (k < 1) return;
  // a line lights while its call leaves, its answer is read or its options change
  const hl = [
    { ln: L.client, t0: T.local - 0.1, t1: T.m2 + 0.9, c: C.ice },
    { ln: L.run, t0: T.m1 + 0.05, t1: T.m2 + 1.05, c: C.ice },
    { ln: L.run, t0: T.hello - 0.3, t1: T.rrun + 0.7, c: C.ice },
    { ln: L.result, t0: T.resultAt - 0.05, t1: T.rresultAt + 1.0, c: C.cyan },
    { ln: L.verify, t0: T.tvHl, t1: T.rtv + 0.6, c: C.gold },
    { ln: L.verify, t0: T.proofAt - 0.05, t1: T.proofAt + 1.1, c: C.teal },
  ];
  ROWS.forEach((ln, i) => {
    const t0 = T.type0 + i * T.typeEvery;
    if (t < t0) return;
    const line = lineAt(ln, t);
    const spans = spansOf(line.src);
    const ranges = rangesOf(ln, line.src);
    const r0 = ROW_START.get(ln);
    const y = rowY(r0), yLast = rowY(r0 + ranges.length - 1);
    const n = Math.floor(line.src.length * clamp((t - t0) / T.typeDur));
    const typing = n < line.src.length;
    for (const h of hl) {
      if (h.ln !== ln || t < h.t0 || t > h.t1) continue;
      const hk = smooth(h.t0, h.t0 + 0.2, t) * (1 - smooth(h.t1 - 0.25, h.t1, t));
      const c = line.remote ? C.cyan : h.c;
      rect(R, CODE.x + 6, y - 20, CODE.w - 12, yLast - y + ROW_H, { fill: c, alpha: a * 0.1 * hk });
      rect(R, CODE.x + 6, y - 20, 3, yLast - y + ROW_H, { fill: c, alpha: a * hk, glow: 0.8 });
    }
    text(R, String(ln), codeX - 16, y, { f: 'MM 400', size: CODE_ST.size - 2, color: C.dim, alpha: a * 0.85, align: 'right' });
    // each row: its characters, as far as they are typed
    ranges.forEach(([ra, rb], r) => {
      if (r && n <= ra) return;
      const ind = r ? WRAP_INDENT : 0;
      mono(R, slice(spans, ra, Math.min(rb, n)), codeX + ind * ADV, rowY(r0 + r), { alpha: a, st: CODE_ST, max: CODE_MAX - ind });
    });
    const e = line.edit;
    if (e && t >= line.go - 0.2 && t < line.back + 1.2) {
      // the options are the only characters that change
      const at = argAt(e, ranges);
      const ya = rowY(r0 + at.row);
      const x0 = codeX + at.col * ADV - 3;
      const w = Math.max(ADV, line.arg.length * ADV) + 6;
      const on = smooth(line.go - 0.2, line.go, t) * (1 - smooth(line.back + 0.8, line.back + 1.2, t));
      const c = line.remote ? C.cyan : C.ice;
      rrect(R, x0, ya - 17, w, 23, 5, { color: c, w: 1.2, alpha: a * on * 0.9, fill: c, fillAlpha: 0.08, glow: 0.4 * on });
      if (line.editing) {
        const blink = Math.floor(t * 4) % 2 === 0 ? 1 : 0.35;
        rect(R, x0 + 3 + line.arg.length * ADV + 1, ya - 14, 2, 18, { fill: c, alpha: a * blink, glow: 0.8 });
      }
    }
    // the token a packet leaves from, or lands on, lights up (on its first row)
    for (const mk of MARKS) {
      if (mk.tok.ln !== ln || t < mk.t0 || t > mk.t1 || typing) continue;
      const mk2 = smooth(mk.t0, mk.t0 + 0.15, t) * (1 - smooth(mk.t1 - 0.3, mk.t1, t));
      const x = codeX + mk.tok.col * ADV;
      rrect(R, x - 3, y - 16, mk.tok.len * ADV + 6, 22, 5, { fill: mk.color, fillAlpha: 0.16, color: mk.color, w: 1, alpha: a * mk2, glow: 0.5 * mk2 });
      mono(R, [{ s: line.src.slice(mk.tok.col, mk.tok.col + mk.tok.len), c: mk.color, glow: 0.6 }], x, y, { alpha: a * mk2, st: CODE_ST });
    }
    if (typing || (i === ROWS.length - 1 && t < t0 + T.typeDur + 0.5)) {
      // the cursor: on the row the typing has reached
      const m = typing ? n : line.src.length;
      const r = Math.max(0, ranges.findLastIndex(([ra]) => ra < m || ra === 0));
      const col = (r ? WRAP_INDENT : 0) + Math.min(m, ranges[r][1]) - ranges[r][0];
      const blink = Math.floor(t * 2.4) % 2 === 0 ? 1 : 0.3;
      rect(R, codeX + col * ADV + 1, rowY(r0 + r) - 14, 2, 18, { fill: C.ice, alpha: a * blink, glow: 0.8 });
    }
  });
}
const MARKS = [
  { tok: TOK.client, color: C.ice, t0: T.local - 0.2, t1: T.local + 0.6 },
  { tok: TOK.client, color: C.cyan, t0: T.remote - 0.2, t1: T.remote + 0.6 },
  { tok: TOK.file, color: C.ice, t0: T.hello - 0.3, t1: T.hello + 0.7 },
  { tok: TOK.file, color: C.cyan, t0: T.rcheck - 0.3, t1: T.rcheck + 0.6 },
  { tok: TOK.file, color: C.cyan, t0: T.rrun - 0.3, t1: T.rrun + 0.6 },
  { tok: TOK.result, color: C.cyan, t0: T.resultAt - 0.12, t1: T.rresultAt + 0.8 },
  { tok: TOK.receipt, color: C.gold, t0: T.tv - 0.3, t1: T.tv + 0.7 },
  { tok: TOK.receipt, color: C.gold, t0: T.rtv - 0.3, t1: T.rtv + 0.6 },
  { tok: TOK.proof, color: C.teal, t0: T.proofAt - 0.12, t1: T.proofAt + 1.0 },
];

// ── the app: what it reads ─────────────────────────────────────────────
const HEAD_ST = { f: 'MGW 500', size: 10.5, tracking: 2.5 };
const HEAD_MONO = { f: 'MM 400', size: 13 };
const FOOT_ST = { f: 'MM 400', size: 14 };
function consolePanel(R, t, a) {
  const k = E.snap(seg(t, T.win + 0.1, T.win + 0.6));
  panel(R, CONSOLE, { title: 'what your app reads · run.events() · run.result()', alpha: a, k });
  if (k < 1) return;
  const hy = CONSOLE.y + 76;
  const ck = smooth(T.counts, T.counts + 0.3, t);
  for (const [x, label, kind, n, c] of [[EVX.local, 'LOCAL', 'native-process', local.events.length, C.ice], [EVX.remote, 'REMOTE', 'http', remote.events.length, C.cyan]]) {
    text(R, label, x, hy, { ...HEAD_ST, color: c, alpha: a * 0.95 });
    const x1 = x + measure(label, HEAD_ST) + 12;
    text(R, kind, x1, hy, { ...HEAD_MONO, color: C.dim, alpha: a });
    if (ck > 0) text(R, `${n} EVENTS`, x1 + measure(kind, HEAD_MONO) + 14, hy, { ...HEAD_ST, color: C.teal, alpha: a * ck, glow: 0.3 });
  }
  line(R, CONSOLE.x + 24, hy + 14, CONSOLE.x + CONSOLE.w - 24, hy + 14, { color: C.line, w: 1, alpha: a });
  // what run() threw for the refused file: before any run existed
  const ek = landed('redBack', t) * (1 - smooth(T.ev0 + 0.3, T.ev0 + 0.55, t));
  if (ek > 0) {
    mono(R, [{ s: 'catch ', c: C.dim }, { s: REFUSAL.name, c: C.red, b: true }, { s: ` · ${REFUSAL.code}`, c: C.ink }], EVX.local, evY(0), { alpha: a * ek, st: EV_ST });
    text(R, 'run() threw before any run existed: no events, no trace', EVX.local, evY(1), { ...SMALL, color: C.mist, alpha: a * ek });
  }
  // each event lands as a row where its chip arrives
  local.events.forEach((e, k2) => {
    const on = landed(`ev${k2}`, t);
    if (on <= 0) return;
    const y = evY(k2);
    const detail = e.task ?? (e.kind === 'engine.event' ? `(${e.raw})` : e.status ?? '');
    mono(R, [{ s: e.kind, c: C.ice, b: true }], EVX.local, y, { alpha: a * on, st: EV_ST });
    mono(R, [{ s: detail, c: e.kind === 'engine.event' ? C.dim : C.mist }], EVX.detail, y, { alpha: a * on, st: { f: 'MM 400', size: 15 } });
  });
  remote.events.forEach((e, k2) => {
    const on = landed(`rev${k2}`, t);
    if (on <= 0) return;
    mono(R, [{ s: `#${e.sequence} `, c: C.dim }, { s: e.kind, c: C.cyan, b: true }, { s: ` ${e.status}`, c: C.mist }], EVX.remote, evY(remoteRow(e.sequence)), { alpha: a * on, st: EV_ST });
  });
  // between the server's two frames: none per task, and none invented
  const gk = smooth(P.rev1.t1, P.rev1.t1 + 0.4, t);
  if (gk > 0) {
    const x = EVX.remote + 8;
    line(R, x, evY(0) + 10, x, evY(5) - 22, { color: C.cyan, w: 1.2, alpha: a * gk * 0.6, dash: [2, 5] });
    text(R, 'no per-task frames:', x + 18, evY(2) + 2, { ...SMALL, color: C.dim, alpha: a * gk });
    text(R, 'the server streams two', x + 18, evY(3) - 4, { ...SMALL, color: C.dim, alpha: a * gk });
  }
  // the terminal truth: run.result(), then the typed output
  const rk = landed('result', t, 0.25);
  line(R, CONSOLE.x + 24, FOOT.y1 - 26, CONSOLE.x + CONSOLE.w - 24, FOOT.y1 - 26, { color: C.line, w: 1, alpha: a * rk });
  if (rk > 0) {
    mono(R, [{ s: 'run.result()', c: C.cyan, b: true }], EVX.local, FOOT.y1, { alpha: a * rk, st: { f: 'MM 500', size: 14 } });
    mono(R, [{ s: local.result.status, c: C.teal }, { s: ` · cause ${local.result.cause}`, c: C.mist }], EVX.detail, FOOT.y1, { alpha: a * rk, st: FOOT_ST });
    mono(R, [{ s: 'result.outputs?.greeting', c: '#B3C5DD' }, { s: '  ' }, { s: GREETING, c: C.ink }], EVX.local, FOOT.y2, { alpha: a * rk, st: FOOT_ST });
  }
  const rr = landed('rresult', t, 0.25);
  if (rr > 0) mono(R, [{ s: remote.result.status, c: C.teal }], EVX.remote + 3 * EV_ADV, FOOT.y1, { alpha: a * rr, st: FOOT_ST });
}

// ── the package: a glass column, one junction, two branches ────────────
function column(R, t, a) {
  const k = E.snap(seg(t, T.column, T.column + 0.7));
  if (k <= 0 || R.glowPass) return;
  const { cx, top, bottom, half } = COL;
  const h = (bottom - top) * k;
  const ctx = R.ctx;
  const g = ctx.createLinearGradient(0, top, 0, top + h);
  g.addColorStop(0, rgba('#0f2346', 0));
  g.addColorStop(0.15, rgba('#0f2346', 0.5 * a));
  g.addColorStop(0.85, rgba('#0f2346', 0.5 * a));
  g.addColorStop(1, rgba('#0f2346', 0));
  ctx.fillStyle = g;
  ctx.fillRect(cx - half, top, half * 2, h);
  const e = ctx.createLinearGradient(0, top, 0, top + h);
  e.addColorStop(0, rgba(C.ice, 0));
  e.addColorStop(0.2, rgba(C.ice, 0.3 * a));
  e.addColorStop(0.8, rgba(C.ice, 0.3 * a));
  e.addColorStop(1, rgba(C.ice, 0));
  ctx.fillStyle = e;
  ctx.fillRect(cx - half - 1, top, 1, h);
  ctx.fillRect(cx + half, top, 1, h);
}
// what the package is, in the README's words, under the doors' branches
const NOTE = ["carries the engine's answers, typed", 'never parses a .nika file itself', 'never re-implements the proof'];
function note(R, t, a) {
  const k = a * smooth(5.0, 5.4, t);
  if (k <= 0) return;
  NOTE.forEach((l, i) => text(R, l, J.x, 922 + i * 22, { ...SMALL, color: C.mist, alpha: k * 0.85, align: 'center' }));
}

// the junction: the one client every call goes through
function junction(R, t, a) {
  const jk = a * smooth(P.born.t1 - 0.1, P.born.t1 + 0.2, t);
  if (jk <= 0) return;
  // a refused file is thrown back through it: red while the refusal passes,
  // clear again before the quick start crosses it
  const tThrow = tripTime(P.redBack, junctionAt(P.redBack));
  const th = smooth(tThrow - 0.08, tThrow + 0.05, t) * (1 - smooth(tThrow + 0.25, tThrow + 0.45, t));
  const col = th > 0.5 ? C.red : C.ice;
  circle(R, J.x, J.y, 11, { color: col, w: 1.4, alpha: jk, fill: C.bg0, fillAlpha: 1, glow: 0.6 });
  circle(R, J.x, J.y, 4, { fill: col, alpha: jk, glow: 0.8 });
  text(R, 'nika', J.x, J.y - 22, { f: 'MM 500', size: 14, color: C.ice, alpha: jk * 0.9, align: 'center' });
  if (th > 0) light(R, J.x, J.y, 80, C.red, 0.4 * a * th, 1);
  // a packet crossing it: a flash
  for (const p of PACKETS) {
    if (p.kind === 'spark') continue;
    const tm = tripTime(p, junctionAt(p));
    const fl = smooth(tm - 0.05, tm, t) * (1 - smooth(tm, tm + 0.35, t));
    if (fl > 0) light(R, J.x, J.y, 46, p.color, 0.35 * a * fl, 1);
  }
}
// the fraction of its trip at which a packet crosses the junction
const junctionAt = p => {
  let best = 0, bd = Infinity;
  p.trip.pts.forEach((q, i) => {
    const d = Math.hypot(q[0] - J.x, q[1] - J.y);
    if (d < bd) { bd = d; best = p.trip.acc[i] / p.trip.len; }
  });
  return best;
};
if (tripTime(P.hello, junctionAt(P.hello)) < tripTime(P.redBack, junctionAt(P.redBack)) + 0.5) fail('the quick start crosses the junction while it is still red');
// the moment a packet is at fraction f of its trip
function tripTime(p, f) {
  let lo = p.t0, hi = p.t1;
  for (let i = 0; i < 24; i++) {
    const mid = (lo + hi) / 2;
    if (tripAt(p, mid) < f) lo = mid; else hi = mid;
  }
  return hi;
}

function branches(R, t, a) {
  for (const [door, t0, color] of [['local', T.localDraw, C.ice], ['remote', T.remoteDraw, C.cyan]]) {
    const q = E.inOutCubic(seg(t, t0, t0 + 0.5));
    if (q <= 0) continue;
    poly(R, BRANCH[door], { color, w: 1.8, alpha: a * 0.55, glow: 0.3 }, q);
    if (q < 1) for (let s = 0; s < 4; s++) poly(R, BRANCH[door], { color, w: 2.4 + s * 0.7, alpha: a * (0.18 + s * 0.24), glow: 0.9 }, q, Math.max(0, q - 0.3 + s * 0.075));
    if (q >= 1) {
      const [px, py] = PORT[door];
      circle(R, px, py, 4, { fill: color, alpha: a, glow: 0.8 });
      const fl = 1 - smooth(t0 + 0.5, t0 + 1.2, t);
      if (fl > 0) light(R, px, py, 70, color, 0.4 * a * fl, 1);
    }
  }
}

// What crosses each branch, named as the package names it, riding the
// branch while it passes.
const BRANCH_LABEL = { f: 'MM 500', size: 19 };
const LABEL_AT = (() => {
  const at = { local: { x: J.x + 52, y: PORT.local[1] - 2 }, remote: { x: J.x + 52, y: PORT.remote[1] - 6 } };
  // each label must clear its own branch: above the local one, below the remote one
  for (const door of ['local', 'remote']) {
    const half = measure('traceVerify()', BRANCH_LABEL) / 2 + 6;
    for (const [x, y] of BRANCH[door]) {
      if (x < at[door].x - half || x > at[door].x + half) continue;
      const clear = door === 'local' ? y > at[door].y + 10 : y < at[door].y - BRANCH_LABEL.size - 4;
      if (!clear) fail(`the ${door} branch label sits on its branch`);
    }
  }
  return at;
})();
function branchLabels(R, t, a) {
  for (const p of PACKETS) {
    if (!p.branch) continue;
    const tJ = tripTime(p, junctionAt(p));
    const tIn = p.from ? p.t0 - 0.15 : tJ - 0.1;
    const until = p.until ?? (p.from ? tJ + 0.35 : p.t1 + 0.35);
    const k = smooth(tIn, tIn + 0.2, t) * (1 - smooth(until, until + 0.25, t));
    if (k <= 0) continue;
    const { x, y } = LABEL_AT[p.door];
    const color = p.bad ? C.red : p.color === C.gold || p.color === C.teal ? p.color : p.door === 'local' ? C.ice : C.cyan;
    text(R, p.branch, x, y, { ...BRANCH_LABEL, color, alpha: a * k, align: 'center', glow: 0.3 });
  }
}

// a light between a door's port and the station a packet reaches inside
function inner(R, t, a, p) {
  const st = p.into ?? p.from;
  // a far station (the record, the receipt's dock) is reached without a
  // light across the card: the ticket and the answer appear where they are
  if (!st || Math.hypot(st[0] - PORT[p.door][0], st[1] - PORT[p.door][1]) > 120) return;
  const t0 = p.into ? p.t1 - 0.04 : p.t0 - 0.3;
  const s = seg(t, t0, t0 + 0.3);
  if (s <= 0 || s >= 1) return;
  const [px, py] = PORT[p.door];
  const [a0, b0] = p.into ? [[px, py], st] : [st, [px, py]];
  const q = E.inOutSine(s);
  const x = lerp(a0[0], b0[0], q), y = lerp(a0[1], b0[1], q);
  line(R, lerp(a0[0], b0[0], Math.max(0, q - 0.4)), lerp(a0[1], b0[1], Math.max(0, q - 0.4)), x, y, { color: p.color, w: 2, alpha: a * Math.sin(Math.PI * s), glow: 1 });
  light(R, x, y, 40, p.color, 0.35 * a * Math.sin(Math.PI * s), 1);
}

// a packet: a comet behind it, its name in a pill (or a chip, a ticket)
function packet(R, t, a, p) {
  if (t < p.t0 || t > p.t1 + 0.3) return;
  const q = tripAt(p, t);
  const [x, y] = along(p.trip, q);
  const fade = smooth(p.t0, p.t0 + 0.12, t) * (1 - smooth(p.t1 - 0.04, p.t1 + 0.2, t));
  if (fade <= 0) return;
  // the comet: the last stretch of the trip, brighter towards its head
  const tail = Math.max(0, q - 110 / p.trip.len);
  for (let s = 0; s < 2; s++) poly(R, p.trip.pts, { color: p.color, w: 1.8 + s * 0.9, alpha: a * fade * (0.22 + s * 0.3), glow: 0.7 }, q, tail + ((q - tail) * s) / 2);
  if (p.kind === 'spark') return light(R, x, y, 44, p.color, 0.5 * a * fade, 1);
  // at a door's port it passes into the card small, so it never covers
  // the rows beside the port; an answer leaves the same way
  const s = p.into ? 1 - 0.65 * smooth(0.8, 1, q) : p.from ? 0.35 + 0.65 * smooth(0, 0.2, q) : 1;
  const ctx = R.ctx;
  ctx.save();
  ctx.translate(x, y);
  ctx.scale(s, s);
  if (p.kind === 'ticket') ticketAt(R, t, a * fade, 0, 0);
  else {
    const st = p.kind === 'chip' ? { f: 'MM 500', size: 14 } : CHIP_ST;
    const w = measure(p.label, st) + 26, h = p.kind === 'chip' ? 26 : 32;
    light(R, 0, 0, 44, p.color, 0.22 * a * fade, 1);
    rrect(R, -w / 2, -h / 2, w, h, h / 2, { color: p.color, w: 1.5, alpha: a * fade, fill: '#06101f', fillAlpha: 0.96, glow: 0.4 });
    text(R, p.label, 0, st.size * 0.36, { ...st, color: p.bad ? C.red : C.ink, alpha: a * fade, align: 'center' });
  }
  ctx.restore();
}

// the receipt: a ticket carrying the record's head, its length and its seal
function ticketAt(R, t, a, x, y) {
  if (a <= 0) return;
  const m = smooth(T.verified, T.verified + 0.3, t) * (1 - tamperK(t));
  const col = m > 0.5 ? C.teal : C.gold;
  if (t < T.dock) light(R, x, y, 50, C.gold, 0.22 * a, 1);
  const x0 = x - TICKET.w / 2, y0 = y - TICKET.h / 2;
  rrect(R, x0, y0, TICKET.w, TICKET.h, 8, { color: col, w: 1.5, alpha: a, fill: '#0c1322', fillAlpha: 0.97, glow: 0.35 + m * 0.3 });
  for (const nx of [x0, x0 + TICKET.w]) circle(R, nx, y, 6, { fill: C.bg0, alpha: a, color: col, w: 1.2 });
  mono(R, [{ s: 'result.receipt', c: C.gold }], x0 + 16, y0 + 19, { alpha: a, st: { f: 'MM 500', size: 12.5 } });
  mono(R, [{ s: 'chain_head ', c: C.dim }, { s: short(local.receipt.chain_head), c: m > 0.5 ? C.teal : C.ink }], x0 + 16, y0 + 37, { alpha: a, st: { f: 'MM 400', size: 12 } });
  mono(R, [{ s: `chain_len ${local.receipt.chain_len} · sealed ${local.receipt.sealed}`, c: C.mist }], x0 + 16, y0 + 53, { alpha: a, st: { f: 'MM 400', size: 12 } });
}

// ── the doors ───────────────────────────────────────────────────────────
function station(R, t, a, x, y, label, t0, t1, color = C.ice) {
  if (a <= 0) return;
  const on = smooth(t0 - 0.2, t0, t), done = t >= t1;
  const iconX = x - 22, iconY = y - 5;
  if (done) {
    circle(R, iconX, iconY, 8.5, { color: C.teal, w: 1.4, alpha: a, fill: C.teal, fillAlpha: 0.12, glow: 0.5 });
    check(R, iconX, iconY, 9.5, E.snap(seg(t, t1, t1 + 0.3)), { color: C.teal, w: 2, alpha: a, glow: 0.7 });
  } else {
    circle(R, iconX, iconY, 8.5, { color: C.faint, w: 1.2, alpha: a });
    if (on > 0) arc(R, iconX, iconY, 8.5, (t - t0) * 7, (t - t0) * 7 + Math.PI * 1.3, { color, w: 1.8, alpha: a * on, glow: 0.7 });
  }
  text(R, label, x, y, { ...STATION, color: done ? C.teal : on > 0 ? C.ink : C.dim, alpha: a });
}
function chip(R, x, y, label, color, al, w) {
  if (al <= 0) return;
  rrect(R, x, y - 21, w, 30, 8, { color, w: 1.2, alpha: al, fill: color, fillAlpha: 0.06 });
  text(R, label, x + 12, y, { ...CHIP_ST, color: C.ink, alpha: al });
}
// the tamper beat: 0 → 1 → 0 while one byte is changed and read back
const tamperK = t => smooth(T.tamper, T.tamper + 0.25, t) * (1 - smooth(T.tamperEnd - 0.3, T.tamperEnd, t));

function localDoor(R, t, a) {
  const k = E.snap(seg(t, T.win + 0.1, T.win + 0.6));
  const tk = tamperK(t);
  const badge = tk > 0.5 ? { label: 'BROKEN', color: C.red, alpha: tk }
    : t > T.verified ? { label: 'VERIFIED', color: C.teal, alpha: smooth(T.verified, T.verified + 0.3, t) }
      : t > T.sealed ? { label: 'SEALED', color: C.gold, alpha: smooth(T.sealed, T.sealed + 0.3, t) }
        : t > T.ready ? { label: 'RUN READY', color: C.teal, alpha: smooth(T.ready, T.ready + 0.3, t) }
          : t > T.payload ? { label: 'BUNDLED', color: C.ice, alpha: smooth(T.payload, T.payload + 0.3, t) } : null;
  panel(R, LOCAL, { title: `nika ${ENGINE_VER} (${BUILD}) · child process`, alpha: a, k, badge });
  if (k < 1) return;
  const lit = smooth(T.localDraw + 0.4, T.localDraw + 0.6, t) * (1 - smooth(T.localDraw + 0.9, T.localDraw + 1.6, t));
  if (lit > 0) rrect(R, LOCAL.x, LOCAL.y, LOCAL.w, LOCAL.h, 14, { color: C.ice, w: 1.4, alpha: a * lit * 0.55, glow: 0.25 });
  // the payload npm installed, held to its INTEGRITY.json
  const pk = smooth(T.payload, T.payload + 0.3, t);
  if (pk > 0) mono(R, [{ s: `${PAYLOAD.name} ${PAYLOAD.version}`, c: C.mist }, { s: ' · sha256 = INTEGRITY.json ', c: C.dim }, { s: '✔', c: C.teal }], LX - 26, LOCAL.y + 68, { alpha: a * pk, st: { f: 'MM 400', size: 12 } });

  // CHECK: the refused file, then the quick start
  station(R, t, a, LX, ST.check, 'CHECK · BEFORE ANYTHING RUNS', T.redAt, T.ready);
  chip(R, LX, ST.red, 'list-files.nika', C.red, a * landed('red', t), CHIP_RED_W);
  const sk = smooth(T.redStamp, T.redStamp + 0.2, t);
  if (sk > 0) mono(R, [{ s: '✖ ', c: C.red, glow: 0.5 }, { s: REFUSAL.code, c: C.red, b: true }], VERDICT_X, ST.red, { alpha: a * sk, st: BIG });
  chip(R, LX, ST.hello, 'hello.nika', C.ice, a * landed('hello', t), CHIP_HELLO_W);
  const yk = smooth(T.ready, T.ready + 0.25, t);
  if (yk > 0) mono(R, [{ s: READY, c: C.teal, glow: 0.4 }], VERDICT_X, ST.hello, { alpha: a * yk, st: BIG });
  // the engine's scan of each file as it arrives
  for (const [t0, t1, y, w] of [[T.redAt + 0.15, T.redStamp, ST.red, CHIP_RED_W], [T.helloAt + 0.15, T.ready, ST.hello, CHIP_HELLO_W]]) {
    const s = seg(t, t0, t1);
    if (s <= 0 || s >= 1) continue;
    const sx = LX + w * E.inOutSine(s);
    line(R, sx, y - 25, sx, y + 13, { color: C.ice, w: 2, alpha: a, glow: 1 });
    light(R, sx, y - 6, 44, C.ice, 0.3 * a, 1);
  }
  report(R, t, a);

  // RUN: the one task, on the rehearsal model
  station(R, t, a * smooth(T.task - 0.2, T.task, t), LX, ST.run, 'RUN', T.task, T.taskDone);
  const uk = smooth(T.task - 0.1, T.task + 0.2, t);
  if (uk > 0) {
    const m = PLAN.match(/^(\S+) \((.*)\)$/u);
    mono(R, m ? [{ s: m[1], c: C.ink }, { s: ` (${m[2]})`, c: C.mist }] : [{ s: PLAN, c: C.ink }], LX, ST.runText, { alpha: a * uk, st: { f: 'MM 500', size: 19 } });
    pill(R, LOCAL.x + LOCAL.w - 24, ST.run - 5, 'MOCK/ECHO · NO KEY · NO NETWORK', C.amber, a * smooth(T.task + 0.2, T.task + 0.5, t), 'right');
  }

  // RECORD: each line carries the hash of the one before it
  const rk = a * smooth(T.record - 0.3, T.record, t);
  station(R, t, rk * (1 - tk), LX, ST.record, 'RECORD · HASH-CHAINED', T.jRow(0), T.sealed);
  if (tk > 0) text(R, `ONE BYTE CHANGED · LINE ${local.tamper.line}`, LX, ST.record, { ...STATION, color: C.red, alpha: a * tk });
  journal.forEach((j, i) => {
    const t0 = T.jRow(i);
    const e = smooth(t0, t0 + 0.15, t);
    if (e <= 0) return;
    const y = jRowY(i), dx = 14 * (1 - E.snap(seg(t, t0, t0 + 0.3)));
    const tw = T.walk + i * T.walkEvery, walked = t >= tw;
    const sealed = j.kind === 'run_sealed';
    // the changed line and every line after it: unverified while it is changed
    const bad = i >= CHANGED && tk > 0.5;
    const changed = i === CHANGED && tk > 0.5;
    const col = changed ? C.red : bad ? C.dim : walked ? C.teal : sealed ? C.gold : C.ice;
    if (i > 0) {
      const lk = E.snap(seg(t, t0, t0 + 0.2));
      const broken = i === CHANGED + 1 ? tk : 0;
      line(R, J_COL.dot, y - 15, J_COL.dot, y - 15 + 10 * lk, { color: broken > 0.5 ? C.red : walked ? C.teal : C.dim, w: 1.4, alpha: a * e, glow: walked ? 0.5 : 0 });
      if (broken > 0) cross(R, J_COL.dot - 14, y - 11, 7, broken, { color: C.red, w: 1.6, alpha: a * broken, glow: 0.6 });
    }
    circle(R, J_COL.dot, y - 5, sealed ? 4.2 : 3.3, { fill: col, alpha: a * e, glow: 0.6 });
    if (sealed) {
      circle(R, J_COL.dot, y - 5, 8, { color: C.gold, w: 1.2, alpha: a * e * (bad ? 0.4 : 1), glow: 0.5 });
      const pu = seg(t, T.sealed, T.sealed + 0.7);
      if (pu > 0 && pu < 1) circle(R, J_COL.dot, y - 5, 8 + 26 * E.outCubic(pu), { color: C.gold, w: 1.4, alpha: a * (1 - pu), glow: 1 });
    }
    // the changed line's hash, recomputed: no longer the one line 5 recorded
    const hm = i === journal.length - 1 ? smooth(T.verified, T.verified + 0.3, t) * (1 - tk) : 0;
    const dimmed = bad && !changed ? 0.45 : 1;
    text(R, changed ? `${COMPUTED.slice(0, 8)}…` : short(j.hash), J_COL.hash + dx, y, { f: 'MM 500', size: 13, color: changed ? C.red : hm > 0 ? C.teal : walked ? C.mist : C.ink, alpha: a * e * dimmed, glow: hm * 0.5 + (changed ? 0.5 : 0) });
    text(R, j.kind, J_COL.kind + dx, y, { ...SMALL, color: sealed ? C.gold : C.mist, alpha: a * e * dimmed });
    if (walked && !bad) check(R, J_COL.tick, y - 5, 9, E.snap(seg(t, tw, tw + 0.2)), { color: C.teal, w: 1.8, alpha: a, glow: 0.6 });
  });
  // the verifier walks the chain
  const wEnd = T.walk + journal.length * T.walkEvery;
  if (t > T.walk - 0.05 && t < wEnd + 0.15) {
    const yy = lerp(jRowY(0) - 16, jRowY(journal.length - 1) + 6, clamp((t - T.walk) / (wEnd - T.walk)));
    line(R, LOCAL.x + 18, yy, J_COL.tick + 14, yy, { color: C.teal, w: 1.5, alpha: a * 0.9, glow: 1 });
    light(R, LX + 40, yy, 60, C.teal, 0.25 * a, 1);
  }

  // VERIFY: the receipt the app kept, and the verifier's word
  station(R, t, rk, V.x + 22, V.label, 'VERIFY THE RECEIPT', T.dock, T.verified);
  const dk = E.snap(seg(t, T.dock, T.dock + 0.35));
  if (dk > 0) ticketAt(R, t, a * dk, STP.dock[0], STP.dock[1] + 8 * (1 - dk));
  const vk = smooth(T.verified, T.verified + 0.3, t) * (1 - tk);
  const VERDICT_ST = { f: 'MM 500', size: 26 };
  if (vk > 0) {
    mono(R, [{ s: local.verify.verdict, c: C.teal, glow: 0.5 }], V.x, V.verdict, { alpha: a * vk, st: VERDICT_ST });
    check(R, V.x + measure(local.verify.verdict, VERDICT_ST) + 22, V.verdict - 9, 20, E.snap(seg(t, T.verified + 0.1, T.verified + 0.45)), { color: C.teal, w: 2.6, alpha: a * vk, glow: 0.9 });
  }
  const ok = smooth(T.ok, T.ok + 0.25, t) * (1 - tk);
  if (ok > 0) text(R, OK_LINE.slice(0, OK_LINE.indexOf(' · head')), V.x, V.words, { ...SMALL, color: C.teal, alpha: a * ok });
  if (tk > 0) {
    mono(R, [{ s: `BROKEN at line ${BROKEN_LINE}`, c: C.red, b: true, glow: 0.4 }], V.x, V.verdict, { alpha: a * tk, st: BIG });
    text(R, `not verified · ${local.tamper.verdict}`, V.x, V.words, { ...SMALL, color: C.red, alpha: a * tk });
  }
}

// the engine's report of the file it reads, verbatim, cut to the card
const REPORT_ST = { f: 'MM 400', size: 12 };
const REPORT_MAX = Math.floor((LOCAL.w - 52 - 24) / cw(REPORT_ST));
if (REPORT_HELLO[5].length > REPORT_MAX) fail('the report cuts run ready');
const colourMark = l => {
  const m = l.match(/^(\s*)(✔|✖)(\s+)(\S+)(.*)$/u);
  if (!m) return [{ s: l, c: C.mist }];
  const col = m[2] === '✔' ? C.teal : C.red;
  return [{ s: m[1] }, { s: m[2], c: col, b: true }, { s: m[3] }, { s: m[4], c: col, b: true }, { s: m[5], c: m[2] === '✖' ? C.ink : C.mist }];
};
function report(R, t, a) {
  for (const [lines, t0, t1] of [[REPORT_RED, T.redAt + 0.15, T.helloAt - 0.2], [REPORT_HELLO, T.helloAt + 0.15, T.reportOut]]) {
    if (t < t0 || t > t1) continue;
    const out = 1 - smooth(t1 - 0.2, t1, t);
    lines.forEach((l, i) => {
      const born = t0 + i * 0.07;
      const e = smooth(born, born + 0.12, t) * out;
      if (e > 0) mono(R, i === 0 ? [{ s: l, c: C.ink, b: true }] : colourMark(l), LX - 4, ST.run - 8 + i * 20, { alpha: a * e, st: REPORT_ST, max: REPORT_MAX });
    });
  }
}

function remoteDoor(R, t, a) {
  const k = E.snap(seg(t, T.win + 0.15, T.win + 0.65));
  const badge = t > P.rtv.t1 + 0.1 ? { label: remote.verify.verdict.toUpperCase(), color: C.teal, alpha: smooth(P.rtv.t1 + 0.1, P.rtv.t1 + 0.4, t) }
    : t > T.remoteSub ? { label: 'LOOPBACK HTTP', color: C.cyan, alpha: smooth(T.remoteSub, T.remoteSub + 0.3, t) } : null;
  panel(R, REMOTE, { title: `nika serve · ${remote.health.engineVersion} (${remote.health.buildSha})`, alpha: a, k, badge });
  if (k < 1) return;
  const lit = smooth(T.remoteDraw + 0.4, T.remoteDraw + 0.6, t) * (1 - smooth(T.remoteDraw + 0.9, T.remoteDraw + 1.6, t));
  if (lit > 0) rrect(R, REMOTE.x, REMOTE.y, REMOTE.w, REMOTE.h, 14, { color: C.cyan, w: 1.4, alpha: a * lit * 0.55, glow: 0.25 });
  const sk = smooth(T.remoteSub, T.remoteSub + 0.3, t);
  if (sk > 0) text(R, 'Bearer token from a 0600 file · plain HTTP to a loopback host only', LX - 26, REMOTE.y + 70, { ...SMALL, color: C.mist, alpha: a * sk });
  const rows = [
    [RST.check, 'CHECK', P.rcheck.t1, 'hello.nika, by its served name', 'clean ✔'],
    [RST.run, 'RUN', P.rrun.t1, 'a durable job · same idempotencyKey → same job', null],
    [RST.verify, 'VERIFY', P.rtv.t1, "the server's verdict on its own journal", remote.verify.verdict],
  ];
  for (const [y, label, t1, what, verdict] of rows) {
    station(R, t, a, LX, y, label, t1 - 0.4, t1 + 0.1, C.cyan);
    const wk = smooth(t1 + 0.05, t1 + 0.3, t);
    if (wk <= 0) continue;
    text(R, what, LX + 96, y, { ...SMALL, color: C.mist, alpha: a * wk });
    if (verdict) mono(R, [{ s: verdict, c: C.teal, glow: 0.4 }], REMOTE.x + REMOTE.w - 24 - measure(verdict, BIG), y + 2, { alpha: a * wk, st: BIG });
  }
}

// After the story, a light runs both doors once: one client, two doors.
function lap(R, t, a) {
  const s = seg(t, T.lap, T.lap + 0.9);
  if (s <= 0 || s >= 1) return;
  for (const door of ['local', 'remote']) poly(R, BRANCH[door], { color: C.teal, w: 3, alpha: a * 0.9 * Math.sin(Math.PI * s), glow: 1 }, E.inOutSine(s), Math.max(0, E.inOutSine(s) - 0.35));
}

export function draw(R, t) {
  const a = envelope(t);
  zones(R, t, a);
  column(R, t, a);
  editor(R, t, a);
  consolePanel(R, t, a);
  localDoor(R, t, a);
  remoteDoor(R, t, a);
  branches(R, t, a);
  junction(R, t, a);
  note(R, t, a);
  lap(R, t, a);
  branchLabels(R, t, a);
  for (const p of PACKETS) inner(R, t, a, p);
  for (const p of PACKETS) packet(R, t, a, p);
}
