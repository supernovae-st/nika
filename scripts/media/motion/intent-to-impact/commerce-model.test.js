import test from 'node:test';
import assert from 'node:assert/strict';
import { TASKS, DEFINITIONS, EDGES, BATCHES, FIXTURE, calculateEvidence, WORLD } from './commerce-model.js';
import { YAML_EXCERPTS, renderYaml } from './commerce-yaml.js';
import { CHAPTERS, FILM, FILE_REVIEW, editTime, CUTS } from './film-timing.js';

test('20 definitions expand to 26 instances, with three bounded batches', () => {
  assert.equal(DEFINITIONS.length, 20);
  assert.equal(TASKS.length, 26);
  assert.equal(new Set(TASKS.map(t => t.id)).size, TASKS.length);
  for (const b of BATCHES) { assert.equal(b.ids.length, 3); assert.equal(b.maxParallel, 3); }
});
test('all visible nodes fit and have separate bounds', () => {
  for (const [i, a] of TASKS.entries()) {
    assert.ok(a.x >= 0 && a.y >= 0 && a.x + a.width <= WORLD.width && a.y + a.height <= WORLD.height, a.id);
    for (const b of TASKS.slice(i + 1)) {
      assert.ok(a.x + a.width <= b.x || b.x + b.width <= a.x || a.y + a.height <= b.y || b.y + b.height <= a.y, `${a.id} overlaps ${b.id}`);
    }
  }
});
test('edges resolve and the graph is acyclic', () => {
  const ids = new Set(TASKS.map(t => t.id)), visited = new Set(), active = new Set();
  for (const e of EDGES) { assert.ok(ids.has(e.from)); assert.ok(ids.has(e.to)); }
  function visit(id) { assert.ok(!active.has(id), `cycle at ${id}`); if (visited.has(id)) return; active.add(id); EDGES.filter(e=>e.from===id).forEach(e=>visit(e.to)); active.delete(id); visited.add(id); }
  TASKS.forEach(t=>visit(t.id));
});
test('all writes remain downstream of the approval gate', () => {
  const reachable = new Set();
  const walk = id => { reachable.add(id); EDGES.filter(e=>e.from===id).forEach(e=>{if(!reachable.has(e.to)) walk(e.to);}); };
  walk('approve');
  for (const id of ['brief','issue','telegram','slack','update']) assert.ok(reachable.has(id));
});
test('the deterministic evidence calculation preserves unknowns', () => {
  assert.deepEqual(calculateEvidence(), {abandonment:25,confirmed:2,unknown:1,reviewCandidate:true});
  assert.equal(calculateEvidence({...FIXTURE, abandoned:48}).reviewCandidate, false);
  assert.equal(calculateEvidence({...FIXTURE, competitors:FIXTURE.competitors.map(c=>({...c,guest:-1}))}).reviewCandidate, false);
});
test('notifications wait for both saved evidence and the issue link', () => {
  assert.ok(EDGES.some(e => e.from === 'brief' && e.to === 'issue'));
  for (const id of ['telegram', 'slack', 'update']) {
    assert.deepEqual(EDGES.filter(e => e.to === id).map(e => e.from), ['issue']);
  }
});
test('the source keeps YAML indentation and literal binding expressions', () => {
  assert.match(YAML_EXCERPTS.pages_a, /^  pages:\n    for_each:\n      items: \$\{\{ const\.competitors \}\}/);
  assert.match(YAML_EXCERPTS.parse, /input: \$\{\{ with\.csv \}\}/);
  const lines = renderYaml(YAML_EXCERPTS.csv);
  assert.equal((lines.match(/class="yaml-source-line /g) || []).length, 5);
  assert.match(lines, /        <b>path:<\/b>/);
});
test('the reviewed concurrency is the same bound used by the graph', () => {
  assert.match(YAML_EXCERPTS.pages_a, /max_parallel: 3\n/);
  for (const batch of BATCHES) assert.equal(batch.maxParallel, 3);
});
test('review precedes check, run and results on the shared film clock', () => {
  assert.ok(editTime(FILE_REVIEW.at + 8 + FILE_REVIEW.duration) < FILM.check);
  assert.ok(FILM.check < FILM.run && FILM.run < FILM.result && FILM.result < FILM.resultFrame);
  for (const [i, [, time]] of CHAPTERS.entries()) {
    assert.ok(time < FILM.duration);
    if (i > 0) assert.ok(time > CHAPTERS[i - 1][1]);
  }
});
test('the edit is exactly one minute with continuous, forward-only source time', () => {
  assert.equal(FILM.duration, 60);
  assert.deepEqual(CUTS.at(-1), [104, 60]);
  CUTS.slice(1).forEach(([source, output], i) => {
    assert.ok(source > CUTS[i][0] && output > CUTS[i][1]);
    assert.equal(editTime(source), output);
  });
  assert.equal(editTime(41.5) - editTime(33.5), 2.5);
});
