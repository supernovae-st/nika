// Capture the GSAP edit itself, not a real-time screen recording.
// Preview first: node render-product-film.mjs http://127.0.0.1:4174 --qa
import { chromium } from 'playwright-core';
import { spawn, execFileSync } from 'node:child_process';
import { once } from 'node:events';
import { mkdir, stat } from 'node:fs/promises';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const url = new URL(process.argv[2] || 'http://127.0.0.1:4174');
url.searchParams.set('render', '1');
const qa = process.argv.includes('--qa');
const fps = 30;
const output = resolve(root, 'media/videos/intent-to-impact.mp4');
const samples = [8, 15, 18, 20.5, 23, 26.5, 30, 35, 43.5, 50, 55, 59];
const qaDir = resolve(root, '.cache/film-qa');
async function encodeAssets() {
  const gif = resolve(root, 'media/gifs/intent-to-impact.optimized.gif');
  const poster = resolve(root, 'media/posters/intent-to-impact.png');
  const sheet = resolve(root, 'media/storyboards/intent-to-impact.png');
  for (const path of [gif, poster, sheet]) await mkdir(dirname(path), { recursive: true });
  const ffmpeg = args => execFileSync('ffmpeg', ['-hide_banner', '-loglevel', 'error', '-y', ...args], { stdio: 'inherit' });
  ffmpeg(['-i', output, '-vf', 'fps=8,scale=720:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=96:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4', '-loop', '0', gif]);
  ffmpeg(['-ss', '55', '-i', output, '-frames:v', '1', poster]);
  ffmpeg(['-i', output, '-vf', 'fps=1/10,scale=640:360,tile=3x2', '-frames:v', '1', sheet]);
  if ((await stat(gif)).size > 8_000_000 || (await stat(poster)).size > 1_000_000) {
    throw new Error('Export exceeds the README media budget');
  }
  const seconds = execFileSync('ffprobe', ['-v', 'error', '-show_entries', 'format=duration', '-of', 'csv=p=0', output], { encoding: 'utf8' }).trim();
  if (seconds !== '60.000000') throw new Error(`Encoded duration: ${seconds}`);
}
await mkdir(dirname(output), { recursive: true });
await mkdir(qaDir, { recursive: true });
const browser = await chromium.launch({ channel: 'chrome', headless: true });
let encoder;
try {
  const page = await browser.newPage({ viewport: { width: 1600, height: 900 }, deviceScaleFactor: 1 });
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.goto(url.href, { waitUntil: 'networkidle' });
  await page.waitForFunction(() => Boolean(window.__film));
  await page.evaluate(() => document.fonts.ready);
  if (!await page.evaluate(() => document.fonts.check('600 20px "Clash Display"'))) {
    throw new Error('Clash Display is missing. Follow the font setup in the film README.');
  }
  await page.addStyleTag({ content: `
    .page-shell { padding:0!important; }
    .player { width:1600px!important;height:900px!important;display:block!important; }
    .viewport { width:1600px!important;height:900px!important;border:0!important;border-radius:0!important; }
    .controls,.chapter-nav { display:none!important; }
    #stage { --stage-scale:1!important; }
    *, *::before, *::after { animation-play-state:paused!important; }
  ` });
  const duration = await page.evaluate(() => window.__film.duration);
  if (duration !== 60) throw new Error(`Expected 60 seconds, got ${duration}`);
  if (!qa) {
    encoder = spawn('ffmpeg', ['-y', '-hide_banner', '-loglevel', 'error', '-f', 'image2pipe',
      '-framerate', String(fps), '-vcodec', 'png', '-i', 'pipe:0', '-an',
      '-c:v', 'libx264', '-preset', 'medium', '-crf', '19', '-pix_fmt', 'yuv420p',
      '-movflags', '+faststart', output], { stdio: ['pipe', 'inherit', 'inherit'] });
    encoder.stdin.on('error', () => {});
  }
  const done = encoder ? once(encoder, 'close') : null;
  const times = qa ? samples : Array.from({ length: duration * fps }, (_, i) => i / fps);
  for (const [i, time] of times.entries()) {
    await page.evaluate(t => window.__film.seek(t), time);
    const frame = await page.screenshot({ type: 'png', ...(qa ? { path: resolve(qaDir, `${time}.png`) } : {}) });
    if (encoder && !encoder.stdin.write(frame)) await once(encoder.stdin, 'drain');
    if (i % 150 === 0 || qa) console.log(`Frame ${i + 1}/${times.length} · ${time.toFixed(2)}s`);
  }
  if (encoder) {
    encoder.stdin.end();
    const [code] = await done;
    if (code !== 0) throw new Error(`ffmpeg exited ${code}`);
  }
  if (errors.length) throw new Error(errors.join('\n'));
  if (!qa) await encodeAssets();
  console.log(qa ? `QA frames: ${qaDir}` : `Export: ${output} · 60s · 1600×900 · 30fps`);
} finally {
  encoder?.stdin.destroy();
  if (encoder?.exitCode === null) encoder.kill();
  await browser.close();
}
