import { Nika, isNikaRunSucceeded } from '@supernovae-st/nika';

const nika = new Nika({ cwd: process.cwd() });

// The engine checks the file first: a workflow it refuses throws here and never starts.
const run = await nika.run<{ greeting: string }>('hello.nika', { maxCostUsd: 0 });
const result = await run.result();

// A failed run is data you read, not an exception.
if (!isNikaRunSucceeded(result) || !result.receipt) {
  console.error(result.status, result.error?.code, result.error?.message);
  process.exit(1);
}
console.log(result.outputs?.greeting);

// Ask the engine to verify the run's tamper-evident record.
const proof = await nika.traceVerify(result.receipt);
console.log(proof.verified ? 'receipt verified' : `not verified: ${proof.reason}`);
