#!/usr/bin/env node
/* render-motion.mjs — deterministic frame renderer for the Nika motion scenes.
 *
 * Steps a scene's timeline frame-by-frame in headless Chrome (system install,
 * zero browser download), captures PNG frames, then assembles MP4 + WebM +
 * optimized GIF + poster with ffmpeg.
 *
 *   node render-motion.mjs static-check-fix            # full export
 *   node render-motion.mjs chat-to-workflow dag-execution
 *   node render-motion.mjs static-check-fix --frame 5600   # one frame → stdout path
 *   node render-motion.mjs static-check-fix --keep-frames
 *
 * Scene contract (set by scene.js): window.__scene {name,duration,fps,width,
 * height,posterAt} + window.__seek(ms) applying the complete state for t.
 */
import { chromium } from "playwright-core";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "../..");
const MEDIA = path.join(ROOT, "media");

const args = process.argv.slice(2);
const VALUED = new Set(["--frame", "--fps"]);
const optValues = new Set(args.filter((a, i) => i > 0 && VALUED.has(args[i - 1])));
const scenes = args.filter((a) => !a.startsWith("--") && !optValues.has(a));
const flag = (name) => args.includes(`--${name}`);
const opt = (name) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : null;
};
if (scenes.length === 0) {
  console.error("usage: node render-motion.mjs <scene>... [--frame <ms>] [--keep-frames]");
  process.exit(1);
}

const transcriptsPath = path.join(MEDIA, "raw", "transcripts.json");
const transcripts = fs.existsSync(transcriptsPath)
  ? fs.readFileSync(transcriptsPath, "utf8")
  : "{}";

const ff = (fargs) => execFileSync("ffmpeg", ["-hide_banner", "-loglevel", "error", ...fargs], { stdio: "inherit" });
const mb = (p) => (fs.statSync(p).size / 1024 / 1024).toFixed(2);

const browser = await chromium.launch({ channel: "chrome", headless: true });

for (const scene of scenes) {
  const htmlPath = path.join(HERE, "motion", `${scene}.html`);
  if (!fs.existsSync(htmlPath)) {
    console.error(`no such scene: ${htmlPath}`);
    process.exitCode = 1;
    continue;
  }

  const page = await browser.newPage({ viewport: { width: 1600, height: 900 }, deviceScaleFactor: 1 });
  await page.addInitScript(`window.NIKA_DATA = ${transcripts};`);
  await page.goto(`file://${htmlPath}?render=1`);
  await page.waitForFunction(() => window.__scene && window.__seek);
  await page.waitForTimeout(300); // fonts settle
  const meta = await page.evaluate(() => window.__scene);
  const clip = { x: 0, y: 0, width: meta.width, height: meta.height };
  await page.setViewportSize({ width: meta.width, height: meta.height });

  // single-frame inspection mode
  const frameAt = opt("frame");
  if (frameAt !== null) {
    await page.evaluate((t) => window.__seek(Number(t)), frameAt);
    const out = path.join(process.env.TMPDIR || "/tmp", `${scene}-${frameAt}.png`);
    await page.screenshot({ path: out, clip });
    console.log(out);
    await page.close();
    continue;
  }

  // Geometry gate — nothing drawn may escape its card or the stage.
  // Layout is static across the timeline (the animations move opacity and
  // transforms declared at the same laid-out positions), so one pass at
  // t=duration judges every frame. A line wider than its card is exactly
  // the class a human eye misses in a moving gif and a reader sees at
  // once in the README — the renderer refuses to paint it.
  await page.evaluate((t) => window.__seek(Number(t)), meta.duration);
  const overflows = await page.evaluate(() => {
    const bad = [];
    const tol = 2.5;
    const stage = document.querySelector("#stage") || document.body;
    const stageR = stage.getBoundingClientRect();
    const cards = Array.from(document.querySelectorAll(".card, .buf, #problems, .win, .node"));
    const leaves = Array.from(document.querySelectorAll("body *")).filter(
      (el) =>
        el.children.length === 0 &&
        el.textContent.trim().length > 0 &&
        getComputedStyle(el).display !== "none",
    );
    for (const el of leaves) {
      const r = el.getBoundingClientRect();
      if (r.width === 0 && r.height === 0) continue;
      const card = cards.find((c) => c !== el && c.contains(el));
      const box = card ? card.getBoundingClientRect() : stageR;
      const name = card ? (card.id || card.className.split(" ")[0]) : "stage";
      if (r.right > box.right + tol || r.bottom > box.bottom + tol) {
        bad.push(
          `${name} < "${el.textContent.trim().slice(0, 48)}" · ` +
            `right +${Math.max(0, r.right - box.right).toFixed(0)}px · ` +
            `bottom +${Math.max(0, r.bottom - box.bottom).toFixed(0)}px`,
        );
      }
    }
    return bad;
  });
  if (overflows.length > 0) {
    console.error(`✖ ${scene}: drawn text escapes its frame — refuse to paint`);
    for (const o of overflows) console.error(`   ${o}`);
    process.exitCode = 1;
    await page.close();
    continue;
  }

  const fps = Number(opt("fps") ?? meta.fps ?? 30);
  const gifFps = Number(meta.gifFps ?? 16);
  const gifWidth = Number(meta.gifWidth ?? 1280);
  const gifColors = Number(meta.gifColors ?? 192);
  const frames = Math.ceil((meta.duration / 1000) * fps);
  const framesDir = path.join(MEDIA, ".frames", scene);
  fs.rmSync(framesDir, { recursive: true, force: true });
  fs.mkdirSync(framesDir, { recursive: true });

  process.stdout.write(`▸ ${scene} · ${meta.duration}ms · ${frames} frames @ ${fps}fps `);
  const t0 = Date.now();
  for (let f = 0; f <= frames; f++) {
    await page.evaluate((t) => window.__seek(Number(t)), (f * 1000) / fps);
    await page.screenshot({ path: path.join(framesDir, `f${String(f).padStart(5, "0")}.png`), clip });
    if (f % Math.ceil(frames / 10) === 0) process.stdout.write(".");
  }
  console.log(` ${(Date.now() - t0) / 1000 | 0}s`);

  for (const d of ["videos", "gifs", "posters"]) fs.mkdirSync(path.join(MEDIA, d), { recursive: true });
  const input = ["-framerate", String(fps), "-i", path.join(framesDir, "f%05d.png")];
  const mp4 = path.join(MEDIA, "videos", `${scene}.mp4`);
  const webm = path.join(MEDIA, "videos", `${scene}.webm`);
  const gif = path.join(MEDIA, "gifs", `${scene}.optimized.gif`);
  const poster = path.join(MEDIA, "posters", `${scene}.png`);

  ff(["-y", ...input, "-c:v", "libx264", "-crf", "20", "-preset", "slow", "-pix_fmt", "yuv420p", "-movflags", "+faststart", "-an", mp4]);
  ff(["-y", ...input, "-c:v", "libvpx-vp9", "-b:v", "0", "-crf", "40", "-row-mt", "1", "-an", webm]);
  ff(["-y", "-i", mp4, "-vf",
    `fps=${gifFps},scale=${gifWidth}:-1:flags=lanczos,split[s0][s1];[s0]palettegen=max_colors=${gifColors}[p];[s1][p]paletteuse=dither=bayer:bayer_scale=4`,
    gif]);

  // poster = the exact posterAt frame
  await page.evaluate((t) => window.__seek(Number(t)), meta.posterAt ?? 0);
  await page.screenshot({ path: poster, clip });

  console.log(`  mp4 ${mb(mp4)}MB · webm ${mb(webm)}MB · gif ${mb(gif)}MB · poster ${mb(poster)}MB`);
  if (!flag("keep-frames")) fs.rmSync(framesDir, { recursive: true, force: true });
  await page.close();
}

await browser.close();
