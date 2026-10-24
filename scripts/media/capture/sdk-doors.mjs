#!/usr/bin/env node
// sdk-doors.mjs · capture what the sdk-doors clip shows, from the PUBLISHED
// TypeScript package (@supernovae-st/nika on npm) and the engine it bundles.
// Every version, verdict, event kind, hash and refusal code the clip draws is
// read from what this script writes; the clip refuses to render otherwise.
//
// Usage · node scripts/media/capture/sdk-doors.mjs [--version 0.123.0] [--keep]
//   SDK_DOORS_NPM_CACHE reuses an npm cache (the install still comes from the
//   registry, integrity-checked); --keep leaves the scratch directory.
// Output · media/raw/sdk-doors-*
//
// In a scratch directory, with a scratch HOME and file custody for keys
// (NIKA_KEYCHAIN=off), and with an environment built from nothing (no
// *_API_KEY, *_TOKEN or *SECRET* of the caller reaches a child):
//   1. npm installs @supernovae-st/nika@<version> from the registry into a
//      fresh app; hello.nika and demo.mts are the README's quick start, byte
//      for byte, from the package npm installed;
//   2. the package names the engine it resolved (the host payload), and the
//      payload is held to its INTEGRITY.json and SOURCE.json;
//   3. `nika key init` mints a signing key (scratch HOME), then local.mjs
//      drives new Nika({ cwd }): check() of the quick start and of a red file,
//      run() refusing the red file, run() of the quick start with its events,
//      result, receipt and traceVerify(), then one changed byte in the trace;
//   4. `nika serve` starts on a free loopback port with a 0600 token file
//      (never printed) and the same key, and remote.mjs drives
//      new Nika({ url, token, allowInsecureHttp: true }) against it.
// No path of this machine and no token may reach a capture.
import { spawn, spawnSync } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import fs from 'node:fs';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../../..');
const RAW = path.join(ROOT, 'media/raw');
const OUT = path.join(RAW, 'sdk-doors');
const args = process.argv.slice(2);
const opt = (name, def) => (args.includes(`--${name}`) ? args[args.indexOf(`--${name}`) + 1] : def);
const VERSION = opt('version', '0.123.0');
const PKG = '@supernovae-st/nika';
const fail = why => { throw new Error(`sdk-doors capture: ${why}`); };

// a scratch directory (SDK_DOORS_SCRATCH to choose where), removed at the end
const WORK = fs.mkdtempSync(path.join(process.env.SDK_DOORS_SCRATCH ?? os.tmpdir(), 'sdk-doors-'));
const HOME = path.join(WORK, 'home');
const APP = path.join(WORK, 'app');
const SERVE = path.join(WORK, 'serve');
for (const d of [HOME, APP, SERVE, path.join(WORK, 'tmp')]) fs.mkdirSync(d, { recursive: true });

// An environment built from nothing: the children see these names only.
const NODE_DIR = path.dirname(process.execPath);
const ENV = {
  HOME,
  PATH: `${NODE_DIR}:/usr/bin:/bin:/usr/sbin:/sbin`,
  LANG: 'en_US.UTF-8',
  TERM: 'xterm-256color',
  TMPDIR: path.join(WORK, 'tmp'),
  NIKA_KEYCHAIN: 'off',
  NPM_CONFIG_UPDATE_NOTIFIER: 'false',
  NPM_CONFIG_FUND: 'false',
  NPM_CONFIG_AUDIT: 'false',
  npm_config_cache: process.env.SDK_DOORS_NPM_CACHE ?? path.join(WORK, 'npm-cache'),
};
for (const k of Object.keys(ENV)) if (/_API_KEY$|_TOKEN$|SECRET/i.test(k)) fail(`the child environment names ${k}`);

const run = (cmd, argv, cwd, { allowFail = false } = {}) => {
  const r = spawnSync(cmd, argv, { cwd, env: ENV, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  if (r.error) throw r.error;
  if (r.status !== 0 && !allowFail) fail(`${path.basename(cmd)} ${argv.join(' ')} exited ${r.status}\n${r.stdout}\n${r.stderr}`);
  return r;
};
const sha256 = buf => createHash('sha256').update(buf).digest('hex');

// ── 1 · the published package, from the registry ──────────────────────────
fs.writeFileSync(path.join(APP, 'package.json'), '{\n  "name": "my-app",\n  "private": true,\n  "type": "module"\n}\n');
const install = run(path.join(NODE_DIR, 'npm'), ['install', `${PKG}@${VERSION}`, '--no-audit', '--no-fund'], APP);
const added = (install.stdout.match(/added (\d+) packages?/) ?? [])[1];
const PKG_DIR = path.join(APP, 'node_modules', PKG);
const pkg = JSON.parse(fs.readFileSync(path.join(PKG_DIR, 'package.json'), 'utf8'));
if (pkg.version !== VERSION) fail(`npm installed ${pkg.version}, not ${VERSION}`);

// the README's quick start, byte for byte, from the package npm installed
const readme = fs.readFileSync(path.join(PKG_DIR, 'README.md'), 'utf8').split('\n');
const block = (step, lang) => {
  const s = readme.findIndex(l => l.startsWith(step));
  if (s < 0) fail(`README: no "${step}"`);
  const open = readme.findIndex((l, i) => i > s && l === '```' + lang);
  const close = readme.findIndex((l, i) => i > open && l === '```');
  if (open < 0 || close < 0) fail(`README: no \`\`\`${lang} block after "${step}"`);
  return `${readme.slice(open + 1, close).join('\n')}\n`;
};
const hello = block('**2 · Write a workflow.**', 'yaml');
const demo = block('**3 · Run it and verify it from TypeScript.**', 'ts');
fs.writeFileSync(path.join(APP, 'hello.nika'), hello);
fs.writeFileSync(path.join(APP, 'demo.mts'), demo);
fs.copyFileSync(path.join(HERE, 'sdk-doors/list-files.nika'), path.join(APP, 'list-files.nika'));
for (const f of ['engine.mjs', 'local.mjs']) fs.copyFileSync(path.join(HERE, 'sdk-doors', f), path.join(APP, f));

// ── 2 · the engine the package resolves, held to its payload records ──────
const engine = JSON.parse(run(process.execPath, ['engine.mjs'], APP).stdout);
const BIN = engine.bin;
if (!engine.packageRoot) fail('the package resolved no host payload: this capture is about the bundled engine');
const versionLine = run(BIN, ['--version'], APP).stdout.trim();
const identity = JSON.parse(run(BIN, ['--sdk-identity'], APP).stdout);
const manifest = JSON.parse(fs.readFileSync(path.join(engine.packageRoot, 'package.json'), 'utf8'));
const integrity = JSON.parse(fs.readFileSync(path.join(engine.packageRoot, 'INTEGRITY.json'), 'utf8'));
const source = JSON.parse(fs.readFileSync(path.join(engine.packageRoot, 'SOURCE.json'), 'utf8'));
const binSha = sha256(fs.readFileSync(BIN));
const versions = {
  package: { name: pkg.name, version: pkg.version, npm_added: added ? +added : null, installed: fs.readdirSync(path.join(APP, 'node_modules/@supernovae-st')).sort() },
  engine: {
    source: 'bundled',
    version_line: versionLine,
    engineVersion: identity.engineVersion,
    buildSha: identity.buildSha,
    specSha: identity.specSha,
    machineProtocolVersion: identity.machineProtocolVersion,
    payload: {
      name: manifest.name,
      version: manifest.version,
      sha256: binSha,
      sha256_matches_integrity: binSha === integrity.executable?.sha256,
      release: { tag: source.tag, commit: source.commit },
    },
  },
  node: process.version,
  signing_key: 'nika key init in a scratch HOME (NIKA_KEYCHAIN=off: file custody)',
  captured: new Date().toISOString().slice(0, 10),
};

// ── 3 · the local door ─────────────────────────────────────────────────────
run(BIN, ['key', 'init'], APP);
const checkText = run(BIN, ['check', '--color', 'never', 'hello.nika'], APP).stdout;
const checkRedText = run(BIN, ['check', '--color', 'never', 'list-files.nika'], APP, { allowFail: true }).stdout;
const local = JSON.parse(run(process.execPath, ['local.mjs'], APP).stdout);

// ── 4 · the remote door: nika serve on a free loopback port ────────────────
const freePort = () => new Promise((res, rej) => {
  const srv = net.createServer();
  srv.once('error', rej);
  srv.listen(0, '127.0.0.1', () => { const { port } = srv.address(); srv.close(() => res(port)); });
});
const PORT = await freePort();
if (PORT >= 8787 && PORT <= 8789) fail('the free port is one other sessions use; run again');
fs.writeFileSync(path.join(SERVE, 'nika.yaml'), 'nika: my-project\n');
fs.writeFileSync(path.join(SERVE, 'hello.nika'), hello);
fs.copyFileSync(path.join(HERE, 'sdk-doors/remote.mjs'), path.join(SERVE, 'remote.mjs'));
fs.symlinkSync(path.join(APP, 'node_modules'), path.join(SERVE, 'node_modules'));
fs.mkdirSync(path.join(SERVE, '.nika'), { mode: 0o700 });
const TOKEN = randomBytes(24).toString('hex');
fs.writeFileSync(path.join(SERVE, '.nika/serve.token'), `${TOKEN}\n`, { mode: 0o600 });
fs.chmodSync(path.join(SERVE, '.nika/serve.token'), 0o600);
const URL_ = `http://127.0.0.1:${PORT}`;
const log = fs.openSync(path.join(WORK, 'serve.log'), 'w');
const server = spawn(BIN, ['serve', '--bind', `127.0.0.1:${PORT}`, '--workflows', '.', '--token-file', '.nika/serve.token', '--state-root', '.nika/serve'], { cwd: SERVE, env: ENV, stdio: ['ignore', log, log] });
const exited = new Promise(res => server.once('exit', (code, signal) => res({ code, signal })));
let health = null;
let remote = null;
try {
  const t0 = Date.now();
  while (!health) {
    if (Date.now() - t0 > 30000) fail(`nika serve did not answer /health within 30 s:\n${fs.readFileSync(path.join(WORK, 'serve.log'), 'utf8')}`);
    try {
      const r = await fetch(`${URL_}/health`);
      if (r.ok) health = await r.json();
    } catch {
      await new Promise(res => setTimeout(res, 250));
    }
  }
  remote = JSON.parse(run(process.execPath, ['remote.mjs', URL_], SERVE).stdout);
} finally {
  server.kill('SIGTERM');
  const stop = await Promise.race([exited, new Promise(res => setTimeout(() => res(null), 10000))]);
  if (!stop) server.kill('SIGKILL');
}
remote.health = {
  service: health.service,
  status: health.status,
  engineVersion: health.engineVersion,
  buildSha: health.buildSha,
  machineProtocolVersion: health.machineProtocolVersion,
  supportedCapabilities: health.supportedCapabilities,
};
remote.port = 'a free loopback port';

// ── what the clip will be held to ──────────────────────────────────────────
const files = {
  [`${OUT}-versions.json`]: `${JSON.stringify(versions, null, 1)}\n`,
  [`${OUT}-local.json`]: `${JSON.stringify(local, null, 1)}\n`,
  [`${OUT}-remote.json`]: `${JSON.stringify(remote, null, 1)}\n`,
  [`${OUT}-hello.nika`]: hello,
  [`${OUT}-demo.mts`]: demo,
  [`${OUT}-list-files.nika`]: fs.readFileSync(path.join(APP, 'list-files.nika'), 'utf8'),
  [`${OUT}-check.txt`]: checkText,
  [`${OUT}-check-red.txt`]: checkRedText,
};
// no path of this machine and no token may reach a capture
for (const [file, text] of Object.entries(files)) {
  for (const bad of [WORK, HOME, APP, SERVE, TOKEN, '/Users/', '/private/', '/var/folders/']) {
    if (text.includes(bad)) fail(`${path.basename(file)} carries a machine path or the token`);
  }
}
fs.mkdirSync(RAW, { recursive: true });
for (const [file, text] of Object.entries(files)) fs.writeFileSync(file, text);
if (!args.includes('--keep')) fs.rmSync(WORK, { recursive: true, force: true });
console.log(`sdk-doors captured · ${pkg.name} ${pkg.version} · ${versionLine} · ${manifest.name} · protocol ${identity.machineProtocolVersion}`);
