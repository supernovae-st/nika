import { gsap } from 'gsap';
import { FILE_REVIEW } from './film-timing.js';
import './file-review.css';

const reviewer = new URL('./assets/avatar-approver.png', import.meta.url).href;
const engineer = new URL('./assets/avatar-engineer.png', import.meta.url).href;

export function mountFileReview() {
  document.querySelector('.scene-contract').insertAdjacentHTML('beforeend', `
    <div class="file-review-heading"><span>KEEP THE PROCEDURE</span><h2>A file your team can improve.</h2></div>
  `);
  document.querySelector('.yaml-window').insertAdjacentHTML('beforeend', `
    <aside class="file-review" aria-label="Illustrative workflow source review">
      <header><span class="review-file-icon" aria-hidden="true">{ }</span><div><small>SHARE THE SOURCE</small><strong>checkout.nika.yaml</strong></div><div class="review-people"><img src="${engineer}" alt="Author"/><img src="${reviewer}" alt="Reviewer"/></div></header>
      <div class="review-diff"><div><span>REVIEW THE CHANGE</span><small>previous → proposed</small></div><code class="diff-before"><i>−</i>max_parallel: <b>2</b></code><code class="diff-after"><i>+</i>max_parallel: <b>3</b></code></div>
      <div class="review-comment"><img src="${reviewer}" alt=""/><div><strong>Keep it bounded.</strong><p>Three pages in parallel.<br/>Retries and timeout stay explicit.</p></div></div>
      <footer class="review-history"><span>v1</span><svg viewBox="0 0 112 16" aria-hidden="true"><path d="M2 8H108m-6-5 6 5-6 5"/></svg><span class="review-version">v2</span><strong>Change it. Keep its history.</strong></footer>
    </aside>
    <div class="review-safety">Share the workflow, not credentials or private run data.<span>Illustrative source review · your editor + version control</span></div>
  `);
}

export function animateFileReview(tl) {
  const at = FILE_REVIEW.at;
  // Insert before folding. No cloned file or second graph, and no run during review.
  tl.shiftChildren(FILE_REVIEW.duration, true, at);
  gsap.set('.file-review-heading, .file-review, .review-safety, .review-diff, .review-comment, .review-history', {autoAlpha:0});
  gsap.set('.review-history svg path', {strokeDasharray:112,strokeDashoffset:112});
  const line = '.task-pages_a .yaml-source-line:nth-child(4)';
  tl.addLabel('review', at)
    .to('.workflow-world', {y:-434,duration:.65}, at)
    .to('.task-parse', {autoAlpha:0,duration:.35}, at)
    .to('.task-pages_a', {width:574,height:338,duration:.65}, at)
    .to('.task-pages_a .task-yaml', {fontSize:16,lineHeight:'26px',duration:.65}, at)
    .to('.task-pages_a .yaml-source-line', {height:26,duration:.65}, at)
    .to('.file-review-heading', {autoAlpha:1,duration:.5}, at+.1)
    .fromTo('.file-review', {x:20}, {x:0,autoAlpha:1,duration:.6,immediateRender:false}, at+.25)
    .to('.review-safety', {autoAlpha:1,duration:.45}, at+.6)
    .set('.editor-mode', {textContent:'Readable source'}, at)
    .set('.editor-detail', {textContent:'A proposed revision, before execution'}, at)
    .set('.editor-note', {textContent:'The same file stays in view'}, at)
    .fromTo('.review-diff', {y:8}, {autoAlpha:1,y:0,duration:.45,immediateRender:false}, at+1.35)
    .to(line, {backgroundColor:'rgba(112,219,170,.14)',boxShadow:'inset 2px 0 #70dbaa',duration:.45}, at+1.5)
    .fromTo('.review-comment', {y:8}, {autoAlpha:1,y:0,duration:.45,immediateRender:false}, at+2.75)
    .to('.review-history', {autoAlpha:1,duration:.4}, at+4.15)
    .to('.review-history svg path', {strokeDashoffset:0,duration:.6,ease:'power2.inOut'}, at+4.35)
    .to('.review-version', {borderColor:'#70dbaa',color:'#a6edc7',backgroundColor:'#21392e',duration:.4}, at+4.9)
    .to('.file-review-heading, .file-review, .review-safety', {autoAlpha:0,duration:.45}, at+7.2)
    .to(line, {backgroundColor:'transparent',boxShadow:'inset 0px 0 transparent',duration:.4}, at+7.2)
    .to('.workflow-world', {y:-384,duration:.5}, at+7.4)
    .to('.task-parse', {autoAlpha:1,duration:.4}, at+7.4)
    .to('.task-pages_a', {width:1080,height:290,duration:.5}, at+7.4)
    .to('.task-pages_a .task-yaml', {fontSize:14,lineHeight:'22px',duration:.5}, at+7.4)
    .to('.task-pages_a .yaml-source-line', {height:22,duration:.5}, at+7.4);
}
