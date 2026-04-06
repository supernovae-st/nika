// esbuild.mjs — Build the webview bundle (browser target)
//
// The extension itself is compiled by tsc (out/extension.js).
// This script only builds the DAG webview (browser IIFE).
//
// Usage:
//   node esbuild.mjs              # production build
//   node esbuild.mjs --watch      # dev mode with rebuild on change

import * as esbuild from 'esbuild';

const args = process.argv.slice(2);
const isWatch = args.includes('--watch');
const isDev = isWatch || args.includes('--dev');

// ─── Webview (Browser) ────────────────────────────────────────────────────
// Runs inside a sandboxed <iframe> in VS Code's webview.
// Must be a single self-contained IIFE — no imports at runtime.
const webviewConfig = {
  entryPoints: ['src/webview/dag.ts'],
  outfile: 'out/webview/dag.js',
  bundle: true,
  minify: !isDev,
  sourcemap: isDev ? 'inline' : false,
  platform: 'browser',
  target: 'es2020',
  format: 'iife',
  external: [],
  mainFields: ['browser', 'module', 'main'],
  conditions: ['browser', 'import', 'default'],
  globalName: 'NikaDag',
  alias: {
    'elkjs': 'elkjs/lib/elk.bundled.js',
  },
  logOverride: {
    'commonjs-variable-in-esm': 'silent',
  },
  logLevel: 'info',
};

// ─── CSS for webview ─────────────────────────────────────────────────────
const cssConfig = {
  entryPoints: ['src/webview/dag.css'],
  outfile: 'out/webview/dag.css',
  bundle: true,
  minify: !isDev,
  logLevel: 'info',
};

async function build() {
  if (isWatch) {
    const contexts = await Promise.all([
      esbuild.context(webviewConfig),
      esbuild.context(cssConfig),
    ]);
    await Promise.all(contexts.map((ctx) => ctx.watch()));
    console.log('[nika] Watching webview for changes...');
  } else {
    await Promise.all([
      esbuild.build(webviewConfig),
      esbuild.build(cssConfig),
    ]);
  }
}

build().catch((err) => {
  console.error(err);
  process.exit(1);
});
