// Everything factual the film shows is loaded from a real source here:
// the fixture program (passes `nika check`), transcripts captured from the
// real binary, and hashes recomputed at load so they can never go stale.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { ROOT } from './engine/render.mjs';

const read = p => fs.readFileSync(path.join(ROOT, p), 'utf8');
const FIXTURE = '../../fixtures/invoice-payments.nika';
export const program = read(FIXTURE);
export const programLines = program.split('\n');
export const hashes = JSON.parse(read('captured/hashes.json'));
const sha = s => crypto.createHash('sha256').update(s).digest('hex');
if (sha(fs.readFileSync(path.join(ROOT, FIXTURE))) !== hashes.program_sha256 ||
    sha(fs.readFileSync(path.join(ROOT, 'captured/plan.json'))) !== hashes.plan_sha256) {
  throw new Error('captured/hashes.json is stale: run `python3 tools/hashes.py`');
}
export const short = (h, n = 8) => `${h.slice(0, n)}…${h.slice(-4)}`;

export const checkLines = read('captured/nika-check.txt').split('\n');
export const runLines = read('captured/nika-run-declined.txt').split('\n');
export const traceVerify = read('captured/nika-trace-verify.txt').split('\n')[0].trim();

// Pick check rows by their label.
export function checkRow(label) {
  const l = checkLines.find(x => x.trim().startsWith(`✔ ${label}`));
  if (!l) throw new Error(`check row ${label} not in the capture`);
  return l.trim();
}
// Real per-task durations from the captured run.
export function runDuration(task) {
  const l = runLines.find(x => x.includes(` ${task} `));
  const m = l && l.match(/(\d+ms)\s*$/);
  return m ? m[1] : '';
}

// Select fixture lines by content, in order; returns [{n, text}] with real
// 1-based line numbers so folds are visible and honest.
export function pickLines(patterns) {
  const out = [];
  let from = 0;
  for (const pat of patterns) {
    const i = programLines.findIndex((l, k) => k >= from && l.trimEnd() === pat.trimEnd());
    if (i < 0) throw new Error(`fixture line not found: ${pat}`);
    out.push({ n: i + 1, text: programLines[i] });
    from = i + 1;
  }
  return out;
}
