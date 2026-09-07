import { gsap } from 'gsap';
import { MotionPathPlugin } from 'gsap/MotionPathPlugin';
import { TASKS, DEFINITIONS, EDGES, BATCHES, WORLD, FIXTURE, calculateEvidence } from './commerce-model.js';
import { YAML_EXCERPTS, renderYaml } from './commerce-yaml.js';
import { mountFileReview, animateFileReview } from './file-review.js';
import { CHAPTERS } from './film-timing.js';
import './commerce-scene.css';

gsap.registerPlugin(MotionPathPlugin);
const assets = import.meta.glob('./assets/*.{svg,png}', { eager: true, query: '?url', import: 'default' });
const colors = { tool: '#9fd0ff', fixed: '#70dbaa', ai: '#b69aff', human: '#f2ba6b' };
const esc = s => s.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
const paths = {
  text: '<path d="M4 5h16M4 10h16M4 15h11M4 20h7"/>',
  csv: '<rect x="3" y="3" width="18" height="18" rx="3"/><path d="M3 9h18M3 15h18M10 9v12"/>',
  markdown: '<path d="M5 2h9l5 5v15H5Z"/><path d="M14 2v6h5M8 17v-5l3 3 3-3v5"/>',
  infer: '<path d="m12 3 9 9-9 9-9-9Z"/>',
  calculator: '<rect x="4" y="2" width="16" height="20" rx="3"/><path d="M8 7h8M8 12h2M14 12h2M8 17h2M14 17h2"/>',
  check: '<path d="m5 12 4 4L19 6"/><path d="M20 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h8"/>',
  table: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 10h18M9 4v16M15 10v10"/>',
};
function icon(name) {
  return paths[name] ? `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths[name]}</svg>` : `<img src="${assets[`./assets/${name}.svg`]}" class="${['github', 'linear'].includes(name) ? 'icon-mono' : name === 'telegram' ? 'icon-telegram' : ''}" alt="" />`;
}
const evidence = calculateEvidence();
const words = s => s.split(' ').map(w => `<span>${esc(w)} </span>`).join('');
const briefOutcome = `<div class="outcome outcome-brief">
  <header>${icon('markdown')}<span>checkout-brief.md</span><b>Approved & saved</b></header>
  <div class="brief-body"><h3>Let customers buy without creating an account.</h3>
    <div class="result-metrics"><div><b>25<span>%</span></b><small>checkout abandonment</small></div><div><b>2 <span>of 3</span></b><small>guest options found</small></div><div><b>1</b><small>competitor still unclear</small></div></div>
    <p><i>AI proposal</i> Test guest checkout. Keep receipts and order tracking, then measure completion against the current baseline.</p>
    <div class="result-comparison">${FIXTURE.competitors.map(c=>`<div><b>${c.name}</b><span class="${c.guest === -1 ? 'unknown' : ''}">${c.guest === 1 ? '✓ Guest checkout' : '? Not established'}</span><small>“${c.quote}”</small></div>`).join('')}</div>
    <footer><span>${icon('csv')}sales.csv</span><span>${icon('text')}feedback.md</span><span>${icon('markdown')}product.md</span><span>${icon('web')}3 page snapshots</span></footer>
  </div></div>`;
const outcomes = {
  brief: briefOutcome,
  issue: `<div class="outcome outcome-issue"><header>${icon('github')}<span>GitHub · #128</span><b>Created</b></header><div><strong>Test guest checkout</strong><span>Acceptance criteria + brief + source references attached.</span></div></div>`,
  telegram: `<div class="outcome outcome-update"><header>${icon('telegram')}<span>Telegram · Product team</span><b>Sent</b></header><div><strong>One clear next experiment.</strong><p>Guest checkout proposal approved. The brief and GitHub #128 are ready to review.</p><small>↗ Open the brief & implementation issue</small></div></div>`,
  slack: `<div class="outcome outcome-update"><header>${icon('slack')}<span>Slack · #product</span><b>Sent</b></header><div><strong>The same context, shared with the team.</strong><p>25% baseline · competitor evidence · your approval.</p><small>↗ Linked to GitHub #128 and SHOP-42</small></div></div>`,
  update: `<div class="outcome outcome-update"><header>${icon('linear')}<span>Linear · SHOP-42</span><b>Updated</b></header><div><strong>Guest checkout experiment</strong><p>Brief attached. GitHub #128 linked. Source evidence preserved.</p><small>✓ Ready for planning, not silently deployed</small></div></div>`,
};

export function mountWorkflowScene() {
  document.querySelector('.yaml-window').insertAdjacentHTML('beforeend', `
    <div class="workflow-code-meta"><span class="editor-mode">YAML source</span><span class="editor-detail">Indentation makes the structure visible</span><small class="editor-note">Focused excerpt · setup and 17 other tasks folded</small></div>
    <div class="graph-viewport"><div class="workflow-world">
      <pre class="yaml-source-root"><b>nika:</b> improve-checkout
<i>… model, permits, const, run</i>
<b>tasks:</b></pre>
      <div class="fan-group"><header><b>for_each</b><span>3 competitors, the same instructions</span><i>max_parallel: 3</i></header><footer><span>Read every page</span><span>Extract cited facts</span><span>Validate the shape</span></footer></div>
      <div class="batch-barrier barrier-pages"><span>join</span></div><div class="batch-barrier barrier-facts"><span>join</span></div>
      <svg class="workflow-edges" viewBox="0 0 2160 680" aria-hidden="true"><defs><marker id="flow-arrow" viewBox="0 0 8 8" refX="6" refY="4" markerWidth="4" markerHeight="4" orient="auto"><path d="m2 1 3 3-3 3" fill="none" stroke="#94adbf" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></marker></defs>
      ${EDGES.map((e,i)=>`<path id="edge-${e.from}-${e.to}" class="workflow-edge ${e.batch ? 'batch-edge' : ''}" d="${e.path}" pathLength="1" marker-end="url(#flow-arrow)"/><circle class="workflow-packet packet-${i}" r="4.5"/>`).join('')}</svg>
      <div class="graph-label label-sources">YOUR CONTEXT</div><div class="graph-label label-reasoning">EXPLICIT RULES + GENERATED CONTENT</div><div class="graph-label label-delivery">APPROVE → SHARE</div>
      ${TASKS.map(t=>`<article class="workflow-task task-${t.id} kind-${t.kind} ${t.fan ? 'fan-task' : ''}" data-task-id="${t.id}" data-definition="${t.logical}" style="--task-color:${colors[t.kind]}">
        <b class="task-id">${t.logical}${t.fan ? `<span class="iteration-label">[${t.iteration}]</span>` : ''}</b><code class="task-code" title="${esc(t.code).replaceAll('"','&quot;')}">${esc(t.code)}</code>
        ${YAML_EXCERPTS[t.id] ? `<pre class="task-yaml">${renderYaml(YAML_EXCERPTS[t.id])}</pre>` : ''}
        <div class="task-node-content"><span class="task-symbol">${icon(t.icon)}</span><strong>${t.title}</strong><small class="task-detail">${t.detail}</small></div>
        <span class="task-plan-check">✓</span><span class="task-done">✓</span>${outcomes[t.id] || ''}</article>`).join('')}
    </div></div>
    <div class="foreach-code-strip"><span>One definition, three iterations.</span><pre><b>for_each:</b> { items: "\u0024{{ const.competitors }}", <i>max_parallel: 3</i> }
<b>invoke:</b> { tool: <em>nika:fetch</em>, args: { url: "\u0024{{ item }}", mode: article } }
<b>retry:</b> { max_attempts: 3 }    <b>timeout:</b> "30s"</pre></div>
    <div class="context-preview"><div class="preview-file">${icon('csv')}sales.csv <span>captured input</span></div><div class="csv-row"><span>checkout_started</span><b>480</b></div><div class="csv-row"><span>checkout_completed</span><b>360</b></div><div class="calculation"><span>(480 − 360) ÷ 480</span><b>= ${evidence.abandonment}%</b><small>Calculated by nika:jq, not guessed by AI.</small></div><blockquote>${FIXTURE.feedback}<cite>feedback.md · customer quote</cite></blockquote><p>${icon('markdown')}${FIXTURE.requirement}</p></div>
    <div class="draft-preview"><header>${icon('infer')}GENERATING YOUR BRIEF <span>infer</span></header><div class="draft-words">${words('Let customers buy without creating an account.')}</div><p>Try guest checkout. Keep receipts and order tracking. Measure completion against the 25% abandonment baseline.</p><footer><span>schema: { proposal, evidence, experiment }</span><b>AI wording can vary. Sources stay attached.</b></footer></div>
  `);
  document.querySelector('.scene-contract').insertAdjacentHTML('beforeend', `
    <div class="workflow-heading"><p class="eyebrow">THE CONTRACT IS THE PLAN</p>
      ${Object.entries({file:'One intention. A plan you can inspect.',graph:'One file unfolds into a whole operation.',check:'Check the plan. Nothing has run yet.',context:'Your data becomes useful context.',parallel:'Three competitors. In parallel. Within limits.',compare:'Evidence in. Your rules decide what qualifies.',draft:'AI proposes. The contract keeps control.',approval:'Nothing is published without your approval.',delivery:'One approval. Coordinated actions.',result:'A clear next experiment. Ready for your team.'}).map(([key,title])=>`<h2 class="heading-${key}">${title}</h2>`).join('')}
    </div>
    <div class="workflow-legend"><span class="key-tool"><i>◆</i>Declared tools & paths</span><span class="key-fixed"><i>=</i>Calculated from captured data</span><span class="key-ai"><i>◇</i>AI-generated, may vary</span><span class="key-human">${icon('hand')}You authorize</span></div>
    <div class="workflow-status"><span class="status-dot"></span><span class="status-text">PLAN VIEW · NOTHING RUNNING</span></div>
    <div class="workflow-inspector">
      <div class="inspector-panel inspector-check"><div class="check-command"><small>BEFORE EXECUTION</small><code>❯ nika check</code></div>${[['structure','Graph & bindings','Declared dependencies'],['access','Access boundaries','Named files, hosts, tools'],['limits','Bounded work','3 parallel · capped outputs'],['approval','Write gate','Explicit human approval']].map(([id,title,detail])=>`<div class="plan-check check-${id}"><i>✓</i><span><b>${title}</b><small>${detail}</small></span></div>`).join('')}</div>
      <div class="inspector-panel inspector-context"><span class="inspector-icon fixed">${icon('calculator')}</span><div><small>DETERMINISTIC CALCULATION</small><b>Same captured numbers. Same 25% result.</b><p>External data may change on another run. This calculation does not invent the inputs.</p></div><span class="context-counter">CSV + Markdown + Linear</span></div>
      <div class="inspector-panel inspector-parallel"><span class="inspector-icon">${icon('web')}</span><div><small class="parallel-kicker">BOUNDED FAN-OUT</small><b class="parallel-phase">One collection opens three controlled branches.</b><p class="parallel-detail">The next map waits for the whole previous batch. Results keep their input order.</p></div><span class="parallel-counter">0 / 3 pages</span></div>
      <div class="inspector-panel inspector-evidence"><div class="evidence-metric"><b>25%</b><span>CSV abandonment</span></div>${FIXTURE.competitors.map(c=>`<div class="evidence-store ${c.guest === -1 ? 'unknown' : ''}"><b>${c.name} <span>${c.guest === 1 ? 'guest: 1' : 'guest: −1'}</span></b><p>“${c.quote}”</p><small>${c.guest === 1 ? 'AI-extracted, source attached' : 'Unknown stays unknown'}</small></div>`).join('')}</div>
      <div class="inspector-panel inspector-rule"><span class="inspector-icon fixed">${icon('calculator')}</span><div><small>YOUR AUTHORED RULE · NOT A MODEL VERDICT</small><b><span class="rule-equation">25% ≥ 20% &nbsp; AND &nbsp; 2 supported examples ≥ 2</span><span class="rule-result"> → candidate for review</span></b><p>Same structured facts + same rule = same classification. The experiment still needs your judgement.</p></div></div>
      <div class="inspector-panel inspector-draft"><span class="inspector-icon ai">${icon('infer')}</span><div><small>GENERATED CONTENT, EXPLICIT BOUNDARIES</small><b>The AI drafts a proposal. It cannot publish it.</b><p>Typed output, attached sources and 900 output tokens for this call. A shape check is not a truth check.</p></div></div>
      <div class="inspector-panel inspector-approval"><img src="${assets['./assets/avatar-approver.png']}" alt="Approver"/><div><small>PAUSED · YOUR DECISION</small><b>Save the brief, create the issue and share it with the team?</b><p>GitHub + Telegram + Slack + Linear. No deployment and no silent external write.</p></div><span class="approval-button">${icon('hand')}Approve & continue</span><span class="approval-confirmed">✓ Approved by you</span></div>
      <div class="inspector-panel inspector-delivery"><span class="inspector-icon fixed">${icon('check')}</span><div><small>ONLY AFTER APPROVAL</small><b>Save the brief. Create the issue. Share the same evidence.</b><p>Telegram and Slack receive the links. Linear is updated with the implementation issue.</p></div><span class="delivery-counter">0 / 5 completed</span></div>
    </div>
    <div class="trace-receipt">${icon('check')}<b>Evidence, approval and actions recorded.</b><code>nika trace verify <span>✓ record integrity</span></code><small>A trace proves the record, not an AI claim or a conversion uplift.</small></div>
    <span class="example-disclaimer">Illustrative run · fictional shops and data · no live messages or model calls</span>
  `);
  mountFileReview();
  document.querySelector('.controls').insertAdjacentHTML('afterend', `<nav class="chapter-nav" aria-label="Film chapters">${CHAPTERS.map(([title,time])=>`<button type="button" data-film-time="${time}">${title}</button>`).join('')}</nav>`);
}

export function animateWorkflowScene(tl) {
  gsap.set('.workflow-heading, .workflow-heading h2, .workflow-legend, .workflow-status, .workflow-code-meta, .workflow-task, .foreach-code-strip, .workflow-edges, .fan-group, .batch-barrier, .graph-label, .workflow-inspector, .inspector-panel, .context-preview, .draft-preview, .trace-receipt, .example-disclaimer', {autoAlpha:0});
  gsap.set('.task-node-content, .task-plan-check, .task-done, .outcome, .workflow-packet, .iteration-label, .approval-confirmed, .rule-result', {autoAlpha:0});
  gsap.set('.draft-words span', {autoAlpha:0,y:5});
  gsap.set('.yaml-source-root, .task-yaml, .task-id, .task-code', {autoAlpha:0});
  gsap.set('.plan-check', {autoAlpha:0.22});
  gsap.set('.plan-check i', {scale:0});
  gsap.set('.workflow-edge', {strokeDasharray:1,strokeDashoffset:1});
  DEFINITIONS.forEach((t,i)=>gsap.set(`.task-${t.id}`, {x:i<10?20:565,y:(i%10)*38,width:525,height:32,borderRadius:7}));
  const sourceRows = {csv:[96,134],parse:[250,222],pages_a:[492,290]};
  Object.entries(sourceRows).forEach(([id,[y,height]])=>gsap.set(`.task-${id}`,{x:20,y,width:1080,height,backgroundColor:'transparent',borderColor:'transparent'}));
  TASKS.filter(t=>t.fan&&t.iteration>0).forEach(t=>gsap.set(`.task-${t.id}`, {x:t.x,y:402,width:160,height:64}));
  let currentHeading;
  function heading(key, at) {
    if(currentHeading) tl.to(`.heading-${currentHeading}`,{autoAlpha:0,y:-5,duration:.24},at);
    tl.fromTo(`.heading-${key}`,{y:7},{autoAlpha:1,y:0,duration:.4,immediateRender:false},at+.2);
    currentHeading=key;
  }
  function panel(key, at) {
    tl.to('.inspector-panel',{autoAlpha:0,duration:.18},at)
      .to(`.inspector-${key}`,{autoAlpha:1,duration:.28},at+.18);
  }
  function status(text, color, at) { tl.set('.status-text',{textContent:text},at).to('.workflow-status',{color,duration:.25},at); }
  function camera(values, at, duration=.95) { tl.to('.workflow-world',{...values,duration,ease:'power3.inOut'},at); }
  function focus(ids,at) { tl.to('.workflow-task',{opacity:.24,duration:.4},at).to(ids.map(id=>`.task-${id}`).join(','),{opacity:1,duration:.4},at); }
  function complete(id,at) {
    tl.to(`.task-${id} .task-done`,{autoAlpha:1,scale:1,duration:.28},at)
      .to(`.task-${id}`,{boxShadow:'0 0 20px rgba(112,219,170,.08)',duration:.3},at);
  }
  function packet(from,to,at,duration=.65,color=colors.tool) {
    const i=EDGES.findIndex(e=>e.from===from&&e.to===to);
    if(i<0) throw new Error(`Missing visible edge: ${from} → ${to}`);
    tl.set(`.packet-${i}`,{autoAlpha:1,fill:color},at)
      .fromTo(`.packet-${i}`,{motionPath:{path:`#edge-${from}-${to}`,align:`#edge-${from}-${to}`,alignOrigin:[.5,.5],start:0,end:0}}, {motionPath:{path:`#edge-${from}-${to}`,align:`#edge-${from}-${to}`,alignOrigin:[.5,.5]},duration,ease:'none',immediateRender:false},at)
      .set(`.packet-${i}`,{autoAlpha:0},at+duration);
  }

  tl.addLabel('file',17.2)
    .to('.contract-copy, .yaml-code, .contract-seal',{autoAlpha:0,duration:.35},17.15)
    .to('.yaml-window',{x:-170,y:-5,width:1120,height:680,scale:1,duration:.85},17.2)
    .to('.workflow-code-meta, .yaml-source-root',{autoAlpha:1,duration:.5},17.85)
    .to('.task-csv, .task-parse, .task-pages_a, .task-yaml',{autoAlpha:1,duration:.45,stagger:.04},18.0)
    .to('.workflow-code-meta, .foreach-code-strip, .task-code',{autoAlpha:0,duration:.3},23.0)
    .to('.workflow-heading',{autoAlpha:1,duration:.4},23.1)
    .to('.yaml-window',{x:50,width:1460,duration:1.35},23.2)
    .to(DEFINITIONS.map(t=>`.task-${t.id}`).join(','),{width:160,height:18,duration:.4},23.25)
    .to('.task-id',{fontSize:11,y:-3,duration:.4},23.25);
  // Interleave only while folding the code. Rows remain aligned while reading.
  DEFINITIONS.slice(10).forEach((t,i)=>tl.to(`.task-${t.id}`,{y:i*38+19,duration:.4},23.25));
  heading('graph',23.1);
  DEFINITIONS.forEach(t=>tl.to(`.task-${t.id}`,{x:t.x,duration:.6},23.72).to(`.task-${t.id}`,{y:t.y,height:t.height,borderRadius:11,backgroundColor:t.kind==='ai'?'#1f2032':t.kind==='human'?'#28231e':'#16212c',borderColor:({tool:'#476176',fixed:'#47755e',ai:'#79619c',human:'#95703d'})[t.kind],duration:.7},24.34));
  tl.to('.yaml-window',{y:54,height:510,duration:.75},24.25)
    .to('.graph-viewport',{top:64,duration:.75},24.25);
  camera(WORLD,24.25,.8);
  tl.to('.task-node-content',{autoAlpha:1,duration:.38},24.82)
    .to('.task-id',{y:0,duration:.3},24.82)
    .to('.fan-group, .graph-label',{autoAlpha:1,duration:.4},25.15);
  TASKS.filter(t=>t.fan&&t.iteration>0).forEach(t=>tl.to(`.task-${t.id}`,{autoAlpha:1,y:t.y,duration:.62,ease:'power3.out'},25.2+t.iteration*.18));
  tl.to('.iteration-label',{autoAlpha:1,duration:.3},25.45)
    .to('.workflow-edges, .batch-barrier, .workflow-legend, .workflow-status, .example-disclaimer',{autoAlpha:1,duration:.4},25.95)
    .to('.workflow-edge',{strokeDashoffset:0,duration:.5,stagger:.025},26.05)
    .set('.valid-pill',{textContent:'plan'},26.0);

  heading('check',27.2); panel('check',27.5);
  tl.addLabel('check',27.2).to('.workflow-inspector',{autoAlpha:1,duration:.4},27.5);
  ['structure','access','limits','approval'].forEach((id,i)=>tl.to(`.check-${id}`,{autoAlpha:1,color:colors.fixed,duration:.3},28.0+i*.68).to(`.check-${id} i`,{scale:1,duration:.3,ease:'back.out(1.6)'},28.0+i*.68));
  tl.to('.task-plan-check',{autoAlpha:1,duration:.24,stagger:.04},29.6)
    .to('.yaml-window',{borderColor:'#47785f',duration:.5},30.5)
    .set('.valid-pill',{textContent:'checked',color:colors.fixed},30.6);
  status('CHECKED · NOTHING EXECUTED',colors.fixed,30.6);

  heading('context',32.2); panel('context',32.5); tl.addLabel('run',32.2);
  status('RUNNING THE CHECKED PLAN',colors.tool,32.55);
  tl.set('.valid-pill',{textContent:'running',color:colors.tool},32.55)
    .to('.task-plan-check',{autoAlpha:0,duration:.3},32.5);
  camera({scale:.88,x:20,y:-60},32.65);
  focus(['feedback','themes','csv','parse','metrics','notes','linear'],32.65);
  tl.to('.context-preview',{autoAlpha:1,x:0,duration:.5},33.25);
  complete('feedback',33.1); complete('csv',33.35); complete('notes',33.6); complete('linear',33.85);
  packet('feedback','themes',33.35,.65); packet('csv','parse',33.6,.65);
  complete('parse',34.35); packet('parse','metrics',34.5,.65,colors.fixed); complete('metrics',35.25);
  tl.fromTo('.calculation b',{autoAlpha:0,y:7},{autoAlpha:1,y:0,duration:.5,immediateRender:false},35.25)
    .fromTo('.context-preview blockquote',{autoAlpha:0},{autoAlpha:1,duration:.5,immediateRender:false},36.3);
  complete('themes',37.1);
  tl.to('.task-themes',{borderColor:colors.ai,duration:.35},36.2)
    .set('.task-themes .task-detail',{textContent:'AI theme: forced account'},37.1)
    .to('.context-preview',{autoAlpha:0,duration:.4},39.4);

  heading('parallel',39.6); panel('parallel',39.8); tl.addLabel('parallel',40);
  camera({scale:1.25,x:-510,y:-430},39.9,1.1);
  focus([...BATCHES.flatMap(b=>b.ids),'compare'],39.9);
  tl.to('.fan-group',{borderColor:'#8391ba',backgroundColor:'rgba(98,112,163,.06)',duration:.5},40.5)
    .set('.parallel-phase',{textContent:'Fetch three pages, with at most three in flight.'},40.4)
    .set('.parallel-detail',{textContent:'A retry belongs to the authored policy. It does not create a new plan.'},40.4);
  complete('pages_a',41.9);
  tl.set('.parallel-counter',{textContent:'1 / 3 pages'},41.9)
    .set('.task-pages_b .task-detail',{textContent:'HTTP 429 · retry 2 of 3'},42.0)
    .to('.task-pages_b',{borderColor:colors.human,backgroundColor:'#30271d',duration:.3},42.0)
    .set('.parallel-phase',{textContent:'One page retries. The others keep their results.'},42.0);
  complete('pages_c',42.4);
  tl.set('.parallel-counter',{textContent:'2 / 3 pages'},42.4)
    .set('.task-pages_b .task-detail',{textContent:'Page captured · attempt 2'},43.6)
    .to('.task-pages_b',{borderColor:colors.tool,backgroundColor:'#16212c',duration:.4},43.6);
  complete('pages_b',43.6);
  tl.set('.parallel-counter',{textContent:'3 / 3 pages'},43.6)
    .to('.barrier-pages',{borderColor:colors.fixed,color:colors.fixed,duration:.35},43.8)
    .set('.parallel-kicker',{textContent:'AI EXTRACTION · THREE TYPED RESULTS'},44.1)
    .set('.parallel-phase',{textContent:'All pages arrived. Now each gets the same extraction task.'},44.1)
    .set('.parallel-detail',{textContent:'384 output tokens per call. The schema includes “unknown”; the model is not asked to guess.'},44.1);
  FIXTURE.competitors.forEach((c,i)=>{
    packet(`pages_${c.id}`,`facts_${c.id}`,44.35,.7);
    tl.to(`.task-facts_${c.id}`,{borderColor:colors.ai,boxShadow:'0 0 30px rgba(182,154,255,.22)',duration:.35},45.05)
      .set(`.task-facts_${c.id} .task-detail`,{textContent:c.guest===1?'guest: 1 · quoted source':'guest: −1 · unknown'},46.15+i*.3);
    complete(`facts_${c.id}`,46.15+i*.3);
  });
  tl.set('.parallel-counter',{textContent:'3 / 3 extracts'},46.8)
    .to('.barrier-facts',{borderColor:colors.fixed,color:colors.fixed,duration:.35},46.9)
    .set('.parallel-phase',{textContent:'Validate the structure, not the truth of an AI answer.'},47.1)
    .set('.parallel-detail',{textContent:'An unknown value is valid data. It stays unknown all the way to the brief.'},47.1);
  FIXTURE.competitors.forEach((c,i)=>{
    packet(`facts_${c.id}`,`shape_${c.id}`,47.25,.65,colors.ai);
    complete(`shape_${c.id}`,48.0+i*.1);
    packet(`shape_${c.id}`,'compare',48.6,.8,colors.fixed);
  });
  tl.set('.task-shape_c .task-detail',{textContent:'Valid shape · still unknown'},48.2);
  complete('compare',49.5);
  tl.set('.parallel-counter',{textContent:'[A, B, C]'},49.5)
    .set('.parallel-phase',{textContent:'One ordered collection. No source loses its identity.'},49.5)
    .set('.parallel-detail',{textContent:'for_each returns results in input order, even when the pages finish in a different order.'},49.5);

  heading('compare',51.1); panel('evidence',51.25); camera(WORLD,51.2,1.0);
  tl.to('.workflow-task',{opacity:1,duration:.5},51.2);
  packet('metrics','rule',52.0,.8,colors.fixed); packet('compare','rule',52.0,.8,colors.fixed);
  panel('rule',53.0); complete('rule',53.25);
  tl.to('.rule-result',{autoAlpha:1,duration:.4},53.65)
    .set('.task-rule .task-detail',{textContent:'Threshold met → review'},53.65);

  heading('draft',55.8); panel('draft',56.0); camera({scale:.9,x:-650,y:-118},56.0,1.0);
  focus(['draft','check','approve','brief','issue','telegram','slack','update'],56.0);
  ['themes','notes','linear','rule'].forEach(id=>packet(id,'draft',55.9,.85,id==='themes'?colors.ai:colors.tool));
  tl.to('.draft-preview',{autoAlpha:1,duration:.4},56.8)
    .to('.draft-words span',{autoAlpha:1,y:0,duration:.2,stagger:.12},57.0)
    .to('.task-draft',{borderColor:colors.ai,boxShadow:'0 0 35px rgba(182,154,255,.22)',duration:.4},56.7);
  complete('draft',58.7); packet('draft','check',58.85,.55,colors.ai); complete('check',59.5); packet('check','approve',59.6,.55);

  heading('approval',60.25); panel('approval',60.3); tl.addLabel('approval',60.25);
  camera({scale:.68,x:-80,y:0},60.3,.9);
  status('PAUSED · WAITING FOR YOU',colors.human,60.4);
  tl.set('.valid-pill',{textContent:'paused',color:colors.human},60.4)
    .to('.task-approve',{borderColor:colors.human,boxShadow:'0 0 30px rgba(242,186,107,.2)',duration:.4},60.4)
    .to('.approval-button',{scale:.96,duration:.13,repeat:1,yoyo:true},63.15)
    .to('.approval-button',{autoAlpha:0,duration:.2},63.45)
    .to('.approval-confirmed',{autoAlpha:1,duration:.3},63.65);
  complete('approve',63.7);

  heading('delivery',64.05); panel('delivery',64.1); camera(WORLD,64.05,.8);
  status('RUNNING · APPROVED ACTIONS ONLY',colors.tool,64.15);
  tl.set('.valid-pill',{textContent:'running',color:colors.tool},64.15)
    .to('.draft-preview',{autoAlpha:0,duration:.3},64.05)
    .to('.workflow-task',{opacity:1,duration:.4},64.05);
  packet('approve','brief',64.55,.7,colors.human);
  complete('brief',65.3); packet('brief','issue',65.35,.35);
  complete('issue',65.8);
  tl.set('.delivery-counter',{textContent:'2 / 5 completed'},65.85);
  ['telegram','slack','update'].forEach((id,i)=>{packet('issue',id,66.0,.6);complete(id,66.7+i*.12);});
  tl.set('.delivery-counter',{textContent:'5 / 5 completed'},67.05);

  heading('result',68.0); tl.addLabel('result',68.0);
  tl.to('.workflow-inspector, .workflow-legend, .workflow-status, .workflow-edges, .fan-group, .batch-barrier, .graph-label, .yaml-window > header',{autoAlpha:0,duration:.4},68.0)
    .to(TASKS.filter(t=>!outcomes[t.id]).map(t=>`.task-${t.id}`).join(','),{autoAlpha:0,duration:.35},68.1)
    .to('.task-id, .task-node-content, .task-done',{autoAlpha:0,duration:.3},68.15)
    .set('.graph-viewport, .yaml-window',{overflow:'visible'},68.3)
    .to('.yaml-window',{y:32,height:560,backgroundColor:'transparent',borderColor:'transparent',boxShadow:'none',duration:.8},68.3)
    .to('.graph-viewport',{top:0,duration:1.1},68.3);
  camera({x:0,y:0,scale:1},68.3,1.1);
  const boxes = {brief:[0,0,842,454],issue:[0,468,842,92],telegram:[890,0,570,174],slack:[890,188,570,174],update:[890,376,570,184]};
  Object.entries(boxes).forEach(([id,[x,y,width,height]])=>tl.to(`.task-${id}`,{x,y,width,height,borderRadius:17,borderColor:'#40586b',boxShadow:'0 20px 50px rgba(0,0,0,.18)',duration:1.1,ease:'power3.inOut'},68.3));
  tl.to('.outcome',{autoAlpha:1,duration:.45,stagger:.12},69.25)
    .to('.trace-receipt',{autoAlpha:1,duration:.4},70.25)
    .to('.scene-contract',{autoAlpha:0,y:-8,duration:.65},77.0)
    .set('.scene-end',{autoAlpha:1},77.45)
    .to('.end-lockup > *',{autoAlpha:1,y:0,duration:.55,stagger:.13},77.55);

  // Give the real source its own continuous camera pass, before the outline.
  // Every selected source block below is the very same task node used in the DAG.
  tl.shiftChildren(8, true, 23);
  tl.to('.workflow-world',{y:-384,duration:1.25,ease:'power3.inOut'},21.2)
    .set('.editor-detail',{textContent:'One indented task, three competitors'},22.1)
    .to('.task-pages_a',{backgroundColor:'rgba(159,208,255,.035)',borderColor:'#304859',duration:.5},22.3)
    .to('.task-yaml .yaml-source-line:not(:first-child)',{autoAlpha:0,y:-8,scaleY:.85,duration:.32,stagger:.018},25.5)
    .to('.yaml-source-root, .task-yaml',{autoAlpha:0,duration:.3},26.15)
    .to('.workflow-world',{y:0,duration:.85},26.25)
    .set('.editor-mode',{textContent:'20 named tasks'},26.9)
    .set('.editor-detail',{textContent:'3 bounded maps'},26.9)
    .set('.editor-note',{textContent:'Same task definitions · folded into blocks'},26.9)
    .to('.task-id, .task-code',{autoAlpha:1,duration:.35},27.0)
    .to(DEFINITIONS.map(t=>`.task-${t.id}`).join(','),{autoAlpha:1,duration:.4,stagger:.025},26.9)
    .to('.foreach-code-strip',{autoAlpha:1,duration:.35},27.4);
  DEFINITIONS.forEach((t,i)=>tl.to(`.task-${t.id}`,{x:i<10?20:565,y:(i%10)*38,width:525,height:32,borderColor:({tool:'#476176',fixed:'#47755e',ai:'#79619c',human:'#95703d'})[t.kind],backgroundColor:'#16212c',duration:.85},26.25));
  animateFileReview(tl);
}
