#!/usr/bin/env node
'use strict';

const path = require('path');
const fs = require('fs');
const { execFileSync } = require('child_process');

const binary = path.join(__dirname, 'bin', 'nika');

if (!fs.existsSync(binary)) {
  console.error(
    'nika binary not found.\n' +
    'Run: npm install @supernovae-st/nika\n'
  );
  process.exit(1);
}

try {
  execFileSync(binary, process.argv.slice(2), { stdio: 'inherit' });
} catch (e) {
  // execFileSync throws on non-zero exit codes — forward the exit code
  process.exit(e.status || 1);
}
