// approval-gate · "Nothing ships without a yes."
// gated-ship.nika is the example of the docs' Resume page (a fixture here):
// build, ask a human through `nika:prompt`, ship only on yes. Every terminal
// line is read from media/raw/approval-gate-*, captured offline from the
// real binary by scripts/media/capture/approval-gate.sh:
//  · at a terminal (a pseudo-terminal) the gate asks [y/N], and y ships;
//  · with nobody to ask (stdin is not a terminal, as in CI) the run pauses,
//    exits 4 and prints the exact line that resumes it;
//  · that line, run as printed, reuses build, carries the answer and ships.
// The track's three steps are the audit's PLAN, the door's question is the
// fixture's, and the clip refuses to render if the captures stop telling
// this story. Illustration: the door and its light, the pulse on the track,
// the tethers from the gate to its lines, the keypress and the CI frame.
import { C, E, seg, smooth, clamp, lerp, repoLines, readRepo, NIKA_VERSION, frame, headline, loopFade, mono, pill, cw, cameraPath, frameBox, WIDE } from './kit.mjs';
import { text, rrect, rect, line, circle, poly, arc, check, light, measure } from '../src/engine/render.mjs';
import { rgba } from '../src/engine/core.mjs';

export const meta = { duration: 24.5, poster: 23.4 };

// ── the captures, and the story they must still tell ────────────────────
const FILE = 'gated-ship.nika';
const RUN = `nika run ${FILE}`;
const fixture = repoLines(`scripts/media/fixtures/${FILE}`);
const audit = repoLines('media/raw/approval-gate-check.txt');
const tty = repoLines('media/raw/approval-gate-tty-yes.txt');
const refused = ['no', 'enter'].map(k => repoLines(`media/raw/approval-gate-tty-${k}.txt`));
const ci = repoLines('media/raw/approval-gate-ci.txt');
const resumed = repoLines('media/raw/approval-gate-resume.txt');
const RESUME_CMD = readRepo('media/raw/approval-gate-resume-cmd.txt').trim();
const EXITS = JSON.parse(readRepo('media/raw/approval-gate-exits.json'));
const PAUSED = JSON.parse(readRepo('media/raw/approval-gate-paused-event.json'));
const outputs = repoLines('media/raw/approval-gate-resume-outputs.txt');
const record = repoLines('media/raw/approval-gate-trace-show.txt');

const story = (ok, what) => {
  if (!ok) throw new Error(`the approval-gate capture: ${what}`);
};
const find = (lines, re, what) => {
  const l = lines.find(x => re.test(x));
  story(l !== undefined, what);
  return l;
};
const esc = s => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

// the question the gate asks, as the file words it
const MESSAGE = fixture.map(l => l.match(/^\s+message:\s*"([^"]+)"\s*$/)).find(Boolean)?.[1];
story(MESSAGE, 'the fixture lost its gate message');
// the three steps, as the audit plans them
const PLAN = audit.map(l => l.match(/^\s+wave \d+ (\S+) \((.+)\)$/)).filter(Boolean).map(m => ({ id: m[1], verb: m[2] }));
story(PLAN.length === 3 && PLAN[1].verb === 'invoke · nika:prompt', 'the plan is no longer a step, a nika:prompt gate, a step');
story(audit.some(l => /run ready ✔/u.test(l)), `${FILE} is not run ready`);
const [BUILD, GATE, SHIP] = PLAN;
const taskRow = (lines, glyph, id, what, verbGlyph = '') => find(lines, new RegExp(`^\\s+${glyph}\\s+${verbGlyph}${esc(id)}\\s`, 'u'), what);

// at a terminal: the live render, settled into the screen a person reads
const TTY_BUILD = taskRow(tty, '✔', BUILD.id, 'the terminal run lost its build row', '▷ ');
const TTY_ASK = find(tty, new RegExp(`^\\s+◇ ${esc(GATE.id)} · ${esc(MESSAGE)}\\s+\\[y/N\\] y$`, 'u'), 'the terminal no longer asks the gate\'s question');
const TTY_GATE = taskRow(tty, '✔', GATE.id, 'the terminal answer is no longer bound', '◆ ');
const TTY_SHIP = taskRow(tty, '✔', SHIP.id, 'answered y at the terminal, ship no longer runs', '▷ ');
story(/→ true · \d+B$/.test(TTY_GATE), 'the terminal y no longer binds true');
for (const r of refused) taskRow(r, '↷', SHIP.id, 'answered N or Enter, ship is no longer held back', '· ');
// nobody to ask: exit 4, the pause, the trace it wrote, the printed line
story(EXITS.ci === 4 && EXITS.resume === 0 && EXITS.tty_yes === 0, 'the exits are no longer 4 when paused and 0 when resumed');
const CI_BUILD = taskRow(ci, '✔', BUILD.id, 'the unattended run lost its build row');
const CI_PAUSED = find(ci, new RegExp(`^\\s+◇ paused · awaiting an answer for \`${esc(GATE.id)}\`$`, 'u'), 'the unattended run no longer pauses at the gate');
const CI_TRACE = find(ci, /^\s+trace: \.nika\/traces\/\S+\.ndjson · \d+ events/, 'the unattended run lost its trace line');
const CI_RESUME = find(ci, /^\s+resume: /, 'the unattended run printed no resume line');
story(CI_RESUME.endsWith(`resume: ${RESUME_CMD} · or false`) && RESUME_CMD.startsWith(`${RUN} --resume `) && RESUME_CMD.endsWith(`--answer ${GATE.id}=true`), 'the command run is not the printed resume line');
const TRACE = RESUME_CMD.match(/--resume (\S+)/)[1];
story(CI_TRACE.includes(`trace: ${TRACE} ·`), 'the printed line resumes another trace');
const field = k => PAUSED.fields.find(f => f.key === k)?.value;
story(PAUSED.kind === 'workflow_paused' && field('task') === GATE.id && field('message') === MESSAGE, 'the trace no longer records the pause');
// the printed line, run: build reused, the gate answered, ship run
const R_BUILD = find(resumed, new RegExp(`^\\s+↷\\s+${esc(BUILD.id)}\\s+cache hit \\(resume\\)$`, 'u'), 'the resume no longer reuses build');
const R_GATE = taskRow(resumed, '✔', GATE.id, 'the resume no longer answers the gate');
const R_SHIP = taskRow(resumed, '✔', SHIP.id, 'the resume no longer ships');
const R_SUM = find(resumed, /^\s+resumed · 1 skipped \(cache hit\) · 2 ran live$/u, 'the resume summary changed');
story(record.some(l => l.trim() === 'answer: "true"'), 'the gate record lost its answer');
// what each step handed on (the terminal run's, then the resumed run's)
const arrow = l => l.match(/→ (.+?) · \d+B$/u)?.[1];
const OUT = {
  tty: { build: arrow(TTY_BUILD), ship: arrow(TTY_SHIP) },
  resumed: { ship: find(outputs, new RegExp(`^\\s+${esc(SHIP.id)}\\s`), 'the resumed run has no ship output').match(/ {2}("[^"]*") · \d+B/)?.[1] },
};
story(OUT.tty.build && OUT.tty.ship && OUT.resumed.ship, 'a step no longer hands on its output');
const CACHE_HIT = R_BUILD.match(/cache hit \(resume\)/)[0];

// ── the clock ───────────────────────────────────────────────────────────
const T = {
  track: 0.3,
  // at a terminal
  left: 2.55, cmd1: 3.1, enter1: 3.9, build1: 4.15, reach1: 4.6, ask: 4.7,
  key: 5.85, press: 6.35, open1: 6.45, ship1: 7.15,
  // nobody to ask
  rewind: 9.3, right: 9.45, cmd2: 10.0, enter2: 10.8, build2: 11.0, reach2: 11.45, frame2: 11.55, exit4: 11.85, save: 11.95,
  // the printed line: selected, copied, pasted, run
  sweep: 14.95, prompt: 15.35, fly: 15.45, land: 16.0, answer: 16.2, enter3: 16.45, frame3: 16.6,
  reuse: 19.45, restore: 19.5, open2: 19.82, ship2: 20.5,
};

const CAPS = [
  [0.2, 2.9, 'Nothing ships without a yes.', 'a yes.', C.human, 'GATED-SHIP.NIKA · BUILD, THEN A HUMAN GATE, THEN SHIP'],
  [2.9, 9.6, 'At your terminal, it asks.', 'it asks.', C.human, 'THE RUN STOPS AT THE GATE · YOUR Y OPENS IT'],
  [9.6, 14.7, 'In CI, nobody is there. It pauses.', 'It pauses.', C.amber, `NO HANG · NO FAILURE · EXIT ${EXITS.ci} · ITS STATE IS KEPT IN THE TRACE`],
  [14.7, 20.6, 'One printed line resumes it.', 'resumes it.', C.teal, 'BUILD IS REUSED · YOUR ANSWER RIDES IN THE LINE · SHIP RUNS'],
  [20.6, 26, 'Nothing ships without a yes.', 'a yes.', C.human, `IT ASKS AT A TERMINAL · IT PAUSES IN CI, EXIT ${EXITS.ci} · ONE LINE RESUMES IT`],
];

// ── geometry ────────────────────────────────────────────────────────────
const TY = 340; // the track's line
const BOX = { w: 212, h: 80 };
const BX = 600, GX = 960, SX = 1320; // build · the gate · ship
const DOOR = { x: GX - 66, y: 262, w: 132, h: 156, sign: 36 };
const EM_Y = DOOR.y + DOOR.sign + (DOOR.h - DOOR.sign) / 2;
const PL = { x: 64, y: 470, w: 880, h: 430 }; // at your terminal
const PR = { x: 976, y: 470, w: 880, h: 430 }; // in CI
const ST = { f: 'MM 400', size: 18 };
const ADV = cw(ST);
const PAD = 14;
const COLS = Math.floor((PL.w - 2 * PAD) / ADV);
const LH = 25;
const HEAD = 48;
const rowY = (p, r) => p.y + HEAD + 30 + r * LH;
const colX = (p, c) => p.x + PAD + c * ADV;

// ── lines: colour, wrap, vector glyphs ──────────────────────────────────
const STATUS = { '✔': C.teal, '↷': C.ice, '○': C.dim };
function spansOf(raw) {
  let m;
  if ((m = raw.match(/^(\s*)(◇ )(paused)( · awaiting an answer for )(`[^`]+`)$/u))) {
    return [{ s: m[1] }, { s: m[2], c: C.amber, glow: 0.6 }, { s: m[3], c: C.amber, b: true, glow: 0.3 }, { s: m[4], c: C.mist }, { s: m[5], c: C.ink }];
  }
  if ((m = raw.match(/^(\s*)(◇ )(\S+)( · )(.*?)(\s+)(\[y\/N\])( ?)(.*)$/u))) {
    return [{ s: m[1] }, { s: m[2], c: C.human, glow: 0.6 }, { s: m[3], c: C.ink, b: true }, { s: m[4], c: C.dim }, { s: m[5], c: C.human }, { s: m[6] }, { s: m[7], c: C.human, b: true }, { s: m[8] }, { s: m[9], c: C.human, b: true, glow: 0.7 }];
  }
  if ((m = raw.match(/^(\s*)(✔|↷|○)(\s+)((?:[▷◆·] )?)(\S+)(\s+)(.*)$/u))) {
    const [, ind, mk, sp, vg, id, sp2, rest] = m;
    const k = rest.indexOf('→ ');
    const out = [{ s: ind }, { s: mk, c: STATUS[mk], b: true, glow: mk === '○' ? 0 : 0.5 }, { s: sp }, { s: vg, c: C.dim }, { s: id, c: C.ink, b: true }, { s: sp2 }];
    out.push({ s: k >= 0 ? rest.slice(0, k) : rest, c: mk === '↷' ? C.ice : C.mist });
    if (k >= 0) out.push({ s: rest.slice(k), c: C.ice });
    return out;
  }
  if ((m = raw.match(/^(\s*)(trace: )(\S+)(.*)$/))) return [{ s: m[1] }, { s: m[2], c: C.dim }, { s: m[3], c: C.ice }, { s: m[4], c: C.dim }];
  if ((m = raw.match(/^(\s*)(resume: )(.*?)( · or false)$/))) return [{ s: m[1] }, { s: m[2], c: C.dim }, { s: m[3], c: C.ink, b: true }, { s: m[4], c: C.dim }];
  if ((m = raw.match(/^(\s*)(resumed)( · .*)$/))) return [{ s: m[1] }, { s: m[2], c: C.teal, b: true, glow: 0.4 }, { s: m[3], c: C.mist }];
  return [{ s: raw, c: C.mist }];
}

// Break a line at spaces into [{ a, b, col }] slices of at most COLS cells
// (continuation rows start `indent` cells in). `prefer` breaks before a
// matching word when one fits, so a flag keeps its value on its row. Only
// where the line breaks changes: every character stays.
function wrapCols(s, indent = 12, prefer = null) {
  const out = [];
  let start = 0, col = 0;
  while (s.length - start > COLS - col) {
    const limit = start + COLS - col;
    let cut = -1;
    if (prefer) for (let i = limit; i > start; i--) if (s[i] === ' ' && prefer.test(s.slice(i + 1))) { cut = i; break; }
    if (cut < 0) cut = s.lastIndexOf(' ', limit);
    if (cut <= start) cut = limit;
    out.push({ a: start, b: cut, col });
    start = cut;
    while (s[start] === ' ') start++;
    col = indent;
  }
  out.push({ a: start, b: s.length, col });
  return out;
}
function sliceSpans(spans, a, b) {
  const out = [];
  let pos = 0;
  for (const sp of spans) {
    const s0 = Math.max(a, pos), s1 = Math.min(b, pos + sp.s.length);
    if (s1 > s0) out.push({ ...sp, s: sp.s.slice(s0 - pos, s1 - pos) });
    pos += sp.s.length;
  }
  return out;
}

// Martian Mono has no glyph for the CLI's gate, cache-hit and verb marks:
// they are drawn as vectors in their own cell (the kit does the same for ✔).
const VEC = new Set(['◇', '◆', '▷', '↷']);
function vglyph(R, ch, x0, y, st, color, alpha, glow = 0) {
  const s = st.size, adv = cw(st);
  const cx = x0 + adv / 2, cy = y - s * 0.34, r = s * 0.36;
  const S = { color, w: Math.max(1.3, s * 0.09), alpha, glow };
  if (ch === '◇' || ch === '◆') poly(R, [[cx, cy - r], [cx + r, cy], [cx, cy + r], [cx - r, cy], [cx, cy - r]], ch === '◆' ? { ...S, fill: color } : S);
  else if (ch === '▷') poly(R, [[cx - r * 0.55, cy - r * 0.8], [cx + r * 0.8, cy], [cx - r * 0.55, cy + r * 0.8], [cx - r * 0.55, cy - r * 0.8]], S);
  else if (ch === '↷') {
    const ay = cy + r * 0.3, rr = r * 0.78;
    arc(R, cx, ay, rr, Math.PI, Math.PI * 1.97, S);
    poly(R, [[cx + rr - r * 0.42, ay - r * 0.1], [cx + rr, ay + r * 0.36], [cx + rr + r * 0.34, ay - r * 0.2]], S);
  }
}
function monoX(R, spans, x, y, { alpha = 1, st = ST } = {}) {
  const adv = cw(st);
  let col = 0;
  for (const sp of spans) {
    let run = '', runCol = col;
    const flush = () => {
      if (run) mono(R, [{ ...sp, s: run }], x + runCol * adv, y, { alpha, st });
      run = '';
    };
    for (const ch of sp.s) {
      if (VEC.has(ch)) {
        flush();
        vglyph(R, ch, x + col * adv, y, st, sp.c ?? C.mist, alpha, sp.glow ?? 0);
        col += 1;
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

// ── the two terminals, laid out once ────────────────────────────────────
// { cmd } types after "$ " · { fly } is a command whose words arrive from
// the printed line · { line } a captured line (wrapped, or cut after a
// verbatim prefix) · { fold } stands for captured lines left out.
function layout(p, script) {
  const rows = [];
  for (const it of script) {
    if (it.cmd || it.fly) {
      const s = `$ ${it.cmd ?? it.fly}`;
      for (const w of wrapCols(s, 2, /^--/)) rows.push({ it, kind: it.cmd ? 'cmd' : 'fly', full: s, ...w });
    } else if (it.fold) rows.push({ it, kind: 'fold', a: 0, b: 3, col: 0, spans: [{ s: '  ⋯', c: C.dim }] });
    else {
      const spans = spansOf(it.line);
      if (it.cut && it.line.length > COLS) {
        const b = it.line.lastIndexOf(' · ', COLS - 2) + 1;
        rows.push({ it, kind: 'line', a: 0, b, col: 0, spans: [...sliceSpans(spans, 0, b), { s: '…', c: C.dim }] });
      } else for (const w of wrapCols(it.line, it.indent ?? 12, it.prefer ?? null)) rows.push({ it, kind: 'line', ...w, spans: sliceSpans(spans, w.a, w.b) });
    }
  }
  rows.forEach((r, i) => { r.y = rowY(p, i); });
  return rows;
}
const LEFT = layout(PL, [
  { cmd: RUN, t: T.cmd1, dur: 0.75 },
  { line: TTY_BUILD, t: T.build1, indent: 16, prefer: /^→/ },
  { fold: true, t: T.ask - 0.05 },
  { line: TTY_ASK, t: T.ask, ask: true },
  { fold: true, t: T.press + 0.4 },
  { line: TTY_GATE, t: T.press + 0.45, indent: 16, prefer: /^→/ },
  { line: TTY_SHIP, t: T.ship1, indent: 16, prefer: /^→/ },
]);
const RIGHT = layout(PR, [
  { cmd: RUN, t: T.cmd2, dur: 0.75 },
  { line: CI_BUILD, t: T.frame2 },
  { line: CI_PAUSED, t: T.frame2 + 0.05 },
  { line: CI_TRACE, t: T.frame2 + 0.1, cut: true },
  { line: CI_RESUME, t: T.frame2 + 0.15, indent: 12, prefer: /^--/, resume: true },
  { fly: RESUME_CMD, t: T.prompt },
  { line: R_BUILD, t: T.frame3 },
  { line: R_GATE, t: T.frame3 + 0.05 },
  { line: R_SHIP, t: T.frame3 + 0.1 },
  { line: R_SUM, t: T.frame3 + 0.15 },
]);
const ASK_ROW = LEFT.find(r => r.it.ask);
const TRACE_ROW = RIGHT.find(r => r.it.cut);
const RESUME_ROWS = RIGHT.filter(r => r.it.resume);
const FLY_ROWS = RIGHT.filter(r => r.kind === 'fly');
// The printed command and the pasted one break at the same words, so each
// printed row carries exactly the command text of one pasted row: where
// that text starts in the printed row, and where it starts once pasted.
const CMD0 = RESUME_ROWS[0].it.line.indexOf(RESUME_CMD);
const COPY = FLY_ROWS.map((dst, i) => {
  const src = RESUME_ROWS[i];
  const a = Math.max(dst.a, 2), s = dst.full.slice(a, dst.b);
  const from = CMD0 + a - 2;
  story(src && src.it.line.slice(from, from + s.length) === s, 'the pasted command no longer matches the printed line');
  return { s, a, from: { x: colX(PR, src.col + from - src.a), y: src.y }, to: { x: colX(PR, dst.col + a - dst.a), y: dst.y } };
});

// ── camera ──────────────────────────────────────────────────────────────
// The captions keep their band at the top: every shot frames the world
// inside the area below it.
const AREA = { x: 40, y: 250, w: 1840, h: 758 };
// the opening holds the three steps large; the terminal and CI shots
// frame the track with their pane; the paste frames the whole CI pane
const SHOT_A = frameBox({ x: 430, y: 236, w: 1060, h: 220 }, 60, AREA);
const SHOT_B = frameBox({ x: PL.x, y: 240, w: SX + BOX.w / 2 - PL.x + 24, h: rowY(PL, LEFT.length - 1) + 14 - 240 }, 12, AREA);
const SHOT_C = frameBox({ x: BX - BOX.w / 2 - 24, y: 240, w: PR.x + PR.w - (BX - BOX.w / 2 - 24), h: RESUME_ROWS[2].y + 14 - 240 }, 12, AREA);
const SHOT_D = frameBox(PR, 12, AREA);
const SHOTS = [
  { at: 0, cam: SHOT_A },
  { at: 3.2, cam: SHOT_B, move: 0.7 },
  { at: 10.0, cam: SHOT_C, move: 0.7 },
  { at: 14.9, cam: SHOT_D, move: 0.7 },
  { at: 19.85, cam: WIDE, move: 0.8 },
];
export const camera = t => cameraPath(t, SHOTS);

export function env() {
  return { bgGlow: 1, gridAlpha: 0.22, gridX: 0, gridY: 0, bgY: 560 };
}

const PLATE = `every terminal line captured from the real cli · nika ${NIKA_VERSION} · offline, no model · illustration: the door, its light, the keypress, the paste, the ci frame`;
export function chrome(R, t) {
  const a = loopFade(t, meta.duration);
  // while the camera is in, the world passes under a dark band that keeps
  // the caption readable
  const z = smooth(1.02, 1.1, camera(t).s);
  if (z > 0) {
    // the glow buffer is added after this pass: erase its light there too
    const ctx = R.ctx;
    ctx.save();
    if (R.glowPass) ctx.globalCompositeOperation = 'destination-out';
    const g = ctx.createLinearGradient(0, 0, 0, 262);
    g.addColorStop(0, rgba(C.bg0, z));
    g.addColorStop(0.76, rgba(C.bg0, 0.97 * z));
    g.addColorStop(1, rgba(C.bg0, 0));
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, 1920, 262);
    ctx.restore();
  }
  frame(R, t, { kicker: 'the durable human gate · gated-ship.nika', plate: PLATE, alpha: a, scrim: z });
  for (const [t0, t1, main, accent, color, sub] of CAPS) headline({ ...R, fade: a }, t, t0, t1, main, sub, { accent, accentColor: color });
}

// ── the track ───────────────────────────────────────────────────────────
const until = (t0, d, t) => 1 - smooth(t0, t0 + d, t);
const openK = t => (t < T.rewind + 0.4
  ? E.inOutCubic(seg(t, T.open1, T.open1 + 0.5)) * until(T.rewind, 0.4, t)
  : E.inOutCubic(seg(t, T.open2, T.open2 + 0.5)));
const askK = t => smooth(T.reach1, T.reach1 + 0.3, t) * until(T.open1 + 0.2, 0.5, t);
const pauseK = t => smooth(T.reach2, T.reach2 + 0.3, t) * until(T.open2 - 0.15, 0.3, t);

// arc-length point along a polyline
function along(pts, k) {
  const L = [0];
  for (let i = 1; i < pts.length; i++) L.push(L[i - 1] + Math.hypot(pts[i][0] - pts[i - 1][0], pts[i][1] - pts[i - 1][1]));
  const d = clamp(k) * L[L.length - 1];
  let i = 1;
  while (i < pts.length - 1 && L[i] < d) i++;
  const u = (d - L[i - 1]) / (L[i] - L[i - 1] || 1);
  return [lerp(pts[i - 1][0], pts[i][0], u), lerp(pts[i - 1][1], pts[i][1], u)];
}
function spark(R, x, y, a, color) {
  light(R, x, y, 46, color, 0.3 * a, 1);
  circle(R, x, y, 6.5, { fill: color, alpha: a, glow: 1 });
  circle(R, x, y, 2.6, { fill: '#ffffff', alpha: a * 0.9 });
}
function pulse(R, pts, k, a, color) {
  if (k <= 0 || k >= 1) return;
  for (let i = 6; i >= 1; i--) {
    const q = along(pts, k - i * 0.022);
    circle(R, q[0], q[1], 5.5 - i * 0.6, { fill: color, alpha: a * (0.42 - i * 0.055), glow: 0.5 });
  }
  const p = along(pts, k);
  spark(R, p[0], p[1], a, color);
}

const EDGE1 = [[BX + BOX.w / 2, TY], [DOOR.x - 8, TY]];
const EDGE2 = [[DOOR.x + DOOR.w + 8, TY], [SX - BOX.w / 2, TY]];
const HELD = [DOOR.x - 16, TY];
// the gate's tethers: down the gap between the terminals, then along a line
const tether = (x1, y1) => {
  const y = y1 - 6, dir = Math.sign(x1 - GX), r = 12;
  return [[GX, DOOR.y + DOOR.h], [GX, y - r], [GX + dir * r * 0.3, y - r * 0.3], [GX + dir * r, y], [x1, y]];
};
const ASK_PATH = tether(colX(PL, ASK_ROW.it.line.length) + 18, ASK_ROW.y);
const SAVE_PATH = tether(colX(PR, 3), TRACE_ROW.y);

function edge(R, pts, a, lit, color) {
  poly(R, pts, { color: lit > 0.02 ? color : C.faint, w: 1.6 + lit * 0.6, alpha: a * (0.7 + 0.3 * lit), glow: 0.45 * lit });
  const [x, y] = pts[pts.length - 1];
  poly(R, [[x - 9, y - 5.5], [x, y], [x - 9, y + 5.5]], { color: lit > 0.02 ? color : C.faint, w: 1.6, alpha: a * (0.7 + 0.3 * lit), glow: 0.45 * lit });
}

function station(R, cx, step, s, a) {
  const k = s.show;
  if (k <= 0) return;
  const x = cx - BOX.w / 2, y = TY - BOX.h / 2;
  const lit = s.done;
  const col = s.reused > 0.5 ? C.ice : C.teal;
  const pop = 0.94 + 0.06 * E.outBack(k);
  const ctx = R.ctx;
  ctx.save();
  ctx.translate(cx, TY);
  ctx.scale(pop, pop);
  ctx.translate(-cx, -TY);
  rrect(R, x, y, BOX.w, BOX.h, 16, { color: lit > 0.02 ? col : C.dim, w: 1.4 + lit * 0.8, alpha: a * k, fill: '#07101f', fillAlpha: 0.92, glow: 0.5 * lit });
  if (lit > 0.02) rrect(R, x, y, BOX.w, BOX.h, 16, { fill: col, fillAlpha: 0.07 * lit, alpha: a * k });
  if (s.run > 0 && s.run < 1) rect(R, x + 16, y + BOX.h - 13, (BOX.w - 32) * s.run, 3, { fill: C.ice, alpha: a, glow: 0.8 });
  text(R, step.id, cx, TY + 10, { f: 'Geist 600', size: 30, tracking: -0.6, color: C.ink, alpha: a * k, align: 'center' });
  ctx.restore();
  text(R, step.verb, cx, y + BOX.h + 26, { f: 'MM 400', size: 14, color: C.dim, alpha: a * k, align: 'center' });
  if (lit > 0.02) {
    const bx = x + BOX.w - 6, by = y + 6;
    circle(R, bx, by, 15, { fill: C.bg0, color: col, w: 1.6, alpha: a * lit, glow: 0.5 });
    if (s.reused > 0.5) vglyph(R, '↷', bx - ADV / 2, by + ST.size * 0.34, ST, col, a * lit, 0.5);
    else check(R, bx, by, 13, E.snap(clamp(lit * 1.4)), { color: col, w: 2.2, alpha: a, glow: 0.6 });
  }
  if (s.out && s.outA > 0) text(R, s.out, cx, y + BOX.h + 52, { f: 'MM 500', size: 16, color: s.outC ?? C.ice, alpha: a * s.outA, align: 'center', glow: 0.2 });
}

function door(R, t, a) {
  const { x, y, w, h, sign } = DOOR;
  const build = E.snap(seg(t, T.track + 0.3, T.track + 1.1));
  if (build <= 0) return;
  const open = openK(t), ask = askK(t), pause = pauseK(t);
  const shut = E.inOutCubic(seg(t, T.track + 0.8, T.track + 1.3)); // the leaves slide in once
  const ctx = R.ctx;
  const doorX0 = x + 5, doorX1 = x + w - 5, doorY0 = y + sign + 3, doorY1 = y + h - 2;
  const lw = (doorX1 - doorX0) / 2;
  // a person's light: warm while it asks, a shaft once it opens
  if (ask > 0) light(R, GX, EM_Y, 190, C.human, 0.16 * ask * a, 0.8);
  if (open > 0.01) {
    ctx.save();
    ctx.beginPath();
    ctx.rect(doorX0, doorY0, doorX1 - doorX0, doorY1 - doorY0);
    ctx.clip();
    if (!R.glowPass) {
      const g = ctx.createLinearGradient(doorX0, 0, doorX1, 0);
      g.addColorStop(0, 'rgba(255,241,222,0.05)');
      g.addColorStop(0.5, `rgba(255,241,222,${(0.42 * open * a).toFixed(3)})`);
      g.addColorStop(1, 'rgba(255,241,222,0.05)');
      ctx.fillStyle = g;
      ctx.fillRect(doorX0, doorY0, doorX1 - doorX0, doorY1 - doorY0);
    }
    line(R, doorX0, TY, doorX1, TY, { color: C.human, w: 1.6, alpha: a * open, glow: 0.8 });
    ctx.restore();
    light(R, GX, TY, 150 * open, C.human, 0.22 * open * a, 0.9);
  }
  // the leaves, and the gate's mark riding on them
  const slide = lw * open + lw * (1 - shut);
  ctx.save();
  ctx.beginPath();
  ctx.rect(doorX0, doorY0, doorX1 - doorX0, doorY1 - doorY0);
  ctx.clip();
  for (const side of [-1, 1]) {
    const lx = side < 0 ? doorX0 - slide : GX + slide;
    rrect(R, lx, doorY0, lw, doorY1 - doorY0, 2, { color: C.human, w: 1.1, alpha: a * 0.5, fill: '#16120d', fillAlpha: 0.98 });
    for (const gx of [0.3, 0.62]) line(R, lx + lw * gx, doorY0 + 12, lx + lw * gx, doorY1 - 12, { color: C.human, w: 1, alpha: a * 0.1 });
    ctx.save();
    ctx.beginPath();
    ctx.rect(lx, doorY0, lw, doorY1 - doorY0);
    ctx.clip();
    mark(R, t, GX + side * slide, a, ask, pause);
    ctx.restore();
  }
  ctx.restore();
  // the frame and its sign
  poly(R, [[x, y + h], [x, y + 12], [x + 12, y], [x + w - 12, y], [x + w, y + 12], [x + w, y + h]], { color: C.human, w: 2.4, alpha: a * (0.85 + 0.15 * ask), glow: 0.25 + 0.5 * ask + 0.3 * open }, build);
  line(R, x + 8, y + sign, x + w - 8, y + sign, { color: C.human, w: 1, alpha: a * 0.45 * build });
  line(R, x - 14, y + h, x + w + 14, y + h, { color: C.human, w: 1.2, alpha: a * 0.5 * build });
  text(R, GATE.id, GX, y + 25, { f: 'Geist 600', size: 21, color: C.human, alpha: a * smooth(T.track + 0.7, T.track + 1.0, t), align: 'center', glow: 0.25 * ask });
  text(R, GATE.verb, GX, y - 14, { f: 'MM 400', size: 14, color: C.human, alpha: a * 0.75 * smooth(T.track + 0.9, T.track + 1.2, t), align: 'center' });
  // it asks: one ring leaves the gate
  const ring = seg(t, T.reach1, T.reach1 + 0.9);
  if (ring > 0 && ring < 1) rrect(R, x - 30 * E.outCubic(ring), y - 30 * E.outCubic(ring), w + 60 * E.outCubic(ring), h + 60 * E.outCubic(ring), 16, { color: C.human, w: 1.4, alpha: a * (1 - ring) * 0.8, glow: 0.8 });
  const ring2 = seg(t, T.reach2, T.reach2 + 0.9);
  if (ring2 > 0 && ring2 < 1) rrect(R, x - 30 * E.outCubic(ring2), y - 30 * E.outCubic(ring2), w + 60 * E.outCubic(ring2), h + 60 * E.outCubic(ring2), 16, { color: C.amber, w: 1.4, alpha: a * (1 - ring2) * 0.8, glow: 0.8 });
}

// The gate's mark on its leaves: warm while it asks, amber and paused when
// nobody is there to answer.
function mark(R, t, cx, a, ask, pause) {
  const r = 17;
  const pts = [[cx, EM_Y - r], [cx + r, EM_Y], [cx, EM_Y + r], [cx - r, EM_Y], [cx, EM_Y - r]];
  const col = pause > 0.02 ? C.amber : C.human;
  poly(R, pts, { color: col, w: 2, alpha: a * (0.55 + 0.45 * Math.max(ask, pause)), glow: 0.2 + 0.7 * Math.max(ask, pause), fill: pause > 0.02 ? C.amber : undefined, fillAlpha: 0.95 * pause });
  if (pause > 0.02) {
    rect(R, cx - 5.5, EM_Y - 6.5, 3.5, 13, { fill: C.bg0, alpha: a * pause });
    rect(R, cx + 2, EM_Y - 6.5, 3.5, 13, { fill: C.bg0, alpha: a * pause });
  }
}

function track(R, t, a) {
  const show = i => E.snap(seg(t, T.track + i * 0.35, T.track + i * 0.35 + 0.5));
  const rw = until(T.rewind, 0.4, t);
  // the edges light behind the pulses
  const e1 = Math.max(smooth(T.build1, T.reach1, t) * rw, smooth(T.build2, T.reach2, t));
  const e2 = Math.max(smooth(T.open1 + 0.25, T.ship1, t) * rw, smooth(T.open2 + 0.25, T.ship2, t));
  edge(R, EDGE1, a * show(0.6), e1, C.cyan);
  edge(R, EDGE2, a * show(1.4), e2, C.cyan);
  const reused = smooth(T.reuse, T.reuse + 0.3, t);
  station(R, BX, BUILD, {
    show: show(0),
    run: t < T.build1 ? seg(t, T.enter1, T.build1) : seg(t, T.enter2, T.build2),
    done: Math.max(smooth(T.build1, T.build1 + 0.2, t) * rw, smooth(T.build2, T.build2 + 0.2, t)),
    reused,
    out: reused > 0 ? CACHE_HIT : `→ ${OUT.tty.build}`,
    outA: reused > 0 ? reused : smooth(T.build1 + 0.1, T.build1 + 0.35, t) * rw,
    outC: C.ice,
  }, a);
  station(R, SX, SHIP, {
    show: show(2),
    run: t < T.rewind ? seg(t, T.ship1 - 0.25, T.ship1) : seg(t, T.ship2 - 0.25, T.ship2),
    done: Math.max(smooth(T.ship1, T.ship1 + 0.2, t) * rw, smooth(T.ship2, T.ship2 + 0.2, t)),
    reused: 0,
    out: `→ ${t < T.rewind + 0.4 ? OUT.tty.ship : OUT.resumed.ship}`,
    outA: Math.max(smooth(T.ship1 + 0.1, T.ship1 + 0.35, t) * rw, smooth(T.ship2 + 0.1, T.ship2 + 0.35, t)),
    outC: C.teal,
  }, a);
  door(R, t, a);
  // the pulses: along the track, held at the gate, into the trace and back
  pulse(R, EDGE1, E.inOutCubic(seg(t, T.build1, T.reach1)), a, C.cyan);
  pulse(R, EDGE2, E.inOutCubic(seg(t, T.open1 + 0.25, T.ship1)), a, C.cyan);
  pulse(R, EDGE1, E.inOutCubic(seg(t, T.build2, T.reach2)), a, C.cyan);
  const held = smooth(T.reach2 - 0.02, T.reach2 + 0.05, t) * until(T.save, 0.05, t);
  if (held > 0) {
    light(R, HELD[0], HELD[1], 70, C.amber, 0.28 * a * held, 1);
    spark(R, HELD[0], HELD[1], a * held, C.amber);
  }
  pulse(R, EDGE2, E.inOutCubic(seg(t, T.open2 + 0.25, T.ship2)), a, C.cyan);
}

function tethers(R, t, a) {
  // the question, into the terminal line that asks it
  const ak = seg(t, T.ask, T.ask + 0.45);
  if (ak > 0) {
    const keep = 0.35 + 0.65 * until(T.ship1 + 0.3, 0.6, t);
    poly(R, ASK_PATH, { color: C.human, w: 1.5, alpha: a * keep, glow: 0.5 * keep }, E.inOutCubic(ak));
    if (ak >= 1) circle(R, ASK_PATH[4][0], ASK_PATH[4][1], 3.5, { fill: C.human, alpha: a * keep, glow: 0.6 });
  }
  // the paused state, into the trace the line names; back when it resumes
  const sk = seg(t, T.save, T.save + 0.45);
  if (sk > 0) {
    const back = smooth(T.restore, T.restore + 0.3, t);
    const col = back > 0.5 ? C.ice : C.amber;
    const keep = 0.35 + 0.65 * Math.max(until(T.frame2 + 2.6, 0.6, t), back * until(T.ship2 + 0.4, 0.6, t));
    poly(R, SAVE_PATH, { color: col, w: 1.5, alpha: a * keep, glow: 0.5 * keep }, E.inOutCubic(sk));
    if (sk >= 1) circle(R, SAVE_PATH[4][0], SAVE_PATH[4][1], 3.5, { fill: col, alpha: a * keep, glow: 0.6 });
  }
  pulse(R, SAVE_PATH, E.inOutCubic(seg(t, T.save, T.save + 0.45)), a, C.amber);
  pulse(R, [...SAVE_PATH].reverse(), E.inOutCubic(seg(t, T.restore, T.restore + 0.3)), a, C.ice);
}

// ── the terminals ───────────────────────────────────────────────────────
function paneFrame(R, p, { a, k, title, note }) {
  if (k <= 0 || a <= 0) return;
  const h = Math.max(HEAD, p.h * k);
  rrect(R, p.x, p.y, p.w, h, 14, { color: C.faint, w: 1, alpha: a, fill: '#050c19', fillAlpha: 0.92 });
  for (let i = 0; i < 3; i++) circle(R, p.x + 22 + i * 15, p.y + 24, 4, { fill: C.faint, alpha: a });
  const tst = { f: 'Geist 600', size: 22, tracking: -0.3 };
  text(R, title, p.x + 74, p.y + 32, { ...tst, color: C.ink, alpha: a });
  text(R, note, p.x + 74 + measure(title, tst) + 14, p.y + 32, { f: 'Geist 500', size: 18, color: C.mist, alpha: a * 0.9 });
  line(R, p.x + 1, p.y + HEAD, p.x + p.w - 1, p.y + HEAD, { color: C.line, w: 1, alpha: a });
}

function cmdSpans(r, n, t) {
  const s = r.full.slice(r.a, Math.min(r.b, n));
  const spans = [];
  let pos = r.a;
  if (pos < 2) {
    spans.push({ s: s.slice(0, 2 - pos), c: C.dim });
    pos = 2;
  }
  const rest = s.slice(pos - r.a);
  // the answer the person gives, once it opens the gate
  const i = r.full.indexOf('--answer');
  const warm = r.kind === 'fly' && t >= T.answer ? smooth(T.answer, T.answer + 0.25, t) : 0;
  if (warm > 0 && i >= pos && i < pos + rest.length) {
    spans.push({ s: rest.slice(0, i - pos), c: C.ink, b: true }, { s: rest.slice(i - pos), c: C.human, b: true, glow: 0.6 * warm });
  } else if (warm > 0 && i < pos) spans.push({ s: rest, c: C.human, b: true, glow: 0.6 * warm });
  else spans.push({ s: rest, c: C.ink, b: true });
  return spans;
}

function rows(R, t, p, list, a) {
  for (const r of list) {
    const it = r.it;
    if (t < it.t) continue;
    const k = smooth(it.t, it.t + 0.12, t);
    const y = r.y + 5 * (1 - k);
    const x = colX(p, r.col);
    if (r.kind === 'cmd') {
      const n = 2 + Math.floor((r.full.length - 2) * clamp((t - it.t) / it.dur));
      if (n <= r.a) continue;
      monoX(R, cmdSpans(r, n, t), x, y, { alpha: a * k });
      const typing = t < it.t + it.dur + 0.3 && n >= r.a && n <= r.b;
      if (typing) caret(R, t, x + (Math.min(n, r.b) - r.a) * ADV + 2, y, a);
    } else if (r.kind === 'fly') {
      // the prompt waits for the paste, then the pasted command sits there
      const pasted = t >= T.land;
      if (r.a === 0 || pasted) monoX(R, cmdSpans(r, pasted ? r.b : 2, t), x, y, { alpha: a * k });
      const last = FLY_ROWS[pasted ? FLY_ROWS.length - 1 : 0];
      if (r === last && t < T.enter3) caret(R, t, x + ((pasted ? r.b : 2) - r.a) * ADV + 2, y, a);
    } else {
      let spans = r.spans;
      // the answer is typed at the prompt: it shows once the key is pressed
      if (it.ask && t < T.press + 0.04) spans = sliceSpans(spans, 0, it.line.length - 1 - r.a);
      monoX(R, spans, x, y, { alpha: a * k });
    }
  }
}

function caret(R, t, x, y, a) {
  const blink = Math.floor(t * 2.4) % 2 === 0 ? 1 : 0.25;
  rect(R, x, y - ST.size + 2, 2.5, ST.size + 3, { fill: C.ice, alpha: a * blink, glow: 0.8 });
}

// a row's highlight bar, like the code cards'
function bar(R, p, r0, r1, color, k, a) {
  if (k <= 0) return;
  const y0 = r0.y - LH + 7, h = r1.y - r0.y + LH;
  rect(R, p.x + 6, y0, p.w - 12, h, { fill: color, alpha: a * 0.1 * k });
  rect(R, p.x + 6, y0, 3, h, { fill: color, alpha: a * k, glow: 0.8 });
}

function terminals(R, t, a) {
  const lk = E.snap(seg(t, T.left, T.left + 0.5));
  const rk = E.snap(seg(t, T.right, T.right + 0.5));
  // the terminal story steps back while CI plays, and returns for the end
  const ldim = 1 - (0.72 * smooth(T.rewind, T.rewind + 0.5, t) + 0.2 * smooth(14.3, 14.9, t)) * until(T.reuse, 0.8, t);
  paneFrame(R, PL, { a: a * ldim, k: lk, title: 'At your terminal', note: '· a person answers' });
  paneFrame(R, PR, { a, k: rk, title: 'In CI', note: '· nobody to ask · stdin is not a terminal' });
  if (lk >= 1) {
    bar(R, PL, ASK_ROW, ASK_ROW, C.human, smooth(T.ask, T.ask + 0.25, t) * (0.4 + 0.6 * until(T.ship1 + 0.3, 0.6, t)), a * ldim);
    rows(R, t, PL, LEFT, a * ldim);
  }
  if (rk >= 1) {
    const paused = RIGHT.find(r => r.it.line === CI_PAUSED);
    bar(R, PR, paused, paused, C.amber, smooth(T.frame2 + 0.05, T.frame2 + 0.3, t) * (0.4 + 0.6 * until(T.frame3, 0.6, t)), a);
    bar(R, PR, RESUME_ROWS[0], RESUME_ROWS[2], C.ice, smooth(T.frame2 + 0.6, T.frame2 + 0.9, t) * (0.4 + 0.6 * until(T.frame3, 0.6, t)), a);
    bar(R, PR, FLY_ROWS[0], RIGHT[RIGHT.length - 1], C.teal, smooth(T.frame3 + 0.2, T.frame3 + 0.5, t), a);
    selection(R, t, a);
    rows(R, t, PR, RIGHT, a);
  }
  exits(R, t, a, ldim);
}

// ── the human moments ───────────────────────────────────────────────────
// The question, magnified below its line while the gate waits for the
// person: the captured line from the question on, verbatim.
const LOUPE = { f: 'MM 500', size: 26 };
const ASK_AT = TTY_ASK.indexOf(MESSAGE);
const LOUPE_BOX = (() => {
  const w = (TTY_ASK.length - ASK_AT) * cw(LOUPE) + 44;
  return { x: colX(PL, 0), y: ASK_ROW.y + 22, w, h: 60 };
})();
function loupe(R, t, a) {
  const k = E.snap(seg(t, T.ask + 0.2, T.ask + 0.55)) * until(T.press + 0.3, 0.3, t);
  if (k <= 0) return;
  const { x, y, w, h } = LOUPE_BOX;
  const ctx = R.ctx;
  ctx.save();
  // it grows out of the line it magnifies
  const s = 0.6 + 0.4 * k;
  ctx.translate(x + 30, y);
  ctx.scale(s, s);
  ctx.translate(-(x + 30), -y);
  rrect(R, x, y + 8, w, h, 12, { fill: '#000000', fillAlpha: 0.4, alpha: a * k });
  rrect(R, x, y, w, h, 12, { color: C.human, w: 1.4, alpha: a * k, fill: '#16120d', fillAlpha: 0.97, glow: 0.35 });
  poly(R, [[x + 18, y], [x + 30, y - 12], [x + 42, y]], { color: C.human, w: 1.4, alpha: a * k, fill: '#16120d', fillAlpha: 0.97 });
  const end = t < T.press + 0.04 ? TTY_ASK.length - 1 : TTY_ASK.length;
  monoX(R, sliceSpans(spansOf(TTY_ASK), ASK_AT, end), x + 22, y + 40, { alpha: a * k, st: LOUPE });
  if (end === TTY_ASK.length) {
    const ux = x + 22 + (TTY_ASK.length - 1 - ASK_AT) * cw(LOUPE);
    line(R, ux, y + 48, ux + cw(LOUPE), y + 48, { color: C.human, w: 2, alpha: a * k, glow: 0.8 });
  }
  ctx.restore();
}

// the keypress (illustration): the key the capture typed, pressed once
function keycap(R, t, a) {
  const k = smooth(T.key, T.key + 0.2, t) * until(T.press + 0.45, 0.3, t);
  if (k <= 0) return;
  const press = E.outCubic(seg(t, T.press, T.press + 0.08)) * until(T.press + 0.16, 0.12, t);
  const s = 50;
  const x = LOUPE_BOX.x + LOUPE_BOX.w + 18, y = LOUPE_BOX.y + (LOUPE_BOX.h - s) / 2 + 5 * press;
  rrect(R, x, y + 6 - 5 * press, s, s, 10, { fill: '#000000', fillAlpha: 0.5, alpha: a * k });
  rrect(R, x, y, s, s, 10, { color: C.human, w: 1.6, alpha: a * k, fill: '#1c1812', fillAlpha: 0.98, glow: 0.3 + 0.7 * press });
  text(R, TTY_ASK.slice(-1), x + s / 2, y + s / 2 + 6, { f: 'Geist 600', size: 17, color: C.human, alpha: a * k, align: 'center' });
  const ring = seg(t, T.press, T.press + 0.5);
  if (ring > 0 && ring < 1) circle(R, x + s / 2, y + s / 2, 28 + 40 * E.outCubic(ring), { color: C.human, w: 1.4, alpha: a * (1 - ring) * 0.8, glow: 0.8 });
}

// The exit codes the captures recorded, on the line that settled each run:
// paused is not failed.
const EXIT_ON = [
  { p: PL, row: () => ({ y: LEFT[LEFT.length - 1].y + LH }), code: EXITS.tty_yes, color: C.teal, t0: T.ship1 + 0.25, dim: () => 1 },
  { p: PR, row: () => RIGHT.find(r => r.it.line === CI_PAUSED), code: EXITS.ci, color: C.amber, t0: T.exit4, dim: t => 0.55 + 0.45 * until(T.frame3, 0.6, t) },
  { p: PR, row: () => RIGHT[RIGHT.length - 1], code: EXITS.resume, color: C.teal, t0: T.frame3 + 0.3, dim: () => 1 },
];
function exits(R, t, a, ldim) {
  const st = { f: 'Geist 600', size: 22 };
  for (const e of EXIT_ON) {
    // the terminal's pill leaves with its pane while CI plays
    const lit = e.p === PL ? smooth(0.45, 0.95, ldim) : 1;
    const k = E.snap(seg(t, e.t0, e.t0 + 0.4));
    const al = a * k * e.dim(t) * lit;
    if (al <= 0) continue;
    const label = `exit ${e.code}`, row = e.row();
    const w = measure(label, st) + 36, x = e.p.x + e.p.w - 24 - w, y = row.y - 24 + 6 * (1 - k);
    rrect(R, x, y, w, 34, 17, { color: e.color, w: 1.6, alpha: al, fill: e.color, fillAlpha: 0.12, glow: 0.5 * lit });
    text(R, label, x + 18, y + 24, { ...st, color: e.color, alpha: al, glow: 0.3 * lit });
  }
}

// The printed command is selected (its rows, as a person drags over them),
// copied as one block that floats over the log, and pasted at the prompt:
// the same words, the same breaks, ten columns left and three rows down.
function selection(R, t, a) {
  const k = seg(t, T.sweep, T.sweep + 0.4);
  const on = k * until(T.frame3, 0.4, t);
  if (on <= 0) return;
  COPY.forEach((c, i) => {
    const w = c.s.length * ADV * clamp(k * COPY.length - i);
    if (w > 0) rect(R, c.from.x - 3, c.from.y - LH + 7, w + 6, LH - 2, { fill: C.cyan, alpha: a * 0.22 * on });
  });
}
function paste(R, t, a) {
  const k = (t - T.fly) / (T.land - T.fly); // runs past 1 while the card dissolves
  if (k <= 0 || k >= 1.12) return;
  const e = E.inOutCubic(clamp(k)), lift = Math.sin(Math.PI * clamp(k));
  const dx = COPY[0].to.x - COPY[0].from.x, dy = COPY[0].to.y - COPY[0].from.y;
  const fade = 1 - smooth(1, 1.12, k);
  const w = Math.max(...COPY.map(c => c.s.length)) * ADV;
  const x0 = COPY[0].from.x + dx * e + 14 * lift, y0 = COPY[0].from.y + dy * e - 10 * lift;
  const top = y0 - LH + 4, h = COPY[COPY.length - 1].from.y - COPY[0].from.y + LH + 2;
  const ctx = R.ctx;
  ctx.save();
  const s = 1 + 0.05 * lift;
  ctx.translate(x0 + w / 2, top + h / 2);
  ctx.scale(s, s);
  ctx.translate(-(x0 + w / 2), -(top + h / 2));
  rrect(R, x0 - 14, top + 10 + 10 * lift, w + 28, h + 12, 10, { fill: '#000000', fillAlpha: 0.45 * lift, alpha: a * fade });
  rrect(R, x0 - 12, top - 4, w + 24, h + 8, 10, { color: C.cyan, w: 1.4, alpha: a * fade, fill: '#081a2c', fillAlpha: 0.96, glow: 0.5 });
  COPY.forEach(c => monoX(R, [{ s: c.s, c: C.ink, b: true }], x0 + (c.from.x - COPY[0].from.x), y0 + (c.from.y - COPY[0].from.y), { alpha: a * fade }));
  ctx.restore();
}

export function draw(R, t) {
  const a = loopFade(t, meta.duration);
  tethers(R, t, a);
  track(R, t, a);
  terminals(R, t, a);
  loupe(R, t, a);
  keycap(R, t, a);
  paste(R, t, a);
}
