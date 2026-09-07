// Fictional, inspectable demo data. No network or customer account is accessed.
export const FIXTURE = Object.freeze({
  started: 480, abandoned: 120, threshold: 20,
  feedback: '“Why do I need an account just to place an order?”',
  requirement: 'Guest purchases must keep order tracking and receipts.',
  competitors: [
    { id: 'a', name: 'Shop A', url: 'shop-a.example/checkout', guest: 1, quote: 'Continue as a guest' },
    { id: 'b', name: 'Shop B', url: 'shop-b.example/help', guest: 1, quote: 'No account needed' },
    { id: 'c', name: 'Shop C', url: 'shop-c.example/checkout', guest: -1, quote: 'Sign in to continue', note: 'Guest route not established by this page' },
  ],
});

export function calculateEvidence(data = FIXTURE) {
  const abandonment = data.started ? data.abandoned / data.started * 100 : 0;
  const confirmed = data.competitors.filter(c => c.guest === 1).length;
  const unknown = data.competitors.filter(c => c.guest === -1).length;
  return { abandonment, confirmed, unknown, reviewCandidate: abandonment >= data.threshold && confirmed >= 2 };
}

const task = (id, title, detail, icon, kind, x, y, code, extra = {}) =>
  ({ id, logical: id, title, detail, icon, kind, x, y, width: 160, height: 64, code, ...extra });

export const TASKS = [
  task('feedback', 'Customer words', 'nika:read · feedback.md', 'text', 'tool', 20, 100, 'invoke: { tool: nika:read }'),
  task('themes', 'Find friction', 'infer · 400 output tokens', 'infer', 'ai', 226, 100, 'infer: { max_tokens: 400 }'),
  task('csv', 'Checkout data', 'nika:read · sales.csv', 'csv', 'tool', 20, 224, 'invoke: { tool: nika:read }'),
  task('parse', 'Parse the CSV', 'nika:convert · csv → json', 'csv', 'fixed', 226, 224, 'invoke: { tool: nika:convert }'),
  task('metrics', 'Drop-off rate', 'nika:jq · 120 ÷ 480', 'calculator', 'fixed', 432, 224, 'invoke: { tool: nika:jq }'),
  task('notes', 'Product notes', 'nika:grep · product.md', 'markdown', 'tool', 20, 348, 'invoke: { tool: nika:grep }'),
  task('linear', 'Existing work', 'Linear · SHOP-42', 'linear', 'tool', 20, 472, 'invoke: { tool: "mcp:linear/get_issue" }'),
  ...FIXTURE.competitors.map((c, i) => task(`pages_${c.id}`, `Read ${c.name}`, 'nika:fetch · article', 'web', 'tool', 450, 402 + i * 84, 'invoke: { tool: nika:fetch }', { logical: 'pages', iteration: i, fan: true })),
  ...FIXTURE.competitors.map((c, i) => task(`facts_${c.id}`, 'Extract evidence', 'infer · schema · 384 tokens', 'infer', 'ai', 688, 402 + i * 84, 'infer: { max_tokens: 384, schema: … }', { logical: 'facts', iteration: i, fan: true })),
  ...FIXTURE.competitors.map((c, i) => task(`shape_${c.id}`, 'Check the shape', 'nika:assert · typed fields', 'check', 'fixed', 926, 402 + i * 84, 'invoke: { tool: nika:assert }', { logical: 'shape', iteration: i, fan: true })),
  task('rule', 'Apply your rule', 'nika:jq · explicit threshold', 'calculator', 'fixed', 1144, 224, 'invoke: { tool: nika:jq }'),
  task('compare', 'Join evidence', 'nika:jq · item order kept', 'table', 'fixed', 1144, 486, 'with: { facts: "${{ tasks.facts.output }}" }'),
  task('draft', 'Draft a change', 'infer · 900 output tokens', 'infer', 'ai', 1350, 348, 'infer: { max_tokens: 900, schema: … }'),
  task('check', 'Check the brief', 'nika:assert · required fields', 'check', 'fixed', 1556, 348, 'invoke: { tool: nika:assert }'),
  task('approve', 'Your approval', 'nika:prompt · confirm', 'hand', 'human', 1762, 348, 'invoke: { tool: nika:prompt }'),
  task('brief', 'Save the brief', 'nika:write · checkout.md', 'markdown', 'tool', 1968, 84, 'when: ${{ with.approved == true }}'),
  task('issue', 'Create the issue', 'GitHub · scoped write', 'github', 'tool', 1968, 208, 'when: ${{ with.approved == true }}'),
  task('telegram', 'Notify Telegram', 'Product team', 'telegram', 'tool', 1968, 332, 'invoke: { tool: "mcp:telegram/send_message" }'),
  task('slack', 'Notify Slack', '#product', 'slack', 'tool', 1968, 456, 'invoke: { tool: "mcp:slack/post_message" }'),
  task('update', 'Update Linear', 'Link the brief + issue', 'linear', 'tool', 1968, 580, 'invoke: { tool: "mcp:linear/update_issue" }'),
];

export const DEFINITIONS = TASKS.filter(t => !t.fan || t.iteration === 0);
export const WORLD = { width: 2160, height: 680, scale: 0.645, x: 32, y: 5 };
const byId = Object.fromEntries(TASKS.map(t => [t.id, t]));
const endpoint = (id, side = 'right') => {
  const t = byId[id];
  return [t.x + (side === 'right' ? t.width : side === 'left' ? 0 : t.width / 2), t.y + (side === 'top' ? 0 : side === 'bottom' ? t.height : t.height / 2)];
};
const link = (from, to, path, extra = {}) => {
  const [x1, y1] = endpoint(from), [x2, y2] = endpoint(to, 'left');
  const bend = Math.max(18, (x2 - x1) / 2);
  return { from, to, path: path || `M${x1} ${y1} C${x1 + bend} ${y1} ${x2 - bend} ${y2} ${x2} ${y2}`, ...extra };
};

// Deliberate ports and shared batch barriers, never random control points.
export const EDGES = [
  link('feedback', 'themes'), link('csv', 'parse'), link('parse', 'metrics'),
  link('themes', 'draft', 'M386 132 H1406 Q1430 132 1430 156 V348'),
  link('metrics', 'rule'),
  link('notes', 'draft', 'M180 380 H192 Q208 380 208 364 V318 Q208 302 224 302 H1302 Q1326 302 1326 326 V346 Q1326 366 1350 366'),
  link('linear', 'draft', 'M180 504 H192 Q208 504 208 488 V342 Q208 326 224 326 H1278 Q1302 326 1302 350 V374 Q1302 394 1326 394 H1350'),
  ...FIXTURE.competitors.flatMap(c => [link(`pages_${c.id}`, `facts_${c.id}`, undefined, { batch: true }), link(`facts_${c.id}`, `shape_${c.id}`, undefined, { batch: true })]),
  ...FIXTURE.competitors.map(c => link(`shape_${c.id}`, 'compare', undefined, { batch: true })),
  link('compare', 'rule', 'M1224 486 V288'),
  link('rule', 'draft', 'M1304 256 C1340 256 1314 380 1350 380'),
  link('draft', 'check'), link('check', 'approve'),
  link('approve', 'brief', 'M1922 362 C1948 362 1942 116 1968 116'),
  link('brief', 'issue', 'M2048 148 V208'),
  ...['telegram', 'slack', 'update'].map(id => { const t = byId[id]; return link('issue', id, `M2128 240 H2132 Q2150 240 2150 258 V${t.y + 14} Q2150 ${t.y + 32} 2132 ${t.y + 32} H2128`); }),
];

// Scheduling barriers: a dependent for_each starts after the WHOLE prior batch.
export const BATCHES = ['pages', 'facts', 'shape'].map(name => ({ name, ids: FIXTURE.competitors.map(c => `${name}_${c.id}`), maxParallel: 3 }));
export const EXTERNAL_ACTIONS = ['issue', 'telegram', 'slack', 'update'];
