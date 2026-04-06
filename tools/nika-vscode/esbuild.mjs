// esbuild.mjs — Dual-target build: Node.js extension host + browser webview
//
// Usage:
//   node esbuild.mjs              # production build (both targets)
//   node esbuild.mjs --watch      # dev mode with rebuild on change
//   node esbuild.mjs --extension  # build extension only
//   node esbuild.mjs --webview    # build webview only

import * as esbuild from 'esbuild';
import { readFileSync } from 'fs';

const args = process.argv.slice(2);
const isWatch = args.includes('--watch');
const extensionOnly = args.includes('--extension');
const webviewOnly = args.includes('--webview');
const isDev = isWatch || args.includes('--dev');

const pkg = JSON.parse(readFileSync('./package.json', 'utf8'));

// Shared options
const shared = {
  bundle: true,
  minify: !isDev,
  sourcemap: isDev ? 'inline' : false,
  logLevel: 'info',
  define: {
    'process.env.NODE_ENV': isDev ? '"development"' : '"production"',
  },
};

// ─── Target 1: Extension host (Node.js) ─────────────────────────────────────
// Runs in VS Code's Node.js extension host process.
// Must use CommonJS (VS Code loads extensions via require()).
// External: vscode (provided by host), any native modules.
const extensionConfig = {
  ...shared,
  entryPoints: ['src/extension.ts'],
  outfile: 'dist/extension.js',
  platform: 'node',
  target: 'node18',
  format: 'cjs',
  external: [
    'vscode', // Provided by VS Code runtime — never bundle
  ],
  // Tree-shake unused code from dependencies
  treeShaking: true,
  // Node.js built-ins are available in extension host
  mainFields: ['module', 'main'],
  conditions: ['node', 'import', 'require'],
};

// ─── Target 2: Webview (Browser) ────────────────────────────────────────────
// Runs inside a sandboxed <iframe> in VS Code's webview.
// Must be a single self-contained IIFE — no imports at runtime.
// elkjs/optimized is the web-friendly build (no Worker, runs synchronously).
const webviewConfig = {
  ...shared,
  entryPoints: ['src/webview/dag.ts'],
  outfile: 'dist/webview/dag.js',
  platform: 'browser',
  target: 'es2020',
  format: 'iife',
  // Everything must be bundled — webview has no module loader
  external: [],
  // D3 has complex module structure — help esbuild resolve it
  mainFields: ['browser', 'module', 'main'],
  conditions: ['browser', 'import', 'default'],
  // Inject a global name so we can reference exports if needed
  globalName: 'NikaDag',
  // Alias elkjs to the bundled (no-worker) version for browser
  alias: {
    'elkjs': 'elkjs/lib/elk.bundled.js',
  },
  // Handle D3's circular dependencies gracefully
  logOverride: {
    'commonjs-variable-in-esm': 'silent',
  },
};

// ─── CSS for webview ─────────────────────────────────────────────────────────
const cssConfig = {
  ...shared,
  entryPoints: ['src/webview/dag.css'],
  outfile: 'dist/webview/dag.css',
  // esbuild handles CSS natively — just bundle + minify
  loader: { '.css': 'css' },
};

// ─── Build ───────────────────────────────────────────────────────────────────
async function build() {
  const targets = [];

  if (!webviewOnly) targets.push(extensionConfig);
  if (!extensionOnly) targets.push(webviewConfig, cssConfig);

  if (isWatch) {
    // Create contexts for incremental watch builds
    const contexts = await Promise.all(
      targets.map((config) => esbuild.context(config))
    );

    // Start watching all targets in parallel
    await Promise.all(contexts.map((ctx) => ctx.watch()));

    console.log('[nika-vscode] Watching for changes...');
    // Contexts stay alive — process runs until killed
  } else {
    // One-shot production build
    const results = await Promise.all(
      targets.map((config) => esbuild.build(config))
    );

    // Report bundle sizes
    for (let i = 0; i < targets.length; i++) {
      const outfile = targets[i].outfile;
      const meta = results[i].metafile;
      console.log(`[nika-vscode] Built: ${outfile}`);
    }
  }
}

build().catch((err) => {
  console.error(err);
  process.exit(1);
});
