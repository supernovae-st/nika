// remote.mjs · the remote door: the same facade over HTTP, against a
// `nika serve` that capture/sdk-doors.mjs started on a free loopback port
// with a 0600 token file and its own signing key. Only the constructor
// changes. It prints one JSON document; the token is read from its file and
// never printed.
import { readFile } from 'node:fs/promises';
import { Nika, NikaOperationError } from '@supernovae-st/nika';

const url = process.argv[2];
if (!url) throw new Error('usage: node remote.mjs <url>');
const token = (await readFile('.nika/serve.token', 'utf8')).trim();
const nika = new Nika({ url, token, allowInsecureHttp: true }); // plain HTTP: loopback only
const out = { transportKind: nika.transportKind };

out.served = [...(await nika.listWorkflows())];
const report = await nika.check('hello.nika');
out.check = { clean: report.clean };

// Store the key before the call; a retry with the same key reuses the job.
const idempotencyKey = 'sdk-doors-capture-0001';
const run = await nika.run('hello.nika', { idempotencyKey });
out.job = { idShape: /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(run.id) ? 'uuid' : 'other' };
out.events = [];
for await (const event of run.events()) {
  out.events.push({ sequence: event.sequence ?? null, kind: event.kind, raw: event.raw?.kind ?? null, ...(event.status ? { status: event.status } : {}) });
}
const result = await run.result();
out.result = { status: result.status, transport: result.transport, greeting: result.outputs?.greeting };
if (result.receipt) {
  out.receipt = { keys: Object.keys(result.receipt).sort() };
  const proof = await nika.traceVerify(result.receipt);
  out.verify = { verified: proof.verified, verdict: proof.verdict ?? null, reason: proof.reason ?? null };
}

const retry = await nika.run('hello.nika', { idempotencyKey });
out.retry = { sameJob: retry.id === run.id };
await retry.result();

try {
  await new Nika({ url, token: 'not-the-token-'.repeat(3), allowInsecureHttp: true }).listWorkflows();
  out.badToken = { refused: false };
} catch (error) {
  if (!(error instanceof NikaOperationError)) throw error;
  out.badToken = { refused: true, name: error.name, status: error.status, code: error.code, operation: error.operation };
}

process.stdout.write(`${JSON.stringify(out, null, 1)}\n`);
