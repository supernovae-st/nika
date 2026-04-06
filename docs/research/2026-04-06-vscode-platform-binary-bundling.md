# Research: VS Code Platform-Specific Binary Bundling

> Date: 2026-04-06
> Purpose: Technical implementation details for bundling Nika LSP binary inside a .vsix
> Sources: rust-analyzer, Deno, Go, clangd extension source code (GitHub, cloned at HEAD)

---

## Executive Summary

rust-analyzer is the gold standard for platform-specific binary bundling in VS Code extensions. They build 9 platform-specific `.vsix` files (15-20 MB each), each containing the LSP server binary at `server/rust-analyzer[.exe]`. The extension discovers it at runtime via `context.extensionUri` + `server/` path. VS Code Marketplace natively supports `--target` platform tags since late 2021, and the marketplace automatically serves the correct `.vsix` to each user. Open VSX also supports this pattern. Deno, Go, and clangd use a **different approach** (download-at-runtime), making rust-analyzer the only major reference for the bundled-binary pattern.

---

## 1. How rust-analyzer Bundles the LSP Binary

### Build Step: `cargo xtask dist`

The custom `xtask` build system (`xtask/src/dist.rs`) does two things:

1. **Compiles the server binary** for the target platform with `--profile release` and LTO thin
2. **Copies the binary into the extension directory** at `editors/code/server/`

```rust
// xtask/src/dist.rs — dist_client()
fn dist_client(sh: &Shell, version: &str, release_tag: &str, target: &Target) -> anyhow::Result<()> {
    let bundle_path = Path::new("editors").join("code").join("server");
    sh.create_dir(&bundle_path)?;
    sh.copy_file(&target.server_path, &bundle_path)?;
    // Also copies .pdb debug symbols on Windows
    if let Some(symbols_path) = &target.symbols_path {
        sh.copy_file(symbols_path, &bundle_path)?;
    }
    // Patches package.json: version + releaseTag
    let mut patch = Patch::new(sh, "./package.json")?;
    patch.replace(r#""version": "0.5.0-dev""#, &format!(r#""version": "{version}""#))
         .replace(r#""releaseTag": null"#, &format!(r#""releaseTag": "{release_tag}""#));
    patch.commit(sh)?;
    Ok(())
}
```

### .vscodeignore Controls What Goes In

The `.vscodeignore` uses a **deny-all, allow-list** pattern:

```
**
!icon.png
!language-configuration.json
!LICENSE
!node_modules/@hpcc-js/wasm/dist/graphvizlib.wasm
!node_modules/d3-graphviz/build/d3-graphviz.min.js
!node_modules/d3/dist/d3.min.js
!out/main.js
!package-lock.json
!package.json
!ra_syntax_tree.tmGrammar.json
!server                          <-- THE BINARY LIVES HERE
!README.md
!walkthrough-setup-tips.md
```

The `!server` line includes the entire `server/` directory, which at build time contains:
- `rust-analyzer` (or `rust-analyzer.exe` on Windows)
- `rust_analyzer.pdb` (Windows debug symbols, optional)

### Package.json Configuration

```json
{
    "name": "rust-analyzer",
    "version": "0.5.0-dev",      // patched at build time
    "releaseTag": null,           // patched to "nightly" or date string
    "extensionKind": ["workspace"]
}
```

Key: `"releaseTag": null` in dev mode signals "use `rust-analyzer` from PATH" (for contributors). When built for release, it's patched to the actual tag, signaling "use bundled binary."

---

## 2. CI Pipeline for Platform-Specific Builds

### Matrix Strategy (`.github/workflows/release.yaml`)

rust-analyzer builds **9 platform targets** in parallel:

| Runner | Rust Target | VS Code Target | Notes |
|--------|-------------|----------------|-------|
| `windows-latest` | `x86_64-pc-windows-msvc` | `win32-x64` | PGO enabled |
| `windows-latest` | `i686-pc-windows-msvc` | *(none)* | Standalone only |
| `windows-latest` | `aarch64-pc-windows-msvc` | `win32-arm64` | |
| `ubuntu-latest` (container) | `x86_64-unknown-linux-gnu` | `linux-x64` | PGO, glibc 2.28 container |
| `ubuntu-24.04-arm` (container) | `aarch64-unknown-linux-gnu` | `linux-arm64` | PGO, glibc 2.28 container |
| `ubuntu-latest` | `arm-unknown-linux-gnueabihf` | `linux-armhf` | Uses Zig cross-compilation |
| `macos-14` | `x86_64-apple-darwin` | `darwin-x64` | PGO enabled |
| `macos-14` | `aarch64-apple-darwin` | `darwin-arm64` | PGO enabled |
| Alpine separate job | `x86_64-unknown-linux-musl` | `alpine-x64` | musl libc |

### Build Steps Per Target

```yaml
# 1. Compile + copy binary into editors/code/server/
- name: Dist
  run: cargo xtask dist --client-patch-version ${{ github.run_number }}

# 2. Install npm deps
- run: npm ci
  working-directory: editors/code

# 3. Package with platform target
- name: Package Extension (release)
  if: github.ref == 'refs/heads/release'
  run: npx vsce package -o "../../dist/rust-analyzer-${{ matrix.code-target }}.vsix" --target ${{ matrix.code-target }}
  working-directory: editors/code
```

### Special: "no-server" Universal Fallback

After packaging the platform `.vsix`, they **delete the server directory** and build a universal `.vsix`:

```yaml
- if: matrix.target == 'x86_64-unknown-linux-gnu'
  run: rm -rf editors/code/server

- if: matrix.target == 'x86_64-unknown-linux-gnu'
  run: npx vsce package -o ../../dist/rust-analyzer-no-server.vsix
  working-directory: editors/code
```

This creates a 0.88 MB `.vsix` without any binary, for users who provide their own `rust-analyzer` via rustup or manual build.

### PGO (Profile-Guided Optimization)

rust-analyzer uses PGO on 5 of 9 targets. The training crate is `clap-rs/clap@v4.5.36` -- they analyze a real Rust project to generate optimization profiles. This is done via `xtask/src/pgo.rs`.

---

## 3. `vsce package --target` Usage

### The Command

```bash
npx vsce package -o "../../dist/rust-analyzer-darwin-arm64.vsix" --target darwin-arm64
```

### Valid Target Values

These are VS Code's platform identifiers (not Rust target triples):

| Target | Platform |
|--------|----------|
| `win32-x64` | Windows x64 |
| `win32-arm64` | Windows ARM64 |
| `win32-ia32` | Windows x86 (32-bit) |
| `linux-x64` | Linux x64 (glibc) |
| `linux-arm64` | Linux ARM64 |
| `linux-armhf` | Linux ARM hard-float |
| `alpine-x64` | Linux x64 (musl/Alpine) |
| `alpine-arm64` | Linux ARM64 (musl/Alpine) |
| `darwin-x64` | macOS Intel |
| `darwin-arm64` | macOS Apple Silicon |
| `web` | Browser (web extensions) |

### How It Works

When you run `vsce package --target <platform>`:
1. The `.vsix` file's `package.json` gets a `__metadata.targetPlatform` field
2. The Marketplace uses this to serve the right `.vsix` to each client
3. If no platform-specific `.vsix` matches, the Marketplace falls back to the "universal" (no `--target`) version

### For Nika: Relevant Targets

Nika realistically needs 5 targets to cover 99%+ of users:

| Priority | Target | Nika Rust Target |
|----------|--------|------------------|
| P0 | `darwin-arm64` | `aarch64-apple-darwin` |
| P0 | `darwin-x64` | `x86_64-apple-darwin` |
| P0 | `linux-x64` | `x86_64-unknown-linux-gnu` |
| P1 | `win32-x64` | `x86_64-pc-windows-msvc` |
| P2 | `linux-arm64` | `aarch64-unknown-linux-gnu` |

---

## 4. Typical .vsix Sizes

From the latest rust-analyzer release (measured 2026-04-06):

| File | Size |
|------|------|
| `rust-analyzer-darwin-arm64.vsix` | **16.77 MB** |
| `rust-analyzer-darwin-x64.vsix` | **17.01 MB** |
| `rust-analyzer-linux-x64.vsix` | **17.51 MB** |
| `rust-analyzer-linux-arm64.vsix` | **17.42 MB** |
| `rust-analyzer-linux-armhf.vsix` | 14.55 MB |
| `rust-analyzer-alpine-x64.vsix` | 17.17 MB |
| `rust-analyzer-win32-x64.vsix` | **19.80 MB** |
| `rust-analyzer-win32-arm64.vsix` | 19.19 MB |
| `rust-analyzer-no-server.vsix` | **0.88 MB** |

### Breakdown

- **Extension JS + assets**: ~0.88 MB (from the no-server .vsix)
- **Server binary (compressed in .vsix)**: ~14-19 MB depending on platform
- **Raw binary before compression**: typically 30-50 MB (the .vsix uses ZIP deflate)

### Nika Estimate

Nika binary (release, LTO thin, stripped) is likely 15-25 MB compressed depending on features compiled in. Total .vsix per platform: **16-26 MB**. This is well within Marketplace limits (the Marketplace has no hard size limit, but recommends under 100 MB).

---

## 5. Runtime Binary Discovery

### rust-analyzer's Discovery Chain (`bootstrap.ts`)

The `getServer()` function implements a 4-level priority chain:

```
1. Explicit config    rust-analyzer.server.path setting or __RA_LSP_SERVER_DEBUG env
2. Toolchain override rust-toolchain.toml with rust-analyzer component (via rustup)
3. Bundled binary     context.extensionUri + "server/rust-analyzer[.exe]"
4. Error              Show message: "we don't ship binaries for your platform"
```

### The Key Code

```typescript
// bootstrap.ts — getServer()

// Priority 1: explicit path from settings
const explicitPath = process.env["__RA_LSP_SERVER_DEBUG"] ?? config.serverPath;
if (explicitPath) return explicitPath;

// Priority 2: rust-toolchain.toml override (checks all workspace folders)
// ... rustup which rust-analyzer ...

// Priority 3: bundled binary
if (packageJson.releaseTag === null) return "rust-analyzer"; // dev mode: use PATH
const ext = process.platform === "win32" ? ".exe" : "";
const bundled = vscode.Uri.joinPath(context.extensionUri, "server", `rust-analyzer${ext}`);
const bundledExists = await fileExists(bundled);
if (bundledExists) {
    // NixOS special case: copy + patchelf for dynamic linker
    if (await isNixOs()) { /* ... patchelf ... */ }
    return bundled.fsPath;
}

// Priority 4: no binary available
await vscode.window.showErrorMessage(
    "Unfortunately we don't ship binaries for your platform yet..."
);
return undefined;
```

### Validation

Before using any discovered binary, it runs `--version` to verify it's executable:

```typescript
export async function isValidExecutable(path: string, extraEnv: Env): Promise<boolean> {
    const res = await spawnAsync(path, ["--version"], { env: newEnv });
    return res.status === 0;
}
```

### For Nika

The equivalent discovery chain would be:

```
1. nika.server.path       User config (explicit override)
2. PATH lookup            nika --version (if installed globally via Homebrew/cargo)
3. Bundled binary         context.extensionUri + "server/nika[.exe]"
4. Prompt to install      "Install Nika: brew install supernovae-studio/tap/nika"
```

---

## 6. Cursor / VSCodium / Open VSX Support

### Open VSX

rust-analyzer **explicitly publishes to Open VSX** using `ovsx`:

```yaml
# release.yaml
- name: Publish Extension (OpenVSX, release)
  run: npx ovsx publish --pat ${{ secrets.OPENVSX_TOKEN }} --packagePath ../../dist/rust-analyzer-*.vsix
  timeout-minutes: 2
```

Open VSX supports platform-specific extensions via the same `--target` mechanism. The `ovsx` CLI is a drop-in replacement for `vsce` for publishing.

### Cursor

Cursor uses the VS Code Marketplace directly (or Open VSX). Platform-specific `.vsix` files work without modification in Cursor. The extension just needs to be published to the VS Code Marketplace.

### VSCodium

VSCodium defaults to Open VSX as its marketplace. Since rust-analyzer publishes to Open VSX, it works. For Nika, publishing to both VS Code Marketplace + Open VSX covers all forks.

### Key Dependencies

```json
// package.json devDependencies
{
    "@vscode/vsce": "^3.7.1",    // VS Code Marketplace publishing
    "ovsx": "0.10.10"             // Open VSX publishing
}
```

---

## 7. Fallback When Bundled Binary Doesn't Work

### rust-analyzer's Approach

1. **Version check**: Runs `rust-analyzer --version` on the bundled binary
2. **If it fails**: Shows an error with guidance to install manually
3. **NixOS workaround**: Copies binary to global storage, runs `patchelf` to fix dynamic linker
4. **"no-server" .vsix**: Published as universal fallback for unsupported platforms
5. **Config override**: Users can always set `rust-analyzer.server.path` to their own binary

### The "no-server" Pattern

This is the cleverest part of the architecture. They publish a **universal .vsix** (`rust-analyzer-no-server.vsix`, 0.88 MB) alongside the platform-specific ones. If the Marketplace can't match a platform, users get this one. The extension then:
- Looks for `rust-analyzer` on PATH (from `rustup component add rust-analyzer`)
- Shows a helpful error if not found

### For Nika

Recommended fallback chain:
1. Try bundled binary (`server/nika`)
2. Run `nika --version` to validate
3. If fails, try `nika` from PATH (Homebrew / manual install)
4. If not found, show actionable install prompt with platform-specific instructions

---

## 8. Comparison: Other Extensions

### Deno (`vscode_deno`)

**Pattern: NO bundled binary. Universal .vsix only.**

- Single `.vsix` file, no platform targets
- Expects `deno` on PATH (user installs Deno separately)
- CI: Single build on `ubuntu-latest`, packages with `npx vsce package -o vscode-deno.vsix`
- No `--target` usage at all
- Publishes to VS Code Marketplace only (no Open VSX in CI)

### Go (`vscode-go`)

**Pattern: NO bundled binary. Downloads `gopls` at runtime.**

- Single universal `.vsix`
- On activation, checks if `gopls` is installed via `go install`
- If missing, prompts to install: `go install golang.org/x/tools/gopls@latest`
- Complex tool management system (`goInstallTools.ts`) handles installation, updates, version checking
- No platform-specific builds

### clangd (`vscode-clangd`)

**Pattern: NO bundled binary. Downloads clangd from GitHub releases at runtime.**

- Uses `@clangd/install` npm package for download/update logic
- On activation, checks PATH for `clangd` binary
- If not found, prompts to download from `github.com/clangd/clangd/releases`
- Downloads are stored in `globalStoragePath` (VS Code managed per-extension storage)
- Publishes to both VS Code Marketplace and Open VSX

### Summary Table

| Extension | Binary Bundled? | Platform .vsix? | Runtime Download? | Open VSX? |
|-----------|----------------|-----------------|-------------------|-----------|
| **rust-analyzer** | Yes | Yes (9 targets) | No | Yes |
| **Deno** | No | No | No (expects PATH) | No |
| **Go** | No | No | Yes (`go install`) | No |
| **clangd** | No | No | Yes (GitHub release) | Yes |
| **Nika (proposed)** | **Yes** | **Yes (5 targets)** | **No (fallback to PATH)** | **Yes** |

---

## 9. VS Code Marketplace Platform Support

### History

- **November 2021**: VS Code 1.63 introduced platform-specific extensions
- **API**: `vsce package --target <platform>` and `vsce publish --target <platform>`
- **Marketplace behavior**: When a user installs an extension, the Marketplace checks their platform and serves the matching `.vsix`. Falls back to universal if no match.

### How Publishing Works

You publish **multiple .vsix files** to the **same extension ID**. The Marketplace handles routing:

```bash
# Publish all platform .vsix files at once
npx vsce publish --pat $TOKEN --packagePath dist/nika-*.vsix
```

Or individually:

```bash
npx vsce publish --pat $TOKEN --packagePath dist/nika-darwin-arm64.vsix
npx vsce publish --pat $TOKEN --packagePath dist/nika-linux-x64.vsix
# ...
```

### Important: Version Must Match

All platform `.vsix` files for a given version must have the **same version number** in `package.json`. The Marketplace groups them by version.

---

## 10. Implementation Blueprint for Nika

### File Structure

```
nika/
  editors/
    vscode/
      .vscodeignore          # deny-all, allow-list
      package.json           # extension manifest
      src/
        extension.ts         # activation entry point
        bootstrap.ts         # binary discovery logic
        client.ts            # LSP client setup
      server/                # EMPTY in repo, filled at build time
        .gitkeep
```

### .vscodeignore

```
**
!icon.png
!out/main.js
!package.json
!LICENSE
!server
!README.md
```

### Build Script (equivalent to xtask dist)

```bash
#!/bin/bash
# build-vsix.sh <rust-target> <vscode-target>
RUST_TARGET=$1
VSCODE_TARGET=$2

# 1. Build Nika binary
cargo build --manifest-path tools/nika-cli/Cargo.toml \
  --target $RUST_TARGET --profile release

# 2. Copy into extension server/
mkdir -p editors/vscode/server
cp target/$RUST_TARGET/release/nika editors/vscode/server/

# 3. Patch version in package.json
# (use node script or sed)

# 4. Package
cd editors/vscode
npm ci
npx vsce package -o ../../dist/nika-$VSCODE_TARGET.vsix --target $VSCODE_TARGET
```

### CI Matrix

```yaml
strategy:
  matrix:
    include:
      - os: macos-14
        target: aarch64-apple-darwin
        code-target: darwin-arm64
      - os: macos-14
        target: x86_64-apple-darwin
        code-target: darwin-x64
      - os: ubuntu-latest
        target: x86_64-unknown-linux-gnu
        code-target: linux-x64
      - os: windows-latest
        target: x86_64-pc-windows-msvc
        code-target: win32-x64
      - os: ubuntu-24.04-arm
        target: aarch64-unknown-linux-gnu
        code-target: linux-arm64
```

### Binary Discovery (bootstrap.ts)

```typescript
async function getServer(context: vscode.ExtensionContext, config: Config): Promise<string | undefined> {
    // 1. Explicit config
    const explicitPath = config.get<string | null>('server.path');
    if (explicitPath) return explicitPath;

    // 2. Bundled binary
    const ext = process.platform === 'win32' ? '.exe' : '';
    const bundled = vscode.Uri.joinPath(context.extensionUri, 'server', `nika${ext}`);
    if (await fileExists(bundled)) {
        // Validate
        const ok = await isValidExecutable(bundled.fsPath);
        if (ok) return bundled.fsPath;
    }

    // 3. PATH fallback
    const pathBinary = await which('nika');
    if (pathBinary) return pathBinary;

    // 4. Prompt install
    const choice = await vscode.window.showErrorMessage(
        'Nika LSP server not found. Install Nika to enable language features.',
        'Install via Homebrew', 'Set Path Manually'
    );
    if (choice === 'Install via Homebrew') {
        vscode.env.openExternal(vscode.Uri.parse('https://nika.dev/install'));
    }
    return undefined;
}
```

### Publish Script

```bash
# Publish to both marketplaces
npx vsce publish --pat $MARKETPLACE_TOKEN --packagePath dist/nika-*.vsix
npx ovsx publish --pat $OPENVSX_TOKEN --packagePath dist/nika-*.vsix
```

---

## Key Takeaways for Nika

1. **rust-analyzer is THE reference** -- the only major extension doing platform-specific binary bundling. Others download at runtime.

2. **The pattern is simple**: build binary, copy to `server/`, run `vsce package --target`, publish all `.vsix` files to the same extension ID.

3. **5 targets cover 99%+**: darwin-arm64, darwin-x64, linux-x64, win32-x64, linux-arm64. Start here.

4. **Expect 15-20 MB per .vsix** with the Nika binary. This is normal and accepted by the marketplace.

5. **Always publish a "no-server" universal .vsix** as fallback (under 1 MB).

6. **Binary discovery**: bundled path via `context.extensionUri` + `server/`, with PATH fallback.

7. **Publish to both** VS Code Marketplace (`vsce`) and Open VSX (`ovsx`) to cover Cursor, VSCodium, and other forks.

8. **Validate the binary** by running `nika --version` before starting the LSP client.

9. **NixOS workaround** is nice-to-have (patchelf), but not P0 for launch.

10. **PGO** is a luxury optimization rust-analyzer does that meaningfully improves perf. Consider for post-launch.
