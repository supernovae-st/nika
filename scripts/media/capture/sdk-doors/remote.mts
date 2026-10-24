// remote.mts · the server door: the same facade over HTTP, against a
// `nika serve` that capture/sdk-doors.mjs started on a free loopback port
// with a 0600 token file and its own signing key. The methods are the
// quick start's; two lines take other options. The constructor names the
// server and its token (plain HTTP to a loopback host also needs
// allowInsecureHttp), and run() takes the idempotencyKey a server requires
// where the quick start passes maxCostUsd, which a server run refuses (the
// server applies its own ceiling). It also records those three refusals.
// It prints one JSON document; the token is read from its file and never
// printed.
import { readFile } from 'node:fs/promises';
import { Nika, NikaCompatibilityError, NikaConfigurationError, NikaOperationError } from '@supernovae-st/nika';

const url = process.argv[2];
if (!url) throw new Error('usage: node remote.mts <url>');
const token = (await readFile('.nika/serve.token', 'utf8')).trim();
const nika = new Nika({ url, token, allowInsecureHttp: true }); // plain HTTP: loopback only
const out: Record<string, unknown> = { transportKind: nika.transportKind };

out.served = [...(await nika.listWorkflows())];
const report = await nika.check('hello.nika');
out.check = { clean: report.clean };

// Store the key before the call; a retry with the same key reuses the job.
const idempotencyKey = 'sdk-doors-capture-0001';
const run = await nika.run<{ greeting: string }>('hello.nika', { idempotencyKey });
out.job = { idShape: /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(run.id) ? 'uuid' : 'other' };
const events: unknown[] = [];
for await (const event of run.events()) {
  events.push({ sequence: event.sequence ?? null, kind: event.kind, raw: event.raw?.kind ?? null, ...(event.status ? { status: event.status } : {}) });
}
out.events = events;
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

// What the server door refuses of the quick start, before any request:
// its constructor without allowInsecureHttp on plain HTTP, its maxCostUsd,
// and a run without a key. None of them reaches the server.
const refusal = async (attempt: () => unknown) => {
  try {
    await attempt();
    return { refused: false };
  } catch (error) {
    if (!(error instanceof NikaConfigurationError || error instanceof NikaCompatibilityError)) throw error;
    return { refused: true, name: error.name, message: error.message };
  }
};
out.refusals = {
  plainHttp: await refusal(() => new Nika({ url, token })),
  maxCostUsd: await refusal(() => nika.run('hello.nika', { idempotencyKey: 'sdk-doors-capture-0002', maxCostUsd: 0 })),
  noKey: await refusal(() => nika.run('hello.nika')),
};

process.stdout.write(`${JSON.stringify(out, null, 1)}\n`);
