// ONE timing system for picture and sound.
//
// 120 BPM: a beat is 0.5 s = 30 frames at 60 fps, a bar is 2 s, and the
// 30-second film is exactly 15 bars. Every cue below is a beat position;
// the renderer reads the seconds, and `soundCues()` hands the same numbers
// to the audio score (audio/score.py via .cache/timeline.json). Nothing is
// timed twice, so the edit and the music cannot drift apart.

export const FPS = 60;
export const DURATION = 30;
export const BPM = 120;
export const BEAT = 60 / BPM;
const b = n => n * BEAT; // beats → seconds

export const T = {
  // S1 · INTENT ─ bars 1–2
  open: 0,
  lines: [b(0.5), b(1), b(1.5), b(2)], // four sentences land on 8ths
  act: b(2.6), // session classifies the message: NEW WORK
  readA: b(3), // intent reader sweep
  readB: b(4),
  fracture: b(4), // bar 2 downbeat: words lift off the sentence
  atoms: [b(5), b(5.25), b(5.5), b(5.75), b(6), b(6.25)], // six obligations land on 16ths
  chain: b(6.5),
  dive: b(7.1), // zoom into "invoices"

  // S2 · OBSERVE ─ bars 3–4
  observe: b(8),
  crystal: b(9.5),
  fields: [b(9.5), b(10), b(10.5), b(11)], // customer_id, status, amount_cents, sku lock
  currencySeed: b(10.75),
  ghost: b(11.3), // `id` is proposed — and refused: not observed
  ghostNo: b(11.75),
  rise: b(12),

  // S3 · PROPOSE ─ bars 4–5
  foundry: b(13),
  reflex: b(14.5),
  clm: b(15),
  tagPropose: b(15.5),
  jev: b(16.5), // closed choice resolves to A
  block: b(16.5),

  // S4 · PLAN (hero) ─ bars 5–7
  plan: b(18),
  sockets: [b(18.5), b(19), b(19.5), b(20), b(20.5)], // grounded evidence fills each socket
  alive: b(20.5),
  amberSeed: b(23),
  toProof: b(23.5),

  // S5 · PROVE + ASK ─ bars 7–8
  ring: b(25),
  pass: b(26), // nodes cross the detector
  checks: Array.from({ length: 10 }, (_, i) => b(26 + i * 0.25)),
  unknown: b(28.5),
  ask: b(29.5),
  answer: b(30.5),
  returned: b(31.25),
  verified: b(31.5),
  tunnel: b(31.75),

  // S6 · LOWER → CHECK → MEANING ─ bars 9–10
  lower: b(32),
  code: b(33.5),
  check: b(34),
  checkRows: Array.from({ length: 7 }, (_, i) => b(34.25 + i * 0.25)),
  closure: b(36),
  lock: b(38),
  represented: b(38),

  // S7 · READY → REVIEW → CONSENT ─ bar 11
  ready: b(39),
  stamp: b(40),
  question: b(41),
  discuss: b(41.5),
  preview: b(41.75),
  approve: b(42.75),
  consent: b(44), // bar 12 downbeat: the gate opens

  // S8 · RUN ─ bars 12–13
  descend: b(44),
  waves: [b(45.5), b(46), b(46.5), b(47), b(47.5), b(48)],
  effect: b(48), // bar 13 downbeat: exactly one effect

  // S9 · RESULT + PROOF → REVEAL → TITLE ─ bars 13–15
  result: b(48.5),
  receipt: b(49.5),
  receiptRows: Array.from({ length: 5 }, (_, i) => b(49.75 + i * 0.25)), // sixteenths, then the seal
  seal: b(51.5),
  reveal: b(52), // bar 14 downbeat
  wide: b(53.5), // the pullback lands: the whole map in view
  principles: [b(53.5), b(54), b(54.5), b(55)], // each lands as the pulse crosses its region
  collapse: b(55.5), // everything rushes to one point
  title: b(56), // bar 15 downbeat: final impact
  end: 30,
};

// Scene windows (with overlap for transitions).
export const SCENES = {
  intent: [0, T.observe + 0.3],
  observe: [T.dive + 0.2, T.alive + 0.6],
  propose: [T.rise, T.plan + 0.6],
  plan: [T.block, T.ring + 0.9],
  prove: [T.toProof + 0.1, T.lower + 0.6],
  lower: [T.tunnel, T.ready + 0.4],
  consent: [T.lock + 0.2, T.descend + 0.9],
  run: [T.consent - 0.1, T.result + 0.7],
  result: [T.effect, T.reveal + 0.8],
  reveal: [T.reveal - 0.1, T.title + 0.5],
  title: [T.title - 0.3, DURATION],
};

// The instrument rail (HUD): stage label → time it becomes active.
export const STAGES = [
  ['INTENT', 0],
  ['OBSERVE', T.observe],
  ['PROPOSE', T.foundry],
  ['PLAN', T.plan],
  ['PROVE', T.ring],
  ['LOWER', T.lower],
  ['CHECK', T.check],
  ['MEANING', T.closure],
  ['READY', T.ready],
  ['CONSENT', T.question],
  ['RUN', T.descend],
  ['PROOF', T.result],
];

// Temporal supersampling (motion blur) per time range: [t0, t1, samples].
export const MOTION_BLUR = [
  [T.fracture, T.chain + 0.2, 6],
  [T.dive - 0.05, T.observe + 0.45, 8],
  [T.observe + 0.45, T.crystal + 0.4, 4],
  [T.rise, T.foundry + 0.5, 3],
  [T.block, T.plan + 0.3, 6],
  [T.plan + 0.3, T.alive, 3],
  [T.alive, T.toProof, 2],
  [T.toProof, T.pass + 0.2, 6],
  [T.pass + 0.2, T.verified, 3],
  [T.verified, T.lower + 0.5, 8],
  [T.lower + 0.5, T.code + 0.1, 4],
  [T.closure, T.lock + 0.1, 3],
  [T.ready - 0.1, T.stamp, 3],
  [T.consent - 0.1, T.waves[0], 6],
  [T.waves[0], T.effect, 2],
  [T.effect - 0.05, T.result + 0.45, 6],
  [T.seal + 0.3, T.reveal + 0.9, 6],
  [T.reveal + 0.9, T.title - 0.35, 2],
  [T.title - 0.35, T.title + 0.4, 8],
];

export function mbSamples(t) {
  for (const [a, c, n] of MOTION_BLUR) if (t >= a && t < c) return n;
  return 1;
}

// Sound events share the cues above. Kinds are synthesized in audio/score.py.
export function soundCues() {
  const ev = [];
  const e = (t, kind, extra = {}) => ev.push({ t: +t.toFixed(4), kind, ...extra });
  e(T.open, 'impact_low');
  e(T.open, 'data_tone');
  // one soft tick per word as the human sentence arrives
  const words = [3, 3, 3, 5];
  T.lines.forEach((t0, li) => {
    for (let w = 0; w < words[li]; w++) e(t0 + w * 0.06, 'word_tick', { i: li * 5 + w });
  });
  e(T.act, 'chirp');
  e(T.readA, 'scan_sweep', { dur: T.readB - T.readA });
  e(T.fracture - 0.5, 'reverse_swell', { dur: 0.5 });
  e(T.fracture, 'shatter');
  T.atoms.forEach((t, i) => e(t, 'pluck', { step: i }));
  e(T.chain, 'chain_draw');
  e(T.dive, 'riser', { dur: T.observe - T.dive });
  e(T.observe, 'impact_mid');
  e(T.observe, 'data_storm', { dur: T.crystal - T.observe });
  T.fields.forEach((t, i) => e(t, 'lock', { step: i }));
  e(T.currencySeed, 'amber_hint');
  e(T.ghost, 'ghost');
  e(T.ghostNo, 'deny');
  e(T.rise, 'whoosh_up');
  e(T.foundry, 'shimmer', { dur: T.block - T.foundry });
  e(T.reflex, 'scan_sweep', { dur: 0.7 });
  for (let i = 0; i < 6; i++) e(T.clm + i * 0.125, 'rank_tick', { i });
  e(T.jev, 'select');
  e(T.block + 0.05, 'whoosh');
  T.sockets.forEach((t, i) => e(t, 'socket', { step: i }));
  e(T.alive, 'swell', { dur: T.toProof - T.alive + 0.5 });
  e(T.amberSeed, 'amber_hint');
  e(T.toProof, 'whoosh');
  e(T.ring, 'spin_up', { dur: T.pass - T.ring });
  T.checks.forEach((t, i) => e(t, i === 5 ? 'check_amber' : 'check', { i }));
  e(T.unknown, 'amber_divergence', { dur: T.returned - T.unknown });
  e(T.ask, 'ask');
  e(T.answer, 'human_tap');
  e(T.returned, 'resolve');
  e(T.verified, 'verified');
  e(T.tunnel, 'tunnel', { dur: T.lower + 0.6 - T.tunnel });
  e(T.lower, 'compress', { dur: T.code - T.lower });
  for (let i = 0; i < 22; i++) e(T.lower + 0.25 + i * 0.0625, 'type_click', { i });
  T.checkRows.forEach((t, i) => e(t, 'check_soft', { i }));
  e(T.closure, 'phase_lock', { dur: T.lock - T.closure });
  e(T.lock, 'lock_final');
  e(T.ready, 'print', { dur: T.stamp - T.ready });
  e(T.stamp, 'stamp');
  e(T.question, 'human_text');
  e(T.discuss, 'gate_hold');
  e(T.preview, 'soft_tick');
  e(T.approve, 'human_text');
  e(T.consent - 0.5, 'reverse_swell', { dur: 0.5 });
  e(T.consent, 'consent');
  e(T.consent, 'gate_open');
  T.waves.forEach((t, i) => e(t, i === 5 ? 'effect' : 'wave', { i }));
  e(T.result, 'result_chord');
  T.receiptRows.forEach((t, i) => e(t, 'receipt_tick', { i }));
  e(T.seal, 'seal');
  e(T.reveal, 'reveal', { dur: T.title - T.reveal });
  T.principles.forEach((t, i) => e(t, 'principle', { i }));
  e(T.title - 0.75, 'reverse_swell', { dur: 0.75 });
  e(T.title, 'impact_final');
  return ev.sort((x, y) => x.t - y.t);
}

// Music sections (bar-aligned) for the score's rhythm bed.
export const SECTIONS = [
  { id: 'intro', a: 0, b: b(8) },
  { id: 'observe', a: b(8), b: b(18) },
  { id: 'plan', a: b(18), b: b(25) },
  { id: 'prove', a: b(25), b: b(32) },
  { id: 'lower', a: b(32), b: b(39) },
  { id: 'consent', a: b(39), b: b(44) },
  { id: 'run', a: b(44), b: b(52) },
  { id: 'reveal', a: b(52), b: b(56) },
  { id: 'title', a: b(56), b: 30 },
];
