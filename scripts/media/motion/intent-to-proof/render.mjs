#!/usr/bin/env node
// Nika · From intent to proof — render CLI.
//
//   node render.mjs still 12.5 [--scale 1] [--mb]        one PNG → .cache/stills/
//   node render.mjs stills 1,4.2,9 [--scale 0.5]         several PNGs
//   node render.mjs sheet 2.0 3.2 12 [--scale 0.4]       contact sheet of a range
//   node render.mjs video --scale 0.5 --fps 30 [--mb]    parallel preview MP4
//   node render.mjs master [--resume]                    4K60 master + X cut (--resume keeps finished segments)
//   node render.mjs exports                              web cut, poster, storyboard, hero, share copy, QA stills
//
// Every frame is a pure function of time, so N workers render disjoint
// frame ranges and the segments are concatenated losslessly.
import { spawn, execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { createCanvas } from '@napi-rs/canvas';
import { init, renderFrame } from './src/film.mjs';
import { FPS, DURATION, soundCues, SECTIONS, BPM, T } from './src/timeline.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const CACHE = path.join(HERE, '.cache');
const REPO = path.resolve(HERE, '../../../..');
const args = process.argv.slice(2);
const cmd = args[0];
const opt = (name, def) => {
  const i = args.indexOf(`--${name}`);
  if (i < 0) return def;
  const v = args[i + 1];
  return v === undefined || v.startsWith('--') ? true : v;
};
const mkdir = d => fs.mkdirSync(d, { recursive: true });

function toPNG(surf, rgba, file) {
  const c = createCanvas(surf.W, surf.H);
  const ctx = c.getContext('2d');
  const img = ctx.createImageData(surf.W, surf.H);
  img.data.set(rgba);
  ctx.putImageData(img, 0, 0);
  mkdir(path.dirname(file));
  fs.writeFileSync(file, c.toBuffer('image/png'));
}

async function still(times, scale, mb) {
  const surf = init(scale);
  for (const t of times) {
    const t0 = performance.now();
    const rgba = renderFrame(surf, t, FPS, mb);
    const file = opt('out') && times.length === 1 ? path.resolve(opt('out')) : path.join(CACHE, 'stills', `t${t.toFixed(3)}.png`);
    toPNG(surf, rgba, file);
    console.log(`${file}  (${(performance.now() - t0).toFixed(0)} ms)`);
  }
}

async function sheet(a, b, n, scale, cols) {
  const surf = init(scale);
  const rows = Math.ceil(n / cols);
  const pad = 6;
  const W = cols * surf.W + (cols + 1) * pad, H = rows * (surf.H + 22) + pad;
  const out = createCanvas(W, H);
  const o = out.getContext('2d');
  o.fillStyle = '#000';
  o.fillRect(0, 0, W, H);
  const tile = createCanvas(surf.W, surf.H);
  const tctx = tile.getContext('2d');
  for (let i = 0; i < n; i++) {
    const t = n === 1 ? a : a + ((b - a) * i) / (n - 1);
    const rgba = renderFrame(surf, t, FPS, false);
    const img = tctx.createImageData(surf.W, surf.H);
    img.data.set(rgba);
    tctx.putImageData(img, 0, 0);
    const x = pad + (i % cols) * (surf.W + pad), y = pad + Math.floor(i / cols) * (surf.H + 22);
    o.drawImage(tile, x, y);
    o.fillStyle = '#9fd0ff';
    o.font = '14px "MM 400"';
    o.fillText(`${t.toFixed(3)}s · f${Math.round(t * FPS)}`, x + 4, y + surf.H + 16);
  }
  const file = path.join(CACHE, 'sheets', `sheet_${a.toFixed(2)}-${b.toFixed(2)}.png`);
  mkdir(path.dirname(file));
  fs.writeFileSync(file, out.toBuffer('image/png'));
  console.log(file);
}

// ── segment worker: frames [f0, f1) → one encoded segment ───────────────
async function segment() {
  const f0 = +opt('from'), f1 = +opt('to'), scale = +opt('scale', 1), fps = +opt('fps', FPS);
  const out = opt('out'), mb = !!opt('mb'), lossless = !!opt('lossless');
  const surf = init(scale);
  const enc = lossless
    ? ['-c:v', 'libx264', '-preset', 'ultrafast', '-qp', '0', '-pix_fmt', 'yuv444p']
    : ['-c:v', 'libx264', '-preset', 'veryfast', '-crf', '16', '-pix_fmt', 'yuv420p'];
  const ff = spawn('ffmpeg', ['-y', '-hide_banner', '-loglevel', 'error', '-f', 'rawvideo', '-pix_fmt', 'rgba',
    '-s', `${surf.W}x${surf.H}`, '-r', String(fps), '-i', 'pipe:0', ...enc, '-threads', '1', out], { stdio: ['pipe', 'inherit', 'inherit'] });
  const write = buf => new Promise(res => (ff.stdin.write(buf) ? res() : ff.stdin.once('drain', res)));
  const tStart = performance.now();
  for (let f = f0; f < f1; f++) {
    const t = f / fps;
    const rgba = renderFrame(surf, t, FPS, mb);
    await write(Buffer.from(rgba));
    if ((f - f0) % 30 === 0) {
      const el = (performance.now() - tStart) / 1000;
      process.stdout.write(`[seg ${f0}] ${f - f0 + 1}/${f1 - f0} · ${(el / (f - f0 + 1)).toFixed(2)} s/f\n`);
    }
  }
  ff.stdin.end();
  await new Promise((res, rej) => ff.on('close', c => (c === 0 ? res() : rej(new Error(`ffmpeg ${c}`)))));
  fs.writeFileSync(`${out}.done`, String(f1 - f0)); // resume marker: this segment is complete
}

async function video({ scale, fps, mb, workers, lossless, outFile, from = 0, to = DURATION, resume = false }) {
  const dir = path.join(CACHE, 'segments', `${scale}x_${fps}`);
  if (!resume) fs.rmSync(dir, { recursive: true, force: true });
  mkdir(dir);
  const F0 = Math.round(from * fps), F1 = Math.round(to * fps);
  // interleave uneven work: many chunks, pulled by a worker pool
  const chunk = Math.max(15, Math.round(fps / 2));
  const jobs = [];
  for (let f = F0; f < F1; f += chunk) jobs.push([f, Math.min(F1, f + chunk)]);
  const ext = lossless ? 'mkv' : 'mp4';
  let next = 0;
  const t0 = performance.now();
  const doneAlready = k => resume && fs.existsSync(path.join(dir, `seg_${String(k).padStart(4, '0')}.${ext}.done`));
  const runOne = () => new Promise((res, rej) => {
    const loop = () => {
      while (next < jobs.length && doneAlready(next)) next++;
      if (next >= jobs.length) return res();
      const k = next++;
      const [a, b] = jobs[k];
      const out = path.join(dir, `seg_${String(k).padStart(4, '0')}.${ext}`);
      const p = spawn(process.execPath, [path.join(HERE, 'render.mjs'), 'segment', '--from', a, '--to', b, '--scale', scale, '--fps', fps, '--out', out, ...(mb ? ['--mb'] : []), ...(lossless ? ['--lossless'] : [])], { stdio: ['ignore', 'ignore', 'inherit'] });
      p.on('close', c => {
        if (c !== 0) return rej(new Error(`segment ${a}-${b} failed`));
        const done = next;
        const el = (performance.now() - t0) / 1000;
        process.stdout.write(`\r${Math.min(done, jobs.length)}/${jobs.length} chunks · ${el.toFixed(0)} s`);
        loop();
      });
    };
    loop();
  });
  await Promise.all(Array.from({ length: workers }, runOne));
  process.stdout.write('\n');
  const list = path.join(dir, 'list.txt');
  fs.writeFileSync(list, jobs.map((_, k) => `file 'seg_${String(k).padStart(4, '0')}.${ext}'`).join('\n'));
  const joined = outFile || path.join(dir, `joined.${ext}`);
  execFileSync('ffmpeg', ['-y', '-hide_banner', '-loglevel', 'error', '-f', 'concat', '-safe', '0', '-i', list, '-c', 'copy', joined]);
  return joined;
}

function audio() {
  mkdir(path.join(CACHE, 'audio'));
  const tl = path.join(CACHE, 'timeline.json');
  fs.writeFileSync(tl, JSON.stringify({ fps: FPS, duration: DURATION, bpm: BPM, T, sections: SECTIONS, cues: soundCues() }, null, 1));
  const wav = path.join(CACHE, 'audio', 'score.wav');
  execFileSync('python3', [path.join(HERE, 'audio', 'score.py'), tl, wav], { stdio: 'inherit' });
  return wav;
}

// Loudness-normalized AAC mux. With a poster still, frame 0 becomes that
// still: platforms take frame 0 as the idle thumbnail, and replacing it
// (not adding a frame) keeps the frame count and the sync.
function mux(videoIn, wav, out, vf, venc, poster = null) {
  const video = poster
    ? ['-i', poster, '-filter_complex', `[0:v]${vf}[v];[2:v]${vf}[p];[v][p]overlay=0:0:enable='eq(n,0)'[o]`, '-map', '[o]']
    : [...(vf ? ['-vf', vf] : []), '-map', '0:v:0'];
  execFileSync('ffmpeg', ['-y', '-hide_banner', '-loglevel', 'error', '-i', videoIn, '-i', wav, ...video, ...venc,
    '-af', 'loudnorm=I=-14:TP=-1.5:LRA=11', '-ar', '48000', '-c:a', 'aac', '-b:a', '256k',
    '-map', '1:a:0', '-t', String(DURATION), '-movflags', '+faststart', out], { stdio: 'inherit' });
  console.log('wrote', out);
}

// Frames chosen for the poster, the hero thumbnail and the QA stills.
const POSTER_T = 25.9; // €228.00 + PROOF + VERIFIED: the payoff, settled and readable alone
const HERO_T = 13.62; // the detector ring sweeping the plan
const QA_T = [0.3, 1.8, 2.45, 3.3, 4.02, 4.6, 5.9, 7.9, 9.6, 11.2, 13.62, 15.3, 16.12, 17.8, 19.2, 20.1, 21.6, 22.1, 23.6, 24.05, 25.9, 27.6, 29.2];

// Committed exports + QA stills, all derived from the 4K master.
function exportsFromMaster() {
  const dist = path.join(CACHE, 'dist');
  const master = path.join(dist, 'intent-to-proof-4k60.mp4');
  if (!fs.existsSync(master)) throw new Error('render the master first: npm run master');
  const ff = a => execFileSync('ffmpeg', ['-y', '-hide_banner', '-loglevel', 'error', ...a], { stdio: 'inherit' });
  const media = p => path.join(REPO, 'media', p);
  const poster = path.join(dist, 'poster-3840x2160.png');
  ff(['-ss', String(POSTER_T), '-i', master, '-frames:v', '1', poster]);
  // web cut for README/docs: 1600×900 at 30 fps like the other films, under
  // 8 MB, the poster as frame 0. Downscaled in 16 bits and finished again,
  // so the master's dither averages into a smooth ramp instead of new
  // steps. Its sound comes straight from the score (one AAC generation),
  // with 2 dB of true-peak headroom for the 128k encode.
  const web = `format=yuv444p16le,scale=1600:900:flags=lanczos,${finish(24)}`;
  const wav = path.join(CACHE, 'audio', 'score.wav');
  ff(['-i', master, '-i', poster, '-i', wav, '-filter_complex', `[0:v]fps=30,${web}[v];[1:v]${web}[p];[v][p]overlay=0:0:enable='eq(n,0)'[o]`,
    '-map', '[o]', '-map', '2:a:0', '-c:v', 'libx264', '-preset', 'slow', '-crf', '23', '-x264-params', 'aq-mode=3',
    '-pix_fmt', 'yuv420p', '-profile:v', 'high', '-af', 'loudnorm=I=-14:TP=-2:LRA=11', '-ar', '48000', '-c:a', 'aac', '-b:a', '128k',
    '-t', String(DURATION), '-movflags', '+faststart', media('videos/intent-to-proof.mp4')]);
  // poster (1600×900, the README budget is 1 MB) and the storyboard contact sheet
  ff(['-ss', String(POSTER_T), '-i', master, '-frames:v', '1', '-vf', 'scale=1600:900:flags=lanczos', media('posters/intent-to-proof.png')]);
  ff(['-i', master, '-vf', 'fps=12/30,scale=480:270:flags=lanczos,tile=4x3', '-frames:v', '1', media('storyboards/intent-to-proof.png')]);
  // hero thumbnail + QA stills (not committed)
  mkdir(path.join(dist, 'stills'));
  ff(['-ss', String(HERO_T), '-i', master, '-frames:v', '1', '-vf', 'scale=1920:1080:flags=lanczos', path.join(dist, 'hero-1920x1080.png')]);
  // the thumbnail to upload where a platform accepts one, and the caption
  ff(['-i', poster, '-vf', 'scale=1920:1080:flags=lanczos', '-q:v', '2', path.join(dist, 'intent-to-proof-poster.jpg')]);
  fs.copyFileSync(path.join(HERE, 'share-copy.txt'), path.join(dist, 'share-copy.txt'));
  for (const t of QA_T) ff(['-ss', String(t), '-i', master, '-frames:v', '1', '-vf', 'scale=1920:1080:flags=lanczos', path.join(dist, 'stills', `t${t.toFixed(2)}.png`)]);
  for (const f of ['videos/intent-to-proof.mp4', 'posters/intent-to-proof.png', 'storyboards/intent-to-proof.png']) {
    console.log(f, (fs.statSync(media(f)).size / 1e6).toFixed(2), 'MB');
  }
}

// Final grade for 8-bit delivery. Skia quantizes the dark gradients to 8
// bits, which shows as contour rings on good displays. deband smooths those
// one-level steps in 16-bit precision (range in output pixels; contrast above
// about one level is kept), then an ordered dither and a static luma grain
// carry the smooth ramp back into 8 bits.
const finish = range =>
  `format=yuv444p16le,deband=1thr=0.004:2thr=0.004:3thr=0.004:range=${range}:blur=1,scale=sws_dither=a_dither,format=yuv444p,noise=c0s=3:c0f=u`;

async function main() {
  if (cmd === 'still') return still([+args[1]], +opt('scale', 1), !!opt('mb'));
  if (cmd === 'stills') return still(args[1].split(',').map(Number), +opt('scale', 0.5), !!opt('mb'));
  if (cmd === 'sheet') return sheet(+args[1], +args[2], +args[3], +opt('scale', 0.25), +opt('cols', 4));
  if (cmd === 'segment') return segment();
  if (cmd === 'audio') return console.log(audio());
  if (cmd === 'video') {
    const scale = +opt('scale', 0.5), fps = +opt('fps', 30), workers = +opt('workers', os.cpus().length);
    const joined = await video({ scale, fps, mb: !!opt('mb'), workers, lossless: false, from: +opt('from', 0), to: +opt('to', DURATION) });
    const wav = audio();
    const out = path.resolve(opt('out', path.join(CACHE, `preview_${scale}x_${fps}.mp4`)));
    mux(joined, wav, out, null, ['-c:v', 'copy']);
    return;
  }
  if (cmd === 'master') {
    const workers = +opt('workers', os.cpus().length);
    const joined = await video({ scale: 2, fps: FPS, mb: true, workers, lossless: true, outFile: path.join(CACHE, 'master_lossless.mkv'), resume: !!opt('resume') });
    const wav = audio();
    const dist = path.join(CACHE, 'dist');
    mkdir(dist);
    // the master stays pure; the X cut carries the poster as frame 0
    mux(joined, wav, path.join(dist, 'intent-to-proof-4k60.mp4'), finish(64),
      ['-c:v', 'libx264', '-preset', 'slow', '-crf', '13', '-pix_fmt', 'yuv420p', '-profile:v', 'high', '-level', '5.2', '-tune', 'grain', '-x264-params', 'aq-mode=3']);
    const still = path.join(CACHE, 'poster-master.png');
    execFileSync('ffmpeg', ['-y', '-hide_banner', '-loglevel', 'error', '-ss', String(POSTER_T), '-i', joined, '-frames:v', '1', still]);
    mux(joined, wav, path.join(dist, 'intent-to-proof-x-1080p60.mp4'), `format=yuv444p16le,scale=1920:1080:flags=lanczos,${finish(32)}`,
      ['-c:v', 'libx264', '-preset', 'slow', '-crf', '16', '-maxrate', '24M', '-bufsize', '48M', '-pix_fmt', 'yuv420p', '-profile:v', 'high', '-level', '4.2', '-x264-params', 'aq-mode=3'], still);
    return;
  }
  if (cmd === 'exports') return exportsFromMaster();
  console.log('usage: still <t> | stills <t,t> | sheet <a> <b> <n> | video | master | exports | audio');
}

main().catch(e => {
  console.error(e);
  process.exit(1);
});
