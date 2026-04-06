# Plan 3: Distribution Simplification — Single Download Strategy

> **Date**: 2026-04-06 | **Version**: v0.73.0 | **Target**: v0.75.0
> **Effort**: 2-3 days | **14 tasks** | **5 phases**
> **Prerequisites**: Plan 2 (LSP decoupling) for VSIX bundling

## Executive Summary

Make Nika a true "1 download = everything" experience across all channels.
Fix version desync (npm 27 versions behind, VS Code 22 behind).
Bundle binary in VSIX. Eliminate feature flag complexity for end users.

```
BEFORE: Download binary + install extension + start daemon + configure PATH = works maybe
AFTER:  brew install nika  OR  install extension  OR  npm i -g nika = everything works
```

## Current State — 9 Channels

| Channel | Status | Version | vs v0.73 | Auto-Update |
|---------|--------|---------|----------|-------------|
| GitHub Releases | ACTIVE | v0.73.0 | Current | Manual |
| Homebrew | ACTIVE | v0.72.0 | 1 behind | Auto (CI) |
| Docker Hub + GHCR | ACTIVE | v0.73.0 | Current | Auto (CI) |
| crates.io | ACTIVE | v0.73.0 | Current | Auto (CI) |
| Scoop (Windows) | ACTIVE | v0.73.0 | Current | Auto (CI) |
| AUR (Arch) | ACTIVE | v0.73.0 | Current | Auto (CI) |
| **npm** | **BROKEN** | **v0.46.1** | **27 behind** | Broken |
| **VS Code** | **THIN** | **v0.51.0** | **22 behind** | Broken |
| Open VSX | STALE | v0.51.0 | 22 behind | With VSCE |

### Problems

1. **npm v0.46.1** — 27 versions behind Cargo. `npx nika` installs ancient binary.
2. **VS Code v0.51.0** — 22 versions behind. Missing features, 8 bugs.
3. **No binary in VSIX** — Users must install binary separately.
4. **Mistral.rs feature flag** — `native-inference` adds 100MB+ to binary.
   Default binaries DON'T include it. Users who want local GGUF must `cargo install`.
5. **Windows unsigned** — No code signing for .exe.

---

## Build Targets (7 platforms)

| Target | Platform | Artifact | Signing |
|--------|----------|----------|---------|
| `aarch64-apple-darwin` | macOS arm64 | tar.gz ~18MB | Code-signed + notarized |
| `x86_64-apple-darwin` | macOS x64 | tar.gz ~18MB | Code-signed + notarized |
| `aarch64-unknown-linux-gnu` | Linux arm64 | tar.gz ~15MB | None |
| `x86_64-unknown-linux-gnu` | Linux x64 | tar.gz ~15MB | None |
| `x86_64-pc-windows-msvc` | Windows x64 | zip ~20MB | TODO (SignPath) |
| `x86_64-unknown-linux-musl` | Docker x64 | tar.gz ~12MB | None |
| `aarch64-unknown-linux-musl` | Docker arm64 | tar.gz ~12MB | None |

**Default features compiled in**: tui, lsp, serve, media-core, fetch-extract.
**NOT included**: `native-inference` (mistral.rs — too large, niche use case).

---

## Phase 1: Fix npm Version Desync (2-3 hours)

### Task 1.1: Update npm Package Versions

Update ALL package.json files to match current Cargo version:

```
packages/npm/package.json                    0.46.1 -> 0.73.0
packages/nika-darwin-arm64/package.json      0.46.1 -> 0.73.0
packages/nika-darwin-x64/package.json        0.46.1 -> 0.73.0
packages/nika-linux-x64/package.json         0.46.1 -> 0.73.0
packages/nika-linux-arm64/package.json       0.46.1 -> 0.73.0
packages/nika-win32-x64/package.json         0.46.1 -> 0.73.0
```

Also update `optionalDependencies` in the main package to match.

```
Commit: chore(npm): sync npm package versions to v0.73.0
```

### Task 1.2: Fix npm Publish in release.yml

Verify the `npm-publish` job in `.github/workflows/release.yml`:
- Downloads correct platform binary from GitHub release
- Copies to correct path in each platform package
- Publishes in correct order (platform packages first, then main wrapper)
- Uses `NPM_TOKEN` secret

**Test with dry-run**:
```bash
cd packages/npm && npm publish --dry-run
```

```
Commit: fix(ci): ensure npm publish uses correct binary artifacts
```

### Task 1.3: Add Version Sync Guard to CI

Add a preflight check in `release.yml` that verifies:
```bash
CARGO_VERSION=$(grep '^version' tools/Cargo.toml | head -1 | cut -d'"' -f2)
NPM_VERSION=$(node -p "require('./packages/npm/package.json').version")
VSCODE_VERSION=$(node -p "require('./editors/vscode/package.json').version")

if [ "$CARGO_VERSION" != "$NPM_VERSION" ]; then
  echo "ERROR: npm version $NPM_VERSION != Cargo version $CARGO_VERSION"
  exit 1
fi
```

```
Commit: feat(ci): add cross-channel version sync guard to release preflight
```

---

## Phase 2: VS Code Extension Version Alignment (1-2 hours)

### Task 2.1: Update Extension Version

```
editors/vscode/package.json  0.51.0 -> 0.73.0
```

Update `engines.vscode` if needed (currently `^1.75.0` — keep this).

```
Commit: chore(vscode): sync extension version to v0.73.0
```

### Task 2.2: Add Version Sync Script

Create `scripts/sync-versions.sh` that propagates Cargo.toml version to:
- `packages/npm/package.json` + all platform packages
- `editors/vscode/package.json`
- Validates all match

```bash
#!/usr/bin/env bash
set -euo pipefail

VERSION=$(grep '^version' tools/Cargo.toml | head -1 | cut -d'"' -f2)
echo "Syncing all packages to v${VERSION}"

# npm packages
for pkg in packages/*/package.json; do
  jq --arg v "$VERSION" '.version = $v' "$pkg" > tmp.$$ && mv tmp.$$ "$pkg"
done

# npm optional dependencies
jq --arg v "$VERSION" '
  .optionalDependencies |= with_entries(.value = $v)
' packages/npm/package.json > tmp.$$ && mv tmp.$$ packages/npm/package.json

# VS Code extension
jq --arg v "$VERSION" '.version = $v' editors/vscode/package.json > tmp.$$ \
  && mv tmp.$$ editors/vscode/package.json

echo "All packages synced to v${VERSION}"
```

```
Commit: feat(scripts): add version sync script for cross-channel alignment
```

---

## Phase 3: Platform-Specific VSIX Bundling (1 day)

> This is Plan 1 Phase 1 detailed here from the distribution angle.

### Task 3.1: Add Binary Bundling to Extension

**Pattern**: rust-analyzer approach.

1. **Binary location**: `editors/vscode/server/nika[.exe]`
2. **`.vscodeignore`**: deny-all, whitelist only essentials
3. **`findBundledBinary()`**: Check `server/` before PATH

**`.vscodeignore`**:
```
**
!server/
!server/nika
!server/nika.exe
!out/**/*.js
!out/**/*.css
!syntaxes/**
!snippets/**
!icons/**
!package.json
!README.md
!LICENSE
```

### Task 3.2: Add Platform VSIX Matrix to release.yml

Replace the single `vscode-publish` job with a matrix:

```yaml
vscode-publish:
  needs: [github-release]
  strategy:
    matrix:
      include:
        - platform: darwin-arm64
          target: aarch64-apple-darwin
          binary: nika
        - platform: darwin-x64
          target: x86_64-apple-darwin
          binary: nika
        - platform: linux-x64
          target: x86_64-unknown-linux-gnu
          binary: nika
        - platform: linux-arm64
          target: aarch64-unknown-linux-gnu
          binary: nika
        - platform: win32-x64
          target: x86_64-pc-windows-msvc
          binary: nika.exe
        - platform: universal
          target: ""
          binary: ""
  steps:
    - uses: actions/checkout@v4
    - uses: actions/setup-node@v4
    - run: npm ci
      working-directory: editors/vscode
    - if: matrix.target != ''
      uses: actions/download-artifact@v4
      with:
        name: nika-${{ matrix.target }}
        path: editors/vscode/server/
    - if: matrix.target != ''
      run: chmod +x editors/vscode/server/${{ matrix.binary }}
    - run: npx vsce package --no-git-tag-version --target ${{ matrix.platform }}
      working-directory: editors/vscode
    - run: npx vsce publish --target ${{ matrix.platform }}
      working-directory: editors/vscode
      env:
        VSCE_PAT: ${{ secrets.VSCE_PAT }}
    - if: env.OVSX_PAT
      run: npx ovsx publish --target ${{ matrix.platform }}
      working-directory: editors/vscode
      env:
        OVSX_PAT: ${{ secrets.OVSX_PAT }}
```

### Task 3.3: Test VSIX Build Locally

```bash
# Build nika binary
cd tools && cargo build --release -p nika

# Copy to extension
mkdir -p ../editors/vscode/server
cp ../target/release/nika ../editors/vscode/server/

# Package
cd ../editors/vscode
npx vsce package --no-git-tag-version --target darwin-arm64

# Verify size (~18MB expected)
ls -lh *.vsix
```

```
Commit: feat(ci): platform-specific VSIX with bundled nika binary
```

---

## Phase 4: Simplify Feature Flags for Users (1-2 hours)

### Task 4.1: Document Default vs Optional Features

**Default binary** (what 99% of users get):
- tui (terminal UI)
- lsp (language server)
- serve (HTTP API)
- media-core (thumbnail, convert, strip, dimensions)
- fetch-extract (HTML, markdown, article, feed extraction)
- All 62 builtin tools
- All 9 providers (anthropic, openai, mistral, groq, deepseek, gemini, xai, native, mock)

**Optional** (must build from source):
- `native-inference` — mistral.rs GGUF runtime (+100MB binary, needs CMake)
- `media-pdf` — PDF extraction (requires poppler)
- `media-chart` — SVG chart generation
- `media-provenance` — C2PA provenance

**Rule**: Default binary = everything most users need. No feature flag selection.
Native inference is the only meaningful opt-in, and it's a niche use case
(local GGUF models on your own hardware).

### Task 4.2: Update Installation Docs

Simplify to 4 methods (no feature flags mentioned):

```markdown
## Install Nika

### macOS
brew install supernovae-st/tap/nika

### Any platform (npm)
npm install -g @supernovae-st/nika

### VS Code / Cursor
Search "Nika" in extensions → Install (binary included!)

### Direct download
https://github.com/supernovae-st/nika/releases/latest
```

**Local models** section (separate, advanced):
```markdown
## Local Models (Optional)

For running GGUF models locally with mistral.rs:
cargo install nika --features native-inference
```

```
Commit: docs: simplify installation to 4 methods — zero feature flag decisions
```

---

## Phase 5: Version Lifecycle Automation (1-2 hours)

### Task 5.1: Add Bump Script

Create `scripts/bump-version.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

NEW_VERSION="${1:?Usage: bump-version.sh <version>}"

echo "Bumping all packages to v${NEW_VERSION}"

# 1. Cargo workspace
sed -i '' "s/^version = \".*\"/version = \"${NEW_VERSION}\"/" tools/Cargo.toml

# 2. npm packages
./scripts/sync-versions.sh

# 3. VS Code extension
jq --arg v "$NEW_VERSION" '.version = $v' editors/vscode/package.json > tmp.$$ \
  && mv tmp.$$ editors/vscode/package.json

# 4. Verify
echo "Verify:"
grep '^version' tools/Cargo.toml
node -p "require('./packages/npm/package.json').version"
node -p "require('./editors/vscode/package.json').version"

echo "Done. Now: git add -A && git commit -m 'chore: bump version to v${NEW_VERSION}'"
```

```
Commit: feat(scripts): add bump-version.sh for atomic cross-channel version bumps
```

### Task 5.2: Add Smoke Test to Release Pipeline

After all publish jobs, add a smoke test:

```yaml
smoke-test:
  needs: [npm-publish, vscode-publish, crates-publish, update-homebrew]
  runs-on: ubuntu-latest
  steps:
    # Verify npm
    - run: npx @supernovae-st/nika --version | grep "${{ github.ref_name }}"
    # Verify crates.io (may have propagation delay)
    - run: cargo install nika --version "$VERSION" --dry-run || true
    # Verify homebrew formula updated
    - run: |
        curl -s https://raw.githubusercontent.com/supernovae-st/homebrew-tap/main/Formula/nika.rb \
          | grep "$VERSION"
```

```
Commit: feat(ci): add post-release smoke test across all channels
```

### Task 5.3: Fix Homebrew 1-Version Lag

Homebrew is at v0.72.0 while Cargo is at v0.73.0. The `update-homebrew` job
should run AFTER GitHub release creates the release artifacts.

Verify the dependency chain in release.yml:
```yaml
update-homebrew:
  needs: [github-release]  # Must wait for release artifacts
```

Check that SHA256 generation uses the correct artifact URLs.

```
Commit: fix(ci): ensure homebrew formula updates on every release
```

---

## Distribution Channels — Target State

After this plan, all channels publish atomically on tag push:

```
git tag v0.75.0 && git push --tags
  |
  +-> Build (7 targets, parallel) ~15 min
  |
  +-> GitHub Release (binaries + checksums + SLSA) ~2 min
  |
  +-> Parallel publish ~10 min:
       +-> Homebrew (auto-update formula)
       +-> npm (6 platform packages + main wrapper)
       +-> VS Code Marketplace (6 platform VSIX + universal)
       +-> Open VSX (if OVSX_PAT set)
       +-> crates.io (13 crates in order)
       +-> Docker Hub + GHCR (multi-arch)
       +-> Scoop (Windows manifest)
       +-> AUR (Arch Linux PKGBUILD)
  |
  +-> Smoke test (verify all channels) ~5 min
  |
  +-> Telegram notification
```

**Total**: ~30 min from tag to all 9 channels published.

**Version alignment**: ALL channels at same version. No more desync.

**User experience**:
- `brew install supernovae-st/tap/nika` → v0.75.0
- `npm install -g @supernovae-st/nika` → v0.75.0
- VS Code "Install Extension" → v0.75.0 with binary
- `cargo install nika` → v0.75.0
- `docker pull ghcr.io/supernovae-st/nika` → v0.75.0

---

## Gotchas

### Gotcha 1: npm Publish Order Matters
Platform packages MUST publish BEFORE the main wrapper.
Main package has `optionalDependencies` on platform packages.
If main publishes first, `npm install` fails to find deps.

### Gotcha 2: VSIX Size Limits
VS Code Marketplace has a 200MB limit per VSIX.
Our ~18MB binaries are well within limits.

### Gotcha 3: Homebrew SHA256 Race
The `update-homebrew` job computes SHA256 from GitHub release assets.
If assets aren't fully uploaded when the job runs, SHA256 will be wrong.
Add a retry loop or explicit asset check.

### Gotcha 4: npm Token Scope
`NPM_TOKEN` must have publish access to `@supernovae-st` scope.
Verify with `npm whoami --registry https://registry.npmjs.org/`.

### Gotcha 5: crates.io Publish Order
13 crates must publish in dependency order:
```
nika-core -> nika-event -> nika-lsp-core -> nika-mcp -> nika-media
  -> nika-init -> nika-engine -> nika-daemon -> nika-cli -> nika-tui
  -> nika-lsp -> nika-vault -> nika
```
Each `cargo publish` must wait for crates.io to index the previous.
Add `sleep 15` between publishes or use `--no-verify` (faster but riskier).

### Gotcha 6: Windows Code Signing (TODO)
Windows binaries are currently unsigned. This causes SmartScreen warnings.
Fix: Purchase code signing certificate (~$210/yr from SSL.com).
Integrate SignPath in release.yml (secret: `SIGNPATH_API_TOKEN`).

---

## Verification Checklist

```bash
# 1. All versions match
grep '^version' tools/Cargo.toml
node -p "require('./packages/npm/package.json').version"
node -p "require('./editors/vscode/package.json').version"
# All must show same version

# 2. npm dry-run
cd packages/npm && npm publish --dry-run

# 3. VSIX builds
cd editors/vscode && npx vsce package --no-git-tag-version --target darwin-arm64
ls -lh *.vsix  # ~18MB

# 4. Version sync script
./scripts/sync-versions.sh
git diff  # Should show NO changes (already synced)

# 5. Release dry-run (manual dispatch)
# GitHub Actions -> Release -> Run workflow -> check dry_run
```

---

## Summary

| Phase | Tasks | Time | Deliverable |
|-------|-------|------|-------------|
| 1 | 3 | 2-3 hours | npm version sync (v0.46 -> v0.73) |
| 2 | 2 | 1-2 hours | VS Code version alignment + sync script |
| 3 | 3 | 1 day | Platform-specific VSIX with bundled binary |
| 4 | 2 | 1-2 hours | Simplified install docs, zero feature flags |
| 5 | 3 | 1-2 hours | Bump script + smoke test + Homebrew fix |
| **Total** | **14** | **2-3 days** | **All 9 channels in sync, 1 download = everything** |

---

## Relation to Other Plans

| Dependency | Direction | Details |
|------------|-----------|---------|
| Plan 1 Phase 0 | Prereq for Phase 3 | Fix bugs before bundling |
| Plan 2 | Prereq for Phase 3 | Smaller binary improves VSIX size |
| Plan 1 Phase 1 | Same as Phase 3 here | Binary bundling is shared work |
| S9-S12 sprints | Independent | Distribution fixes don't block sprints |

**Execution order**: Plan 2 (1-2 days) -> Plan 3 Phase 1-2 (1 day) -> Plan 1 Phase 0 (30 min) -> Plan 3 Phase 3 (1 day) -> Plan 1 remaining phases
