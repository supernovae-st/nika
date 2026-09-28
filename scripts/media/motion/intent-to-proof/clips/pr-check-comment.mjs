// pr-check-comment · "Every pull request gets a verdict."
// The sticky comment supernovae-st/nika-action posts on a pull request,
// for two pushes of one workflow. Its body is the action's own
// render_comment.py run on `nika check --json` and `nika inspect --format
// mermaid`, replayed exactly as the action runs them by
// scripts/media/capture/pr-check-comment.sh (media/raw/pr-check-comment-*).
// The first push reads `tasks.asses`: the comment comes back red with that
// one finding, and without a graph (inspect cannot project a DAG while
// conformance fails, and the action then drops it). The second push fixes
// the line; the same comment (one hidden marker, upserted) comes back clean
// with the graph inspect drew. Every word, code, count and node of the
// comment is read from those captures and typeset here in a GitHub-like
// style, as the renderer means it (its footer rule would need a blank line
// before `---` for GitHub's markdown to agree). The job's step names are
// action.yml's own, recorded by the capture; the exit codes are the
// captured ones. The pull request around it (title, commit messages,
// author line, the diff view) and the lighting are an illustration, and
// the plate says so.
import { C, E, seg, smooth, clamp, lerp, readRepo, repoLines, mono, yamlSpans, cw, squiggle, pill, lineDiff, frame, headline, loopFade, cameraPath, frameBox, titleFade, WIDE } from './kit.mjs';
import { text, rrect, rect, line, circle, arc, poly, check, cross, measure, bezierPts, streak, light } from '../src/engine/render.mjs';

export const meta = { duration: 22.5, poster: 21.8 };

// ── the captures, and the story they must still tell ────────────────────
const RUN = JSON.parse(readRepo('media/raw/pr-check-comment-run.json'));
const load = state => ({
  md: readRepo(`media/raw/pr-check-comment-${state}.md`),
  report: JSON.parse(readRepo(`media/raw/pr-check-comment-${state}.json`)),
  mmd: readRepo(`media/raw/pr-check-comment-${state}.mmd`),
  file: repoLines(`media/raw/pr-check-comment-${state}.nika`),
  exit: RUN.pushes[state].check_exit,
});
const RED = load('red'), CLEAN = load('clean');
const FINDING = RED.report.conformance?.[0];
// the second push as a diff shows it: in a run of changes, removals first
function removalsFirst(ops) {
  const out = [];
  let run = [];
  const flush = () => { out.push(...run.filter(o => o.op === '-'), ...run.filter(o => o.op === '+')); run = []; };
  for (const o of ops) {
    if (o.op === '=') { flush(); out.push(o); } else run.push(o);
  }
  flush();
  return out;
}
const OPS = removalsFirst(lineDiff(RED.file, CLEAN.file));
const CHANGED = OPS.filter(o => o.op !== '=');
const markerOf = md => md.match(/^<!-- (nika-action:v1:\S+) -->$/m)?.[1];
const mermaidOf = md => md.match(/^```mermaid\n([\s\S]*?)\n```$/m)?.[1];
const fail = why => { throw new Error(`the pr-check-comment capture: ${why}`); };
if (RED.report.clean !== false || RED.exit !== 2 || !RED.md.startsWith('❌ **nika check** — 1 finding(s)')) fail('the first push\'s comment is not red');
if (CLEAN.report.clean !== true || CLEAN.exit !== 0 || !CLEAN.md.startsWith('✅ **nika check** — clean')) fail('the second push\'s comment is not clean');
if (RED.report.conformance.length !== 1 || FINDING.code !== 'NIKA-DAG-002' || !FINDING.offending || !FINDING.suggestion) fail('the red push no longer carries its one NIKA-DAG-002 finding');
if (CHANGED.length !== 2 || !CHANGED.some(o => o.op === '-' && o.text.includes(`tasks.${FINDING.offending}.`)) || !CHANGED.some(o => o.op === '+' && o.text.includes(`tasks.${FINDING.suggestion}.`))) fail('the second push no longer fixes exactly the line the finding names');
if (mermaidOf(RED.md) !== undefined || RED.mmd.trim() || mermaidOf(CLEAN.md)?.trim() !== CLEAN.mmd.trim()) fail('the comments no longer carry the graph nika inspect drew (none for the red push)');
if (!markerOf(RED.md) || markerOf(RED.md) !== markerOf(CLEAN.md)) fail('the two pushes no longer upsert one sticky comment');
const STEP = name => RUN.action.steps.find(s => s.startsWith(name)) ?? fail(`action.yml has no "${name}" step`);
const STEP_CHECK = STEP('nika check (');
const STEP_STICKY = STEP('Sticky PR comment (');
const ENGINE = RUN.engine;

// ── timeline (seconds) ──────────────────────────────────────────────────
const T = {
  page: 0.3, rows: 0.5, every: 0.014,
  commit1: 0.75, job: 1.0,
  scan: 1.7, scanEnd: 2.6, // the check step runs while the scan reads the file
  post: 3.05, posted: 3.85, fail1: 3.95,
  hl: [4.8, 5.5, 7.2, 8.2], // the reading guide: verdict · finding · cost floor · requires
  beam: 9.25,
  push2: 10.5, fix: 10.8, fixEnd: 11.4,
  xray: 13.05, flip: 13.9, morph: 14.55, dag: 15.2, dagEnd: 17.0,
  pass2: 14.7,
};
// the action's job on the first push, step by step as action.yml names
// them: when each step starts and how it ends (mode: check skips the
// golden lane; the gate's red is the commit's status once the comment is up)
const JOB = RUN.action.steps.map((name, i) => {
  const at = [
    [/^Validate inputs/, 1.3, 1.3, 'ok'],
    [/^Install nika/, 1.35, 1.6, 'ok'],
    [/^nika check \(/, 1.65, 2.6, 'ok'],
    [/^nika test \(/, 2.65, 2.65, 'skip'],
    [/^Render /, 2.7, 2.8, 'ok'],
    [/^Sticky PR comment/, 2.85, 2.95, 'ok'],
    [/^Gate /, 3.0, Infinity, 'ok'],
  ].find(([re]) => re.test(name));
  return { name, t0: at?.[1] ?? 1.3 + i * 0.2, t1: at?.[2] ?? 1.3 + i * 0.2, end: at?.[3] ?? 'ok' };
});

// ── layout (world = the 1920×1080 design frame) ─────────────────────────
const LEFT = { x: 64, w: 720 }, RIGHT = { x: 816, w: 1040 };
const DIFF = { x: LEFT.x, y: 304, w: LEFT.w, h: 720 };
const CODE = { f: 'MM 400', size: 15 };
const DLH = 19, FIRST = 5, LAST = 38;
const DHEAD = 40, ROW0 = DIFF.y + DHEAD + 26;
const COMMIT1_Y = 262; // baseline of the first commit's message
const CARD = { x: RIGHT.x, y: 306, w: RIGHT.w };
const HEAD_H = 46, PAD_T = 16, PAD_B = 12, PAD_X = 28;
const BODY_X = CARD.x + PAD_X, BODY_W = CARD.w - 2 * PAD_X;
const LHB = 29, BASE = 21; // body line height, baseline within a line

// ── inline markdown: the subset render_comment.py writes ────────────────
const ST = {
  text: { f: 'Geist 400', size: 19, color: '#BFD0E6' },
  bold: { f: 'Geist 600', size: 19, color: C.ink },
  link: { f: 'Geist 400', size: 19, color: C.cyan },
  code: { f: 'MM 400', size: 16, color: '#DDE9F8' },
};
const SUB = { text: { ...ST.text, size: 14, color: C.dim }, bold: { ...ST.bold, size: 14 }, link: { ...ST.link, size: 14 }, code: { ...ST.code, size: 12.5 } };
function inline(s) {
  const out = [];
  const re = /\*\*(.+?)\*\*|`([^`]+)`|\[([^\]]+)\]\([^)]+\)|(❌|✅|💰|🔐|🌊|🗺|⚠)/gu;
  let last = 0, m;
  while ((m = re.exec(s))) {
    if (m.index > last) out.push({ t: 'text', s: s.slice(last, m.index) });
    if (m[1] !== undefined) out.push({ t: 'bold', s: m[1] });
    else if (m[2] !== undefined) out.push({ t: 'code', s: m[2] });
    else if (m[3] !== undefined) out.push({ t: 'link', s: m[3] });
    else out.push({ t: 'icon', s: m[4] });
    last = m.index + m[0].length;
  }
  if (last < s.length) out.push({ t: 'text', s: s.slice(last) });
  return out;
}
const CHIP = 6; // code chip padding
const iconW = st => st.size * 1.25;
// runs → rows of positioned atoms, wrapped at spaces within maxW
function layoutInline(runs, maxW, sty = ST) {
  const atoms = [];
  for (const r of runs) {
    const st = sty[r.t] ?? sty.text;
    if (r.t === 'icon') atoms.push({ t: 'icon', s: r.s, w: iconW(sty.text), st: sty.text });
    else if (r.t === 'code') atoms.push({ t: 'code', s: r.s, w: measure(r.s, st) + 2 * CHIP, st });
    else for (const p of r.s.split(/( )/)) if (p) atoms.push({ t: r.t, s: p, w: measure(p, st), st, space: p === ' ' });
  }
  const rows = [[]];
  let x = 0;
  for (const a of atoms) {
    let row = rows[rows.length - 1];
    if (!a.space && x + a.w > maxW && row.length) { rows.push((row = [])); x = 0; }
    if (a.space && !row.length) continue;
    row.push({ ...a, x });
    x += a.w + (a.t === 'code' ? 2 : 0);
  }
  // merge neighbouring words of one style into one draw
  return rows.map(row => {
    const merged = [];
    for (const a of row) {
      const prev = merged[merged.length - 1];
      if (prev && prev.t === a.t && a.t !== 'code' && a.t !== 'icon') {
        prev.s += a.s;
        prev.w = a.x + a.w - prev.x;
        prev.space = !prev.s.trim();
      } else merged.push({ ...a });
    }
    while (merged.length && merged[merged.length - 1].space) merged.pop();
    return merged;
  });
}
function drawAtoms(R, atoms, x, y, alpha, over = {}) {
  for (const a of atoms) {
    if (a.space || alpha <= 0) continue;
    const st = { ...a.st, ...(over[a.s] ?? {}) };
    if (a.t === 'icon') icon(R, a.s, x + a.x, y, a.st.size, alpha);
    else if (a.t === 'code') {
      rrect(R, x + a.x, y - a.st.size - 3, a.w, a.st.size + 10, 5, { fill: st.chip ?? C.faint, fillAlpha: 0.55, alpha, color: st.chip, w: st.chip ? 1.2 : 0, glow: st.chip ? 0.4 : 0 });
      text(R, a.s, x + a.x + CHIP, y, { ...st, alpha, glow: st.glow ?? 0 });
    } else text(R, a.s, x + a.x, y, { ...st, alpha, glow: st.glow ?? 0 });
  }
}

// The emoji the comment carries, painted as vectors in their own cell (the
// fonts have no colour emoji): the characters are the comment's own.
function icon(R, ch, x, y, s, alpha = 1) {
  const cx = x + s * 0.55, cy = y - s * 0.36;
  const S = (color, w = Math.max(1.3, s * 0.08)) => ({ color, w, alpha });
  switch (ch) {
    case '❌': {
      const r = s * 0.3;
      poly(R, [[cx - r, cy - r], [cx + r, cy + r]], { ...S(C.red, s * 0.15), glow: 0.7 });
      poly(R, [[cx + r, cy - r], [cx - r, cy + r]], { ...S(C.red, s * 0.15), glow: 0.7 });
      break;
    }
    case '✅': {
      const r = s * 0.42;
      rrect(R, cx - r, cy - r, 2 * r, 2 * r, s * 0.16, { fill: C.teal, alpha, glow: 0.6 });
      check(R, cx + s * 0.01, cy + s * 0.03, s * 0.52, 1, { color: C.bg0, w: s * 0.13, alpha });
      break;
    }
    case '💰': {
      circle(R, cx, cy + s * 0.1, s * 0.33, { ...S(C.gold), fill: C.gold, fillAlpha: 0.18 });
      poly(R, [[cx - s * 0.15, cy - s * 0.36], [cx - s * 0.07, cy - s * 0.22], [cx + s * 0.07, cy - s * 0.22], [cx + s * 0.15, cy - s * 0.36]], S(C.gold));
      text(R, '$', cx, cy + s * 0.27, { f: 'MM 500', size: s * 0.5, color: C.gold, alpha, align: 'center' });
      break;
    }
    case '🔐': {
      arc(R, cx, cy - s * 0.04, s * 0.19, Math.PI, Math.PI * 2, S(C.gold, Math.max(1.5, s * 0.09)));
      line(R, cx - s * 0.19, cy - s * 0.04, cx - s * 0.19, cy + s * 0.02, S(C.gold, Math.max(1.5, s * 0.09)));
      line(R, cx + s * 0.19, cy - s * 0.04, cx + s * 0.19, cy + s * 0.02, S(C.gold, Math.max(1.5, s * 0.09)));
      rrect(R, cx - s * 0.31, cy + s * 0.02, s * 0.62, s * 0.4, s * 0.07, { ...S(C.gold), fill: C.gold, fillAlpha: 0.22 });
      circle(R, cx, cy + s * 0.2, s * 0.05, { fill: C.gold, alpha });
      break;
    }
    case '🌊': {
      for (let k = 0; k < 2; k++) {
        const pts = [];
        for (let i = 0; i <= 16; i++) pts.push([cx - s * 0.42 + (i / 16) * s * 0.84, cy - s * 0.1 + k * s * 0.24 + Math.sin((i / 16) * Math.PI * 2) * s * 0.07]);
        poly(R, pts, S('#5b8cff', Math.max(1.4, s * 0.09)));
      }
      break;
    }
    case '🗺': {
      const X = [-0.42, -0.14, 0.14, 0.42].map(v => cx + v * s), up = [-0.3, -0.38, -0.3, -0.38], dn = [0.36, 0.28, 0.36, 0.28];
      poly(R, [...X.map((x0, i) => [x0, cy + up[i] * s]), ...X.map((x0, i) => [x0, cy + dn[i] * s]).reverse(), [X[0], cy + up[0] * s]], { ...S(C.teal), fill: C.teal, fillAlpha: 0.14 });
      for (const i of [1, 2]) line(R, X[i], cy + up[i] * s, X[i], cy + dn[i] * s, S(C.teal, 1));
      break;
    }
    case '⚠': {
      const r = s * 0.38;
      poly(R, [[cx, cy - r], [cx + r * 1.08, cy + r * 0.8], [cx - r * 1.08, cy + r * 0.8], [cx, cy - r]], { ...S(C.amber), fill: C.amber, fillAlpha: 0.12 });
      line(R, cx, cy - r * 0.32, cx, cy + r * 0.24, S(C.amber));
      circle(R, cx, cy + r * 0.52, s * 0.04, { fill: C.amber, alpha });
      break;
    }
    default: break;
  }
}

// ── the comment's blocks, as render_comment.py lays them out ────────────
function parseComment(md) {
  const lines = md.replace(/\n$/, '').split('\n');
  const blocks = [];
  let para = null;
  const flush = () => { if (para) blocks.push({ kind: 'p', text: para.join(' ') }); para = null; };
  for (let i = 0; i < lines.length; i++) {
    const l = lines[i];
    let m;
    if (!l.trim()) flush();
    else if (l.startsWith('|')) {
      flush();
      const rows = [];
      while (i < lines.length && lines[i].startsWith('|')) rows.push(lines[i++]);
      i--;
      const cells = r => r.slice(1, -1).split(/(?<!\\)\|/).map(c => c.trim().replace(/\\\|/g, '|'));
      blocks.push({ kind: 'table', head: cells(rows[0]), rows: rows.slice(2).map(cells) });
    } else if (l.startsWith('- ')) { flush(); blocks.push({ kind: 'li', text: l.slice(2) }); }
    else if ((m = l.match(/^<details><summary>(.*)<\/summary>$/))) { flush(); blocks.push({ kind: 'summary', text: m[1] }); }
    else if (l === '```mermaid') {
      flush();
      const body = [];
      while (lines[++i] !== '```') body.push(lines[i]);
      blocks.push({ kind: 'mermaid', text: body.join('\n') });
    } else if (l === '</details>') flush();
    // the footer's rule and small print (typeset as the renderer means
    // them: GitHub needs a blank line before `---` to agree)
    else if (l === '---') { flush(); blocks.push({ kind: 'hr' }); }
    else if ((m = l.match(/^<sub>(.*)<\/sub>$/))) { flush(); blocks.push({ kind: 'sub', text: m[1] }); }
    else if (/^<!-- .* -->$/.test(l)) { flush(); blocks.push({ kind: 'marker', text: l }); }
    else (para ||= []).push(l);
  }
  flush();
  return blocks;
}

// the mermaid graph TD, laid out top-down like the comment renders it
const NODE = { f: 'Geist 500', size: 18 };
function layoutGraph(src) {
  const nodes = [], edges = [], cls = {};
  for (const l of src.split('\n')) {
    let m;
    if ((m = l.match(/^\s*(\w+)\["([^"]+)"\]:::(\w+)\s*$/))) nodes.push({ id: m[1], label: m[2], cls: m[3] });
    else if ((m = l.match(/^\s*(\w+) --> (\w+)\s*$/))) edges.push([m[1], m[2]]);
    else if ((m = l.match(/^\s*classDef (\w+) fill:(#[0-9a-fA-F]{6})([0-9a-fA-F]{2})?,stroke:(#[0-9a-fA-F]{6}),color:(#[0-9a-fA-F]{6})/))) {
      cls[m[1]] = { fill: m[2], fillA: m[3] ? parseInt(m[3], 16) / 255 : 1, stroke: m[4], color: m[5] };
    }
  }
  if (!/^graph TD\b/.test(src) || !nodes.length) fail('the clean comment no longer carries a graph TD');
  const rank = {};
  const visit = (id, depth = 0) => {
    if (depth > nodes.length) fail('the graph has a cycle');
    return (rank[id] ??= Math.max(0, ...edges.filter(([, b]) => b === id).map(([a]) => visit(a, depth + 1) + 1)));
  };
  nodes.forEach(n => visit(n.id));
  const H = 40, GAP = 30;
  const ranks = Math.max(...Object.values(rank)) + 1;
  const pos = {};
  for (let r = 0; r < ranks; r++) {
    const row = nodes.filter(n => rank[n.id] === r);
    const ws = row.map(n => measure(n.label, NODE) + 44);
    let x = BODY_W / 2 - (ws.reduce((s, w) => s + w, 0) + (row.length - 1) * 30) / 2;
    row.forEach((n, i) => { pos[n.id] = { x, y: 8 + r * (H + GAP), w: ws[i], h: H }; x += ws[i] + 30; });
  }
  // the draw order: ranks top-down, each rank's incoming edges before it
  const order = [];
  for (let r = 0; r < ranks; r++) {
    for (const [a, b] of edges) if (rank[b] === r && !order.some(o => o.edge && o.edge[0] === a && o.edge[1] === b)) order.push({ edge: [a, b] });
    for (const n of nodes) if (rank[n.id] === r) order.push({ node: n });
  }
  return { nodes, edges, cls, pos, order, rank, ranks, h: 8 + ranks * H + (ranks - 1) * GAP + 10 };
}

const TABLE = { head: { f: 'Geist 600', size: 17, color: C.ink }, pad: 13, lh: 26 };
function layoutTable(b) {
  const P = TABLE.pad;
  const cellRuns = b.rows.map(r => r.map(inline));
  const w0 = Math.max(measure(b.head[0], TABLE.head), ...b.rows.map(r => measure(r[0], ST.text))) + 2 * P;
  const w1 = Math.max(measure(b.head[1], TABLE.head), ...cellRuns.map(r => layoutInline(r[1], 9999)[0].reduce((w, a) => Math.max(w, a.x + a.w), 0))) + 2 * P;
  const cols = [w0, w1, BODY_W - w0 - w1];
  const rows = cellRuns.map(r => r.map((runs, c) => layoutInline(runs, cols[c] - 2 * P)));
  const rowH = rows.map(r => Math.max(...r.map(c => c.length)) * TABLE.lh + 18);
  return { cols, rows, rowH, headH: 40, h: 40 + rowH.reduce((s, h) => s + h, 0) };
}

function measureBlock(b, i) {
  if (b.kind === 'p' || b.kind === 'li') {
    const rows = layoutInline(inline(b.text), BODY_W - (b.kind === 'li' ? 30 : 0));
    return { ...b, rows, h: rows.length * LHB, key: i === 0 ? 'verdict' : `${b.kind}:${b.text}` };
  }
  if (b.kind === 'table') { const L = layoutTable(b); return { ...b, L, h: L.h, key: 'table' }; }
  if (b.kind === 'summary') return { ...b, rows: layoutInline(inline(b.text), BODY_W), h: LHB, key: 'summary' };
  if (b.kind === 'mermaid') { const G = layoutGraph(b.text); return { ...b, G, h: G.h, key: 'mermaid' }; }
  if (b.kind === 'hr') return { ...b, h: 1, key: 'hr' };
  if (b.kind === 'sub') return { ...b, rows: layoutInline(inline(b.text), BODY_W, SUB), h: 22, key: `sub:${b.text}` };
  return { ...b, h: 24, key: 'marker' };
}
const gapBefore = (prev, b) => (!prev ? 0 : b.kind === 'li' ? 2 : b.kind === 'mermaid' ? 6 : b.kind === 'sub' ? 10 : b.kind === 'marker' ? 6 : prev.kind === 'hr' ? 10 : 13);

const BLOCKS = { red: parseComment(RED.md).map(measureBlock), clean: parseComment(CLEAN.md).map(measureBlock) };
// the union of both bodies, in order: what stays, what folds, what opens
const UNION = lineDiff(BLOCKS.red.map(b => b.key), BLOCKS.clean.map(b => b.key)).map(o => ({
  op: o.op, red: o.a !== undefined ? BLOCKS.red[o.a] : null, clean: o.b !== undefined ? BLOCKS.clean[o.b] : null,
}));
UNION.forEach(u => { u.b = u.clean ?? u.red; });
const openK = (t, a, d = 0.5) => E.inOutCubic(seg(t, a, a + d));
// the finding folds while the new lines open, so the card changes height
// once, then grows as the graph opens
function presence(u, t) {
  if (u.op === '=') return 1;
  if (u.op === '-') return 1 - openK(t, T.morph, 0.5);
  if (u.b.kind === 'summary') return openK(t, T.morph + 0.08, 0.45);
  if (u.b.kind === 'mermaid') return openK(t, T.dag, 0.55);
  return openK(t, T.morph, 0.45);
}
// every block's top (card-relative) and the card's height at time t
function bodyLayout(t) {
  let y = HEAD_H + PAD_T, prev = null;
  const out = [];
  for (const u of UNION) {
    const k = presence(u, t);
    const g = gapBefore(prev, u.b) * k;
    out.push({ u, k, y: y + g });
    y += g + u.b.h * k;
    if (k > 0.5) prev = u.b;
  }
  return { rows: out, h: y + PAD_B };
}
const H_RED = bodyLayout(0).h, H_CLEAN = bodyLayout(99).h;
const cardH = t => bodyLayout(t).h;

// the verdict line, in segments: a changed segment flips, the rest glide
const splitVerdict = md => {
  const first = md.split('\n')[0];
  const [iconCh, rest] = [first.slice(0, first.indexOf(' ')), first.slice(first.indexOf(' ') + 1)];
  const [who, facts] = rest.split(' — ');
  const segs = [iconCh, who, ...facts.split(' · ')];
  const seps = [' ', ' — ', ...facts.split(' · ').slice(1).map(() => ' · ')];
  return { segs, seps };
};
function layoutVerdict(md) {
  const { segs, seps } = splitVerdict(md);
  let x = 0;
  const out = [];
  segs.forEach((s, i) => {
    if (i) {
      const w = measure(seps[i - 1], ST.text);
      out.push({ sep: true, s: seps[i - 1], atoms: [{ t: 'text', s: seps[i - 1], x: 0, w, st: ST.text }], x, w });
      x += w;
    }
    const atoms = layoutInline(inline(s), 9999)[0];
    const w = atoms.reduce((m, a) => Math.max(m, a.x + a.w), 0);
    out.push({ s, atoms, x, w });
    x += w;
  });
  return out;
}
const VERDICT = { red: layoutVerdict(RED.md), clean: layoutVerdict(CLEAN.md) };
if (VERDICT.red.length !== VERDICT.clean.length) fail('the verdict lines no longer share a shape');

// ── helpers in world space ──────────────────────────────────────────────
const rowY = n => ROW0 + (n - FIRST) * DLH; // diff row baseline, before the fix opens a row
const FIXED_N = CHANGED.find(o => o.op === '-').a + 1;
const COL = s => DIFF.x + 84 + s * cw(CODE);
const tableAt = t => {
  const r = bodyLayout(t).rows.find(r => r.u.b.kind === 'table');
  return r ? CARD.y + r.y : null;
};
// the finding's `asses` chip in the table, and its line in the diff
function beamEnds(t) {
  const tb = BLOCKS.red.find(b => b.kind === 'table');
  const L = tb.L, top = tableAt(t);
  let from = null;
  L.rows[0][2].forEach((row, ri) => row.forEach(a => {
    if (a.t === 'code' && a.s === FINDING.offending) from = [BODY_X + L.cols[0] + L.cols[1] + TABLE.pad + a.x + a.w / 2, top + L.headH + 9 + (ri + 1) * TABLE.lh - 10];
  }));
  const src = RED.file[FIXED_N - 1];
  const i = src.indexOf(`tasks.${FINDING.offending}.`) + 6;
  const to = [COL(i + FINDING.offending.length / 2), rowY(FIXED_N) - 5];
  return { from, to, toX: [COL(i), COL(i + FINDING.offending.length)] };
}

// ── camera ──────────────────────────────────────────────────────────────
const S1 = frameBox({ x: RIGHT.x, y: COMMIT1_Y - 30, w: RIGHT.w, h: CARD.y + H_RED - (COMMIT1_Y - 30) }, 18);
const S2 = frameBox({ x: DIFF.x, y: rowY(14) - 34, w: DIFF.w, h: rowY(LAST) + DLH + 18 - (rowY(14) - 34) }, 18);
const S3 = frameBox({ x: RIGHT.x, y: CARD.y, w: RIGHT.w, h: H_CLEAN + 76 }, 18);
const SHOTS = [
  { at: 0, cam: WIDE },
  { at: 4.5, cam: S1, move: 0.6 },
  { at: 10.0, cam: S2, move: 0.6 },
  { at: 12.9, cam: S3, move: 0.6 },
  { at: 19.3, cam: WIDE, move: 0.6 },
];
export const camera = t => cameraPath(t, SHOTS);
// a focus pull: the column the camera is not on recedes while it is in
const held = (t, i) => smooth(SHOTS[i].at - SHOTS[i].move, SHOTS[i].at, t) * (1 - smooth(SHOTS[i + 1].at - SHOTS[i + 1].move, SHOTS[i + 1].at, t));
// (the first commit sits above the last close-up's frame, under the kicker)
const focus = t => ({ left: 1 - 0.68 * (held(t, 1) + held(t, 3)), right: 1 - 0.68 * held(t, 2), above: 1 - 0.85 * held(t, 3) });

// the grid behind the page is a far plane: it moves less than the page
// while the camera travels (and holds with it); the light stays put, so
// the GIF does not repaint the background's gradient on every move
export function env(t) {
  const cam = camera(t);
  return { bgGlow: 1, gridAlpha: 0.22, gridX: -(cam.x - 960) * 0.12, gridY: -(cam.y - 540) * 0.12, bgY: 560 };
}

export function chrome(R, t) {
  const a = loopFade(t, meta.duration, 0.15, 0.2);
  const cam = camera(t);
  frame(R, t, { kicker: 'nika-action · the pull request gate', plate: `comment: the action's own renderer on nika ${ENGINE} check --json and inspect · the pull request page and its lighting are an illustration`, alpha: a, scrim: smooth(1, 1.12, cam.s) });
  // the accent is still undecided at the start and teal once the verdict
  // is clean (it changes while the camera is in and the title is away)
  headline({ ...R, fade: a * titleFade(cam) }, t, 0.2, meta.duration + 1, 'Every pull request gets a verdict.', 'BEFORE ANYONE SPENDS A TOKEN · NOTHING RUNS · NO SECRETS', { accent: 'a verdict.', accentColor: t < T.flip ? C.ice : C.teal });
}

// a small right-aligned label in a rounded box, vertically centred on y
function tag(R, x, y, label, color, alpha) {
  if (alpha <= 0) return;
  const st = { f: 'MGW 500', size: 10.5, tracking: 2.2 };
  const w = measure(label, st) + 20;
  rrect(R, x - w, y - 9, w, 18, 9, { color, w: 1.1, alpha, fill: color, fillAlpha: 0.1, glow: 0.3 });
  text(R, label, x - w + 10, y + 4, { ...st, color, alpha });
}

// ── the pull request (illustration) ─────────────────────────────────────
function prHeader(R, t, a) {
  const k = smooth(T.page, T.page + 0.4, t) * a;
  if (k <= 0) return;
  pill(R, LEFT.x, 272, 'OPEN', C.ice, k);
  text(R, 'Add the pr-risk-review workflow', LEFT.x + 84, 281, { f: 'Geist 600', size: 26, tracking: -0.4, color: C.ink, alpha: k });
}

// a commit in the timeline and the action's check on it
function status(R, t, x, y, { t0, done, ok, a }) {
  if (t < t0) return;
  const k = smooth(t0, t0 + 0.2, t) * a;
  if (t < done) {
    const ph = ((t - t0) * 1.1) % 1;
    circle(R, x, y, 5, { fill: C.amber, alpha: k, glow: 0.8 });
    circle(R, x, y, 5 + ph * 9, { color: C.amber, w: 1.4, alpha: k * (1 - ph) * 0.8, glow: 0.4 });
    return;
  }
  const p = E.outBack(seg(t, done, done + 0.35));
  const col = ok ? C.teal : C.red;
  circle(R, x, y, 9 * p, { fill: col, fillAlpha: 0.16, color: col, w: 1.5, alpha: a, glow: 0.6 });
  if (ok) check(R, x, y + 0.5, 9, E.snap(seg(t, done + 0.05, done + 0.35)), { color: col, w: 2, alpha: a, glow: 0.7 });
  else cross(R, x, y, 7, E.snap(seg(t, done + 0.05, done + 0.35)), { color: col, w: 2, alpha: a, glow: 0.7 });
  if (t < done + 0.6) circle(R, x, y, 9 + 26 * E.outCubic(seg(t, done, done + 0.6)), { color: col, w: 1.4, alpha: a * (1 - seg(t, done, done + 0.6)), glow: 0.8 });
}
function commit(R, t, y, { msg, t0, done, ok, exit, a }) {
  const k = E.snap(seg(t, t0, t0 + 0.45));
  if (k <= 0) return;
  const al = a * k, x = RIGHT.x, dy = 8 * (1 - k);
  circle(R, x + 12, y - 7 + dy, 7, { color: C.mist, w: 1.8, alpha: al, fill: C.bg0 });
  text(R, msg, x + 34, y + dy, { f: 'Geist 500', size: 19, color: C.ink, alpha: al });
  const sy = y + 26 + dy;
  status(R, t, x + 42, sy - 5, { t0: t0 + 0.2, done, ok, a: al });
  const col = t < done ? C.amber : ok ? C.teal : C.red;
  mono(R, [{ s: STEP_CHECK, c: t < done ? C.amber : C.mist }], x + 60, sy, { alpha: al * smooth(t0 + 0.2, t0 + 0.4, t), st: { f: 'MM 400', size: 13.5 } });
  const res = t < done ? 'in progress' : `exit ${exit}`;
  text(R, res, x + RIGHT.w - 6, sy, { f: 'MM 500', size: 14, color: col, alpha: al * smooth(t0 + 0.2, t0 + 0.4, t), align: 'right', glow: t < done ? 0 : 0.4 });
}

// ── the diff: the file the pull request adds, then the push that fixes it
function diffView(R, t, a) {
  const k = E.snap(seg(t, T.page, T.page + 0.55));
  if (k <= 0) return;
  rrect(R, DIFF.x, DIFF.y, DIFF.w, Math.max(44, DIFF.h * k), 12, { color: C.faint, w: 1, alpha: a, fill: '#050c19', fillAlpha: 0.92 });
  if (k < 1) return;
  const pushed = smooth(T.push2, T.push2 + 0.3, t);
  const mp = E.inOutCubic(seg(t, T.fix, T.fixEnd));
  const tint = E.inOutCubic(seg(t, T.fix, T.fix + 0.25)); // quick: a slow fade repaints every row
  // header: the file, and what this view shows
  const ha = a * smooth(T.page + 0.3, T.page + 0.6, t);
  rect(R, DIFF.x + 1, DIFF.y + 1, DIFF.w - 2, DHEAD - 1, { fill: '#0a1628', alpha: ha * 0.8 });
  line(R, DIFF.x + 1, DIFF.y + DHEAD, DIFF.x + DIFF.w - 1, DIFF.y + DHEAD, { color: C.line, w: 1, alpha: ha });
  poly(R, [[DIFF.x + 22, DIFF.y + 11], [DIFF.x + 32, DIFF.y + 11], [DIFF.x + 37, DIFF.y + 16], [DIFF.x + 37, DIFF.y + 30], [DIFF.x + 22, DIFF.y + 30], [DIFF.x + 22, DIFF.y + 11]], { color: C.mist, w: 1.2, alpha: ha });
  text(R, 'flows/pr-risk-review.nika', DIFF.x + 50, DIFF.y + 26, { f: 'MM 500', size: 15, color: C.ink, alpha: ha });
  const adds = OPS.filter(o => o.op === '+').length, dels = OPS.filter(o => o.op === '-').length;
  const tagA = [{ label: `NEW FILE · +${RED.file.length}`, c: C.teal, a: ha * (1 - pushed) }, { label: `SECOND PUSH · +${adds} −${dels}`, c: C.ice, a: ha * pushed }];
  for (const g of tagA) if (g.a > 0) pill(R, DIFF.x + DIFF.w - 16, DIFF.y + 20, g.label, g.c, g.a, 'right');
  const ctx = R.ctx;
  ctx.save();
  ctx.beginPath();
  ctx.rect(DIFF.x, DIFF.y + DHEAD + 2, DIFF.w, DIFF.h - DHEAD - 6);
  ctx.clip();
  let y = ROW0, idx = 0;
  const caught = smooth(T.fail1, T.fail1 + 0.4, t) * (1 - smooth(T.fixEnd, T.fixEnd + 0.4, t));
  for (const o of OPS) {
    const n = o.op === '+' ? o.b + 1 : o.a + 1;
    if (n < FIRST || n > LAST) continue;
    const hk = o.op === '+' ? mp : 1;
    if (hk <= 0.001) continue;
    const by = y + DLH * 0.5 * (hk - 1);
    const rv = smooth(T.rows + idx * T.every, T.rows + idx * T.every + 0.12, t);
    idx++;
    const ra = a * rv;
    if (ra > 0.004) {
      const top = by - DLH + 5;
      if (hk < 0.999) { ctx.save(); ctx.beginPath(); ctx.rect(DIFF.x, y - DLH + 5, DIFF.w, DLH * hk); ctx.clip(); }
      // the tint: every row is new in the first push; the second push
      // leaves one line out and one in
      const added = o.op === '+' ? 1 : 1 - tint;
      const removed = o.op === '-' ? tint : 0;
      if (added > 0) {
        rect(R, DIFF.x + 1, top, DIFF.w - 2, DLH, { fill: C.teal, alpha: ra * 0.06 * added });
        rect(R, DIFF.x + 1, top, 58, DLH, { fill: C.teal, alpha: ra * 0.08 * added });
      }
      if (removed > 0) {
        rect(R, DIFF.x + 1, top, DIFF.w - 2, DLH, { fill: C.red, alpha: ra * 0.1 * removed });
        rect(R, DIFF.x + 1, top, 58, DLH, { fill: C.red, alpha: ra * 0.12 * removed });
      }
      if (o.op === '+') rect(R, DIFF.x + 1, top, DIFF.w - 2, DLH, { fill: C.teal, alpha: ra * 0.22 * (1 - smooth(T.fixEnd, T.fixEnd + 0.6, t)) });
      text(R, String(n), DIFF.x + 44, by, { f: 'MM 400', size: 12.5, color: C.dim, alpha: ra * 0.85, align: 'right' });
      const mk = o.op === '-' ? (tint < 0.5 ? '+' : '−') : '+';
      const mka = o.op === '=' ? 1 - tint : 1;
      if (mka > 0) text(R, mk, DIFF.x + 54, by, { f: 'MM 500', size: 14, color: o.op === '-' && tint >= 0.5 ? C.red : C.teal, alpha: ra * mka });
      const marks = [];
      if (o.op === '-' && caught > 0) marks.push({ re: new RegExp(`\\b${FINDING.offending}\\b`), c: C.red, glow: 0.6 });
      if (o.op === '+') marks.push({ re: new RegExp(`\\b${FINDING.suggestion}\\b`), c: C.teal, glow: 0.7 });
      mono(R, yamlSpans(o.text, marks), COL(0), by, { alpha: ra * (o.op === '-' ? lerp(1, 0.72, mp) : 1), st: CODE, max: 66 });
      if (o.op === '-' && caught > 0) {
        const i = o.text.indexOf(`tasks.${FINDING.offending}.`) + 6;
        squiggle(R, COL(i), COL(i + FINDING.offending.length), by + 4, C.red, ra * caught, 0);
      }
      // the new line came with the second push
      if (o.op === '+') tag(R, DIFF.x + DIFF.w - 14, by - 5, 'SECOND PUSH', C.ice, ra * smooth(T.fix + 0.35, T.fix + 0.65, t));
      if (hk < 0.999) ctx.restore();
    }
    y += DLH * hk;
  }
  ctx.restore();

  // the static audit reads the file: a scan, nothing runs
  const sp = seg(t, T.scan, T.scanEnd);
  if (sp > 0 && sp < 1) {
    const sy = lerp(ROW0 - 22, rowY(LAST) + 8, E.inOutSine(sp));
    const sa = a * Math.min(smooth(0, 0.12, sp), 1 - smooth(0.88, 1, sp));
    rect(R, DIFF.x + 2, sy - 46, DIFF.w - 4, 46, { fill: C.ice, alpha: sa * 0.035 });
    line(R, DIFF.x + 2, sy, DIFF.x + DIFF.w - 2, sy, { color: C.ice, w: 1.4, alpha: sa * 0.9, glow: 1 });
  }
}

// ── the sticky comment ──────────────────────────────────────────────────
function drawTable(R, b, x, y, alpha, t) {
  const { cols, rows, rowH, headH } = b.L;
  const W = BODY_W, H = b.L.h;
  rrect(R, x, y, W, H, 6, { color: C.faint, w: 1, alpha });
  rect(R, x + 1, y + 1, W - 2, headH - 1, { fill: '#0b1730', alpha: alpha * 0.9 });
  line(R, x, y + headH, x + W, y + headH, { color: C.faint, w: 1, alpha });
  let cx = x;
  cols.forEach((w, i) => {
    text(R, b.head[i], cx + TABLE.pad, y + 28, { ...TABLE.head, alpha });
    if (i) line(R, cx, y, cx, y + H, { color: C.faint, w: 1, alpha });
    cx += w;
  });
  let ry = y + headH;
  rows.forEach((r, ri) => {
    let x0 = x;
    r.forEach((cell, ci) => {
      cell.forEach((row, li) => drawAtoms(R, row, x0 + TABLE.pad, ry + 9 + (li + 1) * TABLE.lh - 4, alpha, {
        [FINDING.offending]: { chip: C.red, color: '#FFD3D9', glow: 0.3 * smooth(T.hl[1], T.hl[1] + 0.3, t) },
        [FINDING.suggestion]: { chip: C.teal, color: '#D2FFF1', glow: 0.3 * smooth(T.hl[1] + 0.5, T.hl[1] + 0.8, t) },
      }));
      x0 += cols[ci];
    });
    ry += rowH[ri];
  });
}

function drawGraph(R, b, x, y, alpha, t) {
  const G = b.G;
  const n = G.order.length;
  const span = T.dagEnd - T.dag - 0.2;
  G.order.forEach((o, i) => {
    const t0 = T.dag + 0.2 + (i / n) * span, t1 = t0 + span / n + 0.15;
    const p = E.inOutCubic(seg(t, t0, t1));
    if (p <= 0) return;
    if (o.edge) {
      const A = G.pos[o.edge[0]], B = G.pos[o.edge[1]];
      const ex = x + A.x + A.w / 2, y0 = y + A.y + A.h, y1 = y + B.y - 2;
      line(R, ex, y0, ex, lerp(y0, y1, p), { color: C.mist, w: 1.6, alpha: alpha * 0.85 });
      if (p >= 1) poly(R, [[ex - 6, y1 - 9], [ex, y1], [ex + 6, y1 - 9], [ex - 6, y1 - 9]], { color: C.mist, w: 1.4, alpha: alpha * 0.9, fill: C.mist });
      if (p < 1) circle(R, ex, lerp(y0, y1, p), 4, { fill: C.cyan, alpha, glow: 1 });
      return;
    }
    const nd = o.node, P = G.pos[nd.id], c = G.cls[nd.cls] ?? { fill: C.mist, fillA: 0.15, stroke: C.mist, color: C.ink };
    const nx = x + P.x, ny = y + P.y;
    const per = [[nx + 8, ny], [nx + P.w - 8, ny], [nx + P.w, ny + 8], [nx + P.w, ny + P.h - 8], [nx + P.w - 8, ny + P.h], [nx + 8, ny + P.h], [nx, ny + P.h - 8], [nx, ny + 8], [nx + 8, ny]];
    if (p >= 1) rrect(R, nx, ny, P.w, P.h, 8, { fill: c.fill, fillAlpha: c.fillA * 1.6, alpha: alpha * smooth(t1, t1 + 0.2, t) });
    poly(R, per, { color: c.stroke, w: 1.6, alpha, glow: 0.5 * (1 - smooth(t1, t1 + 0.6, t)) + 0.15 }, p);
    const shown = nd.label.slice(0, Math.ceil(nd.label.length * clamp(p * 1.25)));
    text(R, shown, nx + P.w / 2, ny + 29, { ...NODE, color: c.color, alpha: alpha * smooth(t0 + 0.05, t0 + 0.2, t), align: 'center' });
  });
  // once whole, one pulse runs the graph top to bottom, rank by rank
  const flow = seg(t, T.dagEnd + 0.15, T.dagEnd + 1.2);
  if (flow <= 0 || flow >= 1 || G.ranks < 2) return;
  const at = E.inOutSine(flow) * (G.ranks - 1);
  for (const [a0, b0] of G.edges) {
    const u = clamp(at - G.rank[a0]);
    if (u <= 0 || u >= 1) continue;
    const A = G.pos[a0], B = G.pos[b0];
    circle(R, x + A.x + A.w / 2, lerp(y + A.y + A.h, y + B.y - 2, u), 4, { fill: C.cyan, alpha, glow: 1 });
  }
  for (const nd of G.nodes) {
    const d = Math.abs(at - G.rank[nd.id]);
    if (d >= 0.4) continue;
    const P = G.pos[nd.id], c = G.cls[nd.cls] ?? { stroke: C.mist };
    rrect(R, x + P.x - 3, y + P.y - 3, P.w + 6, P.h + 6, 10, { color: c.stroke, w: 1.4, alpha: alpha * (1 - d / 0.4), glow: 0.9 });
  }
}

function drawBlock(R, t, b, x, y, alpha) {
  switch (b.kind) {
    case 'p': b.rows.forEach((row, i) => drawAtoms(R, row, x, y + BASE + i * LHB, alpha)); break;
    case 'li':
      circle(R, x + 10, y + BASE - 7, 3, { fill: C.mist, alpha });
      b.rows.forEach((row, i) => drawAtoms(R, row, x + 30, y + BASE + i * LHB, alpha));
      break;
    case 'table': drawTable(R, b, x, y, alpha, t); break;
    case 'summary': {
      const open = E.inOutCubic(seg(t, T.dag - 0.05, T.dag + 0.2));
      const cx = x + 8, cy = y + BASE - 7, r = 6, an = open * Math.PI / 2;
      const P = [[r, 0], [-r * 0.6, -r * 0.8], [-r * 0.6, r * 0.8]].map(([px, py]) => [cx + px * Math.cos(an) - py * Math.sin(an), cy + px * Math.sin(an) + py * Math.cos(an)]);
      poly(R, [...P, P[0]], { color: C.mist, fill: C.mist, w: 1, alpha });
      drawAtoms(R, b.rows[0], x + 24, y + BASE, alpha);
      break;
    }
    case 'mermaid': drawGraph(R, b, x, y, alpha, t); break;
    case 'hr': line(R, x, y, x + BODY_W, y, { color: C.faint, w: 1, alpha }); break;
    case 'sub': drawAtoms(R, b.rows[0], x, y + 16, alpha); break;
    default: break;
  }
}

function verdictLine(R, t, x, y, alpha) {
  const f = t - T.flip;
  const glide = E.inOutCubic(seg(t, T.flip, T.flip + 0.6));
  const red = VERDICT.red, clean = VERDICT.clean;
  let order = 0;
  red.forEach((sr, i) => {
    const sc = clean[i];
    const sx = lerp(sr.x, sc.x, glide);
    const same = sr.sep || sr.s === sc.s;
    if (same) { drawAtoms(R, sr.atoms, x + sx, y, alpha); return; }
    // a changed segment flips over, staggered left to right
    const d = 0.09 * order++;
    const u = seg(f, d, d + 0.34);
    const squash = u < 0.5 ? 1 - E.inCubic(u * 2) : E.outBack(u * 2 - 1);
    const seg1 = u < 0.5 ? sr : sc;
    if (squash <= 0.01) return;
    const ctx = R.ctx;
    const cy = y - 7;
    ctx.save();
    ctx.translate(0, cy);
    ctx.scale(1, squash);
    ctx.translate(0, -cy);
    drawAtoms(R, seg1.atoms, x + sx, y, alpha, seg1 === sr && /finding/.test(sr.s) ? { [sr.s]: { color: C.red } } : {});
    ctx.restore();
  });
}

// With its default token (github.token) the action's comment is posted
// by the GitHub Actions bot account: the author line says so, beside a
// neutral bot glyph (the page is an illustration; no logo is drawn).
const AUTHOR = 'github-actions';
function avatar(R, x, y, a) {
  circle(R, x, y, 15, { color: C.ice, w: 1.2, alpha: a, fill: '#10213d' });
  rrect(R, x - 8, y - 5, 16, 12, 3, { color: C.ice, w: 1.4, alpha: a });
  circle(R, x - 3.5, y + 1, 1.6, { fill: C.ice, alpha: a });
  circle(R, x + 3.5, y + 1, 1.6, { fill: C.ice, alpha: a });
  line(R, x, y - 5, x, y - 9, { color: C.ice, w: 1.4, alpha: a });
  circle(R, x, y - 10, 1.6, { fill: C.ice, alpha: a });
}

// The action's job on the first push, in the card the comment will fill:
// the steps tick as the job runs, then the print head wipes them into the
// comment the job posted.
const JOB_LH = 33, JOB_H = HEAD_H + 14 + JOB.length * JOB_LH + 10;
const headY = t => CARD.y + HEAD_H + (H_RED - HEAD_H) * E.inOutSine(seg(t, T.post, T.posted));
function jobSteps(R, t, a) {
  JOB.forEach((s, i) => {
    const top = CARD.y + HEAD_H + 14 + i * JOB_LH;
    const al = a * smooth(T.job + 0.25 + i * 0.05, T.job + 0.45 + i * 0.05, t);
    if (al <= 0.004) return;
    const y = top + 22, ix = BODY_X + 8, iy = y - 6;
    const skip = s.end === 'skip' && t >= s.t0, running = !skip && t >= s.t0 && t < s.t1, done = !skip && t >= s.t1;
    if (skip) {
      circle(R, ix, iy, 7, { color: C.dim, w: 1.3, alpha: al });
      line(R, ix - 4.5, iy + 4.5, ix + 4.5, iy - 4.5, { color: C.dim, w: 1.3, alpha: al });
    } else if (running) arc(R, ix, iy, 7, (t - s.t0) * 8, (t - s.t0) * 8 + Math.PI * 1.4, { color: C.amber, w: 1.8, alpha: al, glow: 0.7 });
    else if (done) {
      circle(R, ix, iy, 8, { fill: C.teal, fillAlpha: 0.14, color: C.teal, w: 1.3, alpha: al });
      check(R, ix, iy + 0.5, 8, E.snap(seg(t, s.t1, s.t1 + 0.25)), { color: C.teal, w: 1.8, alpha: al, glow: 0.6 });
    } else circle(R, ix, iy, 7, { color: C.faint, w: 1.3, alpha: al });
    const col = skip ? C.dim : running ? C.amber : done ? C.ink : C.mist;
    text(R, s.name, ix + 22, y, { f: 'Geist 500', size: 17, color: col, alpha: al, glow: running ? 0.25 : 0 });
    if (skip) text(R, 'skipped · mode: check', CARD.x + CARD.w - PAD_X, y, { f: 'Geist 400', size: 15, color: C.dim, alpha: al, align: 'right' });
  });
}

function comment(R, t, a) {
  if (t < T.job) return null;
  const L = bodyLayout(t);
  const ctx = R.ctx;
  const hy = headY(t);
  const printing = t >= T.post && t < T.posted;
  const vis = t < T.post ? Math.max(HEAD_H, JOB_H * E.snap(seg(t, T.job, T.job + 0.45))) : t < T.posted ? Math.max(JOB_H, hy - CARD.y + 2) : L.h;
  const flipped = smooth(T.flip, T.flip + 0.5, t);
  const redTone = smooth(T.post + 0.4, T.post + 0.9, t) * (1 - flipped);
  // the card: its edge carries the verdict's colour
  rrect(R, CARD.x, CARD.y, CARD.w, vis, 12, { color: C.faint, w: 1, alpha: a, fill: '#060d1a', fillAlpha: 0.95 });
  if (redTone > 0) rrect(R, CARD.x, CARD.y, CARD.w, vis, 12, { color: C.red, w: 1.4, alpha: a * 0.5 * redTone, glow: 0.35 });
  if (flipped > 0) rrect(R, CARD.x, CARD.y, CARD.w, vis, 12, { color: C.teal, w: 1.4, alpha: a * 0.55 * flipped, glow: 0.4 });
  // header: the job, then who posted the comment and that it was edited
  rect(R, CARD.x + 1, CARD.y + 1, CARD.w - 2, HEAD_H - 1, { fill: '#0b1730', alpha: a * 0.85 });
  line(R, CARD.x + 1, CARD.y + HEAD_H, CARD.x + CARD.w - 1, CARD.y + HEAD_H, { color: C.line, w: 1, alpha: a });
  avatar(R, CARD.x + 30, CARD.y + 24, a);
  // the header rolls over from the job to the comment it posted
  const hc = E.inOutCubic(seg(t, T.post, T.post + 0.35));
  ctx.save();
  ctx.beginPath();
  ctx.rect(CARD.x + 50, CARD.y + 2, CARD.w - 52, HEAD_H - 3);
  ctx.clip();
  if (hc < 1) {
    const y = CARD.y + 30 - 22 * hc, ja = a * (1 - hc);
    text(R, RUN.action.name, CARD.x + 56, y, { f: 'Geist 600', size: 17, color: C.ink, alpha: ja });
    text(R, 'job', CARD.x + 64 + measure(RUN.action.name, { f: 'Geist 600', size: 17 }), y, { f: 'Geist 400', size: 16, color: C.dim, alpha: ja });
  }
  if (hc > 0) {
    const ha = a * hc, dy = 22 * (1 - hc), y = CARD.y + 30 + dy;
    text(R, AUTHOR, CARD.x + 56, y, { f: 'Geist 600', size: 17, color: C.ink, alpha: ha });
    const nw = measure(AUTHOR, { f: 'Geist 600', size: 17 });
    rrect(R, CARD.x + 64 + nw, CARD.y + 14 + dy, 38, 20, 10, { color: C.dim, w: 1, alpha: ha });
    text(R, 'bot', CARD.x + 64 + nw + 19, CARD.y + 28 + dy, { f: 'Geist 500', size: 12, color: C.mist, alpha: ha, align: 'center' });
    text(R, 'commented', CARD.x + 114 + nw, y, { f: 'Geist 400', size: 16, color: C.dim, alpha: ha });
    const ed = smooth(T.flip, T.flip + 0.3, t);
    if (ed > 0) text(R, '· edited', CARD.x + 122 + nw + measure('commented', { f: 'Geist 400', size: 16 }), y, { f: 'Geist 500', size: 16, color: C.teal, alpha: ha * ed, glow: 0.3 });
  }
  ctx.restore();
  ctx.save();
  ctx.beginPath();
  ctx.rect(CARD.x, CARD.y + HEAD_H, CARD.w, vis - HEAD_H);
  ctx.clip();
  // the print head is a hard wipe: the job below it, the comment above it
  const wipe = (y0, y1) => { ctx.save(); ctx.beginPath(); ctx.rect(CARD.x, y0, CARD.w, y1 - y0); ctx.clip(); };
  if (t < T.posted) {
    wipe(printing ? hy : CARD.y, CARD.y + 2000);
    jobSteps(R, t, a);
    ctx.restore();
  }
  if (t >= T.post) {
    wipe(CARD.y, printing ? hy : CARD.y + 2000);
    for (const r of L.rows) {
      if (r.k <= 0.001) continue;
      const b = r.u.b;
      const pa = a * (r.u.op === '=' ? 1 : smooth(0.35, 1, r.k));
      if (pa <= 0.004 || b.kind === 'marker') continue;
      const y = CARD.y + r.y;
      const partial = r.k < 0.999;
      if (partial) { ctx.save(); ctx.beginPath(); ctx.rect(CARD.x, y - 4, CARD.w, b.h * r.k + 8); ctx.clip(); }
      if (r.u.b.key === 'verdict') verdictLine(R, t, BODY_X, y + BASE, pa);
      else drawBlock(R, t, b, BODY_X, y, pa);
      if (partial) ctx.restore();
    }
    ctx.restore();
  }
  ctx.restore();
  // the verdict, lit once it has turned
  const vk = smooth(T.flip + 0.35, T.flip + 0.8, t);
  if (vk > 0) {
    const y = CARD.y + L.rows[0].y - 4;
    rect(R, CARD.x + 5, y, 3, LHB + 8, { fill: C.teal, alpha: a * vk, glow: 0.8 });
    rect(R, CARD.x + 9, y, CARD.w - 14, LHB + 8, { fill: C.teal, alpha: a * vk * 0.045 });
  }
  // the red verdict lands: a flare at its mark as the print head passes
  const rf = seg(t, T.post + 0.1, T.post + 0.75);
  if (rf > 0 && rf < 1) {
    const vx = BODY_X + 12, vy = CARD.y + HEAD_H + PAD_T + BASE - 7;
    light(R, vx, vy, 60 + 50 * rf, C.red, 0.3 * (1 - rf), 1);
    circle(R, vx, vy, 12 + 56 * E.outCubic(rf), { color: C.red, w: 1.5, alpha: a * (1 - rf), glow: 1 });
  }
  if (printing) {
    rect(R, CARD.x + 2, hy - 34, CARD.w - 4, 34, { fill: C.ice, alpha: a * 0.035 });
    line(R, CARD.x + 8, hy, CARD.x + CARD.w - 8, hy, { color: C.ice, w: 1.6, alpha: a * 0.9, glow: 1 });
  }
  return t >= T.post ? L : null;
}

// the reading guide over the red comment: one line lit at a time
function guide(R, t, a) {
  if (t < T.hl[0] || t > T.beam + 0.4) return;
  const L = bodyLayout(t);
  const keyOf = i => [
    r => r.u.b.key === 'verdict',
    r => r.u.b.kind === 'table',
    r => r.u.b.key.startsWith('p:💰'),
    r => r.u.b.key.startsWith('p:🔐'),
  ][i];
  const out = 1 - smooth(T.beam, T.beam + 0.4, t);
  T.hl.forEach((h, i) => {
    const r = L.rows.find(keyOf(i));
    if (!r) return;
    const end = T.hl[i + 1] ?? T.beam;
    const k = smooth(h, h + 0.3, t) * (1 - smooth(end, end + 0.3, t) * (i < 3 ? 0.75 : 0)) * out;
    if (k <= 0) return;
    const col = i === 2 ? C.gold : i === 3 ? C.ice : C.red;
    const y = CARD.y + r.y - 4, hh = (i === 2 ? r.u.b.h + LHB + 2 : r.u.b.h) + 8;
    rect(R, CARD.x + 4, y, 4, hh, { fill: col, alpha: a * k, glow: 0.8 });
    rect(R, CARD.x + 10, y, CARD.w - 14, hh, { fill: col, alpha: a * k * 0.05 });
  });
  // the cost floor: underlined as it is read
  const r = L.rows.find(keyOf(2));
  if (r) {
    const bold = r.u.b.rows[0].find(x => x.t === 'bold');
    const u = E.inOutCubic(seg(t, T.hl[2] + 0.15, T.hl[2] + 0.6)) * out;
    if (bold && u > 0) line(R, BODY_X + bold.x, CARD.y + r.y + BASE + 6, BODY_X + bold.x + bold.w * u, CARD.y + r.y + BASE + 6, { color: C.gold, w: 2, alpha: a, glow: 0.8 });
  }
}

// the finding points at its line: a beam from the table to the diff
function beam(R, t, a) {
  const on = smooth(T.beam, T.beam + 0.15, t) * (1 - smooth(T.fixEnd - 0.1, T.fixEnd + 0.3, t));
  if (on <= 0) return;
  const { from, to, toX } = beamEnds(Math.min(t, T.morph - 0.01));
  if (!from) return;
  const pts = bezierPts(from, [from[0] - 260, from[1] + 40], [to[0] + 320, to[1] - 200], to, 48);
  const p = E.inOutCubic(seg(t, T.beam, T.beam + 0.85));
  const RR = { ...R, fade: a * on };
  poly(RR, pts, { color: C.red, w: 1.8, alpha: 0.85, glow: 0.9 }, p);
  circle(RR, from[0], from[1], 4, { fill: C.red, glow: 1 });
  if (p > 0 && p < 1) {
    const q = pts[Math.min(pts.length - 1, Math.round(p * (pts.length - 1)))];
    circle(RR, q[0], q[1], 5, { fill: '#FFD3D9', glow: 1 });
  }
  if (p >= 1) {
    const ring = E.outCubic(seg(t, T.beam + 0.85, T.beam + 1.4));
    circle(RR, to[0], to[1], 4, { fill: C.red, glow: 1 });
    rrect(RR, toX[0] - 5, to[1] - 12, toX[1] - toX[0] + 10, 23, 6, { color: C.red, w: 1.4, alpha: 0.9, glow: 0.8 });
    if (ring < 1) circle(RR, to[0], to[1], 20 + 40 * ring, { color: C.red, w: 1.4, alpha: 1 - ring, glow: 0.8 });
  }
}

// the second push, the hidden marker, and the verdict turning over
function upsert(R, t, a, L) {
  // x-ray: a sweep down the card finds the marker the action upserts by
  const sp = seg(t, T.xray, T.xray + 0.6);
  const H = L.h;
  if (sp > 0 && sp < 1) {
    const y = CARD.y + HEAD_H + (H - HEAD_H) * E.inOutSine(sp);
    rect(R, CARD.x + 2, y - 60, CARD.w - 4, 60, { fill: C.ice, alpha: a * 0.04 });
    line(R, CARD.x + 2, y, CARD.x + CARD.w - 2, y, { color: C.ice, w: 1.4, alpha: a * 0.9, glow: 1 });
  }
  const mk = smooth(T.xray + 0.45, T.xray + 0.7, t) * (1 - smooth(T.flip + 1.8, T.flip + 2.2, t)) * a;
  const r = L.rows.find(x => x.u.b.kind === 'marker');
  if (mk > 0 && r) {
    const y = CARD.y + r.y + 17;
    const st = { f: 'MM 400', size: 16 };
    const w = r.u.b.text.length * cw(st);
    rrect(R, BODY_X - 8, y - 17, w + 16, 24, 5, { color: C.ice, w: 1.1, alpha: mk * 0.8, dash: [4, 4], glow: 0.4 });
    // mono draws the comment's own characters, never a font ligature
    mono(R, [{ s: r.u.b.text, c: C.ice, glow: 0.4 }], BODY_X, y, { alpha: mk, st });
    text(R, STEP_STICKY, CARD.x + CARD.w - PAD_X, y, { f: 'Geist 500', size: 16, color: C.ice, alpha: mk, align: 'right' });
  }
  // the verdict turns over: a flare and a ring at the mark, a streak
  // along the line, and the new colour running round the card's edge
  const f = seg(t, T.flip, T.flip + 0.8);
  if (f > 0 && f < 1) {
    const vx = BODY_X + 12, vy = CARD.y + HEAD_H + PAD_T + BASE - 7;
    light(R, vx, vy, 70 + 60 * f, C.teal, 0.35 * (1 - f), 1);
    circle(R, vx, vy, 14 + 70 * E.outCubic(f), { color: C.teal, w: 1.6, alpha: a * (1 - f), glow: 1 });
    streak(R, vx + 380 * E.outCubic(f), vy, 260, a * 0.7 * (1 - f), '#8FFFE0', 1);
    const x0 = CARD.x, x1 = CARD.x + CARD.w, top = CARD.y, bot = CARD.y + L.h, mid = (top + bot) / 2, rr = 12;
    const p = E.inOutCubic(f);
    for (const pts of [
      [[x0, vy], [x0, top + rr], [x0 + rr, top], [x1 - rr, top], [x1, top + rr], [x1, mid]],
      [[x0, vy], [x0, bot - rr], [x0 + rr, bot], [x1 - rr, bot], [x1, bot - rr], [x1, mid]],
    ]) poly(R, pts, { color: '#8FFFE0', w: 2.4, alpha: a * (1 - smooth(0.75, 1, f)), glow: 1 }, p, Math.max(0, p - 0.16));
  }
}

export function draw(R, t) {
  const a = loopFade(t, meta.duration, 0.15, 0.2);
  const fo = focus(t);
  // the pull request slides up into place
  const enter = E.snap(seg(t, 0.2, 1.1));
  const ctx = R.ctx;
  ctx.save();
  if (enter < 1) {
    const s = lerp(0.965, 1, enter);
    ctx.translate(960, 640 + 46 * (1 - enter));
    ctx.scale(s, s);
    ctx.translate(-960, -640);
  }
  drawPage(R, t, a, fo);
  ctx.restore();
}

function drawPage(R, t, a, fo) {
  prHeader(R, t, a * fo.left);
  diffView(R, t, a * fo.left);
  const ar = a * fo.right;
  // the conversation's timeline: the first commit, the comment, the second
  const tl = E.snap(seg(t, T.commit1 + 0.2, T.job + 0.4));
  if (tl > 0) {
    const y0 = COMMIT1_Y + 1, y1 = t >= T.push2 ? CARD.y + cardH(t) + 34 - 15 : CARD.y + 12;
    line(R, RIGHT.x + 12, y0, RIGHT.x + 12, lerp(y0, y1, tl), { color: C.faint, w: 1.6, alpha: ar });
  }
  commit(R, t, COMMIT1_Y, { msg: 'Add the pr-risk-review workflow', t0: T.commit1, done: T.fail1, ok: false, exit: RED.exit, a: ar * fo.above });
  const L = comment(R, t, ar);
  if (L) {
    guide(R, t, ar);
    upsert(R, t, ar, L);
    commit(R, t, CARD.y + L.h + 34, { msg: 'Fix the task reference', t0: T.push2, done: T.pass2, ok: true, exit: CLEAN.exit, a: ar });
  }
  beam(R, t, a);
}
