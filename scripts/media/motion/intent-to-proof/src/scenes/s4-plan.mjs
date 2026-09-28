// S4 · SEMANTIC PLAN — the hero.
//
// The plan forms where three streams meet: the obligations descend from
// intent into the slots of the Foundry block; observed evidence rises from
// below into its sockets. The template dissolves — what remains is meaning,
// grounded above (what was said) and below (what was observed). The camera
// orbits so the three strata separate in depth, joined by provenance.
import { C, E, TAU, clamp, lerp, seg, smooth, win } from '../engine/core.mjs';
import { text, line, poly, circle, rrect, measure, check, light } from '../engine/render.mjs';
import { orbit, project, REF } from '../engine/cam.mjs';
import { T } from '../timeline.mjs';
import { ATOMS, atomX, OP, ARG, KIND, OP_Y, ARG_Y, KIND_Y } from './s1-intent.mjs';
import { drawBlock, BLOCK, ROW } from './s3-propose.mjs';
import { fieldAnchor, FILE_ANCHOR, CAP_ANCHOR } from './s2-observe.mjs';

export const NODES = [
  { op: 'READ', kind: 'SOURCE', detail: 'invoices.json', badge: 'file · JSON · 4 records', clause: 0 },
  { op: 'FILTER', kind: 'RULE', detail: 'status ≠ "rejected"', badge: 'Enum ∋ "rejected"', clause: 1 },
  { op: 'GROUP', kind: 'KEY', detail: 'customer_id', badge: 'String', clause: 2 },
  { op: 'SUM', kind: 'AGGREGATE', detail: 'amount_cents', badge: 'Integer · cents', unit: true, clause: 2 },
  { op: 'GATE', kind: 'HUMAN', detail: 'human approval', badge: 'before any effect', human: true, clause: 3 },
  { op: 'PAY', kind: 'EFFECT', detail: 'POST payments', badge: 'effect · 1 · irreversible', clause: 3 },
];
const CLAUSES = ['Read my invoices.', 'Ignore rejected ones.', 'Sum by customer.', 'Pay only after I approve.'];
const CLAUSE_NODES = [[0], [1], [2, 3], [4, 5]];
// socket k fills node SOCKET_NODE[k] at T.sockets[k] with the observed token
const SOCKET_NODE = [0, 1, 2, 3, 5];
const TOKENS = ['invoices.json', 'status', 'customer_id', 'amount_cents', 'payments.example.invalid'];
const ANCHOR = [() => FILE_ANCHOR(), () => fieldAnchor(1), () => fieldAnchor(0), () => fieldAnchor(2), () => CAP_ANCHOR()];
const EVIDENCE = ['invoices.json', 'status', 'customer_id', 'amount_cents', null, 'nika:fetch · permit'];

const OPS = { f: 'Geist 600', size: 40, tracking: -0.8 };
const DETAIL = { f: 'MM 500', size: 14 };
const BADGE = { f: 'MM 400', size: 11 };
const STRATA = { intentY: 290, evidY: 880, z: 420 };

export const nodeX = i => atomX(i);
export function socketFill(t, i) {
  const k = SOCKET_NODE.indexOf(i);
  if (k < 0) return 0;
  return smooth(T.sockets[k] - 0.02, T.sockets[k] + 0.12, t);
}
// When the currency unknown resolves (set by the proof scene's answer).
export function currencyResolved(t) {
  return smooth(T.returned, T.returned + 0.25, t);
}

export function camAt(t) {
  const u = E.inOutSine(seg(t, T.alive, T.amberSeed + 0.2));
  const back = E.inOutCubic(seg(t, T.toProof - 0.15, T.ring + 0.1));
  const swing = u * (1 - back);
  const yaw = 0.52 * swing;
  const pitch = 0.2 * swing;
  const dist = REF + 150 - 60 * swing;
  const tx = 960 + 120 * swing;
  return orbit([tx, 540, 150], dist, yaw, pitch);
}

const P = (cam, x, y, z = 0) => project(cam, [x, y, z]);
function t3(R, cam, x, y, z, str, st) {
  const p = P(cam, x, y, z);
  if (!p.ok) return null;
  text(R, str, p.x, p.y, { ...st, size: st.size * p.s });
  return p;
}
function line3(R, cam, a, b, st) {
  const p = P(cam, ...a), q = P(cam, ...b);
  if (!p.ok || !q.ok) return;
  line(R, p.x, p.y, q.x, q.y, st);
}
function curve3(R, cam, a, b, st, prog = 1, lift = 0) {
  const pts = [];
  for (let i = 0; i <= 24; i++) {
    const u = i / 24;
    const x = lerp(a[0], b[0], u), y = lerp(a[1], b[1], u), z = lerp(a[2], b[2], u);
    const bow = Math.sin(u * Math.PI) * lift;
    const p = P(cam, x, y + bow, z);
    if (p.ok) pts.push([p.x, p.y]);
  }
  poly(R, pts, st, prog);
}

// ── the plan nodes (shared with the proof scene) ───────────────────────
// opts: alpha, fills(i) → 0..1, unitState: 'unknown'|'resolved', verified(i) → 0..1
export function drawNodes(R, cam, t, o = {}) {
  const alpha = o.alpha ?? 1;
  const fills = o.fills || (i => socketFill(t, i));
  for (let i = 0; i < NODES.length; i++) {
    const n = NODES[i];
    const x = nodeX(i);
    const na = alpha * (o.nodeAlpha ? o.nodeAlpha(i) : 1);
    if (na <= 0.01) continue;
    const pOp = P(cam, x, ROW.op, 0);
    if (!pOp.ok) continue;
    const s = pOp.s;
    const col = n.human ? C.human : C.ink;
    const v = o.verified ? o.verified(i) : 0;
    t3(R, cam, x, ROW.kind, 0, `${String(i + 1).padStart(2, '0')}  ${n.kind}`, { ...KIND, size: 11, color: n.human ? C.human : C.dim, alpha: na * 0.9, align: 'center' });
    t3(R, cam, x, ROW.op, 0, n.op, { ...OPS, color: col, alpha: na, align: 'center', glow: 0.2 + 0.25 * v });
    // socket / gate glyph
    const ps = P(cam, x, ROW.socket, 0);
    const f = fills(i);
    if (n.human) {
      line(R, ps.x - 7 * s, ps.y - 10 * s, ps.x - 7 * s, ps.y + 10 * s, { color: C.human, w: 1.8 * s, alpha: na });
      line(R, ps.x + 7 * s, ps.y - 10 * s, ps.x + 7 * s, ps.y + 10 * s, { color: C.human, w: 1.8 * s, alpha: na });
    } else if (f > 0) {
      circle(R, ps.x, ps.y, 9 * s * lerp(1.6, 1, f), { color: C.teal, w: 1.2 * s, alpha: na * f * 0.7 });
      circle(R, ps.x, ps.y, 4.2 * s, { fill: C.teal, alpha: na * f, glow: 0.8 });
    } else {
      circle(R, ps.x, ps.y, 9 * s, { color: C.cyan, w: 1.2 * s, alpha: na, dash: [2.5 * s, 3 * s], dashOffset: t * 16 });
    }
    // grounded detail (the vague intent argument is replaced by evidence)
    const dA = n.human ? smooth(T.sockets[3] + 0.1, T.sockets[4] - 0.05, t) : f;
    if (dA > 0) {
      const dstr = n.detail.slice(0, Math.ceil(n.detail.length * clamp(dA * 1.25)));
      t3(R, cam, x, ROW.detail, 0, dstr, { ...DETAIL, color: n.human ? C.human : C.ink, alpha: na * dA, align: 'center' });
      const bw = measure(n.badge, BADGE) + 16;
      const pb = P(cam, x, ROW.badge, 0);
      rrect(R, pb.x - (bw * s) / 2, pb.y - 15 * s, bw * s, 21 * s, 5 * s, { color: n.human ? C.human : C.faint, w: 1 * s, alpha: na * dA * 0.9, fill: C.bg0, fillAlpha: 0.5 });
      text(R, n.badge, pb.x, pb.y, { ...BADGE, size: 11 * s, color: n.human ? C.human : C.mist, alpha: na * dA, align: 'center' });
      if (!n.human) {
        const obs = smooth(0.4, 1, dA);
        check(R, ps.x + 18 * s, ps.y - 1 * s, 9 * s, obs, { color: C.teal, w: 1.5 * s, glow: 0.5, alpha: na });
      }
    }
    // the unit of money: observed cents — of which currency?
    if (n.unit && dA > 0.5) {
      const res = o.resolved ?? currencyResolved(t);
      const pu = P(cam, x, ROW.badge + 27, 0);
      const str = res > 0.5 ? 'Money<EUR · cents>' : 'currency ?';
      const col2 = res > 0.5 ? C.teal : C.amber;
      const w = measure(str, BADGE) + 16;
      const pulse = res > 0.5 ? 1 : 0.7 + 0.3 * Math.sin(t * 8);
      rrect(R, pu.x - (w * s) / 2, pu.y - 15 * s, w * s, 21 * s, 5 * s, { color: col2, w: 1 * s, alpha: na * pulse * smooth(0.5, 1, dA), glow: 0.6 });
      text(R, str, pu.x, pu.y, { ...BADGE, size: 11 * s, color: col2, alpha: na * smooth(0.5, 1, dA), align: 'center', glow: 0.4 });
    }
    // verification mark from the proof scene
    if (v > 0) {
      const pv = P(cam, x, ROW.kind - 26, 0);
      check(R, pv.x, pv.y, 12 * s, v, { color: C.teal, w: 2 * s, glow: 0.9, alpha: na });
    }
  }
  // chain connectors
  for (let i = 0; i < NODES.length - 1; i++) {
    const xa = nodeX(i) + measure(NODES[i].op, OPS) / 2 + 16;
    const xb = nodeX(i + 1) - measure(NODES[i + 1].op, OPS) / 2 - 16;
    const pa = P(cam, xa, ROW.op - 14, 0), pb = P(cam, xb, ROW.op - 14, 0);
    if (!pa.ok || !pb.ok) continue;
    line(R, pa.x, pa.y, pb.x, pb.y, { color: C.ice, w: 1.2 * pa.s, alpha: alpha * 0.65 });
    const ang = Math.atan2(pb.y - pa.y, pb.x - pa.x), ah = 7 * pb.s;
    poly(R, [[pb.x - ah * Math.cos(ang - 0.55), pb.y - ah * Math.sin(ang - 0.55)], [pb.x, pb.y], [pb.x - ah * Math.cos(ang + 0.55), pb.y - ah * Math.sin(ang + 0.55)]], { color: C.ice, w: 1.2 * pb.s, alpha: alpha * 0.75 });
  }
}

export function env(t) {
  const swing = E.inOutSine(seg(t, T.alive, T.amberSeed + 0.2)) * (1 - E.inOutCubic(seg(t, T.toProof - 0.15, T.ring + 0.1)));
  return { bgGlow: 1.1 + 0.25 * swing, gridAlpha: 0.5 * (1 - swing * 0.6), gridX: -swing * 120, gridY: 380, bgY: 560 };
}

export function draw(R, t) {
  const cam = camAt(t);
  const endFade = t < T.ring ? 1 : 0; // the proof scene takes the nodes over on the same camera

  // ── the template block (from Foundry), dissolving as meaning takes over
  if (t >= T.plan + 0.15) {
    const dissolve = smooth(T.alive, T.alive + 0.45, t);
    if (dissolve < 1) {
      const fill = SOCKET_NODE.map(() => 0);
      const f6 = [0, 1, 2, 3, 4, 5].map(i => socketFill(t, i));
      f6.names = [0, 1, 2, 3, 4, 5].map(i => smooth(T.plan + 0.35 + i * 0.03, T.plan + 0.55 + i * 0.03, t));
      drawBlock(R, t, { ...BLOCK, p: 1, alpha: 1, fill: f6, dissolve });
    }
  }

  // ── the obligations descend from the intent dock into the slots ───────
  const landed = smooth(T.plan + 0.5, T.plan + 0.62, t);
  for (let i = 0; i < 6; i++) {
    const a = ATOMS[i];
    const p = E.snap(seg(t, T.plan + 0.02 + i * 0.035, T.plan + 0.6 + i * 0.035));
    if (landed >= 1) break;
    const s0 = 0.42, s1 = OPS.size / OP.size;
    const dockX = 960 + (atomX(i) - 960) * s0;
    const x = lerp(dockX, atomX(i), p);
    const s = lerp(s0, s1, p);
    const yOp = lerp(150, ROW.op, p);
    const al = 1 - landed;
    text(R, a.op, x, yOp, { ...OP, size: OP.size * s, color: a.human ? C.human : C.ink, alpha: al, align: 'center', glow: 0.3 * Math.sin(p * Math.PI) });
    text(R, `${String(i + 1).padStart(2, '0')}  ${a.kind}`, x, yOp + (KIND_Y - OP_Y) * s, { ...KIND, size: KIND.size * s, color: a.human ? C.human : C.dim, alpha: al * 0.9, align: 'center' });
    text(R, a.arg, x, yOp + (ARG_Y - OP_Y) * s, { ...ARG, size: ARG.size * s, color: C.mist, alpha: al * (1 - p * 0.6), align: 'center' });
  }
  if (t < T.plan + 0.6) {
    // the S3 dock label hands over
    text(R, 'INTENT', 150, lerp(116, 300, E.snap(seg(t, T.plan, T.plan + 0.5))), { f: 'MGW 500', size: 10.5, tracking: 4, color: C.human, alpha: 0.9 * (1 - seg(t, T.plan, T.plan + 0.4)) });
  }
  if (landed > 0) drawNodes(R, cam, t, { alpha: landed * endFade });

  // ── evidence rises into the sockets ───────────────────────────────────
  const threads = 1 - smooth(T.alive, T.alive + 0.4, t); // dock threads hand over to the strata
  SOCKET_NODE.forEach((ni, k) => {
    const tl = T.sockets[k], tf0 = tl - 0.42;
    if (t < tf0) return;
    const a0 = ANCHOR[k]();
    const sock = [nodeX(ni), ROW.socket];
    const p = E.inOutCubic(seg(t, tf0, tl));
    const c0 = [a0[0], a0[1] - 170], c1 = [sock[0], sock[1] + 150];
    const u = 1 - p;
    const pos = [
      u * u * u * a0[0] + 3 * u * u * p * c0[0] + 3 * u * p * p * c1[0] + p * p * p * sock[0],
      u * u * u * a0[1] + 3 * u * u * p * c0[1] + 3 * u * p * p * c1[1] + p * p * p * sock[1],
    ];
    // provenance thread from the dock to the token (persists)
    const pts = [];
    for (let j = 0; j <= 20; j++) {
      const q = (j / 20) * p, w = 1 - q;
      pts.push([
        w * w * w * a0[0] + 3 * w * w * q * c0[0] + 3 * w * q * q * c1[0] + q * q * q * sock[0],
        w * w * w * a0[1] + 3 * w * w * q * c0[1] + 3 * w * q * q * c1[1] + q * q * q * sock[1],
      ]);
    }
    poly(R, pts, { color: C.teal, w: 0.9, alpha: 0.45 * threads * endFade });
    if (p < 1) {
      text(R, TOKENS[k], pos[0], pos[1] - 12, { f: 'MM 500', size: 14, color: C.teal, alpha: 1, align: 'center', glow: 0.7 });
      circle(R, pos[0], pos[1], 3, { fill: C.teal, alpha: 1, glow: 1 });
    }
    // lock flash
    const fl = seg(t, tl, tl + 0.3);
    if (fl > 0 && fl < 1) {
      circle(R, sock[0], sock[1], 9 + 34 * E.outCubic(fl), { color: C.teal, w: 1.2, alpha: (1 - fl) * 0.8, glow: 1 });
      text(R, 'OBSERVED', sock[0] + 26, sock[1] + 4, { f: 'MGW 500', size: 9, tracking: 2, color: C.teal, alpha: 1 - fl });
    }
  });

  // ── the living plan: strata in depth, joined by provenance ────────────
  const alive = smooth(T.alive + 0.05, T.alive + 0.5, t) * (1 - smooth(T.toProof, T.toProof + 0.35, t));
  if (alive > 0) {
    // intent stratum (what was said)
    CLAUSES.forEach((c, l) => {
      const ids = CLAUSE_NODES[l];
      const cx = (nodeX(ids[0]) + nodeX(ids[ids.length - 1])) / 2;
      t3(R, cam, cx, STRATA.intentY, STRATA.z, c, { f: 'Geist 400', size: 21, color: C.human, alpha: alive * 0.85, align: 'center' });
      ids.forEach(i => curve3(R, cam, [cx, STRATA.intentY + 8, STRATA.z], [nodeX(i), ROW.kind - 16, 0], { color: C.human, w: 0.8, alpha: alive * 0.35 }, E.snap(seg(t, T.alive + 0.1, T.alive + 0.6))));
    });
    // evidence stratum (what was observed)
    EVIDENCE.forEach((ev, i) => {
      if (!ev) return;
      t3(R, cam, nodeX(i), STRATA.evidY, STRATA.z, ev, { f: 'MM 500', size: 15, color: C.teal, alpha: alive * 0.9, align: 'center' });
      curve3(R, cam, [nodeX(i), ROW.socket + 6, 0], [nodeX(i), STRATA.evidY - 22, STRATA.z], { color: C.teal, w: 0.9, alpha: alive * 0.45 }, E.snap(seg(t, T.alive + 0.05, T.alive + 0.55)));
    });
    // exploded view: three planes of meaning, labeled at their corners
    const lA = alive * smooth(T.alive + 0.2, T.alive + 0.6, t);
    const frame = (y0, y1, z, col, label) => {
      const c = [[70, y0, z], [1850, y0, z], [1850, y1, z], [70, y1, z], [70, y0, z]].map(q => P(cam, ...q));
      if (c.some(q => !q.ok)) return;
      const grow = E.snap(seg(t, T.alive + 0.05, T.alive + 0.7));
      poly(R, c.map(q => [q.x, q.y]), { color: col, w: 0.8, alpha: lA * 0.35 }, grow);
      const tl = c[0];
      text(R, label, tl.x + 10 * tl.s, tl.y - 8 * tl.s, { f: 'MGW 500', size: 10.5 * tl.s, tracking: 4, color: col, alpha: lA });
    };
    frame(STRATA.intentY - 34, STRATA.intentY + 18, STRATA.z, C.human, 'INTENT · WHAT WAS SAID');
    frame(ROW.kind - 40, ROW.badge + 44, 0, C.ice, 'PLAN · WHAT IT MEANS');
    frame(STRATA.evidY - 30, STRATA.evidY + 20, STRATA.z, C.teal, 'EVIDENCE · WHAT WAS OBSERVED');
    // a pulse travels the chain: the plan is one connected meaning
    const pu = seg(t, T.alive + 0.4, T.amberSeed);
    if (pu > 0 && pu < 1) {
      const u = E.inOutSine(pu) * 5, i = Math.min(4, Math.floor(u));
      const xx = lerp(nodeX(i), nodeX(i + 1), u - i);
      const pp = P(cam, xx, ROW.op - 14, 0);
      if (pp.ok) { circle(R, pp.x, pp.y, 3.5 * pp.s, { fill: C.cyan, alpha: alive, glow: 1 }); light(R, pp.x, pp.y, 60 * pp.s, C.cyan, 0.25 * alive, 0.6); }
    }
    // orbital structure around each node
    for (let i = 0; i < 6; i++) {
      const c = [nodeX(i), ROW.op - 14, 0];
      const pts = [];
      for (let j = 0; j <= 48; j++) {
        const an = (j / 48) * TAU;
        const p = P(cam, c[0] + Math.cos(an) * 104, c[1] + Math.sin(an) * 10, c[2] + Math.sin(an) * 104);
        if (p.ok) pts.push([p.x, p.y]);
      }
      poly(R, pts, { color: i === 3 ? C.amber : C.ice, w: 0.8, alpha: alive * (i === 3 ? 0.45 : 0.22) });
      const an = t * 1.6 + i * 1.1;
      const pd = P(cam, c[0] + Math.cos(an) * 104, c[1] + Math.sin(an) * 10, c[2] + Math.sin(an) * 104);
      if (pd.ok) circle(R, pd.x, pd.y, 2.4 * pd.s, { fill: i === 3 ? C.amber : C.cyan, alpha: alive, glow: 1 });
    }
  }

  // title: the architectural center
  const ti = win(t, T.alive + 0.1, T.toProof + 0.3, 0.35, 0.3);
  if (ti > 0) {
    const k = E.snap(seg(t, T.alive + 0.1, T.alive + 0.7));
    text(R, 'Semantic plan', 150, 204 + 14 * (1 - k), { f: 'Geist 600', size: 66, tracking: -2, color: C.ink, alpha: ti, glow: 0.2 });
    text(R, 'PRIVATE IR · WHAT NIKA BELIEVES YOU ASKED · MEANING, NOT CODE', 152, 238, { f: 'MGW 500', size: 10.5, tracking: 3, color: C.ice, alpha: ti * 0.9 });
    const stats = [['OBLIGATIONS', '6/6', C.ink], ['GROUNDED', '5/5', C.teal], ['UNKNOWN', '1', C.amber]];
    stats.forEach(([k2, v, col], j) => {
      const x = 1770 - (2 - j) * 150;
      text(R, k2, x, 188, { f: 'MGW 500', size: 9.5, tracking: 2.4, color: C.dim, alpha: ti, align: 'right' });
      text(R, v, x, 222, { f: 'Geist 300', size: 30, color: col, alpha: ti, align: 'right', glow: 0.3 });
    });
  }
  // the one amber seed
  const am = win(t, T.amberSeed, T.toProof + 0.4, 0.15, 0.3);
  if (am > 0) {
    const p = P(cam, nodeX(3), ROW.badge + 27, 0);
    const f = seg(t, T.amberSeed, T.amberSeed + 0.6);
    circle(R, p.x, p.y, 20 + 60 * E.outCubic(f), { color: C.amber, w: 1.2, alpha: am * (1 - f) * 0.9, glow: 1 });
    light(R, p.x, p.y, 120, C.amber, 0.12 * am, 0.6);
  }
}
