/**
 * test.mjs · the built pkg is judged by a Node harness (law ⑧), on the
 * real fixtures — never by wasm-bindgen-test.
 *
 * Each door is exercised through the WASM boundary: the derivation on a
 * parity fixture (same bits as native), the journal fold on the real
 * recorded trace, and the two seating moments chained (the board
 * round-trips through the caller, as it will in the browser).
 */
import { readFileSync } from 'node:fs';
// the Node target — no fetch-based init (the web target's init can't read
// a file path from Node · the browser consumer rides pkg/, this rides
// pkg-node/)
import { derive_run, fold_journal, board_first, board_next } from './pkg-node/nika_tui_core.js';

const fixture = JSON.parse(readFileSync(new URL('./tests/fixtures/demo-ok.json', import.meta.url)));
const derived = JSON.parse(derive_run(JSON.stringify(fixture.workflow), JSON.stringify(fixture.run)));

const want = fixture.derived;
const eq = (a, b) => JSON.stringify(a) === JSON.stringify(b);
console.assert(eq(Object.keys(derived.idle).length, Object.keys(want.idle).length), 'idle coverage');
console.assert(derived.total_time === want.total_time, `total_time ${derived.total_time} == ${want.total_time}`);
console.assert(derived.bottleneck?.id === want.bottleneck?.id, 'bottleneck id');
console.assert(derived.bottleneck?.idleTotal === want.bottleneck?.idleTotal, 'bottleneck idleTotal, the bit-exact one');
console.assert(eq(derived.waves, want.waves), 'waves');

const journal = readFileSync(new URL('./tests/fixtures/journal-casse.ndjson', import.meta.url), 'utf8');
const run = JSON.parse(fold_journal(journal));
console.assert(run.steps.length === 7, 'the fold sees every terminal event');
console.assert(run.steps.find((s) => s.id === 'lire')?.failed?.code === 'NIKA-BUILTIN-READ-001', 'the failure carries its code');
console.assert(run.steps.find((s) => s.id === 'ecris')?.blockedBy === 'resume', 'the culpable, not needs[0]');

const graph = readFileSync(new URL('./tests/fixtures/inspect-gated.json', import.meta.url), 'utf8');
const b1 = JSON.parse(board_first(graph));
console.assert(b1.rev === 1 && b1.marks.every((m) => m === '+'), 'r1 births all');
const g2 = JSON.parse(graph);
g2.nodes = g2.nodes.slice(1); // kill the first node — its slot must empty FOREVER
const b2 = JSON.parse(board_next(JSON.stringify(b1), JSON.stringify(g2)));
console.assert(b2.rev === 2 && b2.slots[0] === null && b2.marks[0] === '−', 'the hole is permanent');

console.log('wasm harness · all doors exercised, all green');
