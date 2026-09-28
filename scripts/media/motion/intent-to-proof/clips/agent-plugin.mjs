// agent-plugin · "Your agent writes it. Nika checks it. You keep it."
// A person asks their coding agent for a repeatable job in one sentence.
// The agent loads the Nika plugin's authoring skill, writes a .nika file,
// runs `nika check`, repairs what the check refuses, checks again,
// rehearses the run and leaves a file the person keeps.
//
// Real, and read here: the skill's name and its own one-line description
// (.agents/plugins/nika/skills/nika-authoring/SKILL.md) and the clients
// the kit names (.agents/plugins/nika/CHANGELOG.md); both versions of the
// file (scripts/media/fixtures/release-notes-draft.nika, the draft, and
// release-notes.nika, the fix) and the line diff between them; both
// verdicts, the rehearsal and the files it left, captured from the binary
// by scripts/media/capture/agent-plugin.sh (media/raw/agent-plugin-*).
// The run is `--model mock/echo`: a rehearsal, and the clip says so.
// Illustration, and named on the plate: the agent session and its step
// labels, the project frame, and the scan that stands for the audit.
// The clip refuses to render if the captures stop telling this story.
import { C, E, seg, smooth, clamp, lerp, repoLines, readRepo, NIKA_VERSION, codeCard, frame, headline, loopFade, panel, pill, mono, cliSpans, cw, lineDiff, cameraPath, frameBox, WIDE } from './kit.mjs';
import { text, rrect, rect, line, circle, arc, poly, check, measure, bezierPts, light, brackets, diamond, DW, DH } from '../src/engine/render.mjs';
import { rgba } from '../src/engine/core.mjs';

export const meta = { duration: 24.0, poster: 23.4 };

// ── what the plugin says about itself ───────────────────────────────────
const SKILL = readRepo('.agents/plugins/nika/skills/nika-authoring/SKILL.md');
const SKILL_NAME = SKILL.match(/^name:\s*(\S+)\s*$/m)?.[1];
// the description's first sentence, as the agent reads it when it picks a skill
const SKILL_LINE = SKILL.match(/^description:\s*(.+?\.)(?:\s|$)/m)?.[1];
const VERBS = ['Author,', 'check', 'repair'];
if (SKILL_NAME !== 'nika-authoring' || !SKILL_LINE || !SKILL_LINE.startsWith('Author, check and repair ')) {
  throw new Error('the plugin kit no longer describes nika-authoring as the clip quotes it');
}
const CLIENTS = repoLines('.agents/plugins/nika/CHANGELOG.md').find(l => /^The bundle every marketplace installs \(.+\)\.$/.test(l))?.match(/\((.+)\)/)?.[1];
if (!CLIENTS) throw new Error('the plugin kit no longer names the clients it installs into');

// ── the file, its two verdicts and the rehearsal ────────────────────────
const draft = repoLines('scripts/media/fixtures/release-notes-draft.nika');
const fixed = repoLines('scripts/media/fixtures/release-notes.nika');
const checkDraft = repoLines('media/raw/agent-plugin-check-draft.txt');
const checkFixed = repoLines('media/raw/agent-plugin-check-fixed.txt');
const run = repoLines('media/raw/agent-plugin-run.txt');
const files = repoLines('media/raw/agent-plugin-files.txt');
const exits = JSON.parse(readRepo('media/raw/agent-plugin-exits.json'));

const FINDING = checkDraft.find(l => /^ ✖ TOOLS /u.test(l));
const GUESS = FINDING?.match(/`(nika:\w+)`/)?.[1];
const TASK = FINDING?.match(/\(task `(\w+)`\)/)?.[1];
const FINDINGS = checkDraft.filter(l => /^ ✖ (?!findings above)/u.test(l)).length;
const LAYERS = checkFixed.find(l => /^ layers · .*run ready ✔/u.test(l));
// the repair: every line the diff touches swaps the guessed name for the real one
const DIFF = lineDiff(draft, fixed);
const GONE = DIFF.filter(o => o.op === '-'), CAME = DIFF.filter(o => o.op === '+');
const REAL = CAME.find(o => /^\s+tool:/.test(o.text))?.text.match(/"(nika:\w+)"/)?.[1];
const TOOL_LINE = draft.findIndex(l => /^\s+tool:/.test(l) && l.includes(`"${GUESS}"`)) + 1;
const taskAt = n => { for (let i = n - 1; i >= 0; i--) { const m = draft[i].match(/^ {2}(\w+):\s*$/); if (m) return m[1]; } return null; };
if (!FINDING || !GUESS || FINDINGS !== 1 || exits.check_draft !== 2 || !checkFixed.some(l => /^ ✔ TOOLS /u.test(l)) || !LAYERS || exits.check_fixed !== 0
  || !REAL || !GONE.length || GONE.length !== CAME.length || !GONE.every((o, i) => o.text.replaceAll(GUESS, REAL) === CAME[i].text)
  || !TOOL_LINE || taskAt(TOOL_LINE) !== TASK) {
  throw new Error('the agent-plugin captures no longer tell the story: one guessed tool refused, the same file repaired and passed');
}
const TASKS = fixed.slice(fixed.indexOf('tasks:') + 1).map(l => l.match(/^ {2}(\w+):\s*$/)?.[1]).filter(Boolean);
const TASK_LINE = Object.fromEntries(TASKS.map(id => [id, fixed.indexOf(`  ${id}:`) + 1]));
// each task's own row of the run, as the CLI printed it
const RAN = Object.fromEntries(run.map(l => l.match(/^\s+✔\s+(\w+)\s{2,}(.+?)\s*$/u)).filter(Boolean).map(m => [m[1], m[2]]));
const DONE = run.find(l => /\d+\/\d+ done/u.test(l))?.replace(/[─\s]+$/u, '').trim();
const ECHOED = run.find(l => /^\s+rehearsal · /u.test(l))?.trim();
const WROTE = Object.fromEntries(run.map(l => l.match(/^\s+wrote (\S+)(?: \((\d+B)\))?\s*$/u)).filter(Boolean).map(m => [m[1], m[2] ?? '']));
if (exits.run !== 0 || !/^rehearsal: mock\/echo/u.test(run[0]) || !DONE?.includes(`${TASKS.length}/${TASKS.length} done`) || !ECHOED
  || !TASKS.every(id => RAN[id]) || !('./release-notes.md' in WROTE) || !('.nika/traces' in WROTE)
  || !['CHANGELOG.md', 'release-notes.md', 'release-notes.nika', '.nika'].every(f => files.includes(f))) {
  throw new Error('the agent-plugin rehearsal no longer runs every task of the fixed file');
}
// the file the agent writes, as the capture left it in the project
const NAME = files.find(f => /^[\w-]+\.nika$/.test(f));
if (NAME !== 'release-notes.nika') throw new Error('the project the rehearsal left no longer holds release-notes.nika');
// the commands the capture ran (less its --color never)
const CMD_CHECK = `nika check ${NAME} --native-strict`;
const CMD_RUN = `nika run ${NAME} --model mock/echo`;

// ── timeline (seconds) ──────────────────────────────────────────────────
const T = {
  title: 0.2, session: 0.3,
  ask: 0.7, askEvery: 0.07,
  skill: 2.0,
  titleOut: 5.45,
  dock: 5.25, condense: 5.62, s2: 6.25,
  author: 6.3, file: 6.45, lines: 6.7, every: 0.024,
  check1: 7.9, cmd1: 8.05, scan1: 8.75, found: 9.5,
  s2b: 10.9,
  repair: 12.95, morph: 13.1, morphEnd: 13.8,
  s2back: 14.9,
  check2: 14.6, cmd2: 14.75, scan2: 15.45, passed: 16.15,
  run: 16.7, cmd3: 16.85, tasks: 17.5, taskEvery: 0.3,
  wide: 21.8,
  clauses: 21.2, clauseEvery: 0.3,
  keep: 21.85, land: 22.45,
};
T.authorDone = T.lines + draft.length * T.every + 0.15;
T.hit = T.scan1 + 0.7 * (TOOL_LINE - 0.5) / draft.length; // the scan reaches the guessed tool
T.ran = T.tasks + TASKS.length * T.taskEvery + 0.1;

// ── layout (world = the 1920×1080 frame at rest) ────────────────────────
const SESSION = { x: 64, y: 244, w: 680, h: 780 };
const FILE = { x: 772, y: 244, w: 640, h: 780 };
const PROJECT = { x: 1440, y: 244, w: 416, h: 780 };
const CODE = { f: 'MM 400', size: 17 };
const LH = 20;
const codeY = n => FILE.y + 44 + 32 + (n - 1) * LH; // baseline of line n
const colX = c => FILE.x + 70 + c * cw(CODE);
const GUESS_COL = draft[TOOL_LINE - 1].indexOf(GUESS);

const ICON_X = 104, KIND_X = 134;
const KIND = { f: 'MGW 500', size: 17, tracking: 2.4 };
const BODY = { f: 'MM 400', size: 18 };
const INLINE_X = KIND_X + measure('REHEARSE', KIND) + 22;
const ROW = 27, GAP = 15;
const OUT_CELLS = Math.floor((SESSION.x + SESSION.w - 24 - KIND_X) / cw(BODY));

// Break a CLI line into rows that fit, word by word; where a row must
// break, it breaks after the line's own ` · ` separator if that keeps the
// row at least half full. Only the breaks change.
function wrapDots(raw, max) {
  const rows = [];
  let cur = '';
  for (const w of raw.split(' ')) {
    const next = cur ? `${cur} ${w}` : w;
    if (!cur || next.length <= max) { cur = next; continue; }
    const dot = cur.lastIndexOf(' ·');
    if (dot >= max / 2 && dot + 2 < cur.length) {
      rows.push(cur.slice(0, dot + 2));
      cur = `${cur.slice(dot + 3)} ${w}`;
    } else {
      rows.push(cur);
      cur = w;
    }
  }
  rows.push(cur);
  return rows;
}
// rows keep the colours of the whole line: each row is coloured as a slice of it
function outRows(raw, marks) {
  const rows = wrapDots(raw, OUT_CELLS);
  const spans = cliSpans(raw, marks);
  let pos = 0;
  return rows.map(r => {
    const a = raw.indexOf(r, pos), b = a + r.length;
    pos = b;
    const row = [];
    let p = 0;
    for (const sp of spans) {
      const s0 = Math.max(a, p), s1 = Math.min(b, p + sp.s.length);
      if (s1 > s0) row.push({ ...sp, s: raw.slice(s0, s1) });
      p += sp.s.length;
    }
    return row;
  });
}

// the request (the person's words: illustration), large while it is read,
// then one line at the head of the session
const ASK = 'Every Friday, turn CHANGELOG.md into release notes.';
const ASK_BIG = { f: 'Geist 500', size: 23 }, ASK_SMALL = { f: 'Geist 500', size: 20 };
const BUBBLE = { x: SESSION.x + 24, y: SESSION.y + 58, w: SESSION.w - 48 };
function layoutWords(s, st, x0, y0, maxW, lh) {
  const out = [];
  const space = measure(' ', st);
  let x = x0, y = y0;
  for (const w of s.split(' ')) {
    const ww = measure(w, st);
    if (x > x0 && x + ww > x0 + maxW) { x = x0; y += lh; }
    out.push({ w, x, y, ww });
    x += ww + space;
  }
  return out;
}
// one line in both states, so it condenses as one block and no word crosses another
const ASK_A = layoutWords(ASK, ASK_BIG, BUBBLE.x + 24, BUBBLE.y + 72, 9999, 0);
const ASK_B = layoutWords(ASK, ASK_SMALL, BUBBLE.x + 88, BUBBLE.y + 34, 9999, 0);
if (measure(ASK, ASK_BIG) > BUBBLE.w - 48) throw new Error('the ask no longer fits its bubble on one line');
const BUBBLE_H = [98, 52];

// the skill card, open while the agent reads it, then docked in the header
const CARD = { x: SESSION.x + 24, y: BUBBLE.y + BUBBLE_H[0] + 22, w: SESSION.w - 48 };
const DESC_ST = { f: 'Geist 400', size: 24 };
const DESC_WORDS = layoutWords(SKILL_LINE, DESC_ST, KIND_X + 2, CARD.y + 128, CARD.w - 80, 34);
CARD.h = DESC_WORDS[DESC_WORDS.length - 1].y - CARD.y + 30;
const CHIP = `SKILL · ${SKILL_NAME.toUpperCase()}`;
const CHIP_W = measure(CHIP, { f: 'MGW 500', size: 11, tracking: 2.5 }) + 24;
const CHIP_BOX = { x: SESSION.x + SESSION.w - 18 - CHIP_W, y: SESSION.y + 10, w: CHIP_W, h: 24 };

// the agent's loop: a header row, then its command and what came back
const STEPS = [
  { id: 'author', kind: 'AUTHOR', icon: 'write', t: T.author, done: T.authorDone, inline: `${NAME} · ${draft.length} lines`, ok: true },
  { id: 'check1', kind: 'CHECK', icon: 'check', t: T.check1, done: T.found, cmd: CMD_CHECK, cmdT: T.cmd1, ok: false, pill: [`EXIT ${exits.check_draft}`, C.red],
    out: outRows(FINDING.trim(), [{ re: new RegExp(`\`${GUESS}\``), c: C.red }, { re: /`nika catalog --tools`/, c: C.teal }]), outT: T.found },
  { id: 'repair', kind: 'REPAIR', icon: 'repair', t: T.repair, done: T.morphEnd, inline: `${GUESS} → ${REAL}`, ok: true },
  { id: 'check2', kind: 'CHECK', icon: 'check', t: T.check2, done: T.passed, cmd: CMD_CHECK, cmdT: T.cmd2, ok: true, pill: [`EXIT ${exits.check_fixed}`, C.teal],
    out: outRows(LAYERS.trim(), [{ re: /run ready ✔/u, c: C.teal, glow: 0.7 }]), outT: T.passed },
  { id: 'run', kind: 'REHEARSE', icon: 'run', t: T.run, done: T.ran, cmd: CMD_RUN, cmdT: T.cmd3, ok: true, pill: ['MOCK/ECHO', C.amber],
    out: [...outRows(DONE, [{ re: /\d+\/\d+ done/u, c: C.teal }]), ...outRows(ECHOED, [{ re: /^rehearsal/u, c: C.amber }])], outT: T.ran },
];
{
  let y = BUBBLE.y + BUBBLE_H[1] + 44;
  for (const s of STEPS) {
    s.y = y;
    s.cmdY = y + ROW;
    s.outY = y + ROW * (s.cmd ? 2 : 1);
    y += ROW * (1 + (s.cmd ? 1 : 0) + (s.out ? s.out.length : 0)) + GAP;
  }
}
const stepOf = id => STEPS.find(s => s.id === id);
const LAST = STEPS[STEPS.length - 1];
const LAST_ROW = LAST.outY + (LAST.out.length - 1) * ROW;

// ── camera ──────────────────────────────────────────────────────────────
// Close on the ask and the skill; the session beside the file while the
// agent works; closer on the refusal and the name it refuses, where the
// repair lands; back for the re-check and the rehearsal; then the room.
// the close-up keeps the title's band clear: the session's top edge at y 250
const S1 = { x: SESSION.x + SESSION.w / 2, y: SESSION.y + (540 - 250) / 1.85, s: 1.85 };
const S2 = frameBox({ x: SESSION.x, y: SESSION.y, w: FILE.x + FILE.w - SESSION.x, h: Math.max(LAST_ROW, codeY(draft.length)) + 14 - SESSION.y }, 12);
const c1 = stepOf('check1'), rp = stepOf('repair');
// the refusal and the name it refuses, side by side: the step's icon to the file's token
const S2B = { x: (ICON_X - 28 + colX(GUESS_COL + GUESS.length) + 24) / 2, y: (c1.y - 30 + rp.y + 12) / 2, s: 1.72 };
const SHOTS = [
  { at: 0, cam: S1 },
  { at: T.s2, cam: S2, move: 0.65 },
  { at: T.s2b, cam: S2B, move: 0.6 },
  { at: T.s2back, cam: S2, move: 0.6 },
  { at: T.wide, cam: WIDE, move: 0.8 },
];
export const camera = t => cameraPath(t, SHOTS);

// The panels stop above the plate band: in every settled shot their bottom
// edge rests at FOOT on screen, and between shots it travels with the camera.
const FOOT = 996;
const footFor = cam => cam.y + (FOOT - 540) / cam.s;
// where the file card's clip (4 px above its edge) would cut a code line,
// the edge rises to the gap above that line instead
const onGrid = y => {
  for (let n = 2; n <= draft.length; n++) {
    if (codeY(n) - 14 < y - 4 && codeY(n) + 5 > y - 4) return codeY(n - 1) + 9;
  }
  return y;
};
// (only in the shots where the file card is on screen)
const footOf = sh => (sh.at >= T.file ? onGrid(footFor(sh.cam)) : footFor(sh.cam));
const FOOTS = SHOTS.map(sh => ({ at: sh.at, move: sh.move, cam: { x: 0, y: footOf(sh), s: 1 } }));
const bottomAt = t => cameraPath(t, FOOTS).y;
const sized = (b, t) => ({ ...b, h: bottomAt(t) - b.y });
// each shot still holds what it shows above that edge
const [F1, F2, F2B] = FOOTS.map(f => f.cam.y);
if (CARD.y + CARD.h + 24 > F1 || codeY(draft.length) + 10 > F2 || LAST_ROW + 10 > F2 || rp.y + 10 > F2B || codeY(TOOL_LINE) + 10 > F2B) {
  throw new Error('a shot no longer holds its content above the plate band');
}

export function env() {
  return { bgGlow: 1, gridAlpha: 0.22, gridX: 0, gridY: 0, bgY: 560 };
}

// ── chrome: the frame, the opening title and the closing three clauses ──
const CLAUSES = [
  { s: 'Your agent writes it.', c: C.ice, box: SESSION, tag: `with the plugin's ${SKILL_NAME} skill` },
  { s: 'Nika checks it.', c: C.teal, box: FILE, tag: `nika check · ${FINDINGS} finding repaired · run ready` },
  { s: 'You keep it.', c: C.human, box: PROJECT, tag: 'a file in your project, run it again' },
];
const CLAUSE_ST = { f: 'Geist 600', size: 52, tracking: -52 * 0.028 };

export function chrome(R, t) {
  const a = loopFade(t, meta.duration);
  const cam = camera(t);
  // while the camera is in, the world fades out under the kicker and the
  // plate: the background colour, nearly opaque at the frame's edge,
  // transparent 110 px in
  const sc = smooth(1.02, 1.15, cam.s) * a;
  if (sc > 0 && !R.glowPass) {
    const ctx = R.ctx;
    for (const [edge, inner] of [[0, 110], [DH, DH - 110]]) {
      const g = ctx.createLinearGradient(0, edge, 0, inner);
      g.addColorStop(0, rgba(C.bg0, 0.96 * sc));
      g.addColorStop(0.35, rgba(C.bg0, 0.82 * sc));
      g.addColorStop(1, rgba(C.bg0, 0));
      ctx.fillStyle = g;
      ctx.fillRect(0, Math.min(edge, inner), DW, 110);
    }
  }
  frame(R, t, { kicker: 'for coding agents · the nika plugin', plate: `agent session, project and scan: illustration · skill: the plugin kit · check and run captured from nika ${NIKA_VERSION} · the run is a mock/echo rehearsal`, alpha: a, scrim: 0 });
  headline({ ...R, fade: a }, t, T.title, T.titleOut, 'Your coding agent, fluent in Nika.', `ONE PLUGIN · ${CLIENTS.toUpperCase()}`, { accent: 'fluent in Nika.', accentColor: C.teal });
  CLAUSES.forEach((cl, i) => {
    const t0 = T.clauses + i * T.clauseEvery;
    const k = E.snap(seg(t, t0, t0 + 0.6));
    if (k <= 0) return;
    text(R, cl.s, cl.box.x, 176 + 16 * (1 - k), { ...CLAUSE_ST, color: cl.c, alpha: a * k, glow: 0.3, blur: (1 - k) * 5 });
    text(R, cl.tag.toUpperCase(), cl.box.x + 2, 208, { f: 'MGW 500', size: 10.5, tracking: 3, color: cl.c, alpha: a * 0.85 * smooth(t0 + 0.2, t0 + 0.6, t) });
  });
}

// ── the session ─────────────────────────────────────────────────────────
function icon(R, kind, x, y, col, alpha) {
  const S = { color: col, w: 1.7, alpha, glow: 0.4 };
  if (kind === 'skill') diamond(R, x, y, 6.5, { fill: col, alpha, glow: 0.6 });
  else if (kind === 'write') {
    poly(R, [[x - 5, y - 7], [x + 2, y - 7], [x + 6, y - 3], [x + 6, y + 7], [x - 5, y + 7], [x - 5, y - 7]], S);
    line(R, x - 2, y, x + 3, y, S);
    line(R, x - 2, y + 3.5, x + 3, y + 3.5, S);
  } else if (kind === 'check') {
    circle(R, x - 1.5, y - 1.5, 5, S);
    line(R, x + 2, y + 2, x + 6.5, y + 6.5, { ...S, w: 2.2 });
  } else if (kind === 'repair') {
    line(R, x - 5, y - 3, x + 5, y - 3, S);
    line(R, x, y - 8, x, y + 2, S);
    line(R, x - 5, y + 6, x + 5, y + 6, S);
  } else if (kind === 'run') poly(R, [[x - 4, y - 6.5], [x + 6.5, y], [x - 4, y + 6.5], [x - 4, y - 6.5]], { ...S, fill: col, fillAlpha: 0.25 });
}

function stepRow(R, t, s, i, a) {
  const k = E.snap(seg(t, s.t, s.t + 0.45));
  if (k <= 0) return;
  const y = s.y;
  // the rail from the step before: a pulse hands the work down
  if (i > 0) {
    const prev = STEPS[i - 1];
    const y0 = prev.y + 10, y1 = y - 22;
    const p = E.inOutCubic(seg(t, s.t - 0.3, s.t + 0.05));
    line(R, ICON_X, y0, ICON_X, lerp(y0, y1, p), { color: C.faint, w: 1.4, alpha: a });
    if (p > 0 && p < 1) circle(R, ICON_X, lerp(y0, y1, p), 3.2, { fill: C.ice, alpha: a, glow: 1 });
  }
  const running = t < s.done;
  const col = running ? C.ice : s.ok ? C.teal : C.red;
  circle(R, ICON_X, y - 6, 15, { color: C.faint, w: 1.2, alpha: a * k, fill: '#050c19', fillAlpha: 0.95 });
  if (running) arc(R, ICON_X, y - 6, 15, (t - s.t) * 6, (t - s.t) * 6 + Math.PI * 1.2, { color: C.ice, w: 1.8, alpha: a * k, glow: 0.8 });
  else circle(R, ICON_X, y - 6, 15, { color: col, w: 1.6, alpha: a * smooth(s.done, s.done + 0.25, t), glow: 0.15 + 0.5 * (1 - smooth(s.done + 0.3, s.done + 1.2, t)) });
  icon(R, s.icon, ICON_X, y - 6, running ? C.mist : col, a * k);
  // the landing flash when a step settles
  const fl = 1 - smooth(s.done, s.done + 0.5, t);
  if (!running && fl > 0) circle(R, ICON_X, y - 6, 15 + 14 * (1 - fl), { color: col, w: 1.2, alpha: a * fl * 0.8, glow: 0.8 });
  text(R, s.kind, KIND_X + 10 * (1 - k), y, { ...KIND, color: C.ice, alpha: a * k, glow: 0.15 });
  if (s.inline) mono(R, [{ s: s.inline, c: C.ink }], INLINE_X + 10 * (1 - k), y, { alpha: a * k, st: BODY });
  if (s.pill && !running) pill(R, SESSION.x + SESSION.w - 20, y - 6, s.pill[0], s.pill[1], a * smooth(s.done, s.done + 0.3, t), 'right');
  // the command, typed
  if (s.cmd && t >= s.cmdT) {
    const n = Math.floor(s.cmd.length * clamp((t - s.cmdT) / 0.6));
    mono(R, [{ s: s.cmd.slice(0, n), c: C.ink, b: true }], KIND_X, s.cmdY, { alpha: a, st: BODY });
    if (t < s.cmdT + 0.9) {
      const blink = Math.floor(t * 2.4) % 2 === 0 ? 1 : 0.25;
      rect(R, KIND_X + n * cw(BODY) + 2, s.cmdY - BODY.size + 2, 2.5, BODY.size + 3, { fill: C.ice, alpha: a * blink, glow: 0.8 });
    }
  }
  // what came back
  if (s.out) {
    // a refusal the repair answered stays readable, quieter
    const old = s.id === 'check1' ? 1 - 0.45 * smooth(T.morphEnd, T.morphEnd + 0.5, t) : 1;
    s.out.forEach((spans, r) => {
      const rk = smooth(s.outT + r * 0.06, s.outT + r * 0.06 + 0.2, t);
      if (rk > 0) mono(R, spans, KIND_X, s.outY + r * ROW + 5 * (1 - rk), { alpha: a * rk * old, st: BODY, max: OUT_CELLS });
    });
  }
}

function skillCard(R, t, a) {
  const k = E.snap(seg(t, T.skill, T.skill + 0.5));
  if (k <= 0) return;
  // the card folds shut; its chip lifts out of it and docks in the
  // session's header, where the skill stays active for the whole loop
  const fold = E.inOutCubic(seg(t, T.dock, T.dock + 0.32));
  const lift = E.inOutCubic(seg(t, T.dock + 0.2, T.dock + 0.62));
  const click = smooth(0.85, 1, lift) * (1 - smooth(T.dock + 0.62, T.dock + 1.2, t));
  const h = CARD.h * k * (1 - fold);
  if (fold < 1) {
    rrect(R, CARD.x, CARD.y, CARD.w, Math.max(0, h), 14, { color: C.ice, w: 1.2, alpha: a * 0.7 * (1 - fold), fill: '#071427', fillAlpha: 0.95, glow: 0.35 * (1 - smooth(T.skill + 0.5, T.skill + 1.5, t)) });
    // what the card holds folds away with it
    const ctx = R.ctx;
    ctx.save();
    ctx.beginPath();
    ctx.rect(CARD.x, CARD.y, CARD.w, Math.max(0, h));
    ctx.clip();
    skillContent(R, t, a * (1 - smooth(0, 0.6, fold)), k);
    ctx.restore();
  }
  if (fold > 0) {
    const cy = lerp(CARD.y + 8, CHIP_BOX.y, lift), cx = lerp(CARD.x + CARD.w - CHIP_BOX.w - 16, CHIP_BOX.x, lift);
    const ck = smooth(0, 0.4, fold);
    if (lift > 0 && lift < 1) light(R, cx + CHIP_BOX.w / 2, cy + 12, 70, C.ice, 0.18, 1);
    rrect(R, cx, cy, CHIP_BOX.w, CHIP_BOX.h, 12, { color: C.ice, w: 1.2, alpha: a * ck, fill: '#071427', fillAlpha: 0.95, glow: 0.35 + 0.5 * click });
    text(R, CHIP, cx + 12, cy + 16.5, { f: 'MGW 500', size: 11, tracking: 2.5, color: C.ice, alpha: a * ck, glow: 0.3 + 0.5 * click });
    if (click > 0) light(R, CHIP_BOX.x + CHIP_BOX.w / 2, CHIP_BOX.y + 12, 110, C.ice, 0.22 * click, 1);
  }
}

function skillContent(R, t, ca, k) {
  if (ca <= 0) return;
  // a light passes the card as the agent takes it in
  const sw = seg(t, T.skill + 0.25, T.skill + 1.05);
  if (sw > 0 && sw < 1) {
    const x = lerp(CARD.x, CARD.x + CARD.w, E.inOutCubic(sw));
    line(R, x, CARD.y + 6, x - 40, CARD.y + CARD.h * k - 6, { color: C.ice, w: 2, alpha: ca * 0.5 * Math.sin(Math.PI * sw), glow: 1 });
  }
  if (k < 1) return;
  const tk = smooth(T.skill + 0.2, T.skill + 0.45, t);
  icon(R, 'skill', CARD.x + 26, CARD.y + 38, C.ice, ca * tk);
  text(R, 'SKILL · FROM THE NIKA PLUGIN', KIND_X, CARD.y + 44, { f: 'MGW 500', size: 13, tracking: 3, color: C.ice, alpha: ca * tk });
  // the name types in; the description arrives word by word
  const name = SKILL_NAME.slice(0, Math.ceil(SKILL_NAME.length * clamp((t - T.skill - 0.35) / 0.45)));
  text(R, name, KIND_X, CARD.y + 88, { f: 'MM 500', size: 32, color: C.ink, alpha: ca, glow: 0.2 });
  DESC_WORDS.forEach((wd, i) => {
    const t0 = T.skill + 0.85 + i * 0.045;
    const wk = E.snap(seg(t, t0, t0 + 0.4));
    if (wk <= 0) return;
    const verb = VERBS.indexOf(wd.w);
    const lit = verb >= 0 ? smooth(T.skill + 1.45 + verb * 0.25, T.skill + 1.75 + verb * 0.25, t) : 0;
    text(R, wd.w, wd.x, wd.y + 8 * (1 - wk), { ...DESC_ST, color: verb >= 0 ? (lit > 0.5 ? C.ice : C.ink) : C.mist, alpha: ca * wk, glow: 0.35 * lit, blur: (1 - wk) * 5 });
    if (lit > 0) {
      const ww = measure(wd.w.replace(/,$/, ''), DESC_ST);
      line(R, wd.x, wd.y + 8, wd.x + ww * E.snap(lit), wd.y + 8, { color: C.ice, w: 2, alpha: ca, glow: 0.6 });
    }
  });
}

function session(R, t, a) {
  const pk = E.snap(seg(t, T.session, T.session + 0.5));
  const plug = smooth(T.session + 0.3, T.session + 0.6, t) * (1 - smooth(T.dock + 0.1, T.dock + 0.3, t));
  panel(R, sized(SESSION, t), { title: 'agent session · illustration', alpha: a, k: pk, badge: plug > 0 ? { label: 'NIKA PLUGIN', color: C.ice, alpha: plug } : null });
  if (pk < 1) return;
  // the ask: large while it is read, then one line beside YOU as one block
  const m = E.inOutCubic(seg(t, T.condense, T.condense + 0.5));
  const bk = E.snap(seg(t, T.ask - 0.1, T.ask + 0.3));
  if (bk > 0) {
    rrect(R, BUBBLE.x, BUBBLE.y + 8 * (1 - bk), BUBBLE.w, lerp(BUBBLE_H[0], BUBBLE_H[1], m), 18, { color: C.dim, w: 1.2, alpha: a * bk, fill: '#0b1628', fillAlpha: 0.92 });
    const ly = lerp(BUBBLE.y + 32, BUBBLE.y + 33, m);
    text(R, 'YOU', BUBBLE.x + 24, ly, { f: 'MGW 500', size: 13, tracking: 3, color: C.human, alpha: a * bk });
    const to = a * bk * (1 - smooth(0, 0.35, m));
    if (to > 0) {
      poly(R, [[BUBBLE.x + 76, ly - 5], [BUBBLE.x + 96, ly - 5]], { color: C.dim, w: 1.3, alpha: to });
      poly(R, [[BUBBLE.x + 91, ly - 9.5], [BUBBLE.x + 96, ly - 5], [BUBBLE.x + 91, ly - 0.5]], { color: C.dim, w: 1.3, alpha: to });
      text(R, 'YOUR CODING AGENT', BUBBLE.x + 106, ly, { f: 'MGW 500', size: 13, tracking: 3, color: C.dim, alpha: to });
    }
    ASK_A.forEach((wa, i) => {
      const t0 = T.ask + i * T.askEvery;
      const wk = E.snap(seg(t, t0, t0 + 0.42));
      if (wk <= 0) return;
      const wb = ASK_B[i];
      const size = lerp(ASK_BIG.size, ASK_SMALL.size, m);
      text(R, wa.w, lerp(wa.x, wb.x, m), lerp(wa.y, wb.y, m) + 12 * (1 - wk), { f: 'Geist 500', size, color: C.human, alpha: a * wk, blur: (1 - wk) * 7 });
    });
  }
  // the agent at work before it answers: three beats, then the skill
  const wa = smooth(T.ask + 1.0, T.ask + 1.2, t) * (1 - smooth(T.skill - 0.1, T.skill + 0.1, t));
  if (wa > 0) {
    for (let i = 0; i < 3; i++) {
      const pulse = 0.35 + 0.65 * Math.max(0, Math.sin((t - T.ask) * 7 - i * 0.9));
      circle(R, KIND_X + 6 + i * 16, CARD.y + 34, 4, { fill: C.ice, alpha: a * wa * pulse, glow: 0.6 });
    }
  }
  skillCard(R, t, a);
  STEPS.forEach((s, i) => stepRow(R, t, s, i, a));
}

// ── the file ────────────────────────────────────────────────────────────
function scan(R, t, t0, col, a) {
  const p = seg(t, t0, t0 + 0.7);
  if (p <= 0 || p >= 1) return;
  const y = lerp(codeY(1) - 22, codeY(draft.length) + 10, E.inOutSine(p));
  const fade = Math.sin(Math.PI * p);
  if (!R.glowPass) {
    const ctx = R.ctx;
    const g = ctx.createLinearGradient(0, y - 90, 0, y);
    const [r, gg, b] = col === C.teal ? [60, 227, 180] : [159, 208, 255];
    g.addColorStop(0, `rgba(${r},${gg},${b},0)`);
    g.addColorStop(1, `rgba(${r},${gg},${b},${(0.1 * fade * a).toFixed(4)})`);
    ctx.fillStyle = g;
    ctx.fillRect(FILE.x + 6, y - 90, FILE.w - 12, 90);
  }
  line(R, FILE.x + 6, y, FILE.x + FILE.w - 6, y, { color: col, w: 2, alpha: a * fade, glow: 1 });
  // the scan's tab says what it stands for
  const st = { f: 'MGW 500', size: 10, tracking: 2.5 };
  const tw = measure('NIKA CHECK', st) + 20;
  rrect(R, FILE.x + FILE.w - 14 - tw, y - 20, tw, 20, 4, { color: col, w: 1, alpha: a * fade, fill: '#050c19', fillAlpha: 0.96 });
  text(R, 'NIKA CHECK', FILE.x + FILE.w - 14 - tw + 10, y - 6.5, { ...st, color: col, alpha: a * fade, glow: 0.4 });
}

// A light traced along a card's border: once red on the refusal, once teal
// on the clean re-check, which stays as the card's edge.
function borderPts(b, r = 14) {
  const pts = [];
  const arcPts = (cx, cy, a0) => { for (let i = 0; i <= 6; i++) { const an = a0 + (i / 6) * Math.PI / 2; pts.push([cx + Math.cos(an) * r, cy + Math.sin(an) * r]); } };
  arcPts(b.x + b.w - r, b.y + r, -Math.PI / 2);
  arcPts(b.x + b.w - r, b.y + b.h - r, 0);
  arcPts(b.x + r, b.y + b.h - r, Math.PI / 2);
  arcPts(b.x + r, b.y + r, Math.PI);
  pts.push(pts[0]);
  return pts;
}
function trace(R, t, t0, col, a, keep) {
  const p = E.inOutCubic(seg(t, t0, t0 + 0.9));
  if (p <= 0) return;
  const edge = borderPts(sized(FILE, t));
  const head = 1 - smooth(t0 + 0.9, t0 + 1.4, t);
  if (keep) poly(R, edge, { color: col, w: 1.4, alpha: a * 0.55, glow: 0.3 }, p);
  // the bright head of the stroke
  if (head > 0) poly(R, edge, { color: col, w: 2.4, alpha: a * head, glow: 1 }, p, Math.max(0, p - 0.12));
}

// a wave that flattens as it heals: red while refused, teal once repaired
function wave(R, x0, x1, y, amp, color, alpha, t) {
  if (alpha <= 0) return;
  const pts = [];
  for (let x = x0; x <= x1; x += 2) pts.push([x, y + Math.sin((x - x0) * 0.55 + t * 6) * amp]);
  poly(R, pts, { color, w: 1.7, alpha, glow: 0.5 });
}

function file(R, t, a) {
  const fk = E.snap(seg(t, T.file, T.file + 0.45));
  if (fk <= 0) return;
  const repaired = t >= T.morph;
  const ready = smooth(T.passed, T.passed + 0.3, t);
  const refused = smooth(T.found, T.found + 0.3, t);
  const badge = ready > 0 ? { label: 'NIKA CHECK · RUN READY', color: C.teal, alpha: ready }
    : repaired ? { label: 'REPAIRED · A REAL DIFF', color: C.teal, alpha: smooth(T.morph, T.morph + 0.3, t) }
    : refused > 0 ? { label: `${FINDINGS} FINDING · EXIT ${exits.check_draft}`, color: C.red, alpha: refused } : null;
  codeCard(R, t, sized(FILE, t), {
    title: NAME, alpha: a, k: fk, badge,
    before: draft, after: fixed,
    reveal: { t0: T.lines - T.every, every: T.every },
    morph: { t0: T.morph, t1: T.morphEnd },
    st: CODE, lh: LH,
    marks: [
      { line: TOOL_LINE, re: new RegExp(GUESS), c: C.red, t0: T.hit, t1: T.morph + 0.3, version: 'before' },
      ...GONE.filter(o => o.a + 1 !== TOOL_LINE).map(o => ({ line: o.a + 1, re: new RegExp(GUESS), c: C.red, t0: T.found, t1: T.morph + 0.3, version: 'before' })),
      ...CAME.map(o => ({ line: o.b + 1, re: new RegExp(`${REAL}(?!\\w)`), c: C.teal, t0: T.morphEnd - 0.3, version: 'after' })),
    ],
    highlights: TASKS.flatMap((id, i) => {
      const t0 = T.tasks + i * T.taskEvery;
      return [
        { line: TASK_LINE[id], t0, t1: t0 + T.taskEvery * 0.8, c: C.cyan, running: true },
        { line: TASK_LINE[id], t0: t0 + T.taskEvery * 0.8, c: C.teal },
      ];
    }),
  });
  if (fk < 1) return;
  // the write head: the agent's file arrives line by line
  const wn = Math.floor((t - T.lines) / T.every) + 1;
  if (wn >= 1 && wn <= draft.length) {
    const x = colX(draft[wn - 1].length) + 3, y = codeY(wn);
    rect(R, x, y - 15, 9, 19, { fill: C.ice, alpha: a * 0.9, glow: 1 });
    light(R, x + 4, y - 6, 60, C.ice, 0.18 * a, 1);
  }
  // the audit reads the file (illustration): once, once more after the repair
  scan(R, t, T.scan1, C.ice, a);
  scan(R, t, T.scan2, C.teal, a);
  trace(R, t, T.found, C.red, a, false);
  trace(R, t, T.passed, C.teal, a, true);
  // the refusal, pinned to the name it refuses; the wave heals with the repair
  const hit = smooth(T.hit, T.hit + 0.25, t);
  if (hit > 0) {
    const y = codeY(TOOL_LINE);
    const x0 = colX(GUESS_COL), x1 = colX(GUESS_COL + (repaired ? REAL.length : GUESS.length));
    const lock = E.snap(seg(t, T.hit, T.hit + 0.35));
    const heal = E.snap(seg(t, T.morphEnd - 0.15, T.morphEnd + 0.45));
    const gone = 1 - smooth(T.morphEnd + 0.5, T.morphEnd + 1.0, t);
    const redA = 1 - smooth(T.morph, T.morph + 0.25, t);
    // the old name folds away with its line; the new one arrives, then flattens
    // the wave sits in the gap under its line, clear of the line below
    if (redA > 0) wave(R, x0, colX(GUESS_COL + GUESS.length), y + 4, 2, C.red, a * hit * redA, t);
    if (t >= T.morphEnd - 0.15) wave(R, x0, x1, y + 4, 2 * (1 - heal), C.teal, a * gone * smooth(T.morphEnd - 0.15, T.morphEnd, t), t);
    const grow = lerp(1.6, 1, lock);
    const bw = (colX(GUESS_COL + GUESS.length) - x0 + 12) * grow, bh = 30 * grow;
    const cx = (x0 + colX(GUESS_COL + GUESS.length)) / 2;
    brackets(R, cx - bw / 2, y - 7 - bh / 2, bw, bh, 7, { color: redA > 0 ? C.red : C.teal, w: 1.4, alpha: a * hit * (redA > 0 ? 1 : gone), glow: 0.6 });
    if (lock < 1) light(R, cx, y - 7, 90, C.red, 0.3 * (1 - lock), 1);
  }
  // the rehearsal flows task to task; each task's own row of the run beside it
  TASKS.forEach((id, i) => {
    const done = T.tasks + i * T.taskEvery + T.taskEvery * 0.8;
    if (i + 1 < TASKS.length) {
      const y0 = codeY(TASK_LINE[id]) - 6, y1 = codeY(TASK_LINE[TASKS[i + 1]]) - 6;
      const p = E.inOutCubic(seg(t, done, done + T.taskEvery * 0.6));
      if (p > 0) {
        line(R, FILE.x + 16, y0, FILE.x + 16, lerp(y0, y1, p), { color: C.teal, w: 1.5, alpha: a * 0.8, glow: 0.5 });
        if (p < 1) circle(R, FILE.x + 16, lerp(y0, y1, p), 3.2, { fill: C.teal, alpha: a, glow: 1 });
      }
    }
    const rk = smooth(done, done + 0.25, t);
    if (rk > 0) mono(R, [{ s: RAN[id], c: C.teal }], colX(`  ${id}: `.length + 2), codeY(TASK_LINE[id]), { alpha: a * rk, st: { f: 'MM 400', size: 15 } });
  });
}

// the finding, pinned: from the session's refusal to the name in the file
function leader(R, t, a) {
  const c = stepOf('check1');
  const p = E.inOutCubic(seg(t, T.found + 0.1, T.found + 0.5));
  const k = p * (1 - smooth(T.morph, T.morph + 0.35, t));
  if (k <= 0) return;
  const x0 = SESSION.x + SESSION.w - 14, y0 = c.outY - 6;
  const x1 = colX(GUESS_COL) - 14, y1 = codeY(TOOL_LINE) - 7;
  const pts = bezierPts([x0, y0], [x0 + 50, y0], [x1 - 70, y1], [x1, y1], 36);
  const RR = { ...R, fade: a * k };
  circle(RR, x0, y0, 3.5, { fill: C.red, glow: 1 });
  poly(RR, pts.slice(0, Math.max(2, Math.round(pts.length * p))), { color: C.red, w: 1.6, alpha: 0.9, glow: 0.9 });
  // a pulse runs it once it is whole
  const q = seg(t, T.found + 0.5, T.found + 1.1);
  if (q > 0 && q < 1) {
    const pt = pts[Math.round((pts.length - 1) * E.inOutSine(q))];
    circle(RR, pt[0], pt[1], 3.4, { fill: C.red, glow: 1 });
  }
}

// the write: a beam from the step to the file it creates
function beam(R, t, a) {
  const s = stepOf('author');
  const p = E.inOutCubic(seg(t, T.author + 0.15, T.author + 0.55));
  const k = p * (1 - smooth(T.authorDone, T.authorDone + 0.4, t));
  if (k <= 0) return;
  const x0 = INLINE_X + measure(s.inline, BODY) + 12, y0 = s.y - 6;
  const x1 = FILE.x + 14, y1 = FILE.y + 22;
  const pts = bezierPts([x0, y0], [x0 + 40, y0], [x1 - 50, y1], [x1, y1], 30);
  poly({ ...R, fade: a * k }, pts.slice(0, Math.max(2, Math.round(pts.length * p))), { color: C.ice, w: 1.5, alpha: 0.85, glow: 0.8 });
  const q = pts[Math.min(pts.length - 1, Math.round((pts.length - 1) * p))];
  circle({ ...R, fade: a * k }, q[0], q[1], 3.5, { fill: C.ice, glow: 1 });
}

// ── the project ─────────────────────────────────────────────────────────
const TREE_ST = { f: 'MM 500', size: 22 };
const NOTE = { f: 'MGW 500', size: 11, tracking: 1.8 };
const readsFrom = fixed.find(l => /^\s+read:/.test(l))?.match(/"\.\/([^"]+)"/)?.[1];
const TREE = files.map(f => {
  if (f === '.nika') return { name: '.nika/', note: 'the rehearsal\'s trace' };
  if (f === readsFrom) return { name: f, note: 'what the job reads' };
  if (`./${f}` in WROTE) return { name: f, note: `written by the rehearsal · ${WROTE[`./${f}`]}` };
  if (f === NAME) return { name: f, note: 'checked · the file you keep', kept: true };
  return { name: f, note: '' };
});
const treeY = i => PROJECT.y + 150 + i * 68;
const KEPT_I = TREE.findIndex(r => r.kept);
const NEXT_Y = treeY(TREE.length - 1) + 74;

function project(R, t, a) {
  // revealed as the camera pulls back: the room the file is kept in
  const pk = E.snap(seg(t, T.wide - 0.75, T.wide - 0.2));
  if (pk <= 0) return;
  panel(R, sized(PROJECT, t), { title: 'your project', alpha: a, k: pk, badge: t > T.land ? { label: 'KEPT', color: C.human, alpha: smooth(T.land, T.land + 0.3, t) } : null });
  if (pk < 1) return;
  text(R, './', PROJECT.x + 24, PROJECT.y + 98, { ...TREE_ST, color: C.dim, alpha: a });
  const x = PROJECT.x + 34;
  TREE.forEach((r, i) => {
    const k = E.snap(seg(t, T.wide - 0.2 + i * 0.07, T.wide + 0.2 + i * 0.07));
    if (k <= 0) return;
    const y = treeY(i);
    const last = i === TREE.length - 1;
    line(R, x, y - 40, x, last ? y - 8 : y + 28, { color: C.faint, w: 1.2, alpha: a * k });
    line(R, x, y - 8, x + 18, y - 8, { color: C.faint, w: 1.2, alpha: a * k });
    const kept = r.kept ? smooth(T.land, T.land + 0.3, t) : 0;
    if (kept > 0) {
      rect(R, PROJECT.x + 8, y - 34, PROJECT.w - 16, 58, { fill: C.human, alpha: a * 0.07 * kept });
      rect(R, PROJECT.x + 8, y - 34, 3, 58, { fill: C.human, alpha: a * kept, glow: 0.8 });
    }
    text(R, r.name, x + 30 + 8 * (1 - k), y, { ...TREE_ST, color: kept > 0 ? C.human : r.name.endsWith('/') ? C.mist : C.ink, alpha: a * k, glow: 0.3 * kept });
    text(R, r.note.toUpperCase(), x + 32 + 8 * (1 - k), y + 22, { ...NOTE, color: kept > 0 ? C.human : C.dim, alpha: a * k * 0.95 });
  });
  // what the person runs from now on: the file, with its own model
  const fk = E.snap(seg(t, T.land + 0.15, T.land + 0.6));
  if (fk > 0) {
    rrect(R, PROJECT.x + 16, NEXT_Y, PROJECT.w - 32, 92, 12, { color: C.faint, w: 1, alpha: a * fk, fill: '#071427', fillAlpha: 0.9 });
    text(R, 'NEXT FRIDAY, AND EVERY ONE AFTER', PROJECT.x + 34, NEXT_Y + 32, { f: 'MGW 500', size: 11, tracking: 2.5, color: C.dim, alpha: a * fk });
    text(R, `nika run ${NAME}`, PROJECT.x + 34, NEXT_Y + 68, { f: 'MM 500', size: 19, color: C.teal, alpha: a * fk, glow: 0.2 });
  }
}

// the keeping: the checked file travels into the project
function keep(R, t, a) {
  const p = seg(t, T.keep, T.land);
  if (p <= 0 || KEPT_I < 0) return;
  const x0 = FILE.x + 110, y0 = FILE.y + 22;
  // it lands where the tree's branch meets the file's name
  const x1 = PROJECT.x + 56, y1 = treeY(KEPT_I) - 8;
  const pts = bezierPts([x0, y0], [x0 + 220, y0 - 150], [x1 - 120, y1 - 200], [x1, y1], 48);
  const q = E.inOutCubic(p);
  if (p < 1) {
    const i = Math.round((pts.length - 1) * q);
    const tail = Math.max(0, Math.round((pts.length - 1) * (q - 0.3)));
    poly({ ...R, fade: a }, pts.slice(tail, i + 1), { color: C.human, w: 2, alpha: 0.8, glow: 0.9 });
    // the checked file itself travels: its check and its name
    const [hx, hy] = pts[i];
    const st = { f: 'MM 500', size: 14 };
    const w = measure(NAME, st) + 46;
    const ta = a * smooth(0, 0.12, p) * (1 - smooth(0.9, 1, p));
    rrect(R, hx - 16, hy - 15, w, 30, 15, { color: C.human, w: 1.4, alpha: ta, fill: '#0b1628', fillAlpha: 0.95, glow: 0.7 });
    check(R, hx, hy, 12, 1, { color: C.teal, w: 2, alpha: ta, glow: 0.6 });
    text(R, NAME, hx + 16, hy + 5, { ...st, color: C.human, alpha: ta });
  }
  const land = 1 - smooth(T.land, T.land + 0.7, t);
  if (t >= T.land && land > 0) {
    circle(R, x1, y1, 8 + 40 * (1 - land), { color: C.human, w: 1.4, alpha: a * land, glow: 0.9 });
    light(R, x1, y1, 120, C.human, 0.2 * land, 1);
  }
}

// each clause lights the top edge of the column it names
function edges(R, t, a) {
  CLAUSES.forEach((cl, i) => {
    const t0 = T.clauses + i * T.clauseEvery + 0.2;
    const k = E.snap(seg(t, t0, t0 + 0.7));
    if (k <= 0) return;
    const b = cl.box;
    line(R, b.x + 14, b.y, b.x + 14 + (b.w - 28) * k, b.y, { color: cl.c, w: 2, alpha: a * 0.9, glow: 0.8 });
  });
}

export function draw(R, t) {
  const a = loopFade(t, meta.duration);
  session(R, t, a);
  file(R, t, a);
  project(R, t, a);
  beam(R, t, a);
  leader(R, t, a);
  keep(R, t, a);
  edges(R, t, a);
}
