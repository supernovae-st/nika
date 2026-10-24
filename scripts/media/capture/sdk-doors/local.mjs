// local.mjs · the local door, as an app drives it: new Nika({ cwd }) and the
// engine the package bundles, as a child process. capture/sdk-doors.mjs runs
// it in a scratch app with a scratch HOME and a minted signing key. It prints
// one JSON document of what the package and the engine returned; no path of
// this machine is printed (the trace is named relative to the app).
import fs from 'node:fs/promises';
import path from 'node:path';
import { Nika, NikaOperationError, isNikaRunSucceeded } from '@supernovae-st/nika';

const nika = new Nika({ cwd: process.cwd() });
const traces = async () => (await fs.readdir('.nika/traces').catch(() => [])).filter(f => f.endsWith('.ndjson')).sort();
const out = { transportKind: nika.transportKind };

// 1 · check(): the audit alone. Nothing runs and no model is called.
const clean = await nika.check('hello.nika');
out.check = { clean: clean.clean, exitCode: clean.exitCode };
const red = await nika.check('list-files.nika');
out.checkRed = {
  clean: red.clean,
  exitCode: red.exitCode,
  errors: (red.findings ?? []).filter(f => f.severity === 'error').map(f => ({ code: f.code, message: f.message, task: f.task })),
};

// 2 · run() checks first: a refused file never becomes a run.
const before = await traces();
try {
  await nika.run('list-files.nika', { maxCostUsd: 0 });
  out.refused = { refused: false };
} catch (error) {
  if (!(error instanceof NikaOperationError)) throw error;
  out.refused = { refused: true, name: error.name, operation: error.operation, code: error.code, status: error.status, transport: error.transport };
}
out.refused.tracesWritten = (await traces()).length - before.length;

// 3 · run(): admitted, observed, settled.
const run = await nika.run('hello.nika', { maxCostUsd: 0 });
out.events = [];
for await (const event of run.events()) {
  out.events.push({ kind: event.kind, raw: event.raw?.kind ?? null, ...(event.task ? { task: event.task } : {}), ...(event.status ? { status: event.status } : {}) });
}
const result = await run.result();
if (!isNikaRunSucceeded(result) || !result.receipt) throw new Error(`the quick-start run did not succeed: ${result.status}`);
out.result = {
  status: result.status,
  transport: result.transport,
  greeting: result.outputs?.greeting,
  cause: result.settlement?.cause ?? null,
  tasks: result.settlement?.tasks ? { ok: result.settlement.tasks.ok, total: result.settlement.tasks.total } : null,
};
const receipt = result.receipt;
out.receipt = {
  keys: Object.keys(receipt).sort(),
  sealed: receipt.sealed,
  chain_len: receipt.chain_len,
  chain_head: receipt.chain_head,
  trace: path.relative(process.cwd(), String(receipt.trace_path)),
};

// 4 · traceVerify(): the engine re-reads the trace the receipt names.
const proof = await nika.traceVerify(receipt);
out.verify = { verified: proof.verified, verdict: proof.verdict ?? null, reason: proof.reason ?? null, first: (proof.output ?? '').split('\n')[0] };

// The journal as a chain: each line's kind, the `chain` it carries and the
// sha256 of its own bytes. No payloads, ids or timestamps.
const { createHash } = await import('node:crypto');
const sha = s => createHash('sha256').update(s, 'utf8').digest('hex');
const tracePath = String(receipt.trace_path);
const lines = (await fs.readFile(tracePath, 'utf8')).split('\n').filter(l => l.trim());
out.journal = lines.map(l => { const e = JSON.parse(l); return { kind: e.kind, chain: e.chain, hash: sha(l) }; });

// 5 · one changed byte: line 4 of this scratch trace, then the same
// traceVerify() on the receipt the app kept.
const raw = (await fs.readFile(tracePath, 'utf8')).split('\n');
const target = 3;
const from = 'greeting', to = 'greetinG';
if (!raw[target].includes(from)) throw new Error(`line ${target + 1} has no ${from}`);
const column = raw[target].indexOf(from) + from.length;
raw[target] = raw[target].replace(from, to);
await fs.writeFile(tracePath, raw.join('\n'));
const broken = await nika.traceVerify(receipt);
out.tamper = {
  line: target + 1,
  kind: JSON.parse(raw[target]).kind,
  column,
  from,
  to,
  verified: broken.verified,
  verdict: broken.verdict ?? null,
  reason: broken.reason ?? null,
  first: (broken.output ?? '').split('\n')[0],
};

process.stdout.write(`${JSON.stringify(out, null, 1)}\n`);
