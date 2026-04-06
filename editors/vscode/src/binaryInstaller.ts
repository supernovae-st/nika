// binaryInstaller.ts — Binary download, verification, and extraction
//
// Handles discovering, downloading, and validating the Nika binary from GitHub releases.
// Pure functions with no module-level state — all dependencies passed as parameters.

import { window, ProgressLocation, ExtensionContext } from 'vscode';
import { execFile } from 'child_process';
import * as https from 'https';
import * as fs from 'fs';
import * as path from 'path';
import * as crypto from 'crypto';
import * as zlib from 'zlib';
import { IncomingMessage } from 'http';

const GITHUB_RELEASES_API = 'https://api.github.com/repos/supernovae-st/nika/releases/latest';
export const GITHUB_INSTALL_URL = 'https://github.com/supernovae-st/nika#installation';

/** Maps process.platform + process.arch to a GitHub release artifact prefix. */
export function getArtifactName(): string | null {
  const { platform, arch } = process;
  if (platform === 'darwin' && arch === 'arm64') {
    return 'nika-macos-arm64';
  }
  if (platform === 'darwin' && arch === 'x64') {
    return 'nika-macos-x64';
  }
  if (platform === 'linux' && arch === 'x64') {
    return 'nika-linux-x64';
  }
  if (platform === 'linux' && arch === 'arm64') {
    return 'nika-linux-arm64';
  }
  if (platform === 'win32' && arch === 'x64') {
    return 'nika-windows-x64';
  }
  return null;
}

/** Follows HTTP redirects (GitHub redirects asset downloads). */
function httpGet(url: string): Promise<IncomingMessage> {
  return new Promise((resolve, reject) => {
    const request = (targetUrl: string, redirectsLeft: number): void => {
      https.get(targetUrl, { headers: { 'User-Agent': 'vscode-nika-extension' } }, (res) => {
        if (
          (res.statusCode === 301 || res.statusCode === 302 || res.statusCode === 307 || res.statusCode === 308)
          && res.headers.location
          && redirectsLeft > 0
        ) {
          res.resume();
          request(res.headers.location, redirectsLeft - 1);
          return;
        }
        resolve(res);
      }).on('error', reject);
    };
    request(url, 5);
  });
}

/** Reads the full body of an HTTP response as a string. */
function readBody(res: IncomingMessage): Promise<string> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    res.on('data', (chunk: Buffer) => chunks.push(chunk));
    res.on('end', () => resolve(Buffer.concat(chunks).toString('utf-8')));
    res.on('error', reject);
  });
}

/** Downloads a URL to a file path, streaming directly to disk. */
function downloadToFile(url: string, destPath: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(destPath);
    const cleanup = (err: Error): void => {
      file.destroy();
      fs.unlink(destPath, () => undefined);
      reject(err);
    };

    const request = (targetUrl: string, redirectsLeft: number): void => {
      https.get(targetUrl, { headers: { 'User-Agent': 'vscode-nika-extension' } }, (res) => {
        if (
          (res.statusCode === 301 || res.statusCode === 302 || res.statusCode === 307 || res.statusCode === 308)
          && res.headers.location
          && redirectsLeft > 0
        ) {
          res.resume();
          request(res.headers.location, redirectsLeft - 1);
          return;
        }
        if (res.statusCode !== 200) {
          cleanup(new Error(`HTTP ${res.statusCode} downloading binary`));
          return;
        }
        res.pipe(file);
        file.on('finish', () => file.close(() => resolve()));
        file.on('error', cleanup);
      }).on('error', cleanup);
    };
    request(url, 5);
  });
}

/**
 * Extracts the `nika` binary from a .tar.gz archive.
 * The archive layout is: `{artifactName}-{version}/nika`
 * Uses a streaming state machine: decompress -> parse 512-byte TAR blocks -> write target entry.
 */
function extractBinaryFromTarGz(archivePath: string, destPath: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const output = fs.createWriteStream(destPath);
    let resolved = false;

    const finish = (err?: Error): void => {
      if (resolved) { return; }
      resolved = true;
      output.close();
      if (err) {
        fs.unlink(destPath, () => undefined);
        reject(err);
      } else {
        resolve();
      }
    };

    // TAR state machine: header -> copy/skip -> header -> ...
    // Each TAR block is 512 bytes. Data sections are padded to 512-byte boundaries.
    type TarState = 'header' | 'copy' | 'skip';
    let tarState: TarState = 'header';
    let tarSkipBlocks = 0;  // 512-byte blocks left to skip for current non-target entry
    let tarCopyBlocks = 0;  // 512-byte blocks left to consume for the target entry
    let tarWriteBytes = 0;  // real bytes left to write (excludes padding)
    let found = false;

    const buf: Buffer[] = [];
    let bufLen = 0;

    const consumeBlocks = (): void => {
      while (bufLen >= 512) {
        const full = Buffer.concat(buf, bufLen);
        const block = Buffer.from(full.subarray(0, 512));
        const after = full.subarray(512);
        buf.length = 0;
        if (after.length > 0) { buf.push(Buffer.from(after)); }
        bufLen -= 512;

        switch (tarState) {
          case 'skip':
            tarSkipBlocks--;
            if (tarSkipBlocks === 0) { tarState = 'header'; }
            break;

          case 'copy': {
            const toWrite = Math.min(tarWriteBytes, 512);
            if (toWrite > 0 && !resolved) {
              output.write(block.subarray(0, toWrite));
              tarWriteBytes -= toWrite;
            }
            tarCopyBlocks--;
            if (tarCopyBlocks === 0) {
              tarState = 'header';
              found = true;
              finish();
              return;
            }
            break;
          }

          case 'header': {
            const entryName = block.toString('utf-8', 0, 100).replace(/\0/g, '');
            if (!entryName) { break; } // null block (end of archive)

            const sizeOctal = block.toString('utf-8', 124, 136).replace(/\0/g, '').trim();
            const entryBytes = parseInt(sizeOctal, 8) || 0;
            const flag = block[156];
            // typeflag '0' (0x30) or NUL (0x00) = regular file
            const isReg = flag === 0x30 || flag === 0x00;
            const base = entryName.split('/').pop() ?? '';
            const isTarget = isReg && (base === 'nika' || base === 'nika.exe');

            if (isTarget && entryBytes > 0) {
              tarState = 'copy';
              tarCopyBlocks = Math.ceil(entryBytes / 512);
              tarWriteBytes = entryBytes;
            } else if (entryBytes > 0) {
              tarState = 'skip';
              tarSkipBlocks = Math.ceil(entryBytes / 512);
            }
            // entryBytes === 0: directory or empty entry — stay in 'header' state
            break;
          }
        }
      }
    };

    const decompressed = zlib.createGunzip();
    const src = fs.createReadStream(archivePath);

    src.on('error', finish);
    decompressed.on('error', finish);
    output.on('error', finish);

    decompressed.on('data', (chunk: Buffer) => {
      buf.push(Buffer.from(chunk));
      bufLen += chunk.length;
      consumeBlocks();
    });

    decompressed.on('end', () => {
      if (!found && !resolved) {
        finish(new Error('nika binary not found in archive'));
      }
    });

    src.pipe(decompressed);
  });
}

/**
 * Extracts the `nika.exe` binary from a .zip archive.
 * Finds the entry ending in `/nika.exe` and writes it to destPath.
 * Uses a pure-JS ZIP parser (no dependencies).
 */
function extractBinaryFromZip(archivePath: string, destPath: string): Promise<void> {
  const MAX_ZIP_SIZE = 500 * 1024 * 1024; // 500 MB

  return new Promise((resolve, reject) => {
    fs.stat(archivePath, (statErr, stats) => {
      if (statErr) { reject(statErr); return; }
      if (stats.size > MAX_ZIP_SIZE) {
        reject(new Error(
          `ZIP archive too large: ${stats.size} bytes exceeds ${MAX_ZIP_SIZE} byte limit`
        ));
        return;
      }

      fs.readFile(archivePath, (readErr, data) => {
        if (readErr) { reject(readErr); return; }

      // Locate End of Central Directory record (EOCD): signature 0x06054b50
      let eocdOffset = -1;
      for (let i = data.length - 22; i >= 0; i--) {
        if (data[i] === 0x50 && data[i + 1] === 0x4b && data[i + 2] === 0x05 && data[i + 3] === 0x06) {
          eocdOffset = i;
          break;
        }
      }
      if (eocdOffset === -1) { reject(new Error('Invalid ZIP: no EOCD')); return; }

      const cdOffset = data.readUInt32LE(eocdOffset + 16);
      const cdSize = data.readUInt32LE(eocdOffset + 12);

      let pos = cdOffset;
      const cdEnd = cdOffset + cdSize;
      let found = false;

      while (pos < cdEnd) {
        // Central directory file header signature: 0x02014b50
        if (
          data[pos] !== 0x50 || data[pos + 1] !== 0x4b ||
          data[pos + 2] !== 0x01 || data[pos + 3] !== 0x02
        ) {
          break;
        }
        const compMethod = data.readUInt16LE(pos + 10);
        const compSize = data.readUInt32LE(pos + 20);
        const uncompSize = data.readUInt32LE(pos + 24);
        const fnLen = data.readUInt16LE(pos + 28);
        const extraLen = data.readUInt16LE(pos + 30);
        const commentLen = data.readUInt16LE(pos + 32);
        const localHeaderOffset = data.readUInt32LE(pos + 42);
        const fileName = data.toString('utf-8', pos + 46, pos + 46 + fnLen);

        const base = fileName.split('/').pop() ?? '';
        if (base === 'nika.exe') {
          // Read local file header to find actual data offset
          const localPos = localHeaderOffset;
          const localFnLen = data.readUInt16LE(localPos + 26);
          const localExtraLen = data.readUInt16LE(localPos + 28);
          const dataOffset = localPos + 30 + localFnLen + localExtraLen;

          let fileData: Buffer;
          if (compMethod === 0) {
            // Stored (no compression)
            fileData = data.subarray(dataOffset, dataOffset + uncompSize);
          } else if (compMethod === 8) {
            // Deflate
            const compressed = data.subarray(dataOffset, dataOffset + compSize);
            try {
              fileData = zlib.inflateRawSync(compressed);
            } catch (e) {
              reject(new Error(`ZIP inflate error: ${e}`));
              return;
            }
          } else {
            reject(new Error(`Unsupported ZIP compression method: ${compMethod}`));
            return;
          }

          fs.writeFile(destPath, fileData, (writeErr) => {
            if (writeErr) { reject(writeErr); } else { resolve(); }
          });
          found = true;
          break;
        }

        pos += 46 + fnLen + extraLen + commentLen;
      }

      if (!found) {
        reject(new Error('nika.exe not found in ZIP archive'));
      }
    });
    });
  });
}

/**
 * Downloads the latest nika binary from GitHub releases.
 * Returns the path to the downloaded binary, or null on failure.
 */
export async function downloadNikaBinary(storagePath: string): Promise<string | null> {
  const artifactName = getArtifactName();
  if (!artifactName) {
    return null;
  }

  const isWindows = process.platform === 'win32';
  const binaryName = isWindows ? 'nika.exe' : 'nika';
  const binaryDest = path.join(storagePath, binaryName);

  return window.withProgress(
    {
      location: ProgressLocation.Notification,
      title: 'Nika: Downloading language server...',
      cancellable: false,
    },
    async (progress) => {
      try {
        progress.report({ message: 'Fetching release info from GitHub...' });

        // Fetch latest release metadata
        const apiRes = await httpGet(GITHUB_RELEASES_API);
        if (apiRes.statusCode !== 200) {
          throw new Error(`GitHub API returned HTTP ${apiRes.statusCode}`);
        }
        const body = await readBody(apiRes);
        const release = JSON.parse(body) as {
          tag_name: string;
          assets: Array<{ name: string; browser_download_url: string }>;
        };

        const version = release.tag_name.replace(/^v/, '');
        const archiveExt = isWindows ? '.zip' : '.tar.gz';
        const archiveName = `${artifactName}-${version}${archiveExt}`;
        const asset = release.assets.find((a) => a.name === archiveName);

        if (!asset) {
          throw new Error(`No asset named '${archiveName}' in release ${release.tag_name}`);
        }

        progress.report({ message: `Downloading ${archiveName}...` });

        // Ensure storage directory exists
        fs.mkdirSync(storagePath, { recursive: true });

        const archiveDest = path.join(storagePath, archiveName);
        await downloadToFile(asset.browser_download_url, archiveDest);

        // SHA256 checksum verification
        progress.report({ message: 'Verifying checksum...' });
        const checksumName = `${archiveName}.sha256`;
        const checksumAsset = release.assets.find((a) => a.name === checksumName);
        if (checksumAsset) {
          const checksumRes = await httpGet(checksumAsset.browser_download_url);
          if (checksumRes.statusCode === 200) {
            const checksumBody = await readBody(checksumRes);
            // Format: "<hash>  <filename>"
            const expectedHash = checksumBody.trim().split(/\s+/)[0].toLowerCase();

            const fileBuffer = fs.readFileSync(archiveDest);
            const actualHash = crypto.createHash('sha256').update(fileBuffer).digest('hex');

            if (actualHash !== expectedHash) {
              fs.unlinkSync(archiveDest);
              throw new Error(
                `SHA256 mismatch for ${archiveName}: expected ${expectedHash}, got ${actualHash}`
              );
            }
          } else {
            console.warn(`Nika: checksum file returned HTTP ${checksumRes.statusCode}, skipping verification`);
            checksumRes.resume();
          }
        } else {
          console.warn('Nika: no .sha256 checksum file in release, skipping verification');
        }

        progress.report({ message: 'Extracting binary...' });

        if (isWindows) {
          await extractBinaryFromZip(archiveDest, binaryDest);
        } else {
          await extractBinaryFromTarGz(archiveDest, binaryDest);
          fs.chmodSync(binaryDest, 0o755);
        }

        // Clean up archive
        fs.unlink(archiveDest, () => undefined);

        progress.report({ message: 'Done.' });
        return binaryDest;
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        void message; // handled by caller
        throw err;
      }
    },
  );
}

/** Checks if the binary at the given path is functional. */
export function isBinaryWorking(binaryPath: string): Promise<boolean> {
  return new Promise((resolve) => {
    execFile(binaryPath, ['--version'], { timeout: 5000 }, (error) => {
      resolve(!error);
    });
  });
}

/** Check for bundled binary in platform-specific VSIX (rust-analyzer pattern). */
export function findBundledBinary(context: ExtensionContext): string | null {
  const binaryName = process.platform === 'win32' ? 'nika.exe' : 'nika';
  const bundled = path.join(context.extensionPath, 'server', binaryName);
  return fs.existsSync(bundled) ? bundled : null;
}
