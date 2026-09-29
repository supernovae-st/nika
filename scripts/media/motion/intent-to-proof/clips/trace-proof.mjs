// trace-proof · "Change one byte. Verify says so."
// Every run leaves a hash-chained trace: each journal line carries, in its
// `chain` field, the sha256 of the exact bytes of the line before it. The
// clip draws the real trace of a `hello` run (mock/echo: a rehearsal, and
// it says so) as blocks on that chain, lets `nika trace verify` read it
// back intact, changes one byte of one line in a copy, and lets verify
// refuse the copy at the next link.
//
// Everything on screen is captured by scripts/media/capture/trace-proof.sh
// (media/raw/trace-proof-*): the run, the trace itself, both verifies, the
// exit codes, the edited byte and `cmp -l` of the two files. At load the
// clip recomputes every link of the captured trace, finds the one byte
// that differs, and refuses to render unless the run, the verify, the
// refusal and the exit codes tell exactly that story. The scan, the
// lighting and the comparisons illustrate what verify recomputes.
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { C, E, seg, smooth, clamp, lerp, REPO, readRepo, repoLines, NIKA_VERSION, terminal, frame, loopFade, cameraPath, frameBox, titleFade, WIDE, layoutRows, cw } from './kit.mjs';
import { text, rrect, rect, line, poly, circle, measure, light, streak, check, DW, DH } from '../src/engine/render.mjs';
import { hash, rgba, TAU } from '../src/engine/core.mjs';

export const meta = { duration: 21.9, poster: 21.3 };

// ── the captures, checked ───────────────────────────────────────────────
const fail = m => { throw new Error(`the trace-proof capture: ${m}`); };
const sha256 = s => createHash('sha256').update(s).digest('hex');
const bytes = f => fs.readFileSync(path.join(REPO, 'media/raw', `trace-proof-${f}`));
const TRACE = bytes('trace.ndjson'), COPY = bytes('tampered.ndjson');
const linesOf = buf => buf.toString('utf8').split('\n').filter(l => l.trim());
const LINES = linesOf(TRACE), COPY_LINES = linesOf(COPY);
const EVENTS = LINES.map(l => JSON.parse(l)), COPY_EVENTS = COPY_LINES.map(l => JSON.parse(l));
const run = repoLines('media/raw/trace-proof-run.txt');
const verify = repoLines('media/raw/trace-proof-verify.txt');
const refusal = repoLines('media/raw/trace-proof-verify-tampered.txt');
const EDIT = JSON.parse(readRepo('media/raw/trace-proof-tamper.json'));
const EXITS = JSON.parse(readRepo('media/raw/trace-proof-exits.json'));
const CMP = readRepo('media/raw/trace-proof-cmp.txt').trim().split(/\s+/);

// the chain, recomputed: line 1 links to the tag the engine hashes first,
// every later line to the sha256 of the exact bytes of the line before it
const GENESIS = readRepo('crates/nika-dap/src/chain.rs').match(/CHAIN_GENESIS: &\[u8\] = b"([^"]+)"/)?.[1] ?? fail('no genesis tag in nika-dap');
const HASH = LINES.map(sha256);
if (EVENTS[0].chain !== sha256(GENESIS)) fail('line 1 does not link to the genesis tag');
EVENTS.forEach((e, i) => { if (i && e.chain !== HASH[i - 1]) fail(`line ${i + 1} does not carry the sha256 of line ${i}`); });
const HEAD = HASH[HASH.length - 1];

// what the run printed, and what verify read back
const printed = run.map(l => l.match(/^\s*trace: (\S+) · (\d+) events · chain ([0-9a-f]{64})$/u)).find(Boolean) ?? fail('the run printed no trace line');
const intact = verify.map(l => l.match(/^OK — (\d+) events · (chain intact) · (head) ([0-9a-f]{64})$/u)).find(Boolean) ?? fail('verify does not report the chain intact');
if (+printed[2] !== LINES.length || +intact[1] !== LINES.length) fail('the event counts differ');
if (printed[3] !== HEAD || intact[4] !== HEAD) fail('the head verify read is not the chain the run printed');
if (!run[0].startsWith('rehearsal:')) fail('the run no longer says it is a rehearsal');
if (!verify[0].startsWith('nika trace: reading ')) fail('verify no longer names the trace it reads');

// the copy: one byte differs from the trace, the one the capture recorded
const diff = [];
for (let i = 0; i < Math.max(TRACE.length, COPY.length); i++) if (TRACE[i] !== COPY[i]) diff.push(i);
if (diff.length !== 1 || diff[0] + 1 !== EDIT.byte || +CMP[0] !== EDIT.byte) fail('the copy does not differ in exactly the recorded byte');
if (TRACE[diff[0]] !== EDIT.from.charCodeAt(0) || COPY[diff[0]] !== EDIT.to.charCodeAt(0)) fail('the recorded byte values are not the bytes');
if (parseInt(CMP[1], 8) !== TRACE[diff[0]] || parseInt(CMP[2], 8) !== COPY[diff[0]]) fail('cmp -l names other bytes');
const K = EDIT.line - 1; // the edited line, 0-based
const field = (e, key) => e.fields.find(f => f.key === key)?.value;
if (EVENTS[K].kind !== EDIT.kind || String(field(EVENTS[K], EDIT.key)) !== EDIT.value_from || String(field(COPY_EVENTS[K], EDIT.key)) !== EDIT.value_to) fail('the edited field is not where the record says');
const COMPUTED = sha256(COPY_LINES[K]);

// what verify said about the copy, and the exit codes
const broken = refusal[0].match(/^(BROKEN at line (\d+)) — (recorded chain) ([0-9a-f]{16}) · (computed) ([0-9a-f]{16})$/u) ?? fail('verify did not refuse the copy');
if (+broken[2] !== K + 2) fail('the refusal is not at the line after the edit');
if (broken[4] !== EVENTS[K + 1].chain.slice(0, 16) || broken[6] !== COMPUTED.slice(0, 16)) fail('the refusal names other hashes');
const UNVERIFIED = refusal[1]?.trim().match(/^every line from here on is unverified/u)?.[0] ?? fail('the refusal no longer says what it leaves unverified');
if (EXITS.run !== 0 || EXITS.verify !== 0 || EXITS.verify_tampered !== 2) fail('exit codes are not run 0 · verify 0 · refused 2');
const CAVEAT = verify.join('\n').match(/tamper-evident, not tamper-proof/u)?.[0] ?? fail('verify no longer states its caveat');

const short = h => h.slice(0, 16); // verify's own short form
const N = LINES.length;
const RECORDED = broken[3], COMPUTED_W = broken[5];

// ── what each block shows of its line ───────────────────────────────────
const SHOW = { workflow_started: ['workflow', 'engine_version'], task_scheduled: ['task'], task_started: ['task', 'note'], task_completed: ['task', EDIT.key], workflow_completed: ['status', 'tasks_ok'] };
const rowsOf = e => (SHOW[e.kind] ?? []).map(k => [k, field(e, k)]).filter(([, v]) => v !== undefined).map(([k, v]) => [k, String(v)]);
// a field's key, a little smaller than its value, and the value cut to the
// room left in the card: a verbatim prefix and an ellipsis, never a rewrite
const KEY_ST = { f: 'MM 400', size: 13.5 }, VAL_ST = { f: 'MM 400', size: 15.5 };
const fit = (v, room) => {
  const n = Math.floor(room / cw(VAL_ST));
  return v.length > n ? `${v.slice(0, n - 1)}…` : v;
};

// ── layout ──────────────────────────────────────────────────────────────
const X0 = 96, G = 48, HW = 244;
const BW = Math.floor((1856 - X0 - HW - N * G) / N);
const BH = 250, BY = 330;
const bx = i => X0 + i * (BW + G); // block i; i = N is the head
const HX = bx(N);
const SPINE = BY + 29;
const HASH_ST = { f: 'MM 500', size: 18 };
const ADV = cw(HASH_ST);
const LABEL = { f: 'MGW 500', size: 10.5, tracking: 2.2 };
const TERM = { x: 64, y: 688, w: 1792, h: 328 };
const TST = { f: 'MM 400', size: 16 };
const TLH = 24;

const T = {
  term: 0.2, cmdRun: 0.3, outRun: 0.95,
  ignite: 1.0, land: 1.12, landEvery: 0.17,
  fly: 2.15, flyEnd: 2.65,
  clear: 4.2, cmdVerify: 4.35, reading: 5.05,
  scan: 5.1, scanFast: 0.95,
  lens: 6.2, chip: 6.3, dock: 7.85, match: 8.05,
  lensH: 8.1, chipH: 8.2, dockH: 9.25, matchH: 9.45,
  ok: 9.5,
  edit: 11.3, avalanche: 11.62, crack: 12.25,
  cmdCopy: 13.5, scan2: 14.45, snap: 15.55, refused: 15.65,
  // where the camera arrives: in on the last links, back, in on the break,
  // back for the settled end
  in1: 6.6, out1: 13.9, in2: 16.45, settle: 19.75,
};
const landT = i => T.land + i * T.landEvery;

// ── camera ──────────────────────────────────────────────────────────────
const S1 = frameBox({ x: bx(K) - 10, y: BY - 142, w: HX + HW - bx(K) + 20, h: BH + 142 + 66 }, 0);
const S3 = frameBox({ x: bx(K) - 10, y: BY - 150, w: 2 * BW + G + 40, h: BH + 150 + 76 }, 0);
const SHOTS = [
  { at: 0, cam: WIDE },
  { at: T.in1, cam: S1, move: 0.7 },
  { at: T.out1, cam: WIDE, move: 0.7 },
  { at: T.in2, cam: S3, move: 0.7 },
  { at: T.settle, cam: WIDE, move: 0.7 },
];
export const camera = t => cameraPath(t, SHOTS);

export function env() {
  return { bgGlow: 1, gridAlpha: 0.2, gridX: 0, gridY: 0, bgX: 1100, bgY: 450 };
}

// ── chrome: the frame and a kinetic headline ────────────────────────────
const HEAD_ST = { f: 'Geist 600', size: 58, tracking: -58 * 0.028 };
const TITLE = [['Change', C.ink], ['one', C.ink], ['byte.', C.ink], ['Verify', C.teal], ['says', C.teal], ['so.', C.teal]];
function title(R, t) {
  let x = 64;
  const space = measure(' ', HEAD_ST);
  TITLE.forEach(([w, col], i) => {
    const t0 = 0.15 + i * 0.06 + (i >= 3 ? 0.16 : 0);
    const k = E.snap(seg(t, t0, t0 + 0.6));
    // the promise glows once as the camera settles on the refusal
    const pulse = col === C.teal ? 0.5 * Math.sin(Math.PI * seg(t, T.settle + (i - 3) * 0.08, T.settle + 0.8 + (i - 3) * 0.08)) : 0;
    text(R, w, x, 172 + 20 * (1 - k), { ...HEAD_ST, color: col, alpha: smooth(t0, t0 + 0.22, t), glow: (col === C.teal ? 0.4 : 0.18) + pulse });
    x += measure(w, HEAD_ST) + space;
  });
  text(R, 'EACH LINE OF A RUN\'S TRACE CARRIES THE SHA256 OF THE LINE BEFORE IT', 66, 206, { f: 'MGW 500', size: 10.5, tracking: 3, color: C.teal, alpha: 0.9 * smooth(0.75, 1.1, t) });
}

// While the camera is in, the chain passes under the kicker and the plate:
// a band of the background colour at each edge, nearly opaque at the frame
// and clear 110 px in, keeps them on a clean ground. In the glow pass the
// bands erase light, so nothing under them shines through.
const BAND = 110;
function bands(R, k) {
  if (k <= 0.002) return;
  const ctx = R.ctx;
  ctx.save();
  if (R.glowPass) ctx.globalCompositeOperation = 'destination-out';
  for (const [edge, inner] of [[0, BAND], [DH, DH - BAND]]) {
    const g = ctx.createLinearGradient(0, edge, 0, inner);
    g.addColorStop(0, rgba(C.bg0, 0.95 * k));
    g.addColorStop(1, rgba(C.bg0, 0));
    ctx.fillStyle = g;
    ctx.fillRect(0, Math.min(edge, inner), DW, BAND);
  }
  ctx.restore();
}

export function chrome(R, t) {
  const a = loopFade(t, meta.duration);
  const cam = camera(t);
  const scrim = smooth(1, 1.12, cam.s);
  bands(R, a * scrim);
  frame(R, t, { kicker: 'the proof · a run\'s hash-chained trace', plate: `run, trace and both verifies from the real cli · nika ${NIKA_VERSION} · a mock/echo rehearsal · one byte changed in a copy · the scan is an illustration`, alpha: a, scrim });
  title({ ...R, fade: a * titleFade(cam) }, t);
}

// ── small drawing helpers ───────────────────────────────────────────────
// A vertical gradient inside a rounded box (main pass only).
function sheen(R, x, y, w, h, r, color, a0, a1, alpha = 1) {
  if (R.glowPass) return;
  const a = alpha * (R.fade ?? 1);
  if (a <= 0.002) return;
  const ctx = R.ctx;
  const g = ctx.createLinearGradient(0, y, 0, y + h);
  g.addColorStop(0, rgba(color, a0 * a));
  g.addColorStop(1, rgba(color, a1 * a));
  ctx.save();
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, r);
  ctx.fillStyle = g;
  ctx.fill();
  ctx.restore();
}

// A hash that resolves left to right: a wave of scrambling cells runs
// over it, leaving the new value behind and eating the old one (`from`)
// ahead of it. Each part is one string, so no lone glyph lingers.
const HEX = '0123456789abcdef';
function hexWave(R, t, to, x, y, { from = null, t0 = 0, dur = 0.12, stagger = 0.03, st = HASH_ST, color = C.ink, fromColor = color, hot = C.cyan, alpha = 1, glow = 0 } = {}) {
  if (alpha <= 0) return;
  const n = to.length, adv = cw(st), f = Math.floor(t * 30);
  let done = 0, begun = 0;
  while (done < n && t >= t0 + done * stagger + dur) done++;
  while (begun < n && t >= t0 + begun * stagger) begun++;
  if (done) text(R, to.slice(0, done), x, y, { ...st, color, alpha, glow });
  if (begun > done) {
    let noise = '';
    for (let i = done; i < begun; i++) noise += HEX[Math.floor(hash(i, f, 7) * 16)];
    text(R, noise, x + done * adv, y, { ...st, color: hot, alpha, glow: glow + 0.4 });
  }
  if (from && begun < n) text(R, from.slice(begun), x + begun * adv, y, { ...st, color: fromColor, alpha, glow });
}

// ── the terminal ────────────────────────────────────────────────────────
// The run's lines, its rehearsal notice first; its pointers (explore, see
// it whole) and its echoes of what it wrote and said stay out. Verify's
// lines run to the first rung of its ladder after the chain verdict (here
// UNSEALED: the capture has no signing key), cut to one row where long.
const runShown = run.filter(l => l.trim() && !/^\s*(explore:|see it whole:|said |wrote )/u.test(l));
const rung = verify.findIndex((l, i) => i > 1 && !/^\s/u.test(l));
const verifyShown = verify.slice(0, rung < 0 ? verify.length : rung + 1);
const WRAP = /^rehearsal:/u;
const STEPS = [
  { t: T.cmdRun, cmd: 'nika run hello.nika', dur: 0.6 },
  { t: T.outRun, out: runShown, every: 0.012, wrap: WRAP, marks: [{ re: /rehearsal/u, c: C.amber }, { re: /chain [0-9a-f]{64}/u, c: C.ice, glow: 0.4 }] },
  { t: T.clear, clear: true },
  { t: T.cmdVerify, cmd: 'nika trace verify', dur: 0.6 },
  { t: T.reading, out: verifyShown.slice(0, 1) },
  { t: T.ok, out: verifyShown.slice(1), every: 0.012, marks: [{ re: /OK — .*chain intact/u, c: C.teal, glow: 0.5 }, { re: /tamper-evident, not tamper-proof/u, c: C.ink }] },
  { t: T.cmdCopy, cmd: `nika trace verify ${EDIT.file}`, dur: 0.85 },
  { t: T.refused, out: refusal, every: 0.012, marks: [{ re: /BROKEN at line \d+/u, c: C.red, glow: 0.6 }, { re: /every line from here on is unverified/u, c: C.red }] },
];

// where the run's printed chain sits in the terminal (it flies to the head)
const RUN_ROWS = layoutRows(runShown, { box: TERM, st: TST, wrap: WRAP });
const TRACE_ROW = RUN_ROWS.findIndex(r => r.spans.map(s => s.s).join('').includes(HEAD));
const TRACE_TEXT = RUN_ROWS[TRACE_ROW].spans.map(s => s.s).join('');
const FLY_FROM = { x: TERM.x + 24 + TRACE_TEXT.indexOf(HEAD) * cw(TST), y: TERM.y + 44 + 30 + (TRACE_ROW + 1) * TLH };

function term(R, t, a) {
  const badge = t < T.clear ? { label: 'MOCK/ECHO · A REHEARSAL', color: C.amber, alpha: smooth(T.outRun, T.outRun + 0.3, t) * (1 - smooth(T.clear - 0.25, T.clear, t)) }
    : t >= T.refused ? { label: `EXIT ${EXITS.verify_tampered} · BROKEN`, color: C.red, alpha: smooth(T.refused, T.refused + 0.3, t) }
      : t >= T.ok ? { label: `EXIT ${EXITS.verify} · CHAIN INTACT`, color: C.teal, alpha: smooth(T.ok, T.ok + 0.3, t) * (1 - smooth(T.cmdCopy - 0.2, T.cmdCopy, t)) } : null;
  terminal(R, t, TERM, STEPS, { title: 'terminal · offline', alpha: a, k: E.snap(seg(t, T.term, T.term + 0.45)), st: TST, lh: TLH, badge });
}

// ── the scan ────────────────────────────────────────────────────────────
// Scanner x over time: pass 1 reads the whole chain (slowing where the
// camera is in), pass 2 reads the copy and stops at the break.
// the ring rests over a line's right side, clear of the hash it recorded
const REST = BW * 0.9;
const SCAN1 = [[T.scan, X0 - 40], [T.scan + T.scanFast, bx(K) + REST], [T.dock, bx(K) + REST], [T.dock + 0.3, bx(K + 1) + REST], [T.dockH, bx(K + 1) + REST], [T.dockH + 0.3, HX + HW / 2]];
const SCAN2 = [[T.scan2, X0 - 40], [T.scan2 + 0.85, bx(K) + REST], [T.snap - 0.2, bx(K) + REST], [T.snap, bx(K) + BW + G / 2]];
const SCAN1_END = T.matchH + 0.15, SCAN2_END = T.snap + 0.45;
function track(t, keys) {
  if (t < keys[0][0]) return null;
  for (let i = 1; i < keys.length; i++) {
    const [t0, x0] = keys[i - 1], [t1, x1] = keys[i];
    if (t <= t1) return lerp(x0, x1, E.inOutSine(seg(t, t0, t1)));
  }
  return keys[keys.length - 1][1];
}
// when the scanner crosses x (the inverse of track's eased segments)
function crossT(keys, x) {
  for (let i = 1; i < keys.length; i++) {
    const [t0, x0] = keys[i - 1], [t1, x1] = keys[i];
    if (x1 > x0 && x >= x0 && x <= x1) return lerp(t0, t1, Math.acos(1 - 2 * (x - x0) / (x1 - x0)) / Math.PI);
  }
  return Infinity;
}
const linkX = i => bx(i) - G / 2;
// the moment each link (into node i; node N is the head) is verified
const pass1 = i => (i === K + 1 ? T.match : i === N ? T.matchH : crossT(SCAN1, linkX(i)));
const pass2 = i => (i <= K ? crossT(SCAN2, linkX(i)) : Infinity);

// link states: teal once verified · amber once the copy's hash no longer
// matches · red once verify refuses it · dead past the break
function linkState(t, i) {
  if (t < T.edit) return { v: smooth(pass1(i), pass1(i) + 0.15, t), vt: pass1(i) };
  if (i === K + 1) return { amber: smooth(T.crack, T.crack + 0.3, t), red: smooth(T.snap, T.snap + 0.08, t), dead: smooth(T.snap + 0.1, T.snap + 0.5, t) };
  if (i > K + 1) return { dead: smooth(T.snap + 0.1, T.snap + 0.5, t) };
  return { v: smooth(pass2(i), pass2(i) + 0.15, t), vt: pass2(i) };
}

// once verify refuses the copy, the part of the chain it can no longer
// vouch for drops away from the rest: node i's offset (node N is the head)
const APART = { x: 18, y: 12 };
function apart(t, i) {
  if (i <= K || t < T.snap) return [0, 0];
  const k = E.outBack(seg(t, T.snap, T.snap + 0.7));
  return [APART.x * k, APART.y * k];
}

// the scanner: a detector ring around the chain, drawn in two halves so
// the blocks pass through it
const RING = { cy: BY + BH / 2, rx: 26, ry: BH / 2 + 62 };
function scanState(t) {
  for (const [keys, end] of [[SCAN1, SCAN1_END], [SCAN2, SCAN2_END]]) {
    const sx = track(t, keys);
    if (sx === null || t > end) continue;
    const fade = smooth(keys[0][0], keys[0][0] + 0.2, t) * (1 - smooth(end - 0.35, end, t));
    const red = keys === SCAN2 ? smooth(T.snap - 0.05, T.snap + 0.05, t) : 0;
    // the lens flare belongs to the sweep: bright while it moves, low at rest
    const speed = clamp(Math.abs(track(t + 1 / 60, keys) - sx) * 60 / 900);
    return { sx, fade, speed, col: red > 0 ? C.red : C.cyan, spin: t * 1.6 };
  }
  return null;
}
function ringHalf(R, t, a, front) {
  const s = scanState(t);
  if (!s || a * s.fade <= 0) return;
  const al = a * s.fade;
  const pts = [];
  const a0 = front ? -Math.PI / 2 : Math.PI / 2, a1 = a0 + Math.PI;
  for (let k = 0; k <= 40; k++) {
    const an = lerp(a0, a1, k / 40);
    pts.push([s.sx + Math.cos(an) * RING.rx, RING.cy + Math.sin(an) * RING.ry]);
  }
  poly(R, pts, { color: s.col, w: front ? 1.8 : 1, alpha: al * (front ? 0.9 : 0.35), glow: front ? 0.9 : 0.3 });
  if (front) {
    for (let k = 0; k < 24; k++) {
      const an = -Math.PI / 2 + ((k + (s.spin % 1)) / 24) * Math.PI;
      const c = Math.cos(an), sn = Math.sin(an);
      const L = k % 6 === 0 ? 12 : 6;
      line(R, s.sx + c * RING.rx, RING.cy + sn * RING.ry, s.sx + c * (RING.rx + L * 0.5), RING.cy + sn * (RING.ry + L), { color: s.col, w: 1, alpha: al * 0.6 });
    }
    line(R, s.sx, RING.cy - RING.ry + 6, s.sx, RING.cy + RING.ry - 6, { color: s.col, w: 1.2, alpha: al * 0.55, glow: 0.8 });
    light(R, s.sx, SPINE, 80, s.col, (0.12 + 0.25 * s.speed) * al, 1);
    streak(R, s.sx, SPINE, 170, (0.1 + 0.5 * s.speed) * al, s.col, 1);
    text(R, 'NIKA TRACE VERIFY', s.sx, RING.cy - RING.ry - 14, { f: 'MGW 500', size: 9, tracking: 2.4, color: s.col, alpha: al * 0.9 * (1 - smooth(T.snap, T.snap + 0.15, t)), align: 'center' });
  }
}
// how brightly the ring lights a box at x..x+w
function scanOver(t, x, w) {
  const s = scanState(t);
  if (!s) return 0;
  return s.fade * clamp(1 - Math.max(0, x - s.sx, s.sx - (x + w)) / 30);
}

// ── the chain ───────────────────────────────────────────────────────────
function spine(R, t, a) {
  // genesis: the tag the engine hashes before line 1
  const ig = seg(t, T.ignite, T.ignite + 0.7);
  const ga = a * smooth(T.ignite, T.ignite + 0.1, t);
  if (ga > 0) {
    circle(R, X0 - 30, SPINE, 5, { fill: C.ice, alpha: ga, glow: 1 });
    circle(R, X0 - 30, SPINE, 9, { color: C.ice, w: 1, alpha: ga * 0.5 });
    if (ig < 1) {
      streak(R, X0 - 30, SPINE, lerp(80, 520, E.outCubic(ig)), 0.9 * (1 - ig) ** 2 * a, C.ice, 1);
      light(R, X0 - 30, SPINE, 90, C.ice, 0.5 * (1 - ig) * a, 1);
    }
    text(R, 'GENESIS', X0 - 30, SPINE + 30, { f: 'MGW 500', size: 8, tracking: 1.6, color: C.dim, alpha: ga * 0.8, align: 'center' });
  }
  for (let i = 0; i <= N; i++) {
    const x0 = i === 0 ? X0 - 24 : bx(i - 1) + BW, x1 = bx(i);
    const t0 = i < N ? landT(i) : T.fly + 0.2;
    const draw = E.inOutCubic(seg(t, t0 - 0.15, t0 + 0.15));
    if (draw <= 0) continue;
    const s = linkState(t, i);
    if (i === K + 1 && s.red > 0) { snapped(R, t, x0, x1, apart(t, i), a); continue; }
    const [ox, oy] = apart(t, i);
    const on = s.amber || s.v || 0;
    const col = s.amber ? C.amber : s.v ? C.teal : C.ice;
    const la = a * (s.dead ? lerp(0.8, 0.15, s.dead) : 0.8 + 0.2 * on);
    const y = SPINE + oy;
    line(R, x0 + ox, y, lerp(x0, x1, draw) + ox, y, { color: col, w: 1.8 + on, alpha: la, glow: 0.35 + on * 0.5 });
    // couplers on the block edges
    circle(R, x0 + ox, y, 3.2, { fill: col, alpha: la, glow: 0.6 });
    if (draw >= 1) circle(R, x1 + ox, y, 3.2, { fill: col, alpha: la, glow: 0.6 });
    // the pulse that crosses a link as it verifies
    if (s.vt !== undefined && t >= s.vt - 0.18 && t < s.vt + 0.05) {
      const p = seg(t, s.vt - 0.18, s.vt);
      circle(R, lerp(x0, x1, E.inOutCubic(p)), SPINE, 4.5, { fill: C.teal, alpha: a, glow: 1 });
    }
    // verify's OK runs one confirmation pulse through the links into the head
    const cp = seg(t, T.ok, T.ok + 0.45);
    if (t < T.edit && cp > 0 && cp < 1 && i > K) {
      const px = lerp(bx(K), HX, E.inOutCubic(cp));
      if (px >= x0 - 4 && px <= x1 + 4) circle(R, px, y, 5, { fill: C.teal, alpha: a, glow: 1 });
    }
    // a hairline crack once the copy's hash no longer matches
    if (s.amber > 0) {
      const cx = (x0 + x1) / 2;
      poly(R, [[cx - 4, SPINE - 13], [cx + 2, SPINE - 3], [cx - 2, SPINE + 3], [cx + 4, SPINE + 14]], { color: C.amber, w: 1.5, alpha: a * s.amber, glow: 0.8 });
    }
  }
}

// the broken link: two halves recoil, sparks fly, a ring goes out, and a
// fracture stays in the gap
function snapped(R, t, x0, x1, [ox, oy], a) {
  const u = seg(t, T.snap, T.snap + 0.7);
  const k = E.outBack(clamp(u * 1.4));
  const xr = x1 + ox, yr = SPINE + oy;
  const mid = (x0 + xr) / 2, my = SPINE + oy / 2, half = (x1 - x0) / 2;
  const ang = 0.6 * k, len = half * (1 - 0.3 * k);
  const L = [x0 + Math.cos(ang) * len, SPINE + Math.sin(ang) * len];
  const Rr = [xr - Math.cos(ang) * len, yr + Math.sin(ang) * len];
  line(R, x0, SPINE, L[0], L[1], { color: C.red, w: 2.6, alpha: a, glow: 0.9 });
  line(R, xr, yr, Rr[0], Rr[1], { color: C.red, w: 2.6, alpha: a, glow: 0.9 });
  circle(R, L[0], L[1], 3.5, { fill: C.red, alpha: a, glow: 1 });
  circle(R, Rr[0], Rr[1], 3.5, { fill: C.red, alpha: a, glow: 1 });
  circle(R, x0, SPINE, 3.2, { fill: C.red, alpha: a, glow: 0.6 });
  circle(R, xr, yr, 3.2, { fill: C.red, alpha: a, glow: 0.6 });
  // the gap keeps a fracture and a low red light: the chain no longer holds
  const fk = E.snap(seg(t, T.snap, T.snap + 0.25));
  const F = [[-6, -34], [3, -14], [-3, -4], [5, 10], [-2, 20], [4, 34]];
  const beat = u >= 1 ? 0.5 + 0.5 * Math.sin(TAU * (t - T.snap - 0.7) / 1.4) : 1;
  poly(R, F.map(([dx, dy]) => [mid + dx, my + dy * fk]), { color: C.red, w: 2, alpha: a, glow: 0.7 + 0.3 * beat });
  light(R, mid, my, 70, C.red, (0.2 + 0.12 * beat) * a, 0.9);
  if (u < 1) {
    for (let j = 0; j < 12; j++) {
      const an = (j / 12) * TAU + hash(j, 3) * 0.5;
      const sp = 0.6 + hash(j, 4) * 0.8;
      const r0 = 5 + 70 * sp * E.outCubic(u), r1 = r0 + 16 * (1 - u);
      line(R, mid + Math.cos(an) * r0, my + Math.sin(an) * r0, mid + Math.cos(an) * r1, my + Math.sin(an) * r1, { color: j % 3 ? C.red : C.gold, w: 1.6, alpha: a * (1 - u), glow: 1 });
    }
    circle(R, mid, my, 10 + 110 * E.outCubic(u), { color: C.red, w: 1.6, alpha: a * (1 - u) * 0.8, glow: 1 });
    light(R, mid, my, 200, C.red, 0.7 * (1 - u) ** 2 * a, 1);
  }
}

// a glass card: the body, a sheen, a top highlight
function card(R, x, y, w, h, { border, bw = 1.2, alpha, glow = 0, dash = null }) {
  rrect(R, x, y, w, h, 14, { fill: '#050c19', fillAlpha: 0.96, alpha });
  sheen(R, x, y, w, h, 14, C.ice, 0.07, 0.0, alpha);
  rrect(R, x, y, w, h, 14, { color: border, w: bw, alpha, glow, dash });
  line(R, x + 16, y + 0.8, x + w - 16, y + 0.8, { color: C.ice, w: 1, alpha: alpha * 0.22 });
}

function block(R, t, i, a) {
  const lt = landT(i);
  const k = E.snap(seg(t, lt, lt + 0.5));
  if (k <= 0) return;
  const s = linkState(t, i);
  const dead = s.dead || 0;
  const ok = (s.v || 0) * (1 - dead);
  const edited = i === K && t >= T.edit;
  const reading = scanOver(t, bx(i), BW);
  // the refused line shudders once when its link breaks
  const shake = i === K + 1 && t >= T.snap && t < T.snap + 0.35 ? Math.sin((t - T.snap) * 60) * 6 * (1 - seg(t, T.snap, T.snap + 0.35)) : 0;
  const [ox, oy] = apart(t, i);
  const x = bx(i) + shake + ox, y = BY - 50 * (1 - k) + oy;
  const flicker = dead > 0 && dead < 1 ? (hash(Math.floor(t * 30), i) > 0.5 ? 1 : 0.4) : 1;
  const fa = a * smooth(lt, lt + 0.2, t) * lerp(1, 0.42, dead) * flicker;
  // the refused line's recorded chain stays legible: verify names it
  const fb = i === K + 1 ? a * smooth(lt, lt + 0.2, t) * lerp(1, 0.9, dead) * flicker : fa;
  const border = dead > 0 ? C.red : edited ? C.amber : ok > 0.5 ? C.teal : reading > 0.2 ? C.ice : C.faint;
  card(R, x, y, BW, BH, { border, bw: 1.2 + 0.7 * Math.max(ok, reading), alpha: fa, glow: 0.3 * Math.max(ok, reading, edited ? 1 : 0), dash: dead > 0.5 ? [7, 5] : null });
  if (reading > 0) sheen(R, x, y, BW, BH, 14, C.cyan, 0.1 * reading, 0.02 * reading, fa);
  // the line number, large and faint
  text(R, String(i + 1), x + BW - 16, y + BH - 16, { f: 'Geist 700', size: 120, tracking: -4, color: dead > 0 ? C.red : C.ice, alpha: fa * 0.07, align: 'right' });
  // the chain band: what this line recorded about the line before it
  const band = dead > 0 ? C.red : ok > 0.5 ? C.teal : C.ice;
  rrect(R, x + 1, y + 1, BW - 2, 57, 13, { fill: band, fillAlpha: 0.05 + 0.06 * ok, alpha: fa });
  line(R, x + 1, y + 58, x + BW - 1, y + 58, { color: C.line, w: 1, alpha: fa });
  text(R, RECORDED, x + 16, y + 21, { ...LABEL, color: dead > 0 ? C.red : C.dim, alpha: fb });
  hexWave(R, t, short(EVENTS[i].chain), x + 16, y + 46, { t0: lt + 0.05, dur: 0.1, stagger: 0.012, color: band, alpha: fb, glow: 0.25 + 0.35 * ok });
  if (s.vt !== undefined && ok > 0) check(R, x + BW - 20, y + 17, 11, E.snap(seg(t, s.vt, s.vt + 0.3)), { color: C.teal, w: 2, alpha: fa, glow: 0.9 });
  // the line itself
  text(R, `LINE ${i + 1}`, x + 16, y + 92, { ...LABEL, tracking: 3, color: C.dim, alpha: fa });
  text(R, EVENTS[i].kind, x + 16, y + 124, { f: 'MM 500', size: 18, color: C.ink, alpha: fa });
  rowsOf(EVENTS[i]).forEach(([key, v], r) => {
    const yy = y + 162 + r * 28;
    const vx = x + 16 + measure(key, KEY_ST) + 10;
    text(R, key, x + 16, yy, { ...KEY_ST, color: C.dim, alpha: fa });
    if (i === K && key === EDIT.key) editedValue(R, t, vx, yy, VAL_ST, fa);
    else text(R, fit(v, x + BW - 16 - vx), vx, yy, { ...VAL_ST, color: C.mist, alpha: fa });
  });
  if (edited) editFlash(R, t, x, y, fa);
}

// the edit lands on the whole card: a flash and a few displaced slices
function editFlash(R, t, x, y, a) {
  const g = seg(t, T.edit, T.edit + 0.45);
  if (g >= 1) return;
  const f = Math.floor(t * 30);
  sheen(R, x, y, BW, BH, 14, C.amber, 0.22 * (1 - g), 0.05 * (1 - g), a);
  for (let j = 0; j < 4; j++) {
    const yy = y + 60 + hash(f, j, 1) * (BH - 70);
    rect(R, x + (hash(f, j, 2) - 0.5) * 30, yy, BW * (0.3 + hash(f, j, 3) * 0.6), 1.5 + hash(f, j, 4) * 3, { fill: j % 2 ? C.red : C.cyan, alpha: a * 0.5 * (1 - g), glow: 1 });
  }
}

// the edited value: the original until the edit, a glitch, then the copy's
function editedValue(R, t, x, y, st, a) {
  const adv = cw(st);
  const from = EDIT.value_from, to = EDIT.value_to;
  if (t < T.edit) { text(R, from, x, y, { ...st, color: C.mist, alpha: a }); return; }
  if (to.length > 1) text(R, to.slice(1), x + adv, y, { ...st, color: C.mist, alpha: a });
  const g = seg(t, T.edit, T.edit + 0.45);
  if (g < 1) {
    // the one byte flickers between its two values with a chromatic split
    const f = Math.floor(t * 30);
    const ch = hash(f, 5) < 0.5 ? from[0] : to[0];
    const j = (hash(f, 9) - 0.5) * 10 * (1 - g);
    const ctx = R.ctx;
    ctx.save();
    ctx.globalCompositeOperation = 'lighter';
    text(R, ch, x - 3 + j, y, { ...st, color: C.red, alpha: a * 0.9, glow: 0.9 });
    text(R, ch, x + 3 + j, y, { ...st, color: C.cyan, alpha: a * 0.9, glow: 0.9 });
    ctx.restore();
    text(R, ch, x + j * 0.3, y, { ...st, color: C.ink, alpha: a });
  } else {
    const pop = 1 + 0.35 * (1 - E.outBack(seg(t, T.edit + 0.45, T.edit + 0.8)));
    rrect(R, x - 4, y - 16, adv + 8, 22, 4, { color: C.amber, w: 1.3, alpha: a, fill: C.amber, fillAlpha: 0.14, glow: 0.6 });
    const ctx = R.ctx;
    ctx.save();
    ctx.translate(x + adv / 2, y - 5);
    ctx.scale(pop, pop);
    text(R, to[0], -adv / 2, 5, { ...st, f: 'MM 500', color: C.amber, alpha: a, glow: 0.7 });
    ctx.restore();
  }
}

// the head: the chain the run printed, which verify's last hash must meet
function headCard(R, t, a) {
  const k = E.snap(seg(t, T.fly - 0.1, T.fly + 0.4));
  if (k <= 0) return;
  const s = linkState(t, N);
  const ok = s.v || 0, dead = s.dead || 0;
  const [ox, oy] = apart(t, N);
  const x = HX + ox, y = BY - 50 * (1 - k) + oy;
  const flicker = dead > 0 && dead < 1 ? (hash(Math.floor(t * 30), 9) > 0.5 ? 1 : 0.4) : 1;
  const fa = a * smooth(T.fly - 0.1, T.fly + 0.15, t) * lerp(1, 0.42, dead) * flicker;
  card(R, x, y, HW, BH, { border: dead > 0 ? C.red : ok > 0.5 ? C.teal : C.faint, bw: 1.2 + 0.7 * ok, alpha: fa, glow: 0.3 * ok, dash: dead > 0.5 ? [7, 5] : null });
  rrect(R, x + 1, y + 1, HW - 2, 57, 13, { fill: ok > 0.5 ? C.teal : C.ice, fillAlpha: 0.05 + 0.06 * ok, alpha: fa });
  line(R, x + 1, y + 58, x + HW - 1, y + 58, { color: C.line, w: 1, alpha: fa });
  text(R, 'THE RUN PRINTED', x + 16, y + 21, { ...LABEL, color: C.dim, alpha: fa });
  const landed = smooth(T.flyEnd - 0.06, T.flyEnd + 0.06, t);
  text(R, short(HEAD), x + 16, y + 46, { ...HASH_ST, color: ok > 0.5 ? C.teal : C.ice, alpha: fa * landed, glow: 0.3 + 0.3 * ok });
  if (ok > 0) check(R, x + HW - 20, y + 17, 11, E.snap(seg(t, T.matchH, T.matchH + 0.3)), { color: C.teal, w: 2, alpha: fa, glow: 0.9 });
  text(R, 'HEAD', x + 16, y + 100, { f: 'MGW 700', size: 22, tracking: 7, color: ok > 0.5 ? C.teal : C.ink, alpha: fa, glow: 0.2 + 0.3 * ok });
  text(R, 'the chain\'s last link', x + 16, y + 130, { f: 'MM 400', size: 14, color: C.dim, alpha: fa });
}

// the run's printed chain flies from the terminal to the head card
function fly(R, t, a) {
  const u = seg(t, T.fly, T.flyEnd);
  if (u <= 0 || u >= 1) return;
  const p = E.inOutCubic(u);
  const x = lerp(FLY_FROM.x, HX + 16, p), y = lerp(FLY_FROM.y, BY + 46, p) - Math.sin(p * Math.PI) * 70;
  const size = lerp(TST.size, HASH_ST.size, p);
  const fa = a * smooth(0, 0.12, u) * (1 - smooth(0.9, 1, u));
  const w = cw({ ...HASH_ST, size }) * 16;
  // a short light trail behind the chip
  const q = E.inOutCubic(clamp(u - 0.12));
  line(R, lerp(FLY_FROM.x, HX + 16, q) + w / 2, lerp(FLY_FROM.y, BY + 46, q) - Math.sin(q * Math.PI) * 70 - size * 0.35, x + w / 2, y - size * 0.35, { color: C.ice, w: 2, alpha: fa * 0.5, glow: 1 });
  rrect(R, x - 10, y - size - 6, w + 20, size + 16, 9, { color: C.ice, w: 1.2, alpha: fa, fill: '#081830', fillAlpha: 0.95, glow: 0.6 });
  text(R, short(HEAD), x, y, { ...HASH_ST, size, color: C.ice, alpha: fa, glow: 0.7 });
}

// a computed hash in a chip above a node
const CHIP_H = 56, CHIP_Y = BY - CHIP_H - 14;
// `shell` fades the chip's frame and label apart from its hash, so a
// docking hash can land on the recorded one alone
function chipAt(R, t, x, w, label, value, { from = null, t0, dur = 0.12, stagger = 0.03, color = C.ice, hot = color, alpha = 1, shell = 1 }) {
  if (alpha <= 0) return;
  const sa = alpha * shell;
  rrect(R, x, CHIP_Y, w, CHIP_H, 12, { color, w: 1.4, alpha: sa, fill: '#081830', fillAlpha: 0.97, glow: 0.45 });
  sheen(R, x, CHIP_Y, w, CHIP_H, 12, color, 0.14, 0.02, sa);
  text(R, label, x + 16, CHIP_Y + 20, { f: 'MGW 500', size: 10, tracking: 1.2, color, alpha: sa });
  hexWave(R, t, value, x + 16, CHIP_Y + 44, { from, t0, dur, stagger, color: C.ink, fromColor: C.mist, hot, alpha, glow: 0.35 });
}

// one tick per character between a computed hash and a recorded one:
// teal where they agree, red where they differ
function compareTicks(R, t, x, got, want, t0, a) {
  for (let i = 0; i < 16; i++) {
    const k = smooth(t0 + i * 0.015, t0 + i * 0.015 + 0.12, t) * a;
    if (k <= 0) continue;
    const cx = x + 16 + i * ADV + ADV / 2;
    line(R, cx, CHIP_Y + CHIP_H + 3, cx, BY - 3, { color: got[i] === want[i] ? C.teal : C.red, w: 1.6, alpha: k, glow: 0.7 });
  }
}

// a computed hash rises out of the line it hashes, crosses to the next
// node and meets what that node recorded; on a match it settles into it
function travelling(R, t, a, { from, to, w, value, lens, chip, dock, match, label }) {
  if (t < lens || t >= match + 0.3) return;
  const rise = E.snap(seg(t, lens, chip + 0.3));
  const slide = E.inOutCubic(seg(t, chip + 0.3, chip + 0.75));
  const merge = E.inCubic(seg(t, dock, match));
  const x = lerp(from, to, slide);
  const col = t >= match - 0.05 ? C.teal : C.ice;
  const ctx = R.ctx;
  ctx.save();
  ctx.translate(0, 46 * (1 - rise) + (CHIP_H + 14 + 3) * merge);
  chipAt(R, t, x, w, label, value, { t0: lens + 0.05, stagger: 0.025, color: col, alpha: a * rise * (1 - smooth(match, match + 0.3, t)), shell: 1 - smooth(dock, dock + 0.12, t) });
  ctx.restore();
  // a tether back to the line it came from, while it travels
  const tk = a * smooth(chip, chip + 0.2, t) * (1 - smooth(dock - 0.2, dock, t));
  if (tk > 0) poly(R, [[from + BW / 2, BY], [from + BW / 2, CHIP_Y + CHIP_H / 2], [x, CHIP_Y + CHIP_H / 2]], { color: C.ice, w: 1, alpha: tk * 0.5, dash: [3, 4] });
  if (t >= chip + 0.8 && t < dock) compareTicks(R, t, to, value, value, chip + 0.85, a);
}

function chips(R, t, a) {
  // pass 1 · the line before the break, then the last line into the head
  travelling(R, t, a, { from: bx(K), to: bx(K + 1), w: BW, value: short(HASH[K]), lens: T.lens, chip: T.chip, dock: T.dock, match: T.match, label: `${COMPUTED_W} · sha256(line ${K + 1})` });
  travelling(R, t, a, { from: bx(N - 1), to: HX, w: HW, value: short(HASH[N - 1]), lens: T.lensH, chip: T.chipH, dock: T.dockH, match: T.matchH, label: `${COMPUTED_W} · sha256(line ${N})` });
  // the edit · the same hash over the copy: a wave leaves every cell new
  if (t >= T.edit + 0.25) {
    const ea = a * smooth(T.edit + 0.25, T.edit + 0.5, t);
    const red = smooth(T.snap - 0.05, T.snap + 0.1, t);
    const col = red > 0 ? C.red : t >= T.crack ? C.amber : C.ice;
    const shake = red > 0 && t < T.snap + 0.35 ? Math.sin((t - T.snap) * 70) * 6 * (1 - seg(t, T.snap, T.snap + 0.35)) : 0;
    const drop = E.snap(seg(t, T.edit + 0.25, T.edit + 0.6));
    const [ox, oy] = apart(t, K + 1);
    const ctx = R.ctx;
    ctx.save();
    ctx.translate(ox, oy);
    if (t >= T.crack) compareTicks(R, t, bx(K + 1), short(COMPUTED), short(EVENTS[K + 1].chain), T.crack, ea);
    ctx.translate(shake, -30 * (1 - drop));
    chipAt(R, t, bx(K + 1), BW, `${COMPUTED_W} · sha256(line ${K + 1})`, short(COMPUTED), { from: short(HASH[K]), t0: T.avalanche, dur: 0.14, stagger: 0.032, color: col, hot: C.amber, alpha: ea });
    ctx.restore();
  }
}

// ── verify's own words, placed where they apply ─────────────────────────
// A verdict lands like a stamp: a short scale-down onto its place.
function stamp(R, t, t0, x, y, draw) {
  const k = seg(t, t0, t0 + 0.4);
  const s = 1 + 0.35 * (1 - E.outBack(k));
  const ctx = R.ctx;
  ctx.save();
  ctx.translate(x, y);
  ctx.scale(s, s);
  draw();
  ctx.restore();
}

function annotations(R, t, a) {
  // pass 1 · the verdict and its caveat, over the head
  const ia = a * smooth(T.ok, T.ok + 0.3, t) * (1 - smooth(T.edit, T.edit + 0.3, t));
  if (ia > 0) {
    const x = HX + HW;
    stamp(R, t, T.ok, x, BY - 126, () => text(R, intact[2], 0, 8, { f: 'MM 500', size: 24, color: C.teal, alpha: ia, align: 'right', glow: 0.5 }));
    text(R, CAVEAT, x, BY - 92, { f: 'MM 400', size: 14, color: C.mist, alpha: ia * smooth(T.ok + 0.15, T.ok + 0.4, t), align: 'right' });
  }
  // the edit · one byte, on the line that changed
  const ea = a * smooth(T.edit + 0.4, T.edit + 0.6, t);
  if (ea > 0) {
    const x = bx(K) + 16, y = BY + BH + 36;
    text(R, `ONE BYTE · ${EDIT.from} → ${EDIT.to}`, x, y, { f: 'MGW 500', size: 12, tracking: 2.5, color: C.amber, alpha: ea, glow: 0.3 });
    text(R, `byte ${EDIT.byte} of ${EDIT.file}`, x, y + 24, { f: 'MM 400', size: 14, color: C.dim, alpha: ea });
  }
  // pass 2 · the refusal, at the link it names
  const ra = a * smooth(T.refused, T.refused + 0.3, t);
  if (ra > 0) {
    const [ox, oy] = apart(t, K + 1);
    const cx = bx(K + 1) - G / 2 + ox / 2;
    const VST = { f: 'MM 500', size: 34 };
    stamp(R, t, T.refused, cx, BY - 120, () => text(R, broken[1], 0, 12, { ...VST, color: C.red, alpha: ra, align: 'center', glow: 0.6 }));
    const px = cx + measure(broken[1], VST) / 2 + 18;
    rrect(R, px, BY - 136, 88, 32, 16, { color: C.red, w: 1.3, alpha: ra, fill: C.red, fillAlpha: 0.12, glow: 0.4 });
    text(R, `EXIT ${EXITS.verify_tampered}`, px + 44, BY - 115, { f: 'MGW 700', size: 12.5, tracking: 2.5, color: C.red, alpha: ra, align: 'center' });
    text(R, `${UNVERIFIED} …`, bx(K + 1) + 16 + ox, BY + BH + 36 + oy, { f: 'MM 400', size: 15, color: C.red, alpha: ra * 0.95 });
  }
}

// the file the chain is drawn from, above its first blocks
function fileTag(R, t, a) {
  const ta = a * smooth(T.land, T.land + 0.3, t);
  if (ta <= 0) return;
  const copy = smooth(T.edit, T.edit + 0.2, t);
  const y = BY - 24;
  text(R, 'THE TRACE', X0, y, { ...LABEL, color: C.dim, alpha: ta * (1 - copy) });
  text(R, printed[1], X0 + 110, y, { f: 'MM 400', size: 14, color: C.mist, alpha: ta * (1 - copy) });
  text(R, 'THE COPY', X0, y, { ...LABEL, color: C.amber, alpha: ta * copy });
  text(R, `${EDIT.file} · one byte changed`, X0 + 110, y, { f: 'MM 400', size: 14, color: C.amber, alpha: ta * copy });
}

export function draw(R, t) {
  const a = loopFade(t, meta.duration);
  term(R, t, a);
  fileTag(R, t, a);
  ringHalf(R, t, a, false);
  spine(R, t, a);
  for (let i = 0; i < N; i++) block(R, t, i, a);
  headCard(R, t, a);
  ringHalf(R, t, a, true);
  fly(R, t, a);
  chips(R, t, a);
  annotations(R, t, a);
}
