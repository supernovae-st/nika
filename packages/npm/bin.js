#!/usr/bin/env node
'use strict';

const { execFileSync } = require('child_process');
const path = require('path');

const PLATFORM_PACKAGES = {
  'darwin-arm64': '@supernovae-st/nika-darwin-arm64',
  'darwin-x64': '@supernovae-st/nika-darwin-x64',
  'linux-x64': '@supernovae-st/nika-linux-x64',
  'linux-arm64': '@supernovae-st/nika-linux-arm64',
  'win32-x64': '@supernovae-st/nika-win32-x64',
};

function getBinaryPath() {
  const key = `${process.platform}-${process.arch}`;
  const pkg = PLATFORM_PACKAGES[key];

  if (!pkg) {
    console.error(
      `Unsupported platform: ${key}\n` +
      `Supported: ${Object.keys(PLATFORM_PACKAGES).join(', ')}\n` +
      'Install manually: https://github.com/supernovae-st/nika/releases'
    );
    process.exit(1);
  }

  const binaryName = process.platform === 'win32' ? 'nika.exe' : 'nika';

  try {
    // Resolve the platform package and find the binary inside it
    const pkgDir = path.dirname(require.resolve(`${pkg}/package.json`));
    return path.join(pkgDir, binaryName);
  } catch (e) {
    console.error(
      `Platform package ${pkg} not found.\n` +
      'This usually means your package manager did not install the optional dependency.\n' +
      'Try: npm install @supernovae-st/nika --force\n' +
      'Or install directly: https://github.com/supernovae-st/nika/releases'
    );
    process.exit(1);
  }
}

try {
  execFileSync(getBinaryPath(), process.argv.slice(2), { stdio: 'inherit' });
} catch (e) {
  process.exit(e.status || 1);
}
