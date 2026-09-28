// S6 · LOWER → CHECK → MEANING.
//
// The verified plan enters a deterministic assembler: the intelligence phase
// is over. Plan nodes land on their task headers while a print head lays down
// the program — the REAL fixture lines, with their real line numbers and
// visible folds. Then `nika check` (real rows). Check-clean is not the same
// as meant, so meaning closure follows: three rings — intent, plan, program —
// turn like a combination lock until every obligation aligns on one spoke.
import { C, E, TAU, clamp, lerp, seg, smooth, win, hash } from '../engine/core.mjs';
import { text, line, circle, rrect, rect, measure, check, light, arc } from '../engine/render.mjs';
import { T } from '../timeline.mjs';
import { NODES, nodeX } from './s4-plan.mjs';
import { pickLines, checkRow, hashes, short } from '../facts.mjs';
import { beatTitle } from './shared.mjs';

const PICK = [
  'nika: invoice-payments',
  'const:',
  '  currency: EUR',
  'permits:',
  'tasks:',
  '  read_invoices:',
  '      tool: nika:read',
  '  keep_valid:',
  '      tool: nika:jq',
  `        expression: 'fromjson | map(select(.status != "rejected"))'`,
  '  by_customer:',
  '      tool: nika:jq',
  '        expression: group_by(.customer_id)',
  '  totals:',
  '      tool: nika:jq',
  `        expression: 'map({customer_id: .[0].customer_id, amount_cents: (map(.amount_cents) | add)})'`,
  '  approval:',
  '      tool: nika:prompt',
  '  pay:',
  '    when: ${{ with.approved == true }}',
  '      tool: nika:fetch',
  '        method: POST',
];
const TASK_ROWS = [5, 7, 10, 13, 16, 18];
const TASK_IDS = ['read_invoices', 'keep_valid', 'by_customer', 'totals', 'approval', 'pay'];
const CODE = { f: 'MM 400', size: 15 };
const CODEB = { f: 'MM 500', size: 15 };
const PANEL = { x: 214, y: 150, w: 1060, lh: 23, gap: 6 };

let LAYOUT = null;
function layout() {
  if (LAYOUT) return LAYOUT;
  const lines = pickLines(PICK);
  let y = PANEL.y + 88;
  const rows = lines.map((l, i) => {
    const folded = i > 0 && l.n - lines[i - 1].n > 1;
    if (folded) y += PANEL.gap;
    const r = { ...l, y, folded };
    y += PANEL.lh;
    return r;
  });
  const cw = measure('M', CODE);
  LAYOUT = { rows, cw, bottom: y + 10 };
  return LAYOUT;
}
const codeX = () => PANEL.x + 78;

// Minimal YAML syntax coloring for the drawn excerpt.
function spans(src) {
  const out = [];
  const m = src.match(/^(\s*)([\w.-]+)(:)(.*)$/);
  if (!m) return [{ s: src, c: C.mist }];
  const [, ind, key, colon, rest] = m;
  const top = ind.length === 0, task = ind.length === 2 && TASK_IDS.includes(key);
  out.push({ s: ind, c: C.mist });
  out.push({ s: key, c: top ? C.ice : task ? C.ink : '#8DB4FF', b: top || task });
  out.push({ s: colon, c: C.dim });
  const re = /(\$\{\{.*?\}\})|('[^']*')|(nika:[\w-]+)|(EUR|POST)/g;
  let last = 0, mm;
  while ((mm = re.exec(rest))) {
    if (mm.index > last) out.push({ s: rest.slice(last, mm.index), c: C.mist });
    const tok = mm[0];
    out.push({ s: tok, c: mm[1] ? '#6FA0FF' : mm[2] ? '#C9D8EE' : mm[3] ? C.cyan : tok === 'EUR' ? C.teal : C.ink, tok });
    last = mm.index + tok.length;
  }
  if (last < rest.length) out.push({ s: rest.slice(last), c: C.mist });
  return out;
}
function drawCode(R, str, x, y, alpha, glowK = 0) {
  const { cw } = layout();
  let cx = x;
  for (const sp of spans(str)) {
    if (sp.s.trim()) text(R, sp.s, cx, y, { ...(sp.b ? CODEB : CODE), color: sp.c, alpha, glow: glowK * (sp.tok ? 0.4 : 0.15) });
    cx += sp.s.length * cw;
  }
}

// ── tunnel ─────────────────────────────────────────────────────────────
function tunnel(R, t) {
  const a = win(t, T.tunnel - 0.02, T.lower + 0.75, 0.12, 0.35);
  if (a <= 0) return;
  for (let i = 0; i < 90; i++) {
    const an = hash(i, 71) * TAU;
    const sp = 0.6 + hash(i, 72) * 1.4;
    const u = ((t - T.tunnel) * sp * 1.6 + hash(i, 73)) % 1;
    const r0 = lerp(30, 1300, u * u), r1 = r0 + lerp(10, 260, u);
    line(R, 960 + Math.cos(an) * r0, 540 + Math.sin(an) * r0, 960 + Math.cos(an) * r1, 540 + Math.sin(an) * r1,
      { color: i % 7 === 0 ? C.cyan : C.ice, w: lerp(0.5, 1.6, u), alpha: a * u * 0.55, glow: 0.4 });
  }
  light(R, 960, 540, 420, C.blue, 0.25 * a, 0.5);
}

export function env(t) {
  const close = smooth(T.closure - 0.1, T.closure + 0.4, t);
  return { bgGlow: lerp(0.9, 1.2, close), gridAlpha: 0.3 * smooth(T.lower + 0.4, T.lower + 0.9, t), gridY: 0, gridX: 0, bgY: 560 };
}

export function draw(R, t) {
  tunnel(R, t);
  beatTitle(R, t, T.lower + 0.36, T.closure - 0.05, 'Lowered, not generated.', 'DETERMINISTIC ASSEMBLER · THEN THE REAL `nika check`', { y: 948, accent: 'not generated.', accentColor: C.cyan, size: 56 });
  const toCore = E.inOutCubic(seg(t, T.closure - 0.05, T.closure + 0.45)); // the program becomes the inner ring
  const codeA = 1 - toCore;
  if (codeA > 0.01) {
    const ctx = R.ctx;
    ctx.save();
    const s = lerp(1, 0.16, toCore);
    ctx.translate(lerp(0, 960 - (PANEL.x + PANEL.w / 2) * s, toCore), lerp(0, 560 - 520 * s, toCore));
    ctx.scale(s, s);
    program(R, t, codeA);
    ctx.restore();
    checkPanel(R, t, codeA);
  }
  closure(R, t);
}

function program(R, t, alpha) {
  const L = layout();
  const frameK = E.snap(seg(t, T.lower + 0.15, T.lower + 0.55));
  const x0 = PANEL.x, y0 = PANEL.y, w = PANEL.w, h = L.bottom - y0 + 6;
  if (frameK > 0) {
    rrect(R, x0, y0, w * frameK, h, 14, { color: C.faint, w: 1, alpha: alpha, fill: C.bg0, fillAlpha: 0.82 * alpha });
    text(R, 'invoice-payments.nika', x0 + 26, y0 + 36, { f: 'MM 500', size: 14, color: C.ink, alpha: alpha * frameK });
    text(R, 'LOWERED · DETERMINISTIC ASSEMBLER', x0 + 262, y0 + 36, { f: 'MGW 500', size: 10, tracking: 3, color: C.cyan, alpha: alpha * frameK, glow: 0.3 });
    text(R, `plan ${short(hashes.plan_sha256)}  →  program ${short(hashes.program_sha256)}`, x0 + w - 26, y0 + 36, { f: 'MM 400', size: 11.5, color: C.dim, alpha: alpha * frameK, align: 'right' });
    text(R, 'same plan → same program · no model in this step', x0 + w - 26, y0 + 56, { f: 'MM 400', size: 11, color: C.teal, alpha: alpha * frameK * 0.9, align: 'right' });
    line(R, x0 + 1, y0 + 68, x0 + w - 1, y0 + 68, { color: C.faint, w: 1, alpha: alpha * frameK });
  }
  // print head
  const h0 = L.rows[0].y - 22, h1 = L.bottom;
  const headY = lerp(h0, h1, E.inOutSine(seg(t, T.lower + 0.3, T.code + 0.08)));
  const printing = win(t, T.lower + 0.28, T.code + 0.12, 0.05, 0.08);
  L.rows.forEach((r, i) => {
    const isTask = TASK_ROWS.includes(i);
    const tp = T.lower + 0.3 + (T.code + 0.08 - T.lower - 0.3) * Math.acos(1 - 2 * clamp((r.y - h0) / (h1 - h0))) / Math.PI;
    const k = isTask ? smooth(landT(TASK_ROWS.indexOf(i)), landT(TASK_ROWS.indexOf(i)) + 0.1, t) : smooth(tp, tp + 0.08, t);
    if (k <= 0) return;
    if (r.folded) text(R, '⋯', x0 + 44, r.y - PANEL.lh + 6, { f: 'MM 400', size: 12, color: C.dim, alpha: alpha * k * 0.9, align: 'right' });
    text(R, String(r.n), x0 + 50, r.y, { f: 'MM 400', size: 12, color: C.dim, alpha: alpha * k * 0.8, align: 'right' });
    const flash = 1 - smooth(tp, tp + 0.25, t);
    drawCode(R, r.text, codeX(), r.y, alpha * k, isTask ? 0.3 : flash * 0.8);
    // the answered unknown, lowered into const
    if (r.text.includes('currency: EUR')) {
      const hk = win(t, T.code - 0.2, T.check + 0.6, 0.15, 0.3);
      if (hk > 0) {
        const cx = codeX() + 12 * L.cw;
        rrect(R, cx - 5, r.y - 17, 3 * L.cw + 10, 23, 5, { color: C.teal, w: 1.2, alpha: alpha * hk, glow: 0.6 });
        text(R, '← the answer to the one unknown', codeX() + 17 * L.cw, r.y, { f: 'MM 400', size: 12, color: C.teal, alpha: alpha * hk });
      }
    }
    // check gutter ticks
    const ct = T.check + 0.05 + (i / L.rows.length) * 0.45;
    const ck = smooth(ct, ct + 0.1, t);
    if (ck > 0) check(R, x0 + 64, r.y - 5, 8, ck, { color: C.teal, w: 1.4, alpha: alpha * 0.9, glow: 0.4 });
  });
  if (printing > 0) {
    line(R, x0 + 8, headY, x0 + w - 8, headY, { color: C.cyan, w: 1.4, alpha: alpha * printing, glow: 1 });
    rect(R, x0 + 8, headY - 26, w - 16, 26, { fill: C.cyan, alpha: 0.045 * printing * alpha });
  }
  // check scan bar
  const sc = win(t, T.check, T.check + 0.55, 0.05, 0.1);
  if (sc > 0) {
    const y = lerp(L.rows[0].y - 20, L.bottom, seg(t, T.check + 0.02, T.check + 0.52));
    line(R, x0 + 8, y, x0 + w - 8, y, { color: C.teal, w: 1.2, alpha: alpha * sc, glow: 0.9 });
  }
  // plan nodes fly onto their task headers
  NODES.forEach((n, i) => {
    const r = L.rows[TASK_ROWS[i]];
    const t0 = T.lower + 0.12 + i * 0.05, t1 = landT(i);
    if (t < T.lower || t > t1 + 0.1) return;
    // the plan's own row comes out of the tunnel, then collapses into code
    const appear = E.outCubic(seg(t, T.lower - 0.06, T.lower + 0.22));
    const p = E.inOutCubic(seg(t, t0, t1));
    const sx = 960 + (nodeX(i) - 960) * lerp(1.5, 1, appear), sy = 522;
    const dx = codeX() + 2 * L.cw, dy = r.y - 5;
    const x = lerp(sx, dx, p), y = lerp(sy, dy, p);
    const m = smooth(0.45, 0.9, p);
    const a = alpha * appear * (1 - smooth(t1, t1 + 0.1, t));
    text(R, n.op, x, y, { f: 'Geist 600', size: lerp(40, 15, p) * lerp(1.6, 1, appear), color: n.human ? C.human : C.ink, alpha: a * (1 - m), baseline: 'middle', glow: 0.6, blur: (1 - appear) * 10 });
    text(R, `${TASK_IDS[i]}:`, x, y, { ...CODEB, color: C.ink, alpha: a * m, baseline: 'middle', glow: 0.5 });
  });
}
function landT(i) {
  return T.lower + 0.5 + i * 0.07;
}

const ROWS = ['PLAN', 'TYPES', 'ARGS', 'GATES', 'PERMITS', 'TRIFECTA', 'audited'];
function checkPanel(R, t, alpha) {
  const k = smooth(T.check - 0.05, T.check + 0.2, t) * alpha;
  if (k <= 0) return;
  const x = 1318, y = 300, w = 470;
  rrect(R, x, y, w, 300, 12, { color: C.faint, w: 1, alpha: k, fill: C.bg0, fillAlpha: 0.82 });
  text(R, '$ nika check invoice-payments.nika', x + 22, y + 34, { f: 'MM 500', size: 12.5, color: C.ink, alpha: k });
  ROWS.forEach((lab, i) => {
    const tr = T.checkRows[i];
    const rk = smooth(tr, tr + 0.08, t);
    if (rk <= 0) return;
    let row = checkRow(lab).replace(/^✔\s*/, '');
    const max = 52;
    if (row.length > max) row = row.slice(0, max - 1).trimEnd() + '…';
    const yy = y + 70 + i * 30;
    check(R, x + 28, yy - 5, 10, E.snap(seg(t, tr, tr + 0.15)), { color: C.teal, w: 1.6, glow: 0.6, alpha: k });
    const [head, ...tail] = row.split(/\s{2,}|\s(?=\S)/);
    text(R, head, x + 44, yy, { f: 'MM 500', size: 11.5, color: C.teal, alpha: k * rk });
    text(R, row.slice(head.length).trim(), x + 128, yy, { f: 'MM 400', size: 11.5, color: C.mist, alpha: k * rk });
  });
  const f = smooth(T.checkRows[6] + 0.05, T.checkRows[6] + 0.2, t);
  text(R, 'CHECK-CLEAN ≠ WHAT YOU MEANT', x + 22, y + 290, { f: 'MGW 500', size: 10, tracking: 3, color: C.amber, alpha: k * f, glow: 0.3 });
}

// ── meaning closure: the combination lock ──────────────────────────────
const RINGS = [
  { name: 'PROGRAM', r: 205, amp: 2.6, col: C.cyan, labels: TASK_IDS, st: { f: 'MM 500', size: 12.5 } },
  { name: 'PLAN', r: 305, amp: -1.4, col: C.ice, labels: NODES.map(n => n.op), st: { f: 'Geist 600', size: 17 } },
  { name: 'INTENT', r: 405, amp: 1.9, col: C.human, labels: ['read', 'ignore rejected', 'by customer', 'sum', 'after I approve', 'pay'], st: { f: 'Geist 400', size: 17 } },
];
const CX = 960, CY = 572;
function closure(R, t) {
  const on = smooth(T.closure, T.closure + 0.35, t) * (1 - smooth(T.ready - 0.05, T.ready + 0.3, t));
  if (on <= 0) return;
  const p = seg(t, T.closure + 0.05, T.lock);
  const locked = smooth(T.lock, T.lock + 0.12, t);
  RINGS.forEach((rg, ri) => {
    const phi = rg.amp * (1 - p) ** 2.4 + 0.05 * Math.sin((t - T.lock) * 30) * Math.exp(-(t - T.lock) * 9) * (t > T.lock ? 1 : 0);
    const grow = E.snap(seg(t, T.closure + ri * 0.06, T.closure + 0.45 + ri * 0.06));
    const r = rg.r * lerp(0.6, 1, grow);
    // ring line with tick marks
    arc(R, CX, CY, r, 0, TAU, { color: rg.col, w: 1.1, alpha: on * grow * 0.55, glow: 0.3 });
    for (let k = 0; k < 72; k++) {
      const an = (k / 72) * TAU + phi;
      const l2 = k % 12 === 0 ? 9 : 4;
      line(R, CX + Math.cos(an) * r, CY + Math.sin(an) * r, CX + Math.cos(an) * (r + l2), CY + Math.sin(an) * (r + l2), { color: rg.col, w: 0.8, alpha: on * grow * 0.4 });
    }
    // ring name travels with the ring
    const an0 = phi + 0.02;
    text(R, rg.name, CX + Math.cos(an0) * (r + 18), CY + Math.sin(an0) * (r + 18) + 4, { f: 'MGW 500', size: 9.5, tracking: 3, color: rg.col, alpha: on * grow * 0.9 });
    // notches: one per obligation
    rg.labels.forEach((lab, k) => {
      const an = -Math.PI / 2 + (k * TAU) / 6 + phi;
      const nx = CX + Math.cos(an) * r, ny = CY + Math.sin(an) * r;
      circle(R, nx, ny, 5, { fill: C.bg0, color: rg.col, w: 1.4, alpha: on * grow, glow: 0.5 });
      if (locked > 0) circle(R, nx, ny, 2.4, { fill: C.teal, alpha: on * locked, glow: 1 });
      const lr = r + (ri === 2 ? 34 : -30);
      const lx = CX + Math.cos(an) * lr, ly = CY + Math.sin(an) * lr;
      text(R, lab, lx, ly + 5, { ...rg.st, color: rg.col, alpha: on * grow * 0.95, align: 'center', glow: 0.15 });
    });
  });
  // carriers light up along the six spokes once aligned
  if (locked > 0) {
    for (let k = 0; k < 6; k++) {
      const an = -Math.PI / 2 + (k * TAU) / 6;
      const b = E.snap(seg(t, T.lock + k * 0.025, T.lock + 0.3 + k * 0.025));
      line(R, CX + Math.cos(an) * 205, CY + Math.sin(an) * 205, CX + Math.cos(an) * lerp(205, 405, b), CY + Math.sin(an) * lerp(205, 405, b), { color: C.teal, w: 1.6, alpha: on * locked * 0.9, glow: 1 });
    }
    const fl = seg(t, T.lock, T.lock + 0.6);
    if (fl < 1) circle(R, CX, CY, 405 + 60 * E.outCubic(fl), { color: C.teal, w: 1.5, alpha: on * (1 - fl) * 0.8, glow: 1 });
  }
  // center readout
  const cA = on * smooth(T.closure + 0.3, T.closure + 0.6, t);
  const n = locked > 0 ? 6 : Math.min(5, Math.floor(p * 6));
  text(R, `${n}/6`, CX, CY + 18, { f: 'Geist 300', size: 64, color: locked > 0 ? C.teal : C.ink, alpha: cA, align: 'center', glow: 0.35 * locked });
  text(R, locked > 0 ? 'REPRESENTED' : 'ALIGNING', CX, CY + 52, { f: 'MGW 500', size: 10.5, tracking: 3.5, color: locked > 0 ? C.teal : C.dim, alpha: cA, align: 'center' });
  // title
  const ti = on * smooth(T.closure + 0.15, T.closure + 0.5, t);
  text(R, 'Meaning closure', 150, 206, { f: 'Geist 600', size: 56, tracking: -1.6, color: C.ink, alpha: ti, glow: 0.15 });
  text(R, 'INTENT ↔ PLAN ↔ PROGRAM · EVERY OBLIGATION HAS A CARRIER', 152, 238, { f: 'MGW 500', size: 10.5, tracking: 3, color: C.ice, alpha: ti * 0.9 });
}
