'use strict';

const https = require('https');
const http = require('http');
const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');

const VERSION = require('./package.json').version;

const PLATFORM_MAP = {
  'darwin-arm64': 'macos-arm64',
  'darwin-x64': 'macos-x64',
  'linux-x64': 'linux-x64',
  'linux-arm64': 'linux-arm64',
};

function getPlatformKey() {
  const key = `${process.platform}-${process.arch}`;
  const mapped = PLATFORM_MAP[key];
  if (!mapped) {
    console.error(
      `Unsupported platform: ${key}\n` +
      'Supported: darwin-arm64, darwin-x64, linux-x64, linux-arm64\n' +
      'Install manually from https://github.com/supernovae-st/nika/releases'
    );
    process.exit(1);
  }
  return mapped;
}

function followRedirects(url, callback) {
  const client = url.startsWith('https') ? https : http;
  client.get(url, { headers: { 'User-Agent': 'npm-@supernovae-st/nika' } }, (res) => {
    if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
      followRedirects(res.headers.location, callback);
    } else if (res.statusCode !== 200) {
      callback(new Error(`Download failed: HTTP ${res.statusCode} from ${url}`));
    } else {
      callback(null, res);
    }
  }).on('error', callback);
}

function download(url, dest) {
  return new Promise((resolve, reject) => {
    followRedirects(url, (err, res) => {
      if (err) return reject(err);

      const file = fs.createWriteStream(dest);
      res.pipe(file);
      file.on('finish', () => {
        file.close(resolve);
      });
      file.on('error', (e) => {
        fs.unlink(dest, () => {});
        reject(e);
      });
    });
  });
}

async function install() {
  const platformKey = getPlatformKey();
  const tarballName = `nika-${platformKey}-${VERSION}.tar.gz`;
  const url = `https://github.com/supernovae-st/nika/releases/download/v${VERSION}/${tarballName}`;

  const binDir = path.join(__dirname, 'bin');
  const tarballPath = path.join(__dirname, tarballName);
  const binaryPath = path.join(binDir, 'nika');

  // Skip if binary already exists
  if (fs.existsSync(binaryPath)) {
    console.log('nika binary already installed.');
    return;
  }

  console.log(`Downloading nika v${VERSION} for ${platformKey}...`);
  console.log(`  ${url}`);

  await download(url, tarballPath);

  // Create bin directory
  fs.mkdirSync(binDir, { recursive: true });

  // Extract binary from tarball
  // Tarball structure: nika-PLATFORM-VERSION/nika
  try {
    execFileSync('tar', [
      'xzf', tarballPath,
      '--strip-components=1',
      '-C', binDir,
    ], { stdio: 'pipe' });
  } catch (e) {
    console.error('Failed to extract tarball:', e.message);
    cleanup(tarballPath);
    fallback();
    return;
  }

  // Ensure binary is executable
  try {
    fs.chmodSync(binaryPath, 0o755);
  } catch (e) {
    // chmod may fail on some systems, non-fatal
  }

  // Clean up tarball
  cleanup(tarballPath);

  console.log(`nika v${VERSION} installed successfully.`);

  // Post-install: trigger first-run setup + start daemon in background.
  // Detects editors, installs AI rules, shell completions, and starts daemon.
  // Non-fatal — setup retries on first `nika` command if this fails.
  try {
    const versionOutput = execFileSync(binaryPath, ['--version'], {
      stdio: 'pipe',
      timeout: 5000,
    });
    if (!versionOutput.toString().includes('nika')) {
      return; // Not a valid nika binary
    }
  } catch (_) {
    return; // Binary doesn't work, skip daemon
  }

  try {
    execFileSync(binaryPath, ['--quiet', 'daemon', 'start'], {
      stdio: 'ignore',
      timeout: 45000,
    });
  } catch (_) {
    // Silently ignore — setup will run on first nika command
  }
}

function cleanup(tarballPath) {
  try {
    fs.unlinkSync(tarballPath);
  } catch (e) {
    // Ignore cleanup errors
  }
}

function fallback() {
  console.error(
    '\nFailed to install nika binary.\n' +
    'You can install it manually from:\n' +
    `  https://github.com/supernovae-st/nika/releases/tag/v${VERSION}\n`
  );
  process.exit(1);
}

install().catch((err) => {
  console.error('Installation error:', err.message);
  fallback();
});
