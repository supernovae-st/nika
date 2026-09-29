// spec-anatomy · "What a .nika file is made of."
// One real workflow, scripts/media/fixtures/ship-notes.nika, dissected in
// the language's own words. The whole file shows first as a faint
// specimen; each envelope key then lights up with what the spec the binary
// embeds says it is for (the schema's description of that key, `nika spec
// --schema`, shortened by picking verbatim phrases out of it). Inside
// `tasks:`, the verbs fly to the tasks that bind them, with the canon's
// line for each (`nika spec --canon`), the colour `nika inspect` gives them
// and the glyph `nika try` prints. The real `nika check` of that exact
// file follows, and the file folds to its skeleton: the poster. The file
// drawn is the copy the capture checked; scripts/media/capture/spec-anatomy.sh
// writes it and every other capture here (media/raw/spec-anatomy*); the
// glyphs come from the shared `nika try` capture (media/raw/try-gallery.txt).
// The clip refuses to render when the fixture drifts from what was checked,
// when the file stops showing every key and verb it labels, when a label is
// no longer the spec's wording, or when the check stops saying run ready.
// Illustration: the framing, the lighting and the fold (the file's own
// lines, folded as an editor folds them).
import { C, E, clamp, lerp, seg, smooth, repoLines, readRepo, mono, yamlSpans, frame, headline, loopFade, cameraPath, panel, terminal } from './kit.mjs';
import { text, rrect, rect, line, circle, poly, bezierPts, bezierAt, brackets, streak, light, measure, check as tick, DW, DH } from '../src/engine/render.mjs';
import { rgba } from '../src/engine/core.mjs';

// ── sources ─────────────────────────────────────────────────────────────
const FIXTURE = 'scripts/media/fixtures/ship-notes.nika';
const fail = msg => { throw new Error(`spec-anatomy: ${msg}`); };
// the bytes the capture checked, which the clip draws: the fixture must
// still be exactly them
const CHECKED = readRepo('media/raw/spec-anatomy.nika');
if (readRepo(FIXTURE) !== CHECKED) fail(`${FIXTURE} changed since its capture: run scripts/media/capture/spec-anatomy.sh`);
const SRC = CHECKED.replace(/\n$/, '').split('\n');
const SCHEMA = JSON.parse(readRepo('media/raw/spec-anatomy-envelope.json'));
const CANON = repoLines('media/raw/spec-anatomy-verbs.txt');
const MMD = repoLines('media/raw/spec-anatomy-graph.mmd');
const CHECK = repoLines('media/raw/spec-anatomy-check.txt');
const VERSION = readRepo('media/raw/spec-anatomy-version.txt').trim().split(/\s+/)[1];
const PACK = readRepo('media/raw/spec-anatomy-pack.txt').match(/language pack (\S+)/)?.[1];
const LEGEND = repoLines('media/raw/try-gallery.txt').find(l => /^verbs ·/u.test(l)) ?? '';
// the file's name, as the check that judged it prints it
const FILE = CHECK[0]?.match(/^nika check · (\S+\.nika)$/u)?.[1] ?? fail('the check capture has no header');
if (FILE !== FIXTURE.split('/').pop()) fail(`the check judged ${FILE}, not ${FIXTURE}`);

// ── the envelope, as the file writes it and as the schema defines it ────
const TOP = SRC.map((l, i) => ({ key: l.match(/^([a-z_]+):/)?.[1], n: i + 1 })).filter(k => k.key);
const KEYS = SCHEMA.keys.map(k => k.key);
if (TOP.map(k => k.key).join() !== KEYS.join()) fail(`the file's top-level keys (${TOP.map(k => k.key)}) are not the schema's (${KEYS})`);
if (!PACK || !VERSION) fail('the version captures changed shape');
// a key's block: its row to the last non-blank row before the next key
TOP.forEach((k, i) => {
  let last = (TOP[i + 1]?.n ?? SRC.length + 1) - 1;
  while (last > k.n && !SRC[last - 1].trim()) last--;
  k.last = last;
});
const keyOf = key => TOP.find(k => k.key === key);

// What each key is for: phrases picked verbatim out of the schema's own
// description of that key (backticks dropped, a first letter capitalised).
// A phrase the spec no longer says stops the render.
const PICK = {
  nika: [/^The file's NAME/, /and the mark that says « this is a nika file »/],
  model: [/^Default model/, /<provider>\/<name>/],
  inputs: [/^Typed workflow inputs/, /the parameters an author declares and a caller supplies/],
  const: [/^Named constants/, /a fixed value baked into the workflow/],
  secrets: [/masked references/, /never inline literals/],
  permits: [/^The declared capability boundary/, /default-deny unless listed/],
  run: [/^The run's entropy and clock declaration/, /declared, never ambient/],
  tasks: [/^The task map/, /the graph alone schedules/],
  outputs: [/^The workflow's return value/, /symmetric to inputs/],
};
// the poster's table has less time to read than the dissection: a shorter
// verbatim phrase where the dissection's is long
const PICK_TABLE = { inputs: /a caller supplies/ };
const cap = s => s[0].toUpperCase() + s.slice(1);
const LABEL = Object.fromEntries(SCHEMA.keys.map(({ key, description }) => {
  const d = description.replace(/`/g, '');
  const [a, b] = PICK[key] ?? fail(`no label picked for \`${key}\``);
  const m1 = d.match(a), m2 = d.match(b), m3 = d.match(PICK_TABLE[key] ?? b);
  if (!m1 || !m2 || !m3) fail(`the spec no longer describes \`${key}\` in the words the clip shows`);
  return [key, [cap(m1[0]), m2[0], m3[0]]];
}));

// ── the verbs: the canon's list, inspect's colours, try's glyphs ────────
const VERBS = CANON.map(l => l.match(/name: (\w+),\s*semantic: "([^"]+)"/)).filter(Boolean).map(m => ({ verb: m[1], semantic: m[2] }));
const VCOUNT = +(CANON.find(l => /^\s+count:/.test(l))?.match(/\d+/)?.[0] ?? NaN);
if (!VERBS.length || VERBS.length !== VCOUNT) fail('the canon verbs section changed shape');
const COLOR = Object.fromEntries(MMD.map(l => l.match(/^\s*classDef (\w+) .*stroke:(#[0-9a-fA-F]{6})/)).filter(Boolean).map(m => [m[1], m[2]]));
const GLYPH = Object.fromEntries([...LEGEND.matchAll(/([◇▷◆✦]) (\w+) \(/gu)].map(m => [m[2], m[1]]));
const SEMANTIC = Object.fromEntries(VERBS.map(v => [v.verb, v.semantic]));

// the tasks, the verb each one binds, and where both sit in the file
const VERB_RE = new RegExp(`^ {4}(${VERBS.map(v => v.verb).join('|')}):`);
const TASKS = [];
for (let n = keyOf('tasks').n + 1; n <= keyOf('tasks').last; n++) {
  const l = SRC[n - 1];
  const id = l.match(/^ {2}([a-z_]+):\s*$/)?.[1];
  if (id) TASKS.push({ id, n, verb: null, vn: null });
  const v = l.match(VERB_RE)?.[1];
  if (v) {
    if (!TASKS.length || TASKS.at(-1).verb) fail(`a task binds two verbs (line ${n})`);
    Object.assign(TASKS.at(-1), { verb: v, vn: n });
  }
}
const INSPECT = Object.fromEntries(MMD.map(l => l.match(/^\s*(\w+)\["[^"]*"\]:::(\w+)/)).filter(Boolean).map(m => [m[1], m[2]]));
for (const tk of TASKS) {
  if (!tk.verb) fail(`task \`${tk.id}\` binds no verb`);
  if (INSPECT[tk.id] !== tk.verb) fail(`nika inspect gives \`${tk.id}\` the verb ${INSPECT[tk.id]}, the file ${tk.verb}`);
  if (!COLOR[tk.verb] || !GLYPH[tk.verb]) fail(`no colour or glyph for ${tk.verb}`);
}
for (const v of VERBS) if (!TASKS.some(tk => tk.verb === v.verb)) fail(`no task of the file uses the verb ${v.verb}`);

// ── the verdict ─────────────────────────────────────────────────────────
const VERDICT = CHECK.find(l => /^ layers · /u.test(l));
if (!VERDICT || !/run ready ✔/u.test(VERDICT)) fail('nika check no longer says the file is run ready');
if (CHECK.some(l => /^ ✖/u.test(l))) fail('nika check reports a finding on the file');
for (const tk of TASKS) if (!CHECK.some(l => new RegExp(`^\\s+wave \\d+ .*\\b${tk.id} \\(${tk.verb}\\b`).test(l))) fail(`the check's PLAN does not run \`${tk.id}\` as ${tk.verb}`);
const LAYERS = VERDICT.trim().split(' · ').slice(1); // valid ✔ · access ready ✔ · …
if (!LAYERS.every(l => /✔$/u.test(l))) fail('a layer of the verdict is not ✔');
const READY = LAYERS.at(-1).replace(/\s*✔$/u, ''); // "run ready"
const CHECK_SHOWN = CHECK.slice(1); // the typed command already names the file

// ── timing ──────────────────────────────────────────────────────────────
const T = {
  title: 0.1, card: 0.2, ghost: 0.3, counts: 0.45,
  pushA: 3.0,
  land: { nika: 3.0, model: 3.9, inputs: 4.8, const: 5.7, secrets: 6.6, permits: 7.5, run: 8.4 },
  panB: 11.5,
  tasks: 11.5, outputs: 12.65,
  dock: 12.95, verbs: 13.35, verbEvery: 0.4, fly: 0.55,
  panC: 17.0,
  cmd: 16.85, out: 17.5, stack: 17.75,
  whip: 20.6,
  fold: 19.95, foldEnd: 20.75,
  table: 20.6,
};
export const meta = { duration: 24.6, poster: 24.0 };
// the dissection's labels and legends clear the way for the terminal
const dissecting = t => 1 - smooth(T.panC - 1.05, T.panC - 0.8, t);
const DUR = meta.duration;
const land = key => T.land[key] ?? T[key];

// ── layout (world px) ───────────────────────────────────────────────────
const U = { size: 22, lh: 31 }; // the file, unfolded: read at 22 px
const P = { size: 28, lh: 49 }; // the skeleton, folded: the poster
const CARD = { x: 64, y: 244 };
const HEAD = 44;
const adv = size => size * 0.7; // Martian Mono advance
const gutterW = size => size * 3.4;
const x0 = size => CARD.x + gutterW(size);
const yU = n => CARD.y + HEAD + 36 + (n - 1) * U.lh;
const MAXW = Math.max(...SRC.map(l => l.length));
const CARD_W = Math.ceil(gutterW(U.size) + MAXW * adv(U.size) + 40);
const CARD_HU = yU(SRC.length) + 26 - CARD.y;
const LX = CARD.x + CARD_W + 56; // label column (dissection)

// the skeleton: the nine keys and the task headers; everything else folds
const SKEL = [];
for (const k of TOP) {
  SKEL.push({ n: k.n, key: k.key, folds: k.last > k.n && k.key !== 'tasks' });
  if (k.key === 'tasks') for (const tk of TASKS) SKEL.push({ n: tk.n, task: tk, folds: true });
}
const skelIndex = new Map(SKEL.map((s, i) => [s.n, i]));
const yF = i => CARD.y + HEAD + 42 + i * P.lh;
const parentSkel = n => { let i = 0; SKEL.forEach((s, j) => { if (s.n <= n) i = j; }); return i; };
const TASK_W = Math.max(...TASKS.map(tk => SRC[tk.n - 1].length));
const PILL_W = 46;
const CHIP_H = 36;
const chipW = (verb, size) => 46 + verb.length * adv(size) + 16;
const CHIP_WMAX = Math.max(...VERBS.map(v => chipW(v.verb, P.size)));
// where a task's chip sits: past the widest task row (and its fold mark)
const chipX = (size, f) => x0(size) + (TASK_W + 1) * adv(size) + lerp(12, PILL_W + 20, f);
const FOLD_W = Math.max(660, Math.ceil(chipX(P.size, 1) - CARD.x + CHIP_WMAX + 36));
const LXF = CARD.x + FOLD_W + 64; // label column (poster)

// While the camera is in, the world fades out under the chrome: a band at
// the top (under the kicker) and one at the bottom (under the plate), each
// held nearly opaque to `hold` px from the frame's edge and clear at `end`.
// The pushed-in shots are framed so that no row of text lands in a fade.
const SCRIM = { hold: 78, end: 118 };
const ROW_ASCENT = Math.ceil(U.size * 0.78);
// the dissection's two shots: the envelope keys, then tasks and outputs
const A = { x: 960, y: CARD.y - (SCRIM.end + 2) + 540, s: 1 }; // the card's top edge just clear of the band
const B = { x: 960, y: yU(keyOf('tasks').n) + 540 - (SCRIM.end + ROW_ASCENT), s: 1 }; // `tasks:` just clear; the rows above it hidden
// the highest a label may sit in a shot: its index tag clears the band
const clearTop = cam => cam.y - 540 + SCRIM.end + 50;

// label slots: close to their key, never closer to each other than `gap`,
// never above `lo`
function spread(wants, gap, lo = -Infinity) {
  const pos = [...wants];
  pos[0] = Math.max(pos[0], lo);
  for (let i = 1; i < pos.length; i++) pos[i] = Math.max(pos[i], pos[i - 1] + gap);
  let i = 0;
  while (i < pos.length) {
    let j = i;
    while (j + 1 < pos.length && pos[j + 1] - pos[j] <= gap + 0.5) j++;
    const shift = (wants.slice(i, j + 1).reduce((s, w) => s + w, 0) - pos.slice(i, j + 1).reduce((s, p) => s + p, 0)) / (j - i + 1);
    const floor = i ? pos[i - 1] + gap : lo;
    const d = Math.max(shift, floor - pos[i]);
    for (let k = i; k <= j; k++) pos[k] += Math.min(0, d);
    i = j + 1;
  }
  return pos;
}
const GROUP_A = ['nika', 'model', 'inputs', 'const', 'secrets', 'permits', 'run'];
const GROUP_B = ['tasks', 'outputs'];
if ([...GROUP_A, ...GROUP_B].join() !== KEYS.join()) fail('the dissection does not visit every key in order');
const SLOT = {};
spread(GROUP_A.map(key => yU(keyOf(key).n) + 6), 96, clearTop(A)).forEach((y, i) => { SLOT[GROUP_A[i]] = y; });
// the tasks section: its label, the verbs' legend (a chip, then the
// canon's line), the outputs label; each no higher than its row
{
  const items = [
    { want: Math.max(yU(keyOf('tasks').n) + 6, clearTop(B)), gap: 146 },
    ...TASKS.map(tk => ({ want: yU(tk.n) + 6, gap: 80 })),
    { want: yU(keyOf('outputs').n) + 6, gap: 0 },
  ];
  const ys = [];
  items.forEach((it, i) => ys.push(i ? Math.max(it.want, ys[i - 1] + items[i - 1].gap) : it.want));
  SLOT.tasks = ys[0];
  SLOT.verbs = ys.slice(1, -1);
  SLOT.outputs = ys.at(-1);
}

// the terminal and the verdict stack (world): the terminal opens where the
// labels were, right of the file, so every camera move crosses the file;
// it holds the command and the 29 lines of the check, and fits between
// the bands
const TERM = { x: LX, y: yU(keyOf('tasks').n) - 70, w: 1180, h: 836 };
if (TERM.h > DH - 2 * SCRIM.end) fail('the terminal does not fit between the chrome bands');
const STACK = { x: TERM.x + TERM.w + 64, y: TERM.y + 250 };

// ── camera ──────────────────────────────────────────────────────────────
// the opening: the whole file, small, beside the counts; then the dive
const SI = 0.55;
const I = { x: CARD.x + CARD_W / 2 - (1478 - 960) / SI, y: CARD.y + CARD_HU / 2 - (548 - 540) / SI, s: SI };
// the opening counts sit left of the specimen: world placed from screen
const COUNTS = { x: (64 - 960) / SI + I.x, y: (520 - 540) / SI + I.y };
// (the dissection's shots A and B are framed with the chrome bands above)
// the check: the terminal and the verdict, the file just out of frame
const Cc = { x: (TERM.x - 70 + STACK.x + 480) / 2 + 60, y: TERM.y + TERM.h / 2, s: 1 };
const F = { x: 960, y: 540, s: 1 };
const SHOTS = [
  { at: 0, cam: I },
  { at: T.pushA, cam: A, move: 0.9 },
  { at: T.panB, cam: B, move: 0.7 },
  { at: T.panC, cam: Cc, move: 0.75 },
  { at: T.whip, cam: F, move: 0.8 },
];
export const camera = t => cameraPath(t, SHOTS);
// the title band is the frame's while the camera is out (the opening, the poster)
const titleK = t => clamp((1 - smooth(T.pushA - 0.75, T.pushA - 0.45, t)) + smooth(T.whip - 0.25, T.whip + 0.2, t));

export function env(t) {
  const cam = camera(t);
  return { bgGlow: 1, gridAlpha: 0.22, gridX: -(cam.x - 960) * 0.1 * cam.s, gridY: -(cam.y - 540) * 0.1 * cam.s, bgY: 560 };
}

// the bands, in screen space: the background colour, opaque from the
// frame's edge to `hold` (bright text leaks through even 95%), then eased
// out; in the glow pass they erase the light of whatever passes beneath,
// so nothing blooms through
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
  const a = loopFade(t, DUR);
  const tk = titleK(t);
  scrims(R, 1 - tk);
  frame(R, t, {
    kicker: 'the language · the anatomy of one .nika file',
    plate: `${FIXTURE} · labels: nika spec, pack ${PACK} · verdict: nika check · nika ${VERSION} · framing, fold: illustration`,
    alpha: a,
  });
  headline({ ...R, fade: a * tk }, t, T.title, DUR + 1, 'What a .nika file is made of.', `${KEYS.length} ENVELOPE KEYS · ${VERBS.length} VERBS · ONE FILE, CHECKED BEFORE IT RUNS`, { accent: 'made of.', accentColor: C.teal });
}

// ── helpers ─────────────────────────────────────────────────────────────
// a text wiped on from the left with a light at its head
function wipe(R, str, x, y, st, p, alpha, color = C.ink, glow = 0) {
  if (p <= 0 || alpha <= 0) return;
  const w = measure(str, st);
  const q = E.outCubic(clamp(p));
  const ctx = R.ctx;
  ctx.save();
  ctx.beginPath();
  ctx.rect(x - 6, y - st.size * 1.3, (w + 12) * q, st.size * 2);
  ctx.clip();
  text(R, str, x, y, { ...st, color, alpha, glow });
  ctx.restore();
  if (p < 1) light(R, x + w * q, y - st.size * 0.35, st.size * 1.6, C.cyan, 0.35 * alpha * (1 - p));
}
// the verbs' glyphs, as `nika try` prints them, drawn as shapes
function glyph(R, g, x, y, s, color, alpha) {
  const S = { color, w: 1.8, alpha, glow: 0.6 };
  if (g === '◇' || g === '◆') poly(R, [[x, y - s], [x + s, y], [x, y + s], [x - s, y], [x, y - s]], g === '◆' ? { ...S, fill: color } : S);
  else if (g === '▷') poly(R, [[x - s * 0.8, y - s], [x + s, y], [x - s * 0.8, y + s], [x - s * 0.8, y - s]], S);
  else if (g === '✦') {
    const pts = [];
    for (let k = 0; k < 8; k++) { const r = k % 2 ? s * 0.38 : s * 1.1, an = -Math.PI / 2 + (k * Math.PI) / 4; pts.push([x + r * Math.cos(an), y + r * Math.sin(an)]); }
    poly(R, [...pts, pts[0]], { ...S, fill: color });
  }
}
function chip(R, verb, x, y, size, alpha, lit = 1) {
  const col = COLOR[verb];
  const w = chipW(verb, size), st = { f: 'MM 500', size };
  rrect(R, x, y - CHIP_H / 2, w, CHIP_H, CHIP_H / 2, { color: col, w: 1.4, alpha, fill: col, fillAlpha: 0.1 + 0.08 * lit, glow: 0.5 * lit });
  glyph(R, GLYPH[verb], x + 23, y, 7.5, col, alpha);
  text(R, verb, x + 42, y + size * 0.36, { ...st, color: col, alpha, glow: 0.35 * lit });
  return w;
}

// ── the file ────────────────────────────────────────────────────────────
// when each row lights up: a key's own row first, then its body
const LIT = new Map();
for (const k of TOP) {
  const t0 = land(k.key);
  LIT.set(k.n, t0);
  const every = k.key === 'tasks' ? 0.035 : 0.09;
  for (let n = k.n + 1; n <= k.last; n++) LIT.set(n, t0 + 0.4 + (n - k.n - 1) * every);
}
const foldK = t => E.inOutCubic(seg(t, T.fold, T.foldEnd));
const ghostK = t => smooth(T.ghost, T.ghost + 0.5, t);

function rowGeom(n, f) {
  const si = skelIndex.get(n);
  const size = lerp(U.size, P.size, f);
  if (si !== undefined) return { y: lerp(yU(n), yF(si), f), size, alpha: 1 };
  return { y: lerp(yU(n), yF(parentSkel(n)), f), size, alpha: 1 - smooth(0, 0.45, f) };
}

function drawRow(R, t, n, a, f) {
  const raw = SRC[n - 1];
  if (!raw.trim()) return;
  const g = rowGeom(n, f);
  const alpha = a * g.alpha;
  if (alpha <= 0.003) return;
  const st = { f: 'MM 400', size: g.size };
  const X = x0(g.size);
  const t0 = LIT.get(n);
  const lit = t >= t0 ? E.inOutCubic(seg(t, t0, t0 + 0.32)) : 0;
  // the specimen: the file's silhouette, token by token in the colours
  // its text will take, before each part is dissected
  const gh = ghostK(t) * (1 - lit);
  if (gh > 0) {
    // rows develop top to bottom
    const dev = gh * smooth(T.ghost + (n / SRC.length) * 0.5, T.ghost + (n / SRC.length) * 0.5 + 0.25, t);
    let col = 0;
    for (const sp of yamlSpans(raw)) {
      for (const m of sp.s.matchAll(/\S+/g)) {
        const x = X + (col + m.index) * adv(g.size);
        rrect(R, x, g.y - g.size * 0.64, m[0].length * adv(g.size) - adv(g.size) * 0.3, g.size * 0.52, g.size * 0.2, { fill: sp.c || C.mist, alpha: alpha * 0.2 * dev });
      }
      col += sp.s.length;
    }
    text(R, String(n), X - 16, g.y, { f: 'MM 400', size: g.size - 7, color: C.dim, alpha: alpha * 0.45 * dev, align: 'right' });
  }
  if (lit <= 0) return;
  text(R, String(n), X - 16, g.y, { f: 'MM 400', size: g.size - 7, color: C.dim, alpha: alpha * 0.9 * lit, align: 'right' });
  // a verb, once its chip has landed, takes the colour nika inspect gives it
  const tk = TASKS.find(x => x.vn === n);
  const marks = tk && t >= verbLands(tk) ? [{ re: new RegExp(`^\\s+${tk.verb}(?=:)`), c: COLOR[tk.verb], glow: 0.55 }] : [];
  const spans = yamlSpans(raw, marks);
  const top = TOP.find(k => k.n === n);
  const ctx = R.ctx;
  // the light passes left to right: behind it the row is lit
  const w = raw.length * adv(g.size);
  ctx.save();
  ctx.beginPath();
  ctx.rect(X - 4, g.y - g.size * 1.2, (w + 10) * lit, g.size * 1.7);
  ctx.clip();
  if (top) {
    const spansNoKey = spans.map(sp => (sp.s === top.key ? { s: ' '.repeat(sp.s.length) } : sp));
    mono(R, spansNoKey, X, g.y, { alpha, st });
  } else mono(R, spans, X, g.y, { alpha, st });
  ctx.restore();
  if (lit < 1) {
    const hx = X + (w + 10) * lit;
    line(R, hx, g.y - g.size * 0.95, hx, g.y + g.size * 0.3, { color: C.cyan, w: 2, alpha: alpha * (1 - lit), glow: 1 });
  }
  if (top) {
    // the key itself lands: tracked out and soft, it snaps to its column
    const kk = E.snap(seg(t, t0, t0 + 0.5));
    const tr = 22 * (1 - kk);
    text(R, top.key, X, g.y, { f: 'MM 500', size: g.size, tracking: tr, color: kk > 0.7 ? C.ice : C.cyan, alpha: alpha * smooth(t0, t0 + 0.1, t), glow: 0.9 * (1 - kk) + 0.25, blur: 4 * (1 - kk) });
  }
}

// ── the labels (dissection): a leader from the key, the spec's words ────
const IDX = Object.fromEntries(KEYS.map((k, i) => [k, i]));
function anchorOf(key) {
  const k = keyOf(key);
  return { x: x0(U.size) + SRC[k.n - 1].length * adv(U.size) + 16, y: yU(k.n) - U.size * 0.34 };
}
const S1 = { f: 'Geist 600', size: 26, tracking: -0.2 };
const S2 = { f: 'Geist 400', size: 22 };
function label(R, t, key, a) {
  const t0 = land(key);
  const la = a * dissecting(t);
  if (t < t0 + 0.2 || la <= 0) return;
  const [l1, l2] = LABEL[key];
  const an = anchorOf(key);
  const ly = SLOT[key];
  const p = E.inOutCubic(seg(t, t0 + 0.22, t0 + 0.6));
  poly(R, [[an.x, an.y], [LX - 44, an.y], [LX - 22, ly - 9], [LX - 10, ly - 9]], { color: C.ice, w: 1.3, alpha: la * 0.75, glow: 0.35 }, p);
  circle(R, an.x, an.y, 3.2, { fill: C.ice, alpha: la * smooth(t0 + 0.2, t0 + 0.3, t), glow: 0.8 });
  // which of the envelope's keys this is
  const idx = `${String(IDX[key] + 1).padStart(2, '0')} / ${String(KEYS.length).padStart(2, '0')} · ${key.toUpperCase()}`;
  wipe(R, idx, LX, ly - 34, { f: 'MGW 500', size: 12, tracking: 3 }, seg(t, t0 + 0.45, t0 + 0.8), la, C.cyan);
  wipe(R, l1, LX, ly, S1, seg(t, t0 + 0.5, t0 + 0.85), la, C.ink, 0.12);
  wipe(R, l2, LX, ly + 32, S2, seg(t, t0 + 0.68, t0 + 1.0), la, C.mist);
}

// the poster's table: each row of the skeleton, what the spec says it is
// (its header is the verdict, carried there by verdict())
function table(R, t, a) {
  if (t < T.table) return;
  SKEL.forEach((s, i) => {
    const t0 = T.table + i * 0.055;
    const k = seg(t, t0, t0 + 0.4);
    if (k <= 0) return;
    const y = yF(i);
    const xa = s.task ? chipX(P.size, 1) + chipW(s.task.verb, P.size) + 16 : x0(P.size) + (SRC[s.n - 1].trimEnd().length + 1) * adv(P.size) + (s.folds ? PILL_W + 14 : 6);
    line(R, xa, y - P.size * 0.3, lerp(xa, LXF - 18, E.inOutCubic(k)), y - P.size * 0.3, { color: C.faint, w: 1.2, alpha: a * 0.9, dash: [2, 7] });
    if (s.task) {
      wipe(R, SEMANTIC[s.task.verb], LXF, y, { f: 'Geist 500', size: 24 }, seg(t, t0 + 0.1, t0 + 0.5), a, C.mist);
      return;
    }
    const [l1, , l2] = LABEL[s.key];
    const st1 = { f: 'Geist 600', size: 26, tracking: -0.2 };
    wipe(R, l1, LXF, y, st1, seg(t, t0 + 0.1, t0 + 0.45), a, C.ink, 0.1);
    const w1 = measure(l1, st1);
    circle(R, LXF + w1 + 17, y - 8, 2.8, { fill: C.dim, alpha: a * smooth(t0 + 0.3, t0 + 0.4, t) });
    wipe(R, l2, LXF + w1 + 34, y, { f: 'Geist 400', size: 23 }, seg(t, t0 + 0.25, t0 + 0.6), a, C.mist);
  });
}

// ── the verbs (dissection of tasks) ─────────────────────────────────────
// The column becomes a legend of the verbs, in the order the file's tasks
// bind them: a chip and the canon's line for each. A copy of each chip
// flies to the task that binds it; a leader ties the task back to its
// legend entry, as the keys' leaders do.
const verbLands = tk => T.verbs + TASKS.indexOf(tk) * T.verbEvery + T.fly;
function verbs(R, t, a, f) {
  if (t < T.dock) return;
  const out = dissecting(t);
  const la = a * out;
  const head = SLOT.verbs[0] - 70;
  wipe(R, `THE ${VERBS.length} VERBS · EACH TASK BINDS ONE`, LX, head, { f: 'MGW 500', size: 12, tracking: 3 }, seg(t, T.dock, T.dock + 0.4), la, C.cyan);
  TASKS.forEach((tk, i) => {
    const col = COLOR[tk.verb];
    const ly = SLOT.verbs[i], cy = ly - 26;
    const td = T.dock + 0.1 + i * 0.1;
    const t0 = verbLands(tk) - T.fly, tl = verbLands(tk);
    const p = E.inOutCubic(seg(t, t0, t0 + T.fly));
    // the legend entry: the chip waits, then stays as an outline once it has flown
    const lk = smooth(td, td + 0.3, t);
    if (lk > 0 && out > 0) {
      const ghost = smooth(t0, t0 + 0.2, t);
      const cw0 = chipW(tk.verb, U.size);
      if (ghost > 0) rrect(R, LX, cy - CHIP_H / 2, cw0, CHIP_H, CHIP_H / 2, { color: col, w: 1.2, alpha: la * 0.55 * ghost, dash: [4, 5] });
      if (p <= 0) chip(R, tk.verb, LX, cy, U.size, la * lk, 0.6);
      else if (ghost > 0) {
        glyph(R, GLYPH[tk.verb], LX + 23, cy, 7.5, col, la * 0.6 * ghost);
        text(R, tk.verb, LX + 42, cy + U.size * 0.36, { f: 'MM 500', size: U.size, color: col, alpha: la * 0.75 });
      }
      wipe(R, SEMANTIC[tk.verb], LX + 2, ly + 16, { f: 'Geist 500', size: 22 }, seg(t, td + 0.1, td + 0.45), la, C.ink);
    }
    if (p <= 0) return;
    const g = rowGeom(tk.n, f);
    const sx = chipX(g.size, f), sy = g.y - g.size * 0.34;
    const src = [LX, cy], dst = [sx, sy];
    const c0 = [lerp(src[0], dst[0], 0.25), src[1] - 140], c1 = [lerp(src[0], dst[0], 0.8), dst[1] - 110];
    if (p < 1) {
      const trail = bezierPts(src, c0, c1, dst, 40);
      const i1 = Math.round(p * 40), i0 = Math.max(0, i1 - 12);
      poly(R, trail.slice(i0, i1 + 1), { color: col, w: 2.2, alpha: a * 0.8, glow: 1 });
      const [x, y] = bezierAt(src, c0, c1, dst, p);
      chip(R, tk.verb, x, y, U.size, a, 1);
      return;
    }
    // landed: the chip sits on its task's row, the verb's own line lights up
    const ring = seg(t, tl, tl + 0.45);
    if (ring < 1) circle(R, sx + 22, sy, 12 + 46 * E.outCubic(ring), { color: col, w: 1.5, alpha: a * (1 - ring), glow: 0.9 });
    chip(R, tk.verb, sx, sy, g.size, a, 1 - 0.5 * smooth(tl, tl + 0.6, t));
    const hl = smooth(tl, tl + 0.3, t) * out;
    if (hl > 0) {
      const vy = yU(tk.vn);
      rect(R, CARD.x + 6, vy - U.lh + 8, CARD_W - 12, U.lh, { fill: col, alpha: a * 0.08 * hl });
      rect(R, CARD.x + 6, vy - U.lh + 8, 3, U.lh, { fill: col, alpha: a * hl, glow: 0.8 });
      // the leader back to the legend
      const lx0 = sx + chipW(tk.verb, U.size) + 12;
      const lp = E.inOutCubic(seg(t, tl + 0.05, tl + 0.4));
      poly(R, [[lx0, sy], [LX - 44, sy], [LX - 22, cy], [LX - 8, cy]], { color: col, w: 1.2, alpha: la * 0.6, glow: 0.3 }, lp);
    }
  });
}

// ── the check ───────────────────────────────────────────────────────────
function checkShot(R, t, a) {
  if (t < T.panC - 0.8 || t > T.whip + 0.2) return;
  const marks = [
    ...TASKS.map(tk => ({ re: new RegExp(`\\b${tk.id} \\(${tk.verb}[^)]*\\)`), c: COLOR[tk.verb], glow: 0.4 })),
    { re: /sanctioned · secret .*host pinned/u, c: C.amber },
    { re: /run ready ✔/u, c: C.teal, glow: 0.9 },
  ];
  terminal(R, t, TERM, [
    { t: T.cmd, cmd: `nika check ${FILE}`, dur: 0.6 },
    { t: T.out, out: CHECK_SHOWN, every: 0.012, marks },
  ], {
    title: 'terminal', alpha: a, k: E.snap(seg(t, T.panC - 0.75, T.panC - 0.3)), st: { f: 'MM 400', size: 19 }, lh: 25,
    badge: t > T.stack ? { label: READY.toUpperCase(), color: C.teal, alpha: smooth(T.stack, T.stack + 0.3, t) } : null,
  });
}

// The verdict, layer by layer: lifted out of the check's last line into a
// stack beside the terminal; it clears as the camera leaves, and the same
// layers stamp into the header of the poster's table as the camera lands.
const HEAD_Y = CARD.y + 32;
const HEAD_LABEL = { f: 'MGW 500', size: 12, tracking: 3 };
function verdict(R, t, a) {
  if (t < T.stack) return;
  const gone = 1 - smooth(T.whip - 0.8, T.whip - 0.55, t);
  if (gone > 0) {
    text(R, `NIKA CHECK · ${FILE.toUpperCase()}`, STACK.x, STACK.y - 62, { ...HEAD_LABEL, color: C.dim, alpha: a * gone * smooth(T.stack, T.stack + 0.3, t) });
    LAYERS.forEach((layer, i) => {
      const t0 = T.stack + 0.1 + i * 0.14;
      const k = E.snap(seg(t, t0, t0 + 0.45));
      if (k <= 0) return;
      const last = i === LAYERS.length - 1;
      const words = layer.replace(/\s*✔$/u, '');
      const st = last ? { f: 'Geist 600', size: 62, tracking: -1.2 } : { f: 'Geist 500', size: 32 };
      const y = STACK.y + i * 60 + (last ? 50 : 0) + 12 * (1 - k);
      text(R, words, STACK.x, y, { ...st, color: last ? C.teal : C.ink, alpha: a * k * gone, glow: last ? 0.5 : 0.1 });
      const w = measure(words, st);
      tick(R, STACK.x + w + (last ? 40 : 26), y - st.size * 0.33, last ? 36 : 21, E.snap(seg(t, t0 + 0.15, t0 + 0.5)), { color: C.teal, w: last ? 4 : 2.4, alpha: a * gone, glow: 0.9 });
    });
  }
  // the poster: the table's header is the verdict
  const h0 = T.whip - 0.1;
  if (t < h0) return;
  text(R, 'NIKA CHECK', LXF, HEAD_Y - 1, { ...HEAD_LABEL, color: C.dim, alpha: a * smooth(h0, h0 + 0.3, t) });
  let x = LXF + measure('NIKA CHECK', HEAD_LABEL) + 28;
  LAYERS.forEach((layer, i) => {
    const t0 = h0 + 0.12 + 0.1 * i;
    const last = i === LAYERS.length - 1;
    const words = layer.replace(/\s*✔$/u, '');
    const st = { f: last ? 'Geist 600' : 'Geist 500', size: 22 };
    const k = E.snap(seg(t, t0, t0 + 0.4));
    text(R, words, x, HEAD_Y + 3 + 8 * (1 - k), { ...st, color: last ? C.teal : C.ink, alpha: a * smooth(t0, t0 + 0.2, t), glow: last ? 0.4 : 0 });
    const w = measure(words, st);
    tick(R, x + w + 16, HEAD_Y - 4, 16, E.snap(seg(t, t0 + 0.15, t0 + 0.45)), { color: C.teal, w: 2.4, alpha: a, glow: 0.8 });
    x += w + 50;
  });
}

// ── the opening counts ──────────────────────────────────────────────────
function counts(R, t, a) {
  const out = 1 - smooth(T.pushA - 0.6, T.pushA - 0.25, t);
  if (t < T.counts || out <= 0) return;
  const rowsC = [
    { n: KEYS.length, what: 'envelope keys', src: 'THE SCHEMA · NIKA SPEC --SCHEMA', c: C.ice },
    { n: VERBS.length, what: 'verbs', src: 'THE CANON · NIKA SPEC --CANON', c: C.teal },
  ];
  rowsC.forEach((r, i) => {
    const t0 = T.counts + i * 0.25;
    const k = E.snap(seg(t, t0, t0 + 0.6));
    if (k <= 0) return;
    const y = COUNTS.y + i * 470;
    // the count rolls up to the number the capture holds
    const shown = Math.min(r.n, Math.floor(r.n * seg(t, t0, t0 + 0.35)) + 1);
    const al = a * out * k;
    text(R, String(shown), COUNTS.x, y + 30 * (1 - k), { f: 'Geist 700', size: 300, tracking: -8, color: r.c, alpha: al, glow: 0.35 });
    const nw = measure(String(r.n), { f: 'Geist 700', size: 300, tracking: -8 });
    text(R, r.what, COUNTS.x + nw + 50, y - 110, { f: 'Geist 600', size: 92, tracking: -2, color: C.ink, alpha: al });
    text(R, r.src, COUNTS.x + nw + 56, y - 20, { f: 'MGW 500', size: 28, tracking: 6, color: C.dim, alpha: al * 0.9 });
  });
}

// ── frame ───────────────────────────────────────────────────────────────
export function draw(R, t) {
  const a = loopFade(t, DUR);
  const f = foldK(t);
  const box = { x: CARD.x, y: CARD.y, w: lerp(CARD_W, FOLD_W, f), h: lerp(CARD_HU, yF(SKEL.length - 1) + 32 - CARD.y, f) };
  panel(R, box, { title: FILE, alpha: a, k: E.snap(seg(t, T.card, T.card + 0.6)) });
  counts(R, t, a);
  if (t < T.card + 0.3) return;
  // the block being dissected: a soft bar and a focus lock
  const cur = TOP.findLast(k => t >= land(k.key));
  if (cur && f <= 0) {
    const nxt = TOP[TOP.indexOf(cur) + 1];
    const t0 = land(cur.key), t1 = nxt ? land(nxt.key) : T.dock;
    const on = smooth(t0, t0 + 0.2, t) * (1 - smooth(t1 - 0.15, t1, t));
    if (on > 0) {
      const y0 = yU(cur.n) - U.lh + 6, y1 = yU(cur.last) + 10;
      rect(R, CARD.x + 6, y0, CARD_W - 12, y1 - y0, { fill: C.ice, alpha: a * 0.035 * on });
      const bk = E.snap(seg(t, t0, t0 + 0.4));
      const grow = lerp(1.06, 1, bk);
      const bw = (CARD_W - 28) * grow, bh = (y1 - y0 + 8) * grow;
      brackets(R, CARD.x + 14 + (CARD_W - 28 - bw) / 2, y0 - 4 + (y1 - y0 + 8 - bh) / 2, bw, bh, 14, { color: C.cyan, w: 1.4, alpha: a * on * lerp(0.9, 0.45, smooth(t0 + 0.4, t0 + 1, t)), glow: 0.6 * (1 - bk) });
      const sw = seg(t, t0 + 0.02, t0 + 0.5);
      if (sw > 0 && sw < 1) streak(R, lerp(x0(U.size), CARD.x + CARD_W, E.inOutCubic(sw)), yU(cur.n) - 8, 200, a * 0.5 * (1 - sw), C.cyan, 1);
    }
  }
  for (let n = 1; n <= SRC.length; n++) drawRow(R, t, n, a, f);
  // folded bodies: the fold marks, as an editor draws them
  if (f > 0.5) {
    const fk = a * smooth(0.5, 1, f);
    for (const s of SKEL) {
      if (!s.folds) continue;
      const g = rowGeom(s.n, f);
      const k = g.size / P.size;
      const x = x0(g.size) + (SRC[s.n - 1].trimEnd().length + 1) * adv(g.size);
      rrect(R, x, g.y - g.size * 0.74, PILL_W * k, g.size * 0.92, 7 * k, { color: C.faint, w: 1, alpha: fk, fill: C.faint, fillAlpha: 0.35 });
      for (let i = -1; i <= 1; i++) circle(R, x + (PILL_W / 2 + i * 9) * k, g.y - g.size * 0.28, 2.3 * k, { fill: C.mist, alpha: fk });
    }
  }
  for (const key of KEYS) label(R, t, key, a);
  verbs(R, t, a, f);
  table(R, t, a);
  checkShot(R, t, a);
  verdict(R, t, a);
}
