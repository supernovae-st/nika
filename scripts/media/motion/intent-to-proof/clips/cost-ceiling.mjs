// cost-ceiling · "Know the cost. Before the first token."
// A cloud workflow priced before any call, then refused by the budget it
// was launched under. Every number on screen is read from the real
// binary's captures (scripts/media/capture/cost-ceiling.sh → media/raw/
// cost-ceiling-*): the COST rung of `nika check` on the file as first
// written (UNBOUNDED: no max_tokens) and after the one-line fix (a hard
// worst-case output ceiling), the price and catalog snapshot the check
// used (`nika check --json`), and `nika run --max-cost-usd` refusing to
// start (NIKA-1709, exit 2, no run journal, no key in the environment).
// The file is the real fixture pair and the fix is their real line diff.
// The receipt, the meter, the barrier and the stamp illustrate those
// numbers; the clip refuses to render if the captures stop telling this
// story.
import { C, E, seg, smooth, clamp, lerp, repoLines, readRepo, codeCard, terminal, frame, loopFade, cw, lineDiff, layoutRows, cameraPath, WIDE } from './kit.mjs';
import { text, rrect, line, poly, circle, measure, passAlpha, light, streak, bezierAt } from '../src/engine/render.mjs';

export const meta = { duration: 21.4, poster: 20.6 };

// ── the captures ────────────────────────────────────────────────────────
const before = repoLines('scripts/media/fixtures/cost-unbounded.nika');
const after = repoLines('scripts/media/fixtures/cost-ceiling.nika');
const check0 = repoLines('media/raw/cost-ceiling-check-unbounded.txt');
const check1 = repoLines('media/raw/cost-ceiling-check.txt');
const report = JSON.parse(readRepo('media/raw/cost-ceiling-check.json'));
const runOut = repoLines('media/raw/cost-ceiling-run.txt');
const run = JSON.parse(readRepo('media/raw/cost-ceiling-run.json'));
const explain = repoLines('media/raw/cost-ceiling-explain.txt');
const fail = why => { throw new Error(`the cost-ceiling capture: ${why}`); };

// the file: the fix is one added line and nothing else
const at = (arr, re) => arr.findIndex(l => re.test(l));
const W0 = at(before, /^nika:/) + 1;
const fixOps = lineDiff(before.slice(W0 - 1), after.slice(W0 - 1)).filter(o => o.op !== '=');
if (fixOps.length !== 1 || fixOps[0].op !== '+' || !/^\s+max_tokens: \d+$/.test(fixOps[0].text)) fail('the fix is no longer one added max_tokens line');
const MODEL_ROW = at(after, /^model: /) + 1;
const FIX_ROW = at(after, /^\s+max_tokens: /) + 1;

// the COST rung and the rows under it, as each check printed them
const RUNG = /^ [✔✖⚠○]\s+[A-Z]{3,}/u;
function costSection(lines) {
  const i = at(lines, /^ [✔⚠✖○]\s+COST\b/u);
  if (i < 0) fail('a check lost its COST rung');
  let j = i + 1;
  while (j < lines.length && !RUNG.test(lines[j]) && !/^ ↳ /u.test(lines[j])) j++;
  return lines.slice(i, j);
}
const HEADER = check1[0].match(/^nika check · (\S+\.nika)$/)?.[1];
if (!HEADER || check0[0] !== check1[0]) fail('the two checks are not of the same file');
const cost0 = costSection(check0), cost1 = costSection(check1);
const hint0 = check0.find(l => /^ ↳ HINT\s+\[cost\] declare `max_tokens`/u.test(l));
if (!/no total ceiling/.test(cost0[0]) || !cost0.some(l => /UNBOUNDED — no max_tokens declared/.test(l)) || !hint0) fail('the first check no longer names the uncapped task and its fix');
const rung1 = cost1[0].match(/^ ✔ COST\s+(\$[\d.]+) – (\$[\d.]+) worst-case output ceiling/u);
const row1 = cost1.find(l => /≤\d+ tk\s+\$[\d.]+$/u.test(l));
if (!rung1 || rung1[1] !== rung1[2] || !row1) fail('the fixed check no longer prints one hard worst-case ceiling');

// the numbers, from the report the check printed them from
const task = report.cost.tasks[0];
const price = report.pricing.models.find(m => m.model === task.model);
if (report.cost.tasks.length !== 1 || report.cost.has_unbounded || !price) fail('the report no longer prices one bounded task');
if (Math.abs((task.max_tokens * price.output_per_million) / 1e6 - task.usd) > 1e-12) fail('the ceiling is not max_tokens × the output price');
if (`$${task.usd.toFixed(4)}` !== rung1[1] || !row1.includes(`≤${task.max_tokens} tk`) || !row1.endsWith(rung1[1])) fail('the rung and the report disagree');
const MODEL = task.model, MAXTOK = String(task.max_tokens), CEIL = report.cost.bounded_total_usd, CEIL_TXT = rung1[1];
const PRICE_TXT = `$${Number.isInteger(price.output_per_million) ? price.output_per_million : price.output_per_million.toFixed(2)}`;
const AS_OF = report.pricing.snapshot.as_of, SOURCE = new URL(report.pricing.snapshot.source).hostname;
const VERSION = report.engine_version;
if (!after[MODEL_ROW - 1].endsWith(MODEL) || !after[FIX_ROW - 1].endsWith(MAXTOK)) fail('the file and the report name different seats');

// the run: refused on its budget before anything started
const refusal = runOut.join(' ').match(/^NIKA-1709 · refusing to start: the workflow's unavoidable cost floor \$([\d.]+) exceeds --max-cost-usd \$([\d.]+)/);
const BUDGET_ARG = run.command.match(/^nika run (\S+) --max-cost-usd (\S+)$/);
if (!refusal || !BUDGET_ARG || BUDGET_ARG[1] !== HEADER) fail('the run no longer refuses on its budget');
const FLOOR = +refusal[1], BUDGET = +refusal[2];
if (run.exit !== 2 || run.journal_written !== false) fail('the refused run exited otherwise or left a journal');
if (run.env.some(n => /KEY|TOKEN|SECRET|CRED|AUTH|PASS/i.test(n))) fail('a credential variable reached the refused run');
if (+BUDGET_ARG[2] !== BUDGET || !(FLOOR > BUDGET) || Math.abs(FLOOR - report.cost.min_path_total_usd) > 5e-7) fail('the refused floor is not the checked one, or does not exceed the budget');
const BUDGET_TXT = `$${BUDGET_ARG[2]}`;
const EXIT = run.exit;
// the same numbers in beginner words, and the flight recorder, read after the refusal
const EXPLAIN_HEAD = explain[at(explain, /^cost before a token is spent$/)];
const EXPLAIN_COST = explain[at(explain, /^cost before a token is spent$/) + 1]?.trim();
if (!EXPLAIN_COST || !EXPLAIN_COST.includes(`≤ ${CEIL_TXT} worst case`)) fail('nika explain no longer narrates the ceiling');
if (!explain.some(l => /no runs recorded here yet/.test(l))) fail('the flight recorder recorded a run');
// the terminal's title says no key was set: every check says so too
if (![check0, check1].every(c => c.some(l => /_API_KEY unset in process env/.test(l)))) fail('a check ran with a provider key set');

// ── what the terminal shows ─────────────────────────────────────────────
const CMD_CHECK = `nika check ${HEADER}`;
const CMD_RUN = run.command;
const page1 = [...cost0, hint0];
const page2 = cost1;
const page3 = runOut;

// ── layout ──────────────────────────────────────────────────────────────
const CODE = { f: 'MM 400', size: 22 };
const CODE_LH = 34;
const TERM = { f: 'MM 400', size: 22 };
const LH = 30;
const FILE = { x: 64, y: 244, w: 812, h: 500 };
const TERMBOX = { x: 64, y: 768, w: 1792, h: 240 };
const codeY = n => FILE.y + 44 + 32 + (n - W0) * CODE_LH; // baseline of file line n
const codeX = col => FILE.x + 58 + 12 + col * cw(CODE);
const termY = row => TERMBOX.y + 44 + 30 + row * LH; // row 0: the typed command
const termX = col => TERMBOX.x + 24 + col * cw(TERM);

// the receipt and the meter
const GX = 944;
const ROW_A = codeY(MODEL_ROW), ROW_B = ROW_A + 68, RULE_Y = ROW_B + 28, ROW_C = RULE_Y + 88;
const LABEL_X = GX + 160;
const VAL = { f: 'Geist 600', size: 44, tracking: -1 };
const BIG = { f: 'Geist 600', size: 88, tracking: -2.5 };
const LABEL = { f: 'Geist 500', size: 24, tracking: -0.2 };
const BUDGET_ST = { f: 'Geist 600', size: 30, tracking: -0.5 };
const M = { x0: GX, x1: 1824, y: ROW_C + 62, h: 20 };
const SCALE = Math.ceil((CEIL * 1.25) / 0.02) * 0.02;
const mx = usd => M.x0 + ((M.x1 - M.x0) * usd) / SCALE;
// the budget's label, centred under its line: "$" then the flag's number
const BUD_X0 = mx(BUDGET) - measure(BUDGET_TXT, BUDGET_ST) / 2;
const BUD_NUM_X = BUD_X0 + measure('$', BUDGET_ST) + BUDGET_ST.tracking;

// ── timing ──────────────────────────────────────────────────────────────
const T = {
  card: 0.35, term: 0.55, meter: 0.7,
  model: 1.2, price: 1.7,
  cmd1: 3.1, out1: 4.2, fly1: 4.5, bar1: 4.9,
  fix: 8.3, fixEnd: 9.1, fly4096: 8.95,
  clear2: 9.4, cmd2: 9.55, out2: 10.65, fly2: 10.9, snap: 11.35,
  clear3: 14.3, cmd3: 14.45, budget: 15.8, drop: 16.25, out3: 16.7, block: 16.85, stamp: 17.15, calm: 17.75,
};
const CMD3_DUR = 1.25;
const FLY = 0.6;

// where a token sits in the terminal once a page is printed (for the
// flights): the first row matching `rowRe` that carries it
function termToken(out, page0Rows, needle, rowRe, wrap) {
  const rows = layoutRows(out, { box: TERMBOX, st: TERM, wrap });
  for (let i = 0; i < rows.length; i++) {
    const s = rows[i].spans.map(sp => sp.s).join('');
    const c = s.indexOf(needle);
    if (c >= 0 && rowRe.test(s)) return { x: termX(c), y: termY(page0Rows + i) };
  }
  fail(`the terminal no longer shows ${needle}`);
}
const WRAP1 = /COST/u;
const FROM_UNBOUNDED = termToken(page1, 1, 'UNBOUNDED', /UNBOUNDED — no max_tokens declared/, WRAP1);
const FROM_CEIL = termToken(page2, 1, CEIL_TXT, /≤\d+ tk/u, false);
const FROM_BUDGET = { x: termX(2 + CMD_RUN.lastIndexOf(BUDGET_ARG[2])), y: termY(0) };
const FROM_MAXTOK = { x: codeX(after[FIX_ROW - 1].indexOf(MAXTOK)), y: codeY(FIX_ROW) };

// One move: the clip opens close on the model line and its price, then
// pulls back to the whole stage as the check starts, and stays there, so
// the numbers can travel between the file, the terminal and the receipt
// in one frame (every frame of a move repaints the whole GIF frame). The
// close shot leaves the headline's band empty, so the title stays up.
const SHOTS = [
  { at: 0, cam: { x: 723, y: 385, s: 1.42 } },
  { at: T.cmd1 - 0.05, cam: WIDE, move: 0.75 },
];
export const camera = t => cameraPath(t, SHOTS);

export function env() {
  return { bgGlow: 1, gridAlpha: 0.22, gridX: 0, gridY: 0, bgY: 560 };
}

export function chrome(R, t) {
  const a = loopFade(t, meta.duration);
  frame(R, t, { kicker: 'cost honesty · priced before the run · the budget gate', plate: `prices are catalog estimates · captured from nika ${VERSION} · no model was called · the receipt, meter and stamp are illustrations`, alpha: a, scrim: smooth(1, 1.12, camera(t).s) });
  kineticHeadline({ ...R, fade: a }, t, 0.2, 'Know the cost.', 'Before the first token.', 'NIKA CHECK PRICES A WORKFLOW OFFLINE · A BUDGET YOU SET REFUSES THE RUN BEFORE IT STARTS');
}

// The clips' headline (the kit's position, face, size and accent), set
// word by word: each rises into place and comes into focus in turn, the
// accent after the claim it answers.
function kineticHeadline(R, t, t0, head, accent, sub) {
  const x = 64, y = 172, st = { f: 'Geist 600', size: 58, tracking: -58 * 0.028 };
  const words = `${head} ${accent}`.split(' ');
  const nHead = head.split(' ').length;
  let prefix = '';
  words.forEach((w, i) => {
    const t1 = t0 + i * 0.075 + (i >= nHead ? 0.18 : 0);
    const k = E.snap(seg(t, t1, t1 + 0.55));
    if (k > 0) {
      const accentWord = i >= nHead;
      text(R, w, x + (prefix ? measure(`${prefix} `, st) + st.tracking : 0), y + 16 * (1 - k), { ...st, color: accentWord ? C.teal : C.ink, alpha: smooth(t1, t1 + 0.3, t), glow: accentWord ? 0.4 : 0.18, blur: 5 * (1 - k) });
    }
    prefix = prefix ? `${prefix} ${w}` : w;
  });
  const sk = smooth(t0 + 0.8, t0 + 1.1, t);
  if (sk > 0) text(R, sub, x + 2, y + 32, { f: 'MGW 500', size: 10.5, tracking: 3, color: C.teal, alpha: 0.9 * sk });
}

// ── small helpers ───────────────────────────────────────────────────────
// A token lifting off its source and landing in its slot along a curve
// (c0, c1: bezier controls, so it travels over empty space, not over
// text): eased position, size interpolated in log space, softened by its
// own speed.
function fly(R, t, t0, str, from, to, st0, st1, color, c0 = from, c1 = to) {
  const k = seg(t, t0, t0 + FLY);
  if (k <= 0 || k >= 1) return;
  const p = E.inOutCubic(k);
  const speed = (E.inOutCubic(clamp(k + 0.03)) - E.inOutCubic(clamp(k - 0.03))) / 0.06;
  const size = Math.exp(lerp(Math.log(st0.size), Math.log(st1.size), p));
  const [x, y] = bezierAt([from.x, from.y], [c0.x, c0.y], [c1.x, c1.y], [to.x, to.y], p);
  text(R, str, x, y, { f: st1.f, size, tracking: lerp(st0.tracking ?? 0, st1.tracking ?? 0, p), color, glow: 0.25 + 0.55 * Math.sin(Math.PI * p), blur: 2.2 * speed });
}

// digits roll into place, one column at a time (the film's odometer)
function roll(R, t, t0, str, x, y, st, color, glow = 0.3) {
  const dW = Math.max(...'0123456789'.split('').map(d => measure(d, st)));
  const ctx = R.ctx;
  let xx = x;
  [...str].forEach((ch, i) => {
    const isDigit = /\d/.test(ch);
    const w = isDigit ? dW : measure(ch, st);
    const p = E.snap(seg(t, t0 + i * 0.04, t0 + 0.45 + i * 0.05));
    if (!isDigit || p >= 1) text(R, ch, xx + (isDigit ? (w - measure(ch, st)) / 2 : 0), y, { ...st, color, alpha: smooth(t0, t0 + 0.15, t), glow });
    else {
      const pos = +ch + 10 * (1 + (i % 3)) * (1 - p);
      ctx.save();
      ctx.beginPath();
      ctx.rect(xx - 4, y - st.size * 0.8, w + 8, st.size * 0.8 + st.size * 0.06);
      ctx.clip();
      const base = Math.floor(pos);
      for (let d = base - 1; d <= base + 1; d++) {
        // a neighbour rolled out of the window is not drawn at all
        const near = clamp(1 - Math.abs(d - pos));
        if (near <= 0.02) continue;
        const dig = ((d % 10) + 10) % 10;
        const off = (d - pos) * st.size * 0.92;
        text(R, String(dig), xx + (w - measure(String(dig), st)) / 2, y + off, { ...st, color, alpha: smooth(t0, t0 + 0.1, t) * near, glow, blur: Math.min(6, (1 - p) * 14) });
      }
      ctx.restore();
    }
    xx += w + (st.tracking ?? 0);
  });
}

// ── the receipt: price × tokens = ceiling ───────────────────────────────
function receipt(R, t, a) {
  // the zone's heading: nika explain's own words for it
  const hk = smooth(T.meter, T.meter + 0.4, t) * a;
  if (hk > 0) {
    const y = FILE.y + 28;
    line(R, GX, y - 4, GX + 12, y - 4, { color: C.ice, w: 1, alpha: hk * 0.8 });
    text(R, `${EXPLAIN_HEAD.toUpperCase()} · NIKA EXPLAIN`, GX + 20, y, { f: 'MGW 500', size: 11, tracking: 3, color: C.ice, alpha: hk * 0.75 });
  }
  // the price, from the catalog, attached to the model line
  const lead = E.inOutCubic(seg(t, T.model, T.model + 0.55));
  if (lead > 0) {
    const x0 = codeX(after[MODEL_ROW - 1].length) + 14, y0 = ROW_A - 8;
    const x1 = GX - 18;
    poly(R, [[x0, y0], [lerp(x0, x1, lead), y0]], { color: C.ice, w: 1.4, alpha: a * 0.7, dash: [2, 7], glow: 0.3 });
    circle(R, x0, y0, 3.2, { fill: C.ice, alpha: a * lead, glow: 0.8 });
    // the price travels the leader from the catalog to the receipt
    if (lead < 1) circle(R, lerp(x0, x1, lead), y0, 4.5, { fill: C.ice, alpha: a, glow: 1 });
    else circle(R, x1, y0, 3.2, { fill: C.ice, alpha: a, glow: 0.8 });
  }
  const pk = smooth(T.price, T.price + 0.3, t) * a;
  if (pk > 0) {
    roll({ ...R, fade: a }, t, T.price, PRICE_TXT, GX, ROW_A, VAL, C.ink);
    text(R, 'per 1M output tokens', LABEL_X, ROW_A, { ...LABEL, color: C.mist, alpha: pk });
    text(R, `${MODEL} · catalog ${SOURCE} · ${AS_OF}`, LABEL_X, ROW_A + 26, { f: 'MM 400', size: 14, color: C.dim, alpha: pk * 0.9 });
  }
  // × output tokens: unknown until the file declares them
  const bk = smooth(T.out1 + 0.2, T.out1 + 0.5, t) * a;
  if (bk > 0) {
    const fixed = t >= T.fly4096 + FLY;
    text(R, '×', GX, ROW_B, { ...VAL, color: fixed ? C.ink : C.amber, alpha: bk });
    const vx = GX + measure('× ', VAL);
    if (!fixed) {
      const q = 1 - smooth(T.fly4096, T.fly4096 + 0.25, t);
      text(R, '?', vx, ROW_B, { ...VAL, color: C.amber, alpha: bk * q, glow: 0.4 });
      text(R, 'no max_tokens declared', LABEL_X, ROW_B, { ...LABEL, color: C.amber, alpha: bk * q });
    } else {
      text(R, MAXTOK, vx, ROW_B, { ...VAL, color: C.ink, alpha: a, glow: 0.2 * (1 - smooth(T.fly4096 + FLY, T.fly4096 + FLY + 0.8, t)) });
      text(R, 'output tokens, at most', LABEL_X, ROW_B, { ...LABEL, color: C.mist, alpha: a * smooth(T.fly4096 + FLY, T.fly4096 + FLY + 0.3, t) });
    }
    line(R, GX, RULE_Y, GX + (M.x1 - GX) * E.snap(seg(t, T.out1 + 0.3, T.out1 + 0.9)), RULE_Y, { color: C.faint, w: 1, alpha: bk });
  }
  // = the total: UNBOUNDED until the check can bound it
  const ck = smooth(T.fly1 + FLY - 0.05, T.fly1 + FLY + 0.1, t) * a;
  if (ck > 0) {
    // it leaves as the checked number lifts off, so the slot is empty
    // before the number lands (no crossfade of two values in one place)
    const swap = seg(t, T.fly2, T.fly2 + 0.3);
    if (swap < 1) {
      const u = E.inCubic(swap);
      text(R, 'UNBOUNDED', GX, ROW_C - 30 * u, { ...BIG, color: C.amber, alpha: ck * (1 - u), glow: 0.35 });
      text(R, 'no total ceiling', GX + measure('UNBOUNDED', BIG) + 28, ROW_C, { ...LABEL, color: C.amber, alpha: ck * (1 - u) });
    }
  }
  if (t >= T.fly2 + FLY) {
    const lead = '≤ ';
    text(R, lead, GX, ROW_C, { ...BIG, color: C.teal, alpha: a, glow: 0.35 });
    const w = measure(lead, BIG) + measure(CEIL_TXT, BIG);
    text(R, CEIL_TXT, GX + measure(lead, BIG), ROW_C, { ...BIG, color: C.teal, alpha: a, glow: 0.35 });
    text(R, 'worst-case output ceiling', GX + w + 28, ROW_C, { ...LABEL, color: C.teal, alpha: a * smooth(T.fly2 + FLY, T.fly2 + FLY + 0.3, t) });
  }
}

// ── the meter: the envelope against the budget ──────────────────────────
function barEnd(t) {
  if (t < T.bar1) return M.x0;
  if (t < T.snap) return lerp(M.x0, 2060, E.inCubic(seg(t, T.bar1, T.bar1 + 0.75)));
  // the cap: the runaway snaps back and settles on the ceiling
  const k = seg(t, T.snap, T.snap + 0.55);
  return lerp(2060, mx(CEIL), E.outBackSoft(k));
}

// a filled bar segment in one colour (both passes; glow as given)
function fillBar(R, x0, x1, color, alpha, glow, radii) {
  const ga = passAlpha(R, alpha, glow);
  if (!ga || x1 <= x0) return;
  const ctx = R.ctx;
  ctx.save();
  ctx.globalAlpha = ga;
  ctx.fillStyle = color;
  ctx.beginPath();
  ctx.roundRect(x0, M.y - M.h / 2, x1 - x0, M.h, radii);
  ctx.fill();
  ctx.restore();
}

function meter(R, t, a) {
  const k = E.snap(seg(t, T.meter, T.meter + 0.7));
  if (k <= 0) return;
  const y = M.y, h = M.h;
  const reach = lerp(M.x0, M.x1, k);
  // the track: the scale the envelope is read against, a tick a cent
  rrect(R, M.x0 - 4, y - h / 2 - 4, reach - M.x0 + 8, h + 8, (h + 8) / 2, { color: C.faint, w: 1, alpha: a, fill: '#07101f', fillAlpha: 0.9 });
  for (let c = 0; c <= Math.round(SCALE * 100); c++) {
    const x = mx(c / 100);
    if (x > reach + 1) break;
    line(R, x, y + h / 2 + 8, x, y + h / 2 + (c % 2 ? 12 : 16), { color: C.dim, w: 1, alpha: a * 0.8 });
  }
  const lk = a * smooth(T.meter + 0.4, T.meter + 0.8, t);
  text(R, '$0', M.x0, y + h / 2 + 40, { f: 'MM 400', size: 16, color: C.dim, alpha: lk });
  text(R, `$${SCALE.toFixed(2)}`, M.x1, y + h / 2 + 40, { f: 'MM 400', size: 16, color: C.dim, alpha: lk, align: 'right' });

  const end = barEnd(t);
  const capped = t >= T.snap;
  const warm = 1 - smooth(T.snap, T.snap + 0.3, t);
  const bx = mx(BUDGET), cx = mx(CEIL);
  const block = E.snap(seg(t, T.block, T.block + 0.3));
  if (end > M.x0 + 1) {
    // the runaway: amber, fading off the end of the scale
    if (!capped || warm > 0) {
      const ga = passAlpha(R, a, 0.55);
      if (ga) {
        const ctx = R.ctx;
        const g = ctx.createLinearGradient(M.x0, 0, 1920, 0);
        g.addColorStop(0, `rgba(255,181,71,${0.92 * warm})`);
        g.addColorStop((M.x1 - M.x0) / (1920 - M.x0), `rgba(255,181,71,${0.6 * warm})`);
        g.addColorStop(1, 'rgba(255,181,71,0)');
        ctx.save();
        ctx.globalAlpha = ga;
        ctx.fillStyle = g;
        ctx.beginPath();
        ctx.roundRect(M.x0, y - h / 2, Math.max(0, end - M.x0), h, h / 2);
        ctx.fill();
        ctx.restore();
      }
    }
    // bounded: teal up to the ceiling; once the budget judges it, the part
    // past the budget is the part it cannot cover
    if (capped) {
      const tealEnd = block > 0 ? bx : end;
      fillBar(R, M.x0, tealEnd, C.teal, a * (1 - warm), 0.5, [h / 2, 0, 0, h / 2]);
      if (block > 0) {
        fillBar(R, bx, end, C.teal, a * (1 - block), 0.5, [0, 3, 3, 0]);
        fillBar(R, bx, end, C.red, a * block, 0.7, [0, 3, 3, 0]);
        // no-go hatching over the part the budget cannot cover
        if (!R.glowPass) {
          const ctx = R.ctx;
          ctx.save();
          ctx.beginPath();
          ctx.rect(bx, y - h / 2, end - bx, h);
          ctx.clip();
          for (let sx = bx - h; sx < end + h; sx += 9) line(R, sx, y + h / 2, sx + h, y - h / 2, { color: '#3a0a14', w: 3, alpha: a * block * 0.55, cap: 'butt' });
          ctx.restore();
        }
      }
    }
    // no end: the tip streaks off the scale, then chevrons keep streaming
    // while nothing bounds it
    const grow = seg(t, T.bar1, T.bar1 + 0.75);
    if (grow > 0 && grow < 1) streak(R, end, y, 70 + 170 * grow, a * 0.85 * Math.sin(Math.PI * grow), C.amber, 1);
    if (!capped) {
      const on = smooth(T.bar1 + 0.6, T.bar1 + 0.9, t);
      for (let i = 0; i < 3; i++) {
        const chx = M.x1 + 30 + i * 22 + ((t * 40) % 22);
        poly(R, [[chx - 6, y - 8], [chx + 2, y], [chx - 6, y + 8]], { color: C.amber, w: 2.2, alpha: a * on * (1 - i * 0.28), glow: 0.6 });
      }
    }
    // the cap: a hard stop at the ceiling
    const ck = capped ? E.snap(seg(t, T.snap + 0.3, T.snap + 0.55)) : 0;
    if (ck > 0) {
      const col = block > 0.5 ? C.red : C.teal;
      line(R, cx, y - 24, cx, y + 24, { color: col, w: 5, alpha: a * ck, glow: 0.9, cap: 'butt' });
      line(R, cx - 7, y - 24, cx + 7, y - 24, { color: col, w: 2, alpha: a * ck, glow: 0.6 });
      line(R, cx - 7, y + 24, cx + 7, y + 24, { color: col, w: 2, alpha: a * ck, glow: 0.6 });
      const ring = seg(t, T.snap + 0.45, T.snap + 1.05);
      if (ring > 0 && ring < 1) {
        circle(R, cx, y, 18 + 60 * E.outCubic(ring), { color: C.teal, w: 1.5, alpha: a * (1 - ring) * 0.8, glow: 1 });
        light(R, cx, y, 120, C.teal, a * 0.45 * (1 - ring), 1);
      }
    }
  }

  // the budget: the operator's line, dropped where the flag puts it
  const bk = E.snap(seg(t, T.drop, T.drop + 0.35));
  if (bk > 0) {
    const y0 = lerp(y - 170, y - 34 - 16 * block, bk), y1 = y + 30 + 10 * block;
    line(R, bx, y0, bx, y1, { color: C.human, w: 2.4 + 5.6 * block, alpha: a * bk, glow: 0.5 + 0.5 * block, cap: 'butt' });
    const ring = seg(t, T.block, T.block + 0.7);
    if (ring > 0 && ring < 1) {
      circle(R, bx, y, 22 + 110 * E.outCubic(ring), { color: C.human, w: 2, alpha: a * (1 - ring), glow: 1 });
      light(R, bx, y, 160, C.human, a * 0.4 * (1 - ring), 1);
    }
  }
  // the label under it: the number the flag carried (drawn where it landed)
  const lab = t >= T.budget + FLY ? a : 0;
  if (lab > 0) {
    text(R, '$', BUD_X0, y + 76, { ...BUDGET_ST, color: C.human, alpha: lab, glow: 0.3 });
    text(R, BUDGET_ARG[2], BUD_NUM_X, y + 76, { ...BUDGET_ST, color: C.human, alpha: lab, glow: 0.3 });
    text(R, 'your budget', bx, y + 104, { f: 'Geist 500', size: 22, color: C.human, alpha: lab * 0.9 * smooth(T.budget + FLY, T.budget + FLY + 0.3, t), align: 'center' });
  }
}

// ── the verdict ─────────────────────────────────────────────────────────
function stamp(R, t, a) {
  const k = seg(t, T.stamp, T.stamp + 0.32);
  if (k <= 0) return;
  const s = lerp(1.7, 1, E.outCubic(k));
  const al = a * smooth(0, 0.35, k);
  const cx = FILE.x + FILE.w / 2 + 40, cy = codeY(FIX_ROW + 2) + 8;
  const ctx = R.ctx;
  ctx.save();
  ctx.translate(cx, cy);
  ctx.rotate(-0.07);
  ctx.scale(s, s);
  const main = { f: 'Geist 700', size: 50, tracking: 1.5 };
  const w = measure('REFUSED TO START', main) + 76, h = 116;
  rrect(R, -w / 2, -h / 2, w, h, 12, { color: C.red, w: 3.5, alpha: al, fill: '#1a0710', fillAlpha: 0.88, glow: 0.5 });
  rrect(R, -w / 2 + 7, -h / 2 + 7, w - 14, h - 14, 8, { color: C.red, w: 1.2, alpha: al * 0.7 });
  text(R, 'REFUSED TO START', 0, 6, { ...main, color: C.red, alpha: al, align: 'center', glow: 0.5 });
  text(R, `EXIT ${EXIT} · NIKA-1709 · NO RUN RECORDED`, 0, 38, { f: 'MGW 500', size: 13, tracking: 3.5, color: C.red, alpha: al * 0.95, align: 'center' });
  ctx.restore();
  // the impact
  const ring = seg(t, T.stamp + 0.28, T.stamp + 0.9);
  if (ring > 0 && ring < 1) {
    circle(R, cx, cy, 60 + 260 * E.outCubic(ring), { color: C.red, w: 1.6, alpha: a * (1 - ring) * 0.7, glow: 1 });
    light(R, cx, cy, 300, C.red, a * 0.28 * (1 - ring), 1);
  }
}

function calm(R, t, a) {
  const k = smooth(T.calm, T.calm + 0.35, t) * a;
  if (k <= 0) return;
  const y = M.y + 136;
  const st = { f: 'Geist 600', size: 28, tracking: -0.3 };
  text(R, 'Nothing ran.', GX, y, { ...st, color: C.teal, alpha: k, glow: 0.3 });
  text(R, '0 tokens · $0 spent · no model was called', GX + measure('Nothing ran. ', st), y, { f: 'Geist 500', size: 24, color: C.mist, alpha: k });
}

// ── the frame ───────────────────────────────────────────────────────────
export function draw(R, t) {
  const a = loopFade(t, meta.duration);
  const fixed = t >= T.fix;
  const stamped = smooth(T.stamp, T.stamp + 0.3, t);
  codeCard({ ...R, fade: a * (1 - 0.35 * stamped) }, t, FILE, {
    title: HEADER, k: E.snap(seg(t, T.card, T.card + 0.5)),
    before, after, win: { a: [W0, before.length], b: [W0, after.length] },
    reveal: { t0: T.card + 0.25 - W0 * 0.03, every: 0.03 },
    morph: { t0: T.fix, t1: T.fixEnd },
    st: CODE, lh: CODE_LH,
    badge: fixed ? { label: 'THE FIX · ONE LINE', color: C.teal, alpha: smooth(T.fix, T.fix + 0.3, t) } : null,
    highlights: [
      { line: MODEL_ROW, t0: T.model, c: C.ice, icon: 'none' },
      { line: FIX_ROW, t0: T.fixEnd - 0.2, c: C.teal, icon: 'none' },
    ],
    marks: [
      { line: MODEL_ROW, re: new RegExp(MODEL.replace(/[.*+?^${}()|[\]\\/]/g, '\\$&')), c: C.ice, t0: T.model },
      { line: FIX_ROW, re: /max_tokens: \d+/, c: C.teal, t0: T.fixEnd - 0.3, version: 'after' },
    ],
  });
  receipt(R, t, a);
  meter(R, t, a);

  const refused = t >= T.out3;
  terminal(R, t, TERMBOX, [
    { t: T.cmd1, cmd: CMD_CHECK, dur: 0.9 },
    { t: T.out1, out: page1, every: 0.012, wrap: WRAP1, marks: [{ re: /UNBOUNDED — no max_tokens declared/, c: C.amber }, { re: /no total ceiling/, c: C.amber }, { re: /declare `max_tokens` on `update`/, c: C.teal }] },
    { t: T.clear2, clear: true },
    { t: T.cmd2, cmd: CMD_CHECK, dur: 0.9 },
    { t: T.out2, out: page2, every: 0.012, marks: [{ re: /\$[\d.]+ – \$[\d.]+ worst-case output ceiling/, c: C.teal, glow: 0.5 }, { re: /≤\d+ tk\s+\$[\d.]+/u, c: C.teal }] },
    { t: T.clear3, clear: true },
    { t: T.cmd3, cmd: CMD_RUN, dur: CMD3_DUR },
    { t: T.out3, out: page3, wrap: true, indent: 0, marks: [{ re: /refusing to start/, c: C.red }, { re: /unavoidable cost floor \$[\d.]+/, c: C.ink }, { re: /exceeds --max-cost-usd \$[\d.]+/, c: C.human }] },
  ], {
    title: 'terminal · no api key set', alpha: a, k: E.snap(seg(t, T.term, T.term + 0.5)), st: TERM, lh: LH,
    badge: refused ? { label: `EXIT ${EXIT} · REFUSED`, color: C.red, alpha: smooth(T.out3, T.out3 + 0.3, t) }
      : t >= T.out2 ? { label: 'COST · HARD CEILING', color: C.teal, alpha: smooth(T.out2, T.out2 + 0.3, t) * (1 - smooth(T.clear3 - 0.3, T.clear3, t)) }
        : t >= T.out1 ? { label: 'COST · UNBOUNDED', color: C.amber, alpha: smooth(T.out1, T.out1 + 0.3, t) * (1 - smooth(T.clear2 - 0.3, T.clear2, t)) } : null,
  });

  // the numbers lift out of what printed them and land in the receipt:
  // up out of the terminal, then across into the slot
  const TO_UNB = { x: GX, y: ROW_C }, TO_CEIL = { x: GX + measure('≤ ', BIG), y: ROW_C };
  fly({ ...R, fade: a }, t, T.fly1, 'UNBOUNDED', FROM_UNBOUNDED, TO_UNB, TERM, BIG, C.amber, { x: FROM_UNBOUNDED.x + 60, y: FROM_UNBOUNDED.y - 190 }, { x: TO_UNB.x - 70, y: TO_UNB.y + 110 });
  fly({ ...R, fade: a }, t, T.fly2, CEIL_TXT, FROM_CEIL, TO_CEIL, TERM, BIG, C.teal, { x: FROM_CEIL.x + 40, y: FROM_CEIL.y - 190 }, { x: TO_CEIL.x - 40, y: TO_CEIL.y + 120 });
  // along the new line's empty right side, then up into the receipt
  const TO_TOK = { x: GX + measure('× ', VAL), y: ROW_B };
  fly({ ...R, fade: a }, t, T.fly4096, MAXTOK, FROM_MAXTOK, TO_TOK, CODE, VAL, C.teal, { x: FILE.x + FILE.w - 40, y: FROM_MAXTOK.y }, { x: TO_TOK.x - 60, y: TO_TOK.y + 90 });
  // off the typed flag, onto the meter
  const TO_BUD = { x: BUD_NUM_X, y: M.y + 76 };
  fly({ ...R, fade: a }, t, T.budget, BUDGET_ARG[2], FROM_BUDGET, TO_BUD, TERM, BUDGET_ST, C.human, { x: FROM_BUDGET.x + 260, y: FROM_BUDGET.y - 30 }, { x: TO_BUD.x - 120, y: TO_BUD.y + 40 });

  stamp(R, t, a);
  calm(R, t, a);
}
